//! 原生宿主导入材料；B生成实际IAT及关联，Rust持有最终COFF容器。
use super::Result;
use crate::object::{self, Definition, ImportKey, LibraryMember, Relocation, Section, Target};
use super::resources::ImportBytes;
use std::{ops::Range, path::Path};

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
struct NativeImportOutput {sections:*mut NativeImportSection,relocations:*mut NativeImportRelocation,member:*mut u8}
#[derive(Clone,Copy)]
#[repr(C)]
struct NativeImportPlan {slot_start:u64,slot_end:u64,member:u64,hint:u64,relocations:u64,sections:u64,error:u64}
unsafe extern "C" {
    #[link_name="bootstrap.names::order_name_pairs"]
    fn order_import_keys(pairs: *mut ImportKey<'_>, count: u64) -> u64;
    #[link_name="bootstrap.native_import::plan_import"]
    fn plan_import(input:NativeImportInput,output:NativeImportOutput)->NativeImportPlan;
    #[link_name="bootstrap.native_import::encode_import"]
    fn encode_import(input:NativeImportInput,output:NativeImportOutput,plan:NativeImportPlan);
}
fn member(import: ImportKey<'_>) -> Result<(LibraryMember, Range<usize>)> {
    let input=NativeImportInput{dll:ImportBytes::new(import.dll()),name:ImportBytes::new(import.symbol()),runtime:ImportBytes::new(crate::application::RUNTIME_LIBRARY)};
    let mut rows=[NativeImportSection{name:0,length:0,flags:0,owner:0,bytes:std::ptr::null_mut()};5];
    let mut relocations=[NativeImportRelocation{section:0,offset:0,kind:0,target:0,target_section:0,value:0};5];
    let mut output=NativeImportOutput{sections:rows.as_mut_ptr(),relocations:relocations.as_mut_ptr(),member:std::ptr::null_mut()};
    let plan=unsafe{plan_import(input,output)};
    match plan.error{0=>{},1=>return Err("原生导入材料超出COFF格式范围".into()),code=>return Err(format!("原生导入布局失败 {code}").into())}
    let rows = &mut rows[..usize::try_from(plan.sections)?];
    let names=[".idata$2",".idata$3",".idata$4",".idata$5",".idata$6",".rdata"];
    let mut sections:Vec<_>=rows.iter_mut().map(|row|->Result<Section>{
        let mut bytes=Vec::with_capacity(usize::try_from(row.length)?);row.bytes=bytes.as_mut_ptr();
        Ok(Section{name:names[usize::try_from(row.name)?].into(),bytes,flags:u32::try_from(row.flags)?,
            relocations:Vec::new(),associated:Vec::new(),layout:None})
    }).collect::<Result<_>>()?;
    let member_length = usize::try_from(plan.member)?;
    let slot = usize::try_from(plan.slot_start)?..usize::try_from(plan.slot_end)?;
    let mut member = Vec::with_capacity(member_length);
    output.member = member.as_mut_ptr();
    unsafe{encode_import(input,output,plan);}
    // B initializes the actual final buffers. Inputs and all allocations remain
    // owned here until encoding and the same COFF consumer both finish.
    for(section,row)in sections.iter_mut().zip(rows.iter()){unsafe{section.bytes.set_len(usize::try_from(row.length)?);}}
    let member = unsafe {
        member.set_len(member_length);
        String::from_utf8_unchecked(member)
    };
    for row in &relocations[..usize::try_from(plan.relocations)?]{
        let target=match row.target{
            0=>Target::Section(usize::try_from(row.target_section)?,u32::try_from(row.value)?),
            1=>Target::Symbol(import.symbol().to_owned()),code=>return Err(format!("未知原生导入目标 {code}").into()),
        };
        sections[usize::try_from(row.section)?].relocations.push(Relocation{offset:usize::try_from(row.offset)?,kind:u16::try_from(row.kind)?,target});
    }
    for(index,row)in rows.iter().enumerate(){if row.owner!=0{sections[usize::try_from(row.owner-1)?].associated.push(index);}}
    let symbol = Definition { name: &member[slot.clone()], section: 0, value: 0, function: false };
    let bytes = object::encode_native(&member[..slot.end], &sections, std::slice::from_ref(&symbol))?;
    Ok((LibraryMember::new(member, bytes), slot))
}

pub(super) fn archive_with_imports(
    members: &[LibraryMember], symbols: &[(usize, String)], mut keys: Vec<ImportKey<'_>>,
) -> Result<Vec<u8>> {
    let count = unsafe { order_import_keys(keys.as_mut_ptr(), keys.len() as u64) } as usize;
    let imported = keys.into_iter().take(count).map(member).collect::<Result<Vec<_>>>()?;
    let slots = imported.iter().enumerate().map(|(index, (member, slot))| {
        (members.len() + index, &member.name[slot.clone()])
    });
    let names = symbols.iter().map(|(index, name)| (*index, name.as_str())).chain(slots);
    object::library_bytes(members.iter().chain(imported.iter().map(|(member, _)| member)), names)
}

/// 保存已编译模块及其原生导入材料，供C／C++／Rust静态宿主链接。
/// 不读取B源码、不生成执行代码；原生IAT成员不作为B接口模块装入。
pub fn host_archive(inputs: &[impl AsRef<Path>], path: &Path) -> Result<()> {
    let source = object::library_members(inputs)?;
    let mut members = Vec::new();
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    for original in &source {
        for (member, names) in original.host_members(&mut imports)? {
            let index = members.len();
            symbols.extend(names.into_iter().map(|name| (index, name)));
            members.push(member);
        }
    }
    let output = archive_with_imports(&members, &symbols, imports)?;
    if let Some(parent) = path.parent() {
        crate::file_bytes::create_dir_all(parent)?;
    }
    crate::file_bytes::write(&path, &output)?;
    Ok(())
}
