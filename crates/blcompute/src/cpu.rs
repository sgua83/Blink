pub use crate::texture::{TextureInput, CpuTextureBinding, CpuTextures};
mod dataset;
mod observation;
mod sort;
use crate::{Dataset, DomainBindings, ReadDataset, Result, Value, arguments, error};
use crate::bindings::{self, DomainRelations, StorageSlots};
use crate::application::{FrameArguments, ResidentData};
use crate::library::Library;
use blanguage::{
    ExecutableProgram, TemplateProgram,
    ir::{Bits, CollectiveOperator, ExecutionPlan as Program, Scalar, Ty},
    native::{BufferAddress, NativeSymbol, RangeEntry, RangeResult},
};
pub use dataset::CpuDataset;
pub use observation::{CpuRangeCall, CpuRangeTiming, CpuRangeTimings, CpuThreadTime, CpuTimePoint};
use std::{collections::{BTreeMap, btree_map::Entry}, ffi::c_void, mem::MaybeUninit, ptr::NonNull, sync::Arc};
use std::sync::atomic::{AtomicUsize, Ordering};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn VirtualAlloc(address: *mut c_void, size: usize, kind: u32, protect: u32) -> *mut c_void;
    fn VirtualProtect(address: *mut c_void, size: usize, protect: u32, old: *mut u32) -> i32;
    fn VirtualFree(address: *mut c_void, size: usize, kind: u32) -> i32;
    fn VirtualQuery(address: *const c_void, information: *mut MemoryBasicInformation, size: usize) -> usize;
    fn FlushInstructionCache(process: *mut c_void, address: *const c_void, size: usize) -> i32;
    fn GetCurrentProcess() -> *mut c_void;
    fn RtlAddFunctionTable(table: *mut RuntimeFunction, count: u32, base: u64) -> u8;
    fn RtlDeleteFunctionTable(table: *mut RuntimeFunction) -> u8;
    fn CreateThreadpoolWork(
        callback: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void),
        context: *mut c_void,
        environment: *const c_void,
    ) -> *mut c_void;
    fn SubmitThreadpoolWork(work: *mut c_void);
    fn WaitForThreadpoolWorkCallbacks(work: *mut c_void, cancel: i32);
    fn CloseThreadpoolWork(work: *mut c_void);
}
struct RangeWork<'a> {
    frame: Option<(&'a Arc<crate::application::Definitions>, usize, usize, &'a [CpuBuffer<'a>], Option<&'a Arc<Vec<crate::DomainRelation>>>)>,
    entry: *mut c_void,
    buffers: *const BufferAddress,
    // 范围摘要替换数据集后的私有槽；普通 map 的完整表在其后还可包含纹理。
    range_parameter_slot: usize,
    uniforms: *const u32,
    count: usize,
    grain: usize,
    next: AtomicUsize,
    result: RangeResult,
    partials: *mut u32,
    errors: *mut *mut c_void,
    range_parameters: *const u32,
    range_parameter_words: usize,
    stage: usize,
    timing: Option<*mut CpuRangeTiming>,
}
impl RangeWork<'_> {
    unsafe fn invoke(
        &self,
        address: *mut c_void,
        buffers: *const BufferAddress,
        uniforms: *mut u32,
        end: u32,
        begin: u32,
        output: *mut u32,
        timing: Option<*mut CpuRangeTiming>,
    ) -> *mut c_void {
        let mut frame;
        if let Some((definitions, offset, words, inputs, input_domains)) = self.frame {
            frame = unsafe {
                crate::application::Frame::from_definitions(definitions.clone(), inputs, input_domains)
            };
            let pointer = std::ptr::from_mut(&mut frame) as usize as u64;
            // A framed call owns its complete uniform block for this invocation.
            let frame_uniforms = unsafe { std::slice::from_raw_parts_mut(uniforms, words) };
            frame_uniforms[offset] = pointer as u32;
            frame_uniforms[offset + 1] = (pointer >> 32) as u32;
        }
        let entry:RangeEntry=unsafe {std::mem::transmute(address)};
        unsafe {
            observation::measure(timing, begin, end, || entry(buffers, uniforms, end, begin, output))
        }
    }

    unsafe fn invoke_serial(
        &self,
        address: *mut c_void,
        buffers: &mut [BufferAddress],
        replacements: &mut [(usize, BufferAddress)],
        uniforms: &mut [u32],
        end: u32,
        output: *mut u32,
        timings: &mut Option<&mut CpuRangeTimings>,
    ) -> Result<*mut c_void> {
        // The parallel pass has joined. This entry alone borrows the original
        // bindings and uniforms until both are restored.
        for (slot, replacement) in replacements.iter_mut() {
            std::mem::swap(&mut buffers[*slot], replacement);
        }
        let result = unsafe {
            observation::capture(timings, self.stage, address as usize, 1, |timing| {
                let frame_words = self.frame.map(|(_, offset, _, _, _)| {
                    (offset, [uniforms[offset], uniforms[offset + 1]])
                });
                let failure = self.invoke(address, buffers.as_ptr(), uniforms.as_mut_ptr(), end, 0, output, timing);
                if let Some((offset, words)) = frame_words {
                    uniforms[offset..offset + words.len()].copy_from_slice(&words);
                }
                Ok(failure)
            })
        };
        // The source and target can name the same slot.
        for (slot, original) in replacements.iter_mut().rev() {
            std::mem::swap(&mut buffers[*slot], original);
        }
        result
    }
}
unsafe extern "system" fn execute_ranges(
    _instance: *mut c_void,
    context: *mut c_void,
    _work: *mut c_void,
) {
    let work = unsafe { &*context.cast::<RangeWork>() };
    let mut begin = work.next.fetch_add(work.grain, Ordering::Relaxed);
    if begin >= work.count {
        return;
    }
    let mut frame_uniforms = work.frame.map(|(_, _, words, _, _)| unsafe {
        std::slice::from_raw_parts(work.uniforms, words).to_vec()
    });
    let uniforms = frame_uniforms.as_mut().map_or(work.uniforms.cast_mut(), Vec::as_mut_ptr);
    let mut private_buffers = if work.range_parameters.is_null() {
        Vec::new()
    } else {
        let mut pointers = Vec::with_capacity(work.range_parameter_slot + 1);
        pointers.extend_from_slice(unsafe {
            std::slice::from_raw_parts(work.buffers, work.range_parameter_slot)
        });
        pointers.push(BufferAddress { address: std::ptr::null_mut(), stride: work.range_parameter_words * 4 });
        pointers
    };
    while begin < work.count {
        let end = (begin + work.grain).min(work.count);
        let index = begin / work.grain;
        unsafe {
            let buffers = if work.range_parameters.is_null() {
                work.buffers
            } else {
                private_buffers[work.range_parameter_slot].address = work
                    .range_parameters
                    .add(index * work.range_parameter_words)
                    .cast_mut();
                private_buffers.as_ptr()
            };
            let words = work.result.words();
            let output = if words == 0 {
                std::ptr::null_mut()
            } else {
                work.partials.add(index * words)
            };
            let timing = work.timing.map(|first| first.add(index));
            let failure = work.invoke(
                work.entry, buffers, uniforms, end as u32, begin as u32, output, timing,
            );
            work.errors.add(index).write(failure);
        }
        begin = work.next.fetch_add(work.grain, Ordering::Relaxed);
    }
}
pub(crate) struct WorkHandle(*mut c_void);
impl WorkHandle {
    /// The context stays valid until the submitted callbacks and this handle end.
    pub(crate) unsafe fn new(
        entry: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void), context: *mut c_void,
    ) -> std::io::Result<Self> {
        let work = unsafe { CreateThreadpoolWork(entry, context, std::ptr::null()) };
        if work.is_null() { Err(std::io::Error::last_os_error()) } else { Ok(Self(work)) }
    }

    pub(crate) unsafe fn submit(&self) {
        unsafe { SubmitThreadpoolWork(self.0) };
    }
}
impl Drop for WorkHandle {
    fn drop(&mut self) {
        unsafe {
            WaitForThreadpoolWorkCallbacks(self.0, 0);
            CloseThreadpoolWork(self.0);
        }
    }
}
#[repr(C)]
struct RuntimeFunction { begin: u32, end: u32, unwind: u32 }
#[repr(C)]
struct MemoryBasicInformation {
    base_address: *mut c_void,
    allocation_base: *mut c_void,
    allocation_protect: u32,
    partition_id: u16,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}
/// 原CPU代码入口的当前Windows VM区段快照；不持有代码或授予地址访问权。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuCodeMemory {
    /// 同一ExecutionPlan中的阶段索引；一个阶段可以包含多个原生代码入口。
    pub stage: usize,
    pub entry_address: usize,
    /// 所属执行程序是否持有代码分配。外部已链接模块的代码为false。
    pub runtime_owned: bool,
    pub allocation_address: usize,
    pub region_address: usize,
    /// VirtualQuery从入口所在页返回的区段跨度，不是单函数机器码长度。
    pub region_bytes: usize,
    /// Windows原始AllocationProtect、Protect、State及Type值。
    pub allocation_protection: u32,
    pub protection: u32,
    pub state: u32,
    pub kind: u32,
}
struct Allocation {
    address: *mut c_void,
    functions: Vec<NonNull<RuntimeFunction>>,
    _libraries: Vec<Library>,
}
struct LoadedCode {
    allocation: Arc<Allocation>,
    table: NonNull<[RuntimeFunction]>,
}
impl LoadedCode {
    fn entries(&self) -> &[RuntimeFunction] {
        // The initialized root table is read-only and belongs to allocation.
        unsafe { self.table.as_ref() }
    }
}
fn external_symbol<'image>(
    libraries: &mut BTreeMap<&'image str, Library>, name: &'image blanguage::ast::ExternalName,
) -> Result<*const ()> {
    if name.library == blanguage::resource::RUNTIME_LIBRARY {
        return crate::application::resource_symbol(&name.symbol)
            .ok_or_else(|| error(format!("未知的Blink资源C符号 {}", name.symbol)));
    }
    let library = match libraries.entry(name.library.as_str()) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => entry.insert(Library::load(&name.library)
            .map_err(|error| crate::error(format!("加载 {} 失败：{error}", name.library)))?),
    };
    library.symbol(&std::ffi::CString::new(name.symbol.as_str())?)
        .map_err(|error| crate::error(format!("解析 {}!{} 失败：{error}", name.library, name.symbol)))
}

fn write_stdout(text: &[u8]) -> u32 {
    use std::io::Write;
    match std::io::stdout().lock().write(text) {
        Ok(n) => u32::from(n == text.len()),
        Err(_) => 0,
    }
}
fn print_line(value: impl std::fmt::Display) -> u32 {
    write_stdout(format!("{value}\n").as_bytes())
}
extern "C" fn print_u32(value: u32) -> u32 {
    print_line(value)
}
extern "C" fn print_i32(value: i32) -> u32 {
    print_line(value)
}
extern "C" fn print_u64(value: u64) -> u32 {
    print_line(value)
}
extern "C" fn print_i64(value: i64) -> u32 {
    print_line(value)
}
extern "C" fn print_bool(value: u32) -> u32 {
    print_line(value != 0)
}
#[link(name = "ucrt")]
unsafe extern "C" {
    fn _create_locale(category: i32, locale: *const std::ffi::c_char) -> *mut c_void;
    fn _free_locale(locale: *mut c_void);
    fn __stdio_common_vsprintf(
        options: u64,
        buffer: *mut std::ffi::c_char,
        count: usize,
        format: *const std::ffi::c_char,
        locale: *mut c_void,
        arguments: *const c_void,
    ) -> i32;
}
// 与独立 PE 使用同一 UCRT 格式及 C 数值区域，不改变宿主的当前区域设置。
extern "C" fn print_float(bits: u64, width: u32) -> u32 {
    let (value, format) = if width == 64 {
        (f64::from_bits(bits), c"%.17g\n")
    } else {
        (f32::from_bits(bits as u32) as f64, c"%.9g\n")
    };
    let mut text = [0u8; 32];
    let count = unsafe {
        let locale = _create_locale(4, c"C".as_ptr());
        if locale.is_null() {
            return 0;
        }
        let count = __stdio_common_vsprintf(
            2,
            text.as_mut_ptr().cast(),
            text.len(),
            format.as_ptr(),
            locale,
            (&value as *const f64).cast(),
        );
        _free_locale(locale);
        count
    };
    if count < 0 {
        return 0;
    }
    write_stdout(&text[..count as usize])
}
impl CpuCodeMemory {
    fn query(stage: usize, address: *mut c_void, runtime_owned: bool) -> Result<Self> {
        let mut information = std::mem::MaybeUninit::<MemoryBasicInformation>::zeroed();
        if unsafe { VirtualQuery(address, information.as_mut_ptr(), std::mem::size_of::<MemoryBasicInformation>()) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let information = unsafe { information.assume_init() };
        Ok(Self {
            stage,
            entry_address: address as usize,
            runtime_owned,
            allocation_address: information.allocation_base as usize,
            region_address: information.base_address as usize,
            region_bytes: information.region_size,
            allocation_protection: information.allocation_protect,
            protection: information.protect,
            state: information.state,
            kind: information.kind,
        })
    }
}
impl Allocation {
    fn place_code(cursor: &mut usize, code: &blanguage::native::NativeCode) -> (usize, usize) {
        let begin = cursor.div_ceil(16) * 16;
        let unwind = (begin + code.bytes.len()).div_ceil(4) * 4;
        *cursor = unwind + code.unwind.len();
        (begin, unwind)
    }

    fn register_functions(&mut self, table: NonNull<RuntimeFunction>, count: u32) -> Result<()> {
        if count != 0 {
            if unsafe { RtlAddFunctionTable(table.as_ptr(), count, self.address as u64) } == 0 {
                return Err(error("Windows未接受CPU代码的原生展开信息"));
            }
            self.functions.push(table);
        }
        Ok(())
    }

    fn load<P>(
        image: &blanguage::native::ProgramImage<P>, workers: usize,
    ) -> Result<(Option<LoadedCode>, Option<crate::application::Definitions>)> {
        let module = image.module_code();
        let mut size = 0usize;
        let mut count = 0usize;
        for code in image.code() {
            let (_, unwind) = Self::place_code(&mut size, code);
            // The aligned unwind offset also bounds the preceding code offset.
            u32::try_from(unwind)?;
            count += 1;
        }
        if count == 0 && module.is_none() { return Ok((None, None)); }
        let mut sections = Vec::new();
        let mut imports = std::collections::BTreeMap::new();
        if let Some(module) = module {
            for section in &module.sections {
                let alignment = (1usize << ((section.flags >> 20) & 15).saturating_sub(1)).max(4096);
                size = size.div_ceil(alignment) * alignment;
                sections.push(size);
                size += section.bytes.len();
                for relocation in &section.relocations {
                    if let blanguage::native::ModuleTarget::Import(name) = &relocation.target {
                        imports.entry(name).or_insert(0usize);
                    }
                }
            }
            size = size.div_ceil(4096) * 4096;
        }
        for offset in imports.values_mut() { *offset = size; size += 8; }
        let table_offset = size.div_ceil(4) * 4;
        let count = u32::try_from(count)?;
        size = table_offset + count as usize * std::mem::size_of::<RuntimeFunction>();
        let address = unsafe { VirtualAlloc(std::ptr::null_mut(), size, 0x3000, 0x04) };
        if address.is_null() { return Err(std::io::Error::last_os_error().into()); }
        let mut allocation = Self { address, functions: Vec::new(), _libraries: Vec::new() };
        let table = unsafe { NonNull::new_unchecked(address.cast::<u8>().add(table_offset).cast::<RuntimeFunction>()) };
        let mut cursor = 0;
        for (index, code) in image.code().enumerate() {
            let (begin, unwind) = Self::place_code(&mut cursor, code);
            let begin_rva = begin as u32;
            unsafe {
                std::ptr::copy_nonoverlapping(code.bytes.as_ptr(), address.cast::<u8>().add(begin), code.bytes.len());
                std::ptr::copy_nonoverlapping(code.unwind.as_ptr(), address.cast::<u8>().add(unwind), code.unwind.len());
                table.as_ptr().add(index).write(RuntimeFunction {
                    begin: begin_rva, end: begin_rva + u32::try_from(code.function_end)?, unwind: unwind as u32,
                });
            }
        }
        let root_table = NonNull::slice_from_raw_parts(table, count as usize);
        let entries = unsafe { root_table.as_ref() };
        let mut functions = image.functions().iter().zip(&entries[image.stage_code_count()..])
            .map(|((name, _), row)| {
                let entry = unsafe { address.cast::<u8>().add(row.begin as usize) }.cast::<c_void>();
                (name.as_str(), entry)
            }).collect::<BTreeMap<_, _>>();
        let mut libraries = std::collections::BTreeMap::new();
        for (&name, &offset) in &imports {
            let symbol = external_symbol(&mut libraries, name)?;
            unsafe { std::ptr::write_unaligned(address.cast::<u8>().add(offset).cast::<u64>(), symbol as u64); }
        }
        if let Some(module) = module {
            for (section, &begin) in module.sections.iter().zip(&sections) {
                let entry = unsafe { address.cast::<u8>().add(begin) };
                unsafe { std::ptr::copy_nonoverlapping(section.bytes.as_ptr(), entry, section.bytes.len()); }
                let bytes = unsafe { std::slice::from_raw_parts_mut(entry, section.bytes.len()) };
                for relocation in &section.relocations {
                    let target = match &relocation.target {
                        blanguage::native::ModuleTarget::Section { section, value } => sections[*section] + *value as usize,
                        blanguage::native::ModuleTarget::Import(name) => imports[name],
                    };
                    blanguage::native::coff_relocate(bytes, relocation.offset, relocation.kind,
                        address as u64, (begin + relocation.offset) as u64, target as u64)
                        .map_err(error)?;
                }
                if section.name == ".pdata" {
                    let count = u32::try_from(section.bytes.len() / std::mem::size_of::<RuntimeFunction>())?;
                    let table = entry.cast::<RuntimeFunction>();
                    unsafe { std::slice::from_raw_parts_mut(table, count as usize) }.sort_unstable_by_key(|entry| entry.begin);
                }
            }
            for (name, &(section, value)) in &module.symbols {
                let entry = unsafe { address.cast::<u8>().add(sections[section] + value as usize) }.cast();
                functions.insert(name.as_str(), entry);
            }
        }
        for (code, row) in image.code().zip(entries) {
            let entry = unsafe { address.cast::<u8>().add(row.begin as usize) };
            for relocation in &code.relocations {
                let ptr = match &relocation.target {
                    NativeSymbol::Function(name) => *functions.get(name.as_str())
                        .ok_or_else(|| error(format!("范围映像缺少所属模块函数 {name}")))? as *const (),
                    NativeSymbol::External(name) => external_symbol(&mut libraries, name)?,
                    NativeSymbol::Print(scalar) => match scalar {
                        Scalar::U8 | Scalar::U16 | Scalar::U32 => print_u32 as *const (),
                        Scalar::I8 | Scalar::I16 | Scalar::I32 => print_i32 as *const (),
                        Scalar::U64 => print_u64 as *const (),
                        Scalar::I64 => print_i64 as *const (),
                        Scalar::Bool => print_bool as *const (),
                        Scalar::F32 | Scalar::F64 => print_float as *const (),
                    },
                };
                unsafe {
                    std::ptr::copy_nonoverlapping((ptr as u64).to_le_bytes().as_ptr(), entry.add(relocation.offset), 8);
                }
            }
        }
        allocation._libraries = libraries.into_values().collect();
        unsafe {
            let mut old = 0;
            if VirtualProtect(address, size, 0x20, &mut old) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            if let Some(module) = module {
                for (section, &begin) in module.sections.iter().zip(&sections) {
                    if section.bytes.is_empty() { continue; }
                    let protect = match section.flags & 0xe0000000 {
                        0xe0000000 | 0xa0000000 => 0x40,
                        0x60000000 => 0x20,
                        0x20000000 => 0x10,
                        0xc0000000 | 0x80000000 => 0x04,
                        0x40000000 => 0x02,
                        _ => 0x01,
                    };
                    if VirtualProtect(address.cast::<u8>().add(begin).cast(), section.bytes.len(), protect, &mut old) == 0 {
                        return Err(std::io::Error::last_os_error().into());
                    }
                }
            }
            if FlushInstructionCache(GetCurrentProcess(), address, size) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        allocation.register_functions(table, count)?;
        if let Some(module) = module {
            for (section, &begin) in module.sections.iter().zip(&sections) {
                if section.name == ".pdata" {
                    // Relocation and sorting already checked this section's entry count.
                    let count = (section.bytes.len() / std::mem::size_of::<RuntimeFunction>()) as u32;
                    let table = unsafe { NonNull::new_unchecked(address.cast::<u8>().add(begin).cast::<RuntimeFunction>()) };
                    allocation.register_functions(table, count)?;
                }
            }
        }
        let allocation = Arc::new(allocation);
        // The compiler-produced table is already relocated in this allocation.
        // Definitions consumes it synchronously while the code owner is live.
        let factories = match functions.get("__blink_module_count") {
            Some(&count) => unsafe {
                let count = *count.cast::<usize>();
                let table = functions["__blink_module_factories"].cast::<blanguage::resource::ModuleFactory>();
                std::slice::from_raw_parts(table, count)
            },
            None => &[],
        };
        let definitions = if factories.is_empty() {
            None
        } else {
            Some(unsafe { crate::application::Definitions::linked(factories, |image, code| {
                Ok(CpuTemplate { code: CpuCode::bind_indices(image, workers, code, Some(&allocation)) })
            }) }?)
        };
        Ok((Some(LoadedCode { allocation, table: root_table }), definitions))
    }
}
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            for table in &self.functions { RtlDeleteFunctionTable(table.as_ptr()); }
            VirtualFree(self.address, 0, 0x8000);
        }
    }
}

// A summary word may contain both initialized field bytes and padding.
// The selected scalar must be within the row and initialized by the producer.
unsafe fn collective_bits(scalar: Scalar, words: &[MaybeUninit<u32>], byte: usize) -> Bits {
    let mut bytes = [0; 8];
    unsafe { std::ptr::copy_nonoverlapping(words.as_ptr().cast::<u8>().add(byte), bytes.as_mut_ptr(), scalar.size() as usize); }
    Bits::from_ne_bytes(bytes)
}
unsafe fn store_collective_bits(scalar: Scalar, words: &mut [MaybeUninit<u32>], byte: usize, bits: Bits) {
    unsafe { std::ptr::copy_nonoverlapping(bits.to_ne_bytes().as_ptr(), words.as_mut_ptr().cast::<u8>().add(byte), scalar.size() as usize); }
}
fn collective_pair(scalar: Scalar, operator: CollectiveOperator, a: Bits, b: Bits) -> Bits {
    scalar.truncate(match (scalar, operator) {
        (_, CollectiveOperator::BitAnd) => a & b,
        (_, CollectiveOperator::BitOr) => a | b,
        (_, CollectiveOperator::BitXor) => a ^ b,
        (Scalar::F64, CollectiveOperator::Sum) => (f64::from_bits(a)+f64::from_bits(b)).to_bits(),
        (Scalar::F64, CollectiveOperator::Min) => if f64::from_bits(a)<f64::from_bits(b) { a } else { b },
        (Scalar::F64, CollectiveOperator::Max) => if f64::from_bits(a)>f64::from_bits(b) { a } else { b },
        (Scalar::F32, CollectiveOperator::Sum) => Bits::from((f32::from_bits(a as u32) + f32::from_bits(b as u32)).to_bits()),
        (Scalar::F32, CollectiveOperator::Min) => {
            if f32::from_bits(a as u32) < f32::from_bits(b as u32) {
                a
            } else {
                b
            }
        }
        (Scalar::F32, CollectiveOperator::Max) => {
            if f32::from_bits(a as u32) > f32::from_bits(b as u32) {
                a
            } else {
                b
            }
        }
        (_, CollectiveOperator::Sum) => a.wrapping_add(b),
        (_, CollectiveOperator::Min) if scalar.is_signed() => if scalar.signed(a) < scalar.signed(b) {a} else {b},
        (_, CollectiveOperator::Max) if scalar.is_signed() => if scalar.signed(a) > scalar.signed(b) {a} else {b},
        (_, CollectiveOperator::Min) => a.min(b),
        (_, CollectiveOperator::Max) => a.max(b),
    })
}

enum Continuation {
    Apply(*mut c_void),
    Scan {
        prefix: *mut c_void,
        apply: *mut c_void,
        source: usize,
        target: usize,
    },
}
enum StageCode {
    Element {
        main: *mut c_void,
        continuation: Option<Continuation>,
        result: RangeResult,
    },
    Sort(sort::Code),
}
pub struct CpuProgram {
    code: CpuCode,
}
pub struct CpuTemplate {
    code: CpuCode,
}
struct CpuCode {
    program: Program,
    definitions: Option<std::sync::Arc<crate::application::Definitions>>,
    stages: Vec<StageCode>,
    workers: usize,
    // All addresses in this program borrow the same loaded code.
    allocation: Option<Arc<Allocation>>,
}
pub struct Bindings<D = Vec<Dataset>> {
    pub datasets: D,
    pub textures: Vec<TextureInput>,
}
/// 错误连同本次实际拥有组交回；数据及纹理只移动，已发生的CPU写入保持。
pub type OwnedRunResult = std::result::Result<
    DomainBindings<CpuDataset>,
    (Box<dyn std::error::Error + Send + Sync>, Bindings<DomainBindings<CpuDataset>>),
>;
impl<D: Into<CpuDataset>> From<Vec<D>> for Bindings<DomainBindings<CpuDataset>> {
    fn from(datasets: Vec<D>) -> Self {
        Self {
            datasets: datasets
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>()
                .into(),
            textures: Vec::new(),
        }
    }
}
impl From<DomainBindings<Dataset>> for Bindings<DomainBindings<CpuDataset>> {
    fn from(datasets: DomainBindings<Dataset>) -> Self {
        Self {
            datasets: datasets.into(),
            textures: Vec::new(),
        }
    }
}
impl From<DomainBindings<CpuDataset>> for Bindings<DomainBindings<CpuDataset>> {
    fn from(datasets: DomainBindings<CpuDataset>) -> Self {
        Self {
            datasets,
            textures: Vec::new(),
        }
    }
}
impl From<Bindings<DomainBindings<Dataset>>> for Bindings<DomainBindings<CpuDataset>> {
    fn from(bindings: Bindings<DomainBindings<Dataset>>) -> Self {
        Self {
            datasets: bindings.datasets.into(),
            textures: bindings.textures,
        }
    }
}
impl<D: Into<CpuDataset>> From<Bindings<Vec<D>>> for Bindings<DomainBindings<CpuDataset>> {
    fn from(bindings: Bindings<Vec<D>>) -> Self {
        Self {
            datasets: bindings
                .datasets
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>()
                .into(),
            textures: bindings.textures,
        }
    }
}
impl CpuProgram {
    /// 只读查询原代码入口的当前VM区段和原拥有关系；结果不延长代码寿命。
    /// 外部模块的多个入口可能共用区段，不能逐入口累加为独占分配。
    pub fn code_memory(&self) -> Result<Vec<CpuCodeMemory>> {
        self.code.code_memory()
    }
    /// 借用实际执行程序的唯一数据布局，供原生消费者创建绑定。
    pub fn program(&self) -> &Program {
        &self.code.program
    }
    #[cfg(feature="compiler")]
    pub fn new(program: ExecutableProgram) -> Result<Self> {
        Self::from_image(blanguage::native::compile_program(program).map_err(error)?)
    }
    pub fn from_image(image: blanguage::native::ProgramImage<ExecutableProgram>) -> Result<Self> {
        Ok(Self {
            code: CpuCode::raw(image).map_err(|(error, _)| error)?,
        })
    }
    /// 借用所属原生模块的已链接代码；执行布局仍由同一编译产物交接。
    ///
    /// # Safety
    /// 映像必须来自同一无前置条件入口的编译产物。每个地址都必须具有对应范围ABI；
    /// 所属模块及其依赖须保持到最后真实CPU工作和本程序消费者结束。
    #[doc(hidden)]
    pub unsafe fn from_linked(image: blanguage::native::ProgramImage<ExecutableProgram, *const u8>) -> Result<Self> {
        Ok(Self { code: CpuCode::linked(image).map_err(|(error, _)| error)? })
    }
    /// 绑定同次静态链接的范围代码与唯一模块工厂目录，不创建中间Frame或代码VM。
    ///
    /// # Safety
    /// image的目录已交给factories而不再嵌入映像；工厂顺序对应静态基址，
    /// 所有代码与资源合同来自同次可信产物，模块及依赖保持到最后消费者结束。
    #[doc(hidden)]
    pub unsafe fn from_linked_modules(
        image: blanguage::native::ProgramImage<ExecutableProgram, *const u8>,
        factories: &[blanguage::resource::ModuleFactory],
    ) -> Result<Self> {
        unsafe { Self::bind_linked_root(image, |address| *address, factories) }
    }

    /// The image, entry addresses and optional factory table come from the same trusted link.
    pub(crate) unsafe fn from_linked_indices(
        image: blanguage::native::ProgramImage<ExecutableProgram, usize>,
        entries: &[*const u8],
        factories: Option<&[blanguage::resource::ModuleFactory]>,
    ) -> Result<Self> {
        if let Some(factories) = factories {
            return unsafe { Self::bind_linked_root(image, |index| entries[*index], factories) };
        }
        let workers = std::thread::available_parallelism()?.get();
        Ok(Self { code: CpuCode::bind_indices(image, workers, entries, None) })
    }

    unsafe fn bind_linked_root<C>(
        image: blanguage::native::ProgramImage<ExecutableProgram, C>,
        entry: impl FnMut(&C) -> *const u8,
        factories: &[blanguage::resource::ModuleFactory],
    ) -> Result<Self> {
        let workers = std::thread::available_parallelism()?.get();
        let stages = CpuCode::load_linked_stages(image.stages(), entry);
        let (program, _, _, _) = image.into_parts();
        let definitions = unsafe {
            crate::application::Definitions::linked(factories, |image, code| CpuTemplate::from_linked_indices(image, code))
        }?;
        Ok(Self { code: CpuCode {
            program, stages, workers, definitions: Some(Arc::new(definitions)), allocation: None,
        } })
    }
    pub fn run(&self, datasets: &mut [Dataset], values: &[Value]) -> Result<()> {
        self.code.run(datasets, &[], values)
    }
    pub fn run_owned(
        &self,
        bindings: impl Into<Bindings<DomainBindings<CpuDataset>>>,
        values: &[Value],
    ) -> OwnedRunResult {
        self.code.run_owned(bindings.into(), values, None)
    }

    /// Execute the original owned run and append the native calls actually made.
    pub fn run_owned_observed(
        &self,
        bindings: impl Into<Bindings<DomainBindings<CpuDataset>>>,
        values: &[Value],
        timings: &mut CpuRangeTimings,
    ) -> OwnedRunResult {
        self.code.run_owned(bindings.into(), values, Some(timings))
    }
}
impl CpuTemplate {
    /// 只读查询原代码入口的当前VM区段和原拥有关系；结果不延长代码寿命。
    /// 外部模块的多个入口可能共用区段，不能逐入口累加为独占分配。
    pub fn code_memory(&self) -> Result<Vec<CpuCodeMemory>> {
        self.code.code_memory()
    }
    pub fn program(&self) -> &Program {
        &self.code.program
    }
    #[cfg(feature="compiler")]
    pub fn new(program: TemplateProgram) -> Result<Self> {
        Self::from_image(blanguage::native::compile_program(program).map_err(error)?)
            .map_err(|(error, _)| error)
    }
    /// 借用所属原生模块的已链接条件范围，失败交回同一未改映像。
    ///
    /// # Safety
    /// 映像及地址必须来自同一TemplateProgram编译产物并满足原范围ABI；
    /// 所属模块与依赖保持到最后CPU工作和程序消费者结束，原前缀前提由实际调用交接。
    #[doc(hidden)]
    pub unsafe fn from_linked(image: blanguage::native::ProgramImage<TemplateProgram, *const u8>) -> std::result::Result<Self, (Box<dyn std::error::Error + Send + Sync>, blanguage::native::ProgramImage<TemplateProgram, *const u8>)> {
        CpuCode::linked(image).map(|code| Self { code })
    }
    /// The template and entry table belong to the same linked module.
    pub(crate) unsafe fn from_linked_indices(
        image: blanguage::native::ProgramImage<TemplateProgram, usize>,
        entries: &[*const u8],
    ) -> Result<Self> {
        let workers = std::thread::available_parallelism()?.get();
        Ok(Self { code: CpuCode::bind_indices(image, workers, entries, None) })
    }
    /// 装入失败交回原错误和同一未改映像；本调用没有执行数据。
    pub fn from_image(image: blanguage::native::ProgramImage<TemplateProgram>) -> std::result::Result<Self, (Box<dyn std::error::Error + Send + Sync>, blanguage::native::ProgramImage<TemplateProgram>)> {
        CpuCode::raw(image).map(|code| Self { code })
    }
    pub fn run(
        &self,
        bindings: impl Into<Bindings<DomainBindings<CpuDataset>>>,
        values: &[Value],
    ) -> OwnedRunResult {
        self.code.run_owned(bindings.into(), values, None)
    }

    /// Execute the original template and append the native calls actually made.
    pub fn run_observed(
        &self,
        bindings: impl Into<Bindings<DomainBindings<CpuDataset>>>,
        values: &[Value],
        timings: &mut CpuRangeTimings,
    ) -> OwnedRunResult {
        self.code.run_owned(bindings.into(), values, Some(timings))
    }

    pub(crate) fn run_projected(
        &self,
        datasets: &mut [&mut ResidentData],
        domains: &mut DomainRelations,
        arguments: &FrameArguments<'_>,
        textures: &[CpuTextureBinding<'_>],
        uniforms: Vec<u32>,
        definitions: &std::sync::Arc<crate::application::Definitions>,
    ) -> Result<()> {
        self.code.run_mapped(datasets, domains, textures, uniforms,
            StorageSlots::Frame(arguments), Some(definitions), None)
    }
}
#[derive(Clone, Copy)]
pub(crate) struct CpuBuffer<'a> {
    pub(crate) ty: &'a Ty,
    pub(crate) stride: u32,
    pub(crate) count: u32,
    pub(crate) pointer: *mut u32,
    pub(crate) writable: bool,
}
impl<'a> CpuBuffer<'a> {
    fn require_layout(data: &Dataset) -> Result<()> {
        if data.stride != blanguage::ir::align_up(data.ty.size, data.ty.align) {
            return Err(error("原生CPU绑定的物理记录布局与其类型不一致"));
        }
        Ok(())
    }
    fn owned(data: &'a mut Dataset) -> Result<Self> {
        Self::require_layout(data)?;
        data.require_views()?;
        Ok(Self {
            ty: &data.ty,
            stride: data.stride,
            count: data.count,
            pointer: data.words.as_mut_ptr(),
            writable:true,
        })
    }
    fn read(data: &'a ReadDataset) -> Result<Self> {
        Self::require_layout(data)?;
        Ok(Self {
            ty: &data.ty,
            stride: data.stride,
            count: data.count,
            pointer: data.words.as_ptr().cast_mut(),
            writable:false,
        })
    }
}
trait CpuStorage:crate::DomainStorage {
    fn cpu_metadata(&self)->(&Ty,u32,bool);
    fn cpu_buffer(&mut self)->Result<CpuBuffer<'_>>;
}
impl CpuStorage for CpuDataset {
    fn cpu_metadata(&self)->(&Ty,u32,bool){(&self.ty,self.count,!matches!(self,CpuDataset::Read(_)))}
    fn cpu_buffer(&mut self)->Result<CpuBuffer<'_>>{match self {Self::Owned(data)=>CpuBuffer::owned(data),Self::Read(data)=>CpuBuffer::read(data)}}
}
impl CpuStorage for &mut ResidentData {
    fn cpu_metadata(&self)->(&Ty,u32,bool){match &**self {
        ResidentData::Cpu(data)=>data.cpu_metadata(),
        ResidentData::CpuBorrow(data)=>(data.layout(),data.count,data.writable),
        ResidentData::Gpu(data)=>(data.layout(),data.count(),false),
    }}
    fn cpu_buffer(&mut self)->Result<CpuBuffer<'_>>{match &mut **self {
        ResidentData::Cpu(data)=>data.cpu_buffer(),
        ResidentData::CpuBorrow(data)=>Ok(CpuBuffer{ty:data.layout(),stride:data.stride,count:data.count,pointer:data.pointer,writable:data.writable}),
        ResidentData::Gpu(_)=>Err(error("CPU 访问需要先显式 download 数据")),
    }}
}
impl CpuCode {
    fn code_memory(&self) -> Result<Vec<CpuCodeMemory>> {
        let mut result = Vec::new();
        let runtime_owned = self.allocation.is_some();
        let mut add = |stage, address| -> Result<()> {
            result.push(CpuCodeMemory::query(stage, address, runtime_owned)?);
            Ok(())
        };
        for (stage, code) in self.stages.iter().enumerate() {
            match code {
                StageCode::Element { main, continuation, .. } => {
                    add(stage, *main)?;
                    match continuation {
                        Some(Continuation::Apply(code)) => add(stage, *code)?,
                        Some(Continuation::Scan { prefix, apply, .. }) => {
                            add(stage, *prefix)?;
                            add(stage, *apply)?;
                        }
                        None => {}
                    }
                }
                StageCode::Sort(code) => {
                    for address in code.entries() {
                        add(stage, address)?;
                    }
                }
            }
        }
        Ok(result)
    }
    fn raw<P>(image: blanguage::native::ProgramImage<P>) -> std::result::Result<Self, (Box<dyn std::error::Error + Send + Sync>, blanguage::native::ProgramImage<P>)> {
        let workers=match std::thread::available_parallelism(){Ok(workers)=>workers.get(),Err(error)=>return Err((error.into(),image))};
        let (loaded, definitions) = match Allocation::load(&image, workers) {
            Ok(code) => code,
            Err(error) => return Err((error, image)),
        };
        let mut remaining = loaded.as_ref().map_or(&[][..], LoadedCode::entries);
        let allocation = loaded.as_ref().map(|loaded| &loaded.allocation);
        Ok(Self::from_image(image, workers, &mut |stages, functions| {
            let stages = Self::load_stages(stages, &mut |_| {
                let (row, rest) = remaining.split_first().unwrap();
                remaining = rest;
                unsafe { allocation.unwrap().address.cast::<u8>().add(row.begin as usize) }.cast()
            });
            // These entries were already placed and resolved by Allocation::load.
            remaining = &remaining[functions..];
            stages
        }, definitions, allocation))
    }
    fn linked<P>(image: blanguage::native::ProgramImage<P, *const u8>) -> std::result::Result<Self, (Box<dyn std::error::Error + Send + Sync>, blanguage::native::ProgramImage<P, *const u8>)> {
        let workers=match std::thread::available_parallelism(){Ok(workers)=>workers.get(),Err(error)=>return Err((error.into(),image))};
        Ok(Self::from_image(image, workers, &mut |stages, _| {
            Self::load_linked_stages(stages, |address| *address)
        }, None, None))
    }
    fn bind_indices<P>(
        image: blanguage::native::ProgramImage<P, usize>, workers: usize,
        entries: &[*const u8], allocation: Option<&Arc<Allocation>>,
    ) -> Self {
        Self::from_image(image, workers, &mut |stages, _| {
            Self::load_linked_stages(stages, |index| entries[*index])
        }, None, allocation)
    }
    fn from_image<P, C>(
        image: blanguage::native::ProgramImage<P, C>, workers: usize,
        load: &mut impl FnMut(&[blanguage::native::StageCode<C>], usize) -> Vec<StageCode>,
        linked: Option<crate::application::Definitions>,
        allocation: Option<&Arc<Allocation>>,
    ) -> Self {
        use blanguage::resource::Image;
        use crate::application::{Definitions, ProgramSlot};

        let stages = load(image.stages(), image.functions().len());
        let (program, _, _, resources) = image.into_parts();
        let definitions = match resources {
            Some(module) => {
                let programs = module.images.into_iter().map(|image| match image {
                    Image::Cpu(image) => ProgramSlot::Cpu(CpuTemplate {
                        code: Self::from_image(image, workers, load, None, allocation),
                    }),
                    Image::Gpu(image) => ProgramSlot::Gpu(std::sync::Arc::new(image)),
                });
                Some(match linked {
                    Some(mut definitions) => {
                        // Linked module indices start after the root's types and programs.
                        drop(definitions.types.splice(0..0, module.types));
                        drop(definitions.programs.splice(0..0, programs));
                        definitions
                    }
                    None => Definitions { types: module.types, programs: programs.collect() },
                })
            }
            None => linked,
        }.map(std::sync::Arc::new);
        let allocation = allocation.filter(|_| !stages.is_empty()).cloned();
        Self { program, stages, workers, definitions, allocation }
    }
    fn load_linked_stages<C>(
        stages: &[blanguage::native::StageCode<C>],
        mut entry: impl FnMut(&C) -> *const u8,
    ) -> Vec<StageCode> {
        Self::load_stages(stages, &mut |code| entry(code).cast_mut().cast())
    }
    fn load_stages<C>(stages: &[blanguage::native::StageCode<C>], load: &mut impl FnMut(&C) -> *mut c_void) -> Vec<StageCode> {
        use blanguage::native::{ContinuationCode,StageCode as ImageStage};
        stages.iter().map(|image|match image{
            ImageStage::Sort(code)=>StageCode::Sort(sort::Code::load(code,load)),
            ImageStage::Element{main,continuation,result}=>StageCode::Element{
                main:load(main),
                continuation:continuation.as_ref().map(|continuation|match continuation{
                    ContinuationCode::ReductionTail(code)|ContinuationCode::ScanAdd(code)
                    |ContinuationCode::CompactScatter(code)|ContinuationCode::SegmentedFinish(code)=>Continuation::Apply(load(code)),
                    ContinuationCode::Scan{prefix,apply,source,target}=>Continuation::Scan{
                        prefix:load(prefix),apply:load(apply),source:*source,target:*target,
                    },
                }),
                result:*result,
            },
        }).collect()
    }
    fn run_owned(
        &self,
        mut bindings: Bindings<DomainBindings<CpuDataset>>,
        values: &[Value],
        timings: Option<&mut CpuRangeTimings>,
    ) -> OwnedRunResult {
        let uniforms = arguments(&self.program, bindings.datasets.datasets.iter().map(|data| {
            let (ty, count, _) = data.cpu_metadata();
            (ty, count, Some(0))
        }), values);
        let result = uniforms.and_then(|uniforms| {
            let descriptions = CpuTextures::new(&bindings.textures);
            let DomainBindings { datasets, domains } = &mut bindings.datasets;
            self.run_mapped(datasets, domains, &descriptions.bindings(), uniforms,
                StorageSlots::Direct, self.definitions.as_ref(), timings)
        });
        match result {
            Ok(()) => Ok(bindings.datasets),
            Err(error) => Err((error, bindings)),
        }
    }
    fn run_mapped<D:CpuStorage>(
        &self,
        datasets: &mut [D],
        domains: &mut DomainRelations,
        textures: &[CpuTextureBinding<'_>],
        uniforms: Vec<u32>,
        slots: StorageSlots<'_>,
        definitions:Option<&std::sync::Arc<crate::application::Definitions>>,
        timings: Option<&mut CpuRangeTimings>,
    ) -> Result<()> {
        bindings::require(|slot| datasets.get(slot), domains, &self.program, slots)?;
        let physical = |i| slots.location(i);
        let count = self.program.buffers().len();
        for (data, binding) in (0..count)
            .map(|i| &datasets[physical(i).0])
            .zip(self.program.buffers())
        {
            if !data.cpu_metadata().2 && binding.access != blanguage::ast::Access::Read
            {
                return Err(error("冻结的 CPU 存储只能绑定 read dataset"));
            }
        }
        let input_domains = if self.program.native_frame().is_some() {
            bindings::projected_relations(datasets, domains, &self.program, slots)
        } else {
            Vec::new()
        };
        let mut buffers = Vec::with_capacity(count);
        for data in &mut *datasets {
            buffers.push(data.cpu_buffer()?);
        }
        // arguments已核对同一存储类型与偏移；这里只交接原指针描述符。
        let project = |buffers: &[CpuBuffer<'_>], i| {
            let (physical, offset) = physical(i);
            let expected = &self.program.buffers()[i];
            let buffer = buffers[physical];
            CpuBuffer {
                ty: &expected.ty,
                pointer: buffer.pointer.cast::<u8>().wrapping_add(offset as usize).cast(),
                writable: buffer.writable && expected.access != blanguage::ast::Access::Read,
                ..buffer
            }
        };
        let physical_count = buffers.len();
        for i in physical_count..count {
            buffers.push(project(&buffers, i));
        }
        // 物理槽按首次绑定分配，其索引不大于当前逻辑索引。逆序覆盖保留尚未消费的槽。
        for i in (0..physical_count).rev() {
            buffers[i] = project(&buffers, i);
        }
        let original = |i| slots.original(i);
        self.run_buffers(&buffers, textures, uniforms, || {
            bindings::invalidate(domains, &self.program.write_regions, original);
        }, definitions, input_domains, timings)?;
        bindings::produce(domains, &self.program, original, |i| datasets[physical(i).0].element_count());
        Ok(())
    }
    fn run(
        &self,
        datasets: &mut [Dataset],
        textures: &[TextureInput],
        values: &[Value],
    ) -> Result<()> {
        let uniforms = arguments(&self.program, datasets.iter().map(|data| (&data.ty, data.count, None)), values)?;
        let buffers = datasets
            .iter_mut()
            .map(CpuBuffer::owned)
            .collect::<Result<Vec<_>>>()?;
        let descriptions = CpuTextures::new(textures);
        self.run_buffers(&buffers, &descriptions.bindings(), uniforms, || {}, self.definitions.as_ref(), Vec::new(), None)
    }
    fn run_buffers(
        &self,
        buffers: &[CpuBuffer<'_>],
        textures: &[CpuTextureBinding<'_>],
        mut uniforms: Vec<u32>,
        writing: impl FnOnce(),
        definitions:Option<&std::sync::Arc<crate::application::Definitions>>,
        input_domains: Vec<crate::DomainRelation>,
        mut timings: Option<&mut CpuRangeTimings>,
    ) -> Result<()> {
        if textures.len() != self.program.texture_count()
            || textures.iter().zip(self.program.textures().iter().flat_map(|binding|
                std::iter::repeat_n(binding, binding.elements() as usize)))
                .any(|(texture, binding)| texture.dimension() != binding.dimension
                    || binding.format.is_some_and(|format| texture.format_code() != format as u32)) {
            return Err(error("纹理绑定数量、维度或格式与程序接口不一致"));
        }
        let frame=self.program.native_frame().map(|offset|
            definitions.map(|definitions|(definitions,offset as usize/4,uniforms.len(),buffers))
                .ok_or_else(||error("CPU范围映像缺少实际资源定义目录"))).transpose()?;
        let mut pointers: Vec<_> = buffers
            .iter()
            .map(|d| BufferAddress { address: d.pointer, stride: d.stride as usize })
            .chain(textures.iter().map(|texture| BufferAddress { address: texture.pointer(), stride: 0 }))
            .collect();
        // 原参数及投影已接受；取得可写访问时撤回旧计数关系。
        // 后续失败可能已有写入，不将未完成调用发布为完整保证。
        writing();
        let mut errors = Vec::new();
        // Actual input facts move into this scope; declarations never create facts.
        let mut input_domains = DomainRelations::Owned(input_domains);
        for (stage_index, (stage, code)) in self.program.stages().iter().zip(&self.stages).enumerate() {
            crate::bindings::invalidate(&mut input_domains,&stage.write_regions,|binding|(binding,0));
            let count = if let Some(prefix) = &stage.prefix {
                unsafe {
                    buffers[prefix.binding]
                        .pointer.cast::<u8>()
                        .add(prefix.word as usize).cast::<u32>()
                        .read()
                }
            } else {
                buffers[stage.domain].count
            };
            match code {
                StageCode::Sort(code) => sort::run(self, code, &pointers, count, stage_index, &mut errors, &mut timings)?,
                StageCode::Element { main, continuation, result } => {
                    let result = *result;
                    uniforms[0] = count;
                    let grain = (count as usize).div_ceil(self.workers * 4).max(1);
                    let grain = if stage.has_tile_scope() {
                        grain.div_ceil(stage.workgroup_width() as usize) * stage.workgroup_width() as usize
                    } else {
                        grain
                    };
                    let range_count = (count as usize).div_ceil(grain);
                    let partial_words = range_count * result.words();
                    let mut storage = Vec::<u32>::new();
                    storage.try_reserve_exact(partial_words)?;
                    let partials = &mut storage.spare_capacity_mut()[..partial_words];
                    let mut ranges = RangeWork {
                        frame: frame.map(|(definitions, offset, words, inputs)| (definitions, offset, words, inputs, input_domains.share())),
                        entry: *main,
                        buffers: pointers.as_ptr(),
                        range_parameter_slot: self.program.buffers().len(),
                        uniforms: uniforms.as_ptr(),
                        count: count as usize,
                        grain,
                        next: AtomicUsize::new(0),
                        result,
                        partials: partials.as_mut_ptr().cast(),
                        errors: std::ptr::null_mut(),
                        range_parameters: std::ptr::null(),
                        range_parameter_words: result.words(),
                        stage: stage_index,
                        timing: None,
                    };
                    // On success every range has written the fields and metadata consumed below.
                    self.execute(&mut ranges, &mut errors, &mut timings)?;
                    if let blanguage::ir::Operation::Reduce(reduction) = &stage.operation {
                        for (scalar, offset) in &reduction.ty.components {
                            let total = partials
                                .chunks_exact(result.words())
                                .map(|p| unsafe { collective_bits(*scalar, p, *offset as usize) })
                                .fold(reduction.operator.identity(*scalar), |a, b| {
                                    collective_pair(*scalar, reduction.operator, a, b)
                                });
                            unsafe {
                                std::ptr::copy_nonoverlapping(total.to_ne_bytes().as_ptr(),
                                    buffers[reduction.fields.target].pointer.cast::<u8>().add((reduction.fields.target_word + offset) as usize),
                                    scalar.size() as usize);
                            }
                        }
                    }
                    if let Some(continuation) = continuation {
                        let next = match continuation {
                            Continuation::Scan {
                                prefix,
                                apply,
                                source,
                                target,
                            } => {
                                let summary = BufferAddress { address: partials.as_mut_ptr().cast(), stride: result.words() * 4 };
                                let failure = unsafe {
                                    ranges.invoke_serial(
                                        *prefix,
                                        &mut pointers,
                                        &mut [(*source, summary), (*target, summary)],
                                        &mut uniforms,
                                        range_count as u32,
                                        std::ptr::null_mut(),
                                        &mut timings,
                                    )
                                }?;
                                unsafe {crate::application::take_errors([failure])}?;
                                Some(apply)
                            }
                            Continuation::Apply(next) => {
                                if let blanguage::ir::Operation::CustomReduce(r) = &stage.operation {
                                    let k = r.ty.size.div_ceil(4) as usize;
                                    let summary = BufferAddress { address: partials.as_mut_ptr().cast(), stride: k * 4 };
                                    let failure = unsafe {
                                        // The tail writes the declared components only on its success path.
                                        let output = buffers[r.fields.target]
                                            .pointer
                                            .cast::<u8>()
                                            .add(r.fields.target_word as usize)
                                            .cast::<u32>();
                                        ranges.invoke_serial(
                                            *next,
                                            &mut pointers,
                                            &mut [(r.fields.source, summary)],
                                            &mut uniforms,
                                            range_count as u32,
                                            output,
                                            &mut timings,
                                        )
                                    }?;
                                    unsafe {crate::application::take_errors([failure])}?;
                                    None
                                } else {
                                    if let blanguage::ir::Operation::SegmentedReduce(reduction) =
                                        &stage.operation
                                    {
                                        let values = &reduction.values;
                                        let k = values.ty.size.div_ceil(4) as usize;
                                        let mut prefix: Vec<_> = values.ty.components.iter().map(|(scalar,_)| values.operator.identity(*scalar)).collect();
                                        let mut segments = 0u32;
                                        for total in partials.chunks_mut(reduction.summary_words()) {
                                            let reset = unsafe { total[k].assume_init() } != 0;
                                            let next_segments = segments + unsafe { total[k + 1].assume_init() };
                                            for (prefix, (scalar, offset)) in prefix.iter_mut().zip(&values.ty.components) {
                                                let word = *offset as usize;
                                                let bits = unsafe { collective_bits(*scalar, total, word) };
                                                let next = if reset {
                                                    bits
                                                } else {
                                                    collective_pair(*scalar, values.operator, *prefix, bits)
                                                };
                                                unsafe { store_collective_bits(*scalar, total, word, *prefix) };
                                                *prefix = next;
                                            }
                                            total[k].write(count);
                                            total[k + 1].write(segments);
                                            segments = next_segments;
                                        }
                                        unsafe {
                                            buffers[reduction.count.binding]
                                                .pointer.cast::<u8>()
                                                .add(reduction.count.word as usize).cast::<u32>()
                                                .write(segments);
                                        }
                                    } else if let blanguage::ir::Operation::ScanExclusive(scan) =
                                        &stage.operation
                                    {
                                        let mut prefix: Vec<_> = scan.ty.components.iter().map(|(scalar,_)| scan.operator.identity(*scalar)).collect();
                                        let k = scan.ty.size.div_ceil(4) as usize;
                                        for total in partials.chunks_mut(scan.summary_words()) {
                                            let reset = scan.head_word.is_some() && unsafe { total[k].assume_init() } != 0;
                                            for (prefix, (scalar, offset)) in prefix.iter_mut().zip(&scan.ty.components) {
                                                let word = *offset as usize;
                                                let bits = unsafe { collective_bits(*scalar, total, word) };
                                                let next = if reset {
                                                    bits
                                                } else {
                                                    collective_pair(*scalar, scan.operator, *prefix, bits)
                                                };
                                                unsafe { store_collective_bits(*scalar, total, word, *prefix) };
                                                *prefix = next;
                                            }
                                        }
                                    } else {
                                        let mut prefix = 0u32;
                                        for total in partials.chunks_mut(result.words()) {
                                            let next = prefix.wrapping_add(unsafe { total[0].assume_init() });
                                            total[0].write(prefix);
                                            prefix = next;
                                        }
                                        if let blanguage::ir::Operation::Compact(c) = &stage.operation {
                                            unsafe {
                                                buffers[c.count]
                                                    .pointer.cast::<u8>()
                                                    .add(c.count_word as usize).cast::<u32>()
                                                    .write(prefix);
                                            }
                                        }
                                    }
                                    Some(next)
                                }
                            }
                        };
                        if let Some(next) = next {
                            ranges.entry = *next;
                            ranges.buffers = pointers.as_ptr();
                            ranges.uniforms = uniforms.as_ptr();
                            ranges.result = RangeResult::None;
                            ranges.next = AtomicUsize::new(0);
                            ranges.range_parameters = partials.as_ptr().cast();
                            self.execute(&mut ranges, &mut errors, &mut timings)?;
                        }
                    }
                }
            }
            if frame.is_some() {
                input_domains.extend(stage.prefix_guarantees.iter().map(|relation|
                    crate::DomainRelation::Prefix { domain:relation.domain,offset:relation.domain_region.unwrap_or((0,buffers[relation.domain].ty.size)).0,
                        bytes:relation.domain_region.unwrap_or((0,buffers[relation.domain].ty.size)).1,count:relation.count }));
                for range in crate::bindings::produced_ranges(&stage.element_guarantees,
                    |binding| (binding, 0), |target| buffers[target].count) {
                    crate::bindings::produced_element(&mut input_domains, range);
                }
            }
        }
        Ok(())
    }
    fn execute(
        &self,
        ranges: &mut RangeWork<'_>,
        errors: &mut Vec<*mut c_void>,
        timings: &mut Option<&mut CpuRangeTimings>,
    ) -> Result<()> {
        let range_count = ranges.count.div_ceil(ranges.grain);
        if range_count == 0 {
            return Ok(());
        }
        errors.try_reserve_exact(range_count)?;
        let slots = &mut errors.spare_capacity_mut()[..range_count];
        unsafe {
            observation::capture(timings, ranges.stage, ranges.entry as usize, range_count, |timing| {
                let submissions = self.workers.min(range_count);
                let work = std::ptr::from_mut(ranges);
                let handle = WorkHandle::new(execute_ranges, work.cast())?;
                (*work).errors = slots.as_mut_ptr().cast();
                (*work).timing = timing;
                for _ in 0..submissions {
                    handle.submit();
                }
                drop(handle);
                (*work).timing = None;
                Ok(())
            })?;
            // The callbacks have joined and each range has written its slot. The backing
            // vector keeps length zero so the next pass borrows only the capacity again.
            crate::application::take_errors(slots.iter().map(|slot| slot.assume_init()))
        }
    }
}
