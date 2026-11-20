//! B 应用拥有动态数据；原 Dataset、域关系及 GPU 最后使用点仍拥有物理资源。
use crate::{
    Dataset, DomainBindings, Result, Value,
    cpu::{CpuDataset, CpuTemplate},
    error, vulkan,
};
use std::{ptr::NonNull, sync::Arc};
use blanguage::{
    TemplateProgram,
    ir::{ExecutionPlan, Ty},
};

mod abi;
mod task;
#[cfg(windows)]
pub(crate) use abi::{take_errors,resource_symbol};
pub use blanguage::resource::DatasetBinding;

pub enum ProgramImage {
    Cpu(blanguage::native::ProgramImage<TemplateProgram>),
    CpuLinked(blanguage::native::ProgramImage<TemplateProgram, *const u8>),
    Gpu(blanguage::spirv::ProgramImage<TemplateProgram>),
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
pub(crate) struct Definitions {
    pub(crate) types: Vec<Ty>,
    pub(crate) programs: Vec<ProgramSlot>,
}
impl Definitions {
    fn new(types: Vec<Ty>, images: Vec<ProgramImage>) -> Result<Self> {
        let programs = images.into_iter().map(|image| match image {
            ProgramImage::Cpu(image) => CpuTemplate::from_image(image)
                .map(ProgramSlot::Cpu).map_err(|(error, _)| error),
            ProgramImage::CpuLinked(image) => unsafe { CpuTemplate::from_linked(image) }
                .map(ProgramSlot::Cpu).map_err(|(error, _)| error),
            ProgramImage::Gpu(image) => Ok(ProgramSlot::Gpu(Arc::new(image))),
        }).collect::<Result<Vec<_>>>()?;
        Ok(Self { types, programs })
    }
    pub(crate) unsafe fn linked(
        factories: &[blanguage::resource::ModuleFactory],
        mut cpu: impl FnMut(blanguage::native::ProgramImage<TemplateProgram, *const u8>) -> Result<CpuTemplate>,
    ) -> Result<Self> {
        let mut types = Vec::new();
        let mut programs = Vec::new();
        for factory in factories {
            let image = unsafe { factory() };
            let bytes = unsafe { std::slice::from_raw_parts(image.data, image.bytes) };
            let code = unsafe { std::slice::from_raw_parts(image.code, image.codes) };
            let module = unsafe { blanguage::resource::from_linked_compiler_output(bytes, code) }
                .map_err(|e| error(e.to_string()))?;
            types.extend(module.types);
            for image in module.images {
                programs.push(match image {
                    blanguage::resource::Image::Cpu(image) => ProgramSlot::Cpu(cpu(image)?),
                    blanguage::resource::Image::Gpu(image) => ProgramSlot::Gpu(Arc::new(image)),
                });
            }
        }
        Ok(Self { types, programs })
    }
}
enum CpuBorrowLayout {
    Range(NonNull<Ty>),
    Task(Ty),
}

/// 范围输入的真实非拥有CPU借用；载荷及view资源仍由原执行输入保持。
pub struct CpuBorrow {
    layout: CpuBorrowLayout,
    pub(crate) stride: u32,
    pub(crate) count: u32,
    pub(crate) pointer: *mut u32,
    pub(crate) writable: bool,
}
impl CpuBorrow {
    pub(crate) fn layout(&self) -> &Ty {
        match &self.layout {
            // The originating execution keeps the range input's layout alive
            // until the last synchronous consumer in this Frame has finished.
            CpuBorrowLayout::Range(ty) => unsafe { ty.as_ref() },
            CpuBorrowLayout::Task(ty) => ty,
        }
    }
}

/// 当前实际驻留存储；Frame观察只借用该原值，不转移所有权。
pub enum ResidentData {
    Cpu(CpuDataset),
    CpuBorrow(CpuBorrow),
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
    task_completion_writes: Option<task::InputCompletionWrites>,
    gpu_programs: Vec<Option<vulkan::GpuTemplate>>,
    device: Option<vulkan::Device>,
}
impl Frame {
    /// 消费本模块定义并绑定CPU代码；失败时未创建Frame，局部资源按原所有者释放。
    pub fn new(types: Vec<Ty>, images: Vec<ProgramImage>) -> Result<Self> {
        Ok(unsafe { Self::from_definitions(Arc::new(Definitions::new(types, images)?), &[], &[]) })
    }
    /// 从同次静态链接的真实模块工厂创建应用Frame。
    ///
    /// # Safety
    /// 工厂、字节与代码属于同次可信链接，顺序对应原模块基址；
    /// 原模块及依赖保持到最后真实代码、存储及设备消费者结束。
    pub unsafe fn from_linked_modules(factories: &[blanguage::resource::ModuleFactory]) -> Result<Self> {
        let definitions = unsafe { Definitions::linked(factories, |image| CpuTemplate::from_linked(image).map_err(|(error, _)| error)) }?;
        Ok(unsafe { Self::from_definitions(Arc::new(definitions), &[], &[]) })
    }
    /// 本次范围独占局部存储，只借用原模块已绑定的同一不可变定义。
    /// # Safety
    /// inputs的实际存储、布局、权限和借用资源保持到本Frame最后消费者完成；本值不取得其释放权。
    /// input_domains来自已接受的实际输入合同，取得写访问后仍有效；索引对应同一inputs。
    pub(crate) unsafe fn from_definitions(definitions: Arc<Definitions>, inputs:&[crate::cpu::CpuBuffer<'_>], input_domains:&[crate::DomainRelation]) -> Self {
        let datasets=inputs.iter().map(|data|Some(ResidentData::CpuBorrow(CpuBorrow{
            layout:CpuBorrowLayout::Range(NonNull::from(data.ty)),stride:data.stride,count:data.count,pointer:data.pointer,writable:data.writable,
        }))).collect();
        Self {
            data: DomainBindings {datasets,domains:input_domains.to_vec()},
            gpu_programs: (0..definitions.programs.len()).map(|_| None).collect(),
            definitions,
            task_completion_writes: None,
            device: None,
        }
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
    /// 查询本Frame拥有／冻结的CPU数据字存储；范围输入只借用，不重复计为本Frame分配。
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
            let device = Self::gpu_device(&self.definitions, &mut self.device)?;
            self.gpu_programs[index] = Some(device.load_template_ref(image)?);
        }
        Ok(())
    }
    fn gpu_device<'a>(definitions: &Definitions, slot: &'a mut Option<vulkan::Device>) -> Result<&'a vulkan::Device> {
        Ok(match slot {
            Some(device) => device,
            slot @ None => {
                let programs = definitions.programs.iter().filter_map(|definition| match definition {
                    ProgramSlot::Gpu(image) => Some(image.program()),
                    ProgramSlot::Cpu(_) => None,
                });
                slot.insert(vulkan::Device::for_programs(programs)?)
            }
        })
    }
    /// 先接收原CPU输入，再取得设备；成功上传后替换原槽，暂存区按实际完成释放。
    pub fn upload(&mut self, handle: u32) -> Result<()> {
        let data=match self.data.datasets[handle as usize].as_ref().unwrap() {
            ResidentData::Cpu(data)=>data,
            ResidentData::CpuBorrow(_)=>return Err(error("借用的CPU输入不拥有驻留转移权")),
            ResidentData::Gpu(_)=>return Err(error("CPU 访问需要先显式 download 数据")),
        };
        let data = vulkan::Device::upload_cpu(
            data,
            || Self::gpu_device(&self.definitions, &mut self.device),
        )?;
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
            ResidentData::CpuBorrow(_) => return Err(error("借用的CPU输入不拥有调整长度权")),
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
        let previous = std::mem::replace(&mut data.count, count);
        self.data.domains.retain(|relation| !relation.references(index)
            || count <= previous && matches!(relation, crate::DomainRelation::Element(_)));
        Ok(handle)
    }
    pub fn release(&mut self, handle: u32) {
        let index = handle as usize;
        self.data.datasets[index] = None;
        self.data
            .domains
            .retain(|r| !r.references(index));
    }
    /// 将编译器或可信外部提供者的共同字段保证交给原Frame拥有组。
    /// 数值数组只同步借用；本调用不写载荷、不提交或等待设备。
    ///
    /// # Safety
    /// handle是本Frame当前存活存储，offset是其真实标量字段；全部元素已初始化。
    /// intervals以该Scalar有效有序编码描述全部字段值的闭区间并集，与known_bits交集成立。
    /// known_bits是原始固定位(mask,value)，仅用于整数；value只含mask内该Scalar有效位。
    /// 非空limits仅用于u32字段，每项为已保证的严格上界；保证须来自编译器证明
    /// 或可信实现对实际字节的担保，不能凭ABI签名、格式或非零推定。
    /// 实际写已完成或由原存储依赖覆盖后续消费者，最后消费者前的寿命及访问
    /// 约束继续成立；外部可写访问须先沿原address写区撤回，不能保留未跟踪写别名。
    pub unsafe fn complete_field(&mut self, handle: u32, offset: u32,
        intervals: &[blanguage::resource::ElementInterval], limits: &[u32], known_bits: (u64,u64)) {
        let ty = match self.data.datasets[handle as usize].as_ref().unwrap() {
            ResidentData::Cpu(data) => &data.ty,
            ResidentData::CpuBorrow(data) => data.layout(),
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
        unsafe { self.data.produced_element(blanguage::ir::ElementRange { binding: handle as usize, offset, scalar, intervals, known_bits }) };
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
                ResidentData::CpuBorrow(data) => (data.stride,data.count),
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
            ResidentData::CpuBorrow(data)=>data.pointer as usize as u64,
            ResidentData::Gpu(data)=>data.storage_address(),
        }
    }
    /// CPU 在本次同步执行中借用实际绑定的帧槽，参数拒绝保持原域，
    /// 取得可写访问后撤回覆盖的计数关系，只有真实完成才发布新保证。
    /// GPU 参数驻留确认后才装入管线，完整选中的域关系随存储移交。
    /// 提交拒绝交回原关系，已提交的写入撤回旧事实，等待成功才交回新保证。
    pub fn run(&mut self, program: u32, bindings: &[DatasetBinding], values: &[Value], textures: &[crate::cpu::CpuTextureBinding<'_>]) -> Result<()> {
        let index = program as usize;
        let definitions = self.definitions.clone();
        let definition = &definitions.programs[index];
        let layout = definition.layout();
        let mut cpu_resident = false;
        let uniforms = crate::arguments(layout, bindings.iter().map(|binding| {
            let data = self.data.datasets[binding.handle as usize].as_ref().unwrap();
            cpu_resident |= !matches!(data, ResidentData::Gpu(_));
            let ty = match data {
                ResidentData::Cpu(data) => &data.ty,
                ResidentData::CpuBorrow(data) => data.layout(),
                ResidentData::Gpu(data) => data.layout(),
            };
            (ty, crate::DomainStorage::element_count(data), Some(binding.offset))
        }), values)?;
        // 同一物理存储只选一次，物理槽按首次绑定顺序分配，供CPU原位投影。
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
        let selected_slot = |binding| selected.iter().position(|&handle| handle as usize == binding);
        let (domains, result) = match definition {
            ProgramSlot::Cpu(program) => {
                let domains = self.data.domains.iter().filter_map(|r| r.mapped(selected_slot)).collect();
                let mut datasets = Vec::with_capacity(selected.len());
                let slots = self.data.datasets.as_mut_ptr();
                for &handle in &selected {
                    // Argument validation indexes every handle. Selection above records
                    // each slot once, and the slot array stays in place through the join.
                    let data = unsafe { &mut *slots.add(handle as usize) }.as_mut().unwrap();
                    if matches!(data, ResidentData::Gpu(_)) {
                        return Err(error("CPU 访问需要先显式 download 数据"));
                    }
                    datasets.push(data);
                }
                let mut inputs = DomainBindings { datasets, domains };
                let result = program.run_projected(&mut inputs, &order, textures, uniforms, &definitions, || {
                    crate::bindings::invalidate(&mut self.data.domains, &layout.write_regions,
                        |i| (bindings[i].handle as usize, bindings[i].offset));
                });
                (inputs.domains, result)
            }
            ProgramSlot::Gpu(image) => {
                if !textures.is_empty() {
                    return Err(error("CPU纹理借用不能作为GPU驻留资源"));
                }
                if cpu_resident {
                    return Err(error("GPU 执行需要先显式 upload 数据"));
                }
                self.load_gpu(index, image)?;
                let gpu_program = self.gpu_programs[index].as_ref().unwrap();
                let domains = self.data.domains
                    .extract_if(.., |relation| relation.binding_indices().all(|binding| selected_slot(binding).is_some()))
                    .map(|relation| relation.into_mapped(selected_slot).unwrap())
                    .collect();
                let mut datasets = Vec::with_capacity(selected.len());
                for &handle in &selected {
                    let Some(ResidentData::Gpu(data)) = self.data.datasets[handle as usize].take() else {
                        unreachable!("GPU residency was checked during argument binding");
                    };
                    datasets.push(data);
                }
                let (inputs, result) = match gpu_program
                    .submit_projected(DomainBindings { datasets, domains }, &order, uniforms)
                {
                    Err((error, inputs)) => (inputs, Err(error)),
                    Ok(submission) => {
                        let (mut inputs, result) = match submission.wait() {
                            Ok(completed) => (completed.into_bindings(), Ok(())),
                            Err((error, submission)) => (submission.into_bindings(), Err(error)),
                        };
                        crate::bindings::invalidate(&mut self.data.domains, &layout.write_regions,
                            |i| (bindings[i].handle as usize, bindings[i].offset));
                        crate::bindings::invalidate(&mut inputs.domains, &layout.write_regions, |i| order[i]);
                        if result.is_ok() {
                            inputs.produced(layout, |i| order[i]);
                        }
                        (inputs, result)
                    }
                };
                for (&handle, data) in selected.iter().zip(inputs.datasets) {
                    self.data.datasets[handle as usize] = Some(ResidentData::Gpu(data));
                }
                (inputs.domains, result)
            }
        };
        self.data
            .domains
            .extend(domains.into_iter().map(|r| r.into_mapped(|binding| Some(selected[binding] as usize)).unwrap()));
        self.data.domains.sort();
        self.data.domains.dedup();
        result
    }
}

impl crate::DomainStorage for ResidentData {
    fn element_count(&self)->u32 {match self {
        Self::Cpu(data)=>crate::DomainStorage::element_count(data),
        Self::CpuBorrow(data)=>data.count,
        Self::Gpu(data)=>crate::DomainStorage::element_count(data),
    }}
    fn read_identity(&self)->Option<usize> {match self {
        Self::Cpu(data)=>crate::DomainStorage::read_identity(data),
        Self::CpuBorrow(_)=>None,
        Self::Gpu(data)=>crate::DomainStorage::read_identity(data),
    }}
    fn read_binding(&self)->Option<Self> {match self {
        Self::Cpu(data)=>crate::DomainStorage::read_binding(data).map(Self::Cpu),
        Self::CpuBorrow(_)=>None,
        Self::Gpu(data)=>crate::DomainStorage::read_binding(data).map(Self::Gpu),
    }}
}

impl crate::DomainStorage for &mut ResidentData {
    fn element_count(&self) -> u32 {
        crate::DomainStorage::element_count(&**self)
    }
    fn read_identity(&self) -> Option<usize> {
        crate::DomainStorage::read_identity(&**self)
    }
    fn read_binding(&self) -> Option<Self> {
        None
    }
}
