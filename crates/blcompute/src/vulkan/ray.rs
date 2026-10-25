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
struct Structure {
    inner: Rc<Inner>,
    handle: u64,
    address: u64,
    _storage: Allocation,
    models: Vec<BottomLevel>,
}
impl Drop for Structure {
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
#[derive(Clone)]
pub struct BottomLevel(Rc<Structure>);
pub struct RayInstance {
    pub model: BottomLevel,
    /// 行主序仿射 3x4，与 Vulkan instance transform 相同。
    pub transform: [f32; 12],
}
#[derive(Clone)]
pub struct Acceleration(Rc<Structure>);
impl Acceleration {
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
    fn ray_input(&self, words: &[u32]) -> Result<Allocation> {
        let target = Allocation::new(self.inner.clone(), words.len() as u64 * 4, 0xa0002, 1)?;
        if !words.is_empty() {
            self.upload_page(&target, words, |_| {})?;
        }
        Ok(target)
    }
    fn build_acceleration(
        &self,
        geometry: Geometry,
        count: u32,
        structure_type: u32,
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
            flags: 4,
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
        let mut out = Structure {
            inner: self.inner.clone(),
            handle: 0,
            address: 0,
            _storage: storage,
            models: Vec::new(),
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
        commands.barrier(0x1000 | 0x2000000, 0x2000000, 0x1000 | 0x400000, 0x200000);
        unsafe {
            (ray.build)(commands.command, 1, &info, &ranges);
        }
        commands.barrier(0x2000000, 0x800, 0x400000, 0x200000);
        commands.submit()?;
        commands.wait()?;
        let address = AddressInfo {
            kind: 1000150002,
            next: null(),
            structure: out.handle,
        };
        out.address = unsafe { (ray.address)(self.inner.owner.device, &address) };
        Ok(out)
    }
    /// 输入索引必须来自已经检查拓扑的资产；输入只被构建命令借用。
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
        self.build_acceleration(geometry, u32::try_from(indices.len())?, 1)
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
        let mut structure =
            self.build_acceleration(geometry, u32::try_from(instances.len())?, 0)?;
        structure.models = instances
            .into_iter()
            .map(|i| (i.model.0.handle, i.model))
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_values()
            .collect();
        Ok(Acceleration(Rc::new(structure)))
    }
}
