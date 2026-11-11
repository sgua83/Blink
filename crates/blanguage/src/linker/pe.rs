//! PE format is produced by the statically compiled B module. Object bytes remain
//! borrowed until encoding ends; the shared machine relocation owner patches the
//! same final output, also serving raw VM loading.
use super::Result;
use crate::object::{Kind, Object, Target};

#[repr(C)]
struct PeSection {
    name:*const u8, name_length:u64, bytes:*const u8, byte_length:u64,
    relocation_start:u64, relocation_count:u64, flags:u32, rva:u32, raw:u64,
}
#[repr(C)]
struct PeRelocation {
    offset:u64, kind:u32, target_kind:u32, section:u64, value:u64,
    source:u64, target:u64,
}
#[repr(C)]
struct PeImport {
    dll:*const u8, dll_length:u64, name:*const u8, name_length:u64, relocation:u64,
    lookup:u64, iat:u64, hint:u64, dll_at:u64, directory:u64,
}
#[derive(Clone,Copy,Default)]
#[repr(C)]
struct PeAbsolute {page:u64,value:u64,order:u64}
#[derive(Clone,Copy)]
#[repr(C)]
struct PeInput {
    sections:*mut PeSection, section_count:u64,
    relocations:*mut PeRelocation,
    imports:*mut PeImport, import_count:u64,
    absolutes:*mut PeAbsolute, absolute_count:u64, stack_bytes:u64,
}
#[derive(Clone,Copy)]
#[repr(C)]
struct PeLayout {
    length:u64,header:u64,idata:u64,idata_length:u64,import_directory_size:u64,
    iat_start:u64,iat_size:u64,reloc:u64,reloc_length:u64,pdata:u64,pdata_length:u64,
    section_count:u64,image_size:u64,image_base:u64,error:u64,
}
unsafe extern "C" {
    #[link_name="bootstrap.pe::layout"]
    fn pe_layout(input:PeInput)->PeLayout;
    #[link_name="bootstrap.pe::encode"]
    fn pe_encode(input:PeInput,layout:PeLayout,output:*mut u8);
    #[link_name="bootstrap.pe::stack_reserve"]
    fn pe_stack_reserve(stack_bytes:u32)->u64;
}
pub(super) fn stack_reserve(stack_bytes:u32)->u64 {unsafe{pe_stack_reserve(stack_bytes)}}

pub(super) fn link(object:&Object)->Result<Vec<u8>> {
    let stack_bytes=match object.kind {
        Kind::Application{stack_bytes}=>stack_bytes,
        _=>return Err("需要应用目标文件".into()),
    };
    let mut sections=Vec::with_capacity(object.sections.len());
    let mut relocations=Vec::new();
    let mut imports=Vec::new();
    let mut absolute_count=0;
    for section in &object.sections {
        let start=relocations.len();
        for relocation in &section.relocations {
            let (target_kind,target_section,value)=match &relocation.target {
                Target::Section(index,value)=>(0,*index as u64,u64::from(*value)),
                Target::Import(import)=>{
                    imports.push(PeImport{
                        dll:import.dll.as_ptr(),dll_length:import.dll.len() as u64,
                        name:import.name.as_ptr(),name_length:import.name.len() as u64,
                        relocation:relocations.len() as u64,lookup:0,iat:0,hint:0,dll_at:0,directory:0,
                    });
                    (1,0,0)
                },
                Target::Symbol(name)=>return Err(format!("未定义模块符号 {name}").into()),
            };
            absolute_count+=usize::from(relocation.kind==1);
            relocations.push(PeRelocation{
                offset:relocation.offset as u64,kind:u32::from(relocation.kind),
                target_kind,section:target_section,value,source:0,target:0,
            });
        }
        sections.push(PeSection{
            name:section.name.as_ptr(),name_length:section.name.len() as u64,
            bytes:section.bytes.as_ptr(),byte_length:section.bytes.len() as u64,
            relocation_start:start as u64,relocation_count:section.relocations.len() as u64,
            flags:section.flags,rva:0,raw:0,
        });
    }
    let mut absolutes=vec![PeAbsolute::default();absolute_count];
    let input=PeInput{
        sections:sections.as_mut_ptr(),section_count:sections.len() as u64,
        relocations:relocations.as_mut_ptr(),
        imports:imports.as_mut_ptr(),import_count:imports.len() as u64,
        absolutes:absolutes.as_mut_ptr(),absolute_count:absolutes.len() as u64,stack_bytes:u64::from(stack_bytes),
    };
    // Every pointer borrows its original Object or this call's final metadata
    // allocation. B cannot retain them, and encoding does not resize any input.
    let layout=unsafe{pe_layout(input)};
    match layout.error {
        0=>{},
        1=>return Err("PE映像或节目录超出格式范围".into()),
        2=>return Err("重定位引用未知节".into()),
        4=>return Err("应用目标文件缺少展开信息".into()),
        code=>return Err(format!("PE格式生成失败 {code}").into()),
    }
    let mut output=vec![0;usize::try_from(layout.length)?];
    unsafe{pe_encode(input,layout,output.as_mut_ptr());}
    for section in &sections {
        let raw=usize::try_from(section.raw)?;
        let bytes=&mut output[raw..raw+usize::try_from(section.byte_length)?];
        let start=usize::try_from(section.relocation_start)?;
        let count=usize::try_from(section.relocation_count)?;
        for relocation in &relocations[start..start+count] {
            crate::native::coff_relocate(bytes,usize::try_from(relocation.offset)?,relocation.kind as u16,
                layout.image_base,relocation.source,relocation.target)?;
        }
    }
    Ok(output)
}
