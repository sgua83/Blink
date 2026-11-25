//! 算法DLL的只读映像、代码地址与原生宿主入口。
use crate::{
    ExecutableProgram, ast::Target,
    application::factory::{ResourceFactory, call_unwind, external, read_only, wrapper},
    object::{Relocation, Section, Target as Address},
    resource::ModuleImage,
};

#[repr(C)]
struct LibraryRunLayout {
    frame: u32,
    uniforms: u32,
    capacity: u64,
}

#[repr(C)]
struct LibraryEntryCode {
    length: u64,
    function_end: u32,
    prolog: u32,
    frame: u32,
    call: u64,
}

#[repr(C)]
struct LibraryCreateCode {
    entry: LibraryEntryCode,
    cpu: u64,
    gpu: u64,
    factories: u64,
    count: u64,
}

#[repr(C)]
struct SymbolEncoding {
    length: u64,
    field: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::library_create_code"]
    fn library_create_code(cpu: u32, gpu: u32, factories: u32, code: *mut u8) -> LibraryCreateCode;
    #[link_name = "bootstrap.x64_instruction::library_run_layout"]
    fn library_run_layout(datasets: u32, dataset_addresses: u32, uniforms: u32) -> LibraryRunLayout;
    #[link_name = "bootstrap.x64_instruction::library_run_code"]
    fn library_run_code(
        layout: LibraryRunLayout, dataset_addresses: *const u8, datasets: u32, uniforms: u32, code: *mut u8,
    ) -> LibraryEntryCode;
    #[link_name = "bootstrap.x64_instruction::library_destroy_code"]
    fn library_destroy_code(code: *mut u8) -> SymbolEncoding;
}

fn data(sections: &mut Vec<Section>, bytes: Vec<u8>, relocations: Vec<Relocation>) -> usize {
    let index = sections.len();
    sections.push(read_only(bytes, relocations));
    index
}

fn image(
    sections: &mut Vec<Section>, image: Vec<u8>, code: Option<(Address, usize)>,
) -> usize {
    use std::mem::{offset_of, size_of};
    let bytes_at = offset_of!(ModuleImage, bytes);
    let codes_at = offset_of!(ModuleImage, codes);
    let mut bytes = vec![0; size_of::<ModuleImage>()];
    bytes[bytes_at..bytes_at + size_of::<usize>()].copy_from_slice(&image.len().to_le_bytes());
    let section = data(sections, image, Vec::new());
    let mut relocations = vec![Relocation {
        offset: offset_of!(ModuleImage, data), kind: 1, target: Address::Section(section, 0),
    }];
    if let Some((code, count)) = code {
        bytes[codes_at..codes_at + size_of::<usize>()].copy_from_slice(&count.to_le_bytes());
        relocations.push(Relocation { offset: offset_of!(ModuleImage, code), kind: 1, target: code });
    }
    data(sections, bytes, relocations)
}

pub(super) fn compile(
    p: &ExecutableProgram, prefix: &str, dataset_addresses: &[u8], uniforms: u32, worker: &crate::compiler::Worker,
) -> crate::object::Result<(crate::object::ObjectBytes, Vec<(String, crate::native::NativeCode)>, Option<ResourceFactory>)> {
    let mut sections = Vec::new();
    let mut symbols = Vec::new();
    let mut functions = Vec::new();
    let mut resources = None;
    let cpu = if p.target() == Target::Gpu {
        None
    } else {
        let mut entry = |name: Option<&str>, code| {
            let index = functions.len();
            let name = name.map(str::to_owned).unwrap_or_else(|| format!("__blink_range_{prefix}_{index}"));
            functions.push((name, code));
            index
        };
        let root = crate::native::program::compile_linked(worker, p, &mut entry).map_err(std::io::Error::other)?;
        let code = if let Some(directory) = &p.resources {
            let image = crate::resource::compile_linked(directory, worker, |code| entry(None, code))
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let resource_prefix = crate::application::resource_prefix(p.name());
            let ranges = format!("{resource_prefix}_ranges");
            resources = Some(ResourceFactory::new(
                resource_prefix, directory.types.len().try_into()?, directory.programs.len().try_into()?,
                image, Some(ranges.clone()),
            ));
            Address::Symbol(ranges)
        } else if functions.is_empty() {
            // The CPU runtime borrows an aligned slice even when it has no entries.
            Address::Section(sections.len(), 0)
        } else {
            let relocations = functions.iter().enumerate().map(|(index, (name, _))| Relocation {
                offset: index * 8, kind: 1, target: Address::Symbol(name.clone()),
            }).collect();
            let table = data(&mut sections, vec![0; functions.len() * 8], relocations);
            Address::Section(table, 0)
        };
        Some(image(&mut sections, crate::resource::compiler_output(&root), Some((code, functions.len()))))
    };
    let gpu = if p.target() == Target::Cpu {
        None
    } else {
        let root = crate::spirv::program::compile_in(worker, p).map_err(std::io::Error::other)?;
        Some(image(&mut sections, root.compiler_output_bytes(), None))
    };

    let mut code = [0u8; 128];
    let has_factories = resources.is_some();
    let row = unsafe { library_create_code(u32::from(cpu.is_some()), u32::from(gpu.is_some()), u32::from(has_factories), code.as_mut_ptr()) };
    let mut relocations = vec![external(row.entry.call, "blink_library_create")];
    for (section, field) in [(cpu, row.cpu), (gpu, row.gpu)] {
        if let Some(section) = section {
            relocations.push(Relocation { offset: field as usize, kind: 4, target: Address::Section(section, 0) });
        }
    }
    if has_factories {
        relocations.push(external(row.factories, "__blink_module_factories"));
        relocations.push(external(row.count, "__blink_module_count"));
    }
    wrapper(&mut sections, &mut symbols, format!("{prefix}_create"),
        code[..row.entry.length as usize].to_vec(), relocations,
        Some((row.entry.function_end, call_unwind(row.entry.prolog, row.entry.frame))));

    let datasets = dataset_addresses.len().try_into()?;
    let addresses = dataset_addresses.iter().filter(|&&address| address != 0).count().try_into()?;
    let layout = unsafe { library_run_layout(datasets, addresses, uniforms) };
    if layout.capacity == 0 {
        return Err("算法 DLL 宿主入口超出 Windows x64 栈位移或代码范围".into());
    }
    let mut run = Vec::with_capacity(layout.capacity as usize);
    let row = unsafe {
        let row = library_run_code(layout, dataset_addresses.as_ptr(), datasets, uniforms, run.as_mut_ptr());
        run.set_len(row.length as usize);
        row
    };
    wrapper(&mut sections, &mut symbols, format!("{prefix}_run"), run,
        vec![external(row.call, "blink_library_run")],
        Some((row.function_end, call_unwind(row.prolog, row.frame))));

    let row = unsafe { library_destroy_code(code.as_mut_ptr()) };
    wrapper(&mut sections, &mut symbols, format!("{prefix}_destroy"),
        code[..row.length as usize].to_vec(), vec![external(row.field, "blink_library_destroy")], None);

    Ok((crate::object::encode_entry_archive(prefix, &sections, &symbols)?, functions, resources))
}
