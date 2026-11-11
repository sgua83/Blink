//! 原始CPU映像只消费原链接器已选择的COFF，不产生函数代码。
use super::{Result, Roots, combine, partition, select};
use crate::{native::{ModuleCode, ModuleSection, ModuleRelocation, ModuleTarget}, object::{Input, Target}};
use std::collections::BTreeMap;
pub(crate) fn modules(inputs: Vec<Input>, borrowed: &[&crate::object::Object], roots: &BTreeMap<String, Vec<u8>>, local: &std::collections::BTreeSet<&str>, types: u32, programs: u32) -> Result<Option<ModuleCode>> {
    let (mut objects, libraries) = partition(inputs);
    objects.extend(borrowed.iter().copied().map(super::Source::Borrowed));
    let objects = select(objects, libraries, Roots::Symbols(roots, types, programs))?;
    if objects.is_empty() { return Ok(None); }
    let mut symbols = BTreeMap::new();
    let object = combine::objects(objects,Some(&mut symbols))?;
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
