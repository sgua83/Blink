//! 资源工厂在原编译COFF持有已编码映像和代码地址表。
use crate::object::{Object, Relocation, ResourceBinding, Section, Symbol, Target, stack::Frame};
use std::collections::BTreeMap;

pub(crate) struct ResourceFactory {
    pub(crate) binding: ResourceBinding,
    pub(crate) image: Vec<u8>,
    pub(crate) ranges_symbol: Option<String>,
}

#[repr(C)]
struct ModuleFactoryCode {
    length: u64,
    image_field: u64,
    ranges_field: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::module_factory_code"]
    fn module_factory_code(bytes: u64, codes: u64, output: *mut u8) -> ModuleFactoryCode;
}

impl ResourceFactory {
    pub(crate) fn emit(
        self, object: &mut Object, positions: &BTreeMap<String, (usize, u32)>,
    ) {
        let Self { mut binding, image, ranges_symbol } = self;
        let factory = format!("{}_resources", binding.prefix);
        let bytes = image.len() as u64;
        let codes = binding.code.len() as u64;
        let image_section = object.sections.len();
        object.sections.push(read_only(image, Vec::new()));
        let ranges_section = if binding.code.is_empty() {
            // An empty slice still carries a valid aligned address.
            image_section
        } else {
            let relocations = binding.code.iter().enumerate().map(|(index, name)| Relocation {
                offset: index * 8, kind: 1, target: Target::Section(positions[name].0, 0),
            }).collect();
            let section = object.sections.len();
            object.sections.push(read_only(vec![0; binding.code.len() * 8], relocations));
            section
        };
        let mut entry = vec![0; 64];
        let layout = unsafe { module_factory_code(bytes, codes, entry.as_mut_ptr()) };
        entry.truncate(layout.length as usize);
        let section = object.sections.len();
        object.sections.push(Section {
            name: ".text".into(), bytes: entry, flags: 0x60501020,
            relocations: vec![
                Relocation { offset: layout.image_field as usize, kind: 1, target: Target::Section(image_section, 0) },
                Relocation { offset: layout.ranges_field as usize, kind: 1, target: Target::Section(ranges_section, 0) },
            ],
            associated: Vec::new(), layout: None,
        });
        object.symbols.push(Symbol {
            name: factory.clone(), section, value: 0, contract: Vec::new(), function: true,
        });
        if let Some(name) = ranges_symbol {
            object.symbols.push(Symbol {
                name, section: ranges_section, value: 0, contract: Vec::new(), function: false,
            });
        }
        object.stack.push(Frame { name: factory.clone(), coff: Some((section, 0)), local: 0, calls: Vec::new() });
        binding.code.clear();
        binding.code.push(factory);
        object.archive.as_mut().unwrap().resources = Some(binding);
    }
}

fn read_only(bytes: Vec<u8>, relocations: Vec<Relocation>) -> Section {
    Section {
        name: ".rdata".into(), bytes, flags: 0x40401040, relocations,
        associated: Vec::new(), layout: None,
    }
}
