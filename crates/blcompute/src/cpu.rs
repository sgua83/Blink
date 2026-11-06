pub use crate::texture::TextureInput;
mod dataset;
mod sort;
use crate::{Dataset, DomainBindings, Result, Value, arguments, error};
use blanguage::{
    ExecutableProgram, TemplateProgram,
    ir::{Bits, CollectiveOperator, ExecutionPlan as Program, Scalar, Ty},
    native::{BufferAddress, NativeSymbol, RangeEntry, RangeResult},
};
pub use dataset::CpuDataset;
use std::ffi::c_void;
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
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const std::ffi::c_char) -> *const c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
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
    frame: Option<(&'a std::sync::Arc<crate::application::Definitions>,usize,usize,&'a [CpuBuffer<'a>])>,
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
}
impl RangeWork<'_> {
    unsafe fn invoke(&self,address:*mut c_void,buffers:*const BufferAddress,end:u32,begin:u32,output:*mut u32)->*mut c_void {
        let mut local=self.frame.map(|(definitions,offset,words,inputs)|{
            let frame=unsafe {crate::application::Frame::from_definitions(definitions.clone(),inputs)};
            let values=unsafe {std::slice::from_raw_parts(self.uniforms,words)}.to_vec();
            (frame,values,offset)
        });
        let uniforms=match &mut local {
            Some((frame,values,offset))=>{
                let pointer=frame as *mut crate::application::Frame as usize as u64;
                values[*offset]=pointer as u32;values[*offset+1]=(pointer>>32)as u32;
                values.as_ptr()
            },
            None=>self.uniforms,
        };
        let entry:RangeEntry=unsafe {std::mem::transmute(address)};
        unsafe {entry(buffers,uniforms,end,begin,output)}
    }
}
unsafe extern "system" fn execute_ranges(
    _instance: *mut c_void,
    context: *mut c_void,
    _work: *mut c_void,
) {
    let work = unsafe { &*context.cast::<RangeWork>() };
    let mut private_buffers = if work.range_parameters.is_null() {
        Vec::new()
    } else {
        let mut pointers =
            unsafe { std::slice::from_raw_parts(work.buffers, work.range_parameter_slot).to_vec() };
        pointers.push(BufferAddress { address: std::ptr::null_mut(), stride: work.range_parameter_words * 4 });
        pointers
    };
    loop {
        let begin = work.next.fetch_add(work.grain, Ordering::Relaxed);
        if begin >= work.count {
            break;
        }
        let end = (begin + work.grain).min(work.count);
        unsafe {
            let buffers = if work.range_parameters.is_null() {
                work.buffers
            } else {
                private_buffers[work.range_parameter_slot].address = work
                    .range_parameters
                    .add(begin / work.grain * work.range_parameter_words)
                    .cast_mut();
                private_buffers.as_ptr()
            };
            let words = work.result.words();
            let output = if words == 0 {
                std::ptr::null_mut()
            } else {
                work.partials.add(begin / work.grain * words)
            };
            *work.errors.add(begin / work.grain) =
                work.invoke(work.entry,buffers,end as u32,begin as u32,output);
        }
    }
}
struct WorkHandle(*mut c_void);
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
    /// 原Executable是否持有Allocation；外部已链接模块的代码为false。
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
struct Executable {
    address: *mut c_void,
    _allocation: Option<std::sync::Arc<Allocation>>,
}
struct Allocation {
    address: *mut c_void,
    functions: Option<std::ptr::NonNull<RuntimeFunction>>,
    _libraries: Vec<Library>,
}
struct Library(*mut c_void);
impl Library {
    fn load(name: &str) -> Result<Self> {
        let name: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
        let handle = unsafe { LoadLibraryW(name.as_ptr()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(handle))
    }
    fn symbol(&self, name: &str) -> Result<*const ()> {
        let name = std::ffi::CString::new(name)?;
        let address = unsafe { GetProcAddress(self.0, name.as_ptr()) };
        if address.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(address.cast())
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
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
impl Executable {
    fn memory(&self, stage: usize) -> Result<CpuCodeMemory> {
        let mut information = std::mem::MaybeUninit::<MemoryBasicInformation>::zeroed();
        if unsafe { VirtualQuery(self.address, information.as_mut_ptr(), std::mem::size_of::<MemoryBasicInformation>()) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let information = unsafe { information.assume_init() };
        Ok(CpuCodeMemory {
            stage,
            entry_address: self.address as usize,
            runtime_owned: self._allocation.is_some(),
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
    fn new<P>(image: &blanguage::native::ProgramImage<P>) -> Result<(Option<std::sync::Arc<Self>>, std::collections::BTreeMap<usize, *mut c_void>)> {
        let codes = image.code().collect::<Vec<_>>();
        if codes.is_empty() { return Ok((None, std::collections::BTreeMap::new())); }
        let mut size = 0usize;
        let mut layout = Vec::with_capacity(codes.len());
        for code in &codes {
            let begin = u32::try_from(size.div_ceil(16) * 16)?;
            let unwind = u32::try_from((begin as usize + code.bytes.len()).div_ceil(4) * 4)?;
            size = unwind as usize + code.unwind.len();
            layout.push((begin, unwind));
        }
        let table_offset = size.div_ceil(4) * 4;
        let count = u32::try_from(codes.len())?;
        size = table_offset + codes.len() * std::mem::size_of::<RuntimeFunction>();
        let address = unsafe { VirtualAlloc(std::ptr::null_mut(), size, 0x3000, 0x04) };
        if address.is_null() { return Err(std::io::Error::last_os_error().into()); }
        let mut allocation = Self { address, functions: None, _libraries: Vec::new() };
        let table = unsafe { address.cast::<u8>().add(table_offset).cast::<RuntimeFunction>() };
        let mut addresses = std::collections::BTreeMap::new();
        for (index, (code, &(begin, unwind))) in codes.iter().zip(&layout).enumerate() {
            let entry = unsafe { address.cast::<u8>().add(begin as usize) };
            addresses.insert(*code as *const _ as usize, entry.cast::<c_void>());
            unsafe {
                std::ptr::copy_nonoverlapping(code.bytes.as_ptr(), entry, code.bytes.len());
                std::ptr::copy_nonoverlapping(code.unwind.as_ptr(), address.cast::<u8>().add(unwind as usize), code.unwind.len());
                std::ptr::write(table.add(index), RuntimeFunction { begin, end: begin + u32::try_from(code.function_end)?, unwind });
            }
        }
        let functions = image.functions().iter().map(|(name, code)|
            (name.as_str(), addresses[&(code as *const _ as usize)])).collect::<std::collections::BTreeMap<_, _>>();
        let mut libraries = std::collections::BTreeMap::new();
        for code in codes {
            let entry = addresses[&(code as *const _ as usize)];
            for relocation in &code.relocations {
                let ptr = match &relocation.target {
                    NativeSymbol::Function(name) => *functions.get(name.as_str())
                        .ok_or_else(|| error(format!("范围映像缺少所属模块函数 {name}")))? as *const (),
                    NativeSymbol::External(name) if name.library==blanguage::resource::RUNTIME_LIBRARY =>
                        crate::application::resource_symbol(&name.symbol)
                            .ok_or_else(||error(format!("未知的Blink资源C符号 {}",name.symbol)))?,
                    NativeSymbol::External(name) => {
                        if !libraries.contains_key(&name.library) {
                            libraries.insert(name.library.clone(), Library::load(&name.library)
                                .map_err(|error| crate::error(format!("加载 {} 失败：{error}", name.library)))?);
                        }
                        libraries[&name.library].symbol(&name.symbol).map_err(|error|
                            crate::error(format!("解析 {}!{} 失败：{error}", name.library, name.symbol)))?
                    }
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
                    std::ptr::copy_nonoverlapping((ptr as u64).to_le_bytes().as_ptr(), entry.cast::<u8>().add(relocation.offset), 8);
                }
            }
        }
        allocation._libraries = libraries.into_values().collect();
        unsafe {
            let mut old = 0;
            if VirtualProtect(address, size, 0x20, &mut old) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            if FlushInstructionCache(GetCurrentProcess(), address, size) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        if unsafe { RtlAddFunctionTable(table, count, address as u64) } == 0 {
            return Err(error("Windows未接受CPU代码的原生展开信息"));
        }
        allocation.functions = Some(unsafe { std::ptr::NonNull::new_unchecked(table) });
        Ok((Some(std::sync::Arc::new(allocation)), addresses))
    }
}
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            if let Some(table) = self.functions { RtlDeleteFunctionTable(table.as_ptr()); }
            VirtualFree(self.address, 0, 0x8000);
        }
    }
}
fn collective_bits(scalar: Scalar, words: &[u32], byte: usize) -> Bits {
    let mut bytes = [0; 8];
    unsafe { std::ptr::copy_nonoverlapping(words.as_ptr().cast::<u8>().add(byte), bytes.as_mut_ptr(), scalar.size() as usize); }
    Bits::from_ne_bytes(bytes)
}
fn store_collective_bits(scalar: Scalar, words: &mut [u32], byte: usize, bits: Bits) {
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
    Apply(Executable),
    Scan {
        prefix: Executable,
        apply: Executable,
        source: usize,
        target: usize,
    },
}
enum StageCode {
    Element {
        main: Executable,
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
        let workers = std::thread::available_parallelism()?.get();
        let stages = CpuCode::load_stages(image.stages(), &|address| Executable {
            address: address.cast_mut().cast(), _allocation: None,
        });
        let (program, _, _, _) = image.into_parts();
        let definitions = unsafe { crate::application::Definitions::linked(factories) }?;
        Ok(Self { code: CpuCode {
            program, stages, workers, definitions: Some(std::sync::Arc::new(definitions)),
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
        self.code.run_owned(bindings.into(), values)
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
    /// 装入失败交回原错误和同一未改映像；本调用没有执行数据。
    pub fn from_image(image: blanguage::native::ProgramImage<TemplateProgram>) -> std::result::Result<Self, (Box<dyn std::error::Error + Send + Sync>, blanguage::native::ProgramImage<TemplateProgram>)> {
        CpuCode::raw(image).map(|code| Self { code })
    }
    pub fn run(
        &self,
        bindings: impl Into<Bindings<DomainBindings<CpuDataset>>>,
        values: &[Value],
    ) -> OwnedRunResult {
        self.code.run_owned(bindings.into(), values)
    }
    pub(crate) fn run_projected(
        &self,
        bindings: &mut DomainBindings<crate::application::ResidentData>,
        order: &[(usize, u32)],
        values: &[Value],
        definitions: &std::sync::Arc<crate::application::Definitions>,
        writing: impl FnOnce(),
    ) -> Result<()> {
        self.code
            .run_mapped(bindings, &[], values, order.len(), |i| order[i], writing,Some(definitions))
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
    fn require_storage(data: &Dataset) -> Result<()> {
        if data.stride != blanguage::ir::align_up(data.ty.size, data.ty.align) {
            return Err(error("原生CPU绑定的物理记录布局与其类型不一致"));
        }
        data.require_views()
    }
    fn projected(self, expected: &blanguage::ir::Buffer, offset: u32) -> Result<Self> {
        let ty = self.ty.field_layout_at(offset, &expected.ty)
            .ok_or_else(|| error(format!("绑定 {} 的字段类型或偏移与原存储不一致", expected.name)))?;
        Ok(Self {
            ty,
            pointer: self.pointer.cast::<u8>().wrapping_add(offset as usize).cast(),
            writable:self.writable&&expected.access!=blanguage::ast::Access::Read,
            ..self
        })
    }
    fn owned(data: &'a mut Dataset) -> Result<Self> {
        Self::require_storage(data)?;
        Ok(Self {
            ty: &data.ty,
            stride: data.stride,
            count: data.count,
            pointer: data.words.as_mut_ptr(),
            writable:true,
        })
    }
    fn read(data: &'a Dataset) -> Result<Self> {
        Self::require_storage(data)?;
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
impl CpuStorage for crate::application::ResidentData {
    fn cpu_metadata(&self)->(&Ty,u32,bool){match self {
        Self::Cpu(data)=>data.cpu_metadata(),
        Self::CpuBorrow(data)=>(&data.ty,data.count,data.writable),
        Self::Gpu(data)=>(data.layout(),data.count(),false),
    }}
    fn cpu_buffer(&mut self)->Result<CpuBuffer<'_>>{match self {
        Self::Cpu(data)=>data.cpu_buffer(),
        Self::CpuBorrow(data)=>Ok(CpuBuffer{ty:&data.ty,stride:data.stride,count:data.count,pointer:data.pointer,writable:data.writable}),
        Self::Gpu(_)=>Err(error("CPU 访问需要先显式 download 数据")),
    }}
}
impl CpuCode {
    fn code_memory(&self) -> Result<Vec<CpuCodeMemory>> {
        let mut result = Vec::new();
        let mut add = |stage, code: &Executable| -> Result<()> {
            result.push(code.memory(stage)?);
            Ok(())
        };
        for (stage, code) in self.stages.iter().enumerate() {
            match code {
                StageCode::Element { main, continuation, .. } => {
                    add(stage, main)?;
                    match continuation {
                        Some(Continuation::Apply(code)) => add(stage, code)?,
                        Some(Continuation::Scan { prefix, apply, .. }) => {
                            add(stage, prefix)?;
                            add(stage, apply)?;
                        }
                        None => {}
                    }
                }
                StageCode::Sort(code) => {
                    for code in code.executables() {
                        add(stage, code)?;
                    }
                }
            }
        }
        Ok(result)
    }
    fn raw<P>(image: blanguage::native::ProgramImage<P>) -> std::result::Result<Self, (Box<dyn std::error::Error + Send + Sync>, blanguage::native::ProgramImage<P>)> {
        let workers=match std::thread::available_parallelism(){Ok(workers)=>workers.get(),Err(error)=>return Err((error.into(),image))};
        let (allocation, addresses) = match Allocation::new(&image) {
            Ok(code) => code,
            Err(error) => return Err((error, image)),
        };
        Ok(Self::from_image(image,workers,&|code|Executable{
            address:addresses[&(code as *const _ as usize)],_allocation:allocation.clone(),
        }))
    }
    fn linked<P>(image: blanguage::native::ProgramImage<P, *const u8>) -> std::result::Result<Self, (Box<dyn std::error::Error + Send + Sync>, blanguage::native::ProgramImage<P, *const u8>)> {
        let workers=match std::thread::available_parallelism(){Ok(workers)=>workers.get(),Err(error)=>return Err((error.into(),image))};
        Ok(Self::from_image(image,workers,&|address|Executable{address:address.cast_mut().cast(),_allocation:None}))
    }
    fn from_image<P,C>(image:blanguage::native::ProgramImage<P,C>,workers:usize,load:&impl Fn(&C)->Executable)->Self{
        let stages=Self::load_stages(image.stages(),load);
        let(program,_,_,resources)=image.into_parts();
        let definitions=resources.map(|module|{
            let programs=module.images.into_iter().map(|image|match image{
                blanguage::resource::Image::Cpu(image)=>crate::application::ProgramSlot::Cpu(CpuTemplate{code:Self::from_image(image,workers,load)}),
                blanguage::resource::Image::Gpu(image)=>crate::application::ProgramSlot::Gpu(std::sync::Arc::new(image)),
            }).collect();
            std::sync::Arc::new(crate::application::Definitions{types:module.types,programs})
        });
        Self{program,stages,workers,definitions}
    }
    fn load_stages<C>(stages:&[blanguage::native::StageCode<C>],load:&impl Fn(&C)->Executable)->Vec<StageCode>{
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
    ) -> OwnedRunResult {
        let count = bindings.datasets.datasets.len();
        match self.run_mapped(&mut bindings.datasets, &bindings.textures, values, count, |i| (i, 0), || {},self.definitions.as_ref()) {
            Ok(()) => Ok(bindings.datasets),
            Err(error) => Err((error, bindings)),
        }
    }
    fn run_mapped<D:CpuStorage>(
        &self,
        bindings: &mut DomainBindings<D>,
        textures: &[TextureInput],
        values: &[Value],
        count: usize,
        physical: impl Fn(usize) -> (usize, u32) + Copy,
        writing: impl FnOnce(),
        definitions:Option<&std::sync::Arc<crate::application::Definitions>>,
    ) -> Result<()> {
        let uniforms = arguments(&self.program, (0..count).map(|i| {
            let (binding, offset) = physical(i);
            let(ty,count,_)=bindings.datasets[binding].cpu_metadata();
            (ty,count,Some(offset))
        }), values)?;
        crate::bindings::require(&bindings.datasets, &bindings.domains, &self.program, physical)?;
        for (data, binding) in (0..count)
            .map(|i| &bindings.datasets[physical(i).0])
            .zip(self.program.buffers())
        {
            if !data.cpu_metadata().2 && binding.access != blanguage::ast::Access::Read
            {
                return Err(error("冻结的 CPU 存储只能绑定 read dataset"));
            }
        }
        let buffers = bindings
            .datasets
            .iter_mut()
            .map(CpuStorage::cpu_buffer)
            .collect::<Result<Vec<_>>>()?;
        // 只复制原指针描述符，不重复借用或复制实际数据。
        let arguments = (0..count).map(|i| {
            let (physical, offset) = physical(i);
            let expected = self.program.buffers().get(i)
                .ok_or_else(|| error("设备调用的数据集数量不匹配"))?;
            buffers[physical].projected(expected, offset)
        }).collect::<Result<Vec<_>>>()?;
        self.run_buffers(&arguments, textures, uniforms, || {
            crate::bindings::invalidate(&mut bindings.domains, &self.program, physical);
            writing();
        },definitions)?;
        bindings.produced(&self.program, physical);
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
        self.run_buffers(&buffers, textures, uniforms, || {},self.definitions.as_ref())
    }
    fn run_buffers(
        &self,
        buffers: &[CpuBuffer<'_>],
        textures: &[TextureInput],
        mut uniforms: Vec<u32>,
        writing: impl FnOnce(),
        definitions:Option<&std::sync::Arc<crate::application::Definitions>>,
    ) -> Result<()> {
        if textures.len() != self.program.texture_count()
            || textures.iter().zip(self.program.textures().iter().flat_map(|binding|
                std::iter::repeat_n(binding, binding.elements() as usize)))
                .any(|(texture, binding)| texture.dimension() != binding.dimension
                    || binding.format.is_some_and(|format| texture.format() != format)) {
            return Err(error("纹理绑定数量、维度或格式与程序接口不一致"));
        }
        let frame=self.program.native_frame().map(|offset|
            definitions.map(|definitions|(definitions,offset as usize/4,uniforms.len(),buffers))
                .ok_or_else(||error("CPU范围映像缺少实际资源定义目录"))).transpose()?;
        let texture_descriptors = crate::texture::CpuTextures::new(textures);
        let pointers: Vec<_> = buffers
            .iter()
            .map(|d| BufferAddress { address: d.pointer, stride: d.stride as usize })
            .chain(texture_descriptors.pointers().map(|address| BufferAddress { address, stride: 0 }))
            .collect();
        // 原参数及投影已接受；取得可写访问时撤回旧计数关系。
        // 后续失败可能已有写入，不将未完成调用发布为完整保证。
        writing();
        for (stage, code) in self.program.stages().iter().zip(&self.stages) {
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
            let (executable, continuation, result) = match code {
                StageCode::Sort(code) => {
                    sort::run(self, code, &pointers, count)?;
                    continue;
                }
                StageCode::Element {
                    main,
                    continuation,
                    result,
                } => (main, continuation, *result),
            };
            uniforms[0] = count;
            let grain = (count as usize).div_ceil(self.workers * 4).max(1);
            let grain = if stage.has_tile_scope() {
                grain.div_ceil(stage.workgroup_width() as usize) * stage.workgroup_width() as usize
            } else {
                grain
            };
            let mut partials = if matches!(result, RangeResult::None) {
                Vec::new()
            } else {
                vec![0u32; (count as usize).div_ceil(grain) * result.words()]
            };
            let mut ranges = RangeWork {
                frame,
                entry: executable.address,
                buffers: pointers.as_ptr(),
                range_parameter_slot: self.program.buffers().len(),
                uniforms: uniforms.as_ptr(),
                count: count as usize,
                grain,
                next: AtomicUsize::new(0),
                result,
                partials: partials.as_mut_ptr(),
                errors: std::ptr::null_mut(),
                range_parameters: std::ptr::null(),
                range_parameter_words: result.words().max(1),
            };
            self.execute(&mut ranges)?;
            if let blanguage::ir::Operation::Reduce(reduction) = &stage.operation {
                for (scalar, offset) in &reduction.ty.components {
                    let total = partials
                        .chunks_exact(result.words())
                        .map(|p| collective_bits(*scalar, p, *offset as usize))
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
                        let mut inputs = pointers.clone();
                        inputs[*source] = BufferAddress { address: partials.as_mut_ptr(), stride: result.words() * 4 };
                        inputs[*target] = inputs[*source];
                        let failure = unsafe {
                            ranges.invoke(prefix.address,
                                inputs.as_ptr(),
                                (partials.len() / result.words()) as u32,
                                0,
                                std::ptr::null_mut(),
                            )
                        };
                        unsafe {crate::application::take_errors([failure])}?;
                        apply
                    }
                    Continuation::Apply(next) => {
                        if let blanguage::ir::Operation::CustomReduce(r) = &stage.operation {
                            let k = r.ty.size.div_ceil(4) as usize;
                            let mut inputs = pointers.clone();
                            inputs[r.fields.source] = BufferAddress { address: partials.as_mut_ptr(), stride: k * 4 };
                            let mut output = vec![0; k];
                            let failure = unsafe {
                                ranges.invoke(next.address,
                                    inputs.as_ptr(),
                                    (partials.len() / k) as u32,
                                    0,
                                    output.as_mut_ptr(),
                                )
                            };
                            unsafe {crate::application::take_errors([failure])}?;
                            for (scalar, offset) in &r.ty.components {
                                unsafe {
                                    std::ptr::copy_nonoverlapping(
                                        output.as_ptr().cast::<u8>().add(*offset as usize),
                                        buffers[r.fields.target].pointer.cast::<u8>().add((r.fields.target_word + offset) as usize),
                                        scalar.size() as usize,
                                    );
                                }
                            }
                            continue;
                        } else if let blanguage::ir::Operation::SegmentedReduce(reduction) =
                            &stage.operation
                        {
                            let values = &reduction.values;
                            let k = values.ty.size.div_ceil(4) as usize;
                            let mut prefix: Vec<_> = values.ty.components.iter().map(|(scalar,_)| values.operator.identity(*scalar)).collect();
                            let mut segments = 0u32;
                            for total in partials.chunks_mut(reduction.summary_words()) {
                                let reset = total[k] != 0;
                                let next_segments = segments + total[k + 1];
                                for (prefix, (scalar, offset)) in prefix.iter_mut().zip(&values.ty.components) {
                                    let word = *offset as usize;
                                    let bits = collective_bits(*scalar, total, word);
                                    let next = if reset {
                                        bits
                                    } else {
                                        collective_pair(*scalar, values.operator, *prefix, bits)
                                    };
                                    store_collective_bits(*scalar, total, word, *prefix);
                                    *prefix = next;
                                }
                                total[k] = count;
                                total[k + 1] = segments;
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
                                let reset = scan.head_word.is_some() && total[k] != 0;
                                for (prefix, (scalar, offset)) in prefix.iter_mut().zip(&scan.ty.components) {
                                    let word = *offset as usize;
                                    let bits = collective_bits(*scalar, total, word);
                                    let next = if reset {
                                        bits
                                    } else {
                                        collective_pair(*scalar, scan.operator, *prefix, bits)
                                    };
                                    store_collective_bits(*scalar, total, word, *prefix);
                                    *prefix = next;
                                }
                            }
                        } else {
                            let mut prefix = 0u32;
                            for total in partials.chunks_mut(result.words()) {
                                let next = prefix.wrapping_add(total[0]);
                                total[0] = prefix;
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
                        next
                    }
                };
                ranges.entry = next.address;
                ranges.result = RangeResult::None;
                ranges.next = AtomicUsize::new(0);
                ranges.range_parameters = partials.as_ptr();
                self.execute(&mut ranges)?;
            }
        }
        Ok(())
    }
    fn execute(&self, ranges: &mut RangeWork<'_>) -> Result<()> {
        let mut errors = vec![std::ptr::null_mut(); ranges.count.div_ceil(ranges.grain)];
        ranges.errors = errors.as_mut_ptr();
        let work = unsafe {
            CreateThreadpoolWork(
                execute_ranges,
                (ranges as *mut RangeWork).cast(),
                std::ptr::null(),
            )
        };
        if work.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let handle = WorkHandle(work);
        for _ in 0..self.workers.min(ranges.count) {
            unsafe {
                SubmitThreadpoolWork(work);
            }
        }
        drop(handle);
        unsafe {crate::application::take_errors(errors)}
    }
}
