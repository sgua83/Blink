//! 链接期资源基址和工厂地址；只有静态数据与标准重定位，没有运行期注册表。
use super::Result;
use crate::object::{Archive, Kind, Object, Relocation, Section, Symbol, Target};
pub(super) fn resources(archives: &[Archive]) -> Result<Option<Object>> {
    let modules = archives
        .iter()
        .filter_map(|archive| archive.resources.as_ref())
        .collect::<Vec<_>>();
    if modules.is_empty() {
        return Ok(None);
    }
    let abi = &modules[0].abi;
    let mut prefixes = std::collections::BTreeSet::new();
    for module in &modules {
        if &module.abi != abi {
            return Err(
                "模块执行框架的实际 Rust ABI 不一致，须用同一 SDK 和工具链重编资源模块".into(),
            );
        }
        if !prefixes.insert(&module.prefix) {
            return Err(format!("重复的模块资源定义 {}", module.prefix).into());
        }
    }
    let mut bytes = (modules.len() as u64).to_le_bytes().to_vec();
    bytes.resize(8 + 8 * modules.len(), 0);
    let mut symbols = vec![
        Symbol {
            name: "__blink_module_count".into(),
            section: 0,
            value: 0,
            contract: Vec::new(),
            function: false,
        },
        Symbol {
            name: "__blink_module_factories".into(),
            section: 0,
            value: 8,
            contract: Vec::new(),
            function: false,
        },
    ];
    let mut relocations = Vec::new();
    let mut types = 0u32;
    let mut programs = 0u32;
    for (index, module) in modules.into_iter().enumerate() {
        relocations.push(Relocation {
            offset: 8 + 8 * index,
            kind: 1,
            target: Target::Symbol(format!("{}_resources", module.prefix), Vec::new()),
        });
        for (suffix, value) in [("type_base", types), ("program_base", programs)] {
            symbols.push(Symbol {
                name: format!("{}_{suffix}", module.prefix),
                section: 0,
                value: u32::try_from(bytes.len())?,
                contract: Vec::new(),
                function: false,
            });
            bytes.extend(value.to_le_bytes());
        }
        types = types
            .checked_add(module.types)
            .ok_or("模块类型索引超出原生 ABI")?;
        programs = programs
            .checked_add(module.programs)
            .ok_or("模块程序索引超出原生 ABI")?;
    }
    Ok(Some(Object {
        name: "@blink.resources@".into(),
        kind: Kind::Module {
            interface: String::new(),
            stack_bytes: 0,
        },
        sections: vec![Section {
            name: ".rdata".into(),
            bytes,
            flags: 0x40400040,
            relocations,
        }],
        symbols,
        archive: None,
    }))
}
