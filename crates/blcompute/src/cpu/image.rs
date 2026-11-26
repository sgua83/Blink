//! Borrow the existing compiler image for the native loader. Its returned handle
//! holds the executable bytes, unwind registrations and imported DLL references.
use super::{CpuCode, CpuCodeMemory, CpuTemplate, print_bool, print_float, print_i32, print_i64,
    print_u32, print_u64};
use crate::{Result, application::Definitions, error};
use blanguage::{
    ast::ExternalName,
    ir::Scalar,
    native::{ModuleCode, ModuleTarget, NativeCode, NativeSymbol, ProgramImage},
};
use std::{collections::BTreeMap, ffi::{CString, c_void}, io, ptr::{self, NonNull}, slice, str, sync::Arc};

#[derive(Clone, Copy)]
#[repr(C)]
struct Bytes {
    data: *const u8,
    length: usize,
}

impl Bytes {
    const EMPTY: Self = Self { data: ptr::null(), length: 0 };

    fn new(bytes: &[u8]) -> Self {
        Self { data: bytes.as_ptr(), length: bytes.len() }
    }

    unsafe fn text<'a>(self) -> &'a str {
        if self.length == 0 {
            return "";
        }
        // Symbol text comes from the image's original strings or fixed Blink names.
        unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.data, self.length)) }
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Name {
    library: Bytes,
    symbol: Bytes,
}

impl Name {
    const NONE: Self = Self { library: Bytes::EMPTY, symbol: Bytes::EMPTY };

    fn external(name: &ExternalName) -> Self {
        Self { library: Bytes::new(name.library.as_bytes()), symbol: Bytes::new(name.symbol.as_bytes()) }
    }
}

#[repr(u32)]
enum Target { Missing, Root, Section, External, Print }

#[repr(C)]
struct Code {
    bytes: Bytes,
    unwind: Bytes,
    function_end: usize,
    patch_count: usize,
}

#[repr(C)]
struct CodePatch {
    offset: usize,
    target: Target,
    service: *const (),
    name: Name,
}

#[repr(C)]
struct Section {
    bytes: Bytes,
    patch_count: usize,
    flags: u32,
    unwind: u32,
}

#[repr(C)]
struct SectionPatch {
    offset: usize,
    section_index: usize,
    section_value: usize,
    kind: u32,
    external: u32,
    name: Name,
}

#[repr(C)]
struct SymbolPosition {
    index: usize,
    value: usize,
    target: Target,
}

#[repr(C)]
struct Input {
    context: *const c_void,
    code_count: usize,
    section_count: usize,
    has_module: u32,
    runtime_library: Bytes,
    code: unsafe extern "C" fn(*const c_void, usize) -> Code,
    code_patch: unsafe extern "C" fn(*const c_void, usize, usize) -> CodePatch,
    section: unsafe extern "C" fn(*const c_void, usize) -> Section,
    section_patch: unsafe extern "C" fn(*const c_void, usize, usize) -> SectionPatch,
    symbol: unsafe extern "C" fn(*const c_void, Bytes) -> SymbolPosition,
    resource: unsafe extern "C" fn(*const c_void, Bytes) -> *const (),
}

#[repr(C)]
struct ImageError {
    kind: u32,
    detail: u32,
    name: Name,
}

impl ImageError {
    fn result(&self) -> Result<()> {
        let windows = || io::Error::from_raw_os_error(self.detail as i32);
        let library = || unsafe { self.name.library.text() };
        let symbol = || unsafe { self.name.symbol.text() };
        let error = match self.kind {
            0 => return Ok(()),
            1 | 2 => return Err(windows().into()),
            3 | 12 => error("out of range integral type conversion attempted"),
            4 => error("Windows未接受CPU代码的原生展开信息"),
            5 => error(format!("范围映像缺少所属模块函数 {}", symbol())),
            6 => error(format!("加载 {} 失败：{}", library(), windows())),
            7 => error(format!("解析 {}!{} 失败：{}", library(), symbol(), windows())),
            8 => return Err(CString::new(symbol()).unwrap_err().into()),
            9 => error(format!("未知的Blink资源C符号 {}", symbol())),
            10 => error(format!("不支持 AMD64 重定位类型 {}", self.detail)),
            11 => error("COFF 重定位位置越界"),
            _ => unreachable!("Blink native image status"),
        };
        Err(error)
    }
}

#[repr(C)]
struct Loaded {
    owner: *mut c_void,
    base: *mut u8,
    table: *const RuntimeFunction,
    factory_count: *const usize,
    factories: *const blanguage::resource::ModuleFactory,
    error: ImageError,
}

#[repr(C)]
pub(super) struct RuntimeFunction {
    pub(super) begin: u32,
    end: u32,
    unwind: u32,
}

#[repr(C)]
struct MemoryInfo {
    base_address: usize,
    allocation_base: usize,
    allocation_protect: u32,
    partition_id: u16,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}

#[repr(C)]
struct MemoryResult {
    info: MemoryInfo,
    error: u32,
}

unsafe extern "C" {
    #[link_name = "blink.windows.cpu_image::load_image"]
    fn load_image(input: Input) -> Loaded;
    #[link_name = "blink.windows.cpu_image::release_image"]
    fn release_image(owner: *mut c_void);
    #[link_name = "blink.windows.cpu_image::query_image_memory"]
    fn query_image_memory(entry: *const c_void) -> MemoryResult;
}

struct Image<'a> {
    code: Vec<&'a NativeCode>,
    module: Option<&'a ModuleCode>,
    functions: BTreeMap<&'a str, usize>,
}

unsafe extern "C" fn code(context: *const c_void, index: usize) -> Code {
    let image = unsafe { &*context.cast::<Image<'_>>() };
    let code = image.code[index];
    Code {
        bytes: Bytes::new(&code.bytes),
        unwind: Bytes::new(&code.unwind),
        function_end: code.function_end,
        patch_count: code.relocations.len(),
    }
}

unsafe extern "C" fn code_patch(context: *const c_void, index: usize, patch: usize) -> CodePatch {
    let image = unsafe { &*context.cast::<Image<'_>>() };
    let relocation = &image.code[index].relocations[patch];
    let (target, name, service) = match &relocation.target {
        NativeSymbol::Function(name) => (
            Target::Root, Name { library: Bytes::EMPTY, symbol: Bytes::new(name.as_bytes()) }, ptr::null(),
        ),
        NativeSymbol::External(name) => (Target::External, Name::external(name), ptr::null()),
        NativeSymbol::Print(scalar) => {
            let address = match scalar {
                Scalar::U8 | Scalar::U16 | Scalar::U32 => print_u32 as *const (),
                Scalar::I8 | Scalar::I16 | Scalar::I32 => print_i32 as *const (),
                Scalar::U64 => print_u64 as *const (),
                Scalar::I64 => print_i64 as *const (),
                Scalar::Bool => print_bool as *const (),
                Scalar::F32 | Scalar::F64 => print_float as *const (),
            };
            (Target::Print, Name::NONE, address)
        }
    };
    CodePatch { offset: relocation.offset, target, service, name }
}

unsafe extern "C" fn section(context: *const c_void, index: usize) -> Section {
    let image = unsafe { &*context.cast::<Image<'_>>() };
    let section = &image.module.unwrap().sections[index];
    Section {
        bytes: Bytes::new(&section.bytes),
        patch_count: section.relocations.len(),
        flags: section.flags,
        unwind: u32::from(section.name == ".pdata"),
    }
}

unsafe extern "C" fn section_patch(context: *const c_void, index: usize, patch: usize) -> SectionPatch {
    let image = unsafe { &*context.cast::<Image<'_>>() };
    let relocation = &image.module.unwrap().sections[index].relocations[patch];
    let (section_index, section_value, external, name) = match &relocation.target {
        ModuleTarget::Section { section, value } => (*section, *value as usize, 0, Name::NONE),
        ModuleTarget::Import(name) => (0, 0, 1, Name::external(name)),
    };
    SectionPatch { offset: relocation.offset, section_index, section_value, kind: relocation.kind.into(), external, name }
}

unsafe extern "C" fn symbol(context: *const c_void, name: Bytes) -> SymbolPosition {
    let image = unsafe { &*context.cast::<Image<'_>>() };
    let name = unsafe { name.text() };
    if let Some(&(index, value)) = image.module.and_then(|module| module.symbols.get(name)) {
        SymbolPosition { index, value: value as usize, target: Target::Section }
    } else if let Some(&index) = image.functions.get(name) {
        SymbolPosition { index, value: 0, target: Target::Root }
    } else {
        SymbolPosition { index: 0, value: 0, target: Target::Missing }
    }
}

unsafe extern "C" fn resource(_: *const c_void, name: Bytes) -> *const () {
    crate::application::resource_symbol(unsafe { name.text() }).unwrap_or(ptr::null())
}

pub(super) struct Allocation(NonNull<c_void>);

impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe { release_image(self.0.as_ptr()); }
    }
}

pub(super) struct LoadedCode {
    pub(super) allocation: Arc<Allocation>,
    pub(super) base: *mut u8,
    table: NonNull<[RuntimeFunction]>,
}

impl LoadedCode {
    pub(super) fn entries(&self) -> &[RuntimeFunction] {
        // The initialized root table remains live in the original B allocation.
        unsafe { self.table.as_ref() }
    }
}

pub(super) fn load<P>(image: &ProgramImage<P>, workers: usize) -> Result<(Option<LoadedCode>, Option<Definitions>)> {
    let first_function = image.stage_code_count();
    let context = Image {
        code: image.code().collect(),
        module: image.module_code(),
        functions: image.functions().iter().enumerate()
            .map(|(index, (name, _))| (name.as_str(), first_function + index)).collect(),
    };
    let input = Input {
        context: ptr::from_ref(&context).cast(),
        code_count: context.code.len(),
        section_count: context.module.map_or(0, |module| module.sections.len()),
        has_module: u32::from(context.module.is_some()),
        runtime_library: Bytes::new(blanguage::resource::RUNTIME_LIBRARY.as_bytes()),
        code, code_patch, section, section_patch, symbol, resource,
    };
    let loaded = unsafe { load_image(input) };
    loaded.error.result()?;
    let Some(owner) = NonNull::new(loaded.owner) else { return Ok((None, None)); };
    let allocation = Arc::new(Allocation(owner));
    let table = NonNull::slice_from_raw_parts(NonNull::new(loaded.table.cast_mut()).unwrap(), context.code.len());
    let factories = if loaded.factory_count.is_null() {
        &[]
    } else {
        let entries = NonNull::new(loaded.factories.cast_mut()).unwrap();
        unsafe { slice::from_raw_parts(entries.as_ptr(), *loaded.factory_count) }
    };
    let definitions = if factories.is_empty() {
        None
    } else {
        Some(unsafe { Definitions::linked(factories, |image, entries| {
            Ok(CpuTemplate { code: CpuCode::bind_indices(image, workers, entries, Some(&allocation)) })
        }) }?)
    };
    Ok((Some(LoadedCode { allocation, base: loaded.base, table }), definitions))
}

pub(super) fn query(stage: usize, entry: *const c_void, runtime_owned: bool) -> Result<CpuCodeMemory> {
    let result = unsafe { query_image_memory(entry) };
    if result.error != 0 {
        return Err(io::Error::from_raw_os_error(result.error as i32).into());
    }
    let info = result.info;
    Ok(CpuCodeMemory {
        stage,
        entry_address: entry as usize,
        runtime_owned,
        allocation_address: info.allocation_base,
        region_address: info.base_address,
        region_bytes: info.region_size,
        allocation_protection: info.allocation_protect,
        protection: info.protect,
        state: info.state,
        kind: info.kind,
    })
}
