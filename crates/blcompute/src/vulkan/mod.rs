//! Vulkan 执行后端。每个逻辑设备拥有队列，执行资源在原生完成后释放。
mod buffer_memory;
mod compact;
mod execution;
mod memory;
mod prefix;
mod present;
mod raw;
mod ray;
mod scan;
mod selection;
mod sort;
mod texture;
mod transfer;
use crate::application::FrameArguments;
use crate::bindings::{self, StorageSlots};
use crate::views::Views;
use transfer::Readback;
use crate::{Dataset, DomainBindings, Result, Value, arguments, error};
use blanguage::{
    ExecutableProgram, TemplateProgram,
    ir::{DeviceRequirements, ExecutionPlan as Program, ProgramInterface, Ty},
};
pub use execution::{GpuCompletion, GpuStageTimestamps, GpuTimestamps};
use execution::{Completion, Dependencies, DependencySources, Execution, ExecutionResources, QueryPool};
pub use memory::{MemoryBudget, MemoryHeapBudget};
pub use present::{PresentStatus, Swapchain};
use raw::*;
pub use ray::{Acceleration, BottomLevel, RayInstance};
use std::{borrow::BorrowMut, num::NonZeroU32};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, c_char, c_void},
    mem::ManuallyDrop,
    ptr::{null, null_mut},
    rc::Rc,
};
pub use texture::{Texture, TextureCopyLevel, TextureInput};
use texture::{BufferImageCopy, ImageBarrier, SubresourceRange};

type Gipa = unsafe extern "system" fn(VkInstance, *const c_char) -> *const c_void;
type Gdpa = unsafe extern "system" fn(VkDevice, *const c_char) -> *const c_void;
type DestroyInstance = unsafe extern "system" fn(VkInstance, *const c_void);
type DestroyDevice = unsafe extern "system" fn(VkDevice, *const c_void);
fn check(code: i32, operation: &str) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(error(format!("{operation} 返回 VkResult {code}")))
    }
}
fn zero<T>() -> T {
    unsafe { std::mem::zeroed() }
}
unsafe fn function<F: Copy>(address: *const c_void, name: &CStr) -> Result<F> {
    if address.is_null() {
        return Err(error(format!("Vulkan 缺少入口 {}", name.to_string_lossy())));
    }
    Ok(unsafe { std::mem::transmute_copy(&address) })
}
struct Library {
    _library: crate::library::Library,
    gipa: Gipa,
}
impl Library {
    fn new() -> Result<Self> {
        let library = crate::library::Library::load("vulkan-1.dll")?;
        let gipa: Gipa = unsafe { std::mem::transmute(library.symbol(c"vkGetInstanceProcAddr")?) };
        Ok(Self { _library: library, gipa })
    }
    unsafe fn get<F: Copy>(&self, instance: VkInstance, name: &CStr) -> Result<F> {
        unsafe { function((self.gipa)(instance, name.as_ptr()), name) }
    }
}
struct InstanceOwner {
    instance: VkInstance,
    destroy: DestroyInstance,
    library: Library,
    presentation: bool,
}
impl Drop for InstanceOwner {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.instance, null());
        }
    }
}
/// Vulkan 实例；物理设备与逻辑设备共同持有其实际生命周期。
#[derive(Clone)]
pub struct Instance {
    owner: Rc<InstanceOwner>,
}
impl Instance {
    pub fn new() -> Result<Self> {
        Self::create(false)
    }
    /// 显式启用 Windows surface；无需先创建窗口。
    pub fn with_presentation() -> Result<Self> {
        Self::create(true)
    }
    fn create(presentation: bool) -> Result<Self> {
        unsafe {
            let library = Library::new()?;
            let create: unsafe extern "system" fn(
                *const VkInstanceCreateInfo,
                *const c_void,
                *mut VkInstance,
            ) -> i32 = library.get(null_mut(), c"vkCreateInstance")?;
            let mut app: VkApplicationInfo = zero();
            app.sType = 0;
            app.pApplicationName = c"BLCompute".as_ptr();
            app.pEngineName = c"BLCompute".as_ptr();
            app.apiVersion = (1 << 22) | (2 << 12);
            let extensions = if presentation {
                vec![c"VK_KHR_surface".as_ptr(), c"VK_KHR_win32_surface".as_ptr()]
            } else {
                Vec::new()
            };
            let mut info: VkInstanceCreateInfo = zero();
            info.sType = 1;
            info.pApplicationInfo = &app;
            info.enabledExtensionCount = extensions.len() as u32;
            info.ppEnabledExtensionNames = extensions.as_ptr();
            let mut instance = null_mut();
            check(create(&info, null(), &mut instance), "vkCreateInstance")?;
            let destroy = library.get(instance, c"vkDestroyInstance")?;
            Ok(Self {
                owner: Rc::new(InstanceOwner {
                    instance,
                    destroy,
                    library,
                    presentation,
                }),
            })
        }
    }
    /// 返回本实例的真实物理句柄；保留驱动顺序，不筛去集成 GPU 或其他类型。
    pub fn physical_devices(&self) -> Result<Vec<PhysicalDevice>> {
        unsafe {
            let owner = &self.owner;
            let instance = owner.instance;
            let enumerate: unsafe extern "system" fn(
                VkInstance,
                *mut u32,
                *mut VkPhysicalDevice,
            ) -> i32 = owner.library.get(instance, c"vkEnumeratePhysicalDevices")?;
            let properties: unsafe extern "system" fn(
                VkPhysicalDevice,
                *mut VkPhysicalDeviceProperties,
            ) = owner
                .library
                .get(instance, c"vkGetPhysicalDeviceProperties")?;
            let mut count = 0;
            check(
                enumerate(instance, &mut count, null_mut()),
                "vkEnumeratePhysicalDevices(count)",
            )?;
            let mut devices = vec![null_mut(); count as usize];
            check(
                enumerate(instance, &mut count, devices.as_mut_ptr()),
                "vkEnumeratePhysicalDevices",
            )?;
            devices.truncate(count as usize);
            Ok(devices
                .into_iter()
                .map(|physical| {
                    let mut p = zero();
                    properties(physical, &mut p);
                    PhysicalDevice {
                        owner: owner.clone(),
                        physical,
                        properties: p,
                    }
                })
                .collect())
        }
    }
}
#[repr(C)]
struct ExtensionProperties {
    name: [c_char; 256],
    version: u32,
}
/// 从实际实例枚举取得的选择句柄；名称仅供显示，不用于重建或重选资源。
#[derive(Clone)]
pub struct PhysicalDevice {
    owner: Rc<InstanceOwner>,
    physical: VkPhysicalDevice,
    properties: VkPhysicalDeviceProperties,
}
impl PhysicalDevice {
    pub fn name(&self) -> String {
        unsafe { CStr::from_ptr(self.properties.deviceName.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }
    /// 本物理句柄已查询的推送常量容量，单位为字节。
    pub fn max_push_constant_bytes(&self) -> u32 {
        self.properties.limits.maxPushConstantsSize
    }
    pub fn is_discrete(&self) -> bool {
        self.properties.deviceType == 2
    }
    pub fn device(&self) -> Result<Device> {
        Device::create(std::iter::once(self.clone()), std::iter::empty::<&Program>())
    }
    pub fn for_program(&self, program: &Program) -> Result<Device> {
        self.for_programs(std::iter::once(program))
    }
    /// 在选定物理设备上声明各程序能力；呈现能力由原实例的显式创建方式决定。
    pub fn for_programs<'a>(
        &self,
        programs: impl IntoIterator<Item = &'a Program>,
    ) -> Result<Device> {
        Device::create(std::iter::once(self.clone()), programs)
    }
}
struct DeviceOwner {
    device: VkDevice,
    destroy: DestroyDevice,
    _instance: Rc<InstanceOwner>,
}
impl Drop for DeviceOwner {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.device, null());
        }
    }
}

macro_rules! functions{($table:ident {$($name:ident : $ty:ty),* $(,)?})=>{
    #[allow(non_snake_case)]struct $table{$($name:$ty),*}
    impl $table{unsafe fn load(device:VkDevice,get:Gdpa)->Result<Self>{Ok(Self{$($name:unsafe{function(get(device,concat!(stringify!($name),"\0").as_ptr().cast()),CStr::from_bytes_with_nul_unchecked(concat!(stringify!($name),"\0").as_bytes()))}? ,)*})}}
}}
functions! { Functions {
    vkGetDeviceQueue:unsafe extern "system" fn(VkDevice,u32,u32,*mut VkQueue),
    vkCreateImage:unsafe extern "system" fn(VkDevice,*const texture::ImageCreateInfo,*const c_void,*mut u64)->i32,
    vkDestroyImage:unsafe extern "system" fn(VkDevice,u64,*const c_void),
    vkGetImageMemoryRequirements:unsafe extern "system" fn(VkDevice,u64,*mut VkMemoryRequirements),
    vkBindImageMemory:unsafe extern "system" fn(VkDevice,u64,VkDeviceMemory,u64)->i32,
    vkCreateImageView:unsafe extern "system" fn(VkDevice,*const texture::ImageViewCreateInfo,*const c_void,*mut u64)->i32,
    vkDestroyImageView:unsafe extern "system" fn(VkDevice,u64,*const c_void),
    vkCreateSampler:unsafe extern "system" fn(VkDevice,*const texture::SamplerCreateInfo,*const c_void,*mut u64)->i32,
    vkDestroySampler:unsafe extern "system" fn(VkDevice,u64,*const c_void),
    vkCmdCopyBufferToImage:unsafe extern "system" fn(VkCommandBuffer,VkBuffer,u64,u32,u32,*const BufferImageCopy),
    vkCmdCopyImageToBuffer:unsafe extern "system" fn(VkCommandBuffer,u64,u32,VkBuffer,u32,*const BufferImageCopy),
    vkCreateBuffer:unsafe extern "system" fn(VkDevice,*const VkBufferCreateInfo,*const c_void,*mut VkBuffer)->i32,
    vkDestroyBuffer:unsafe extern "system" fn(VkDevice,VkBuffer,*const c_void),
    vkGetBufferMemoryRequirements2:unsafe extern "system" fn(VkDevice,*const buffer_memory::BufferInfo,*mut buffer_memory::Requirements2),
    vkGetBufferDeviceAddress:unsafe extern "system" fn(VkDevice,*const VkBufferDeviceAddressInfo)->u64,
    vkAllocateMemory:unsafe extern "system" fn(VkDevice,*const VkMemoryAllocateInfo,*const c_void,*mut VkDeviceMemory)->i32,
    vkFreeMemory:unsafe extern "system" fn(VkDevice,VkDeviceMemory,*const c_void),
    vkBindBufferMemory:unsafe extern "system" fn(VkDevice,VkBuffer,VkDeviceMemory,u64)->i32,
    vkMapMemory:unsafe extern "system" fn(VkDevice,VkDeviceMemory,u64,u64,u32,*mut *mut c_void)->i32,
    vkUnmapMemory:unsafe extern "system" fn(VkDevice,VkDeviceMemory),
    vkCreateDescriptorSetLayout:unsafe extern "system" fn(VkDevice,*const VkDescriptorSetLayoutCreateInfo,*const c_void,*mut VkDescriptorSetLayout)->i32,
    vkDestroyDescriptorSetLayout:unsafe extern "system" fn(VkDevice,VkDescriptorSetLayout,*const c_void),
    vkCreatePipelineLayout:unsafe extern "system" fn(VkDevice,*const VkPipelineLayoutCreateInfo,*const c_void,*mut VkPipelineLayout)->i32,
    vkDestroyPipelineLayout:unsafe extern "system" fn(VkDevice,VkPipelineLayout,*const c_void),
    vkCreateShaderModule:unsafe extern "system" fn(VkDevice,*const VkShaderModuleCreateInfo,*const c_void,*mut VkShaderModule)->i32,
    vkDestroyShaderModule:unsafe extern "system" fn(VkDevice,VkShaderModule,*const c_void),
    vkCreateComputePipelines:unsafe extern "system" fn(VkDevice,VkPipelineCache,u32,*const VkComputePipelineCreateInfo,*const c_void,*mut VkPipeline)->i32,
    vkDestroyPipeline:unsafe extern "system" fn(VkDevice,VkPipeline,*const c_void),
    vkCreateDescriptorPool:unsafe extern "system" fn(VkDevice,*const VkDescriptorPoolCreateInfo,*const c_void,*mut VkDescriptorPool)->i32,
    vkDestroyDescriptorPool:unsafe extern "system" fn(VkDevice,VkDescriptorPool,*const c_void),
    vkAllocateDescriptorSets:unsafe extern "system" fn(VkDevice,*const VkDescriptorSetAllocateInfo,*mut VkDescriptorSet)->i32,
    vkUpdateDescriptorSets:unsafe extern "system" fn(VkDevice,u32,*const VkWriteDescriptorSet,u32,*const c_void),
    vkCreateCommandPool:unsafe extern "system" fn(VkDevice,*const VkCommandPoolCreateInfo,*const c_void,*mut VkCommandPool)->i32,
    vkDestroyCommandPool:unsafe extern "system" fn(VkDevice,VkCommandPool,*const c_void),
    vkAllocateCommandBuffers:unsafe extern "system" fn(VkDevice,*const VkCommandBufferAllocateInfo,*mut VkCommandBuffer)->i32,
    vkBeginCommandBuffer:unsafe extern "system" fn(VkCommandBuffer,*const VkCommandBufferBeginInfo)->i32,
    vkEndCommandBuffer:unsafe extern "system" fn(VkCommandBuffer)->i32,
    vkCmdCopyBuffer:unsafe extern "system" fn(VkCommandBuffer,VkBuffer,VkBuffer,u32,*const VkBufferCopy),
    vkCmdFillBuffer:unsafe extern "system" fn(VkCommandBuffer,VkBuffer,u64,u64,u32),
    vkCmdPipelineBarrier:unsafe extern "system" fn(VkCommandBuffer,u32,u32,u32,u32,*const VkMemoryBarrier,u32,*const c_void,u32,*const c_void),
    vkCmdBindPipeline:unsafe extern "system" fn(VkCommandBuffer,u32,VkPipeline),
    vkCmdBindDescriptorSets:unsafe extern "system" fn(VkCommandBuffer,u32,VkPipelineLayout,u32,u32,*const VkDescriptorSet,u32,*const u32),
    vkCmdPushConstants:unsafe extern "system" fn(VkCommandBuffer,VkPipelineLayout,u32,u32,u32,*const c_void),
    vkCmdDispatch:unsafe extern "system" fn(VkCommandBuffer,u32,u32,u32),
    vkCmdDispatchIndirect:unsafe extern "system" fn(VkCommandBuffer,VkBuffer,u64),
    vkCreateFence:unsafe extern "system" fn(VkDevice,*const VkFenceCreateInfo,*const c_void,*mut VkFence)->i32,
    vkDestroyFence:unsafe extern "system" fn(VkDevice,VkFence,*const c_void),
    vkResetFences:unsafe extern "system" fn(VkDevice,u32,*const VkFence)->i32,
    vkWaitForFences:unsafe extern "system" fn(VkDevice,u32,*const VkFence,u32,u64)->i32,
    vkGetFenceStatus:unsafe extern "system" fn(VkDevice,VkFence)->i32,
    vkQueueSubmit:unsafe extern "system" fn(VkQueue,u32,*const VkSubmitInfo,VkFence)->i32,
    vkCreateQueryPool:unsafe extern "system" fn(VkDevice,*const QueryPoolCreateInfo,*const c_void,*mut u64)->i32,
    vkDestroyQueryPool:unsafe extern "system" fn(VkDevice,u64,*const c_void),
    vkCmdResetQueryPool:unsafe extern "system" fn(VkCommandBuffer,u64,u32,u32),
    vkCmdWriteTimestamp:unsafe extern "system" fn(VkCommandBuffer,u32,u64,u32),
    vkGetQueryPoolResults:unsafe extern "system" fn(VkDevice,u64,u32,u32,usize,*mut c_void,u64,u32)->i32,
}}
functions! { PresentFunctions {
    vkCreateSwapchainKHR:unsafe extern "system" fn(VkDevice,*const present::SwapchainCreateInfo,*const c_void,*mut u64)->i32,
    vkDestroySwapchainKHR:unsafe extern "system" fn(VkDevice,u64,*const c_void),
    vkGetSwapchainImagesKHR:unsafe extern "system" fn(VkDevice,u64,*mut u32,*mut u64)->i32,
    vkAcquireNextImageKHR:unsafe extern "system" fn(VkDevice,u64,u64,VkSemaphore,VkFence,*mut u32)->i32,
    vkQueuePresentKHR:unsafe extern "system" fn(VkQueue,*const present::PresentInfo)->i32,
    vkCreateSemaphore:unsafe extern "system" fn(VkDevice,*const present::SemaphoreCreateInfo,*const c_void,*mut u64)->i32,
    vkDestroySemaphore:unsafe extern "system" fn(VkDevice,u64,*const c_void),
    vkDeviceWaitIdle:unsafe extern "system" fn(VkDevice)->i32,
}}
#[repr(C)]
struct QueryPoolCreateInfo {
    s_type: u32,
    next: *const c_void,
    flags: u32,
    query_type: u32,
    count: u32,
    statistics: u32,
}
struct Inner {
    owner: Rc<DeviceOwner>,
    functions: Functions,
    physical: VkPhysicalDevice,
    queue: VkQueue,
    family: u32,
    memory: VkPhysicalDeviceMemoryProperties,
    memory_budget: Option<memory::BudgetQuery>,
    buffer_memory: RefCell<buffer_memory::Pool>,
    samplers: RefCell<[std::rc::Weak<texture::Sampler>; 8]>,
    max_buffer_size: Option<u64>,
    properties: VkPhysicalDeviceProperties,
    timestamp_bits: u32,
    ray: Option<ray::RayDevice>,
    sorting: bool,
    maximal_reconvergence: bool,
    texture_indexing: bool,
    position_fetch: bool,
    subgroup_operations: u32,
    subgroup_extended_types: bool,
    float32_preserve: bool,
    float64: bool,
    integer8: bool,
    integer16: bool,
    atomic64: bool,
    shared_atomic64: bool,
    float64_preserve: bool,
    next_submission: Cell<u64>,
}
impl Inner {
    fn next_submission(&self) -> Result<u64> {
        self.next_submission.get().checked_add(1)
            .ok_or_else(|| error("设备队列提交序号耗尽"))
    }

    fn enqueue(&self, next: u64, fence: VkFence, batch: Option<&VkSubmitInfo>) -> Result<()> {
        let count = u32::from(batch.is_some());
        let info = batch.map_or(null(), std::ptr::from_ref);
        unsafe {
            check(
                (self.functions.vkQueueSubmit)(self.queue, count, info, fence),
                "vkQueueSubmit",
            )?;
        }
        self.next_submission.set(next);
        Ok(())
    }
}
#[derive(Clone)]
pub struct Device {
    inner: Rc<Inner>,
}
impl Device {
    /// 是否为同一个逻辑设备与队列所有者；同一物理GPU上的独立设备仍不同。
    pub fn same_device(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
    pub fn new() -> Result<Self> {
        Self::create(Instance::new()?.physical_devices()?, std::iter::empty::<&Program>())
    }
    pub fn for_program(program: &Program) -> Result<Self> {
        Self::for_programs(std::iter::once(program))
    }
    /// 同一逻辑设备消费多个B程序，按实际程序集合声明能力，不在执行中重建设备。
    pub fn for_programs<'a>(
        programs: impl IntoIterator<Item = &'a Program>,
    ) -> Result<Self> {
        Self::create(Instance::new()?.physical_devices()?, programs)
    }
    pub fn with_presentation(program: &Program) -> Result<Self> {
        Self::with_presentation_programs(std::iter::once(program))
    }
    pub fn with_presentation_programs<'a>(
        programs: impl IntoIterator<Item = &'a Program>,
    ) -> Result<Self> {
        Self::create(Instance::with_presentation()?.physical_devices()?, programs)
    }
    fn create<'a>(
        candidates: impl IntoIterator<Item = PhysicalDevice>,
        programs: impl IntoIterator<Item = &'a Program>,
    ) -> Result<Self> {
        let mut requirements = DeviceRequirements::default();
        let mut uniform_bytes = 0;
        let mut texture_count = 0;
        let mut resource_count = 0;
        let mut storage_count = 0;
        let mut acceleration_count = 0;
        let mut workgroup_width = 0;
        let mut shared_bytes = 0;
        for program in programs {
            let stages = program.stages();
            if !stages.is_empty() {
                uniform_bytes = uniform_bytes.max(program.uniform_bytes());
                let textures = program.texture_count();
                texture_count = texture_count.max(textures);
                let storage = program.buffers().len() + program.auxiliary_buffers();
                storage_count = storage_count.max(storage);
                acceleration_count = acceleration_count.max(program.accelerations().len());
                resource_count = resource_count.max(storage + textures);
                for stage in stages {
                    workgroup_width = workgroup_width.max(stage.workgroup_width());
                    shared_bytes = shared_bytes.max(stage.shared_bytes());
                }
            }
            let required = program.requirements();
            requirements.float64 |= required.float64;
            requirements.float32_preserve |= required.float32_preserve;
            requirements.float64_preserve |= required.float64_preserve;
            requirements.integer8 |= required.integer8;
            requirements.integer16 |= required.integer16;
            requirements.atomic64 |= required.atomic64;
            requirements.shared_atomic64 |= required.shared_atomic64;
            requirements.ray_query |= required.ray_query;
            requirements.sorting |= required.sorting;
            requirements.maximal_reconvergence |= required.maximal_reconvergence;
            requirements.texture_indexing |= required.texture_indexing;
            requirements.position_fetch |= required.position_fetch;
            requirements.subgroup.basic |= required.subgroup.basic;
            requirements.subgroup.vote |= required.subgroup.vote;
            requirements.subgroup.arithmetic |= required.subgroup.arithmetic;
            requirements.subgroup.ballot |= required.subgroup.ballot;
            requirements.subgroup.shuffle |= required.subgroup.shuffle;
            requirements.subgroup.shuffle_relative |= required.subgroup.shuffle_relative;
            requirements.subgroup.extended_types |= required.subgroup.extended_types;
        }
        let DeviceRequirements { ray_query, sorting, maximal_reconvergence, texture_indexing,
            position_fetch, float64, float32_preserve, float64_preserve, integer8, integer16,
            atomic64, shared_atomic64, subgroup } = requirements;
        let subgroup_extended_types = subgroup.extended_types;
        let required_subgroup = u32::from(subgroup.basic) | (u32::from(subgroup.vote) << 1)
            | (u32::from(subgroup.arithmetic) << 2) | (u32::from(subgroup.ballot) << 3)
            | (u32::from(subgroup.shuffle) << 4) | (u32::from(subgroup.shuffle_relative) << 5);
        let mut candidates: Vec<_> = candidates.into_iter().collect();
        candidates.sort_by_key(|candidate| !candidate.is_discrete());
        let mut unavailable = Vec::new();
        'physical: for selected in candidates {
            unsafe {
                let PhysicalDevice {
                    owner,
                    physical,
                    properties,
                } = selected;
                let instance = owner.instance;
                let presentation = owner.presentation;
                let device_name = CStr::from_ptr(properties.deviceName.as_ptr()).to_string_lossy();
                let families_fn: unsafe extern "system" fn(
                    VkPhysicalDevice,
                    *mut u32,
                    *mut VkQueueFamilyProperties,
                ) = owner
                    .library
                    .get(instance, c"vkGetPhysicalDeviceQueueFamilyProperties")?;
                let memory_fn: unsafe extern "system" fn(
                    VkPhysicalDevice,
                    *mut VkPhysicalDeviceMemoryProperties,
                ) = owner
                    .library
                    .get(instance, c"vkGetPhysicalDeviceMemoryProperties")?;
                let create_device: unsafe extern "system" fn(
                    VkPhysicalDevice,
                    *const VkDeviceCreateInfo,
                    *const c_void,
                    *mut VkDevice,
                ) -> i32 = owner.library.get(instance, c"vkCreateDevice")?;
                let destroy_device: DestroyDevice = owner.library.get(instance, c"vkDestroyDevice")?;
                let get_device: Gdpa = owner.library.get(instance, c"vkGetDeviceProcAddr")?;
                if properties.apiVersion < ((1 << 22) | (2 << 12)) {
                    unavailable.push(format!("{device_name}: {}", "目标 GPU 需要 Vulkan 1.2"));
                    continue 'physical;
                }
                if uniform_bytes > properties.limits.maxPushConstantsSize {
                    unavailable.push(format!("{device_name}: 程序推送常量需要{uniform_bytes}字节，maxPushConstantsSize为{}", properties.limits.maxPushConstantsSize));
                    continue 'physical;
                }
                if texture_count > properties.limits.maxPerStageDescriptorSamplers as usize
                    || texture_count > properties.limits.maxPerStageDescriptorSampledImages as usize
                    || texture_count > properties.limits.maxDescriptorSetSamplers as usize
                    || texture_count > properties.limits.maxDescriptorSetSampledImages as usize
                {
                    unavailable.push(format!("{device_name}: 纹理描述符需要{texture_count}项，maxPerStageDescriptorSamplers={}，maxPerStageDescriptorSampledImages={}，maxDescriptorSetSamplers={}，maxDescriptorSetSampledImages={}", properties.limits.maxPerStageDescriptorSamplers, properties.limits.maxPerStageDescriptorSampledImages, properties.limits.maxDescriptorSetSamplers, properties.limits.maxDescriptorSetSampledImages));
                    continue 'physical;
                }
                if storage_count > properties.limits.maxPerStageDescriptorStorageBuffers as usize
                    || storage_count > properties.limits.maxDescriptorSetStorageBuffers as usize
                {
                    unavailable.push(format!("{device_name}: storage描述符需要{storage_count}项，maxPerStageDescriptorStorageBuffers={}，maxDescriptorSetStorageBuffers={}", properties.limits.maxPerStageDescriptorStorageBuffers, properties.limits.maxDescriptorSetStorageBuffers));
                    continue 'physical;
                }
                if resource_count > properties.limits.maxPerStageResources as usize {
                    unavailable.push(format!("{device_name}: 程序compute stage资源需要{resource_count}项，maxPerStageResources为{}", properties.limits.maxPerStageResources));
                    continue 'physical;
                }
                if workgroup_width > properties.limits.maxComputeWorkGroupInvocations
                    || workgroup_width > properties.limits.maxComputeWorkGroupSize[0]
                {
                    unavailable.push(format!("{device_name}: 程序工作组需要{workgroup_width}个调用，maxComputeWorkGroupInvocations={}，maxComputeWorkGroupSize[0]={}", properties.limits.maxComputeWorkGroupInvocations, properties.limits.maxComputeWorkGroupSize[0]));
                    continue 'physical;
                }
                if shared_bytes > properties.limits.maxComputeSharedMemorySize as u64 {
                    unavailable.push(format!("{device_name}: 程序共享内存需要{shared_bytes}字节，maxComputeSharedMemorySize为{}", properties.limits.maxComputeSharedMemorySize));
                    continue 'physical;
                }
                let features_fn: unsafe extern "system" fn(
                    VkPhysicalDevice,
                    *mut VkPhysicalDeviceFeatures2,
                ) = owner
                    .library
                    .get(instance, c"vkGetPhysicalDeviceFeatures2")?;
                let mut address: VkPhysicalDeviceBufferDeviceAddressFeatures = zero();
                address.sType = 1000257000;
                let mut position_features = ray::PositionFetchFeatures {
                    kind: 1000481000,
                    next: null_mut(),
                    position_fetch: 0,
                };
                let mut query_features = ray::QueryFeatures {
                    kind: 1000348013,
                    next: if position_fetch {
                        (&mut position_features as *mut ray::PositionFetchFeatures).cast()
                    } else {
                        null_mut()
                    },
                    ray_query: 0,
                };
                let mut acceleration_features: ray::AccelerationFeatures = zero();
                acceleration_features.kind = 1000150013;
                acceleration_features.next = (&mut query_features as *mut ray::QueryFeatures).cast();
                if ray_query {
                    address.pNext =
                        (&mut acceleration_features as *mut ray::AccelerationFeatures).cast();
                }
                let mut features: VkPhysicalDeviceFeatures2 = zero();
                features.sType = 1000059000;
                features.pNext =
                    (&mut address as *mut VkPhysicalDeviceBufferDeviceAddressFeatures).cast();
                let mut atomics: VkPhysicalDeviceShaderAtomicInt64Features = zero();
                atomics.sType = 1000180000;
                if atomic64 || shared_atomic64 {
                    atomics.pNext = features.pNext;
                    features.pNext = (&mut atomics as *mut VkPhysicalDeviceShaderAtomicInt64Features).cast();
                }
                let mut storage8: VkPhysicalDevice8BitStorageFeatures = zero();
                storage8.sType = 1000177000;
                let mut arithmetic8: VkPhysicalDeviceShaderFloat16Int8Features = zero();
                arithmetic8.sType = 1000082000;
                if integer8 {
                    storage8.pNext = features.pNext;
                    arithmetic8.pNext = (&mut storage8 as *mut VkPhysicalDevice8BitStorageFeatures).cast();
                    features.pNext = (&mut arithmetic8 as *mut VkPhysicalDeviceShaderFloat16Int8Features).cast();
                }
                let mut storage16: VkPhysicalDevice16BitStorageFeatures = zero();
                storage16.sType = 1000083000;
                if integer16 {
                    storage16.pNext = features.pNext;
                    features.pNext = (&mut storage16 as *mut VkPhysicalDevice16BitStorageFeatures).cast();
                }
                let mut subgroup_features = sort::Features {
                    kind: 1000225002,
                    next: features.pNext,
                    size_control: 0,
                    full: 0,
                };
                if sorting {
                    features.pNext = (&mut subgroup_features as *mut sort::Features).cast();
                }
                let mut extended_types: VkPhysicalDeviceShaderSubgroupExtendedTypesFeatures = zero();
                extended_types.sType = 1000175000;
                if subgroup_extended_types {
                    extended_types.pNext = features.pNext;
                    features.pNext = (&mut extended_types as *mut VkPhysicalDeviceShaderSubgroupExtendedTypesFeatures).cast();
                }
                let mut reconvergence: VkPhysicalDeviceShaderMaximalReconvergenceFeaturesKHR = zero();
                reconvergence.sType = 1000434000;
                if maximal_reconvergence {
                    reconvergence.pNext = features.pNext;
                    features.pNext = (&mut reconvergence
                        as *mut VkPhysicalDeviceShaderMaximalReconvergenceFeaturesKHR)
                        .cast();
                }
                let mut indexing: texture::IndexingFeatures = zero();
                indexing.kind = 1000161001;
                if texture_indexing {
                    indexing.next = features.pNext;
                    features.pNext = (&mut indexing as *mut texture::IndexingFeatures).cast();
                }
                features_fn(physical, &mut features);
                if atomic64 && atomics.shaderBufferInt64Atomics == 0 {
                    unavailable.push(format!("{device_name}: {}", "64位dataset原子需要shaderBufferInt64Atomics，目标GPU不支持"));
                    continue 'physical;
                }
                if shared_atomic64 && atomics.shaderSharedInt64Atomics == 0 {
                    unavailable.push(format!("{device_name}: {}", "64位tile原子需要shaderSharedInt64Atomics，目标GPU不支持"));
                    continue 'physical;
                }
                if integer8 && (arithmetic8.shaderInt8 == 0 || storage8.storageBuffer8BitAccess == 0) {
                    unavailable.push(format!("{device_name}: {}", "程序需要shaderInt8与storageBuffer8BitAccess，目标GPU不支持"));
                    continue 'physical;
                }
                if integer16 && (features.features[41] == 0 || storage16.storageBuffer16BitAccess == 0) {
                    unavailable.push(format!("{device_name}: {}", "程序需要shaderInt16与storageBuffer16BitAccess，目标GPU不支持"));
                    continue 'physical;
                }
                if subgroup_extended_types && extended_types.shaderSubgroupExtendedTypes == 0 {
                    unavailable.push(format!("{device_name}: {}", "64位subgroup运算需要设备支持shaderSubgroupExtendedTypes"));
                    continue 'physical;
                }
                // VkPhysicalDeviceDescriptorIndexingFeatures::shaderSampledImageArrayNonUniformIndexing。
                if texture_indexing && indexing.capabilities[4] == 0 {
                    unavailable.push(format!("{device_name}: {}", "纹理资源数组需要 sampled-image 非一致下标能力"));
                    continue 'physical;
                }
                if maximal_reconvergence && reconvergence.shaderMaximalReconvergence == 0 {
                    unavailable.push(format!("{device_name}: {}", "append 需要 shaderMaximalReconvergence，以保证分歧路径的活动集合合并预留"));
                    continue 'physical;
                }
                let get: unsafe extern "system" fn(VkPhysicalDevice, *mut VkPhysicalDeviceProperties2) =
                    owner
                        .library
                        .get(instance, c"vkGetPhysicalDeviceProperties2")?;
                let mut ray_properties: ray::Properties = zero();
                ray_properties.kind = 1000150014;
                let mut control: sort::ControlProperties = zero();
                control.kind = 1000225000;
                let mut float_controls: VkPhysicalDeviceFloatControlsProperties = zero();
                float_controls.sType = 1000197000;
                if ray_query {
                    float_controls.pNext = (&mut ray_properties as *mut ray::Properties).cast();
                }
                if sorting {
                    control.next = float_controls.pNext;
                    float_controls.pNext = (&mut control as *mut sort::ControlProperties).cast();
                }
                let mut subgroup: VkPhysicalDeviceSubgroupProperties = zero();
                subgroup.sType = 1000094000;
                subgroup.pNext =
                    (&mut float_controls as *mut VkPhysicalDeviceFloatControlsProperties).cast();
                let mut properties2: VkPhysicalDeviceProperties2 = zero();
                properties2.kind = 1000059001;
                properties2.next = (&mut subgroup as *mut VkPhysicalDeviceSubgroupProperties).cast();
                get(physical, &mut properties2);
                let float32_supported = float_controls.shaderDenormPreserveFloat32 != 0
                    && float_controls.shaderSignedZeroInfNanPreserveFloat32 != 0;
                let float64_supported = float_controls.shaderDenormPreserveFloat64 != 0
                    && float_controls.shaderSignedZeroInfNanPreserveFloat64 != 0;
                let subgroup_operations = if subgroup.stages & 0x20 != 0 { subgroup.operations } else { 0 };
                if float32_preserve && !float32_supported {
                    unavailable.push(format!("{device_name}: f32运算需要DenormPreserve与SignedZeroInfNanPreserve"));
                    continue 'physical;
                }
                if float64_preserve && !float64_supported {
                    unavailable.push(format!("{device_name}: f64运算需要DenormPreserve与SignedZeroInfNanPreserve"));
                    continue 'physical;
                }
                if subgroup_operations & required_subgroup != required_subgroup {
                    unavailable.push(format!("{device_name}: compute subgroup操作需要{required_subgroup:#x}，实际{subgroup_operations:#x}"));
                    continue 'physical;
                }
                if sorting {
                    if subgroup_features.size_control == 0
                        || subgroup_features.full == 0
                        || control.minimum > 32
                        || control.maximum < 32
                        || control.max_groups < 2
                        || control.stages & 0x20 == 0
                        || subgroup.stages & 0x20 == 0
                        || subgroup.operations & 9 != 9
                    {
                        unavailable.push(format!("{device_name}: {}", "稳定键排序需要 compute ballot、32 lane subgroup 与 full subgroup 能力"));
                        continue 'physical;
                    }
                }
                if address.bufferDeviceAddress == 0 || features.features[40] == 0 {
                    unavailable.push(format!("{device_name}: {}", "目标 GPU 需要 bufferDeviceAddress 和 shaderInt64"));
                    continue 'physical;
                }
                if ray_query
                    && (query_features.ray_query == 0 || acceleration_features.acceleration == 0)
                {
                    unavailable.push(format!("{device_name}: {}", "程序需要 accelerationStructure 与 rayQuery，目标设备不支持"));
                    continue 'physical;
                }
                if position_fetch && position_features.position_fetch == 0 {
                    unavailable.push(format!("{device_name}: {}", "程序读取命中顶点，需要 rayTracingPositionFetch"));
                    continue 'physical;
                }
                if float64 && features.features[39] == 0 {
                    unavailable.push(format!("{device_name}: {}", "程序需要 shaderFloat64，目标 GPU 不支持"));
                    continue 'physical;
                }
                if acceleration_count > ray_properties.per_stage as usize
                    || acceleration_count > ray_properties.per_set as usize
                {
                    unavailable.push(format!("{device_name}: AS绑定需要{acceleration_count}项，maxPerStageDescriptorAccelerationStructures={}，maxDescriptorSetAccelerationStructures={}", ray_properties.per_stage, ray_properties.per_set));
                    continue 'physical;
                }
                features.features.fill(0);
                features.features[39] = u32::from(float64); // VkPhysicalDeviceFeatures::shaderFloat64.
                features.features[40] = 1; // VkPhysicalDeviceFeatures::shaderInt64。
                features.features[41] = u32::from(integer16); // VkPhysicalDeviceFeatures::shaderInt16。
                let position_enabled = ray::PositionFetchFeatures {
                    kind: 1000481000,
                    next: null_mut(),
                    position_fetch: 1,
                };
                let query_enabled = ray::QueryFeatures {
                    kind: 1000348013,
                    next: if position_fetch {
                        (&position_enabled as *const ray::PositionFetchFeatures)
                            .cast_mut()
                            .cast()
                    } else {
                        null_mut()
                    },
                    ray_query: 1,
                };
                let acceleration_enabled = ray::AccelerationFeatures {
                    kind: 1000150013,
                    next: (&query_enabled as *const ray::QueryFeatures)
                        .cast_mut()
                        .cast(),
                    acceleration: 1,
                    capture: 0,
                    indirect: 0,
                    host: 0,
                    update_after_bind: 0,
                };
                let address_enabled = VkPhysicalDeviceBufferDeviceAddressFeatures {
                    sType: 1000257000,
                    pNext: if ray_query {
                        (&acceleration_enabled as *const ray::AccelerationFeatures)
                            .cast_mut()
                            .cast()
                    } else {
                        null_mut()
                    },
                    bufferDeviceAddress: 1,
                    bufferDeviceAddressCaptureReplay: 0,
                    bufferDeviceAddressMultiDevice: 0,
                };
                features.pNext = (&address_enabled
                    as *const VkPhysicalDeviceBufferDeviceAddressFeatures)
                    .cast_mut()
                    .cast();
                if atomic64 || shared_atomic64 {
                    atomics.pNext = features.pNext;
                    atomics.shaderBufferInt64Atomics = u32::from(atomic64);
                    atomics.shaderSharedInt64Atomics = u32::from(shared_atomic64);
                    features.pNext = (&mut atomics as *mut VkPhysicalDeviceShaderAtomicInt64Features).cast();
                }
                if integer8 {
                    storage8.pNext = features.pNext;
                    storage8.storageBuffer8BitAccess = 1;
                    storage8.uniformAndStorageBuffer8BitAccess = 0;
                    storage8.storagePushConstant8 = 0;
                    arithmetic8.pNext = (&mut storage8 as *mut VkPhysicalDevice8BitStorageFeatures).cast();
                    arithmetic8.shaderFloat16 = 0;
                    arithmetic8.shaderInt8 = 1;
                    features.pNext = (&mut arithmetic8 as *mut VkPhysicalDeviceShaderFloat16Int8Features).cast();
                }
                if integer16 {
                    storage16.pNext = features.pNext;
                    storage16.storageBuffer16BitAccess = 1;
                    storage16.uniformAndStorageBuffer16BitAccess = 0;
                    storage16.storagePushConstant16 = 0;
                    storage16.storageInputOutput16 = 0;
                    features.pNext = (&mut storage16 as *mut VkPhysicalDevice16BitStorageFeatures).cast();
                }
                let subgroup_enabled = sort::Features {
                    kind: 1000225002,
                    next: features.pNext,
                    size_control: 1,
                    full: 1,
                };
                if sorting {
                    features.pNext = (&subgroup_enabled as *const sort::Features)
                        .cast_mut()
                        .cast();
                }
                let extended_types_enabled = VkPhysicalDeviceShaderSubgroupExtendedTypesFeatures {
                    sType: 1000175000,
                    pNext: features.pNext,
                    shaderSubgroupExtendedTypes: 1,
                };
                if subgroup_extended_types {
                    features.pNext = (&extended_types_enabled as *const VkPhysicalDeviceShaderSubgroupExtendedTypesFeatures).cast_mut().cast();
                }
                let reconvergence_enabled = VkPhysicalDeviceShaderMaximalReconvergenceFeaturesKHR {
                    sType: 1000434000,
                    pNext: features.pNext,
                    shaderMaximalReconvergence: 1,
                };
                if maximal_reconvergence {
                    features.pNext = (&reconvergence_enabled
                        as *const VkPhysicalDeviceShaderMaximalReconvergenceFeaturesKHR)
                        .cast_mut()
                        .cast();
                }
                let mut indexing_enabled: texture::IndexingFeatures = zero();
                indexing_enabled.kind = 1000161001;
                indexing_enabled.capabilities[4] = 1;
                if texture_indexing {
                    indexing_enabled.next = features.pNext;
                    features.pNext = (&indexing_enabled as *const texture::IndexingFeatures)
                        .cast_mut()
                        .cast();
                }
                let mut count = 0;
                families_fn(physical, &mut count, null_mut());
                let mut families = vec![zero::<VkQueueFamilyProperties>(); count as usize];
                families_fn(physical, &mut count, families.as_mut_ptr());
                let Some((family, queue_properties)) = families.iter().enumerate()
                    .find(|(_, q)| q.queueFlags & 2 != 0 && q.queueCount > 0)
                else {
                    unavailable.push(format!("{device_name}: 缺少计算队列"));
                    continue 'physical;
                };
                let priority = 1.0;
                let mut q: VkDeviceQueueCreateInfo = zero();
                q.sType = 2;
                q.queueFamilyIndex = family as u32;
                q.queueCount = 1;
                q.pQueuePriorities = &priority;
                let mut device_extensions = Vec::new();
                if maximal_reconvergence {
                    device_extensions.push(c"VK_KHR_shader_maximal_reconvergence".as_ptr());
                }
                if sorting {
                    device_extensions.push(c"VK_EXT_subgroup_size_control".as_ptr());
                }
                if presentation {
                    device_extensions.push(c"VK_KHR_swapchain".as_ptr());
                }
                if ray_query {
                    device_extensions.extend([
                        c"VK_KHR_acceleration_structure".as_ptr(),
                        c"VK_KHR_ray_query".as_ptr(),
                        c"VK_KHR_deferred_host_operations".as_ptr(),
                    ]);
                }
                if position_fetch {
                    device_extensions.push(c"VK_KHR_ray_tracing_position_fetch".as_ptr());
                }
                let enumerate: unsafe extern "system" fn(
                    VkPhysicalDevice, *const c_char, *mut u32, *mut ExtensionProperties,
                ) -> i32 = owner.library.get(instance, c"vkEnumerateDeviceExtensionProperties")?;
                let mut count = 0;
                check(enumerate(physical, null(), &mut count, null_mut()), "vkEnumerateDeviceExtensionProperties(count)")?;
                let mut extensions: Vec<ExtensionProperties> = (0..count).map(|_| zero()).collect();
                check(enumerate(physical, null(), &mut count, extensions.as_mut_ptr()), "vkEnumerateDeviceExtensionProperties")?;
                let extensions = &extensions[..count as usize];
                for extension in &device_extensions {
                    let required = CStr::from_ptr(*extension);
                    if !extensions.iter().any(|e| CStr::from_ptr(e.name.as_ptr()) == required) {
                        unavailable.push(format!("{device_name}: 需要设备扩展{}", required.to_string_lossy()));
                        continue 'physical;
                    }
                }
                let memory_budget = if extensions.iter().any(|e| CStr::from_ptr(e.name.as_ptr()) == c"VK_EXT_memory_budget") {
                    device_extensions.push(c"VK_EXT_memory_budget".as_ptr());
                    Some(memory::BudgetQuery::load(&owner)?)
                } else { None };
                let max_buffer_size = memory::max_buffer_size(&owner, physical, properties.apiVersion)?;
                let mut ci: VkDeviceCreateInfo = zero();
                ci.sType = 3;
                ci.pNext = (&features as *const VkPhysicalDeviceFeatures2).cast();
                ci.queueCreateInfoCount = 1;
                ci.pQueueCreateInfos = &q;
                ci.enabledExtensionCount = device_extensions.len() as u32;
                ci.ppEnabledExtensionNames = device_extensions.as_ptr();
                let mut device = null_mut();
                check(
                    create_device(physical, &ci, null(), &mut device),
                    "vkCreateDevice",
                )?;
                let device_owner = Rc::new(DeviceOwner {
                    device,
                    destroy: destroy_device,
                    _instance: owner,
                });
                let functions = Functions::load(device, get_device)?;
                let ray = if ray_query {
                    Some(ray::RayDevice::load(device, get_device, ray_properties)?)
                } else {
                    None
                };
                let mut queue = null_mut();
                (functions.vkGetDeviceQueue)(device, family as u32, 0, &mut queue);
                let mut memory = zero();
                memory_fn(physical, &mut memory);
                return Ok(Self {
                    inner: Rc::new(Inner {
                        owner: device_owner,
                        functions,
                        physical,
                        queue,
                        family: family as u32,
                        memory,
                        memory_budget,
                        buffer_memory: RefCell::default(),
                        samplers: RefCell::default(),
                        max_buffer_size,
                        properties,
                        timestamp_bits: queue_properties.timestampValidBits,
                        ray,
                        sorting,
                        maximal_reconvergence,
                        texture_indexing,
                        position_fetch,
                        subgroup_extended_types,
                        subgroup_operations,
                        next_submission: Cell::new(0),
                        float64,
                        integer8,
                        integer16,
                        atomic64,
                        shared_atomic64,
                        float64_preserve: float64_supported,
                        float32_preserve: float32_supported,
                    }),
                });
            }
        }
        Err(error(format!("没有满足本次程序需求的Vulkan计算设备：{unavailable:?}")))
    }
    pub fn name(&self) -> String {
        unsafe { CStr::from_ptr(self.inner.properties.deviceName.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }
    /// 显式复制闭合只读资源图；共享来源在本次传输内只复制一次。
    pub fn upload(&self, data: impl Into<crate::cpu::CpuDataset>) -> Result<GpuDataset> {
        let data = data.into();
        transfer::Upload::accept(&data)?;
        let mut upload = transfer::Upload::new(self);
        let root = upload.record_root(data)?;
        Ok(upload.submit()?.root(root))
    }
    pub fn upload_with_views(
        &self,
        data: Dataset,
        views: Vec<ViewBinding<'_>>,
    ) -> Result<GpuDataset> {
        let views = data.bind_views(views, |source| {
            if !Rc::ptr_eq(&self.inner, &source.0.allocation.inner) {
                return Err(error("view 资源不属于当前 GPU 设备"));
            }
            Ok((&source.0.ty, Rc::as_ptr(&source.0) as usize))
        })?;
        let mut target = GpuDataset::new(self.inner.clone(), data.ty, data.stride, data.count)?;
        target.owned_storage()?.views = views;
        self.upload_payload(&mut target, &data.words)?;
        Ok(target)
    }
    /// 分配原生存储；内容由实际B生产者写入，不额外提交清零工作。
    /// 调用者须在读取前初始化会被消费的字段；有view的非空记录仍需真实绑定。
    pub fn allocate(
        &self,
        program: &dyn ProgramInterface,
        binding: usize,
        count: u32,
    ) -> Result<GpuDataset> {
        let schema = program
            .buffers()
            .get(binding)
            .ok_or_else(|| error("数据绑定编号越界"))?;
        if count != 0 && schema.ty.contains_views() {
            return Err(error(
                "非空 view 数据需要 upload_with_views 绑定资源，不能以零地址初始化",
            ));
        }
        GpuDataset::new(self.inner.clone(), schema.ty.clone(), schema.stride, count)
    }
    pub fn zeroed(
        &self,
        program: &dyn ProgramInterface,
        binding: usize,
        count: u32,
    ) -> Result<GpuDataset> {
        let mut target = self.allocate(program, binding, count)?;
        let bytes = target.stride as u64 * count as u64;
        if bytes > 0 {
            let commands = Commands::new(self.inner.clone())?;
            for page in &target.pages {
                unsafe {
                    (self.inner.functions.vkCmdFillBuffer)(
                        commands.command,
                        page.buffer,
                        0,
                        page.size,
                        0,
                    );
                }
            }
            target.last_use = Some(self.submit_dataset_transfer(commands, Vec::new(), false)?);
        }
        Ok(target)
    }
    /// 读回闭合资源图并绑定真实主机所有者，不返回可执行的设备地址副本。
    pub fn download(&self, data: &GpuDataset) -> Result<Dataset> {
        if !Rc::ptr_eq(&self.inner, &data.allocation.inner) {
            return Err(error("数据集属于另一设备"));
        }
        Readback::new(self).download_graph(data)
    }
    pub fn load(
        &self,
        image: blanguage::spirv::ProgramImage<ExecutableProgram>,
    ) -> Result<GpuProgram> {
        GpuProgram::load_image(self.inner.clone(), &image)
    }
    /// 装入失败交回原错误和同一未改映像；本调用没有提交数据。
    pub fn load_template(
        &self,
        image: blanguage::spirv::ProgramImage<TemplateProgram>,
    ) -> std::result::Result<GpuTemplate, (Box<dyn std::error::Error + Send + Sync>, blanguage::spirv::ProgramImage<TemplateProgram>)> {
        self.load_template_ref(&image).map_err(|error| (error, image))
    }
    pub(crate) fn load_template_ref(
        &self,
        image: &blanguage::spirv::ProgramImage<TemplateProgram>,
    ) -> Result<GpuTemplate> {
        GpuProgram::load_image(self.inner.clone(), image).map(|code| GpuTemplate { code })
    }
    /// 真实传输同时移动存储及其域关系；任意宿主转换不能签发有效域。
    pub fn upload_domains(
        &self,
        bindings: impl Into<DomainBindings<crate::cpu::CpuDataset>>,
    ) -> Result<DomainBindings<GpuDataset>> {
        let DomainBindings { datasets, domains } = bindings.into();
        datasets.iter().try_for_each(transfer::Upload::accept)?;
        let mut upload = transfer::Upload::new(self);
        let roots = datasets
            .into_iter()
            .map(|data| upload.record_root(data))
            .collect::<Result<Vec<_>>>()?;
        let uploaded = upload.submit()?;
        let datasets = roots.into_iter().map(|root| uploaded.root(root)).collect();
        Ok(DomainBindings { datasets, domains })
    }
    pub fn download_domains(
        &self,
        bindings: DomainBindings<GpuDataset>,
    ) -> Result<DomainBindings<crate::cpu::CpuDataset>> {
        let DomainBindings { datasets, domains } = bindings;
        if datasets.iter().any(|data| !Rc::ptr_eq(&self.inner, &data.allocation.inner)) {
            return Err(error("数据集属于另一设备"));
        }
        let mut readback = Readback::new(self);
        let datasets = datasets
            .into_iter()
            .map(|data| readback.download_cpu(data))
            .collect::<Result<Vec<_>>>()?;
        Ok(DomainBindings { datasets, domains })
    }
}
#[derive(Clone, Copy)]
enum Usage {
    Transfer,
    Compute,
}
impl Usage {
    fn stage(self) -> u32 {
        match self {
            Self::Transfer => 0x1000,
            Self::Compute => 0x800,
        }
    }
    fn access(self) -> u32 {
        match self {
            Self::Transfer => 0x1000,
            Self::Compute => 0x40,
        }
    }
}
struct BindingAccess {
    producer: Usage,
    pending_read: bool,
    pending_write: bool,
}
/// 原数据存储中的buffer角色；页的逻辑覆盖不含buffer末尾或分配对齐填充。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuBufferRole {
    BindingTable,
    DataPage { index: usize, logical_offset: u64, logical_bytes: u64 },
    InstancePointers,
}
/// 当前buffer与其真实绑定区间、Vulkan内存对象的元数据；不保活资源。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuBufferMemory {
    pub role: GpuBufferRole,
    pub buffer: u64,
    pub buffer_bytes: u64,
    pub memory: u64,
    pub memory_offset: u64,
    /// 原Range大小，即本buffer的VkMemoryRequirements.size。
    pub range_bytes: u64,
    /// 同一原VkDeviceMemory的allocationSize；多个buffer可能重复报告同一块。
    pub allocation_bytes: u64,
    pub memory_type: u32,
    pub memory_properties: u32,
    pub heap_index: u32,
}
/// 同一GPU数据存储的同步元数据快照；句柄仅标识当时仍存活的原对象。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuDatasetMemory {
    /// VkDevice地址限定buffer／memory句柄的作用域，不是持久设备身份。
    pub device: usize,
    pub logical_bytes: u64,
    pub buffers: Vec<GpuBufferMemory>,
}
/// 原GPU记录的一条真实view绑定；只借用原来源，不复制或保活资源。
#[derive(Clone, Copy)]
pub struct GpuDatasetView<'a> {
    pub record: u32,
    /// 相对本条记录起点的字节偏移；不隐式读回活动载荷或生成字段路径。
    pub offset: u32,
    pub source: &'a GpuDataset,
}
/// 本绑定实际保留完成点的一次观察；不表示同存储所有别名的最后消费者。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuDatasetUse {
    /// 原VkDevice的当时作用域；不是持久设备身份。
    pub device: usize,
    /// 原队列提交序号，从0开始，仅在同一存活逻辑设备内比较。
    pub sequence: u64,
    pub complete: bool,
}
pub struct GpuDataset {
    storage: ManuallyDrop<DatasetStorage>,
    last_use: Option<Rc<Execution>>,
}
enum DatasetStorage {
    Owned(GpuStorage),
    Read(ReadDataset),
}
/// 不透明物理存储；可读借用不允许交换存储与它的执行寿命。
#[doc(hidden)]
pub struct GpuStorage {
    // allocation 持有16字节绑定头及原页数组；数据页随唯一存储转移。
    allocation: Allocation,
    pages: Vec<Allocation>,
    ty: Ty,
    stride: u32,
    count: u32,
    producer: Usage,
    views: Views<ReadDataset>,
    instances: Option<ray::InstanceInput>,
}
impl GpuStorage {
    fn page_array_address(&self) -> u64 {
        let mapped = self.allocation.mapping().unwrap();
        // PagedBuffer::table initializes the immutable page-array address in the mapped header.
        let [low, high] = unsafe { mapped.cast::<[u32; 2]>().read() };
        u64::from(low) | (u64::from(high) << 32)
    }
    fn view_header(&self) -> [u32; 6] {
        let address = self.page_array_address();
        [address as u32, (address >> 32) as u32, self.count, self.stride, 0, 0]
    }
}
impl std::ops::Deref for GpuDataset {
    type Target = GpuStorage;
    fn deref(&self) -> &GpuStorage {
        match &*self.storage {
            DatasetStorage::Owned(storage) => storage,
            DatasetStorage::Read(source) => &source.0,
        }
    }
}
impl Drop for GpuDataset {
    fn drop(&mut self) {
        // Rust 唯一析构入口移交一次物理存储；外层不再重复释放页和依赖。
        let storage = unsafe { ManuallyDrop::take(&mut self.storage) };
        if let Some(execution) = &self.last_use {
            execution.discard(storage);
        } else {
            drop(storage);
        }
    }
}
#[derive(Clone)]
pub struct ReadDataset(Rc<GpuDataset>);
impl std::ops::Deref for ReadDataset {
    type Target = GpuDataset;
    fn deref(&self) -> &GpuDataset {
        &self.0
    }
}
impl crate::DomainStorage for GpuDataset {
    fn element_count(&self) -> u32 { self.count() }
    fn read_identity(&self) -> Option<usize> {
        match &*self.storage {
            DatasetStorage::Owned(_) => None,
            DatasetStorage::Read(source) => Some(Rc::as_ptr(&source.0) as usize),
        }
    }
    fn read_binding(&self) -> Option<Self> {
        match &*self.storage {
            DatasetStorage::Owned(_) => None,
            DatasetStorage::Read(source) => Some(source.binding()),
        }
    }
}
impl DomainBindings<GpuDataset> {
    /// 冻结选中存储并保留原域关系；后续只读别名仍指向同一物理所有者。
    /// 已冻结的选中绑定保留当前最后使用点。
    pub fn freeze(self, bindings: &[usize]) -> Result<Self> {
        self.freeze_with(bindings, |data| {
            Ok(match &*data.storage {
                DatasetStorage::Owned(_) => data.freeze().into_binding(),
                DatasetStorage::Read(_) => data,
            })
        })
    }
}
pub type ViewBinding<'a> = crate::ViewBinding<'a, ReadDataset>;
impl ReadDataset {
    /// 查询同一冻结物理存储，不新增GPU页、传输或资源持有。
    pub fn memory(&self) -> GpuDatasetMemory {
        self.0.memory()
    }
    /// 将同一冻结物理存储直接交给原生read dataset绑定；不创建GPU记录、页表或传输。
    pub fn binding(&self) -> GpuDataset {
        self.clone().into_binding()
    }

    fn into_binding(self) -> GpuDataset {
        GpuDataset {
            storage: ManuallyDrop::new(DatasetStorage::Read(self)),
            last_use: None,
        }
    }

    /// 原存储的物理记录容量；GPU有效前缀仍由其域关系持有。
    pub fn count(&self) -> u32 {
        self.0.count()
    }
    pub fn bytes(&self) -> u64 {
        self.0.bytes()
    }
    pub fn allocated_bytes(&self) -> u64 {
        self.0.allocated_bytes()
    }
    pub fn page_count(&self) -> usize {
        self.0.page_count()
    }
}
impl GpuDataset {
    pub(crate) fn storage_address(&self) -> u64 {
        self.page_array_address() - u64::from(blanguage::spirv::BINDING_BYTES)
    }
    /// 借用同一物理存储的原类型；记录步幅和实际绑定分别查询。
    pub fn layout(&self) -> &Ty {
        &self.ty
    }
    pub fn stride(&self) -> u32 {
        self.stride
    }
    /// 仅比较当前借用中同一个原GpuStorage，不按页／内存块或外层绑定去重。
    pub fn same_storage(&self, other: &Self) -> bool {
        std::ptr::eq(&**self, &**other)
    }
    /// 枚举原Views的真实边；不下载记录、不解析设备地址或克隆Rc。
    pub fn view_bindings(&self) -> impl ExactSizeIterator<Item = GpuDatasetView<'_>> {
        self.views.bindings.iter().map(|&(word, source)| {
            let byte = word as u64 * 4;
            GpuDatasetView {
                record: (byte / u64::from(self.stride)) as u32,
                offset: (byte % u64::from(self.stride)) as u32,
                source: &self.views.sources[source].0,
            }
        })
    }
    /// 观察本绑定保留的原点；无本地使用时沿原冻结来源。
    /// None仅表示无保留点；不证明初始化、设备空闲或其他只读别名已完成。
    /// 原Execution可清理已完成资源，本查询不等待、提交或清除最后使用点。
    pub fn last_use(&self) -> Result<Option<GpuDatasetUse>> {
        let Some(execution) = self.dependency() else { return Ok(None); };
        let complete = execution.is_complete()?;
        Ok(Some(GpuDatasetUse {
            device: execution.point.inner.owner.device as usize,
            sequence: execution.point.sequence,
            complete,
        }))
    }
    /// 读取原buffer／分页／绑定区间；不等待GPU、读回载荷或改变拥有关系。
    pub fn memory(&self) -> GpuDatasetMemory {
        let mut buffers = vec![self.allocation.memory_info(GpuBufferRole::BindingTable)];
        for (index, page) in self.pages.iter().enumerate() {
            let page_bytes = u64::from(self.page_records()) * u64::from(self.stride);
            let logical_offset = index as u64 * page_bytes;
            buffers.push(page.memory_info(GpuBufferRole::DataPage {
                index,
                logical_offset,
                logical_bytes: (self.bytes() - logical_offset).min(page_bytes),
            }));
        }
        if let Some(pointers) = self.instances.as_ref().and_then(|input| input.pointers.as_ref()) {
            buffers.push(pointers.memory_info(GpuBufferRole::InstancePointers));
        }
        GpuDatasetMemory {
            device: self.allocation.inner.owner.device as usize,
            logical_bytes: self.bytes(),
            buffers,
        }
    }
    pub fn freeze(self) -> ReadDataset {
        if let DatasetStorage::Read(source) = &*self.storage {
            return source.clone();
        }
        ReadDataset(Rc::new(self))
    }
    fn dependency(&self) -> Option<&Rc<Execution>> {
        self.last_use.as_ref().or_else(|| match &*self.storage {
            DatasetStorage::Owned(_) => None,
            DatasetStorage::Read(source) => source.0.dependency(),
        })
    }
    fn owned_storage(&mut self) -> Result<&mut GpuStorage> {
        match &mut *self.storage {
            DatasetStorage::Owned(storage) => Ok(storage),
            DatasetStorage::Read(_) => Err(error("只读物理存储不能由原生接口改写")),
        }
    }
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
    /// 各实际buffer的Vulkan需求大小之和，不是共享VkDeviceMemory块的去重总额。
    pub fn allocated_bytes(&self) -> u64 {
        self.allocation.range_bytes()
            + self.pages.iter().map(Allocation::range_bytes).sum::<u64>()
            + self
                .instances
                .as_ref()
                .and_then(|input| input.pointers.as_ref())
                .map_or(0, Allocation::range_bytes)
    }
    fn page_records(&self) -> u32 {
        blanguage::spirv::PAGE_BYTES / self.stride
    }
    fn new(inner: Rc<Inner>, ty: Ty, stride: u32, count: u32) -> Result<Self> {
        let (usage, alignment) = if inner.ray.is_some() {
            (ray::BUILD_INPUT_READ_ONLY, 4)
        } else {
            (0, 1)
        };
        Self::allocate(inner, ty, stride, count, usage, alignment)
    }
    fn allocate(
        inner: Rc<Inner>,
        ty: Ty,
        stride: u32,
        count: u32,
        extra_usage: u32,
        alignment: u64,
    ) -> Result<Self> {
        let PagedBuffer { table: allocation, pages } =
            PagedBuffer::new(inner, stride, count, extra_usage, alignment.max(u64::from(ty.align)))?;
        Ok(Self {
            storage: ManuallyDrop::new(DatasetStorage::Owned(GpuStorage {
                allocation,
                pages,
                ty,
                stride,
                count,
                producer: Usage::Transfer,
                views: Views::default(),
                instances: None,
            })),
            last_use: None,
        })
    }
    pub fn count(&self) -> u32 {
        self.count
    }
    pub fn bytes(&self) -> u64 {
        self.stride as u64 * self.count as u64
    }
}
struct PagedBuffer {
    table: Allocation,
    pages: Vec<Allocation>,
}
impl PagedBuffer {
    fn words(inner: Rc<Inner>, count: u32, words: u32) -> Result<Self> {
        let alignment = match words {
            1 => 4,
            2 => 8,
            _ => 16,
        };
        Self::new(inner, 4 * words, count, 0, alignment)
    }
    fn new(
        inner: Rc<Inner>,
        stride: u32,
        count: u32,
        extra_usage: u32,
        alignment: u64,
    ) -> Result<Self> {
        let mut pages = Vec::new();
        let mut remaining = u64::from(count) * u64::from(stride);
        while remaining > 0 {
            let capacity = blanguage::spirv::PAGE_BYTES / stride;
            if capacity == 0 {
                return Err(error("GPU 页寻址要求单条记录能容纳于一个物理页"));
            }
            let bytes = remaining.min(u64::from(capacity) * u64::from(stride));
            pages.push(Allocation::new(
                inner.clone(),
                bytes,
                0x20023 | extra_usage,
                1,
                alignment,
            )?);
            remaining -= bytes;
        }
        let table = Self::table(inner, &pages, stride)?;
        Ok(Self { table, pages })
    }
    fn table(inner: Rc<Inner>, pages: &[Allocation], stride: u32) -> Result<Allocation> {
        let header = u64::from(blanguage::spirv::BINDING_BYTES);
        let table = Allocation::new(inner, header + pages.len() as u64 * 8, 0x20020, 6, 8)?;
        let address = table.address() + header;
        let mapped = table.mapping()?;
        // The new mapped range holds the header followed by one address per live page.
        unsafe {
            mapped.cast::<[u32; 4]>().write(binding_header(address, stride, 0));
            let entries = mapped.add(header as usize).cast::<[u32; 2]>();
            for (index, page) in pages.iter().enumerate() {
                let address = page.address();
                entries.add(index).write([address as u32, (address >> 32) as u32]);
            }
        }
        Ok(table)
    }
    fn descriptor(&self) -> VkDescriptorBufferInfo {
        self.table.descriptor()
    }
}
fn binding_header(pages: u64, stride: u32, offset: u32) -> [u32; 4] {
    [pages as u32, (pages >> 32) as u32, stride, offset]
}
struct Allocation {
    inner: Rc<Inner>,
    buffer: VkBuffer,
    memory: Option<buffer_memory::Range>,
    size: u64,
}
impl Allocation {
    fn memory_info(&self, role: GpuBufferRole) -> GpuBufferMemory {
        let memory = self.memory.as_ref().unwrap();
        let memory_type = memory.memory_type();
        let properties = self.inner.memory.memoryTypes[memory_type as usize];
        GpuBufferMemory {
            role,
            buffer: self.buffer,
            buffer_bytes: self.size,
            memory: memory.memory(),
            memory_offset: memory.offset,
            range_bytes: memory.bytes(),
            allocation_bytes: memory.allocation_bytes(),
            memory_type,
            memory_properties: properties.propertyFlags,
            heap_index: properties.heapIndex,
        }
    }
    fn descriptor(&self) -> VkDescriptorBufferInfo {
        VkDescriptorBufferInfo { buffer: self.buffer, offset: 0, range: self.size }
    }
    fn address(&self) -> u64 {
        let info = VkBufferDeviceAddressInfo {
            sType: 1000244001,
            pNext: null(),
            buffer: self.buffer,
        };
        unsafe { (self.inner.functions.vkGetBufferDeviceAddress)(self.inner.owner.device, &info) }
    }
    fn new(inner: Rc<Inner>, bytes: u64, usage: u32, flags: u32, alignment: u64) -> Result<Self> {
        let size = (bytes.div_ceil(4) * 4).max(4);
        if let Some(limit) = inner.max_buffer_size {
            if size > limit {
                return Err(error(format!(
                    "VkBuffer 请求 {size} 字节超过设备 maxBufferSize={limit}，usage={usage:#x}",
                )));
            }
        }
        unsafe {
            let mut out = Self {
                inner,
                buffer: 0,
                memory: None,
                size,
            };
            let f = &out.inner.functions;
            let device = out.inner.owner.device;
            let mut ci: VkBufferCreateInfo = zero();
            ci.sType = 12;
            ci.size = out.size;
            ci.usage = usage;
            check(
                (f.vkCreateBuffer)(device, &ci, null(), &mut out.buffer),
                "vkCreateBuffer",
            )?;
            let (required, dedicated) = buffer_memory::requirements(&out.inner, out.buffer);
            let memory = buffer_memory::allocate(&out.inner, required, dedicated, out.buffer, usage, flags, alignment)?;
            out.memory = Some(memory);
            let memory = out.memory.as_ref().unwrap();
            check(
                (f.vkBindBufferMemory)(device, out.buffer, memory.memory(), memory.offset),
                "vkBindBufferMemory",
            )?;
            Ok(out)
        }
    }
    fn range_bytes(&self) -> u64 {
        self.memory.as_ref().unwrap().bytes()
    }
    fn mapping(&self) -> Result<*mut u8> {
        self.memory.as_ref().unwrap().pointer()
            .ok_or_else(|| error("只能映射 host-visible 数据传输内存"))
    }
    fn write_with<T: Copy>(&self, words: &[T], encode: impl FnOnce(&mut [u32])) -> Result<()> {
        let pointer = self.mapping()?.cast::<u32>();
        let bytes = std::mem::size_of_val(words);
        unsafe {
            std::ptr::copy_nonoverlapping(words.as_ptr().cast::<u8>(), pointer.cast(), bytes);
            std::ptr::write_bytes(pointer.cast::<u8>().add(bytes), 0, bytes.div_ceil(4) * 4 - bytes);
            encode(std::slice::from_raw_parts_mut(pointer, bytes.div_ceil(4)));
        }
        Ok(())
    }
    fn read_with<T>(&self, read: impl FnOnce(*const u8) -> T) -> Result<T> {
        let value = read(self.mapping()?);
        Ok(value)
    }
}
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            let f = &self.inner.functions;
            let device = self.inner.owner.device;
            if self.buffer != 0 {
                (f.vkDestroyBuffer)(device, self.buffer, null());
            }
        }
    }
}
struct Commands {
    inner: Rc<Inner>,
    pool: VkCommandPool,
    command: VkCommandBuffer,
    descriptors: VkDescriptorPool,
    queries: u64,
}
impl Commands {
    fn new(inner: Rc<Inner>) -> Result<Self> {
        Self::with_pool_flags(inner, 1)
    }
    fn with_pool_flags(inner: Rc<Inner>, flags: u32) -> Result<Self> {
        unsafe {
            let mut out = Self {
                inner,
                pool: 0,
                command: null_mut(),
                descriptors: 0,
                queries: 0,
            };
            let f = &out.inner.functions;
            let device = out.inner.owner.device;
            let mut pool: VkCommandPoolCreateInfo = zero();
            pool.sType = 39;
            pool.queueFamilyIndex = out.inner.family;
            pool.flags = flags;
            check(
                (f.vkCreateCommandPool)(device, &pool, null(), &mut out.pool),
                "vkCreateCommandPool",
            )?;
            let mut alloc: VkCommandBufferAllocateInfo = zero();
            alloc.sType = 40;
            alloc.commandPool = out.pool;
            alloc.commandBufferCount = 1;
            check(
                (f.vkAllocateCommandBuffers)(device, &alloc, &mut out.command),
                "vkAllocateCommandBuffers",
            )?;
            out.begin()?;
            Ok(out)
        }
    }
    fn begin(&self) -> Result<()> {
        let mut begin: VkCommandBufferBeginInfo = zero();
        begin.sType = 42;
        begin.flags = 1;
        unsafe {
            check(
                (self.inner.functions.vkBeginCommandBuffer)(self.command, &begin),
                "vkBeginCommandBuffer",
            )
        }
    }
    fn barrier(&self, src_stage: u32, dst_stage: u32, src_access: u32, dst_access: u32) {
        let mut b: VkMemoryBarrier = zero();
        b.sType = 46;
        b.srcAccessMask = src_access;
        b.dstAccessMask = dst_access;
        unsafe {
            (self.inner.functions.vkCmdPipelineBarrier)(
                self.command,
                src_stage,
                dst_stage,
                0,
                1,
                &b,
                0,
                null(),
                0,
                null(),
            );
        }
    }
    fn submit_with(&self, wait: &[u64], stages: &[u32], signal: &[u64]) -> Result<Completion> {
        let next = self.end_recording()?;
        Completion::submit(self.inner.clone(), next, Some(&self.batch(wait, stages, signal)))
    }
    fn resubmit(&self, point: &mut Completion) -> Result<()> {
        let next = self.end_recording()?;
        point.reset(next - 1)?;
        self.inner.enqueue(next, point.fence.get(), Some(&self.batch(&[], &[], &[])))
    }
    fn end_recording(&self) -> Result<u64> {
        let next = self.inner.next_submission()?;
        unsafe {
            check((self.inner.functions.vkEndCommandBuffer)(self.command), "vkEndCommandBuffer")?;
        }
        Ok(next)
    }
    fn batch(&self, wait: &[u64], stages: &[u32], signal: &[u64]) -> VkSubmitInfo {
        let mut submit: VkSubmitInfo = zero();
        submit.sType = 4;
        submit.commandBufferCount = 1;
        submit.pCommandBuffers = &self.command;
        submit.waitSemaphoreCount = wait.len() as u32;
        submit.pWaitSemaphores = wait.as_ptr();
        submit.pWaitDstStageMask = stages.as_ptr();
        submit.signalSemaphoreCount = signal.len() as u32;
        submit.pSignalSemaphores = signal.as_ptr();
        submit
    }
}
impl Drop for Commands {
    fn drop(&mut self) {
        unsafe {
            let f = &self.inner.functions;
            let d = self.inner.owner.device;
            if self.queries != 0 {
                (f.vkDestroyQueryPool)(d, self.queries, null());
            }
            if self.descriptors != 0 {
                (f.vkDestroyDescriptorPool)(d, self.descriptors, null());
            }
            if self.pool != 0 {
                (f.vkDestroyCommandPool)(d, self.pool, null());
            }
        }
    }
}
#[derive(Clone, Copy)]
enum TerminalPipeline {
    Canonical,
    Alternate(VkPipeline),
}

struct StagePipelines {
    main: VkPipeline,
    tail: Option<VkPipeline>,
    scan_add: Option<VkPipeline>,
    finish: Option<VkPipeline>,
    dispatch: Option<prefix::Preparation>,
    selector: Option<VkPipeline>,
    sort: Vec<VkPipeline>,
    segmented_single_group: Option<VkPipeline>,
    terminal_scans: [Option<TerminalPipeline>; 2],
}
impl StagePipelines {
    fn segmented_single_group(&self, count: u32) -> Option<VkPipeline> {
        (scan::groups(count) == 1).then_some(self.segmented_single_group).flatten()
    }

    fn scan_terminal(&self, count: u32) -> Option<TerminalPipeline> {
        self.terminal_scans[usize::from(scan::groups(count) > 1)]
    }

    fn handles(&self) -> impl Iterator<Item = VkPipeline> + '_ {
        std::iter::once(self.main)
            .chain(self.tail)
            .chain(self.scan_add)
            .chain(self.finish)
            .chain(self.dispatch.as_ref().map(|prepare| prepare.pipeline))
            .chain(self.selector)
            .chain(self.sort.iter().copied())
            .chain(self.segmented_single_group)
            .chain(self.terminal_scans.iter().filter_map(|pipeline| match pipeline {
                Some(TerminalPipeline::Alternate(handle)) => Some(*handle),
                _ => None,
            }))
    }
}

// The compute bind point belongs to this recording of one compiled layout.
struct ComputeCommands<'a> {
    commands: &'a Commands,
    layout: VkPipelineLayout,
    pipeline: Cell<Option<VkPipeline>>,
    set: Cell<Option<VkDescriptorSet>>,
}
impl<'a> ComputeCommands<'a> {
    fn new(commands: &'a Commands, layout: VkPipelineLayout) -> Self {
        Self { commands, layout, pipeline: Cell::new(None), set: Cell::new(None) }
    }

    fn bind_pipeline(&self, pipeline: VkPipeline) {
        if self.pipeline.replace(Some(pipeline)) == Some(pipeline) {
            return;
        }
        unsafe {
            (self.inner.functions.vkCmdBindPipeline)(self.command, 1, pipeline);
        }
    }

    fn bind_set(&self, set: VkDescriptorSet) {
        if self.set.replace(Some(set)) == Some(set) {
            return;
        }
        unsafe {
            (self.inner.functions.vkCmdBindDescriptorSets)(
                self.command, 1, self.layout, 0, 1, &set, 0, null(),
            );
        }
    }
}
impl std::ops::Deref for ComputeCommands<'_> {
    type Target = Commands;

    fn deref(&self) -> &Commands {
        self.commands
    }
}

struct Compiled {
    inner: Rc<Inner>,
    program: std::sync::Arc<Program>,
    set_layout: VkDescriptorSetLayout,
    layout: VkPipelineLayout,
    stages: Vec<StagePipelines>,
}
impl Drop for Compiled {
    fn drop(&mut self) {
        unsafe {
            let f = &self.inner.functions;
            let d = self.inner.owner.device;
            for handle in self.stages.iter().flat_map(StagePipelines::handles) {
                (f.vkDestroyPipeline)(d, handle, null());
            }
            if self.layout != 0 {
                (f.vkDestroyPipelineLayout)(d, self.layout, null());
            }
            if self.set_layout != 0 {
                (f.vkDestroyDescriptorSetLayout)(d, self.set_layout, null());
            }
        }
    }
}
struct Scratch {
    buffers: Vec<PagedBuffer>,
    arguments: Option<PagedBuffer>,
    sorting: Option<sort::Storage>,
}
struct ComputeResources {
    _compiled: Rc<Compiled>,
    _accelerations: Vec<Acceleration>,
    _textures: Vec<TextureInput>,
    _scratch: Vec<Scratch>,
    _binding_parameters: Option<Allocation>,
}
struct ScratchUse {
    stage: usize,
    slot: usize,
    reused: bool,
    prepare_map: bool,
}
fn pipeline(
    inner: &Inner,
    layout: VkPipelineLayout,
    words: &[u32],
    subgroup: bool,
    entry: &CStr,
    continuation: Option<u64>,
) -> Result<VkPipeline> {
    unsafe {
        let f = &inner.functions;
        let device = inner.owner.device;
        let mut ci: VkShaderModuleCreateInfo = zero();
        ci.sType = 16;
        ci.codeSize = words.len() * 4;
        ci.pCode = words.as_ptr();
        let mut module = 0;
        check(
            (f.vkCreateShaderModule)(device, &ci, null(), &mut module),
            "vkCreateShaderModule",
        )?;
        let mut pipeline: VkComputePipelineCreateInfo = zero();
        pipeline.sType = 29;
        pipeline.layout = layout;
        pipeline.basePipelineIndex = -1;
        pipeline.stage.sType = 18;
        pipeline.stage.stage = 0x20;
        pipeline.stage.module = module;
        let required = sort::RequiredSize {
            kind: 1000225001,
            next: null(),
            size: 32,
        };
        if subgroup {
            pipeline.stage.flags = 2;
            pipeline.stage.pNext = (&required as *const sort::RequiredSize).cast();
        }
        pipeline.stage.pName = entry.as_ptr();
        let map = VkSpecializationMapEntry {
            constantID: blanguage::spirv::DISPATCH_CONTINUATION_SPEC_ID,
            offset: 0,
            size: size_of::<u64>(),
        };
        let data = continuation.map(|value| value.to_le());
        let specialization = data.as_ref().map(|value| VkSpecializationInfo {
            mapEntryCount: 1,
            pMapEntries: &map,
            dataSize: size_of::<u64>(),
            pData: std::ptr::from_ref(value).cast(),
        });
        pipeline.stage.pSpecializationInfo = specialization.as_ref()
            .map_or(null(), |info| std::ptr::from_ref(info).cast());
        let mut handle = 0;
        let result = (f.vkCreateComputePipelines)(device, 0, 1, &pipeline, null(), &mut handle);
        (f.vkDestroyShaderModule)(device, module, null());
        check(result, "vkCreateComputePipelines")?;
        Ok(handle)
    }
}
enum DescriptorStep {
    Single,
    Capture,
    Reuse(usize),
}

struct DescriptorWriter<'a> {
    compiled: &'a Compiled,
    pool: u64,
    infos: &'a [VkDescriptorBufferInfo],
    writes: Vec<VkWriteDescriptorSet>,
    _accelerations: &'a [Acceleration],
    _structures: Vec<ray::WriteAcceleration>,
    _images: Vec<texture::DescriptorImageInfo>,
    step: DescriptorStep,
    step_sets: Vec<u64>,
}
impl<'a> DescriptorWriter<'a> {
    fn new(
        commands: &Commands,
        compiled: &'a Compiled,
        infos: &'a [VkDescriptorBufferInfo],
        accelerations: &'a [Acceleration],
        textures: &[TextureInput],
    ) -> Self {
        let structures: Vec<_> = accelerations.iter().map(|structure| ray::WriteAcceleration {
            kind: 1000150007,
            next: null(),
            count: 1,
            structures: structure.handle(),
        }).collect();
        let images: Vec<_> = textures.iter().map(TextureInput::descriptor).collect();
        let mut writes = Vec::with_capacity(
            infos.len() + accelerations.len() + compiled.program.textures().len() + 4,
        );
        for (binding, info) in infos.iter().enumerate() {
            writes.push(Self::buffer_write(binding, info));
        }
        // The backing allocations remain fixed while the writer borrows their addresses.
        for structure in &structures {
            let mut write: VkWriteDescriptorSet = zero();
            write.sType = 35;
            write.pNext = (structure as *const ray::WriteAcceleration).cast();
            write.dstBinding = writes.len() as u32;
            write.descriptorCount = 1;
            write.descriptorType = 1000150000;
            writes.push(write);
        }
        let mut offset = 0;
        for texture in compiled.program.textures() {
            let mut write: VkWriteDescriptorSet = zero();
            write.sType = 35;
            write.dstBinding = writes.len() as u32;
            write.descriptorCount = texture.elements();
            write.descriptorType = 1;
            write.pImageInfo = (&images[offset] as *const texture::DescriptorImageInfo).cast();
            writes.push(write);
            offset += texture.elements() as usize;
        }
        Self {
            compiled,
            pool: commands.descriptors,
            infos,
            writes,
            _accelerations: accelerations,
            _structures: structures,
            _images: images,
            step: DescriptorStep::Single,
            step_sets: Vec::new(),
        }
    }

    fn buffer_write(binding: usize, info: &VkDescriptorBufferInfo) -> VkWriteDescriptorSet {
        let mut write: VkWriteDescriptorSet = zero();
        write.sType = 35;
        write.dstBinding = binding as u32;
        write.descriptorCount = 1;
        write.descriptorType = 7;
        write.pBufferInfo = info;
        write
    }

    fn binding(&self, binding: usize) -> VkDescriptorBufferInfo {
        self.infos[binding]
    }

    fn set(
        &mut self,
        replacements: &[Option<(usize, VkDescriptorBufferInfo)>],
        auxiliary: Option<VkDescriptorBufferInfo>,
        indirect: Option<VkDescriptorBufferInfo>,
        selection: Option<VkDescriptorBufferInfo>,
        scan_source: Option<VkDescriptorBufferInfo>,
    ) -> Result<u64> {
        if let DescriptorStep::Reuse(index) = &mut self.step {
            let set = self.step_sets[*index];
            *index += 1;
            return Ok(set);
        }
        let compiled = self.compiled;
        let f = &compiled.inner.functions;
        let device = compiled.inner.owner.device;
        let mut alloc: VkDescriptorSetAllocateInfo = zero();
        alloc.sType = 34;
        alloc.descriptorPool = self.pool;
        alloc.descriptorSetCount = 1;
        alloc.pSetLayouts = &compiled.set_layout;
        let mut set = 0;
        check(
            unsafe { (f.vkAllocateDescriptorSets)(device, &alloc, &mut set) },
            "vkAllocateDescriptorSets",
        )?;

        let base = self.writes.len();
        let info_count = self.infos.len();
        for write in &mut self.writes {
            write.dstSet = set;
        }
        // Apply replacements in declaration order so the final one for a root slot wins.
        for (binding, info) in replacements.iter().flatten() {
            if let Some(write) = self.writes[..info_count].get_mut(*binding) {
                write.pBufferInfo = info;
            }
        }
        let auxiliary_infos = [
            (info_count, auxiliary),
            (compiled.program.indirect_buffer(), indirect),
            (compiled.program.selection_buffer(), selection),
            (compiled.program.scan_source_buffer(), scan_source),
        ];
        let resources = base - info_count;
        for (binding, info) in &auxiliary_infos {
            if let Some(info) = info {
                let mut write = Self::buffer_write(binding + resources, info);
                write.dstSet = set;
                self.writes.push(write);
            }
        }
        unsafe {
            (f.vkUpdateDescriptorSets)(device, self.writes.len() as u32, self.writes.as_ptr(), 0, null());
        }
        self.writes.truncate(base);
        for (binding, _) in replacements.iter().flatten() {
            if let Some(write) = self.writes[..info_count].get_mut(*binding) {
                write.pBufferInfo = &self.infos[*binding];
            }
        }
        if matches!(self.step, DescriptorStep::Capture) {
            self.step_sets.push(set);
        }
        Ok(set)
    }
}
#[derive(Clone)]
pub struct GpuProgram {
    compiled: Rc<Compiled>,
    partition_elements: NonZeroU32,
    capture_timestamps: bool,
}
#[derive(Clone)]
pub struct GpuTemplate {
    code: GpuProgram,
}
pub struct Bindings<D = Vec<GpuDataset>> {
    pub datasets: D,
    pub accelerations: Vec<Acceleration>,
    pub textures: Vec<TextureInput>,
}
impl From<Vec<GpuDataset>> for Bindings {
    fn from(datasets: Vec<GpuDataset>) -> Self {
        Self {
            datasets,
            accelerations: Vec::new(),
            textures: Vec::new(),
        }
    }
}
impl From<Bindings> for Bindings<DomainBindings<GpuDataset>> {
    fn from(raw: Bindings) -> Self {
        Self {
            datasets: raw.datasets.into(),
            accelerations: raw.accelerations,
            textures: raw.textures,
        }
    }
}
impl From<Vec<GpuDataset>> for Bindings<DomainBindings<GpuDataset>> {
    fn from(datasets: Vec<GpuDataset>) -> Self {
        Self {
            datasets: datasets.into(),
            accelerations: Vec::new(),
            textures: Vec::new(),
        }
    }
}
impl From<DomainBindings<GpuDataset>> for Bindings<DomainBindings<GpuDataset>> {
    fn from(datasets: DomainBindings<GpuDataset>) -> Self {
        Self {
            datasets,
            accelerations: Vec::new(),
            textures: Vec::new(),
        }
    }
}
impl GpuProgram {
    pub fn program(&self) -> &Program {
        &self.compiled.program
    }
    fn load_image<P>(inner: Rc<Inner>, image: &blanguage::spirv::ProgramImage<P>) -> Result<Self> {
        let program = image.program();
        let has_pipeline = !program.stages().is_empty();
        if program.requires_sort() && !inner.sorting {
            return Err(error(
                "排序程序必须以 Device::for_program 声明 subgroup 能力",
            ));
        }
        if program.requires_maximal_reconvergence() && !inner.maximal_reconvergence {
            return Err(error(
                "append 程序必须以 Device::for_program 声明 maximal reconvergence 能力",
            ));
        }
        if program.requirements().float64 && !inner.float64 {
            return Err(error("f64 程序必须以 Device::for_program 声明 shaderFloat64 能力"));
        }
        if program.requirements().integer8 && !inner.integer8 {
            return Err(error("8位整数程序须以Device::for_program声明设备能力"));
        }
        if program.requirements().integer16 && !inner.integer16 {
            return Err(error("16位整数程序须以Device::for_program声明设备能力"));
        }
        if program.requirements().atomic64 && !inner.atomic64 {
            return Err(error("64位dataset原子须以Device::for_program声明设备能力"));
        }
        if program.requirements().shared_atomic64 && !inner.shared_atomic64 {
            return Err(error("64位tile原子须以Device::for_program声明shared设备能力"));
        }
        if program.requirements().float64_preserve && !inner.float64_preserve {
            return Err(error("f64 运算需要设备支持64位 DenormPreserve 与 SignedZeroInfNanPreserve"));
        }
        let caps = program.subgroup_capabilities();
        if caps.extended_types && !inner.subgroup_extended_types {
            return Err(error("64位subgroup程序必须以Device::for_program声明扩展类型能力"));
        }
        if program.requirements().float32_preserve && !inner.float32_preserve {
            return Err(error(
                "f32 运算需要设备支持 DenormPreserve 与 SignedZeroInfNanPreserve",
            ));
        }
        let required = u32::from(caps.basic)
            | (u32::from(caps.vote) << 1)
            | (u32::from(caps.arithmetic) << 2)
            | (u32::from(caps.ballot) << 3)
            | (u32::from(caps.shuffle) << 4)
            | (u32::from(caps.shuffle_relative) << 5);
        if inner.subgroup_operations & required != required {
            return Err(error(format!(
                "GPU 不支持程序需要的 compute subgroup 操作：需要 {required:#x}，实际 {:#x}",
                inner.subgroup_operations
            )));
        }
        let acceleration_count = program.accelerations().len();
        if acceleration_count != 0 {
            let ray = inner
                .ray
                .as_ref()
                .ok_or_else(|| error("光线查询程序必须以 Device::for_program 声明设备能力"))?;
            if has_pipeline && (acceleration_count > ray.properties.per_stage as usize
                || acceleration_count > ray.properties.per_set as usize)
            {
                return Err(error("加速结构绑定数量超出设备能力"));
            }
        }
        if program.requires_ray_position_fetch() && !inner.position_fetch {
            return Err(error(
                "命中顶点查询必须以 Device::for_program 声明位置读取能力",
            ));
        }
        if program.requires_texture_indexing() && !inner.texture_indexing {
            return Err(error("纹理数组程序必须以 Device::for_program 声明设备能力"));
        }
        let auxiliary = program.auxiliary_buffers();
        if has_pipeline {
            let limits = &inner.properties.limits;
            let texture_count = program.texture_count();
            if texture_count > limits.maxPerStageDescriptorSampledImages as usize
                || texture_count > limits.maxPerStageDescriptorSamplers as usize
                || texture_count > limits.maxDescriptorSetSampledImages as usize
                || texture_count > limits.maxDescriptorSetSamplers as usize
            {
                return Err(error("纹理绑定数量超出设备能力"));
            }
            let storage_count = program.buffers().len() + auxiliary;
            if program.uniform_bytes() > limits.maxPushConstantsSize
                || storage_count > limits.maxPerStageDescriptorStorageBuffers as usize
                || storage_count > limits.maxDescriptorSetStorageBuffers as usize
                || storage_count + texture_count > limits.maxPerStageResources as usize
                || program.stages().iter().any(|s| {
                    s.workgroup_width() > limits.maxComputeWorkGroupInvocations
                        || s.workgroup_width() > limits.maxComputeWorkGroupSize[0]
                        || s.shared_bytes() > limits.maxComputeSharedMemorySize as u64
                })
            {
                return Err(error("计算程序超出目标设备的资源能力"));
            }
        }
        let mut out = Compiled {
            inner,
            program: image.shared_program(),
            set_layout: 0,
            layout: 0,
            stages: Vec::new(),
        };
        if has_pipeline {
            unsafe {
                let f = &out.inner.functions;
                let device = out.inner.owner.device;
                let bindings: Vec<_> = (0..out.program.buffers().len())
                    .map(|_| (7, 1))
                    .chain((0..out.program.accelerations().len()).map(|_| (1000150000, 1)))
                    .chain(out.program.textures().iter().map(|t| (1, t.elements())))
                    .chain((0..auxiliary).map(|_| (7, 1)))
                    .enumerate()
                    .map(|(i, (kind, count))| VkDescriptorSetLayoutBinding {
                        binding: i as u32,
                        descriptorType: kind,
                        descriptorCount: count,
                        stageFlags: 0x20,
                        pImmutableSamplers: null(),
                    })
                    .collect();
                let mut ci: VkDescriptorSetLayoutCreateInfo = zero();
                ci.sType = 32;
                ci.bindingCount = bindings.len() as u32;
                ci.pBindings = bindings.as_ptr();
                check(
                    (f.vkCreateDescriptorSetLayout)(device, &ci, null(), &mut out.set_layout),
                    "vkCreateDescriptorSetLayout",
                )?;
                let range = VkPushConstantRange {
                    stageFlags: 0x20,
                    offset: 0,
                    size: out.program.uniform_bytes(),
                };
                let mut layout: VkPipelineLayoutCreateInfo = zero();
                layout.sType = 30;
                layout.setLayoutCount = 1;
                layout.pSetLayouts = &out.set_layout;
                layout.pushConstantRangeCount = 1;
                layout.pPushConstantRanges = &range;
                check(
                    (f.vkCreatePipelineLayout)(device, &layout, null(), &mut out.layout),
                    "vkCreatePipelineLayout",
                )?;
                for (stage, shader) in image.stages().iter().enumerate() {
                    let make = |words: &[u32]| pipeline(&out.inner, out.layout, words, false, c"main", None);
                    let stage_layout = &out.program.stages()[stage];
                    let entry = shader.dispatch.as_deref().map(prefix::entry);
                    let packed = entry == Some(prefix::PREPARE_ENTRY);
                    let continuation = packed.then_some(blanguage::spirv::DISPATCH_RECORD_BYTES as u64);
                    // The compiled owner records each handle before another creation can fail.
                    out.stages.push(StagePipelines {
                        main: make(&shader.main)?,
                        tail: None,
                        scan_add: None,
                        finish: None,
                        dispatch: None,
                        selector: None,
                        sort: Vec::new(),
                        segmented_single_group: None,
                        terminal_scans: [None; 2],
                    });
                    let pipelines = &mut out.stages[stage];
                    pipelines.tail = shader.tail.as_deref().map(make).transpose()?;
                    pipelines.scan_add = shader.scan_add.as_deref().map(make).transpose()?;
                    pipelines.finish = shader.finish.as_ref().map(|(_, words)| {
                        let offset = if stage_layout.selection.is_some() { continuation } else { None };
                        pipeline(&out.inner, out.layout, words, false, c"main", offset)
                    }).transpose()?;
                    pipelines.dispatch = shader.dispatch.as_deref().zip(entry).map(|(words, name)| {
                        let offset = if matches!(stage_layout.operation, blanguage::ir::Operation::StableSort(_)) {
                            continuation
                        } else {
                            None
                        };
                        pipeline(&out.inner, out.layout, words, false, name, offset)
                            .map(|pipeline| prefix::Preparation { pipeline, packed })
                    }).transpose()?;
                    pipelines.selector = shader.selector.as_deref().map(make).transpose()?;
                    use blanguage::spirv::TerminalScan;
                    let entries = shader.additional_shaders(&out.program, stage_layout);
                    for shader in entries.sort {
                        pipelines.sort.push(pipeline(&out.inner, out.layout, &shader.words, shader.ranks, c"main", None)?);
                    }
                    pipelines.segmented_single_group = entries.segmented_single_group
                        .map(|shader| make(&shader.words)).transpose()?;
                    for (pipeline, shader) in pipelines.terminal_scans.iter_mut().zip(entries.terminal_scans) {
                        *pipeline = match shader {
                            Some(TerminalScan::Canonical) => Some(TerminalPipeline::Canonical),
                            Some(TerminalScan::Alternate(shader)) =>
                                Some(TerminalPipeline::Alternate(make(&shader.words)?)),
                            None => None,
                        };
                    }
                }
            }
        }
        Ok(Self {
            compiled: Rc::new(out),
            partition_elements: NonZeroU32::MAX,
            capture_timestamps: false,
        })
    }
    /// 静态域分批提交；有效前缀域使用 64 线程对齐的 X 行分区，并在 Y/Z 维扩展。
    pub fn with_partition_elements(mut self, count: NonZeroU32) -> Self {
        self.partition_elements = count;
        self
    }
    /// 显式选择后续提交的GPU时间戳采集；默认关闭。
    pub fn with_timestamps(mut self, enabled: bool) -> Result<Self> {
        if enabled && self.compiled.inner.timestamp_bits == 0 {
            return Err(error("当前计算队列不支持GPU时间戳"));
        }
        self.capture_timestamps = enabled;
        Ok(self)
    }
    pub fn submit(
        &self,
        data: impl Into<Bindings<DomainBindings<GpuDataset>>>,
        values: &[Value],
    ) -> Result<Submission> {
        self.submit_steps(data, values, NonZeroU32::MIN)
    }
    /// 多个时间步在一次设备提交中执行，步间数据保持驻留。
    pub fn submit_steps(
        &self,
        bindings: impl Into<Bindings<DomainBindings<GpuDataset>>>,
        values: &[Value],
        steps: NonZeroU32,
    ) -> Result<Submission> {
        let Bindings {
            datasets,
            accelerations,
            textures,
        } = bindings.into();
        self.submit_owned(
            InputOwner {
                dependencies: DependencySources::default(),
                bindings: datasets,
                accelerations,
                textures,
            },
            values,
            steps,
        )
    }
    /// 同队列直接衔接，order 索引前项所有数据以及 additional 中依次追加的数据。
    pub fn submit_after(
        &self,
        previous: Submission,
        order: &[usize],
        additional: impl Into<Bindings<DomainBindings<GpuDataset>>>,
        values: &[Value],
    ) -> Result<Submission> {
        self.submit_join([previous], order, additional, values)
    }
    /// 同队列多生产者汇合，order 索引各前项数据依次连接后再追加的新数据。
    pub fn submit_join(
        &self,
        previous: impl IntoIterator<Item = Submission>,
        order: &[usize],
        additional: impl Into<Bindings<DomainBindings<GpuDataset>>>,
        values: &[Value],
    ) -> Result<Submission> {
        let extra = additional.into();
        let mut owner = InputOwner::after(
            previous,
            extra.accelerations,
            extra.textures,
            &self.compiled.inner,
        )?;
        for data in &extra.datasets.datasets {
            if let Some(used) = data.dependency() {
                if !Rc::ptr_eq(&used.point.inner, &self.compiled.inner) {
                    return Err(error("跨提交衔接必须属于同一设备队列"));
                }
                owner.dependencies.insert(used);
            }
        }
        owner.bindings.append(extra.datasets);
        owner.bindings.remap(order, Vec::new())?;
        self.submit_owned(owner, values, NonZeroU32::MIN)
    }
    fn submit_owned(
        &self,
        owner: InputOwner,
        values: &[Value],
        steps: NonZeroU32,
    ) -> Result<Submission> {
        let InputOwner { dependencies, mut bindings, accelerations, textures } = owner;
        let program = &self.compiled.program;
        let uniforms = arguments(program, bindings.datasets.iter()
            .map(|data| (&data.ty, data.count, Some(0))), values)?;
        let slots = StorageSlots::Direct;
        bindings::require(|slot| bindings.datasets.get(slot), &bindings.domains, program, slots)?;
        let location = |i| slots.location(i);
        let completion = self.submit_mapped(&mut bindings.datasets, dependencies, &accelerations,
            textures, uniforms, steps, slots)?;
        bindings.completed(program, location);
        Ok(Submission { completion, bindings, accelerations })
    }

    fn submit_mapped<D: BorrowMut<GpuDataset>>(
        &self,
        datasets: &mut [D],
        mut dependencies: DependencySources,
        accelerations: &[Acceleration],
        textures: Vec<TextureInput>,
        mut uniforms: Vec<u32>,
        steps: NonZeroU32,
        slots: StorageSlots<'_>,
    ) -> Result<GpuCompletion> {
        let c = &self.compiled;
        let location = |i| slots.location(i);
        let physical = |i| location(i).0;
        if textures.len() != c.program.texture_count()
            || textures.iter().any(|t| !t.belongs_to(&c.inner))
            || textures.iter().zip(c.program.textures().iter().flat_map(|binding|
                std::iter::repeat_n(binding, binding.elements() as usize)))
                .any(|(texture, binding)| texture.dimension() != binding.dimension
                    || binding.format.is_some_and(|format| texture.format() != format))
        {
            return Err(error("纹理数量、维度、格式或设备归属与程序不一致"));
        }
        for texture in &textures {
            dependencies.insert(texture.produced());
        }
        let data = DatasetProjection {
            owners: datasets,
            count: c.program.buffers().len(),
            physical,
        };
        for (dataset, binding) in data.iter().zip(c.program.buffers()) {
            if matches!(&*dataset.storage, DatasetStorage::Read(_))
                && binding.access != blanguage::ast::Access::Read
            {
                return Err(error("冻结的物理存储只能绑定read dataset"));
            }
        }

        if accelerations.len() != c.program.accelerations().len()
            || accelerations.iter().any(|a| !a.belongs_to(&c.inner))
        {
            return Err(error("加速资源的数量或设备归属与程序不一致"));
        }
        for acceleration in accelerations {
            dependencies.insert(acceleration.produced());
        }
        let mut resources: Vec<&ReadDataset> = Vec::new();
        for d in data.owners.iter() {
            let d = d.borrow();
            if !Rc::ptr_eq(&c.inner, &d.allocation.inner) {
                return Err(error("数据集不属于程序的设备"));
            }
            let previous = d.dependency();
            if let Some(used) = previous {
                dependencies.insert(used);
            }
            if !d.views.sources.is_empty() && !previous.is_some_and(|used| used.uploads_dataset_graph()) {
                resources.extend(&d.views.sources);
            }
        }
        let mut resource_stage = 0;
        let mut resource_access = 0;
        let mut visited = BTreeSet::new();
        while let Some(source) = resources.pop() {
            if !visited.insert(Rc::as_ptr(&source.0) as usize) {
                continue;
            }
            let previous = source.0.dependency();
            if let Some(used) = previous {
                dependencies.insert(used);
            } else if dependencies.is_empty() {
                resource_stage |= source.0.producer.stage();
                resource_access |= source.0.producer.access();
            }
            if !source.0.views.sources.is_empty() && !previous.is_some_and(|used| used.uploads_dataset_graph()) {
                resources.extend(&source.0.views.sources);
            }
        }
        let pending_inputs = dependencies.is_empty();
        let dependencies = if pending_inputs {
            Dependencies::Empty
        } else {
            resource_stage = 0;
            resource_access = 0;
            dependencies.prepare()?
        };
        let f = &c.inner.functions;
        let device = c.inner.owner.device;
        let mut scratch = Vec::new();
        let mut binding_parameters = None;
        // 同一串行提交中，完成消费的同容量 where 工作区由后续阶段复用。
        let mut scratch_uses = Vec::new();
        let mut selected_storage = BTreeMap::<(u32, bool, usize), (usize, BTreeSet<usize>)>::new();
        // Each Count word and workgroup width owns one ordinary Map record.
        // The bool describes whether the current step already prepares it.
        let mut map_arguments = BTreeMap::<(usize, u64, u32), (usize, bool)>::new();
        let stage_count = c.program.stages().len();
        let query_count = if self.capture_timestamps {
            u32::try_from(stage_count as u64 * 2 * steps.get() as u64)?
        } else { 0 };
        // Reductions and compact still publish an identity or a count for empty input.
        let stage_has_work = |stage: &blanguage::ir::ExecutionStage, count| {
            use blanguage::ir::Operation;
            count != 0 || !matches!(
                stage.operation,
                Operation::Map | Operation::StableSort(_)
                    | Operation::ScanExclusive(_) | Operation::CustomScan(_)
            )
        };
        let mut needs_root_set = false;
        let mut extra_sets = 0u64;
        for (i, stage) in c.program.stages().iter().enumerate() {
            let count = data[stage.domain].count;
            if !stage_has_work(stage, count) {
                continue;
            }
            let map_prefix = stage.prefix.filter(|_| {
                stage.selection.is_none() && matches!(stage.operation, blanguage::ir::Operation::Map)
            });
            if let Some(prefix) = map_prefix {
                use std::collections::btree_map::Entry;
                let (binding, offset) = location(prefix.binding);
                let key = (binding, u64::from(offset) + u64::from(prefix.word), stage.workgroup_width());
                let (slot, reused, prepare_map) = match map_arguments.entry(key) {
                    Entry::Occupied(mut entry) => {
                        let (slot, prepared) = entry.get_mut();
                        (*slot, true, !std::mem::replace(prepared, true))
                    }
                    Entry::Vacant(entry) => {
                        let slot = scratch.len();
                        scratch.push(Scratch {
                            buffers: Vec::new(),
                            arguments: Some(prefix::allocate(c, i, count)?),
                            sorting: None,
                        });
                        entry.insert((slot, true));
                        extra_sets += 1;
                        (slot, false, true)
                    }
                };
                scratch_uses.push(ScratchUse { stage: i, slot, reused, prepare_map });
            }
            // The body writes after its preparation. An overlapping write ends
            // the current Count's use for all widths, including projected fields.
            if !map_arguments.is_empty() {
                for (&binding, regions) in &stage.write_regions {
                    let (binding, offset) = location(binding);
                    for &(start, end) in regions {
                        let start = (u64::from(offset) + u64::from(start)).saturating_sub(3);
                        let end = u64::from(offset) + u64::from(end);
                        for (_, (_, prepared)) in map_arguments.range_mut((binding, start, 0)..(binding, end, 0)) {
                            *prepared = false;
                        }
                    }
                }
            }
            if map_prefix.is_some() {
                continue;
            }
            if let Some(selection) = stage.selection {
                use std::collections::btree_map::Entry;
                let legacy_summary = c.stages[i].scan_terminal(count).is_none();
                let continuation = prefix::continuation_slot(c, i);
                let ((slot, sources), reused) = match selected_storage.entry((count, legacy_summary, continuation)) {
                    Entry::Occupied(entry) => (entry.into_mut(), true),
                    Entry::Vacant(entry) => {
                        let slot = scratch.len();
                        let mut temporary = selection::allocate(c, i, count)?;
                        temporary.arguments = Some(prefix::allocate(c, i, count)?);
                        scratch.push(temporary);
                        extra_sets += 2;
                        (entry.insert((slot, BTreeSet::new())), false)
                    }
                };
                let levels = scan::levels(count);
                if levels > 1 && sources.insert(selection.binding) {
                    extra_sets += ((levels - 1) * 2) as u64;
                }
                scratch_uses.push(ScratchUse { stage: i, slot: *slot, reused, prepare_map: false });
                continue;
            }
            let temporary = match stage.operation {
                blanguage::ir::Operation::Reduce(blanguage::ir::ReductionSpec {
                    ty: ref reduction_ty,
                    ..
                })
                | blanguage::ir::Operation::CustomReduce(blanguage::ir::ReductionLayout {
                    ty: ref reduction_ty,
                    ..
                }) => {
                    let size = scan::groups(count);
                    if size == 1 && stage.prefix.is_none() {
                        needs_root_set = true;
                    } else {
                        extra_sets += scan::levels(count) as u64;
                    }
                    if size > 1 {
                        let words = reduction_ty.size.div_ceil(4);
                        let mut buffers = vec![PagedBuffer::words(c.inner.clone(), size, words)?];
                        let second = scan::groups(size);
                        if second > 1 {
                            buffers.push(PagedBuffer::words(c.inner.clone(), second, words)?);
                        }
                        Some(Scratch {
                            buffers,
                            arguments: None,
                            sorting: None,
                        })
                    } else {
                        None
                    }
                }
                blanguage::ir::Operation::Compact(_) => {
                    extra_sets += scan::levels(count) as u64 * 2;
                    Some(compact::allocate(c, i, count)?)
                }
                blanguage::ir::Operation::ScanExclusive(ref s) => {
                    extra_sets += (scan::levels(count) * 2 - 1) as u64;
                    Some(scan::allocate(c, i, count, s.summary_words())?)
                }
                blanguage::ir::Operation::CustomScan(ref s) => {
                    extra_sets += (scan::levels(count) * 2 - 1) as u64;
                    Some(scan::allocate(c, i, count, s.summary_words())?)
                }
                blanguage::ir::Operation::SegmentedReduce(_)
                | blanguage::ir::Operation::CustomSegmentedReduce(_)
                    if c.stages[i].segmented_single_group(count).is_some() =>
                {
                    if stage.prefix.is_some() {
                        extra_sets += 1;
                    } else {
                        needs_root_set = true;
                    }
                    None
                }
                blanguage::ir::Operation::CustomSegmentedReduce(ref r) => {
                    extra_sets += (scan::levels(count) * 2 - 1) as u64;
                    Some(scan::allocate(c, i, count, r.summary_words())?)
                }
                blanguage::ir::Operation::SegmentedReduce(ref r) => {
                    extra_sets += (scan::levels(count) * 2 - 1) as u64;
                    Some(scan::allocate(c, i, count, r.summary_words())?)
                }
                blanguage::ir::Operation::StableSort(_) => {
                    let sorting = sort::allocate(c, i, count)?;
                    extra_sets += sorting.descriptor_count();
                    Some(Scratch {
                        buffers: Vec::new(),
                        arguments: None,
                        sorting: Some(sorting),
                    })
                }
                blanguage::ir::Operation::Map => {
                    needs_root_set = true;
                    None
                }
            };
            let temporary = if stage.prefix.is_some() {
                let mut temporary = temporary.unwrap_or(Scratch {
                    buffers: Vec::new(),
                    arguments: None,
                    sorting: None,
                });
                temporary.arguments = Some(prefix::allocate(c, i, count)?);
                Some(temporary)
            } else {
                temporary
            };
            if let Some(temporary) = temporary {
                scratch_uses.push(ScratchUse {
                    stage: i,
                    slot: scratch.len(),
                    reused: false,
                    prepare_map: false,
                });
                scratch.push(temporary);
            }
        }
        let sets = u32::try_from(u64::from(needs_root_set) + extra_sets)?;
        if sets == 0 && query_count == 0 {
            let execution = Execution::submit_fence(c.inner.clone(), dependencies)?;
            return Ok(GpuCompletion { execution, queries: None });
        }
        let mut commands = Commands::new(c.inner.clone())?;
        let mut accesses: Vec<_> = data.owners.iter().map(|dataset| BindingAccess {
            producer: dataset.borrow().producer,
            pending_read: false,
            pending_write: pending_inputs,
        }).collect();
        let used_bindings = unsafe {
            if sets != 0 && !pending_inputs {
                // 同队列完成交接独立于内存可见性。实际计算消费需要 RAW、WAW 与 WAR 屏障。
                let access = 0x60
                    | if accelerations.is_empty() {
                        0
                    } else {
                        0x200000
                    };
                commands.barrier(0x10000, 0x800, 0x10000, access);
            }
            let infos = if sets != 0 {
                let mut sizes = vec![VkDescriptorPoolSize {
                    r#type: 7,
                    descriptorCount: u32::try_from(
                        (data.len() + c.program.auxiliary_buffers()) as u64 * sets as u64,
                    )?,
                }];
                if !accelerations.is_empty() {
                    sizes.push(VkDescriptorPoolSize {
                        r#type: 1000150000,
                        descriptorCount: u32::try_from(accelerations.len() as u64 * sets as u64)?,
                    });
                }
                if !textures.is_empty() {
                    sizes.push(VkDescriptorPoolSize {
                        r#type: 1,
                        descriptorCount: u32::try_from(
                            textures.len() as u64 * sets as u64,
                        )?,
                    });
                }
                let mut pool: VkDescriptorPoolCreateInfo = zero();
                pool.sType = 33;
                pool.maxSets = sets;
                pool.poolSizeCount = sizes.len() as u32;
                pool.pPoolSizes = sizes.as_ptr();
                check(
                    (f.vkCreateDescriptorPool)(device, &pool, null(), &mut commands.descriptors),
                    "vkCreateDescriptorPool",
                )?;
                let bytes = u64::from(blanguage::spirv::BINDING_BYTES);
                let mut infos: Vec<_> = data.iter().map(|d| VkDescriptorBufferInfo {
                    buffer: d.allocation.buffer, offset: 0, range: bytes,
                }).collect();
                if let StorageSlots::Frame(arguments) = slots {
                    let fields = arguments.projected_fields();
                    let field_count = fields.clone().count();
                    if field_count != 0 {
                        let alignment = c.inner.properties.limits.minStorageBufferOffsetAlignment;
                        let step = bytes.div_ceil(alignment) * alignment;
                        let parameters = Allocation::new(c.inner.clone(), step * field_count as u64, 0x20, 6, 8)?;
                        let mapped = parameters.mapping()?;
                        for (index, group) in fields.enumerate() {
                            let first = group[0];
                            let offset = location(first).1;
                            let d = &data[first];
                            let at = step * index as u64;
                            mapped.add(at as usize).cast::<[u32; 4]>()
                                .write(binding_header(d.page_array_address(), d.stride, offset));
                            let info = VkDescriptorBufferInfo { buffer: parameters.buffer, offset: at, range: bytes };
                            for &i in group {
                                infos[i] = info;
                            }
                        }
                        binding_parameters = Some(parameters);
                    }
                }
                infos
            } else {
                Vec::new()
            };
            let mut descriptors = (sets != 0)
                .then(|| DescriptorWriter::new(&commands, c, &infos, accelerations, &textures));
            let root_set = needs_root_set
                .then(|| descriptors.as_mut().unwrap().set(&[], None, None, None, None))
                .transpose()?;
            let selection_sets = selected_storage.into_iter().map(|(key, (slot, sources))| {
                selection::descriptors(descriptors.as_mut().unwrap(), &scratch[slot], key.0, sources)
                    .map(|sets| (slot, sets))
            }).collect::<Result<BTreeMap<_, _>>>()?;
            let map_sets = map_arguments.into_values().map(|(slot, _)| {
                let args = scratch[slot].arguments.as_ref().unwrap();
                descriptors.as_mut().unwrap().set(&[], None, Some(args.descriptor()), None, None)
                    .map(|set| (slot, set))
            }).collect::<Result<BTreeMap<_, _>>>()?;
            let mut sorting_sets = Vec::new();
            for usage in &scratch_uses {
                if let blanguage::ir::Operation::StableSort(sorting) =
                    &c.program.stages()[usage.stage].operation
                {
                    let storage = &scratch[usage.slot];
                    sorting_sets.push(sort::descriptors(
                        descriptors.as_mut().unwrap(),
                        sorting,
                        storage.sorting.as_ref().unwrap(),
                        storage.arguments.as_ref(),
                    )?);
                }
            }
            if self.capture_timestamps {
                let queries = QueryPoolCreateInfo {
                    s_type: 11,
                    next: null(),
                    flags: 0,
                    query_type: 2,
                    count: query_count,
                    statistics: 0,
                };
                check(
                    (f.vkCreateQueryPool)(device, &queries, null(), &mut commands.queries),
                    "vkCreateQueryPool",
                )?;
                (f.vkCmdResetQueryPool)(commands.command, commands.queries, 0, queries.count);
            }
            let commands = ComputeCommands::new(&commands, c.layout);
            for step in 0..steps.get() {
                // Stage bindings and the order of set requests depend on fixed storage.
                // The device updates buffer contents and indirect counts between steps.
                if let Some(descriptors) = &mut descriptors {
                    descriptors.step = match step {
                        0 if steps.get() == 1 => DescriptorStep::Single,
                        0 => DescriptorStep::Capture,
                        _ => DescriptorStep::Reuse(0),
                    };
                }
                let mut uses = scratch_uses.iter().peekable();
                let mut sorting_sets = sorting_sets.iter();
                for (i, stage) in c.program.stages().iter().enumerate() {
                    let count = data[stage.domain].count;
                    let query = self.capture_timestamps
                        .then(|| (step as usize * stage_count + i) as u32 * 2);
                    if !stage_has_work(stage, count) {
                        if let Some(query) = query {
                            (f.vkCmdWriteTimestamp)(commands.command, 0x800, commands.queries, query);
                            (f.vkCmdWriteTimestamp)(commands.command, 0x800, commands.queries, query + 1);
                        }
                        continue;
                    }
                    let write_hazard = stage
                        .reads
                        .iter()
                        .chain(&stage.writes)
                        .any(|&b| accesses[physical(b)].pending_write);
                    let war = stage
                        .writes
                        .iter()
                        .any(|&b| accesses[physical(b)].pending_read);
                    if write_hazard || war || resource_stage != 0 {
                        let mut src = resource_stage | if war { 0x800 } else { 0 };
                        let mut access = resource_access | if war { 0x20 } else { 0 };
                        for binding in &mut accesses {
                            if binding.pending_write {
                                src |= binding.producer.stage();
                                access |= binding.producer.access();
                            }
                            binding.pending_read = false;
                            binding.pending_write = false;
                        }
                        commands.barrier(src, 0x800, access, 0x60);
                        resource_stage = 0;
                        resource_access = 0;
                    }
                    uniforms[0] = count;
                    if let Some(query) = query {
                        (f.vkCmdWriteTimestamp)(commands.command, 0x800, commands.queries, query);
                    }
                    let usage = uses.next_if(|usage| usage.stage == i);
                    let temporary = usage.map(|usage| &scratch[usage.slot]);
                    let arguments = temporary.and_then(|s| s.arguments.as_ref());
                    let pipelines = &c.stages[i];
                    let segmented_single_group = pipelines.segmented_single_group(count);
                    match &stage.operation {
                        blanguage::ir::Operation::StableSort(s) => {
                            let temporary = temporary.unwrap();
                            sort::record(
                                &commands,
                                c,
                                i,
                                s.keys.len(),
                                data[s.source].count,
                                temporary.sorting.as_ref().unwrap(),
                                sorting_sets.next().unwrap(),
                                arguments,
                                &mut uniforms,
                                self.partition_elements,
                                step > 0,
                            );
                        }
                        blanguage::ir::Operation::Map if stage.selection.is_some() => {
                            selection::record(
                                &commands,
                                &selection_sets[&usage.unwrap().slot],
                                c,
                                i,
                                count,
                                arguments.unwrap(),
                                &mut uniforms,
                                self.partition_elements,
                                step > 0 || usage.unwrap().reused,
                            );
                        }
                        blanguage::ir::Operation::Map if stage.prefix.is_some() => {
                            let args = arguments.unwrap();
                            let usage = usage.unwrap();
                            commands.bind_set(map_sets[&usage.slot]);
                            if usage.prepare_map {
                                prefix::prepare(
                                    &commands, c, i, &mut uniforms,
                                    self.partition_elements, step > 0 || usage.reused,
                                );
                            }
                            prefix::dispatch(
                                &commands,
                                c,
                                pipelines.main,
                                args,
                                &mut uniforms,
                                0,
                            );
                        }
                        blanguage::ir::Operation::Map => {
                            commands.bind_pipeline(pipelines.main);
                            commands.bind_set(root_set.unwrap());
                            let width = stage.workgroup_width();
                            let maximum = c.inner.properties.limits.maxComputeWorkGroupCount[0]
                                as u64
                                * width as u64;
                            let requested = if stage.has_tile_scope() {
                                self.partition_elements.get().div_ceil(width) as u64
                                    * width as u64
                            } else {
                                self.partition_elements.get() as u64
                            };
                            let partition = maximum.min(requested);
                            let mut begin = 0u64;
                            while begin < count as u64 {
                                let size = (count as u64 - begin).min(partition);
                                uniforms[0] = (begin + size) as u32;
                                uniforms[1] = begin as u32;
                                (f.vkCmdPushConstants)(
                                    commands.command,
                                    c.layout,
                                    0x20,
                                    0,
                                    c.program.uniform_bytes(),
                                    uniforms.as_ptr().cast(),
                                );
                                (f.vkCmdDispatch)(
                                    commands.command,
                                    (size as u32).div_ceil(width),
                                    1,
                                    1,
                                );
                                begin += size;
                            }
                        }
                        blanguage::ir::Operation::SegmentedReduce(_)
                        | blanguage::ir::Operation::CustomSegmentedReduce(_)
                            if segmented_single_group.is_some() =>
                        {
                            let set = if let Some(args) = arguments {
                                descriptors.as_mut().unwrap().set(
                                    &[], None, Some(args.descriptor()), None, None,
                                )?
                            } else {
                                root_set.unwrap()
                            };
                            commands.bind_set(set);
                            if arguments.is_some() {
                                prefix::prepare(
                                    &commands, c, i, &mut uniforms,
                                    self.partition_elements, step > 0,
                                );
                            }
                            scan::dispatch(
                                &commands, c, segmented_single_group.unwrap(), count, &mut uniforms,
                                self.partition_elements, arguments, 0,
                            );
                        }
                        blanguage::ir::Operation::SegmentedReduce(r) => {
                            scan::record_segmented_reduce(
                                &commands,
                                descriptors.as_mut().unwrap(),
                                c,
                                i,
                                &r.values.fields,
                                data[r.values.fields.source].count,
                                &temporary.unwrap().buffers,
                                arguments,
                                &mut uniforms,
                                self.partition_elements,
                                step > 0,
                            )?;
                        }
                        blanguage::ir::Operation::CustomSegmentedReduce(r) => {
                            scan::record_segmented_reduce(
                                &commands,
                                descriptors.as_mut().unwrap(),
                                c,
                                i,
                                &r.values.fields,
                                data[r.values.fields.source].count,
                                &temporary.unwrap().buffers,
                                arguments,
                                &mut uniforms,
                                self.partition_elements,
                                step > 0,
                            )?;
                        }
                        blanguage::ir::Operation::ScanExclusive(blanguage::ir::Scan {
                            values: fields, head_word, ..
                        })
                        | blanguage::ir::Operation::CustomScan(blanguage::ir::CustomScan {
                            values: blanguage::ir::ReductionLayout { fields, .. },
                            head_word,
                        }) => {
                            let descriptors = descriptors.as_mut().unwrap();
                            let add = pipelines.scan_add.unwrap();
                            let source = descriptors.binding(fields.source);
                            let target = descriptors.binding(fields.target);
                            scan::record(
                                &commands,
                                descriptors,
                                c,
                                i,
                                [pipelines.main, pipelines.tail.unwrap(), add, pipelines.finish.unwrap_or(add)],
                                fields,
                                head_word.is_some(),
                                (data[fields.source].count, source),
                                target,
                                &temporary.unwrap().buffers,
                                arguments,
                                &mut uniforms,
                                self.partition_elements,
                                step > 0,
                            )?;
                        }
                        blanguage::ir::Operation::Compact(selection) => {
                            compact::record(
                                &commands,
                                descriptors.as_mut().unwrap(),
                                c,
                                i,
                                selection,
                                &data,
                                temporary.unwrap(),
                                &mut uniforms,
                                self.partition_elements,
                                step > 0,
                            )?;
                        }
                        blanguage::ir::Operation::Reduce(blanguage::ir::ReductionSpec {
                            fields: reduction,
                            ..
                        })
                        | blanguage::ir::Operation::CustomReduce(
                            blanguage::ir::ReductionLayout {
                                fields: reduction, ..
                            },
                        ) => {
                            let mut length = count;
                            let mut pass = 0;
                            loop {
                                let groups = scan::groups(length);
                                if pass > 0 {
                                    commands.barrier(0x800, 0x800, 0x40, 0x60);
                                }
                                let pass_set = if let Some(scratch) = temporary {
                                    let bindings = [
                                        (pass > 0).then(|| (
                                            reduction.source,
                                            scratch.buffers[(pass - 1) % 2].descriptor(),
                                        )),
                                        (groups > 1).then(|| (
                                            reduction.target,
                                            scratch.buffers[pass % 2].descriptor(),
                                        )),
                                    ];
                                    descriptors.as_mut().unwrap().set(
                                        &bindings,
                                        None,
                                        arguments.map(PagedBuffer::descriptor),
                                        None,
                                        None,
                                    )?
                                } else {
                                    root_set.unwrap()
                                };
                                commands.bind_set(pass_set);
                                if pass == 0 && arguments.is_some() {
                                    prefix::prepare(
                                        &commands, c, i, &mut uniforms,
                                        self.partition_elements, step > 0,
                                    );
                                }
                                uniforms[2 + reduction.target] = groups;
                                scan::dispatch(
                                    &commands,
                                    c,
                                    if pass == 0 {
                                        pipelines.main
                                    } else {
                                        pipelines.tail.unwrap()
                                    },
                                    length,
                                    &mut uniforms,
                                    self.partition_elements,
                                    arguments,
                                    pass,
                                );
                                if groups == 1 {
                                    break;
                                }
                                length = groups;
                                pass += 1;
                            }
                            uniforms[2 + reduction.target] = data[reduction.target].count;
                        }
                    }
                    if let Some(query) = query {
                        (f.vkCmdWriteTimestamp)(
                            commands.command,
                            0x800,
                            commands.queries,
                            query + 1,
                        );
                    }
                    for &b in &stage.reads {
                        accesses[physical(b)].pending_read = true;
                    }
                    for &b in &stage.writes {
                        let binding = &mut accesses[physical(b)];
                        binding.pending_write = true;
                        binding.producer = Usage::Compute;
                    }
                }
            }
            commands.set.get().is_some()
        };
        drop(scratch_uses);
        let queries = (query_count != 0)
            .then(|| QueryPool::take(&mut commands, query_count, c.program.clone()));
        let resources = used_bindings.then(|| ComputeResources {
            _compiled: c.clone(),
            _accelerations: accelerations.to_vec(),
            _textures: textures,
            _scratch: scratch,
            _binding_parameters: binding_parameters,
        });
        let execution = Execution::submit(
            ExecutionResources::Compute {
                commands,
                _resources: resources,
                _queries: queries.clone(),
            },
            dependencies,
        )?;
        if used_bindings {
            for (dataset, access) in data.owners.iter_mut().zip(accesses) {
                let dataset = dataset.borrow_mut();
                if let DatasetStorage::Owned(storage) = &mut *dataset.storage {
                    storage.producer = access.producer;
                }
                dataset.last_use = Some(execution.clone());
            }
        }
        Ok(GpuCompletion { execution, queries })
    }
}
impl GpuTemplate {
    pub fn program(&self) -> &Program {
        self.code.program()
    }
    pub fn with_partition_elements(mut self, count: NonZeroU32) -> Self {
        self.code.partition_elements = count;
        self
    }
    pub fn with_timestamps(mut self, enabled: bool) -> Result<Self> {
        self.code = self.code.with_timestamps(enabled)?;
        Ok(self)
    }
    pub fn submit(
        &self,
        bindings: impl Into<Bindings<DomainBindings<GpuDataset>>>,
        values: &[Value],
    ) -> Result<Submission> {
        self.code.submit(bindings, values)
    }
    pub(crate) fn submit_projected(
        &self,
        datasets: &mut [&mut GpuDataset],
        arguments: &FrameArguments<'_>,
        uniforms: Vec<u32>,
    ) -> Result<GpuCompletion> {
        let slots = StorageSlots::Frame(arguments);
        self.code.submit_mapped(datasets, DependencySources::default(), &[], Vec::new(),
            uniforms, NonZeroU32::MIN, slots)
    }
    pub fn submit_after(
        &self,
        previous: Submission,
        order: &[usize],
        additional: impl Into<Bindings<DomainBindings<GpuDataset>>>,
        values: &[Value],
    ) -> Result<Submission> {
        self.code.submit_after(previous, order, additional, values)
    }
    pub fn submit_join(
        &self,
        previous: impl IntoIterator<Item = Submission>,
        order: &[usize],
        additional: impl Into<Bindings<DomainBindings<GpuDataset>>>,
        values: &[Value],
    ) -> Result<Submission> {
        self.code.submit_join(previous, order, additional, values)
    }
}
// 命令按参数顺序借用同一次实际物理绑定。
struct DatasetProjection<'a, D, F> {
    owners: &'a mut [D],
    count: usize,
    physical: F,
}

impl<D: BorrowMut<GpuDataset>, F: Fn(usize) -> usize> DatasetProjection<'_, D, F> {
    fn len(&self) -> usize {
        self.count
    }

    fn iter(&self) -> impl ExactSizeIterator<Item = &GpuDataset> {
        (0..self.count).map(|i| &self[i])
    }
}

impl<D: BorrowMut<GpuDataset>, F: Fn(usize) -> usize> std::ops::Index<usize> for DatasetProjection<'_, D, F> {
    type Output = GpuDataset;

    fn index(&self, index: usize) -> &GpuDataset {
        self.owners[(self.physical)(index)].borrow()
    }
}
// 数据本身持有最后使用点，输入容器无需维护另一套原生资源所属表。
struct InputOwner {
    dependencies: DependencySources,
    bindings: DomainBindings<GpuDataset>,
    accelerations: Vec<Acceleration>,
    textures: Vec<TextureInput>,
}
impl InputOwner {
    fn after(
        previous: impl IntoIterator<Item = Submission>,
        accelerations: Vec<Acceleration>,
        textures: Vec<TextureInput>,
        inner: &Rc<Inner>,
    ) -> Result<Self> {
        let mut owner = Self {
            dependencies: DependencySources::default(),
            bindings: Vec::new().into(),
            accelerations,
            textures,
        };
        for previous in previous {
            if !Rc::ptr_eq(inner, &previous.completion.execution.point.inner) {
                return Err(error("跨提交衔接必须属于同一设备队列"));
            }
            let Submission {
                completion,
                bindings,
                ..
            } = previous;
            owner.dependencies.insert_owned(completion.execution);
            owner.bindings.append(bindings);
        }
        Ok(owner)
    }
}
pub struct Submission {
    completion: GpuCompletion,
    bindings: DomainBindings<GpuDataset>,
    accelerations: Vec<Acceleration>,
}
pub struct Completed {
    bindings: DomainBindings<GpuDataset>,
    pub accelerations: Vec<Acceleration>,
    pub timings: Option<GpuTimings>,
}
impl Completed {
    pub fn datasets(&self) -> &[GpuDataset] {
        self.bindings.datasets()
    }
    pub fn into_bindings(self) -> DomainBindings<GpuDataset> {
        self.bindings
    }
    pub fn into_datasets(self) -> Vec<GpuDataset> {
        self.bindings.into_datasets()
    }
    pub fn into_parts(self) -> (DomainBindings<GpuDataset>, Vec<Acceleration>, Option<GpuTimings>) {
        (self.bindings, self.accelerations, self.timings)
    }
}
pub struct GpuTimings {
    pub elapsed_ms: f64,
    pub stage_ms: Vec<f64>,
}
impl Submission {
    pub fn is_complete(&self) -> Result<bool> {
        self.completion.is_complete()
    }
    /// 交还尚未等待的数据；物理存储继续持有真实最后使用点。
    pub fn into_bindings(self) -> DomainBindings<GpuDataset> {
        self.bindings
    }
    /// 不等待CPU，交出带真实最后使用点的数据及独立的完成／时间戳观察。
    pub fn detach(self) -> (DomainBindings<GpuDataset>, Vec<Acceleration>, GpuCompletion) {
        (
            self.bindings,
            self.accelerations,
            self.completion,
        )
    }
    pub fn partition(self, groups: &[&[usize]]) -> Result<Vec<DomainBindings<GpuDataset>>> {
        self.into_bindings().partition(groups)
    }
    /// 等待真实完成并读取时间戳；失败交回原提交及其全部输入和最后使用点。
    pub fn wait(mut self) -> std::result::Result<Completed, (Box<dyn std::error::Error + Send + Sync>, Self)> {
        let timings = match self.completion.wait_bound(self.bindings.datasets.iter_mut()) {
            Ok(timings) => timings,
            Err(error) => return Err((error, self)),
        };
        Ok(Completed {
            bindings: self.bindings,
            accelerations: self.accelerations,
            timings,
        })
    }
}
