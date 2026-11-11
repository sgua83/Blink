//! 原生宿主导入材料；B生成实际IAT及关联，Rust持有最终COFF容器。
use super::Result;
use crate::object::{self,Import,Kind,LibraryMember,Relocation,Section,Symbol,Target};
use super::resources::ImportBytes;
use std::{collections::BTreeSet,path::Path};

#[derive(Clone,Copy)]
#[repr(C)]
struct NativeImportSection {name:u64,length:u64,flags:u64,owner:u64,bytes:*mut u8}
#[derive(Clone,Copy)]
#[repr(C)]
struct NativeImportRelocation {section:u64,offset:u64,kind:u64,target:u64,target_section:u64,value:u64}
#[derive(Clone,Copy)]
#[repr(C)]
struct NativeImportInput {dll:ImportBytes,name:ImportBytes,runtime:ImportBytes}
#[derive(Clone,Copy)]
#[repr(C)]
struct NativeImportOutput {sections:*mut NativeImportSection,relocations:*mut NativeImportRelocation,slot:*mut u8,stem:*mut u8,member:*mut u8}
#[derive(Clone,Copy)]
#[repr(C)]
struct NativeImportPlan {slot:u64,stem:u64,member:u64,hint:u64,relocations:u64,root:u64,error:u64}
unsafe extern "C" {
    #[link_name="bootstrap.native_import::plan_import"]
    fn plan_import(input:NativeImportInput,output:NativeImportOutput)->NativeImportPlan;
    #[link_name="bootstrap.native_import::encode_import"]
    fn encode_import(input:NativeImportInput,output:NativeImportOutput,plan:NativeImportPlan);
}
pub(super) fn member(import:&Import)->Result<(LibraryMember,String)> {
    let input=NativeImportInput{dll:ImportBytes::new(&import.dll),name:ImportBytes::new(&import.name),runtime:ImportBytes::new(crate::application::RUNTIME_LIBRARY)};
    let mut rows=[NativeImportSection{name:0,length:0,flags:0,owner:0,bytes:std::ptr::null_mut()};6];
    let mut relocations=[NativeImportRelocation{section:0,offset:0,kind:0,target:0,target_section:0,value:0};5];
    let mut output=NativeImportOutput{sections:rows.as_mut_ptr(),relocations:relocations.as_mut_ptr(),slot:std::ptr::null_mut(),stem:std::ptr::null_mut(),member:std::ptr::null_mut()};
    let plan=unsafe{plan_import(input,output)};
    match plan.error{0=>{},1=>return Err("原生导入材料超出COFF格式范围".into()),code=>return Err(format!("原生导入布局失败 {code}").into())}
    let names=[".idata$2",".idata$3",".idata$4",".idata$5",".idata$6",".rdata"];
    let mut sections:Vec<_>=rows.iter_mut().map(|row|->Result<Section>{
        let mut bytes=Vec::with_capacity(usize::try_from(row.length)?);row.bytes=bytes.as_mut_ptr();
        Ok(Section{name:names[usize::try_from(row.name)?].into(),bytes,flags:u32::try_from(row.flags)?,
            relocations:Vec::new(),associated:Vec::new(),layout:None})
    }).collect::<Result<_>>()?;
    let mut slot=Vec::with_capacity(usize::try_from(plan.slot)?);let mut stem=Vec::with_capacity(usize::try_from(plan.stem)?);let mut member=Vec::with_capacity(usize::try_from(plan.member)?);
    output.slot=slot.as_mut_ptr();output.stem=stem.as_mut_ptr();output.member=member.as_mut_ptr();
    unsafe{encode_import(input,output,plan);}
    // B initializes the actual final buffers. Inputs and all allocations remain
    // owned here until encoding and the same COFF consumer both finish.
    for(section,row)in sections.iter_mut().zip(&rows){unsafe{section.bytes.set_len(usize::try_from(row.length)?);}}
    unsafe{slot.set_len(usize::try_from(plan.slot)?);stem.set_len(usize::try_from(plan.stem)?);member.set_len(usize::try_from(plan.member)?);}
    let slot=unsafe{String::from_utf8_unchecked(slot)};let stem=unsafe{String::from_utf8_unchecked(stem)};let member=unsafe{String::from_utf8_unchecked(member)};
    for row in &relocations[..usize::try_from(plan.relocations)?]{
        let target=match row.target{
            0=>Target::Section(usize::try_from(row.target_section)?,u32::try_from(row.value)?),
            1=>Target::Symbol(import.name.clone()),code=>return Err(format!("未知原生导入目标 {code}").into()),
        };
        sections[usize::try_from(row.section)?].relocations.push(Relocation{offset:usize::try_from(row.offset)?,kind:u16::try_from(row.kind)?,target});
    }
    for(index,row)in rows.iter().enumerate(){if row.owner!=0{sections[usize::try_from(row.owner-1)?].associated.push(index);}}
    let symbol=Symbol{name:slot,section:0,value:0,contract:Vec::new(),function:false};
    let bytes=object::encode_native(&stem,&sections,std::slice::from_ref(&symbol))?;
    Ok((LibraryMember{name:member,bytes},symbol.name))
}

/// 保存已编译模块及其原生导入材料，供C／C++／Rust静态宿主链接。
/// 不读取B源码、不生成执行代码；原生IAT成员不作为B接口模块装入。
pub fn host_archive(inputs: &[impl AsRef<Path>], path: &Path) -> Result<()> {
    let mut members = object::library_members(inputs)?;
    let mut symbols = Vec::new();
    let mut imports = BTreeSet::new();
    for (index, member) in members.iter().enumerate() {
        let object = member.object(false)?;
        if !matches!(object.kind, Kind::Module { .. }) {
            return Err(format!("{} 不是独立模块产物", member.name).into());
        }
        symbols.extend(object.symbols.into_iter().map(|symbol| (index, symbol.name)));
        imports.extend(object.sections.into_iter().flat_map(|section| section.relocations)
            .filter_map(|relocation| match relocation.target { Target::Import(import) => Some(import), _ => None }));
    }
    for import in imports {
        let (member, symbol) = self::member(&import)?;
        symbols.push((members.len(), symbol));
        members.push(member);
    }
    let output = object::library_bytes(&members, &symbols)?;
    let path = std::path::absolute(path)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, output)?;
    Ok(())
}
