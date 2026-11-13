//! B owns grouping, layout, symbol/stack/relocation rebasing and payload copying.
//! The original objects stay alive until synchronous encoding ends. Rust keeps
//! the final Object, symbol map and resource owners; no input payload is cloned.
use super::{Result, resources::ImportBytes};
use crate::object::{Kind,Object,Section,Target,stack::Call};
use std::collections::BTreeMap;

#[repr(C)]
struct CombineObject {section_start:u64,section_count:u64,symbol_start:u64,symbol_count:u64,frame_start:u64,frame_count:u64,stack_bytes:u64}
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
struct CombineFrame {section:u64,start:u64,call_start:u64,call_count:u64,present:u64}
#[derive(Clone,Copy)]
#[repr(C)]
struct CombineInput {
    objects:*const CombineObject,object_count:u64,sections:*mut CombineSection,section_count:u64,
    symbols:*mut CombineSymbol,symbol_count:u64,relocations:*mut CombineRelocation,relocation_count:u64,
    frames:*mut CombineFrame,calls:*mut u64,order:*mut u64,
}
#[repr(C)]
struct CombinePlan {groups:u64,symbols:u64,stack_bytes:u64,error:u64,at:u64}
unsafe extern "C" {
    #[link_name="bootstrap.combine::prepare"]
    fn combine_prepare(input: CombineInput, local: *const ImportBytes, local_count: usize) -> CombinePlan;
    #[link_name="bootstrap.combine::encode"]
    fn combine_encode(input:CombineInput,outputs:*const *mut u8);
}

pub(super) fn objects(
    objects: Vec<Object>, module: Option<(&[&str], &mut BTreeMap<String, (usize, u32)>)>,
) -> Result<Object> {
    let name=objects[0].name.clone();
    let mut inputs=Vec::with_capacity(objects.len());
    let mut rows=Vec::new();let mut symbols=Vec::new();let mut relocations=Vec::new();
    let mut frames=Vec::new();let mut calls=Vec::new();
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
            rows.push(CombineSection{name:section.name.as_ptr(),name_length:section.name.len() as u64,
                bytes:section.bytes.as_ptr(),byte_length:section.bytes.len() as u64,flags:u64::from(section.flags),object:object_index as u64,
                owner:0,group:0,begin:0,size:0,padding:0});
        }
        for symbol in &object.symbols {
            symbols.push(CombineSymbol{name:symbol.name.as_ptr(),name_length:symbol.name.len() as u64,
                section:symbol.section as u64,value:u64::from(symbol.value)});
        }
        for frame in &object.stack {
            let call_start=calls.len();
            if frame.coff.is_some(){for call in &frame.calls {if let Call::Reference{offset,..}=call{calls.push(*offset as u64);}}}
            let (section,start,present)=match frame.coff {Some((section,start))=>(section as u64,u64::from(start),1),None=>(0,0,0)};
            frames.push(CombineFrame{section,start,present,call_start:call_start as u64,call_count:(calls.len()-call_start) as u64});
        }
        let stack_bytes=match object.kind{Kind::Application{stack_bytes}|Kind::Module{stack_bytes,..}=>u64::from(stack_bytes),_=>0};
        inputs.push(CombineObject{section_start:section_start as u64,section_count:object.sections.len() as u64,
            symbol_start:symbol_start as u64,symbol_count:object.symbols.len() as u64,frame_start:frame_start as u64,frame_count:object.stack.len() as u64,stack_bytes});
    }
    // The same temporary permutation serves grouping, then final symbol lookup.
    let mut order=vec![0u64;rows.len().max(symbols.len())];
    let input=CombineInput{objects:inputs.as_ptr(),object_count:inputs.len() as u64,
        sections:rows.as_mut_ptr(),section_count:rows.len() as u64,symbols:symbols.as_mut_ptr(),symbol_count:symbols.len() as u64,
        relocations:relocations.as_mut_ptr(),relocation_count:relocations.len() as u64,frames:frames.as_mut_ptr(),
        calls:calls.as_mut_ptr(),order:order.as_mut_ptr()};
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
    let mut sections=Vec::with_capacity(usize::try_from(plan.groups)?);
    for (i,(section,row)) in objects.iter().flat_map(|object|&object.sections).zip(&rows).enumerate(){
        if row.owner==i as u64{sections.push(Section{name:section.name.clone(),bytes:Vec::with_capacity(usize::try_from(row.size)?),
            flags:u32::try_from(row.flags)?,relocations:Vec::new(),associated:Vec::new(),layout:None});}
    }
    let outputs:Vec<_>=sections.iter_mut().map(|section|section.bytes.as_mut_ptr()).collect();
    unsafe{combine_encode(input,outputs.as_ptr());}
    // Encoding writes every byte, including the real alignment gaps, exactly once.
    for (i,row) in rows.iter().enumerate(){if row.owner==i as u64{unsafe{sections[usize::try_from(row.group)?].bytes.set_len(usize::try_from(row.size)?);}}}
    if let Some((_, defined))=module{for &index in &order[..usize::try_from(plan.symbols)?]{
        let symbol=&symbols[usize::try_from(index)?];
        let name=unsafe{std::str::from_utf8_unchecked(std::slice::from_raw_parts(symbol.name,symbol.name_length as usize))};
        defined.insert(name.to_owned(),(usize::try_from(symbol.section)?,u32::try_from(symbol.value)?));
    }}
    let mut stack=Vec::with_capacity(frames.len());
    let mut section_index=0;let mut relocation_index=0;let mut frame_index=0;
    for object in objects {
        for mut frame in object.stack {
            let row=&frames[frame_index];frame_index+=1;
            if let Some((section,start))=&mut frame.coff {
                *section=usize::try_from(row.section)?;*start=u32::try_from(row.start)?;
                let mut index=usize::try_from(row.call_start)?;
                for call in &mut frame.calls {if let Call::Reference{offset,..}=call{*offset=usize::try_from(calls[index])?;index+=1;}}
            }
            stack.push(frame);
        }
        for section in object.sections {
            let group=usize::try_from(rows[section_index].group)?;section_index+=1;
            for mut relocation in section.relocations {
                let row=&relocations[relocation_index];relocation_index+=1;relocation.offset=usize::try_from(row.offset)?;
                if row.target_kind==0{relocation.target=Target::Section(usize::try_from(row.section)?,u32::try_from(row.value)?);}
                sections[group].relocations.push(relocation);
            }
        }
    }
    Ok(Object{name,kind:Kind::Application{stack_bytes:u32::try_from(plan.stack_bytes)?},sections,stack,
        symbols:Vec::new(),imports:BTreeMap::new(),archive:None})
}
