//! Share native import members between static hosts and MSVC.
use super::Result;
use crate::object::{self, HostMember, ImportKey};
use std::{borrow::Cow, path::Path};

unsafe extern "C" {
    #[link_name = "bootstrap.names::order_name_pairs"]
    fn order_import_keys(pairs: *mut ImportKey<'_>, count: u64) -> u64;
}

pub(super) fn archive_with_imports(
    members: &[HostMember<'_>], symbols: &[(usize, Cow<'_, str>)], mut keys: Vec<ImportKey<'_>>,
) -> Result<Vec<u8>> {
    let count = unsafe { order_import_keys(keys.as_mut_ptr(), keys.len() as u64) } as usize;
    let imported = keys.into_iter().take(count).map(object::native_import_member).collect::<Result<Vec<_>>>()?;
    let slots = imported.iter().enumerate().map(|(index, (member, slot))| {
        (members.len() + index, &member.name[slot.clone()])
    });
    let names = symbols.iter().map(|(index, name)| (*index, name.as_ref())).chain(slots);
    let views = members.iter().map(HostMember::view).chain(imported.iter().map(|(member, _)| member.view()));
    object::library_bytes(views, names)
}

/// 保存已编译模块及其原生导入材料，供C／C++／Rust静态宿主链接。
/// 不读取B源码、不生成执行代码；原生IAT成员不作为B接口模块装入。
pub fn host_archive(inputs: &[impl AsRef<Path>], path: &Path) -> Result<()> {
    let source = object::library_members(inputs)?;
    let mut members = Vec::new();
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    for original in &source {
        original.emit_host_members(&mut imports, |member, names| {
            let index = members.len();
            symbols.extend(names.into_iter().map(|name| (index, name)));
            members.push(member);
        })?;
    }
    let output = archive_with_imports(&members, &symbols, imports)?;
    if let Some(parent) = path.parent() {
        crate::file_bytes::create_dir_all(parent)?;
    }
    crate::file_bytes::write(&path, &output)?;
    Ok(())
}
