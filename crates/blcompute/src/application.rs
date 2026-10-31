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
    Gpu(blanguage::spirv::ProgramImage<TemplateProgram>),
}
enum Program {
    Cpu(CpuTemplate),
    Gpu(vulkan::GpuTemplate),
}
enum ProgramSlot {
    Image(ProgramImage),
    Loaded(Program),
}
impl ProgramSlot {
    fn layout(&self) -> &ExecutionPlan {
        match self {
            Self::Image(ProgramImage::Cpu(image)) => image.program(),
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
enum ResidentData {
    Cpu(CpuDataset),
    Gpu(vulkan::GpuDataset),
}
impl ResidentData {
    fn cpu(self) -> Result<CpuDataset> {
        match self {
            Self::Cpu(d) => Ok(d),
            Self::Gpu(_) => Err(error("CPU 访问需要先显式 download 数据")),
        }
    }
    fn gpu(self) -> Result<vulkan::GpuDataset> {
        match self {
            Self::Cpu(_) => Err(error("GPU 执行需要先显式 upload 数据")),
            Self::Gpu(d) => Ok(d),
        }
    }
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
                CpuTemplate::from_image(image).map(Program::Cpu)
            }
            ProgramSlot::Image(ProgramImage::Gpu(image)) => self
                .device
                .as_ref()
                .unwrap()
                .load_template(image)
                .map(Program::Gpu),
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
    pub fn upload(&mut self, handle: u32) -> Result<()> {
        if self.device.is_none() {
            self.create_device()?;
        }
        let data = self.data.datasets[handle as usize].take().unwrap().cpu()?;
        let data = self.device.as_ref().unwrap().upload(data)?;
        self.data.datasets[handle as usize] = Some(ResidentData::Gpu(data));
        Ok(())
    }
    pub fn download(&mut self, handle: u32) -> Result<()> {
        let data = self.data.datasets[handle as usize].take().unwrap().gpu()?;
        let data = self.device.as_ref().unwrap().download(&data)?;
        self.data.datasets[handle as usize] = Some(ResidentData::Cpu(data.into()));
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
    ///
    /// # Safety
    /// handle 由本 Frame 独占持有且没有存活地址消费者。fill 按该存储的 Ty 布局
    /// 提供全部已初始化分量，保持到本调用结束；填充字节不需要初始化。
    pub unsafe fn resize(&mut self, handle: u32, count: u32, fill: *const u32) -> Result<u32> {
        let index = handle as usize;
        let source = self.data.datasets[index].take().unwrap().cpu()?;
        let CpuDataset::Owned(mut data) = source else {
            return Err(error("冻结的 CPU 存储不能调整长度"));
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
        self.data.datasets[index] = Some(ResidentData::Cpu(CpuDataset::Owned(data)));
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
    ///
    /// # Safety
    /// 指针只在下一次可能移动该存储的执行／释放前有效。调用者必须遵守 B 的
    /// 类型、索引域与独占访问语义；非零bytes交接从offset开始的实际可写字节范围，
    /// 只撤回与其重叠的计数字段关系。返回原根地址，消费不得写出本次交接范围。
    pub unsafe fn address(&mut self, handle: u32, offset: u64, bytes: u32) -> Result<*mut u32> {
        let index = handle as usize;
        let source = self.data.datasets[index].take().unwrap();
        let mut data = source.cpu()?;
        let address = match &mut data {
            CpuDataset::Owned(data) => data.words.as_mut_ptr(),
            CpuDataset::Read(data) => {
                if bytes != 0 {
                    return Err(error("冻结的 CPU 存储不能取得写借用"));
                }
                data.words().as_ptr().cast_mut()
            }
        };
        self.data.datasets[index] = Some(ResidentData::Cpu(data));
        if bytes != 0 {
            self.data.domains.retain(|r| {
                r.count.binding != index
                    || u64::from(r.count.word) + 4 <= offset
                    || offset + u64::from(bytes) <= u64::from(r.count.word)
            });
        }
        Ok(address)
    }
    /// 同一帧只移动本次实际参数；没有被绑定的存储及其有效域继续由帧持有。
    /// CPU 直接消费原存储；GPU 等待本次真实完成后返回，数据仍驻留原设备；不隐式下载。
    pub fn run(&mut self, program: u32, bindings: &[DatasetBinding], values: &[Value]) -> Result<()> {
        let index = program as usize;
        let program = self.load(index)?;
        let layout = program.layout();
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
        let (datasets, domains) = match &program {
            Program::Cpu(program) => {
                let datasets = selected
                    .iter()
                    .map(|&handle| self.data.datasets[handle as usize].take().unwrap().cpu())
                    .collect::<Result<Vec<_>>>()?;
                let result =
                    program.run_projected(DomainBindings { datasets, domains }, &order, values)?;
                (
                    result
                        .datasets
                        .into_iter()
                        .map(ResidentData::Cpu)
                        .collect::<Vec<_>>(),
                    result.domains,
                )
            }
            Program::Gpu(program) => {
                if order.iter().any(|&(_, offset)| offset != 0) {
                    return Err(error("整域GPU字段投影尚未实现"));
                }
                let physical: Vec<_> = order.iter().map(|&(slot, _)| slot).collect();
                let datasets = selected
                    .iter()
                    .map(|&handle| self.data.datasets[handle as usize].take().unwrap().gpu())
                    .collect::<Result<Vec<_>>>()?;
                let result = program
                    .submit_projected(DomainBindings { datasets, domains }, &physical, values)?
                    .wait()?
                    .into_bindings();
                (
                    result
                        .datasets
                        .into_iter()
                        .map(ResidentData::Gpu)
                        .collect::<Vec<_>>(),
                    result.domains,
                )
            }
        };
        for (&handle, data) in selected.iter().zip(datasets) {
            self.data.datasets[handle as usize] = Some(data);
        }
        self.data.domains.retain(|r| {
            !bindings
                .iter()
                .zip(layout.buffers())
                .any(|(binding, b)| {
                    binding.handle as usize == r.count.binding && b.access.writes()
                        && u64::from(r.count.word) >= u64::from(binding.offset)
                        && u64::from(r.count.word) < u64::from(binding.offset) + u64::from(b.ty.size)
                })
        });
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
        self.programs[index] = Some(ProgramSlot::Loaded(program));
        Ok(())
    }
}
