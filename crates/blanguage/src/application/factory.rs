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

#[repr(C)]
struct FrameCreateCode {
    length: u64,
    factory_field: u64,
    count_field: u64,
    target_field: u64,
}

#[repr(C)]
struct ResourceTailCode {
    length: u64,
    base_field: u64,
    target_field: u64,
}

#[repr(C)]
struct ProgramUniformArgument {
    tag: u32,
    scalar: crate::builtin_types::ScalarEncoding,
}

#[repr(C)]
struct ProgramCallLayout {
    frame: u32,
    datasets: u32,
    values: u32,
    textures: u32,
    capacity: u64,
}

#[repr(C)]
struct ProgramCallCode {
    length: u64,
    function_end: u32,
    prolog: u32,
    base_field: u64,
    call_field: u64,
}

#[repr(C)]
struct CompletionLimit {
    parameter: u32,
    distance: u32,
}

#[repr(C)]
struct CompletionField {
    offset: u32,
    interval_count: u32,
    known_mask: u64,
    known_value: u64,
    limits: *const CompletionLimit,
    limit_count: u32,
}

#[repr(C)]
struct CompletionLayout {
    frame: u32,
    capacity: u64,
}

#[repr(C)]
struct CompletionPatch {
    intervals: u64,
    call: u64,
}

#[repr(C)]
struct CompletionCode {
    length: u64,
    function_end: u32,
    prolog: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::frame_create_code"]
    fn frame_create_code(output: *mut u8) -> FrameCreateCode;
    #[link_name = "bootstrap.x64_instruction::resource_allocate_code"]
    fn resource_allocate_code(index: u32, output: *mut u8) -> ResourceTailCode;
    #[link_name = "bootstrap.x64_instruction::task_start_code"]
    fn task_start_code(output: *mut u8) -> ResourceTailCode;
    #[link_name = "bootstrap.x64_instruction::program_call_layout"]
    fn program_call_layout(datasets: u32, values: u32, textures: u32) -> ProgramCallLayout;
    #[link_name = "bootstrap.x64_instruction::program_call_code"]
    fn program_call_code(
        layout: ProgramCallLayout,
        program: u32,
        datasets: u32,
        arguments: *const ProgramUniformArgument,
        values: u32,
        textures: u32,
        output: *mut u8,
    ) -> ProgramCallCode;
    #[link_name = "bootstrap.x64_instruction::completion_layout"]
    fn completion_layout(fields: *const CompletionField, count: u32) -> CompletionLayout;
    #[link_name = "bootstrap.x64_instruction::completion_code"]
    fn completion_code(
        layout: CompletionLayout,
        fields: *const CompletionField,
        count: u32,
        output: *mut u8,
        patches: *mut CompletionPatch,
    ) -> CompletionCode;
}

fn external(offset: u64, name: &str) -> Relocation {
    Relocation { offset: offset as usize, kind: 4, target: Target::Symbol(name.into()) }
}

fn wrapper(
    sections: &mut Vec<Section>,
    symbols: &mut Vec<Symbol>,
    name: String,
    code: Vec<u8>,
    relocations: Vec<Relocation>,
    unwind: Option<(u32, Vec<u8>)>,
) {
    let section = crate::object::function_sections(sections, code, unwind, None);
    symbols.push(Symbol {
        name, section, value: 0, contract: Vec::new(), function: true,
    });
    sections[section].relocations = relocations;
}

fn call_unwind(prolog: u32, frame: u32) -> Vec<u8> {
    let prolog = prolog as u8;
    crate::native::unwind_info(prolog, &[crate::native::UnwindOperation::allocate(prolog, frame)])
}

pub(super) fn wrappers(
    prefix: &str,
    resources: &super::Resources,
    types: u32,
    root: bool,
    tasks: bool,
) -> crate::object::Result<Option<Vec<u8>>> {
    let mut sections = Vec::new();
    let mut symbols = Vec::new();
    let mut code = [0u8; 32];
    if root {
        let row = unsafe { frame_create_code(code.as_mut_ptr()) };
        wrapper(&mut sections, &mut symbols, "blink_create".into(), code[..row.length as usize].to_vec(), vec![
            external(row.factory_field, "__blink_module_factories"),
            external(row.count_field, "__blink_module_count"),
            external(row.target_field, "blink_create_frame"),
        ], None);
    }
    let base = format!("{prefix}_type_base");
    for index in 0..types {
        let row = unsafe { resource_allocate_code(index, code.as_mut_ptr()) };
        wrapper(&mut sections, &mut symbols, format!("{prefix}_allocate_{index}"), code[..row.length as usize].to_vec(), vec![
            external(row.base_field, &base), external(row.target_field, "blink_allocate"),
        ], None);
    }
    if tasks {
        let row = unsafe { task_start_code(code.as_mut_ptr()) };
        wrapper(&mut sections, &mut symbols, format!("{prefix}_task_start"), code[..row.length as usize].to_vec(), vec![
            external(row.base_field, &base), external(row.target_field, "blink_task_start"),
        ], None);
    }
    let base = format!("{prefix}_program_base");
    for (index, (_, program)) in resources.programs.iter().enumerate() {
        let arguments: Vec<_> = program.uniforms.iter().flat_map(|uniform| &uniform.ty.components)
            .map(|&(scalar, _)| ProgramUniformArgument {
                tag: crate::resource::scalar_tag(scalar),
                scalar: scalar.into(),
            }).collect();
        let datasets = u32::try_from(program.buffers.len())?;
        let values = u32::try_from(arguments.len())?;
        let textures = u32::try_from(program.textures.len())?;
        let layout = unsafe { program_call_layout(datasets, values, textures) };
        if layout.capacity == 0 {
            return Err("应用程序包装超出 Windows x64 栈位移或代码范围".into());
        }
        let frame = layout.frame;
        let mut code = Vec::with_capacity(layout.capacity as usize);
        let row = unsafe {
            let row = program_call_code(layout, u32::try_from(index)?, datasets, arguments.as_ptr(),
                values, textures, code.as_mut_ptr());
            code.set_len(row.length as usize);
            row
        };
        wrapper(&mut sections, &mut symbols, format!("{prefix}_run_{index}"), code, vec![
            external(row.base_field, &base), external(row.call_field, "blink_run"),
        ], Some((row.function_end, call_unwind(row.prolog, frame))));
    }
    for (index, fields) in resources.completions.iter().enumerate() {
        let limits: Vec<Vec<_>> = fields.iter().map(|field| {
            field.limits.iter().map(|&(parameter, distance)| {
                Ok::<_, std::num::TryFromIntError>(CompletionLimit {
                    parameter: u32::try_from(parameter)?,
                    distance,
                })
            }).collect::<Result<Vec<_>, _>>()
        }).collect::<Result<_, _>>()?;
        let native_fields: Vec<_> = fields.iter().zip(&limits).map(|(field, limits)| {
            Ok::<_, std::num::TryFromIntError>(CompletionField {
                offset: field.offset,
                interval_count: u32::try_from(field.intervals.len())?,
                known_mask: field.known_bits.0,
                known_value: field.known_bits.1,
                limits: limits.as_ptr(),
                limit_count: u32::try_from(limits.len())?,
            })
        }).collect::<Result<_, _>>()?;
        let count = u32::try_from(native_fields.len())?;
        let layout = unsafe { completion_layout(native_fields.as_ptr(), count) };
        if layout.capacity == 0 {
            return Err("应用完成包装超出 Windows x64 栈位移或代码范围".into());
        }
        let frame = layout.frame;
        let mut code = Vec::with_capacity(layout.capacity as usize);
        let mut patches = Vec::with_capacity(fields.len());
        let row = unsafe {
            let row = completion_code(layout, native_fields.as_ptr(), count, code.as_mut_ptr(), patches.as_mut_ptr());
            code.set_len(row.length as usize);
            patches.set_len(fields.len());
            row
        };

        let data_section = sections.len();
        let mut data = Vec::new();
        let mut relocations = Vec::with_capacity(fields.len() * 2);
        for (field, patch) in fields.iter().zip(patches) {
            relocations.push(Relocation {
                offset: patch.intervals as usize,
                kind: 4,
                target: Target::Section(data_section, u32::try_from(data.len())?),
            });
            relocations.push(external(patch.call, "blink_complete_field"));
            for (lower, upper) in &field.intervals {
                data.extend(lower.to_le_bytes());
                data.extend(upper.to_le_bytes());
            }
        }
        if data.is_empty() { data.resize(8, 0); }
        sections.push(read_only(data, Vec::new()));
        wrapper(&mut sections, &mut symbols, format!("{prefix}_complete_{index}"), code,
            relocations, Some((row.function_end, call_unwind(row.prolog, frame))));
    }
    if symbols.is_empty() {
        Ok(None)
    } else {
        Ok(Some(crate::object::encode_native(prefix, &sections, &symbols)?))
    }
}
