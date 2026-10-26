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
pub(super) struct StructureStorage {
    inner: Rc<Inner>,
    handle: u64,
    address: u64,
    _storage: Allocation,
    models: Vec<BottomLevel>,
    count: u32,
}
impl Drop for StructureStorage {
    fn drop(&mut self) {
        if self.handle != 0 {
            unsafe {
                (self.inner.ray.as_ref().unwrap().destroy)(
                    self.inner.owner.device,
                    self.handle,
                    null(),
                );
            }
        }
    }
}
// 输出对象只持有其实际构建完成点；执行不反向持有这个对象。
struct Structure {
    storage: ManuallyDrop<StructureStorage>,
    built: Rc<Execution>,
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
pub(super) struct RayInput {
    allocation: Allocation,
    ready: Option<Rc<Execution>>,
}
impl RayInput {
    fn address(&self) -> u64 {
        self.allocation.address()
    }
}
impl Drop for RayInput {
    fn drop(&mut self) {
        if let Some(ready) = &self.ready {
            ready.point.finish_for_drop();
        }
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
}
pub(super) struct InstanceInput {
    pub(super) pointers: Allocation,
    pub(super) models: Vec<BottomLevel>,
}
pub struct RayInstance {
    pub model: BottomLevel,
    /// 行主序仿射 3x4，与 Vulkan instance transform 相同。
    pub transform: [f32; 12],
}
#[derive(Clone)]
pub struct Acceleration(Rc<Structure>);
impl Acceleration {
    pub fn is_complete(&self) -> Result<bool> {
        self.0.built.is_complete()
    }
    pub fn wait(&self) -> Result<()> {
        self.0.built.wait()
    }
    pub(super) fn produced(&self) -> Rc<Execution> {
        self.0.built.clone()
    }
    pub fn allocated_bytes(&self) -> u64 {
        self.0._storage.allocated_size
            + self
                .0
                .models
                .iter()
                .map(|m| m.0._storage.allocated_size)
                .sum::<u64>()
    }
    pub(super) fn handle(&self) -> u64 {
        self.0.handle
    }
    pub(super) fn belongs_to(&self, inner: &Rc<Inner>) -> bool {
        Rc::ptr_eq(&self.0.inner, inner)
    }
}
impl Device {
    fn ray_input(&self, words: &[u32]) -> Result<RayInput> {
        let allocation = Allocation::new(self.inner.clone(), words.len() as u64 * 4, 0xa0002, 1)?;
        let ready = if words.is_empty() {
            None
        } else {
            let mut commands = Commands::new(self.inner.clone())?;
            let mut staging = Vec::new();
            self.stage_page(&allocation, words, |_| {}, &mut commands, &mut staging)?;
            Some(self.submit_transfer(commands, staging)?)
        };
        Ok(RayInput { allocation, ready })
    }
    fn build_acceleration(
        &self,
        geometry: Geometry,
        count: u32,
        structure_type: u32,
        producer: Usage,
        inputs: Vec<RayInput>,
        models: Vec<BottomLevel>,
        mut dependencies: Vec<Rc<Execution>>,
    ) -> Result<Structure> {
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
        let mut info = Build {
            kind: 1000150000,
            next: null(),
            structure_type,
            flags: 4 | if self.inner.position_fetch && structure_type == 1 {
                0x800
            } else {
                0
            },
            mode: 0,
            source: 0,
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
        let storage = Allocation::new(self.inner.clone(), sizes.structure, 0x120000, 1)?;
        dependencies.extend(inputs.iter().filter_map(|input| input.ready.clone()));
        dependencies.extend(models.iter().map(|model| model.0.built.clone()));
        let dependencies = Execution::prepare(dependencies)?;
        let mut out = StructureStorage {
            inner: self.inner.clone(),
            handle: 0,
            address: 0,
            _storage: storage,
            models,
            count,
        };
        let ci = Create {
            kind: 1000150017,
            next: null(),
            flags: 0,
            buffer: out._storage.buffer,
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
            sizes.build_scratch + alignment - 1,
            0x20020,
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
        let mut commands = Commands::new(self.inner.clone())?;
        commands.barrier(
            producer.stage() | 0x1000 | 0x2000000,
            0x2000000,
            producer.access() | 0x1000 | 0x400000,
            0x200020,
        );
        unsafe {
            (ray.build)(commands.command, 1, &info, &ranges);
        }
        commands.submit()?;
        let address = AddressInfo {
            kind: 1000150002,
            next: null(),
            structure: out.handle,
        };
        out.address = unsafe { (ray.address)(self.inner.owner.device, &address) };
        let built = Execution::new(
            ExecutionResources::Acceleration {
                commands,
                _scratch: scratch,
                _inputs: inputs,
            },
            dependencies,
        );
        Ok(Structure {
            storage: ManuallyDrop::new(out),
            built,
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
        let vertices: Vec<_> = positions.iter().flatten().map(|v| v.to_bits()).collect();
        let triangles: Vec<_> = indices.iter().flatten().copied().collect();
        let vertices = self.ray_input(&vertices)?;
        let triangles = self.ray_input(&triangles)?;
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
                    vertices: vertices.address(),
                    stride: 12,
                    max_vertex: u32::try_from(positions.len().saturating_sub(1))?,
                    index_type: 1,
                    indices: triangles.address(),
                    transform: 0,
                },
            },
        };
        self.build_acceleration(
            geometry,
            u32::try_from(indices.len())?,
            1,
            Usage::Transfer,
            vec![vertices, triangles],
            Vec::new(),
            Vec::new(),
        )
        .map(|s| BottomLevel(Rc::new(s)))
    }
    /// # Safety
    /// 实例仿射矩阵必须为有限值且可逆；模型由本设备的 BLAS 构造入口提供。
    pub unsafe fn top_level(&self, instances: Vec<RayInstance>) -> Result<Acceleration> {
        let mut words = Vec::with_capacity(instances.len() * 16);
        for instance in &instances {
            if !Rc::ptr_eq(&instance.model.0.inner, &self.inner) {
                return Err(error("BLAS 不属于目标设备"));
            }
            words.extend(instance.transform.map(f32::to_bits));
            words.extend([
                0xff000000,
                0x01000000,
                instance.model.0.address as u32,
                (instance.model.0.address >> 32) as u32,
            ]);
        }
        let input = self.ray_input(&words)?;
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
        let models = instances
            .into_iter()
            .map(|i| (i.model.0.handle, i.model))
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_values()
            .collect();
        let structure = self.build_acceleration(
            geometry,
            u32::try_from(words.len() / 16)?,
            0,
            Usage::Transfer,
            vec![input],
            models,
            Vec::new(),
        )?;
        Ok(Acceleration(Rc::new(structure)))
    }
    /// 创建 Vulkan 实例输入，B 仅声明 offset 0..48 的 12 个 f32 与 stride(64)。
    /// 原生尾部、页内地址及 BLAS 引用随唯一物理存储转移，不暴露为 B 字段。
    pub fn instance_data(
        &self,
        program: &Program,
        binding: usize,
        models: Vec<BottomLevel>,
    ) -> Result<GpuDataset> {
        let data = Dataset::new(program, binding, u32::try_from(models.len())?)?;
        self.instance_data_from(data, models)
    }
    /// CPU矩阵与B产生的矩阵共用实际64字节实例输入，原生尾部仍由平台填写。
    pub fn instance_data_from(
        &self,
        mut data: Dataset,
        models: Vec<BottomLevel>,
    ) -> Result<GpuDataset> {
        if data.count as usize != models.len() {
            return Err(error("矩阵记录与实际BLAS实例数量不同"));
        }
        let components: Vec<_> = (0..12)
            .map(|i| (blanguage::ir::Scalar::F32, i * 4))
            .collect();
        if data.stride != 64 || data.ty.components != components {
            return Err(error(
                "Vulkan 实例输入需要 64 字节 stride 与 offset 0 的 3×4 f32 矩阵",
            ));
        }
        for (model, record) in models.iter().zip(data.words.chunks_exact_mut(16)) {
            if !Rc::ptr_eq(&model.0.inner, &self.inner) {
                return Err(error("BLAS 不属于目标设备"));
            }
            record[12..16].copy_from_slice(&[
                0xff000000,
                0x01000000,
                model.0.address as u32,
                (model.0.address >> 32) as u32,
            ]);
        }
        let mut target = GpuDataset::allocate(
            self.inner.clone(),
            data.ty,
            data.stride,
            data.count,
            0x80000,
        )?;
        let mut addresses = Vec::with_capacity(models.len() * 2);
        for page in &target.pages {
            let base = page.address();
            for i in 0..page.size / 64 {
                let address = base + i * 64;
                addresses.extend([address as u32, (address >> 32) as u32]);
            }
        }
        let pointers = Allocation::new(self.inner.clone(), addresses.len() as u64 * 4, 0xa0002, 1)?;
        target.owned_storage()?.instances = Some(InstanceInput {
            pointers,
            models: models
                .into_iter()
                .map(|m| (m.0.handle, m))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_values()
                .collect(),
        });
        if !data.words.is_empty() {
            let mut commands = Commands::new(self.inner.clone())?;
            let mut staging = Vec::new();
            self.stage_payload(&target, &data.words, &mut commands, &mut staging)?;
            self.stage_page(
                &target.instances.as_ref().unwrap().pointers,
                &addresses,
                |_| {},
                &mut commands,
                &mut staging,
            )?;
            target.last_use = Some(self.submit_transfer(commands, staging)?);
        }
        Ok(target)
    }
    /// GPU 矩阵直入 TLAS。读取真实数据页，不下载或逐记录重新打包矩阵。
    /// # Safety
    /// 每条最终矩阵必须有限且线性部分可逆。调用者只能改写矩阵逻辑字段；
    /// 不得用 bulk copy/sort/compact 等操作破坏或换入未持有 BLAS 的原生尾部。
    pub unsafe fn top_level_device(&self, mut data: GpuDataset) -> Result<Acceleration> {
        if !Rc::ptr_eq(&self.inner, &data.allocation.inner) {
            return Err(error("实例输入不属于目标设备"));
        }
        let input = data
            .instances
            .as_ref()
            .ok_or_else(|| error("数据集不是由设备创建的原生实例输入"))?;
        let geometry = Geometry {
            kind: 1000150006,
            next: null(),
            geometry_type: 2,
            flags: 0,
            data: GeometryData {
                instances: Instances {
                    kind: 1000150004,
                    next: null(),
                    pointers: 1,
                    data: input.pointers.address(),
                },
            },
        };
        let models = input.models.clone();
        let dependencies = data.dependency().cloned().into_iter().collect();
        let structure = self.build_acceleration(
            geometry,
            data.count,
            0,
            data.producer,
            Vec::new(),
            models,
            dependencies,
        )?;
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
