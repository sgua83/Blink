//! B 应用拥有动态数据；原 Dataset、域关系及 GPU 最后使用点仍拥有物理资源。
use crate::{
    Dataset, DomainBindings, Result, Value,
    cpu::{CpuDataset, CpuTemplate},
    error, vulkan,
};
use crate::bindings::DomainRelation;
use blanguage::{
    TemplateProgram,
    ir::{ExecutionPlan, PrefixCount, Ty},
};

mod abi;
pub use blanguage::resource::DatasetBinding;

pub enum ProgramImage {
    Cpu(blanguage::native::ProgramImage<TemplateProgram>),
    CpuLinked(blanguage::native::ProgramImage<TemplateProgram, *const u8>),
    Gpu(blanguage::spirv::ProgramImage<TemplateProgram>),
}
/// 当前已装入的原CPU／GPU程序；观察只借用，不复制代码拥有关系。
pub enum Program {
    Cpu(CpuTemplate),
    Gpu(vulkan::GpuTemplate),
}
/// 同一原程序槽的实际映像或已装入程序；不是执行／完成状态。
pub enum ProgramSlot {
    Image(ProgramImage),
    Loaded(Program),
}
impl ProgramSlot {
    /// 借用当前映像或已装入程序的原执行合同，不触发装入。
    pub fn layout(&self) -> &ExecutionPlan {
        match self {
            Self::Image(ProgramImage::Cpu(image)) => image.program(),
            Self::Image(ProgramImage::CpuLinked(image)) => image.program(),
            Self::Image(ProgramImage::Gpu(image)) => image.program(),
            Self::Loaded(program) => program.layout(),
        }
    }
    fn gpu_layout(&self) -> Option<&ExecutionPlan> {
        match self {
            Self::Image(ProgramImage::Gpu(image)) => Some(image.program()),
            Self::Loaded(Program::Gpu(program)) => Some(program.program()),
            _ => None,
        }
    }
}
impl Program {
    fn layout(&self) -> &ExecutionPlan {
        match self {
            Self::Cpu(p) => p.program(),
            Self::Gpu(p) => p.program(),
        }
    }
}
/// 当前实际驻留存储；Frame观察只借用该原值，不转移所有权。
pub enum ResidentData {
    Cpu(CpuDataset),
    Gpu(vulkan::GpuDataset),
}

/// 同一Frame可见CPU数据存储的同步快照；多个句柄只列一次物理Dataset。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameCpuDataMemory {
    pub handles: Vec<u32>,
    pub memory: crate::DatasetMemory,
}

/// 一个实际 B 应用调用帧的唯一数据所有者。句柄只由编译后的资源声明取得，
/// 源语言不提供从整数构造句柄或复制独占所有权的操作。
pub struct Frame {
    data: DomainBindings<Option<ResidentData>>,
    programs: Vec<Option<ProgramSlot>>,
    device: Option<vulkan::Device>,
    types: Vec<Ty>,
}
impl Frame {
    pub fn new(types: Vec<Ty>, images: Vec<ProgramImage>) -> Self {
        Self {
            data: Vec::new().into(),
            programs: images
                .into_iter()
                .map(|image| Some(ProgramSlot::Image(image)))
                .collect(),
            device: None,
            types,
        }
    }
    /// 借用当前原数据根；槽索引为本Frame实际句柄，None保留已释放空槽。
    /// 不过滤、复制或保活另一份资源；活动借用期间不能改变Frame。
    pub fn data(&self) -> &DomainBindings<Option<ResidentData>> {
        &self.data
    }
    /// 借用同一原程序槽Vec，索引仍为实际程序号，capacity以Option槽数计。
    /// 槽容量不含内部映像／代码VM／设备分配；查询不装入或改变Frame。
    pub fn programs(&self) -> &Vec<Option<ProgramSlot>> {
        &self.programs
    }
    /// 借用原类型Vec，索引为本Frame的类型号，capacity以Ty槽数计。
    /// 槽容量不含Ty内部拥有分配或B数据载荷；查询不复制或改变Frame。
    pub fn types(&self) -> &Vec<Ty> {
        &self.types
    }
    /// 查询本Frame当前可见的CPU数据字存储；不等待或下载GPU数据。
    /// 结果只保存本次元数据，不保活资源；释放后的旧结果不代表当前存活状态。
    /// 不包含代码VM、view来源图、类型／句柄容器或OS堆管理开销。
    pub fn cpu_data_memory(&self) -> Vec<FrameCpuDataMemory> {
        let mut result: Vec<FrameCpuDataMemory> = Vec::new();
        let mut positions = std::collections::BTreeMap::new();
        for (handle, slot) in self.data.datasets.iter().enumerate() {
            if let Some(ResidentData::Cpu(data)) = slot {
                // 仅本次借用内的原对象地址；不把空Vec的悬垂指针当分配身份。
                let storage = &**data as *const Dataset;
                let index = *positions.entry(storage).or_insert_with(|| {
                    let index = result.len();
                    result.push(FrameCpuDataMemory { handles: Vec::new(), memory: data.memory() });
                    index
                });
                result[index].handles.push(handle as u32);
            }
        }
        result
    }
    fn load(&mut self, index: usize) -> Result<Program> {
        if self.device.is_none()
            && matches!(
                self.programs[index],
                Some(ProgramSlot::Image(ProgramImage::Gpu(_)))
            )
        {
            self.create_device()?;
        }
        match self.programs[index].take().unwrap() {
            ProgramSlot::Loaded(program) => Ok(program),
            ProgramSlot::Image(ProgramImage::Cpu(image)) => {
                match CpuTemplate::from_image(image) {
                    Ok(program) => Ok(Program::Cpu(program)),
                    Err((error, image)) => {
                        self.programs[index] = Some(ProgramSlot::Image(ProgramImage::Cpu(image)));
                        Err(error)
                    }
                }
            }
            ProgramSlot::Image(ProgramImage::CpuLinked(image)) => {
                match unsafe { CpuTemplate::from_linked(image) } {
                    Ok(program) => Ok(Program::Cpu(program)),
                    Err((error, image)) => {
                        self.programs[index] = Some(ProgramSlot::Image(ProgramImage::CpuLinked(image)));
                        Err(error)
                    }
                }
            }
            ProgramSlot::Image(ProgramImage::Gpu(image)) => {
                match self.device.as_ref().unwrap().load_template(image) {
                    Ok(program) => Ok(Program::Gpu(program)),
                    Err((error, image)) => {
                        self.programs[index] = Some(ProgramSlot::Image(ProgramImage::Gpu(image)));
                        Err(error)
                    }
                }
            }
        }
    }
    fn create_device(&mut self) -> Result<()> {
        let programs = self
            .programs
            .iter()
            .filter_map(|slot| slot.as_ref().and_then(ProgramSlot::gpu_layout));
        self.device = Some(vulkan::Device::for_programs(programs)?);
        Ok(())
    }
    /// 上传借用原CPU输入；成功交回GPU存储后替换，原暂存区仍按实际完成释放。
    pub fn upload(&mut self, handle: u32) -> Result<()> {
        if self.device.is_none() {
            self.create_device()?;
        }
        let ResidentData::Cpu(data) = self.data.datasets[handle as usize].as_ref().unwrap() else {
            return Err(error("CPU 访问需要先显式 download 数据"));
        };
        let data = self.device.as_ref().unwrap().upload_cpu(data, &mut std::collections::BTreeMap::new())?;
        self.data.datasets[handle as usize] = Some(ResidentData::Gpu(data));
        Ok(())
    }
    /// 借用原 GPU 存储完成下载；取得完整 CPU 结果后才替换原槽。
    /// 驻留拒绝或下载错误仍沿原 Result 返回，原输入资源继续由 Frame 持有。
    pub fn download(&mut self, handle: u32) -> Result<()> {
        let index = handle as usize;
        let ResidentData::Gpu(data) = self.data.datasets[index].as_ref().unwrap() else {
            return Err(error("GPU 执行需要先显式 upload 数据"));
        };
        let data = self.device.as_ref().unwrap().download(data)?;
        self.data.datasets[index] = Some(ResidentData::Cpu(data.into()));
        Ok(())
    }
    pub fn allocate(&mut self, ty: u32, count: u32) -> Result<u32> {
        let data = Some(ResidentData::Cpu(
            Dataset::with_type(self.types[ty as usize].clone(), count)?.into(),
        ));
        if let Some(index) = self.data.datasets.iter().position(Option::is_none) {
            self.data.datasets[index] = data;
            return Ok(u32::try_from(index)?);
        }
        let index = u32::try_from(self.data.datasets.len())?;
        self.data.datasets.push(data);
        Ok(index)
    }
    /// 调整实际 CPU 独占存储，保留共同前缀；新元素只读取填充值的声明分量。
    /// 扩容申请失败时原存储、长度及域关系仍由本 Frame 持有。
    ///
    /// # Safety
    /// handle 由本 Frame 独占持有且没有存活地址消费者。fill 按该存储的 Ty 布局
    /// 提供全部已初始化分量，保持到本调用结束；填充字节不需要初始化。
    pub unsafe fn resize(&mut self, handle: u32, count: u32, fill: *const u32) -> Result<u32> {
        let index = handle as usize;
        let data = match self.data.datasets[index].as_mut().unwrap() {
            ResidentData::Cpu(CpuDataset::Owned(data)) => data,
            ResidentData::Cpu(CpuDataset::Read(_)) => {
                return Err(error("冻结的 CPU 存储不能调整长度"));
            }
            ResidentData::Gpu(_) => return Err(error("CPU 访问需要先显式 download 数据")),
        };
        let stride = data.stride as usize;
        let words = (stride * count as usize).div_ceil(4);
        if words > data.words.len() { data.words.try_reserve(words - data.words.len())?; }
        data.words.resize(words, 0);
        for row in data.count..count {
            for (scalar, offset) in &data.ty.components {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        fill.cast::<u8>().add(*offset as usize),
                        data.words.as_mut_ptr().cast::<u8>().add(row as usize * stride + *offset as usize),
                        scalar.size() as usize,
                    );
                }
            }
        }
        data.count = count;
        self.data.domains.retain(|relation| relation.domain != index && relation.count.binding != index);
        Ok(handle)
    }
    pub fn release(&mut self, handle: u32) {
        let index = handle as usize;
        self.data.datasets[index] = None;
        self.data
            .domains
            .retain(|r| r.domain != index && r.count.binding != index);
    }
    /// 借用当前 CPU 存储，不复制记录或隐式下载 GPU 存储。
    /// 直接借用 Frame 中的原位置；驻留或写权限拒绝时仍保留原存储。
    ///
    /// # Safety
    /// 指针只在下一次可能移动该存储的执行／释放前有效。调用者必须遵守 B 的
    /// 类型、索引域与独占访问语义；写区为offset+i*stride开始的bytes字节，
    /// i<count；bytes或count为零无写入。只撤回真实相交Count，返回原根地址。
    pub unsafe fn address(&mut self, handle: u32, offset: u64, bytes: u32, stride: u64, count: u32) -> Result<*mut u32> {
        let index = handle as usize;
        let data = match self.data.datasets[index].as_mut().unwrap() {
            ResidentData::Cpu(data) => data,
            ResidentData::Gpu(_) => return Err(error("CPU 访问需要先显式 download 数据")),
        };
        let address = match data {
            CpuDataset::Owned(data) => data.words.as_mut_ptr(),
            CpuDataset::Read(data) => {
                if bytes != 0 && count != 0 {
                    return Err(error("冻结的 CPU 存储不能取得写借用"));
                }
                data.words().as_ptr().cast_mut()
            }
        };
        if bytes != 0 && count != 0 {
            self.data.domains.retain(|r| {
                let end = u64::from(r.count.word) + 4;
                if r.count.binding != index || offset >= end { return true; }
                let row = if stride == 0 { 0 } else {
                    ((end - 1 - offset) / stride).min(u64::from(count - 1))
                };
                u64::from(r.count.word) >= offset + row * stride + u64::from(bytes)
            });
        }
        Ok(address)
    }
    /// 原存储的位置地址；GPU返回原绑定头的设备地址，不等待或下载。
    /// 地址不保活存储；CPU解引用仍要求真实CPU位置及原借用权限。
    pub fn storage_address(&self,handle:u32)->u64 {
        match self.data.datasets[handle as usize].as_ref().unwrap() {
            ResidentData::Cpu(data)=>data.words().as_ptr() as usize as u64,
            ResidentData::Gpu(data)=>data.storage_address(),
        }
    }
    /// 同一帧只移动本次实际参数；没有被绑定的存储及其有效域继续由帧持有。
    /// CPU 直接消费原存储，错误与成功均交回原输入及程序；参数拒绝保持原域，
    /// 取得可写访问后撤回覆盖的计数关系，只有真实完成才发布新保证。
    /// GPU 等待本次真实完成后返回，数据仍驻留原设备；不隐式下载。
    pub fn run(&mut self, program: u32, bindings: &[DatasetBinding], values: &[Value]) -> Result<()> {
        let index = program as usize;
        let layout = self.programs[index].as_ref().unwrap().layout();
        // 参数是存储的引用；同一物理存储只移动一次。
        let mut selected = Vec::new();
        let mut order: Vec<(usize, u32)> = Vec::with_capacity(bindings.len());
        for (logical, binding) in bindings.iter().enumerate() {
            let handle = binding.handle;
            let slot = if let Some(slot) = selected.iter().position(|&h| h == handle) {
                use blanguage::ast::Access::{Atomic, Read};
                let current = &layout.buffers()[logical];
                for (before, previous) in bindings[..logical].iter().zip(layout.buffers()) {
                    if before.handle == handle
                        && u64::from(before.offset) < u64::from(binding.offset) + u64::from(current.ty.size)
                        && u64::from(binding.offset) < u64::from(before.offset) + u64::from(previous.ty.size)
                        && !matches!((previous.access, current.access), (Read, Read) | (Atomic, Atomic))
                    {
                        return Err(error("同一存储重叠区域的重复参数必须同为 read 或同为 atomic"));
                    }
                }
                slot
            } else {
                selected.push(handle);
                selected.len() - 1
            };
            order.push((slot, binding.offset));
        }
        let domains: Vec<_> = self
            .data
            .domains
            .iter()
            .filter_map(|r| {
                Some(DomainRelation {
                    domain: selected.iter().position(|&h| h as usize == r.domain)?,
                    offset: r.offset,
                    bytes: r.bytes,
                    count: PrefixCount {
                        binding: selected
                            .iter()
                            .position(|&h| h as usize == r.count.binding)?,
                        word: r.count.word,
                    },
                })
            })
            .collect();
        let loaded = self.load(index)?;
        let layout = loaded.layout();
        let (datasets, domains, result) = match &loaded {
            Program::Cpu(program) => {
                let mut datasets = Vec::new();
                for &handle in &selected {
                    match self.data.datasets[handle as usize].take().unwrap() {
                        ResidentData::Cpu(data) => datasets.push(data),
                        data @ ResidentData::Gpu(_) => {
                            self.data.datasets[handle as usize] = Some(data);
                            for (&handle, data) in selected.iter().zip(datasets) {
                                self.data.datasets[handle as usize] = Some(ResidentData::Cpu(data));
                            }
                            self.programs[index] = Some(ProgramSlot::Loaded(loaded));
                            return Err(error("CPU 访问需要先显式 download 数据"));
                        }
                    }
                }
                let mut inputs = DomainBindings { datasets, domains };
                let result = program.run_projected(&mut inputs, &order, values, || {
                    crate::bindings::invalidate(&mut self.data.domains, layout,
                        |i| (bindings[i].handle as usize, bindings[i].offset));
                });
                (
                    inputs
                        .datasets
                        .into_iter()
                        .map(ResidentData::Cpu)
                        .collect::<Vec<_>>(),
                    inputs.domains,
                    result,
                )
            }
            Program::Gpu(gpu_program) => {
                let mut datasets = Vec::with_capacity(selected.len());
                for &handle in &selected {
                    match self.data.datasets[handle as usize].take().unwrap() {
                        ResidentData::Gpu(data) => datasets.push(data),
                        data @ ResidentData::Cpu(_) => {
                            self.data.datasets[handle as usize] = Some(data);
                            for (&handle, data) in selected.iter().zip(datasets) {
                                self.data.datasets[handle as usize] = Some(ResidentData::Gpu(data));
                            }
                            self.programs[index] = Some(ProgramSlot::Loaded(loaded));
                            return Err(error("GPU 执行需要先显式 upload 数据"));
                        }
                    }
                }
                let submission = match gpu_program
                    .submit_projected(DomainBindings { datasets, domains }, &order, values)
                {
                    Ok(submission) => submission,
                    Err((error, bindings)) => {
                        for (&handle, data) in selected.iter().zip(bindings.datasets) {
                            self.data.datasets[handle as usize] = Some(ResidentData::Gpu(data));
                        }
                        self.programs[index] = Some(ProgramSlot::Loaded(loaded));
                        return Err(error);
                    }
                };
                crate::bindings::invalidate(&mut self.data.domains, layout,
                    |i| (bindings[i].handle as usize, bindings[i].offset));
                let (inputs, result) = match submission.wait() {
                    Ok(completed) => (completed.into_bindings(), Ok(())),
                    Err((error, submission)) => {
                        let mut inputs = submission.into_bindings();
                        inputs.domains.clear();
                        (inputs, Err(error))
                    }
                };
                (
                    inputs
                        .datasets
                        .into_iter()
                        .map(ResidentData::Gpu)
                        .collect::<Vec<_>>(),
                    inputs.domains,
                    result,
                )
            }
        };
        for (&handle, data) in selected.iter().zip(datasets) {
            self.data.datasets[handle as usize] = Some(data);
        }
        self.data
            .domains
            .extend(domains.into_iter().map(|r| DomainRelation {
                domain: selected[r.domain] as usize,
                offset: r.offset,
                bytes: r.bytes,
                count: PrefixCount {
                    binding: selected[r.count.binding] as usize,
                    word: r.count.word,
                },
            }));
        self.data.domains.sort();
        self.data.domains.dedup();
        self.programs[index] = Some(ProgramSlot::Loaded(loaded));
        result
    }
}
