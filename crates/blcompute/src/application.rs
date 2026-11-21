//! B 应用拥有动态数据；原 Dataset、域关系及 GPU 最后使用点仍拥有物理资源。
use crate::{
    Dataset, DomainBindings, Result, Value,
    bindings::{self, DomainRelations},
    cpu::{CpuDataset, CpuTemplate},
    error, vulkan,
};
use std::{ptr::NonNull, sync::Arc};
use blanguage::{
    TemplateProgram,
    ir::{Buffer, ExecutionPlan, Ty},
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
        mut cpu: impl FnMut(blanguage::native::ProgramImage<TemplateProgram, usize>, &[*const u8]) -> Result<CpuTemplate>,
    ) -> Result<Self> {
        let mut types = Vec::new();
        let mut programs = Vec::new();
        for factory in factories {
            let image = unsafe { factory() };
            let bytes = unsafe { std::slice::from_raw_parts(image.data, image.bytes) };
            let code = unsafe { std::slice::from_raw_parts(image.code, image.codes) };
            let bind = |image| -> Result<()> {
                programs.push(match image {
                    blanguage::resource::Image::Cpu(image) => ProgramSlot::Cpu(cpu(image, code)?),
                    blanguage::resource::Image::Gpu(image) => ProgramSlot::Gpu(Arc::new(image)),
                });
                Ok(())
            };
            unsafe { blanguage::resource::decode_linked_module_into(bytes, &mut types, bind) }
                .map_err(|e| error(e.to_string()))??;
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

impl ResidentData {
    fn storage_address(&mut self) -> u64 {
        match self {
            Self::Cpu(CpuDataset::Owned(data)) => data.words.as_mut_ptr() as usize as u64,
            Self::Cpu(CpuDataset::Read(data)) => data.words().as_ptr() as usize as u64,
            Self::CpuBorrow(data) => data.pointer as usize as u64,
            Self::Gpu(data) => data.storage_address(),
        }
    }
}

/// 同一Frame可见CPU数据存储的同步快照；多个句柄只列一次物理Dataset。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameCpuDataMemory {
    pub handles: Vec<u32>,
    pub memory: crate::DatasetMemory,
}

pub(crate) struct FrameArguments<'a> {
    bindings: &'a [DatasetBinding],
    by_handle: Vec<usize>,
    slots: Vec<usize>,
    owner_count: usize,
}

impl<'a> FrameArguments<'a> {
    fn new(bindings: &'a [DatasetBinding], parameters: &[Buffer]) -> Result<Self> {
        use blanguage::ast::Access::{Atomic, Read};

        let mut by_handle: Vec<_> = (0..bindings.len()).collect();
        // 同一起点的零宽字段先处理，保留原字节边界比较。
        by_handle.sort_unstable_by_key(|&i| (bindings[i].handle, bindings[i].offset, parameters[i].ty.size));
        let mut slots = vec![0; bindings.len()];
        for group in by_handle.chunk_by(|&a, &b| bindings[a].handle == bindings[b].handle) {
            let first = *group.iter().min().unwrap();
            let (mut read_end, mut atomic_end, mut write_end) = (0u64, 0u64, 0u64);
            for &i in group {
                let start = u64::from(bindings[i].offset);
                let (conflict, end) = match parameters[i].access {
                    Read => (atomic_end.max(write_end), &mut read_end),
                    Atomic => (read_end.max(write_end), &mut atomic_end),
                    _ => (read_end.max(atomic_end).max(write_end), &mut write_end),
                };
                if start < conflict {
                    return Err(error("同一存储重叠区域的重复参数必须同为 read 或同为 atomic"));
                }
                *end = (*end).max(start + u64::from(parameters[i].ty.size));
                slots[i] = first;
            }
        }

        // 保留本次已排序的实参索引，供按原句柄查找及归并同一字段。
        // 紧凑槽按实参首次出现编号。
        let mut next = 0;
        for i in 0..slots.len() {
            let first = slots[i];
            if first == i {
                slots[i] = next;
                next += 1;
            } else {
                slots[i] = slots[first];
            }
        }
        Ok(Self { bindings, by_handle, slots, owner_count: next })
    }

    fn selected_handles(&self) -> impl Iterator<Item = usize> + '_ {
        let mut next = 0;
        self.bindings.iter().zip(&self.slots).filter_map(move |(binding, &slot)| {
            (slot == next).then(|| {
                next += 1;
                binding.handle as usize
            })
        })
    }

    pub(crate) fn selected(&self, original: usize) -> Option<usize> {
        self.by_handle.binary_search_by_key(&original, |&i| self.bindings[i].handle as usize)
            .ok().map(|i| self.slots[self.by_handle[i]])
    }

    pub(crate) fn location(&self, i: usize) -> (usize, u32) {
        (self.slots[i], self.bindings[i].offset)
    }

    pub(crate) fn projected_fields(&self) -> impl Iterator<Item = &[usize]> + Clone {
        self.by_handle
            .chunk_by(|&a, &b| self.original(a) == self.original(b))
            .filter(|group| self.bindings[group[0]].offset != 0)
    }

    pub(crate) fn original(&self, i: usize) -> (usize, u32) {
        let binding = &self.bindings[i];
        (binding.handle as usize, binding.offset)
    }
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
        let definitions = Arc::new(Definitions::new(types, images)?);
        Ok(unsafe { Self::from_definitions(definitions, &[], None) }.with_gpu_program_slots())
    }
    /// 从同次静态链接的真实模块工厂创建应用Frame。
    ///
    /// # Safety
    /// 工厂、字节与代码属于同次可信链接，顺序对应原模块基址；
    /// 原模块及依赖保持到最后真实代码、存储及设备消费者结束。
    pub unsafe fn from_linked_modules(factories: &[blanguage::resource::ModuleFactory]) -> Result<Self> {
        unsafe { Self::linked_frame(factories) }.map(Self::with_gpu_program_slots)
    }

    unsafe fn linked_frame(factories: &[blanguage::resource::ModuleFactory]) -> Result<Self> {
        let definitions = unsafe {
            Definitions::linked(factories, |image, code| CpuTemplate::from_linked_indices(image, code))
        }?;
        Ok(unsafe { Self::from_definitions(Arc::new(definitions), &[], None) })
    }

    fn with_gpu_program_slots(mut self) -> Self {
        // Rust observers use the original program number even before their GPU slot is installed.
        self.gpu_programs = (0..self.definitions.programs.len()).map(|_| None).collect();
        self
    }
    /// 本次范围独占局部存储，只借用原模块已绑定的同一不可变定义。
    /// # Safety
    /// inputs的实际存储、布局、权限和借用资源保持到本Frame最后消费者完成；本值不取得其释放权。
    /// input_domains来自已接受的实际输入合同，取得写访问后仍有效；索引对应同一inputs。
    pub(crate) unsafe fn from_definitions(
        definitions: Arc<Definitions>, inputs: &[crate::cpu::CpuBuffer<'_>],
        input_domains: Option<&Arc<Vec<crate::DomainRelation>>>,
    ) -> Self {
        let datasets=inputs.iter().map(|data|Some(ResidentData::CpuBorrow(CpuBorrow{
            layout:CpuBorrowLayout::Range(NonNull::from(data.ty)),stride:data.stride,count:data.count,pointer:data.pointer,writable:data.writable,
        }))).collect();
        let domains = input_domains.map_or_else(DomainRelations::default, |domains| {
            DomainRelations::Shared(Arc::clone(domains))
        });
        Self {
            data: DomainBindings { datasets, domains },
            gpu_programs: Vec::new(),
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
    fn load_gpu<'a>(
        definitions: &Definitions,
        slots: &'a mut Vec<Option<vulkan::GpuTemplate>>,
        device: &mut Option<vulkan::Device>,
        index: usize,
        image: &blanguage::spirv::ProgramImage<TemplateProgram>,
    ) -> Result<&'a vulkan::GpuTemplate> {
        if slots.get(index).is_none_or(Option::is_none) {
            let device = Self::gpu_device(definitions, device)?;
            let program = device.load_template_ref(image)?;
            let len = slots.len().max(index + 1);
            slots.try_reserve_exact(len - slots.len())?;
            slots.resize_with(len, || None);
            slots[index] = Some(program);
        }
        Ok(slots[index].as_ref().unwrap())
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
        self.data.domains.sort_unique();
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
        self.data.datasets[handle as usize].as_mut().unwrap().storage_address()
    }
    /// CPU 同步执行借用原帧槽，沿原句柄验证、撤回实际覆盖的事实并发布结果。
    /// GPU 同步记录同样借用原帧槽，由实际数据保留最后使用点。
    /// 队列接受后撤回实际写入覆盖的关系，等待成功才发布新保证。
    pub fn run(&mut self, program: u32, bindings: &[DatasetBinding], values: &[Value], textures: &[crate::cpu::CpuTextureBinding<'_>]) -> Result<()> {
        let index = program as usize;
        let definitions = &self.definitions;
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
        let arguments = FrameArguments::new(bindings, layout.buffers())?;
        match definition {
            ProgramSlot::Cpu(program) => {
                let mut datasets = Vec::with_capacity(arguments.owner_count);
                let slots = self.data.datasets.as_mut_ptr();
                for handle in arguments.selected_handles() {
                    // Argument validation indexes every handle. Selection above records
                    // each slot once, and the slot array stays in place through the join.
                    let data = unsafe { &mut *slots.add(handle) }.as_mut().unwrap();
                    if matches!(data, ResidentData::Gpu(_)) {
                        return Err(error("CPU 访问需要先显式 download 数据"));
                    }
                    datasets.push(data);
                }
                let result = program.run_projected(&mut datasets, &mut self.data.domains,
                    &arguments, textures, uniforms, definitions);
                if result.is_err() {
                    self.data.domains.sort_unique();
                }
                result
            }
            ProgramSlot::Gpu(image) => {
                if !textures.is_empty() {
                    return Err(error("CPU纹理借用不能作为GPU驻留资源"));
                }
                if cpu_resident {
                    return Err(error("GPU 执行需要先显式 upload 数据"));
                }
                let mut datasets = Vec::with_capacity(arguments.owner_count);
                let slots = self.data.datasets.as_mut_ptr();
                for handle in arguments.selected_handles() {
                    // Selection records disjoint slots and the slot array stays in place.
                    let Some(ResidentData::Gpu(data)) = (unsafe { &mut *slots.add(handle) }) else {
                        unreachable!("GPU residency was checked during argument binding");
                    };
                    datasets.push(data);
                }
                let storage_slots = bindings::StorageSlots::Frame(&arguments);
                bindings::require(|slot| datasets.get(slot).map(|data| &**data),
                    &self.data.domains, layout, storage_slots)
                    .inspect_err(|_| self.data.domains.sort_unique())?;
                let gpu_program = Self::load_gpu(definitions, &mut self.gpu_programs, &mut self.device, index, image)?;
                let result = gpu_program.submit_projected(&mut datasets, &arguments, uniforms).and_then(|completion| {
                    let result = completion.wait_bound(datasets.iter_mut().map(|data| &mut **data))
                        .map(|_| ());
                    let original = |i| arguments.original(i);
                    bindings::invalidate(&mut self.data.domains, &layout.write_regions, original);
                    if result.is_ok() {
                        bindings::produce(&mut self.data.domains, layout, original,
                            |i| datasets[arguments.location(i).0].count());
                    }
                    result
                });
                if result.is_err() {
                    self.data.domains.sort_unique();
                }
                result
            }
        }
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
