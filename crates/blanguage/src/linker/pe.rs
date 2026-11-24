//! B lays out the final PE and writes its headers and directories.
//! The section writer fills the same file before shared machine relocation.
use super::{Result, combine::{CombineGroup, CombinePlan, CombineRelocation, CombineSection}};
use crate::native::CoffRelocationStatus;

#[repr(C)]
struct PeSection {
    name:*const u8, name_length:u64, byte_length:u64,
    relocation_start:u64, relocation_count:u64, flags:u32, rva:u32, raw:u64,
}
#[repr(C)]
struct PeImport {
    dll:*const u8, dll_length:u64, name:*const u8, name_length:u64, relocation_or_thunk:u64,
    hint:u64, dll_at:u64, directory:u64,
}
#[repr(C)]
struct PeAbsolute {location:u64,order_or_page_end:u64}
#[derive(Clone,Copy)]
#[repr(C)]
struct PeInput {
    sections:*mut PeSection, section_count:u64,
    relocations:*mut CombineRelocation,
    imports:*mut PeImport, import_count:u64,
    absolutes:*mut PeAbsolute, absolute_count:u64, stack_reserve:u64,
}
#[derive(Clone,Copy)]
#[repr(C)]
struct PeLayout {
    length:u64,header:u64,idata:u64,idata_length:u64,import_directory_size:u64,import_count:u64,
    iat_start:u64,iat_size:u64,reloc:u64,reloc_length:u64,pdata:u64,pdata_length:u64,
    section_count:u64,image_size:u64,image_base:u64,error:u64,
}
#[repr(C)]
struct PeRelocationResult {
    status: CoffRelocationStatus,
    kind: u32,
}

unsafe extern "C" {
    #[link_name="bootstrap.pe::layout"]
    fn pe_layout(input:PeInput, rows:*const CombineSection, groups:*const CombineGroup,
        relocation_count:usize)->PeLayout;
    #[link_name="bootstrap.pe::encode"]
    fn pe_encode(input:PeInput,layout:PeLayout,rows:*const CombineSection,
        row_count:usize,output:*mut u8)->PeRelocationResult;
}

pub(super) fn link(
    rows: &[CombineSection], groups: &[CombineGroup], relocations: &mut [CombineRelocation],
    plan: &CombinePlan, stack_reserve: u64,
) -> Result<Vec<u8>> {
    let mut sections = Vec::<PeSection>::with_capacity(groups.len());
    let mut imports = Vec::<PeImport>::with_capacity(plan.imports);
    let mut absolutes = Vec::<PeAbsolute>::with_capacity(plan.absolutes);
    let input = PeInput {
        sections: sections.as_mut_ptr(), section_count: groups.len() as u64,
        relocations: relocations.as_mut_ptr(), imports: imports.as_mut_ptr(),
        import_count: plan.imports as u64, absolutes: absolutes.as_mut_ptr(),
        absolute_count: plan.absolutes as u64, stack_reserve,
    };
    // B groups the combined relocations in place and initializes the physical section and import slots.
    let layout = unsafe {
        pe_layout(input, rows.as_ptr(), groups.as_ptr(), relocations.len())
    };
    match layout.error {
        0=>{},
        1=>return Err("PE映像或节目录超出格式范围".into()),
        2=>return Err("重定位引用未知节".into()),
        4=>return Err("应用目标文件缺少展开信息".into()),
        code=>return Err(format!("PE格式生成失败 {code}").into()),
    }
    let length=usize::try_from(layout.length)?;
    let mut output=Vec::with_capacity(length);
    // Both original plans and their source bytes stay alive through image encoding.
    let relocation = unsafe { pe_encode(input, layout, rows.as_ptr(), rows.len(), output.as_mut_ptr()) };
    relocation.status.result(relocation.kind)?;
    // On success the writer initialized the entire file, including reserved bytes and padding.
    unsafe { output.set_len(length); }
    Ok(output)
}
