//! 应用编译产物：入口、标准输出、导入跳板和展开信息都在编译阶段生成。
use super::{Import, Relocation, Result, Section, SectionData, Target, stack::{Frame, Call}};
use crate::native::{NativeCode, NativeSymbol, unwind_info, UnwindOperation};
use crate::builtin_types::ScalarEncoding;
use super::coff::facts::observations::Layout;
use std::{collections::{btree_map, BTreeMap}, ffi::c_void, ptr};
#[repr(C)]
struct SymbolEncoding {
    length: u64,
    field: u64,
}

#[repr(C)]
struct ApplicationEntry {
    length: u32,
    frame: u32,
    prolog: u32,
    main_field: u32,
    exit_field: u32,
}

#[repr(C)]
struct PrintEncoding {
    length: u32,
    unwind_length: u32,
    local: u32,
    create_locale: u32,
    format: u32,
    free_locale: u32,
    stdout_handle: u32,
    write: u32,
}

#[repr(C)]
struct RuntimeFunctionFields {
    begin: u32,
    end: u32,
    unwind: u32,
    relocation: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.unwind::runtime_function"]
    fn runtime_function(end: u32, output: *mut u8) -> RuntimeFunctionFields;
    #[link_name = "bootstrap.x64_instruction::print_helper"]
    fn print_helper(scalar: ScalarEncoding, output: *mut u8, unwind: *mut u8) -> PrintEncoding;
    #[link_name = "bootstrap.x64_instruction::import_transfer"]
    fn import_transfer(tail: u32, output: *mut u8) -> SymbolEncoding;
    #[link_name = "bootstrap.x64_instruction::application_entry"]
    fn application_entry(output: *mut u8) -> ApplicationEntry;
}

fn put64(v: &mut [u8], at: usize, n: u64) {
    v[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
fn kernel(name: &'static str) -> Import {
    Import {
        dll: "KERNEL32.dll".into(),
        name: name.into(),
    }
}
fn ucrt(name: &'static str) -> Import {
    Import {
        dll: "ucrtbase.dll".into(),
        name: name.into(),
    }
}
fn function(sections: &mut Vec<Section>, frames: &mut Vec<Frame>, name: String, local: u32, calls: Vec<Call>, bytes: Vec<u8>, unwind: Option<(u32, Vec<u8>)>, layout: Option<Layout>) -> usize {
    let text = function_sections(sections, bytes, unwind, layout);
    frames.push(Frame { name, coff: Some((text, 0)), local, calls });
    text
}

pub(crate) fn function_sections(
    sections: &mut Vec<Section>,
    bytes: Vec<u8>,
    unwind: Option<(u32, Vec<u8>)>,
    layout: Option<Layout>,
) -> usize {
    let text = sections.len();
    sections.push(Section {
        name: ".text".into(), bytes, flags: 0x60501020,
        relocations: Vec::new(), associated: Vec::new(), layout,
    });
    if let Some((end, unwind)) = unwind {
        let pdata = text + 1;
        let xdata = text + 2;
        sections[text].associated = vec![pdata, xdata];
        let mut bytes = Vec::with_capacity(12);
        let fields = unsafe {
            let fields = runtime_function(end, bytes.as_mut_ptr());
            bytes.set_len(12);
            fields
        };
        sections.push(Section {
            name: ".pdata".into(), bytes, flags: 0x40301040,
            relocations: [(fields.begin, text), (fields.end, text), (fields.unwind, xdata)].into_iter()
                .map(|(offset, section)| Relocation {
                    offset: offset as usize,
                    kind: fields.relocation as u16,
                    target: Target::Section(section, 0),
                }).collect(),
            associated: Vec::new(), layout: None,
        });
        sections.push(Section {
            name: ".xdata".into(), bytes: unwind, flags: 0x40301040,
            relocations: Vec::new(), associated: Vec::new(), layout: None,
        });
    }
    text
}
struct Helper {
    local: u32,
    end: u32,
    bytes: Vec<u8>,
    patches: Vec<(usize, Import)>,
    unwind: Vec<u8>,
}
fn print(scalar: crate::ir::Scalar) -> Helper {
    let mut bytes = Vec::with_capacity(224);
    let mut unwind = Vec::with_capacity(12);
    let encoded = unsafe {
        let encoded = print_helper(scalar.into(), bytes.as_mut_ptr(), unwind.as_mut_ptr());
        bytes.set_len(encoded.length as usize);
        unwind.set_len(encoded.unwind_length as usize);
        encoded
    };
    let mut patches = Vec::with_capacity(if encoded.create_locale == 0 { 2 } else { 5 });
    if encoded.create_locale != 0 {
        patches.extend([
            (encoded.create_locale as usize, ucrt("_create_locale")),
            (encoded.format as usize, ucrt("__stdio_common_vsprintf")),
            (encoded.free_locale as usize, ucrt("_free_locale")),
        ]);
    }
    patches.extend([
        (encoded.stdout_handle as usize, kernel("GetStdHandle")),
        (encoded.write as usize, kernel("WriteFile")),
    ]);
    Helper { local: encoded.local, end: encoded.length, bytes, patches, unwind }
}
pub(crate) struct EmittedFunctions {
    pub(crate) sections: Vec<Section<SectionData<Vec<u8>>>>,
    pub(crate) positions: BTreeMap<String, (usize, u32)>,
    pub(crate) stack: Vec<Frame>,
    pub(crate) factory_entries: Vec<Relocation>,
}

pub(crate) fn application(
    native: NativeCode,
    defined: Vec<(String, NativeCode)>,
    factory_entry_count: usize,
    imports: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
    name: &str,
) -> Result<EmittedFunctions> {
    functions(Some((name.into(), native)), defined, factory_entry_count, imports, prefix)
}
pub(crate) fn module(
    defined: Vec<(String, NativeCode)>,
    factory_entry_count: usize,
    imports: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
) -> Result<EmittedFunctions> {
    functions(None, defined, factory_entry_count, imports, prefix)
}
fn native_calls(calls: Vec<Option<usize>>, relocations: &[crate::native::Relocation]) -> Vec<Call> {
    // The emitter appends both sequences in machine-field order.
    let mut relocations = relocations.iter().enumerate();
    calls.into_iter().map(|call| match call {
        Some(offset) => {
            let (relocation, _) = relocations.find(|(_, entry)| entry.offset == offset).unwrap();
            Call::Reference { relocation, tail: false }
        }
        None => Call::Indirect,
    }).collect()
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeObjectPlace { section: usize, present: u32 }

impl NativeObjectPlace {
    const UNRESOLVED: Self = Self { section: 0, present: 0 };

    fn section(section: usize) -> Self {
        Self { section, present: 1 }
    }
}

#[repr(u32)]
enum NativeObjectTargetKind { Done, External, Print }

#[repr(C)]
struct NativeObjectTarget {
    symbol: *const c_void,
    payload: *const c_void,
    output: *mut usize,
    kind: NativeObjectTargetKind,
}

#[repr(C)]
struct NativeObjectTargetHost {
    next: unsafe extern "C" fn(*mut c_void) -> NativeObjectTarget,
    external: unsafe extern "C" fn(*mut c_void, *const c_void, *const c_void) -> usize,
    print: unsafe extern "C" fn(*mut c_void, *const c_void, *const c_void) -> usize,
}

type HelperSections<'a> = BTreeMap<&'a NativeSymbol, usize>;

struct TargetSelection<'a, 'source> {
    targets: btree_map::IterMut<'a, &'source NativeSymbol, usize>,
    sections: &'a mut Vec<Section>,
    frames: &'a mut Vec<Frame>,
    prefix: &'a str,
}

unsafe extern "C" fn next_native_target(context: *mut c_void) -> NativeObjectTarget {
    let selection = unsafe { &mut *context.cast::<TargetSelection<'_, '_>>() };
    let Some((&symbol, output)) = selection.targets.next() else {
        return NativeObjectTarget {
            symbol: ptr::null(), payload: ptr::null(), output: ptr::null_mut(),
            kind: NativeObjectTargetKind::Done,
        };
    };
    let (kind, payload) = match symbol {
        NativeSymbol::External(name) => (NativeObjectTargetKind::External, ptr::from_ref(name).cast()),
        NativeSymbol::Print(scalar) => (NativeObjectTargetKind::Print, ptr::from_ref(scalar).cast()),
        NativeSymbol::Function(_) => unreachable!(),
    };
    NativeObjectTarget { symbol: ptr::from_ref(symbol).cast(), payload, output, kind }
}

unsafe extern "C" fn write_native_thunk(
    context: *mut c_void, symbol: *const c_void, name: *const c_void,
) -> usize {
    let selection = unsafe { &mut *context.cast::<TargetSelection<'_, '_>>() };
    let name = unsafe { &*name.cast::<crate::ast::ExternalName>() };
    let symbol = unsafe { &*symbol.cast::<NativeSymbol>() };
    let name = Import {
        dll: name.library.clone(),
        name: if name.library == crate::application::RUNTIME_LIBRARY
            && ["blink_allocate_", "blink_run_", "blink_complete_"].iter().any(|prefix|
                name.symbol.strip_prefix(prefix).is_some_and(|index| index.parse::<usize>().is_ok())) {
            format!("{}_{}", selection.prefix, name.symbol.strip_prefix("blink_").unwrap())
        } else {
            name.symbol.clone()
        },
    };
    let mut bytes = vec![0; 6];
    let encoded = unsafe { import_transfer(1, bytes.as_mut_ptr()) };
    bytes.truncate(encoded.length as usize);
    let calls = vec![Call::Reference { relocation: 0, tail: true }];
    let text = function(selection.sections, selection.frames, format!("{symbol:?}"), 0, calls,
        bytes, None, Some(Layout::ImportThunk));
    selection.sections[text].relocations.push(Relocation {
        offset: encoded.field as usize, kind: 4, target: Target::Import(name),
    });
    text
}

unsafe extern "C" fn write_native_print(
    context: *mut c_void, symbol: *const c_void, scalar: *const c_void,
) -> usize {
    let selection = unsafe { &mut *context.cast::<TargetSelection<'_, '_>>() };
    let scalar = unsafe { *scalar.cast::<crate::ir::Scalar>() };
    let symbol = unsafe { &*symbol.cast::<NativeSymbol>() };
    let helper = print(scalar);
    let calls = (0..helper.patches.len())
        .map(|relocation| Call::Reference { relocation, tail: false }).collect();
    let text = function(selection.sections, selection.frames, format!("{symbol:?}"), helper.local,
        calls, helper.bytes, Some((helper.end, helper.unwind)), Some(Layout::Print(scalar)));
    selection.sections[text].relocations.extend(helper.patches.into_iter()
        .map(|(offset, import)| Relocation { offset, kind: 4, target: Target::Import(import) }));
    text
}

const TARGET_HOST: NativeObjectTargetHost = NativeObjectTargetHost {
    next: next_native_target,
    external: write_native_thunk,
    print: write_native_print,
};

#[repr(C)]
struct NativeObjectImage { section: usize, fields: usize }

#[repr(C)]
struct NativeObjectFixup {
    offset: usize,
    name: *const c_void,
    place: NativeObjectPlace,
    imported: u32,
}

#[repr(C)]
struct NativeObjectFixupHost {
    image: unsafe extern "C" fn(*const c_void, usize) -> NativeObjectImage,
    field: unsafe extern "C" fn(*const c_void, usize, usize) -> NativeObjectFixup,
    section: unsafe extern "C" fn(*mut c_void, usize, usize, usize),
    symbol: unsafe extern "C" fn(*mut c_void, usize, usize, *const c_void),
    missing: unsafe extern "C" fn(*mut c_void, *const c_void),
}

struct NativeObjectFixups<'a, 'source> {
    images: &'source [(usize, Vec<crate::native::Relocation>)],
    helpers: &'a HelperSections<'source>,
    definitions: &'a BTreeMap<String, (usize, u32)>,
    imports: &'a BTreeMap<String, Vec<u8>>,
    sections: &'a mut [Section],
    missing: Option<&'source String>,
}

unsafe extern "C" fn native_image(context: *const c_void, index: usize) -> NativeObjectImage {
    let fixups = unsafe { &*context.cast::<NativeObjectFixups<'_, '_>>() };
    let (section, fields) = &fixups.images[index];
    NativeObjectImage { section: *section, fields: fields.len() }
}

unsafe extern "C" fn native_field(context: *const c_void, image: usize, index: usize) -> NativeObjectFixup {
    let fixups = unsafe { &*context.cast::<NativeObjectFixups<'_, '_>>() };
    let field = &fixups.images[image].1[index];
    let (name, place, imported) = match &field.target {
        NativeSymbol::Function(name) => {
            let place = fixups.definitions.get(name).map_or(NativeObjectPlace::UNRESOLVED,
                |&(section, _)| NativeObjectPlace::section(section));
            let imported = place.present == 0 && fixups.imports.contains_key(name);
            (ptr::from_ref(name).cast(), place, u32::from(imported))
        }
        target => (ptr::null(), NativeObjectPlace::section(fixups.helpers[target]), 0),
    };
    NativeObjectFixup { offset: field.offset, name, place, imported }
}

unsafe extern "C" fn fixup_section(context: *mut c_void, section: usize, offset: usize, target: usize) {
    let fixups = unsafe { &mut *context.cast::<NativeObjectFixups<'_, '_>>() };
    let output = &mut fixups.sections[section];
    put64(&mut output.bytes, offset, 0);
    output.relocations.push(Relocation { offset, kind: 1, target: Target::Section(target, 0) });
}

unsafe extern "C" fn fixup_symbol(context: *mut c_void, section: usize, offset: usize, name: *const c_void) {
    let fixups = unsafe { &mut *context.cast::<NativeObjectFixups<'_, '_>>() };
    let target = Target::Symbol(unsafe { &*name.cast::<String>() }.clone());
    fixups.sections[section].relocations.push(Relocation { offset, kind: 1, target });
}

unsafe extern "C" fn missing_native_target(context: *mut c_void, name: *const c_void) {
    let fixups = unsafe { &mut *context.cast::<NativeObjectFixups<'_, '_>>() };
    let name = unsafe { &*name.cast::<String>() };
    // Retain the same first diagnostic as the original name-ordered table.
    if fixups.missing.is_none_or(|first| name < first) {
        fixups.missing = Some(name);
    }
}

const FIXUP_HOST: NativeObjectFixupHost = NativeObjectFixupHost {
    image: native_image,
    field: native_field,
    section: fixup_section,
    symbol: fixup_symbol,
    missing: missing_native_target,
};

unsafe extern "C" {
    #[link_name = "bootstrap.coff::resolve_native_object_targets"]
    fn resolve_native_object_targets(context: *mut c_void, host: NativeObjectTargetHost);
    #[link_name = "bootstrap.coff::bind_native_object_fixups"]
    fn bind_native_object_fixups(context: *mut c_void, images: usize, host: NativeObjectFixupHost);
}

fn bind_object_targets(
    sections: &mut Vec<Section>,
    frames: &mut Vec<Frame>,
    images: &[(usize, Vec<crate::native::Relocation>)],
    definitions: &BTreeMap<String, (usize, u32)>,
    imports: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
) -> Result<()> {
    let mut helpers = HelperSections::new();
    for field in images.iter().flat_map(|(_, fields)| fields) {
        if !matches!(field.target, NativeSymbol::Function(_)) {
            helpers.entry(&field.target).or_insert(0);
        }
    }
    let mut selection = TargetSelection { targets: helpers.iter_mut(), sections, frames, prefix };
    unsafe { resolve_native_object_targets(ptr::from_mut(&mut selection).cast(), TARGET_HOST) }

    let mut fixups = NativeObjectFixups { images, helpers: &helpers, definitions, imports, sections, missing: None };
    unsafe { bind_native_object_fixups(ptr::from_mut(&mut fixups).cast(), images.len(), FIXUP_HOST) };
    if let Some(name) = fixups.missing {
        return Err(format!("缺少模块函数接口 {name}").into());
    }
    Ok(())
}

fn functions(
    native: Option<(String, NativeCode)>,
    defined: Vec<(String, NativeCode)>,
    factory_entry_count: usize,
    imports: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
) -> Result<EmittedFunctions> {
    let mut sections = Vec::new();
    let mut frames = Vec::new();
    let mut images = Vec::new();
    let mut symbols = BTreeMap::new();
    let main = if let Some((name, native)) = native {
        let mut bytes = vec![0; 17];
        let layout = unsafe { application_entry(bytes.as_mut_ptr()) };
        bytes.truncate(layout.length as usize);
        let calls = vec![
            Call::Reference { relocation: 0, tail: false },
            Call::Reference { relocation: 1, tail: false },
        ];
        let unwind = unwind_info(layout.prolog as u8, &[UnwindOperation::allocate(layout.prolog as u8, layout.frame)]);
        let entry = function(&mut sections, &mut frames, "mainCRTStartup".into(), layout.frame, calls, bytes,
            Some((layout.length, unwind)), Some(Layout::Startup));
        let calls = native_calls(native.calls, &native.relocations);
        let main = function(&mut sections, &mut frames, format!("entry {name}"), native.stack_bytes, calls, native.bytes, Some((native.function_end.try_into()?, native.unwind)), native.tail_regions.map(|tails| Layout::Function { tails, references: native.tail_references, sources: native.sources }));
        sections[entry].relocations.extend([
            Relocation { offset: layout.main_field as usize, kind: 4, target: Target::Section(main, 0) },
            Relocation { offset: layout.exit_field as usize, kind: 4, target: Target::Import(kernel("ExitProcess")) },
        ]);
        images.push((main, native.relocations));
        Some(main)
    } else {
        None
    };
    // Callers append factory entries in the same order used by the encoded indices.
    let first_factory_entry = defined.len() - factory_entry_count;
    let mut factory_entries = Vec::with_capacity(factory_entry_count);
    for (index, (name, code)) in defined.into_iter().enumerate() {
        let calls = native_calls(code.calls, &code.relocations);
        let text = function(&mut sections, &mut frames, name.clone(), code.stack_bytes, calls, code.bytes, Some((code.function_end.try_into()?, code.unwind)), code.tail_regions.map(|tails| Layout::Function { tails, references: code.tail_references, sources: code.sources }));
        if let Some(entry) = index.checked_sub(first_factory_entry) {
            factory_entries.push(Relocation {
                offset: entry * 8, kind: 1, target: Target::Section(text, 0),
            });
        }
        symbols.insert(name, (text, 0));
        images.push((text, code.relocations));
    }
    bind_object_targets(&mut sections, &mut frames, &images, &symbols, imports, prefix)?;
    if let Some(main) = main {
        symbols.entry("main".into()).or_insert((main, 0));
    }
    let sections = sections.into_iter().map(Section::initialized).collect();
    Ok(EmittedFunctions { sections, positions: symbols, stack: frames, factory_entries })
}
