//! B 应用拥有动态数据；原 Dataset、域关系及 GPU 最后使用点仍拥有物理资源。
use crate::{
    Dataset, DomainBindings, Result, Value,
    cpu::{CpuDataset, CpuTemplate},
    error, vulkan,
};
use std::sync::Arc;
use blanguage::{
    TemplateProgram,
    ir::{ExecutionPlan, Ty},
};

mod abi;
pub use blanguage::resource::DatasetBinding;

pub enum ProgramImage {
    Cpu(blanguage::native::ProgramImage<TemplateProgram>),
    CpuLinked(blanguage::native::ProgramImage<TemplateProgram, *const u8>),
    Gpu(blanguage::spirv::ProgramImage<TemplateProgram>),
}
/// 已装入程序的只读借用；不转移代码或设备的拥有关系。
enum Program<'a> {
    Cpu(&'a CpuTemplate),
    Gpu(&'a vulkan::GpuTemplate),
}
impl Program<'_> {
    fn layout(&self) -> &ExecutionPlan {
        match self {
            Self::Cpu(p) => p.program(),
            Self::Gpu(p) => p.program(),
        }
    }
}
/// 原模块持有的不可变程序定义；CPU代码已绑定，GPU映像供实际设备装入。
pub enum ProgramSlot {
    Cpu(CpuTemplate),
    Gpu(Arc<blanguage::spirv::ProgramImage<TemplateProgram>>),
}
impl ProgramSlot {
    pub fn layout(&self) -> &ExecutionPlan {
        match self {
            Self::Cpu(program) => program.program(),
            Self::Gpu(image) => image.program(),
        }
    }
}
struct Definitions {
    types: Vec<Ty>,
    programs: Vec<ProgramSlot>,
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
    definitions: Arc<Definitions>,
    gpu_programs: Vec<Option<vulkan::GpuTemplate>>,
    device: Option<vulkan::Device>,
}
impl Frame {
    /// 消费本模块定义并绑定CPU代码；失败时未创建Frame，局部资源按原所有者释放。
    pub fn new(types: Vec<Ty>, images: Vec<ProgramImage>) -> Result<Self> {
        let programs = images.into_iter().map(|image| match image {
            ProgramImage::Cpu(image) => CpuTemplate::from_image(image)
                .map(ProgramSlot::Cpu).map_err(|(error, _)| error),
            ProgramImage::CpuLinked(image) => unsafe { CpuTemplate::from_linked(image) }
                .map(ProgramSlot::Cpu).map_err(|(error, _)| error),
            ProgramImage::Gpu(image) => Ok(ProgramSlot::Gpu(Arc::new(image))),
        }).collect::<Result<Vec<_>>>()?;
        Ok(Self {
            data: Vec::new().into(),
            gpu_programs: (0..programs.len()).map(|_| None).collect(),
            definitions: Arc::new(Definitions { types, programs }),
            device: None,
        })
    }
    /// 借用当前原数据根；槽索引为本Frame实际句柄，None保留已释放空槽。
    pub fn data(&self) -> &DomainBindings<Option<ResidentData>> {
        &self.data
    }
    /// 借用原模块的同一程序目录Vec；索引、len及capacity均属于这个真实容器。
    pub fn programs(&self) -> &Vec<ProgramSlot> {
        &self.definitions.programs
    }
    /// 借用本Frame实际GPU程序槽；CPU位置没有本地GPU实例。
    pub fn gpu_programs(&self) -> &Vec<Option<vulkan::GpuTemplate>> {
        &self.gpu_programs
    }
    /// 借用原目录类型Vec，索引仍为本模块实际类型号。
    pub fn types(&self) -> &Vec<Ty> {
        &self.definitions.types
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
    fn load_gpu(&mut self, index: usize, image: &blanguage::spirv::ProgramImage<TemplateProgram>) -> Result<()> {
        if self.gpu_programs[index].is_none() {
            if self.device.is_none() {
                self.create_device()?;
            }
            self.gpu_programs[index] = Some(self.device.as_ref().unwrap().load_template_ref(image)?);
        }
        Ok(())
    }
    fn create_device(&mut self) -> Result<()> {
        let programs = self.definitions.programs.iter().filter_map(|definition| match definition {
            ProgramSlot::Gpu(image) => Some(image.program()),
            ProgramSlot::Cpu(_) => None,
        });
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
            Dataset::with_type(self.definitions.types[ty as usize].clone(), count)?.into(),
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
        self.data.domains.retain(|relation| !relation.references(index));
        Ok(handle)
    }
    pub fn release(&mut self, handle: u32) {
        let index = handle as usize;
        self.data.datasets[index] = None;
        self.data
            .domains
            .retain(|r| !r.references(index));
    }
    // 只由受信任编译完成点调用；字段布局和完整写已由原编译器确定。
    fn complete_field(&mut self, handle: u32, offset: u32,
        intervals: &[blanguage::resource::ElementInterval], limits: &[u32]) {
        let ty = match self.data.datasets[handle as usize].as_ref().unwrap() {
            ResidentData::Cpu(data) => &data.ty,
            ResidentData::Gpu(data) => data.layout(),
        };
        let scalar = ty.components.iter().find(|(_, at)| *at == offset).unwrap().0;
        let mut intervals = intervals.iter().map(|range| (range.lower, range.upper)).collect::<Vec<_>>();
        for &count in limits {
            intervals = intervals.into_iter().filter_map(|(a, b)| {
                let end = b.min(u64::from(count.checked_sub(1)?));
                (a <= end).then_some((a, end))
            }).collect();
        }
        self.data.produced_element(blanguage::ir::ElementRange { binding: handle as usize, offset, scalar, intervals });
        self.data.domains.sort();
        self.data.domains.dedup();
    }
    /// 取得同一存储位置并撤回真实写区相交的Count；不分配、搬移或等待。
    ///
    /// # Safety
    /// handle属于本Frame的存活资源。CPU解引用必须已取得CPU位置及相应访问权，
    /// 冻结存储只读；指针保持到最后消费者结束，期间不得移动或释放原存储。
    /// 写区为offset+i*stride开始的bytes字节，i<count；零bytes或count无写入。
    pub unsafe fn address(&mut self, handle: u32, offset: u64, bytes: u32, stride: u64, count: u32) -> *mut u32 {
        let index = handle as usize;
        let address = self.storage_address(handle) as usize as *mut u32;
        if bytes != 0 && count != 0 {
            let (record_stride, records) = match self.data.datasets[index].as_ref().unwrap() {
                ResidentData::Cpu(data) => (data.stride(), data.count()),
                ResidentData::Gpu(data) => (data.stride(), data.count()),
            };
            self.data.domains.retain(|r| !r.address_written(index, offset, bytes, stride, count, record_stride, records));
        }
        address
    }
    /// 原存储的位置地址；GPU返回原绑定头的设备地址，不等待或下载。
    /// 地址不保活存储；CPU解引用仍要求真实CPU位置及原借用权限。
    pub fn storage_address(&mut self,handle:u32)->u64 {
        match self.data.datasets[handle as usize].as_mut().unwrap() {
            ResidentData::Cpu(CpuDataset::Owned(data))=>data.words.as_mut_ptr() as usize as u64,
            ResidentData::Cpu(CpuDataset::Read(data))=>data.words().as_ptr() as usize as u64,
            ResidentData::Gpu(data)=>data.storage_address(),
        }
    }
    /// 同一帧只移动本次实际参数；没有被绑定的存储及其有效域继续由帧持有。
    /// CPU 直接消费原存储，错误与成功均交回原输入及程序；参数拒绝保持原域，
    /// 取得可写访问后撤回覆盖的计数关系，只有真实完成才发布新保证。
    /// GPU 等待本次真实完成后返回，数据仍驻留原设备；不隐式下载。
    pub fn run(&mut self, program: u32, bindings: &[DatasetBinding], values: &[Value]) -> Result<()> {
        let index = program as usize;
        let definitions = self.definitions.clone();
        let definition = &definitions.programs[index];
        let layout = definition.layout();
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
            .filter_map(|r| r.mapped(|binding| selected.iter().position(|&handle| handle as usize == binding)))
            .collect();
        let loaded = match definition {
            ProgramSlot::Cpu(program) => Program::Cpu(program),
            ProgramSlot::Gpu(image) => {
                self.load_gpu(index, image)?;
                Program::Gpu(self.gpu_programs[index].as_ref().unwrap())
            }
        };
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
            .extend(domains.into_iter().map(|r| r.mapped(|binding| Some(selected[binding] as usize)).unwrap()));
        self.data.domains.sort();
        self.data.domains.dedup();
        result
    }
}
