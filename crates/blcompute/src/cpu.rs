mod dataset;
mod sort;
use crate::{Dataset, DomainBindings, Result, Value, arguments, error};
use blanguage::{
    ExecutableProgram, TemplateProgram,
    ir::{CollectiveOperator, ExecutionPlan as Program, Scalar, Ty},
    native::{NativeIntrinsic, NativeSymbol, RangeResult},
};
pub use dataset::CpuDataset;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn VirtualAlloc(address: *mut c_void, size: usize, kind: u32, protect: u32) -> *mut c_void;
    fn VirtualProtect(address: *mut c_void, size: usize, protect: u32, old: *mut u32) -> i32;
    fn VirtualFree(address: *mut c_void, size: usize, kind: u32) -> i32;
    fn FlushInstructionCache(process: *mut c_void, address: *const c_void, size: usize) -> i32;
    fn GetCurrentProcess() -> *mut c_void;
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
type Entry = unsafe extern "C" fn(*const *mut u32, *const u32, u32, u32);
type UnsignedEntry = unsafe extern "C" fn(*const *mut u32, *const u32, u32, u32) -> u32;
type ValuesEntry = unsafe extern "C" fn(*const *mut u32, *const u32, u32, u32, *mut u32);
struct RangeWork {
    entry: *mut c_void,
    buffers: *const *mut u32,
    // 范围摘要替换数据集后的私有槽；普通 map 的完整表在其后还可包含纹理。
    range_parameter_slot: usize,
    uniforms: *const u32,
    count: usize,
    grain: usize,
    next: AtomicUsize,
    result: RangeResult,
    partials: *mut u32,
    range_parameters: *const u32,
    range_parameter_words: usize,
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
        pointers.push(std::ptr::null_mut());
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
                private_buffers[work.range_parameter_slot] = work
                    .range_parameters
                    .add(begin / work.grain * work.range_parameter_words)
                    .cast_mut();
                private_buffers.as_ptr()
            };
            match work.result {
                RangeResult::Values(words) => {
                    let entry: ValuesEntry = std::mem::transmute(work.entry);
                    entry(
                        buffers,
                        work.uniforms,
                        end as u32,
                        begin as u32,
                        work.partials.add(begin / work.grain * words),
                    );
                }
                RangeResult::UnsignedSum => {
                    let entry: UnsignedEntry = std::mem::transmute(work.entry);
                    *work.partials.add(begin / work.grain) =
                        entry(buffers, work.uniforms, end as u32, begin as u32);
                }
                RangeResult::None => {
                    let entry: Entry = std::mem::transmute(work.entry);
                    entry(buffers, work.uniforms, end as u32, begin as u32);
                }
            }
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
struct Executable {
    address: *mut c_void,
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
extern "C" fn print_f32(value: u32) -> u32 {
    let mut text = [0u8; 32];
    let value = f32::from_bits(value) as f64;
    let count = unsafe {
        let locale = _create_locale(4, c"C".as_ptr());
        if locale.is_null() {
            return 0;
        }
        let count = __stdio_common_vsprintf(
            2,
            text.as_mut_ptr().cast(),
            text.len(),
            c"%.9g\n".as_ptr(),
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
    fn new(mut code: blanguage::native::NativeCode) -> Result<Self> {
        let mut libraries = std::collections::BTreeMap::new();
        for r in &code.relocations {
            let ptr = match &r.target {
                NativeSymbol::Function(name) => {
                    return Err(error(format!("范围映像缺少所属模块函数 {name}")));
                }
                NativeSymbol::External(name) => {
                    if !libraries.contains_key(&name.library) {
                        libraries.insert(
                            name.library.clone(),
                            Library::load(&name.library)
                                .map_err(|e| error(format!("加载 {} 失败：{e}", name.library)))?,
                        );
                    }
                    libraries[&name.library].symbol(&name.symbol).map_err(|e| {
                        error(format!("解析 {}!{} 失败：{e}", name.library, name.symbol))
                    })?
                }
                NativeSymbol::Intrinsic(intrinsic) => match intrinsic {
                    NativeIntrinsic::Sin => sin as *const (),
                    NativeIntrinsic::Cos => cos as *const (),
                    NativeIntrinsic::Floor => floor as *const (),
                    NativeIntrinsic::Exp => exp as *const (),
                    NativeIntrinsic::Exp2 => exp2 as *const (),
                    NativeIntrinsic::Log => log as *const (),
                    NativeIntrinsic::Log2 => log2 as *const (),
                    NativeIntrinsic::Pow => pow as *const (),
                    NativeIntrinsic::PrintU32 => print_u32 as *const (),
                    NativeIntrinsic::PrintI32 => print_i32 as *const (),
                    NativeIntrinsic::PrintBool => print_bool as *const (),
                    NativeIntrinsic::PrintF32 => print_f32 as *const (),
                },
            };
            code.bytes[r.offset..r.offset + 8].copy_from_slice(&(ptr as u64).to_le_bytes());
        }
        let address = unsafe { VirtualAlloc(std::ptr::null_mut(), code.bytes.len(), 0x3000, 0x04) };
        if address.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let out = Self {
            address,
            _libraries: libraries.into_values().collect(),
        };
        unsafe {
            std::ptr::copy_nonoverlapping(code.bytes.as_ptr(), address.cast(), code.bytes.len());
            let mut old = 0;
            if VirtualProtect(address, code.bytes.len(), 0x20, &mut old) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            if FlushInstructionCache(GetCurrentProcess(), address, code.bytes.len()) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(out)
    }
}
impl Drop for Executable {
    fn drop(&mut self) {
        unsafe {
            VirtualFree(self.address, 0, 0x8000);
        }
    }
}
extern "C" fn sin(x: f32) -> f32 {
    x.sin()
}
extern "C" fn cos(x: f32) -> f32 {
    x.cos()
}
extern "C" fn floor(x: f32) -> f32 {
    x.floor()
}
extern "C" fn exp(x: f32) -> f32 {
    x.exp()
}
extern "C" fn exp2(x: f32) -> f32 {
    x.exp2()
}
extern "C" fn log(x: f32) -> f32 {
    x.ln()
}
extern "C" fn log2(x: f32) -> f32 {
    x.log2()
}
extern "C" fn pow(x: f32, y: f32) -> f32 {
    x.powf(y)
}

fn collective_pair(scalar: Scalar, operator: CollectiveOperator, a: u32, b: u32) -> u32 {
    match (scalar, operator) {
        (_, CollectiveOperator::BitAnd) => a & b,
        (_, CollectiveOperator::BitOr) => a | b,
        (_, CollectiveOperator::BitXor) => a ^ b,
        (Scalar::F32, CollectiveOperator::Sum) => (f32::from_bits(a) + f32::from_bits(b)).to_bits(),
        (Scalar::F32, CollectiveOperator::Min) => {
            if f32::from_bits(a) < f32::from_bits(b) {
                a
            } else {
                b
            }
        }
        (Scalar::F32, CollectiveOperator::Max) => {
            if f32::from_bits(a) > f32::from_bits(b) {
                a
            } else {
                b
            }
        }
        (_, CollectiveOperator::Sum) => a.wrapping_add(b),
        (Scalar::I32, CollectiveOperator::Min) => (a as i32).min(b as i32) as u32,
        (Scalar::I32, CollectiveOperator::Max) => (a as i32).max(b as i32) as u32,
        (_, CollectiveOperator::Min) => a.min(b),
        (_, CollectiveOperator::Max) => a.max(b),
    }
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
    stages: Vec<StageCode>,
    workers: usize,
}
pub struct Bindings<D = Vec<Dataset>> {
    pub datasets: D,
    pub textures: Vec<crate::Texture2D>,
}
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
    /// 借用实际执行程序的唯一数据布局，供原生消费者创建绑定。
    pub fn program(&self) -> &Program {
        &self.code.program
    }
    pub fn new(program: ExecutableProgram) -> Result<Self> {
        Self::from_image(blanguage::native::compile_program(program).map_err(error)?)
    }
    pub fn from_image(image: blanguage::native::ProgramImage<ExecutableProgram>) -> Result<Self> {
        Ok(Self {
            code: CpuCode::from_image(image)?,
        })
    }
    pub fn run(&self, datasets: &mut [Dataset], values: &[Value]) -> Result<()> {
        self.code.run(datasets, &[], values)
    }
    pub fn run_owned(
        &self,
        bindings: impl Into<Bindings<DomainBindings<CpuDataset>>>,
        values: &[Value],
    ) -> Result<DomainBindings<CpuDataset>> {
        let Bindings { datasets, textures } = bindings.into();
        self.code.run_owned(datasets, &textures, values)
    }
}
impl CpuTemplate {
    pub fn program(&self) -> &Program {
        &self.code.program
    }
    pub fn new(program: TemplateProgram) -> Result<Self> {
        Self::from_image(blanguage::native::compile_program(program).map_err(error)?)
    }
    pub fn from_image(image: blanguage::native::ProgramImage<TemplateProgram>) -> Result<Self> {
        Ok(Self {
            code: CpuCode::from_image(image)?,
        })
    }
    pub fn run(
        &self,
        bindings: impl Into<Bindings<DomainBindings<CpuDataset>>>,
        values: &[Value],
    ) -> Result<DomainBindings<CpuDataset>> {
        let Bindings { datasets, textures } = bindings.into();
        self.code.run_owned(datasets, &textures, values)
    }
    pub(crate) fn run_projected(
        &self,
        bindings: DomainBindings<CpuDataset>,
        order: &[usize],
        values: &[Value],
    ) -> Result<DomainBindings<CpuDataset>> {
        self.code
            .run_owned_mapped(bindings, &[], values, order.len(), |i| order[i])
    }
}
#[derive(Clone, Copy)]
struct CpuBuffer<'a> {
    ty: &'a Ty,
    stride: u32,
    count: u32,
    pointer: *mut u32,
}
impl<'a> CpuBuffer<'a> {
    fn owned(data: &'a mut Dataset) -> Result<Self> {
        data.require_views()?;
        Ok(Self {
            ty: &data.ty,
            stride: data.stride,
            count: data.count,
            pointer: data.words.as_mut_ptr(),
        })
    }
    fn read(data: &'a Dataset) -> Result<Self> {
        data.require_views()?;
        Ok(Self {
            ty: &data.ty,
            stride: data.stride,
            count: data.count,
            pointer: data.words.as_ptr().cast_mut(),
        })
    }
}
impl CpuCode {
    fn from_image<P>(image: blanguage::native::ProgramImage<P>) -> Result<Self> {
        use blanguage::native::{ContinuationCode, StageCode as ImageStage};
        let (program, images) = image.into_parts();
        let workers = std::thread::available_parallelism()?.get();
        let stages = images
            .into_iter()
            .map(|image| {
                Ok(match image {
                    ImageStage::Sort(code) => StageCode::Sort(sort::Code::load(code)?),
                    ImageStage::Element {
                        main,
                        continuation,
                        result,
                    } => StageCode::Element {
                        main: Executable::new(main)?,
                        continuation: continuation
                            .map(|continuation| -> Result<Continuation> {
                                Ok(match continuation {
                                    ContinuationCode::ReductionTail(code)
                                    | ContinuationCode::ScanAdd(code)
                                    | ContinuationCode::CompactScatter(code)
                                    | ContinuationCode::SegmentedFinish(code) => {
                                        Continuation::Apply(Executable::new(code)?)
                                    }
                                    ContinuationCode::Scan {
                                        prefix,
                                        apply,
                                        source,
                                        target,
                                    } => Continuation::Scan {
                                        prefix: Executable::new(prefix)?,
                                        apply: Executable::new(apply)?,
                                        source,
                                        target,
                                    },
                                })
                            })
                            .transpose()?,
                        result,
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            program,
            stages,
            workers,
        })
    }
    fn run_owned(
        &self,
        bindings: DomainBindings<CpuDataset>,
        textures: &[crate::Texture2D],
        values: &[Value],
    ) -> Result<DomainBindings<CpuDataset>> {
        let count = bindings.datasets.len();
        self.run_owned_mapped(bindings, textures, values, count, |i| i)
    }
    fn run_owned_mapped(
        &self,
        mut bindings: DomainBindings<CpuDataset>,
        textures: &[crate::Texture2D],
        values: &[Value],
        count: usize,
        physical: impl Fn(usize) -> usize + Copy,
    ) -> Result<DomainBindings<CpuDataset>> {
        bindings.require(&self.program, physical)?;
        for (data, binding) in (0..count)
            .map(|i| &bindings.datasets[physical(i)])
            .zip(self.program.buffers())
        {
            if matches!(data, CpuDataset::Read(_)) && binding.access != blanguage::ast::Access::Read
            {
                return Err(error("冻结的 CPU 存储只能绑定 read dataset"));
            }
        }
        let buffers = bindings
            .datasets
            .iter_mut()
            .map(|data| match data {
                CpuDataset::Owned(data) => CpuBuffer::owned(data),
                CpuDataset::Read(data) => CpuBuffer::read(data),
            })
            .collect::<Result<Vec<_>>>()?;
        // 只复制原指针描述符，不重复借用或复制实际数据。
        let arguments: Vec<_> = (0..count).map(|i| buffers[physical(i)]).collect();
        self.run_buffers(&arguments, textures, values)?;
        bindings.completed(&self.program, physical);
        Ok(bindings)
    }
    fn run(
        &self,
        datasets: &mut [Dataset],
        textures: &[crate::Texture2D],
        values: &[Value],
    ) -> Result<()> {
        let buffers = datasets
            .iter_mut()
            .map(CpuBuffer::owned)
            .collect::<Result<Vec<_>>>()?;
        self.run_buffers(&buffers, textures, values)
    }
    fn run_buffers(
        &self,
        buffers: &[CpuBuffer<'_>],
        textures: &[crate::Texture2D],
        values: &[Value],
    ) -> Result<()> {
        if textures.len() != self.program.texture_count() {
            return Err(error("纹理绑定数量与程序接口不一致"));
        }
        let texture_descriptors = crate::texture::CpuTextures::new(textures);
        let mut uniforms = arguments(
            &self.program,
            buffers.iter().map(|d| (d.ty, d.stride, d.count)),
            values,
        )?;
        let pointers: Vec<_> = buffers
            .iter()
            .map(|d| d.pointer)
            .chain(texture_descriptors.pointers())
            .collect();
        for (stage, code) in self.program.stages().iter().zip(&self.stages) {
            let count = if let Some(prefix) = &stage.prefix {
                unsafe {
                    buffers[prefix.binding]
                        .pointer
                        .add(prefix.word as usize)
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
                entry: executable.address,
                buffers: pointers.as_ptr(),
                range_parameter_slot: self.program.buffers().len(),
                uniforms: uniforms.as_ptr(),
                count: count as usize,
                grain,
                next: AtomicUsize::new(0),
                result,
                partials: partials.as_mut_ptr(),
                range_parameters: std::ptr::null(),
                range_parameter_words: result.words().max(1),
            };
            self.execute(&mut ranges)?;
            if let blanguage::ir::Operation::Reduce(reduction) = &stage.operation {
                let scalar = reduction.ty.components[0].0;
                for (component, (_, offset)) in reduction.ty.components.iter().enumerate() {
                    let total = partials
                        .chunks_exact(reduction.ty.components.len())
                        .map(|p| p[component])
                        .fold(reduction.operator.identity(scalar), |a, b| {
                            collective_pair(scalar, reduction.operator, a, b)
                        });
                    unsafe {
                        buffers[reduction.fields.target]
                            .pointer
                            .add((reduction.fields.target_word + offset / 4) as usize)
                            .write(total);
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
                        inputs[*source] = partials.as_mut_ptr();
                        inputs[*target] = partials.as_mut_ptr();
                        let entry: Entry = unsafe { std::mem::transmute(prefix.address) };
                        unsafe {
                            entry(
                                inputs.as_ptr(),
                                uniforms.as_ptr(),
                                (partials.len() / result.words()) as u32,
                                0,
                            );
                        }
                        apply
                    }
                    Continuation::Apply(next) => {
                        if let blanguage::ir::Operation::CustomReduce(r) = &stage.operation {
                            let k = r.ty.components.len();
                            let mut inputs = pointers.clone();
                            inputs[r.fields.source] = partials.as_mut_ptr();
                            let mut output = vec![0; k];
                            let entry: ValuesEntry = unsafe { std::mem::transmute(next.address) };
                            unsafe {
                                entry(
                                    inputs.as_ptr(),
                                    uniforms.as_ptr(),
                                    (partials.len() / k) as u32,
                                    0,
                                    output.as_mut_ptr(),
                                );
                            }
                            for (value, (_, offset)) in output.into_iter().zip(&r.ty.components) {
                                unsafe {
                                    buffers[r.fields.target]
                                        .pointer
                                        .add((r.fields.target_word + offset / 4) as usize)
                                        .write(value);
                                }
                            }
                            continue;
                        } else if let blanguage::ir::Operation::SegmentedReduce(reduction) =
                            &stage.operation
                        {
                            let values = &reduction.values;
                            let k = values.ty.components.len();
                            let scalar = values.ty.components[0].0;
                            let mut prefix = vec![values.operator.identity(scalar); k];
                            let mut segments = 0u32;
                            for total in partials.chunks_mut(reduction.summary_words()) {
                                let reset = total[k] != 0;
                                let next_segments = segments + total[k + 1];
                                for (prefix, total) in prefix.iter_mut().zip(total.iter_mut()) {
                                    let next = if reset {
                                        *total
                                    } else {
                                        collective_pair(scalar, values.operator, *prefix, *total)
                                    };
                                    *total = *prefix;
                                    *prefix = next;
                                }
                                total[k] = count;
                                total[k + 1] = segments;
                                segments = next_segments;
                            }
                            unsafe {
                                buffers[reduction.count.binding]
                                    .pointer
                                    .add(reduction.count.word as usize)
                                    .write(segments);
                            }
                        } else if let blanguage::ir::Operation::ScanExclusive(scan) =
                            &stage.operation
                        {
                            let scalar = scan.ty.components[0].0;
                            let mut prefix =
                                vec![scan.operator.identity(scalar); scan.ty.components.len()];
                            for total in partials.chunks_mut(scan.summary_words()) {
                                let reset = scan.head_word.is_some() && total[prefix.len()] != 0;
                                for (prefix, total) in prefix.iter_mut().zip(total.iter_mut()) {
                                    let next = if reset {
                                        *total
                                    } else {
                                        collective_pair(scalar, scan.operator, *prefix, *total)
                                    };
                                    *total = *prefix;
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
                                        .pointer
                                        .add(c.count_word as usize)
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
    fn execute(&self, ranges: &mut RangeWork) -> Result<()> {
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
        Ok(())
    }
}
