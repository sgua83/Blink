//! B owns layout and rebasing for both reports and native outputs.
//! Native output writes the selected COFF bytes directly into its own sections.
use super::{Result, Selected, pe, resources::ImportBytes};
use crate::object::{Relocation, Section, Target, stack::{Call, Frame}};
use std::collections::BTreeMap;

#[repr(C)]
struct CombineObject {section_start:u64,section_count:u64,symbol_start:u64,symbol_count:u64,frame_start:u64,frame_count:u64}
#[repr(C)]
struct CombineSection {
    name:*const u8,name_length:u64,bytes:*const u8,byte_length:u64,flags:u64,object:u64,
    owner:u64,group:u64,begin:u64,size:u64,padding:u64,
}
#[repr(C)]
struct CombineSymbol {name:*const u8,name_length:u64,section:u64,value:u64}
#[repr(C)]
struct CombineRelocation {source:u64,offset:u64,target_kind:u64,section:u64,value:u64,name:*const u8,name_length:u64}
#[repr(C)]
struct CombineFrame {section:u64,start:u64,present:u64}
#[derive(Clone,Copy)]
#[repr(C)]
struct CombineInput {
    objects:*const CombineObject,object_count:u64,sections:*mut CombineSection,section_count:u64,
    symbols:*mut CombineSymbol,symbol_count:u64,relocations:*mut CombineRelocation,relocation_count:u64,
    frames:*mut CombineFrame,order:*mut u64,
}
#[repr(C)]
struct CombinePlan {groups:u64,symbols:u64,error:u64,at:u64}
unsafe extern "C" {
    #[link_name="bootstrap.combine::prepare"]
    fn combine_prepare(input: CombineInput, local: *const ImportBytes, local_count: usize) -> CombinePlan;
    #[link_name="bootstrap.combine::encode"]
    fn combine_encode(sections: *const CombineSection, count: usize, outputs: *const *mut u8);
}

pub(super) fn objects<Payload: AsRef<[u8]>>(
    objects: Vec<Selected<Payload>>, module: Option<(&[&str], &mut BTreeMap<String, (usize, u32)>)>,
) -> Result<Vec<Section>> {
    combine(objects, module, extent,
        |rows, groups, objects| grouped_sections(rows, groups, objects, Vec::with_capacity),
        |section| &mut section.relocations,
        |rows, mut sections, _| {
            let destinations: Vec<_> = sections.iter_mut().map(|section| section.bytes.as_mut_ptr()).collect();
            unsafe { combine_encode(rows.as_ptr(), rows.len(), destinations.as_ptr()); }
            // The writer initializes the requested extent, including each alignment gap.
            for (i, row) in rows.iter().enumerate() {
                if row.owner == i as u64 {
                    unsafe { sections[row.group as usize].bytes.set_len(row.size as usize); }
                }
            }
            Ok(sections)
        })
}

pub(super) fn link_pe(objects: Vec<Selected>, stack_reserve: u64) -> Result<Vec<u8>> {
    combine(objects, None, extent,
        |rows, groups, objects| grouped_sections(rows, groups, objects, std::convert::identity),
        |section| &mut section.relocations,
        |rows, sections, _| pe::link(&sections, stack_reserve, |destinations| {
            // PE supplies one final byte range for each group in layout order.
            unsafe { combine_encode(rows.as_ptr(), rows.len(), destinations.as_ptr()); }
        }))
}

pub(super) fn report(objects: Vec<Selected<usize>>) -> Result<(Vec<Vec<Relocation>>, Vec<Frame>)> {
    combine(objects, None, |&length| (std::ptr::null(), length),
        |_, groups, _| Ok((0..groups).map(|_| Vec::new()).collect()),
        |relocations| relocations,
        |_, relocations, stack| Ok((relocations, stack)))
}

fn extent(bytes: &impl AsRef<[u8]>) -> (*const u8, usize) {
    let bytes = bytes.as_ref();
    (bytes.as_ptr(), bytes.len())
}

fn combine<Payload, Group, Output>(
    mut objects: Vec<Selected<Payload>>, module: Option<(&[&str], &mut BTreeMap<String, (usize, u32)>)>,
    extent: impl Fn(&Payload) -> (*const u8, usize),
    groups: impl FnOnce(&[CombineSection], usize, &[Selected<Payload>]) -> Result<Vec<Group>>,
    relocation_output: impl Fn(&mut Group) -> &mut Vec<Relocation>,
    finish: impl FnOnce(&[CombineSection], Vec<Group>, Vec<Frame>) -> Result<Output>,
) -> Result<Output> {
    let mut inputs=Vec::with_capacity(objects.len());
    let mut rows=Vec::new();let mut symbols=Vec::new();let mut relocations=Vec::new();
    let mut frames=Vec::new();
    for (object_index,object) in objects.iter().enumerate(){
        let section_start=rows.len();let symbol_start=symbols.len();let frame_start=frames.len();
        for section in &object.sections {
            let source=rows.len();
            for relocation in &section.relocations {
                let (target_kind,target_section,value,name,length)=match &relocation.target {
                    Target::Section(index,value)=>(0,*index as u64,u64::from(*value),std::ptr::null(),0),
                    Target::Import(_)=>(1,0,0,std::ptr::null(),0),
                    Target::Symbol(name)=>(2,0,0,name.as_ptr(),name.len() as u64),
                };
                relocations.push(CombineRelocation{source:source as u64,offset:relocation.offset as u64,
                    target_kind,section:target_section,value,name,name_length:length});
            }
            let (bytes, length) = extent(&section.bytes);
            rows.push(CombineSection{name:section.name.as_ptr(),name_length:section.name.len() as u64,
                bytes,byte_length:length as u64,flags:u64::from(section.flags),object:object_index as u64,
                owner:0,group:0,begin:0,size:0,padding:0});
        }
        for symbol in &object.definitions {
            symbols.push(CombineSymbol{name:symbol.name.as_ptr(),name_length:symbol.name.len() as u64,
                section:symbol.section as u64,value:u64::from(symbol.value)});
        }
        for frame in &object.stack {
            let (section,start,present)=match frame.coff {Some((section,start))=>(section as u64,u64::from(start),1),None=>(0,0,0)};
            frames.push(CombineFrame { section, start, present });
        }
        inputs.push(CombineObject{section_start:section_start as u64,section_count:object.sections.len() as u64,
            symbol_start:symbol_start as u64,symbol_count:object.definitions.len() as u64,frame_start:frame_start as u64,frame_count:object.stack.len() as u64});
    }
    // The same temporary permutation serves grouping, then final symbol lookup.
    let mut order=vec![0u64;rows.len().max(symbols.len())];
    let input=CombineInput{objects:inputs.as_ptr(),object_count:inputs.len() as u64,
        sections:rows.as_mut_ptr(),section_count:rows.len() as u64,symbols:symbols.as_mut_ptr(),symbol_count:symbols.len() as u64,
        relocations:relocations.as_mut_ptr(),relocation_count:relocations.len() as u64,frames:frames.as_mut_ptr(),
        order:order.as_mut_ptr()};
    let local: Vec<_> = module.as_ref().into_iter().flat_map(|(names, _)| names.iter())
        .map(|name| ImportBytes::new(name)).collect();
    let plan = unsafe { combine_prepare(input, local.as_ptr(), local.len()) };
    match plan.error {
        0=>{},1=>return Err("合并节偏移超出32位范围".into()),
        2=>return Err("模块符号地址溢出".into()),3=>return Err("栈函数节地址溢出".into()),4=>return Err("模块节地址溢出".into()),
        5=>{
            let row=&relocations[usize::try_from(plan.at)?];
            // This is the original String's borrowed UTF-8, still owned by objects.
            let name=unsafe{std::str::from_utf8_unchecked(std::slice::from_raw_parts(row.name,row.name_length as usize))};
            return Err(format!("未定义模块符号 {name}").into());
        },
        6 => {
            let symbol = &symbols[usize::try_from(plan.at)?];
            // The original object still owns this selected symbol name.
            let name = unsafe {
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(symbol.name, symbol.name_length as usize))
            };
            return Err(format!("重复定义模块符号 {name}").into());
        },
        code=>return Err(format!("COFF节组合失败 {code}").into()),
    }
    let mut sections = groups(&rows, usize::try_from(plan.groups)?, &objects)?;
    if let Some((_, defined))=module{for &index in &order[..usize::try_from(plan.symbols)?]{
        let symbol=&symbols[usize::try_from(index)?];
        let name=unsafe{std::str::from_utf8_unchecked(std::slice::from_raw_parts(symbol.name,symbol.name_length as usize))};
        defined.insert(name.to_owned(),(usize::try_from(symbol.section)?,u32::try_from(symbol.value)?));
    }}
    let mut stack = Vec::with_capacity(frames.len());
    let mut relocation_bases = Vec::new();
    let mut section_index = 0;
    let mut relocation_index = 0;
    let mut frame_index = 0;
    for object in &mut objects {
        relocation_bases.clear();
        let has_frames = !object.stack.is_empty();
        for section in &mut object.sections {
            let group = usize::try_from(rows[section_index].group)?;
            section_index += 1;
            let output = relocation_output(&mut sections[group]);
            if has_frames {
                relocation_bases.push(output.len());
            }
            for mut relocation in std::mem::take(&mut section.relocations) {
                let row = &relocations[relocation_index];
                relocation_index += 1;
                relocation.offset = usize::try_from(row.offset)?;
                if row.target_kind == 0 {
                    relocation.target = Target::Section(usize::try_from(row.section)?, u32::try_from(row.value)?);
                }
                output.push(relocation);
            }
        }
        for mut frame in std::mem::take(&mut object.stack) {
            let row = &frames[frame_index];
            frame_index += 1;
            if let Some((section, start)) = &mut frame.coff {
                let base = relocation_bases[*section];
                for call in &mut frame.calls {
                    if let Call::Reference { relocation, .. } = call {
                        *relocation += base;
                    }
                }
                *section = usize::try_from(row.section)?;
                *start = u32::try_from(row.start)?;
            }
            stack.push(frame);
        }
    }
    // Selected section names and bytes still back the layout views during writing.
    finish(&rows, sections, stack)
}

fn grouped_sections<Payload, Bytes>(
    rows: &[CombineSection], groups: usize, objects: &[Selected<Payload>],
    payload: impl Fn(usize) -> Bytes,
) -> Result<Vec<Section<Bytes>>> {
    let mut sections = Vec::with_capacity(groups);
    for (i, (section, row)) in objects.iter().flat_map(|object| &object.sections).zip(rows).enumerate() {
        if row.owner == i as u64 {
            sections.push(Section {
                name: section.name.clone(),
                bytes: payload(usize::try_from(row.size)?),
                flags: u32::try_from(row.flags)?,
                relocations: Vec::new(),
                associated: Vec::new(),
                layout: None,
            });
        }
    }
    Ok(sections)
}
