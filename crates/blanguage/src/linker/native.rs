//! 原始CPU映像只消费原链接器已选择的COFF，不产生函数代码。
use super::{Result, Roots, combine, partition, select};
use crate::{native::{ModuleCode, ModuleSection, ModuleRelocation, ModuleTarget}, object::{Input, Target}};
use std::collections::BTreeMap;
pub(crate) fn modules(inputs: Vec<Input>, roots: &BTreeMap<String, Vec<u8>>, local: &std::collections::BTreeSet<&str>) -> Result<Option<ModuleCode>> {
    let (objects, libraries) = partition(inputs);
    let objects = select(objects, libraries, Roots::Symbols(roots))?;
    if objects.is_empty() { return Ok(None); }
    let (object, archives, symbols) = combine::objects(objects)?;
    if !archives.is_empty() {
        return Err("原始SDK的已编译模块资源目录尚未实现；见C-027.35.4.2.2.3.2.2.2.2.2.3.2".into());
    }
    for name in symbols.keys() {
        if local.contains(name.as_str()) { return Err(format!("重复定义模块符号 {name}").into()); }
    }
    let sections = object.sections.into_iter().map(|section| {
        let relocations = section.relocations.into_iter().map(|relocation| {
            let target = match relocation.target {
                Target::Section(section, value) => ModuleTarget::Section { section, value },
                Target::Import(import) => ModuleTarget::Import(crate::ast::ExternalName {
                    library: import.dll, symbol: import.name,
                }),
                Target::Symbol(_) => unreachable!(),
            };
            ModuleRelocation { offset: relocation.offset, kind: relocation.kind, target }
        }).collect();
        ModuleSection { name: section.name, bytes: section.bytes, flags: section.flags, relocations }
    }).collect();
    Ok(Some(ModuleCode { sections, symbols }))
}
