//! B owns layout and rebasing for both reports and native outputs.
//! Native output writes the selected COFF bytes directly into its own sections.
use super::{Result, Selected, pe, resources::ImportBytes};
use crate::{
    native::{ModuleCode, ModuleRelocation, ModuleSection, ModuleTarget},
    object::{LinkSectionBytes, LinkSectionData, Relocation, Target, stack::{self, Call, CallTarget}},
};
use std::collections::BTreeMap;

#[repr(C)]
struct CombineObject {section_start:u64,section_count:u64,symbol_start:u64,symbol_count:u64,frame_start:u64,frame_count:u64}
#[repr(C)]
pub(super) struct CombineSection {
    name:*const u8,name_length:u64,bytes:*const u8,byte_length:u64,flags:u64,object:u64,
    group:u64,begin:u64,padding:u64,relocation_start:u64,relocation_base:u64,
}
#[repr(C)]
pub(super) struct CombineGroup { first:usize, size:usize, relocation_count:usize }
#[repr(C)]
struct CombineSymbol {name:*const u8,name_length:u64,section:u64,value:u64}
#[repr(C)]
pub(super) struct CombineRelocation {
    source:u64,slot:u64,offset:u64,target_kind:u64,section:u64,value:u64,
    name:*const u8,name_length:u64,kind:u64,library:*const u8,library_length:u64,
}
#[repr(C)]
struct CombineFrame {section:u64,start:u64,present:u64,relocation_start:usize}
#[derive(Clone,Copy)]
#[repr(C)]
struct CombineInput {
    objects:*const CombineObject,object_count:u64,sections:*mut CombineSection,section_count:u64,
    symbols:*mut CombineSymbol,symbol_count:u64,relocations:*mut CombineRelocation,relocation_count:u64,
    frames:*mut CombineFrame,order:*mut u64,groups:*mut CombineGroup,
}
#[repr(C)]
pub(super) struct CombinePlan {
    pub(super) groups: usize,
    pub(super) imports: usize,
    pub(super) absolutes: usize,
    error: u64,
    at: u64,
}
unsafe extern "C" {
    #[link_name="bootstrap.combine::prepare"]
    fn combine_prepare(input: CombineInput, local: *const ImportBytes, local_count: usize, pe: u32) -> CombinePlan;
    #[link_name="bootstrap.combine::encode"]
    fn combine_encode(sections: *const CombineSection, count: usize, outputs: *const *mut u8);
}

pub(super) fn module(
    mut objects: Vec<Selected<LinkSectionBytes<'_>>>, local: &[&str],
) -> Result<ModuleCode> {
    let Prepared { rows, groups, symbols, relocations, .. } = prepare(&objects, local, extent, false)?;
    let mut sections: Vec<_> = groups.iter().map(|group| {
        let first = &rows[group.first];
        // The first selected section still owns the original UTF-8 name.
        let name = unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(first.name, first.name_length as usize))
        };
        ModuleSection {
            name: name.to_owned(), bytes: Vec::with_capacity(group.size), flags: first.flags as u32,
            relocations: Vec::with_capacity(group.relocation_count),
        }
    }).collect();
    let destinations: Vec<_> = sections.iter_mut().map(|section| section.bytes.as_mut_ptr()).collect();
    unsafe { combine_encode(rows.as_ptr(), rows.len(), destinations.as_ptr()); }
    for (section, group) in sections.iter_mut().zip(&groups) {
        // The writer initialized the byte extent, including any alignment gaps.
        unsafe { section.bytes.set_len(group.size); }
    }

    // B rebased the unique definitions and relocations in their original rows.
    let mut defined = BTreeMap::new();
    let definitions = objects.iter_mut().flat_map(|object| std::mem::take(&mut object.definitions));
    for (definition, symbol) in definitions.zip(symbols) {
        defined.insert(definition.name, (usize::try_from(symbol.section)?, u32::try_from(symbol.value)?));
    }
    let selected_sections = objects.into_iter().flat_map(|object| object.sections);
    let mut relocations = relocations.into_iter();
    for (section, row) in selected_sections.zip(rows) {
        let output = &mut sections[usize::try_from(row.group)?].relocations;
        for mut relocation in section.relocations {
            let row = relocations.next().unwrap();
            relocation.offset = usize::try_from(row.offset)?;
            if row.target_kind == 0 {
                relocation.target = Target::Section(usize::try_from(row.section)?, u32::try_from(row.value)?);
            }
            output.push(relocation.into());
        }
    }
    Ok(ModuleCode { sections, symbols: defined })
}

pub(super) fn link_pe(objects: Vec<Selected<LinkSectionBytes<'_>>>, stack_reserve: u64) -> Result<Vec<u8>> {
    let Prepared { rows, groups, mut relocations, plan, .. } = prepare(&objects, &[], extent, true)?;
    pe::link(&rows, &groups, &mut relocations, &plan, stack_reserve)
}

pub(super) fn report(
    mut objects: Vec<Selected<usize>>, budget: Option<u64>, reserve: Option<u64>, runtime: bool,
) -> Result<String> {
    let Prepared { relocations, frames, .. } = prepare(&objects, &[], |&length| (std::ptr::null(), length), false)?;
    let source_frames = objects.iter_mut().flat_map(|object| std::mem::take(&mut object.stack));
    let frames = source_frames.zip(frames).map(|(mut frame, row)| {
        if let Some(position) = &mut frame.coff {
            *position = (usize::try_from(row.section)?, u32::try_from(row.start)?);
            for call in &mut frame.calls {
                if let Call::Reference { relocation, .. } = call {
                    *relocation += row.relocation_start;
                }
            }
        }
        Ok(frame)
    }).collect::<Result<Vec<_>>>()?;
    let code = stack::Code {
        target: &|_, slot| {
            let row = &relocations[slot];
            if row.target_kind == 0 {
                CallTarget::Section(row.section as usize, row.value as u32)
            } else {
                // Imported names borrow from the selected objects through report rendering.
                let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(row.name, row.name_length as usize)) };
                let dll = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(row.library, row.library_length as usize)) };
                CallTarget::Import { dll, name }
            }
        },
        frames: &frames,
        definitions: stack::Definitions::Linked(&[]),
    };
    Ok(stack::report(&objects[0].name, code, budget, reserve, runtime))
}

fn extent(bytes: &LinkSectionBytes<'_>) -> (*const u8, usize) {
    match bytes.data() {
        LinkSectionData::Initialized(bytes) => (bytes.as_ptr(), bytes.len()),
        LinkSectionData::ZeroFill(length) => (std::ptr::null(), length),
    }
}

struct Prepared {
    rows: Vec<CombineSection>,
    groups: Vec<CombineGroup>,
    symbols: Vec<CombineSymbol>,
    relocations: Vec<CombineRelocation>,
    frames: Vec<CombineFrame>,
    plan: CombinePlan,
}

// The selected objects own the bytes and names through the final consumer.
fn prepare<Payload>(
    objects: &[Selected<Payload>], local: &[&str],
    extent: impl Fn(&Payload) -> (*const u8, usize),
    pe: bool,
) -> Result<Prepared> {
    let mut inputs=Vec::with_capacity(objects.len());
    let mut rows=Vec::new();let mut symbols=Vec::new();let mut relocations=Vec::new();
    let mut frames=Vec::new();
    for (object_index,object) in objects.iter().enumerate(){
        let section_start=rows.len();let symbol_start=symbols.len();let frame_start=frames.len();
        for section in &object.sections {
            let source=rows.len();
            let relocation_start=relocations.len() as u64;
            for (slot, relocation) in section.relocations.iter().enumerate() {
                let empty = "";
                let (target_kind, target_section, value, name, library) = match &relocation.target {
                    Target::Section(index, value) => (0, *index as u64, u64::from(*value), empty, empty),
                    Target::Import(import) => (1, 0, 0, import.name.as_str(), import.dll.as_str()),
                    Target::Symbol(name) => (2, 0, 0, name.as_str(), empty),
                };
                relocations.push(CombineRelocation{source:source as u64,slot:slot as u64,offset:relocation.offset as u64,
                    target_kind,section:target_section,value,name:name.as_ptr(),name_length:name.len() as u64,
                    kind:u64::from(relocation.kind),library:library.as_ptr(),library_length:library.len() as u64});
            }
            let (bytes, length) = extent(&section.bytes);
            rows.push(CombineSection{name:section.name.as_ptr(),name_length:section.name.len() as u64,
                bytes,byte_length:length as u64,flags:u64::from(section.flags),object:object_index as u64,
                group:0,begin:0,padding:0,relocation_start,relocation_base:0});
        }
        for symbol in &object.definitions {
            symbols.push(CombineSymbol{name:symbol.name.as_ptr(),name_length:symbol.name.len() as u64,
                section:symbol.section as u64,value:u64::from(symbol.value)});
        }
        for frame in &object.stack {
            let (section,start,present)=match frame.coff {Some((section,start))=>(section as u64,u64::from(start),1),None=>(0,0,0)};
            frames.push(CombineFrame { section, start, present, relocation_start: 0 });
        }
        inputs.push(CombineObject{section_start:section_start as u64,section_count:object.sections.len() as u64,
            symbol_start:symbol_start as u64,symbol_count:object.definitions.len() as u64,frame_start:frame_start as u64,frame_count:object.stack.len() as u64});
    }
    // B initializes the temporary permutation for each sort.
    let mut order = Vec::with_capacity(rows.len().max(symbols.len()));
    let mut groups = Vec::with_capacity(rows.len());
    let input=CombineInput{objects:inputs.as_ptr(),object_count:inputs.len() as u64,
        sections:rows.as_mut_ptr(),section_count:rows.len() as u64,symbols:symbols.as_mut_ptr(),symbol_count:symbols.len() as u64,
        relocations:relocations.as_mut_ptr(),relocation_count:relocations.len() as u64,frames:frames.as_mut_ptr(),
        order:order.as_mut_ptr(),groups:groups.as_mut_ptr()};
    let local: Vec<_> = local.iter().map(|name| ImportBytes::new(name)).collect();
    let plan = unsafe { combine_prepare(input, local.as_ptr(), local.len(), u32::from(pe)) };
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
    // B wrote one group record for each first section.
    unsafe { groups.set_len(plan.groups); }
    Ok(Prepared { rows, groups, symbols, relocations, frames, plan })
}

impl From<Relocation> for ModuleRelocation {
    fn from(Relocation { offset, kind, target }: Relocation) -> Self {
        let target = match target {
            Target::Section(section, value) => ModuleTarget::Section { section, value },
            Target::Import(import) => ModuleTarget::Import(crate::ast::ExternalName {
                library: import.dll, symbol: import.name,
            }),
            Target::Symbol(_) => unreachable!(),
        };
        Self { offset, kind, target }
    }
}
