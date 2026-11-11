//! 链接期资源基址和工厂地址；只有静态数据与标准重定位，没有运行期注册表。
use super::Result;
use crate::object::{Archive, Kind, Object, Relocation, Section, Symbol, Target};
#[derive(Clone,Copy)]
#[repr(C)]
pub(super) struct ImportBytes {bytes:*const u8,length:u64}
impl ImportBytes {pub(super) fn new(text:&str)->Self{Self{bytes:text.as_ptr(),length:text.len() as u64}}}
unsafe extern "C" {
    #[link_name="bootstrap.bindings::import_name_size"]
    fn binding_import_name_size(dll:ImportBytes,name:ImportBytes)->u64;
    #[link_name="bootstrap.bindings::import_name"]
    fn binding_import_name(dll:ImportBytes,name:ImportBytes,output:*mut u8);
}
pub(super) fn import_symbol(dll:&str,name:&str)->String {
    let dll=ImportBytes::new(dll);let name=ImportBytes::new(name);
    let length=unsafe{binding_import_name_size(dll,name)} as usize;
    let mut output=Vec::with_capacity(length);
    unsafe{binding_import_name(dll,name,output.as_mut_ptr());output.set_len(length);String::from_utf8_unchecked(output)}
}
pub(super) fn resources(archives: &[&Archive], mut types: u32, mut programs: u32, runtime: &[String], local_imports: &std::collections::BTreeMap<String, String>, root_abi: Option<&[u8]>) -> Result<Option<Object>> {
    let modules = archives
        .iter()
        .filter_map(|archive| archive.resources.as_ref())
        .collect::<Vec<_>>();
    if modules.is_empty() {
        return Ok(None);
    }
    // 静态成品的ABI来自根工厂；原始VM的ABI来自实际宿主运行库。
    let abi = match root_abi { Some(abi) => abi, None => modules[0].abi.as_slice() };
    let mut prefixes = std::collections::BTreeSet::new();
    for module in &modules {
        if module.abi.as_slice() != abi {
            return Err(
                "模块资源 C ABI 不一致，须重编为同一实际资源 ABI".into(),
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
    let mut imports = std::collections::BTreeMap::new();
    for (index, module) in modules.into_iter().enumerate() {
        let resource = format!("{}_resources", module.prefix);
        imports.insert(resource.clone(), Vec::new());
        relocations.push(Relocation {
            offset: 8 + 8 * index,
            kind: 1,
            target: Target::Symbol(resource),
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
    for (slot, name) in local_imports {
        imports.insert(name.clone(), Vec::new());
        let value = u32::try_from(bytes.len())?;
        symbols.push(Symbol { name: slot.clone(), section: 0, value, contract: Vec::new(), function: false });
        bytes.extend([0; 8]);
        relocations.push(Relocation { offset: value as usize, kind: 1, target: Target::Symbol(name.clone()) });
    }
    let mut sections = vec![Section {
            associated: Vec::new(), layout: None,
            name: ".rdata".into(),
            bytes,
            flags: 0x40400040,
            relocations,
        }];
    if !runtime.is_empty() {
        let mut bytes = Vec::new();
        let mut relocations = Vec::new();
        for name in runtime {
            let value = u32::try_from(bytes.len())?;
            symbols.push(Symbol { name: name.clone(), section: sections.len(), value, contract: Vec::new(), function: true });
            bytes.extend([0xff, 0x25, 0, 0, 0, 0]);
            relocations.push(Relocation { offset: value as usize + 2, kind: 4,
                target: Target::Import(crate::object::Import { dll: crate::resource::RUNTIME_LIBRARY.into(), name: name.clone() }),
            });
        }
        sections.push(Section { associated: Vec::new(), layout: None, name: ".text".into(),
            bytes, flags: 0x60500020, relocations });
    }
    Ok(Some(Object {
        stack: Vec::new(),
        name: "@blink.resources@".into(),
        kind: Kind::Module {
            interface: String::new(),
            requirements: std::collections::BTreeMap::new(),
            stack_bytes: 0,
        },
        sections,
        symbols,
        imports,
        archive: None,
    }))
}
