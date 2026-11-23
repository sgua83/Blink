//! Vulkan KHR 加速结构。构建输入只活到构建完成，TLAS 持有真实 BLAS 引用。
use super::*;

#[repr(C)]
pub(super) struct AccelerationFeatures {
    pub kind: u32,
    pub next: *mut c_void,
    pub acceleration: u32,
    pub capture: u32,
    pub indirect: u32,
    pub host: u32,
    pub update_after_bind: u32,
}
#[repr(C)]
pub(super) struct QueryFeatures {
    pub kind: u32,
    pub next: *mut c_void,
    pub ray_query: u32,
}
#[repr(C)]
pub(super) struct PositionFetchFeatures {
    pub kind: u32,
    pub next: *mut c_void,
    pub position_fetch: u32,
}
#[repr(C)]
pub(super) struct Properties {
    pub kind: u32,
    pub next: *mut c_void,
    pub max_geometry: u64,
    pub max_instances: u64,
    pub max_primitives: u64,
    pub per_stage: u32,
    pub per_stage_update: u32,
    pub per_set: u32,
    pub per_set_update: u32,
    pub scratch_alignment: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Triangles {
    kind: u32,
    next: *const c_void,
    format: u32,
    vertices: u64,
    stride: u64,
    max_vertex: u32,
    index_type: u32,
    indices: u64,
    transform: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Instances {
    kind: u32,
    next: *const c_void,
    pointers: u32,
    data: u64,
}
#[repr(C)]
union GeometryData {
    triangles: Triangles,
    instances: Instances,
}
#[repr(C)]
struct Geometry {
    kind: u32,
    next: *const c_void,
    geometry_type: u32,
    data: GeometryData,
    flags: u32,
}
#[repr(C)]
struct Build {
    kind: u32,
    next: *const c_void,
    structure_type: u32,
    flags: u32,
    mode: u32,
    source: u64,
    target: u64,
    geometry_count: u32,
    geometries: *const Geometry,
    pointers: *const *const Geometry,
    scratch: u64,
}
#[repr(C)]
struct Sizes {
    kind: u32,
    next: *const c_void,
    structure: u64,
    update_scratch: u64,
    build_scratch: u64,
}
#[repr(C)]
struct Create {
    kind: u32,
    next: *const c_void,
    flags: u32,
    buffer: u64,
    offset: u64,
    size: u64,
    structure_type: u32,
    address: u64,
}
#[repr(C)]
struct Range {
    primitive_count: u32,
    primitive_offset: u32,
    first_vertex: u32,
    transform_offset: u32,
}
#[repr(C)]
struct AddressInfo {
    kind: u32,
    next: *const c_void,
    structure: u64,
}
#[repr(C)]
pub(super) struct WriteAcceleration {
    pub kind: u32,
    pub next: *const c_void,
    pub count: u32,
    pub structures: *const u64,
}
pub(super) struct RayDevice {
    create: unsafe extern "system" fn(VkDevice, *const Create, *const c_void, *mut u64) -> i32,
    destroy: unsafe extern "system" fn(VkDevice, u64, *const c_void),
    sizes: unsafe extern "system" fn(VkDevice, u32, *const Build, *const u32, *mut Sizes),
    build: unsafe extern "system" fn(VkCommandBuffer, u32, *const Build, *const *const Range),
    address: unsafe extern "system" fn(VkDevice, *const AddressInfo) -> u64,
    pub properties: Properties,
}
impl RayDevice {
    pub unsafe fn load(device: VkDevice, get: Gdpa, properties: Properties) -> Result<Self> {
        unsafe {
            Ok(Self {
                create: function(
                    get(device, c"vkCreateAccelerationStructureKHR".as_ptr()),
                    c"vkCreateAccelerationStructureKHR",
                )?,
                destroy: function(
                    get(device, c"vkDestroyAccelerationStructureKHR".as_ptr()),
                    c"vkDestroyAccelerationStructureKHR",
                )?,
                sizes: function(
                    get(device, c"vkGetAccelerationStructureBuildSizesKHR".as_ptr()),
                    c"vkGetAccelerationStructureBuildSizesKHR",
                )?,
                build: function(
                    get(device, c"vkCmdBuildAccelerationStructuresKHR".as_ptr()),
                    c"vkCmdBuildAccelerationStructuresKHR",
                )?,
                address: function(
                    get(
                        device,
                        c"vkGetAccelerationStructureDeviceAddressKHR".as_ptr(),
                    ),
                    c"vkGetAccelerationStructureDeviceAddressKHR",
                )?,
                properties,
            })
        }
    }
}
pub(super) const BUILD_INPUT_READ_ONLY: u32 = 0x80000;

pub(super) struct StructureStorage {
    handle: u64,
    allocation: Allocation,
    models: Option<Rc<[BottomLevel]>>,
    count: u32,
    flags: u32,
    max_vertex: Option<u32>,
}
impl StructureStorage {
    fn belongs_to(&self, inner: &Rc<Inner>) -> bool {
        Rc::ptr_eq(&self.allocation.inner, inner)
    }
}
impl Drop for StructureStorage {
    fn drop(&mut self) {
        if self.handle != 0 {
            unsafe {
                let inner = &self.allocation.inner;
                (inner.ray.as_ref().unwrap().destroy)(
                    inner.owner.device,
                    self.handle,
                    null(),
                );
            }
        }
    }
}
// 输出持有自己的完成点；最后一个引用退出时，尚在构建的存储交回原执行。
pub(super) struct Structure {
    storage: ManuallyDrop<StructureStorage>,
    built: Rc<Execution>,
    instance_address: u64,
}
impl std::ops::Deref for Structure {
    type Target = StructureStorage;
    fn deref(&self) -> &StructureStorage {
        &self.storage
    }
}
impl Drop for Structure {
    fn drop(&mut self) {
        let storage = unsafe { ManuallyDrop::take(&mut self.storage) };
        self.built.discard_structure(storage);
    }
}
struct BuildInputs {
    commands: Commands,
    allocations: Vec<Allocation>,
    dependencies: DependencySources,
}
impl BuildInputs {
    fn new(inner: &Rc<Inner>) -> Result<Self> {
        Ok(Self {
            commands: Commands::new(inner.clone())?,
            allocations: Vec::new(),
            dependencies: DependencySources::default(),
        })
    }
}
#[derive(Clone)]
pub struct BottomLevel(Rc<Structure>);
impl BottomLevel {
    /// 实际BLAS构建输入的primitive范围，不读取设备计数。
    pub fn primitive_count(&self) -> u32 {
        self.0.count
    }
    pub fn is_complete(&self) -> Result<bool> {
        self.0.built.is_complete()
    }
    pub fn wait(&self) -> Result<()> {
        self.0.built.wait()
    }
    fn instance_tail(&self) -> [u32; 4] {
        let address = self.0.instance_address;
        [0xff000000, 0, address as u32, (address >> 32) as u32]
    }
}
pub(super) struct InstanceInput {
    pub(super) pointers: Option<Allocation>,
    models: Option<Rc<[BottomLevel]>>,
}
fn collect_models(models: impl IntoIterator<Item = BottomLevel>) -> Option<Rc<[BottomLevel]>> {
    let unique = models.into_iter()
        .map(|model| (model.0.handle, model))
        .collect::<BTreeMap<_, _>>();
    (!unique.is_empty()).then(|| unique.into_values().collect())
}
pub struct RayInstance {
    pub model: BottomLevel,
    /// 行主序仿射 3x4，与 Vulkan instance transform 相同。
    pub transform: [f32; 12],
}
#[repr(C)]
struct InstanceRecord {
    transform: [f32; 12],
    tail: [u32; 4],
}
#[derive(Clone)]
pub struct Acceleration(Rc<Structure>);
enum BuildOperation<'a> {
    Static,
    Updatable,
    Refit(&'a Rc<Structure>),
}
impl Acceleration {
    pub fn is_complete(&self) -> Result<bool> {
        self.0.built.is_complete()
    }
    pub fn wait(&self) -> Result<()> {
        self.0.built.wait()
    }
    pub(super) fn produced(&self) -> &Rc<Execution> {
        &self.0.built
    }
    pub fn allocated_bytes(&self) -> u64 {
        self.0.allocation.range_bytes()
            + self
                .0
                .models
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|m| m.0.allocation.range_bytes())
                .sum::<u64>()
    }
    pub(super) fn handle(&self) -> &u64 {
        &self.0.handle
    }
    pub(super) fn belongs_to(&self, inner: &Rc<Inner>) -> bool {
        self.0.belongs_to(inner)
    }
}
impl Device {
    fn stage_ray_input(
        &self,
        bytes: u64,
        alignment: u64,
        write: impl FnOnce(&Allocation) -> Result<()>,
        inputs: &mut BuildInputs,
    ) -> Result<Allocation> {
        let allocation = Allocation::new(self.inner.clone(), bytes, 0xa0002, 1, alignment)?;
        self.stage_page(&allocation, bytes, write, &mut inputs.commands, &mut inputs.allocations)?;
        Ok(allocation)
    }
    fn device_geometry_address(&self, data: &GpuDataset, inputs: &mut BuildInputs) -> Result<(u64, Usage)> {
        if data.count != 0 && let Some(previous) = data.dependency() {
            inputs.dependencies.insert(previous);
        }
        if let [page] = data.pages.as_slice() {
            return Ok((page.address(), data.producer));
        }

        let alignment = u64::from(data.ty.align).max(4);
        let allocation = Allocation::new(self.inner.clone(), data.bytes(), 0xa0002, 1, alignment)?;
        if !data.pages.is_empty() {
            let commands = &inputs.commands;
            commands.barrier(data.producer.stage(), 0x1000, data.producer.access(), 0x800);
            let mut offset = 0;
            for page in &data.pages {
                let copy = VkBufferCopy {
                    srcOffset: 0,
                    dstOffset: offset,
                    size: page.size,
                };
                unsafe {
                    (self.inner.functions.vkCmdCopyBuffer)(
                        commands.command,
                        page.buffer,
                        allocation.buffer,
                        1,
                        &copy,
                    );
                }
                offset += page.size;
            }
        }
        let address = allocation.address();
        inputs.allocations.push(allocation);
        Ok((address, Usage::Transfer))
    }
    fn build_acceleration(
        &self,
        count: u32,
        max_vertex: Option<u32>,
        operation: BuildOperation<'_>,
        prepare: impl FnOnce(&mut BuildInputs) -> Result<(Geometry, Usage, Option<Rc<[BottomLevel]>>)>,
    ) -> Result<Structure> {
        let bottom_level = max_vertex.is_some();
        let structure_type = u32::from(bottom_level);
        let ray = self
            .inner
            .ray
            .as_ref()
            .ok_or_else(|| error("设备未声明硬件光线查询能力"))?;
        let limit = if structure_type == 0 {
            ray.properties.max_instances
        } else {
            ray.properties.max_primitives
        };
        if count as u64 > limit {
            return Err(error("加速结构的 primitive／instance 数量超出设备能力"));
        }
        let static_flags = 4 | if self.inner.position_fetch && bottom_level {
            0x800
        } else {
            0
        };
        let (flags, source) = match operation {
            BuildOperation::Static => (static_flags, None),
            BuildOperation::Updatable => (static_flags | 1, None),
            BuildOperation::Refit(source) => {
                if !source.belongs_to(&self.inner) {
                    return Err(error("refit 来源不属于目标设备"));
                }
                if source.flags & 1 == 0 {
                    return Err(error("refit 来源构建时未声明 ALLOW_UPDATE"));
                }
                if source.count != count || source.max_vertex != max_vertex {
                    return Err(error(
                        "refit 必须保持原 primitive／instance 数量与 maxVertex",
                    ));
                }
                (source.flags, Some(source.clone()))
            }
        };
        let mut inputs = BuildInputs::new(&self.inner)?;
        let (geometry, producer, models) = prepare(&mut inputs)?;
        let BuildInputs { commands, allocations, mut dependencies } = inputs;
        for previous in models.as_deref().unwrap_or_default().iter().map(|model| &model.0.built)
            .chain(source.iter().map(|source| &source.built))
        {
            dependencies.insert(previous);
        }
        let dependencies = dependencies.prepare()?;
        let mut info = Build {
            kind: 1000150000,
            next: null(),
            structure_type,
            flags,
            mode: u32::from(source.is_some()),
            source: source.as_ref().map_or(0, |source| source.handle),
            target: 0,
            geometry_count: 1,
            geometries: &geometry,
            pointers: null(),
            scratch: 0,
        };
        let mut sizes = Sizes {
            kind: 1000150020,
            next: null(),
            structure: 0,
            update_scratch: 0,
            build_scratch: 0,
        };
        unsafe {
            (ray.sizes)(self.inner.owner.device, 1, &info, &count, &mut sizes);
        }
        let storage = Allocation::new(self.inner.clone(), sizes.structure, 0x120000, 1, 1)?;
        let mut out = StructureStorage {
            handle: 0,
            allocation: storage,
            models,
            count,
            flags,
            max_vertex,
        };
        let ci = Create {
            kind: 1000150017,
            next: null(),
            flags: 0,
            buffer: out.allocation.buffer,
            offset: 0,
            size: sizes.structure,
            structure_type,
            address: 0,
        };
        unsafe {
            check(
                (ray.create)(self.inner.owner.device, &ci, null(), &mut out.handle),
                "vkCreateAccelerationStructureKHR",
            )?;
        }
        let alignment = ray.properties.scratch_alignment as u64;
        let scratch = Allocation::new(
            self.inner.clone(),
            if source.is_some() {
                sizes.update_scratch
            } else {
                sizes.build_scratch
            } + alignment
                - 1,
            0x20020,
            1,
            1,
        )?;
        info.target = out.handle;
        info.scratch = (scratch.address() + alignment - 1) & !(alignment - 1);
        let range = Range {
            primitive_count: count,
            primitive_offset: 0,
            first_vertex: 0,
            transform_offset: 0,
        };
        let ranges = &range as *const Range;
        commands.barrier(
            producer.stage() | 0x1000 | 0x2000000,
            0x2000000,
            producer.access() | 0x1000 | 0x400000,
            0x200020,
        );
        unsafe {
            (ray.build)(commands.command, 1, &info, &ranges);
        }
        let built = Execution::submit(
            ExecutionResources::Acceleration {
                commands,
                _scratch: scratch,
                _inputs: allocations,
                _source: source,
                retired: None,
            },
            dependencies,
        )?;
        let instance_address = if bottom_level {
            let address = AddressInfo {
                kind: 1000150002,
                next: null(),
                structure: out.handle,
            };
            unsafe { (ray.address)(self.inner.owner.device, &address) }
        } else {
            0
        };
        Ok(Structure {
            storage: ManuallyDrop::new(out),
            built,
            instance_address,
        })
    }
    /// 输入索引来自已检查拓扑的资产；返回前复制到自有传输存储，构建资源沿真实完成点持有。
    /// # Safety
    /// 所有三角形索引必须落在 positions 中，位置必须为有限值。
    pub unsafe fn bottom_level(
        &self,
        positions: &[[f32; 3]],
        indices: &[[u32; 3]],
    ) -> Result<BottomLevel> {
        unsafe { self.bottom_level_operation(positions, indices, BuildOperation::Static) }
    }
    /// 显式允许后续 refit，其他输入及完成语义与 bottom_level 相同。
    /// # Safety
    /// 索引须落在 positions 内，位置必须有限。
    pub unsafe fn bottom_level_updatable(
        &self,
        positions: &[[f32; 3]],
        indices: &[[u32; 3]],
    ) -> Result<BottomLevel> {
        unsafe { self.bottom_level_operation(positions, indices, BuildOperation::Updatable) }
    }
    /// UPDATE 到新的独立 BLAS；旧结构及其查询保持原快照。
    /// # Safety
    /// 位置必须有限，索引须引用本次 positions；进入 UPDATE 的索引内容须与来源相同。
    /// 数量或 maxVertex 不符由资源入口报告。
    pub unsafe fn refit_bottom_level(
        &self,
        source: &BottomLevel,
        positions: &[[f32; 3]],
        indices: &[[u32; 3]],
    ) -> Result<BottomLevel> {
        unsafe { self.bottom_level_operation(positions, indices, BuildOperation::Refit(&source.0)) }
    }
    unsafe fn bottom_level_operation(
        &self,
        positions: &[[f32; 3]],
        indices: &[[u32; 3]],
        operation: BuildOperation<'_>,
    ) -> Result<BottomLevel> {
        self.bottom_level_inputs(
            12,
            u32::try_from(positions.len().saturating_sub(1))?,
            u32::try_from(indices.len())?,
            operation,
            |inputs| {
                let vertices = self.stage_ray_input(
                    std::mem::size_of_val(positions) as u64,
                    4,
                    |source| source.write_with(positions, |_| {}),
                    inputs,
                )?;
                let triangles = self.stage_ray_input(
                    std::mem::size_of_val(indices) as u64,
                    4,
                    |source| source.write_with(indices, |_| {}),
                    inputs,
                )?;
                let addresses = [vertices.address(), triangles.address()];
                inputs.allocations.extend([vertices, triangles]);
                Ok((addresses, Usage::Transfer))
            },
        )
    }
    /// 在本次构建命令内复制 GPU 顶点和索引，源存储保持到构建完成。
    /// 顶点开头为三个 f32，沿实际记录 stride 取样；三角形为紧密的三个 u32。
    /// 消费完整记录域；冻结来源可直接传入 binding()，不读取有效前缀计数。
    /// # Safety
    /// 全部记录已初始化，位置有限，索引均落在本次顶点域内。
    pub unsafe fn bottom_level_device(
        &self,
        vertices: GpuDataset,
        triangles: GpuDataset,
    ) -> Result<BottomLevel> {
        self.bottom_level_device_operation(vertices, triangles, BuildOperation::Static)
    }
    /// GPU 几何直接构建显式 ALLOW_UPDATE 的 BLAS。
    /// # Safety
    /// 与 bottom_level_device 相同的初始化、有限位置与索引前提。
    pub unsafe fn bottom_level_device_updatable(
        &self,
        vertices: GpuDataset,
        triangles: GpuDataset,
    ) -> Result<BottomLevel> {
        self.bottom_level_device_operation(vertices, triangles, BuildOperation::Updatable)
    }
    /// GPU 几何 UPDATE 到独立 BLAS，旧结构仍保持原快照。
    /// # Safety
    /// 与 bottom_level_device 相同；索引内容／顺序及 primitive 活动状态须与来源相同。
    pub unsafe fn refit_bottom_level_device(
        &self,
        source: &BottomLevel,
        vertices: GpuDataset,
        triangles: GpuDataset,
    ) -> Result<BottomLevel> {
        self.bottom_level_device_operation(vertices, triangles, BuildOperation::Refit(&source.0))
    }
    fn bottom_level_device_operation(
        &self,
        mut vertices: GpuDataset,
        mut triangles: GpuDataset,
        operation: BuildOperation<'_>,
    ) -> Result<BottomLevel> {
        use blanguage::ir::Scalar::{F32, U32};
        if !Rc::ptr_eq(&self.inner, &vertices.allocation.inner)
            || !Rc::ptr_eq(&self.inner, &triangles.allocation.inner)
        {
            return Err(error("几何输入不属于目标设备"));
        }
        if !vertices
            .ty
            .components
            .starts_with(&[(F32, 0), (F32, 4), (F32, 8)])
        {
            return Err(error("BLAS 顶点需要 offset 0 的三个 f32 位置分量"));
        }
        if triangles.stride != 12 || triangles.ty.components != [(U32, 0), (U32, 4), (U32, 8)] {
            return Err(error("BLAS 三角形需要紧密的三个 u32 索引，stride 为 12"));
        }
        let stride = vertices.stride as u64;
        let max_vertex = vertices.count.saturating_sub(1);
        let count = triangles.count;
        let result = self.bottom_level_inputs(stride, max_vertex, count, operation, |inputs| {
            let (vertex_address, vertex_producer) = self.device_geometry_address(&vertices, inputs)?;
            let (index_address, index_producer) = self.device_geometry_address(&triangles, inputs)?;
            let producer = match (vertex_producer, index_producer) {
                (Usage::Transfer, Usage::Transfer) => Usage::Transfer,
                _ => Usage::Compute,
            };
            Ok(([vertex_address, index_address], producer))
        })?;
        for data in [&mut vertices, &mut triangles] {
            if data.count != 0 {
                data.last_use = Some(result.0.built.clone());
            }
        }
        Ok(result)
    }
    fn bottom_level_inputs(
        &self,
        stride: u64,
        max_vertex: u32,
        count: u32,
        operation: BuildOperation<'_>,
        prepare: impl FnOnce(&mut BuildInputs) -> Result<([u64; 2], Usage)>,
    ) -> Result<BottomLevel> {
        self.build_acceleration(count, Some(max_vertex), operation, |inputs| {
            let ([vertices, indices], producer) = prepare(inputs)?;
            let geometry = Geometry {
                kind: 1000150006,
                next: null(),
                geometry_type: 0,
                flags: 1,
                data: GeometryData {
                    triangles: Triangles {
                        kind: 1000150005,
                        next: null(),
                        format: 106,
                        vertices,
                        stride,
                        max_vertex,
                        index_type: 1,
                        indices,
                        transform: 0,
                    },
                },
            };
            Ok((geometry, producer, None))
        })
        .map(|s| BottomLevel(Rc::new(s)))
    }
    /// # Safety
    /// 实例仿射矩阵必须为有限值且可逆；模型由本设备的 BLAS 构造入口提供。
    pub unsafe fn top_level(&self, instances: Vec<RayInstance>) -> Result<Acceleration> {
        unsafe { self.top_level_operation(instances, BuildOperation::Static) }
    }
    /// 显式允许后续 refit。
    /// # Safety
    /// 与 top_level 相同，全部实例矩阵必须有限且可逆。
    pub unsafe fn top_level_updatable(&self, instances: Vec<RayInstance>) -> Result<Acceleration> {
        unsafe { self.top_level_operation(instances, BuildOperation::Updatable) }
    }
    /// UPDATE 到独立 TLAS，允许交接新的实际 BLAS 引用及实例变换。
    /// # Safety
    /// 全部实例矩阵必须有限且可逆，实例保持原活动状态。
    pub unsafe fn refit_top_level(
        &self,
        source: &Acceleration,
        instances: Vec<RayInstance>,
    ) -> Result<Acceleration> {
        unsafe { self.top_level_operation(instances, BuildOperation::Refit(&source.0)) }
    }
    unsafe fn top_level_operation(
        &self,
        instances: Vec<RayInstance>,
        operation: BuildOperation<'_>,
    ) -> Result<Acceleration> {
        if instances.iter().any(|instance| !instance.model.0.belongs_to(&self.inner)) {
            return Err(error("BLAS 不属于目标设备"));
        }
        let count = u32::try_from(instances.len())?;
        let structure = self.build_acceleration(count, None, operation, |inputs| {
            let bytes = u64::from(count) * std::mem::size_of::<InstanceRecord>() as u64;
            let input = self.stage_ray_input(bytes, 16, |source| {
                let records = source.mapping()?.cast::<InstanceRecord>();
                // The mapped range has one full native record for each admitted instance.
                for (index, instance) in instances.iter().enumerate() {
                    unsafe {
                        records.add(index).write(InstanceRecord {
                            transform: instance.transform,
                            tail: instance.model.instance_tail(),
                        });
                    }
                }
                Ok(())
            }, inputs)?;
            let geometry = Geometry {
                kind: 1000150006,
                next: null(),
                geometry_type: 2,
                flags: 0,
                data: GeometryData {
                    instances: Instances {
                        kind: 1000150004,
                        next: null(),
                        pointers: 0,
                        data: input.address(),
                    },
                },
            };
            let models = collect_models(instances.into_iter().map(|instance| instance.model));
            inputs.allocations.push(input);
            Ok((geometry, Usage::Transfer, models))
        })?;
        Ok(Acceleration(Rc::new(structure)))
    }
    /// 创建 Vulkan 实例输入，B 仅声明 offset 0..48 的 12 个 f32 与 stride(64)。
    /// 原生尾部、页内地址及 BLAS 引用随唯一物理存储转移，不暴露为 B 字段。
    pub fn instance_data(
        &self,
        program: &dyn ProgramInterface,
        binding: usize,
        models: Vec<BottomLevel>,
    ) -> Result<GpuDataset> {
        let count = u32::try_from(models.len())?;
        let schema = program.buffers().get(binding).ok_or_else(|| error("数据绑定编号越界"))?;
        let stride = blanguage::ir::align_up(schema.ty.size, schema.ty.align);
        self.upload_instance_data(
            schema.ty.clone(), stride, count, std::iter::repeat([0.0; 12]), models,
        )
    }
    /// CPU矩阵与B产生的矩阵共用实际64字节实例输入，原生尾部仍由平台填写。
    pub fn instance_data_from(
        &self,
        data: Dataset,
        models: Vec<BottomLevel>,
    ) -> Result<GpuDataset> {
        let transforms = data.words.chunks_exact(16).map(|record| {
            std::array::from_fn(|index| f32::from_bits(record[index]))
        });
        self.upload_instance_data(data.ty, data.stride, data.count, transforms, models)
    }
    fn upload_instance_data(
        &self,
        ty: Ty,
        data_stride: u32,
        count: u32,
        transforms: impl Iterator<Item = [f32; 12]>,
        models: Vec<BottomLevel>,
    ) -> Result<GpuDataset> {
        if count as usize != models.len() {
            return Err(error("矩阵记录与实际BLAS实例数量不同"));
        }
        let components = (0..12).map(|i| (blanguage::ir::Scalar::F32, i * 4));
        let stride = std::mem::size_of::<InstanceRecord>();
        if data_stride as usize != stride || !ty.components.iter().copied().eq(components) {
            return Err(error(
                "Vulkan 实例输入需要 64 字节 stride 与 offset 0 的 3×4 f32 矩阵",
            ));
        }
        if models.iter().any(|model| !model.0.belongs_to(&self.inner)) {
            return Err(error("BLAS 不属于目标设备"));
        }
        let mut target = GpuDataset::allocate(
            self.inner.clone(), ty, data_stride, count, BUILD_INPUT_READ_ONLY, 16,
        )?;
        let pointer_bytes = u64::from(count) * 8;
        // A single page already holds the packed native array. Empty builds
        // still need a valid build-input address, supplied by the pointer buffer.
        let pointers = (target.pages.len() != 1)
            .then(|| Allocation::new(self.inner.clone(), pointer_bytes, 0xa0002, 1, 8))
            .transpose()?;
        let transfer = if count == 0 {
            None
        } else {
            let mut commands = Commands::new(self.inner.clone())?;
            let mut staging = Vec::new();
            let mut records = transforms.zip(models.iter());
            for page in &target.pages {
                self.stage_page(
                    page,
                    page.size,
                    |source| {
                        let mapped = source.mapping()?.cast::<InstanceRecord>();
                        let count = page.size as usize / stride;
                        for (index, (transform, model)) in records.by_ref().take(count).enumerate() {
                            unsafe {
                                mapped.add(index).write(InstanceRecord {
                                    transform,
                                    tail: model.instance_tail(),
                                });
                            }
                        }
                        Ok(())
                    },
                    &mut commands,
                    &mut staging,
                )?;
            }
            if let Some(pointers) = &pointers {
                self.stage_page(
                    pointers,
                    pointer_bytes,
                    |source| {
                        let mut entry = source.mapping()?.cast::<[u32; 2]>();
                        // Each page holds whole instance records in the original input order.
                        for page in &target.pages {
                            let base = page.address();
                            for offset in (0..page.size).step_by(stride) {
                                let address = base + offset;
                                unsafe {
                                    entry.write([address as u32, (address >> 32) as u32]);
                                    entry = entry.add(1);
                                }
                            }
                        }
                        Ok(())
                    },
                    &mut commands,
                    &mut staging,
                )?;
            }
            Some((commands, staging))
        };
        target.owned_storage()?.instances = Some(InstanceInput {
            pointers,
            models: collect_models(models),
        });
        if let Some((commands, staging)) = transfer {
            target.last_use = Some(self.submit_dataset_transfer(commands, staging, false)?);
        }
        Ok(target)
    }
    /// GPU 矩阵直入 TLAS。读取真实数据页，不下载或逐记录重新打包矩阵。
    /// # Safety
    /// 每条最终矩阵必须有限且线性部分可逆。调用者只能改写矩阵逻辑字段；
    /// 不得用 bulk copy/sort/compact 等操作破坏或换入未持有 BLAS 的原生尾部。
    pub unsafe fn top_level_device(&self, data: GpuDataset) -> Result<Acceleration> {
        unsafe { self.top_level_device_operation(data, BuildOperation::Static) }
    }
    /// 直接以 B 产生的原生实例数据构建可更新 TLAS。
    /// # Safety
    /// 与 top_level_device 相同的矩阵及原生尾部／BLAS 前提。
    pub unsafe fn top_level_device_updatable(&self, data: GpuDataset) -> Result<Acceleration> {
        unsafe { self.top_level_device_operation(data, BuildOperation::Updatable) }
    }
    /// B 实例数据直接进入 UPDATE；输入及原结构保持到本次更新真正完成。
    /// # Safety
    /// 与 top_level_device 相同，实例保持原活动状态。
    pub unsafe fn refit_top_level_device(
        &self,
        source: &Acceleration,
        data: GpuDataset,
    ) -> Result<Acceleration> {
        unsafe { self.top_level_device_operation(data, BuildOperation::Refit(&source.0)) }
    }
    unsafe fn top_level_device_operation(
        &self,
        mut data: GpuDataset,
        operation: BuildOperation<'_>,
    ) -> Result<Acceleration> {
        if !Rc::ptr_eq(&self.inner, &data.allocation.inner) {
            return Err(error("实例输入不属于目标设备"));
        }
        let input = data
            .instances
            .as_ref()
            .ok_or_else(|| error("数据集不是由设备创建的原生实例输入"))?;
        let structure = self.build_acceleration(data.count, None, operation, |inputs| {
            let (pointers, address) = match &input.pointers {
                Some(pointers) => (1, pointers.address()),
                None => (0, data.pages[0].address()),
            };
            let geometry = Geometry {
                kind: 1000150006,
                next: null(),
                geometry_type: 2,
                flags: 0,
                data: GeometryData {
                    instances: Instances {
                        kind: 1000150004,
                        next: null(),
                        pointers,
                        data: address,
                    },
                },
            };
            if let Some(previous) = data.dependency() {
                inputs.dependencies.insert(previous);
            }
            Ok((geometry, data.producer, input.models.clone()))
        })?;
        data.last_use = Some(structure.built.clone());
        drop(data);
        Ok(Acceleration(Rc::new(structure)))
    }
    /// 同一只读实例存储直接进入原生构建，不另建输入持有类别。
    /// # Safety
    /// 与top_level_device相同的矩阵／模型／原生尾部前提。
    pub unsafe fn top_level_device_read(&self, data: ReadDataset) -> Result<Acceleration> {
        unsafe { self.top_level_device(data.binding()) }
    }
}
