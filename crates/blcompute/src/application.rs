//! B 应用拥有动态数据；原 Dataset、域关系及 GPU 最后使用点仍拥有物理资源。
use crate::{Dataset, DomainBindings, Result, Value, cpu::CpuTemplate, error, vulkan};
use blanguage::{
    TemplateProgram,
    ir::{ExecutionPlan, PrefixCount, PrefixRelation, Ty},
};

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
    Cpu(Dataset),
    Gpu(vulkan::GpuDataset),
}
impl ResidentData {
    fn cpu(self) -> Result<Dataset> {
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
        self.data.datasets[handle as usize] = Some(ResidentData::Cpu(data));
        Ok(())
    }
    pub fn allocate(&mut self, ty: u32, count: u32) -> Result<u32> {
        let data = Some(ResidentData::Cpu(Dataset::with_type(
            self.types[ty as usize].clone(),
            count,
        )?));
        if let Some(index) = self.data.datasets.iter().position(Option::is_none) {
            self.data.datasets[index] = data;
            return Ok(u32::try_from(index)?);
        }
        let index = u32::try_from(self.data.datasets.len())?;
        self.data.datasets.push(data);
        Ok(index)
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
    /// 类型、索引域与独占访问语义；write=true 取得写借用并放弃依赖该计数的域关系。
    pub unsafe fn address(&mut self, handle: u32, write: bool) -> Result<*mut u32> {
        let index = handle as usize;
        let source = self.data.datasets[index].take().unwrap();
        let mut data = source.cpu()?;
        let address = data.words.as_mut_ptr();
        self.data.datasets[index] = Some(ResidentData::Cpu(data));
        if write {
            self.data.domains.retain(|r| r.count.binding != index);
        }
        Ok(address)
    }
    /// 同一帧只移动本次实际参数；没有被绑定的存储及其有效域继续由帧持有。
    /// CPU 直接消费原存储；GPU 提交后返回，数据保持驻留并持有最后使用点。
    pub fn run(&mut self, program: u32, handles: &[u32], values: &[Value]) -> Result<()> {
        let index = program as usize;
        let program = self.load(index)?;
        let layout = program.layout();
        let domains: Vec<_> = self
            .data
            .domains
            .iter()
            .filter_map(|r| {
                Some(PrefixRelation {
                    domain: handles.iter().position(|&h| h as usize == r.domain)?,
                    count: PrefixCount {
                        binding: handles
                            .iter()
                            .position(|&h| h as usize == r.count.binding)?,
                        word: r.count.word,
                    },
                })
            })
            .collect();
        let (datasets, domains) = match &program {
            Program::Cpu(program) => {
                let datasets = handles
                    .iter()
                    .map(|&handle| self.data.datasets[handle as usize].take().unwrap().cpu())
                    .collect::<Result<Vec<_>>>()?;
                let result = program.run(DomainBindings { datasets, domains }, values)?;
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
                let datasets = handles
                    .iter()
                    .map(|&handle| self.data.datasets[handle as usize].take().unwrap().gpu())
                    .collect::<Result<Vec<_>>>()?;
                let result = program
                    .submit(DomainBindings { datasets, domains }, values)?
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
        for (&handle, data) in handles.iter().zip(datasets) {
            self.data.datasets[handle as usize] = Some(data);
        }
        self.data.domains.retain(|r| {
            !handles
                .iter()
                .zip(layout.buffers())
                .any(|(&h, b)| h as usize == r.count.binding && b.access.writes())
        });
        self.data
            .domains
            .extend(domains.into_iter().map(|r| PrefixRelation {
                domain: handles[r.domain] as usize,
                count: PrefixCount {
                    binding: handles[r.count.binding] as usize,
                    word: r.count.word,
                },
            }));
        self.data.domains.sort();
        self.data.domains.dedup();
        self.programs[index] = Some(ProgramSlot::Loaded(program));
        Ok(())
    }
}
