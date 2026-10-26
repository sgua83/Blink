//! 单设备 Vulkan 执行后端。唯一数据持有最后使用点，执行资源在原生完成后释放。
mod compact;
mod execution;
mod prefix;
mod present;
mod raw;
mod ray;
mod scan;
mod selection;
mod sort;
mod texture;
mod transfer;
use crate::views::Views;
use crate::{Dataset, DomainBindings, Result, Value, arguments, error};
use blanguage::{
    ExecutableProgram, TemplateProgram,
    ast::Target,
    ir::{Program, Ty},
};
use execution::{Completion, Execution, ExecutionResources, QueryPool};
pub use present::{PresentStatus, Swapchain};
use raw::*;
pub use ray::{Acceleration, BottomLevel, RayInstance};
use std::num::NonZeroU32;
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, c_char, c_void},
    mem::ManuallyDrop,
    ptr::{null, null_mut},
    rc::Rc,
};
pub use texture::Texture;
use texture::{BufferImageCopy, ImageBarrier, Subresource, SubresourceRange};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *const c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}
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
    handle: *mut c_void,
    gipa: Gipa,
}
impl Library {
    fn new() -> Result<Self> {
        let name: Vec<_> = "vulkan-1.dll\0".encode_utf16().collect();
        let handle = unsafe { LoadLibraryW(name.as_ptr()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let address = unsafe { GetProcAddress(handle, c"vkGetInstanceProcAddr".as_ptr()) };
        let gipa = unsafe { function(address, c"vkGetInstanceProcAddr") }?;
        Ok(Self { handle, gipa })
    }
    unsafe fn get<F: Copy>(&self, instance: VkInstance, name: &CStr) -> Result<F> {
        unsafe { function((self.gipa)(instance, name.as_ptr()), name) }
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.handle);
        }
    }
}
struct InstanceOwner {
    instance: VkInstance,
    destroy: DestroyInstance,
    library: Library,
}
impl Drop for InstanceOwner {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.instance, null());
        }
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
    vkCreateBuffer:unsafe extern "system" fn(VkDevice,*const VkBufferCreateInfo,*const c_void,*mut VkBuffer)->i32,
    vkDestroyBuffer:unsafe extern "system" fn(VkDevice,VkBuffer,*const c_void),
    vkGetBufferMemoryRequirements:unsafe extern "system" fn(VkDevice,VkBuffer,*mut VkMemoryRequirements),
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
    owner: DeviceOwner,
    functions: Functions,
    physical: VkPhysicalDevice,
    queue: VkQueue,
    family: u32,
    memory: VkPhysicalDeviceMemoryProperties,
    properties: VkPhysicalDeviceProperties,
    timestamp_bits: u32,
    ray: Option<ray::RayDevice>,
    sorting: bool,
    maximal_reconvergence: bool,
    texture_indexing: bool,
    position_fetch: bool,
    subgroup_operations: u32,
    float32_preserve: bool,
    next_submission: Cell<u64>,
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
        Self::create(false, false, false, false, false, false)
    }
    pub fn for_program(program: &Program) -> Result<Self> {
        Self::for_programs(std::iter::once(program))
    }
    /// 同一逻辑设备消费多个B程序，按实际程序集合声明能力，不在执行中重建设备。
    pub fn for_programs<'a>(programs: impl IntoIterator<Item = &'a Program>) -> Result<Self> {
        Self::program_device(programs, false)
    }
    pub fn with_presentation(program: &Program) -> Result<Self> {
        Self::with_presentation_programs(std::iter::once(program))
    }
    pub fn with_presentation_programs<'a>(
        programs: impl IntoIterator<Item = &'a Program>,
    ) -> Result<Self> {
        Self::program_device(programs, true)
    }
    fn program_device<'a>(
        programs: impl IntoIterator<Item = &'a Program>,
        presentation: bool,
    ) -> Result<Self> {
        let (mut ray, mut sort, mut reconverge, mut textures, mut positions) =
            (false, false, false, false, false);
        for program in programs {
            ray |= !program.accelerations().is_empty();
            sort |= program.requires_sort();
            reconverge |= program.requires_maximal_reconvergence();
            textures |= program.requires_texture_indexing();
            positions |= program.requires_ray_position_fetch();
        }
        Self::create(ray, sort, presentation, reconverge, textures, positions)
    }
    fn create(
        ray_query: bool,
        sorting: bool,
        presentation: bool,
        maximal_reconvergence: bool,
        texture_indexing: bool,
        position_fetch: bool,
    ) -> Result<Self> {
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
            let owner = Rc::new(InstanceOwner {
                instance,
                destroy,
                library,
            });
            let enumerate: unsafe extern "system" fn(
                VkInstance,
                *mut u32,
                *mut VkPhysicalDevice,
            ) -> i32 = owner.library.get(instance, c"vkEnumeratePhysicalDevices")?;
            let properties_fn: unsafe extern "system" fn(
                VkPhysicalDevice,
                *mut VkPhysicalDeviceProperties,
            ) = owner
                .library
                .get(instance, c"vkGetPhysicalDeviceProperties")?;
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
            let mut chosen = None;
            for physical in devices {
                let mut p = zero();
                properties_fn(physical, &mut p);
                if p.deviceType == 2 {
                    chosen = Some((physical, p));
                    break;
                }
            }
            let (physical, properties) = chosen.ok_or_else(|| error("目标平台需要离散 GPU"))?;
            if properties.apiVersion < ((1 << 22) | (2 << 12)) {
                return Err(error("目标 GPU 需要 Vulkan 1.2"));
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
            let mut subgroup_features = sort::Features {
                kind: 1000225002,
                next: features.pNext,
                size_control: 0,
                full: 0,
            };
            if sorting {
                features.pNext = (&mut subgroup_features as *mut sort::Features).cast();
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
            // VkPhysicalDeviceDescriptorIndexingFeatures::shaderSampledImageArrayNonUniformIndexing。
            if texture_indexing && indexing.capabilities[4] == 0 {
                return Err(error("纹理资源数组需要 sampled-image 非一致下标能力"));
            }
            if maximal_reconvergence && reconvergence.shaderMaximalReconvergence == 0 {
                return Err(error(
                    "append 需要 shaderMaximalReconvergence，以保证分歧路径的活动集合合并预留",
                ));
            }
            let get: unsafe extern "system" fn(VkPhysicalDevice, *mut VkPhysicalDeviceProperties2) =
                owner
                    .library
                    .get(instance, c"vkGetPhysicalDeviceProperties2")?;
            let mut control: sort::ControlProperties = zero();
            control.kind = 1000225000;
            let mut float_controls: VkPhysicalDeviceFloatControlsProperties = zero();
            float_controls.sType = 1000197000;
            if sorting {
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
                    return Err(error(
                        "稳定键排序需要 compute ballot、32 lane subgroup 与 full subgroup 能力",
                    ));
                }
            }
            if address.bufferDeviceAddress == 0 || features.features[40] == 0 {
                return Err(error("目标 GPU 需要 bufferDeviceAddress 和 shaderInt64"));
            }
            if ray_query
                && (query_features.ray_query == 0 || acceleration_features.acceleration == 0)
            {
                return Err(error(
                    "程序需要 accelerationStructure 与 rayQuery，目标设备不支持",
                ));
            }
            if position_fetch && position_features.position_fetch == 0 {
                return Err(error("程序读取命中顶点，需要 rayTracingPositionFetch"));
            }
            features.features.fill(0);
            features.features[40] = 1; // VkPhysicalDeviceFeatures::shaderInt64。
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
            let (family, queue_properties) = families
                .iter()
                .enumerate()
                .find(|(_, q)| {
                    q.queueFlags & 2 != 0 && q.queueCount > 0 && q.timestampValidBits > 0
                })
                .ok_or_else(|| error("缺少支持时间戳的计算队列"))?;
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
            let device_owner = DeviceOwner {
                device,
                destroy: destroy_device,
                _instance: owner,
            };
            let functions = Functions::load(device, get_device)?;
            let ray = if ray_query {
                let mut ray_properties: ray::Properties = zero();
                ray_properties.kind = 1000150014;
                let mut properties2: VkPhysicalDeviceProperties2 = zero();
                properties2.kind = 1000059001;
                properties2.next = (&mut ray_properties as *mut ray::Properties).cast();
                let get: unsafe extern "system" fn(
                    VkPhysicalDevice,
                    *mut VkPhysicalDeviceProperties2,
                ) = device_owner
                    ._instance
                    .library
                    .get(instance, c"vkGetPhysicalDeviceProperties2")?;
                get(physical, &mut properties2);
                Some(ray::RayDevice::load(device, get_device, ray_properties)?)
            } else {
                None
            };
            let mut queue = null_mut();
            (functions.vkGetDeviceQueue)(device, family as u32, 0, &mut queue);
            let mut memory = zero();
            memory_fn(physical, &mut memory);
            Ok(Self {
                inner: Rc::new(Inner {
                    owner: device_owner,
                    functions,
                    physical,
                    queue,
                    family: family as u32,
                    memory,
                    properties,
                    timestamp_bits: queue_properties.timestampValidBits,
                    ray,
                    sorting,
                    maximal_reconvergence,
                    texture_indexing,
                    position_fetch,
                    subgroup_operations: if subgroup.stages & 0x20 != 0 {
                        subgroup.operations
                    } else {
                        0
                    },
                    next_submission: Cell::new(0),
                    float32_preserve: float_controls.shaderDenormPreserveFloat32 != 0
                        && float_controls.shaderSignedZeroInfNanPreserveFloat32 != 0,
                }),
            })
        }
    }
    pub fn name(&self) -> String {
        unsafe { CStr::from_ptr(self.inner.properties.deviceName.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }
    /// 显式复制闭合只读资源图；共享来源在本次传输内只复制一次。
    pub fn upload(&self, data: Dataset) -> Result<GpuDataset> {
        data.require_views()?;
        self.upload_graph(&data, &mut std::collections::BTreeMap::new())
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
    pub fn zeroed(&self, program: &Program, binding: usize, count: u32) -> Result<GpuDataset> {
        let schema = program
            .buffers()
            .get(binding)
            .ok_or_else(|| error("数据绑定编号越界"))?;
        if count != 0 && schema.ty.contains_views() {
            return Err(error(
                "非空 view 数据需要 upload_with_views 绑定资源，不能以零地址初始化",
            ));
        }
        let bytes = schema.stride as u64 * count as u64;
        let mut target =
            GpuDataset::new(self.inner.clone(), schema.ty.clone(), schema.stride, count)?;
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
            target.last_use = Some(self.submit_transfer(commands, Vec::new())?);
        }
        Ok(target)
    }
    /// 读回闭合资源图并绑定真实主机所有者，不返回可执行的设备地址副本。
    pub fn download(&self, data: &GpuDataset) -> Result<Dataset> {
        if !Rc::ptr_eq(&self.inner, &data.allocation.inner) {
            return Err(error("数据集属于另一设备"));
        }
        self.download_graph(data, &mut std::collections::BTreeMap::new())
    }
    fn read_payload(&self, data: &GpuDataset) -> Result<Vec<u32>> {
        let mut words = Vec::with_capacity(data.bytes() as usize / 4);
        let mut remaining = data.bytes();
        for page in &data.pages {
            let bytes = remaining.min(page.size);
            let target = Allocation::new(self.inner.clone(), bytes, 2, 6)?;
            let mut commands = Commands::new(self.inner.clone())?;
            commands.barrier(data.producer.stage(), 0x1000, data.producer.access(), 0x800);
            let copy = VkBufferCopy {
                srcOffset: 0,
                dstOffset: 0,
                size: bytes,
            };
            unsafe {
                (self.inner.functions.vkCmdCopyBuffer)(
                    commands.command,
                    page.buffer,
                    target.buffer,
                    1,
                    &copy,
                );
            }
            commands.barrier(0x1000, 0x4000, 0x1000, 0x2000);
            let dependencies =
                Execution::prepare(data.dependency().cloned().into_iter().collect())?;
            commands.submit()?;
            let execution = Execution::new(
                ExecutionResources::Transfer {
                    commands,
                    _staging: Vec::new(),
                },
                dependencies,
            );
            execution.wait()?;
            words.extend(target.read(bytes as usize / 4)?);
            remaining -= bytes;
        }
        Ok(words)
    }
    pub fn compile(&self, program: ExecutableProgram) -> Result<GpuProgram> {
        GpuProgram::new(self.inner.clone(), (*program).clone())
    }
    pub fn compile_template(&self, program: TemplateProgram) -> Result<GpuTemplate> {
        Ok(GpuTemplate {
            code: GpuProgram::new(self.inner.clone(), (*program).clone())?,
        })
    }
    /// 真实传输同时移动存储及其域关系；任意宿主转换不能签发有效域。
    pub fn upload_domains(
        &self,
        bindings: DomainBindings<Dataset>,
    ) -> Result<DomainBindings<GpuDataset>> {
        let mut memo = std::collections::BTreeMap::new();
        let datasets = bindings
            .datasets
            .into_iter()
            .map(|d| {
                d.require_views()?;
                self.upload_graph(&d, &mut memo)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(DomainBindings {
            datasets,
            domains: bindings.domains,
        })
    }
    pub fn download_domains(
        &self,
        bindings: DomainBindings<GpuDataset>,
    ) -> Result<DomainBindings<Dataset>> {
        let mut memo = std::collections::BTreeMap::new();
        let datasets = bindings
            .datasets
            .iter()
            .map(|d| {
                if !Rc::ptr_eq(&self.inner, &d.allocation.inner) {
                    return Err(error("数据集属于另一设备"));
                }
                self.download_graph(d, &mut memo)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(DomainBindings {
            datasets,
            domains: bindings.domains,
        })
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
    // allocation 是只读页表；数据页与它一起随唯一存储转移。
    allocation: Allocation,
    pages: Vec<Allocation>,
    ty: Ty,
    stride: u32,
    count: u32,
    producer: Usage,
    views: Views<ReadDataset>,
    instances: Option<ray::InstanceInput>,
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
pub type ViewBinding<'a> = crate::ViewBinding<'a, ReadDataset>;
impl ReadDataset {
    /// 将同一冻结物理存储直接交给原生read dataset绑定；不创建GPU记录、页表或传输。
    pub fn binding(&self) -> GpuDataset {
        GpuDataset {
            storage: ManuallyDrop::new(DatasetStorage::Read(self.clone())),
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
    pub fn allocated_bytes(&self) -> u64 {
        self.allocation.allocated_size
            + self.pages.iter().map(|p| p.allocated_size).sum::<u64>()
            + self
                .instances
                .as_ref()
                .map_or(0, |i| i.pointers.allocated_size)
    }
    fn page_records(&self) -> u32 {
        blanguage::spirv::PAGE_BYTES / self.stride
    }
    fn new(inner: Rc<Inner>, ty: Ty, stride: u32, count: u32) -> Result<Self> {
        Self::allocate(inner, ty, stride, count, 0)
    }
    fn allocate(
        inner: Rc<Inner>,
        ty: Ty,
        stride: u32,
        count: u32,
        extra_usage: u32,
    ) -> Result<Self> {
        let capacity = blanguage::spirv::PAGE_BYTES / stride;
        if capacity == 0 {
            return Err(error("GPU 页寻址要求单条记录能容纳于一个物理页"));
        }
        let mut pages = Vec::new();
        let mut remaining = count;
        while remaining > 0 {
            let n = remaining.min(capacity);
            pages.push(Allocation::new(
                inner.clone(),
                n as u64 * stride as u64,
                0x20023 | extra_usage,
                1,
            )?);
            remaining -= n;
        }
        let allocation = Self::table(inner, &pages)?;
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
    fn table(inner: Rc<Inner>, pages: &[Allocation]) -> Result<Allocation> {
        let table = Allocation::new(inner.clone(), (pages.len() as u64 * 8).max(8), 0x20020, 6)?;
        let mut words = Vec::with_capacity(pages.len() * 2);
        for page in pages {
            let info = VkBufferDeviceAddressInfo {
                sType: 1000244001,
                pNext: null(),
                buffer: page.buffer,
            };
            let address =
                unsafe { (inner.functions.vkGetBufferDeviceAddress)(inner.owner.device, &info) };
            words.extend([address as u32, (address >> 32) as u32]);
        }
        table.write(&words)?;
        Ok(table)
    }
    pub fn count(&self) -> u32 {
        self.count
    }
    pub fn bytes(&self) -> u64 {
        self.stride as u64 * self.count as u64
    }
}
struct Allocation {
    inner: Rc<Inner>,
    buffer: VkBuffer,
    memory: VkDeviceMemory,
    size: u64,
    allocated_size: u64,
    host_visible: bool,
}
impl Allocation {
    fn address(&self) -> u64 {
        let info = VkBufferDeviceAddressInfo {
            sType: 1000244001,
            pNext: null(),
            buffer: self.buffer,
        };
        unsafe { (self.inner.functions.vkGetBufferDeviceAddress)(self.inner.owner.device, &info) }
    }
    fn new(inner: Rc<Inner>, bytes: u64, usage: u32, flags: u32) -> Result<Self> {
        unsafe {
            let mut out = Self {
                inner,
                buffer: 0,
                memory: 0,
                size: bytes.max(4),
                allocated_size: 0,
                host_visible: flags & 2 != 0,
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
            let mut required = zero();
            (f.vkGetBufferMemoryRequirements)(device, out.buffer, &mut required);
            out.allocated_size = required.size;
            let memory = &out.inner.memory;
            let index = (0..memory.memoryTypeCount)
                .find(|i| {
                    required.memoryTypeBits & (1 << i) != 0
                        && memory.memoryTypes[*i as usize].propertyFlags & flags == flags
                })
                .ok_or_else(|| error(format!("目标设备缺少内存类型 flags={flags:#x}")))?;
            let mut ai: VkMemoryAllocateInfo = zero();
            ai.sType = 5;
            ai.allocationSize = required.size;
            ai.memoryTypeIndex = index;
            let allocation_flags = VkMemoryAllocateFlagsInfo {
                sType: 1000060000,
                pNext: null(),
                flags: 2,
                deviceMask: 0,
            };
            if usage & 0x20000 != 0 {
                ai.pNext = (&allocation_flags as *const VkMemoryAllocateFlagsInfo).cast();
            }
            let allocated = (f.vkAllocateMemory)(device, &ai, null(), &mut out.memory);
            if allocated != 0 {
                let ty = memory.memoryTypes[index as usize];
                let heap = memory.memoryHeaps[ty.heapIndex as usize];
                return Err(error(format!(
                    "vkAllocateMemory 返回 VkResult {allocated}：buffer_bytes={} allocation_bytes={} usage={usage:#x} memory_type={index} properties={:#x} heap={} heap_bytes={}",
                    out.size, required.size, ty.propertyFlags, ty.heapIndex, heap.size
                )));
            }
            check(
                (f.vkBindBufferMemory)(device, out.buffer, out.memory, 0),
                "vkBindBufferMemory",
            )?;
            Ok(out)
        }
    }
    fn write(&self, words: &[u32]) -> Result<()> {
        self.write_with(words, |_| {})
    }
    fn write_with(&self, words: &[u32], encode: impl FnOnce(&mut [u32])) -> Result<()> {
        if !self.host_visible {
            return Err(error("只能映射 host-visible 数据传输内存"));
        }
        unsafe {
            let mut ptr = null_mut();
            check(
                (self.inner.functions.vkMapMemory)(
                    self.inner.owner.device,
                    self.memory,
                    0,
                    self.size,
                    0,
                    &mut ptr,
                ),
                "vkMapMemory(write)",
            )?;
            std::ptr::copy_nonoverlapping(words.as_ptr(), ptr.cast(), words.len());
            encode(std::slice::from_raw_parts_mut(ptr.cast(), words.len()));
            (self.inner.functions.vkUnmapMemory)(self.inner.owner.device, self.memory);
        }
        Ok(())
    }
    fn read(&self, count: usize) -> Result<Vec<u32>> {
        if !self.host_visible {
            return Err(error("只能映射 host-visible 数据传输内存"));
        }
        let mut out = vec![0; count];
        unsafe {
            let mut ptr = null_mut();
            check(
                (self.inner.functions.vkMapMemory)(
                    self.inner.owner.device,
                    self.memory,
                    0,
                    self.size,
                    0,
                    &mut ptr,
                ),
                "vkMapMemory(read)",
            )?;
            std::ptr::copy_nonoverlapping(ptr.cast(), out.as_mut_ptr(), count);
            (self.inner.functions.vkUnmapMemory)(self.inner.owner.device, self.memory);
        }
        Ok(out)
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
            if self.memory != 0 {
                (f.vkFreeMemory)(device, self.memory, null());
            }
        }
    }
}
struct Commands {
    inner: Rc<Inner>,
    pool: VkCommandPool,
    command: VkCommandBuffer,
    completion: Rc<Completion>,
    descriptors: VkDescriptorPool,
    queries: u64,
    textures: Vec<Texture>,
}
impl Commands {
    fn new(inner: Rc<Inner>) -> Result<Self> {
        unsafe {
            let mut out = Self {
                completion: Completion::new(inner.clone())?,
                inner,
                pool: 0,
                command: null_mut(),
                descriptors: 0,
                queries: 0,
                textures: Vec::new(),
            };
            let f = &out.inner.functions;
            let device = out.inner.owner.device;
            let mut pool: VkCommandPoolCreateInfo = zero();
            pool.sType = 39;
            pool.queueFamilyIndex = out.inner.family;
            pool.flags = 1;
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
            let mut begin: VkCommandBufferBeginInfo = zero();
            begin.sType = 42;
            begin.flags = 1;
            check(
                (f.vkBeginCommandBuffer)(out.command, &begin),
                "vkBeginCommandBuffer",
            )?;
            Ok(out)
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
    fn submit(&mut self) -> Result<()> {
        self.submit_with(&[], &[], &[])
    }
    fn submit_with(&mut self, wait: &[u64], stages: &[u32], signal: &[u64]) -> Result<()> {
        let sequence = self.inner.next_submission.get();
        let next = sequence
            .checked_add(1)
            .ok_or_else(|| error("设备队列提交序号耗尽"))?;
        unsafe {
            let f = &self.inner.functions;
            check((f.vkEndCommandBuffer)(self.command), "vkEndCommandBuffer")?;
            let mut submit: VkSubmitInfo = zero();
            submit.sType = 4;
            submit.commandBufferCount = 1;
            submit.pCommandBuffers = &self.command;
            submit.waitSemaphoreCount = wait.len() as u32;
            submit.pWaitSemaphores = wait.as_ptr();
            submit.pWaitDstStageMask = stages.as_ptr();
            submit.signalSemaphoreCount = signal.len() as u32;
            submit.pSignalSemaphores = signal.as_ptr();
            check(
                (f.vkQueueSubmit)(self.inner.queue, 1, &submit, self.completion.fence.get()),
                "vkQueueSubmit",
            )?;
            self.completion.submitted.set(true);
            self.completion.sequence.set(sequence);
            self.inner.next_submission.set(next);
        }
        Ok(())
    }
    fn wait(&self) -> Result<()> {
        self.completion.wait()
    }
}
impl Drop for Commands {
    fn drop(&mut self) {
        self.completion.finish_for_drop();
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
struct Compiled {
    inner: Rc<Inner>,
    program: Program,
    set_layout: VkDescriptorSetLayout,
    layout: VkPipelineLayout,
    pipelines: Vec<VkPipeline>,
    tails: Vec<Option<VkPipeline>>,
    scan_adds: Vec<Option<VkPipeline>>,
    finishes: Vec<Option<VkPipeline>>,
    dispatches: Vec<Option<VkPipeline>>,
    selectors: Vec<Option<VkPipeline>>,
    sorts: Vec<Option<sort::Pipelines>>,
}
impl Drop for Compiled {
    fn drop(&mut self) {
        unsafe {
            let f = &self.inner.functions;
            let d = self.inner.owner.device;
            for p in self
                .pipelines
                .iter()
                .chain(self.tails.iter().flatten())
                .chain(self.scan_adds.iter().flatten())
                .chain(self.finishes.iter().flatten())
                .chain(self.dispatches.iter().flatten())
                .chain(self.selectors.iter().flatten())
            {
                (f.vkDestroyPipeline)(d, *p, null());
            }
            self.sorts.clear();
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
    buffers: Vec<GpuDataset>,
    arguments: Option<GpuDataset>,
    sorting: Option<sort::Storage>,
}
fn levels(mut count: u32) -> u32 {
    let mut n = 1;
    while count > 64 {
        count = count.div_ceil(64);
        n += 1;
    }
    n
}
fn pipeline(
    inner: &Inner,
    layout: VkPipelineLayout,
    words: &[u32],
    subgroup: bool,
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
        pipeline.stage.pName = c"main".as_ptr();
        let mut handle = 0;
        let result = (f.vkCreateComputePipelines)(device, 0, 1, &pipeline, null(), &mut handle);
        (f.vkDestroyShaderModule)(device, module, null());
        check(result, "vkCreateComputePipelines")?;
        Ok(handle)
    }
}
fn descriptor_set(
    commands: &mut Commands,
    compiled: &Compiled,
    infos: &[VkDescriptorBufferInfo],
    accelerations: &[Acceleration],
    auxiliary: Option<&Allocation>,
    indirect: Option<&Allocation>,
    selection: Option<&Allocation>,
) -> Result<u64> {
    unsafe {
        let f = &compiled.inner.functions;
        let device = compiled.inner.owner.device;
        let mut alloc: VkDescriptorSetAllocateInfo = zero();
        alloc.sType = 34;
        alloc.descriptorPool = commands.descriptors;
        alloc.descriptorSetCount = 1;
        alloc.pSetLayouts = &compiled.set_layout;
        let mut set = 0;
        check(
            (f.vkAllocateDescriptorSets)(device, &alloc, &mut set),
            "vkAllocateDescriptorSets",
        )?;
        let mut writes: Vec<_> = infos
            .iter()
            .enumerate()
            .map(|(i, info)| {
                let mut w: VkWriteDescriptorSet = zero();
                w.sType = 35;
                w.dstSet = set;
                w.dstBinding = i as u32;
                w.descriptorCount = 1;
                w.descriptorType = 7;
                w.pBufferInfo = info;
                w
            })
            .collect();
        let handles: Vec<_> = accelerations.iter().map(Acceleration::handle).collect();
        let structures: Vec<_> = handles
            .iter()
            .map(|h| ray::WriteAcceleration {
                kind: 1000150007,
                next: null(),
                count: 1,
                structures: h,
            })
            .collect();
        for (i, structure) in structures.iter().enumerate() {
            let mut w: VkWriteDescriptorSet = zero();
            w.sType = 35;
            w.pNext = (structure as *const ray::WriteAcceleration).cast();
            w.dstSet = set;
            w.dstBinding = (infos.len() + i) as u32;
            w.descriptorCount = 1;
            w.descriptorType = 1000150000;
            writes.push(w);
        }
        let images: Vec<_> = commands.textures.iter().map(Texture::descriptor).collect();
        let mut offset = 0;
        for (i, texture) in compiled.program.textures().iter().enumerate() {
            let mut w: VkWriteDescriptorSet = zero();
            w.sType = 35;
            w.dstSet = set;
            w.dstBinding = (infos.len() + accelerations.len() + i) as u32;
            w.descriptorCount = texture.elements();
            w.descriptorType = 1;
            w.pImageInfo = (&images[offset] as *const texture::DescriptorImageInfo).cast();
            writes.push(w);
            offset += texture.elements() as usize;
        }
        let auxiliary_infos: Vec<_> = [
            (infos.len(), auxiliary),
            (compiled.program.indirect_buffer(), indirect),
            (compiled.program.selection_buffer(), selection),
        ]
        .into_iter()
        .filter_map(|(binding, allocation)| {
            allocation.map(|a| {
                (
                    binding,
                    VkDescriptorBufferInfo {
                        buffer: a.buffer,
                        offset: 0,
                        range: a.size,
                    },
                )
            })
        })
        .collect();
        for (binding, info) in &auxiliary_infos {
            let mut w: VkWriteDescriptorSet = zero();
            w.sType = 35;
            w.dstSet = set;
            w.dstBinding =
                (binding + accelerations.len() + compiled.program.textures().len()) as u32;
            w.descriptorCount = 1;
            w.descriptorType = 7;
            w.pBufferInfo = info;
            writes.push(w);
        }
        (f.vkUpdateDescriptorSets)(device, writes.len() as u32, writes.as_ptr(), 0, null());
        Ok(set)
    }
}
#[derive(Clone)]
pub struct GpuProgram {
    compiled: Rc<Compiled>,
    partition_elements: NonZeroU32,
}
#[derive(Clone)]
pub struct GpuTemplate {
    code: GpuProgram,
}
pub struct Bindings<D = Vec<GpuDataset>> {
    pub datasets: D,
    pub accelerations: Vec<Acceleration>,
    pub textures: Vec<Texture>,
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
    fn new(inner: Rc<Inner>, program: Program) -> Result<Self> {
        if program.target() == Target::Cpu {
            return Err(error("CPU 专属程序不能提交给 GPU"));
        }
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
        let caps = program.subgroup_capabilities();
        if program
            .stages()
            .iter()
            .any(blanguage::ir::Stage::requires_float32_preserve)
            && !inner.float32_preserve
        {
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
        let limits = &inner.properties.limits;
        if !program.accelerations().is_empty() {
            let ray = inner
                .ray
                .as_ref()
                .ok_or_else(|| error("光线查询程序必须以 Device::for_program 声明设备能力"))?;
            if program.accelerations().len() > ray.properties.per_stage as usize {
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
        if program.texture_count() > limits.maxPerStageDescriptorSampledImages as usize
            || program.texture_count() > limits.maxPerStageDescriptorSamplers as usize
            || program.texture_count() > limits.maxDescriptorSetSampledImages as usize
            || program.texture_count() > limits.maxDescriptorSetSamplers as usize
        {
            return Err(error("纹理绑定数量超出设备能力"));
        }
        let auxiliary = program.auxiliary_buffers();
        if program.uniform_bytes() > limits.maxPushConstantsSize
            || program.buffers().len() + auxiliary
                > limits.maxPerStageDescriptorStorageBuffers as usize
            || program.buffers().len() + auxiliary + program.texture_count()
                > limits.maxPerStageResources as usize
            || program.stages().iter().any(|s| {
                s.workgroup_width() > limits.maxComputeWorkGroupInvocations
                    || s.workgroup_width() > limits.maxComputeWorkGroupSize[0]
                    || s.tile_bytes() > limits.maxComputeSharedMemorySize as u64
                    || matches!(&s.operation, blanguage::ir::Operation::CustomReduce(r)
                        if r.ty.components.len() as u64*64*4 > limits.maxComputeSharedMemorySize as u64)
                    || matches!(&s.operation, blanguage::ir::Operation::CustomScan(r)
                        if r.summary_words() as u64*64*4 > limits.maxComputeSharedMemorySize as u64)
                    || matches!(&s.operation, blanguage::ir::Operation::CustomSegmentedReduce(r)
                        if r.summary_words() as u64*64*4 > limits.maxComputeSharedMemorySize as u64)
            })
        {
            return Err(error("计算程序超出目标设备的资源能力"));
        }
        let mut out = Compiled {
            inner,
            program,
            set_layout: 0,
            layout: 0,
            pipelines: Vec::new(),
            tails: Vec::new(),
            scan_adds: Vec::new(),
            finishes: Vec::new(),
            dispatches: Vec::new(),
            selectors: Vec::new(),
            sorts: Vec::new(),
        };
        if !out.program.stages().is_empty() {
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
                for i in 0..out.program.stages().len() {
                    out.pipelines.push(pipeline(
                        &out.inner,
                        out.layout,
                        &blanguage::spirv::emit(&out.program, i).map_err(error)?,
                        false,
                    )?);
                    use blanguage::{
                        ir::Operation,
                        spirv::{Pass, emit_pass},
                    };
                    let tail = match out.program.stages()[i].operation {
                        Operation::Reduce(_) | Operation::CustomReduce(_) => {
                            Some(Pass::ReductionTail)
                        }
                        Operation::ScanExclusive(_)
                        | Operation::CustomScan(_)
                        | Operation::CustomSegmentedReduce(_)
                        | Operation::Compact(_)
                        | Operation::SegmentedReduce(_) => Some(Pass::ScanTail),
                        Operation::Map if out.program.stages()[i].selection.is_some() => {
                            Some(Pass::ScanTail)
                        }
                        Operation::Map | Operation::StableSort(_) => None,
                    }
                    .map(|pass| {
                        pipeline(
                            &out.inner,
                            out.layout,
                            &emit_pass(&out.program, i, pass).map_err(error)?,
                            false,
                        )
                    })
                    .transpose()?;
                    out.tails.push(tail);
                    let adds = if matches!(
                        out.program.stages()[i].operation,
                        Operation::ScanExclusive(_)
                            | Operation::CustomScan(_)
                            | Operation::CustomSegmentedReduce(_)
                            | Operation::Compact(_)
                            | Operation::SegmentedReduce(_)
                    ) || out.program.stages()[i].selection.is_some()
                    {
                        Some(pipeline(
                            &out.inner,
                            out.layout,
                            &emit_pass(&out.program, i, Pass::ScanAdd).map_err(error)?,
                            false,
                        )?)
                    } else {
                        None
                    };
                    out.scan_adds.push(adds);
                    let finish = match &out.program.stages()[i].operation {
                        Operation::Map if out.program.stages()[i].selection.is_some() => {
                            Some(Pass::SelectionScatter)
                        }
                        Operation::Compact(_) => Some(Pass::CompactScatter),
                        Operation::SegmentedReduce(_) | Operation::CustomSegmentedReduce(_) => {
                            Some(Pass::ScanRootAdd)
                        }
                        Operation::ScanExclusive(s) if s.needs_root_add(&out.program) => {
                            Some(Pass::ScanRootAdd)
                        }
                        Operation::CustomScan(s) if s.needs_root_add(&out.program) => {
                            Some(Pass::ScanRootAdd)
                        }
                        _ => None,
                    }
                    .map(|pass| {
                        pipeline(
                            &out.inner,
                            out.layout,
                            &emit_pass(&out.program, i, pass).map_err(error)?,
                            false,
                        )
                    })
                    .transpose()?;
                    out.finishes.push(finish);
                    let stage = &out.program.stages()[i];
                    let dispatch_pass = if stage.selection.is_some() {
                        Some(Pass::SelectionArguments)
                    } else {
                        stage.prefix.map(Pass::DispatchArguments)
                    };
                    let dispatch = dispatch_pass
                        .map(|pass| {
                            pipeline(
                                &out.inner,
                                out.layout,
                                &emit_pass(&out.program, i, pass).map_err(error)?,
                                false,
                            )
                        })
                        .transpose()?;
                    out.dispatches.push(dispatch);
                    out.selectors.push(if stage.selection.is_some() {
                        Some(pipeline(
                            &out.inner,
                            out.layout,
                            &emit_pass(&out.program, i, Pass::SelectionScan).map_err(error)?,
                            false,
                        )?)
                    } else {
                        None
                    });
                    let sorting =
                        if let Operation::StableSort(s) = &out.program.stages()[i].operation {
                            Some(sort::Pipelines::new(&out, i, s)?)
                        } else {
                            None
                        };
                    out.sorts.push(sorting);
                }
            }
        }
        Ok(Self {
            compiled: Rc::new(out),
            partition_elements: NonZeroU32::MAX,
        })
    }
    /// 静态域分批提交；有效前缀域使用 64 线程对齐的 X 行分区，并在 Y/Z 维扩展。
    pub fn with_partition_elements(mut self, count: NonZeroU32) -> Self {
        self.partition_elements = count;
        self
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
                dependencies: Vec::new(),
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
                owner.dependencies.push(used.clone());
            }
        }
        owner.bindings.append(extra.datasets);
        drop(owner.bindings.remap(order, Vec::new())?);
        self.submit_owned(owner, values, NonZeroU32::MIN)
    }
    fn submit_owned(
        &self,
        mut owner: InputOwner,
        values: &[Value],
        steps: NonZeroU32,
    ) -> Result<Submission> {
        let c = &self.compiled;
        owner.bindings.require(&c.program)?;
        if owner.textures.len() != c.program.texture_count()
            || owner.textures.iter().any(|t| !t.belongs_to(&c.inner))
        {
            return Err(error("纹理数量或设备归属与程序不一致"));
        }
        let data = &mut owner.bindings.datasets;
        for (dataset, binding) in data.iter().zip(c.program.buffers()) {
            if matches!(&*dataset.storage, DatasetStorage::Read(_))
                && binding.access != blanguage::ast::Access::Read
            {
                return Err(error("冻结的物理存储只能绑定read dataset"));
            }
        }

        let accelerations = &owner.accelerations;
        if accelerations.len() != c.program.accelerations().len()
            || accelerations.iter().any(|a| !a.belongs_to(&c.inner))
        {
            return Err(error("加速资源的数量或设备归属与程序不一致"));
        }
        for acceleration in accelerations {
            owner.dependencies.push(acceleration.produced());
        }
        for d in data.iter() {
            if !Rc::ptr_eq(&c.inner, &d.allocation.inner) {
                return Err(error("数据集不属于程序的设备"));
            }
            if let Some(used) = d.dependency() {
                owner.dependencies.push(used.clone());
            }
        }
        let mut resource_stage = 0;
        let mut resource_access = 0;
        let mut visited = BTreeSet::new();
        let mut resources: Vec<_> = data.iter().flat_map(|d| &d.views.sources).collect();
        while let Some(source) = resources.pop() {
            if visited.insert(Rc::as_ptr(&source.0) as usize) {
                resource_stage |= source.0.producer.stage();
                resource_access |= source.0.producer.access();
                if let Some(used) = source.0.dependency() {
                    owner.dependencies.push(used.clone());
                }
                resources.extend(&source.0.views.sources);
            }
        }
        let shapes: Vec<_> = data.iter().map(|d| (&d.ty, d.stride, d.count)).collect();
        let mut uniforms = arguments(&c.program, &shapes, values)?;
        let mut commands = Commands::new(c.inner.clone())?;
        commands.textures = std::mem::take(&mut owner.textures);
        if !owner.dependencies.is_empty() {
            // 同队列提交顺序本身不形成依赖；此交接同时覆盖 RAW、WAW 与 WAR。
            let access = 0x60
                | if accelerations.is_empty() {
                    0
                } else {
                    0x200000
                };
            commands.barrier(0x10000, 0x800, 0x10000, access);
            // 完成状态不替代设备内存依赖，先记录交接，再回收完成的前序资源。
            owner.dependencies = Execution::prepare(std::mem::take(&mut owner.dependencies))?;
        }
        let f = &c.inner.functions;
        let device = c.inner.owner.device;
        let mut scratch = Vec::new();
        // 同一串行提交中，完成消费的同容量 where 工作区由后续阶段复用。
        let mut scratch_indices = Vec::new();
        let mut selected_storage = BTreeMap::new();
        let query_count = u32::try_from(c.pipelines.len() as u64 * 2 * steps.get() as u64)?;
        let mut extra_sets = 0u64;
        for stage in c.program.stages() {
            let count = data[stage.domain].count;
            if stage.selection.is_some() {
                if let Some(&slot) = selected_storage.get(&count) {
                    extra_sets += (levels(count) * 2 + 2) as u64 * steps.get() as u64;
                    scratch_indices.push(slot);
                    scratch.push(None);
                    continue;
                }
                selected_storage.insert(count, scratch.len());
            }
            scratch_indices.push(scratch.len());
            let temporary = match stage.operation {
                blanguage::ir::Operation::Reduce(blanguage::ir::ReductionSpec {
                    ty: ref reduction_ty,
                    ..
                })
                | blanguage::ir::Operation::CustomReduce(blanguage::ir::CustomReduction {
                    ty: ref reduction_ty,
                    ..
                }) => {
                    extra_sets += levels(count) as u64 * steps.get() as u64;
                    if count > 64 {
                        let size = count.div_ceil(64);
                        let ty =
                            Ty::vector(blanguage::ir::Scalar::U32, reduction_ty.components.len());
                        let stride = ty.size;
                        Some(Scratch {
                            buffers: vec![
                                GpuDataset::new(c.inner.clone(), ty.clone(), stride, size)?,
                                GpuDataset::new(c.inner.clone(), ty, stride, size)?,
                            ],
                            arguments: None,
                            sorting: None,
                        })
                    } else {
                        None
                    }
                }
                blanguage::ir::Operation::Compact(_) => {
                    extra_sets += levels(count) as u64 * 2 * steps.get() as u64;
                    Some(compact::allocate(c, count)?)
                }
                blanguage::ir::Operation::ScanExclusive(ref s) => {
                    extra_sets += (levels(count) * 2 - 1) as u64 * steps.get() as u64;
                    Some(scan::allocate(c, count, s.summary_words())?)
                }
                blanguage::ir::Operation::CustomScan(ref s) => {
                    extra_sets += (levels(count) * 2 - 1) as u64 * steps.get() as u64;
                    Some(scan::allocate(c, count, s.summary_words())?)
                }
                blanguage::ir::Operation::CustomSegmentedReduce(ref r) => {
                    extra_sets += (levels(count) * 2 - 1).max(2) as u64 * steps.get() as u64;
                    Some(scan::allocate(c, count, r.summary_words())?)
                }
                blanguage::ir::Operation::SegmentedReduce(ref r) => {
                    extra_sets += (levels(count) * 2 - 1).max(2) as u64 * steps.get() as u64;
                    Some(scan::allocate(c, count, r.summary_words())?)
                }
                blanguage::ir::Operation::StableSort(ref s) => {
                    let histogram = count.div_ceil(64).max(1) * 16;
                    extra_sets += (1 + s.keys.len() as u64
                        * (1 + 8 * (2 * levels(histogram) + 1)) as u64)
                        * steps.get() as u64;
                    Some(sort::allocate(c, count)?)
                }
                blanguage::ir::Operation::Map if stage.selection.is_some() => {
                    extra_sets += (levels(count) * 2 + 1) as u64 * steps.get() as u64;
                    Some(selection::allocate(c, count)?)
                }
                blanguage::ir::Operation::Map => None,
            };
            let temporary = if stage.prefix.is_some() || stage.selection.is_some() {
                extra_sets += steps.get() as u64;
                let mut temporary = temporary.unwrap_or(Scratch {
                    buffers: Vec::new(),
                    arguments: None,
                    sorting: None,
                });
                temporary.arguments = Some(prefix::allocate(c, stage)?);
                Some(temporary)
            } else {
                temporary
            };
            scratch.push(temporary);
        }
        if query_count != 0 {
            unsafe {
                let sets = u32::try_from(1 + extra_sets)?;
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
                if !commands.textures.is_empty() {
                    sizes.push(VkDescriptorPoolSize {
                        r#type: 1,
                        descriptorCount: u32::try_from(
                            commands.textures.len() as u64 * sets as u64,
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
                let infos: Vec<_> = data
                    .iter()
                    .map(|d| VkDescriptorBufferInfo {
                        buffer: d.allocation.buffer,
                        offset: 0,
                        range: d.allocation.size,
                    })
                    .collect();
                let set =
                    descriptor_set(&mut commands, c, &infos, &accelerations, None, None, None)?;
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
                let mut pending: BTreeSet<_> = (0..data.len()).collect();
                let mut pending_reads = BTreeSet::new();
                let mut last: Vec<_> = data.iter().map(|d| d.producer).collect();
                for step in 0..steps.get() {
                    for (i, stage) in c.program.stages().iter().enumerate() {
                        let affected: Vec<_> = stage
                            .reads
                            .iter()
                            .chain(&stage.writes)
                            .copied()
                            .filter(|b| pending.contains(b))
                            .collect();
                        let war = stage.writes.iter().any(|b| pending_reads.contains(b));
                        if !affected.is_empty() || war || resource_stage != 0 {
                            let src = pending
                                .iter()
                                .fold(resource_stage | if war { 0x800 } else { 0 }, |s, b| {
                                    s | last[*b].stage()
                                });
                            let access = pending
                                .iter()
                                .fold(resource_access | if war { 0x20 } else { 0 }, |s, b| {
                                    s | last[*b].access()
                                });
                            commands.barrier(src, 0x800, access, 0x60);
                            pending.clear();
                            pending_reads.clear();
                            resource_stage = 0;
                            resource_access = 0;
                        }
                        let count = data[stage.domain].count;
                        uniforms[0] = count;
                        let query = (step as usize * c.pipelines.len() + i) as u32 * 2;
                        (f.vkCmdBindPipeline)(commands.command, 1, c.pipelines[i]);
                        (f.vkCmdBindDescriptorSets)(
                            commands.command,
                            1,
                            c.layout,
                            0,
                            1,
                            &set,
                            0,
                            null(),
                        );
                        (f.vkCmdWriteTimestamp)(commands.command, 0x800, commands.queries, query);
                        let prefix_set = if let Some(args) = scratch[scratch_indices[i]]
                            .as_mut()
                            .and_then(|s| s.arguments.as_mut())
                        {
                            Some(prefix::prepare(
                                &mut commands,
                                c,
                                i,
                                args,
                                &infos,
                                &accelerations,
                                &mut uniforms,
                                self.partition_elements,
                                step > 0 || scratch_indices[i] != i,
                            )?)
                        } else {
                            None
                        };
                        match &stage.operation {
                            blanguage::ir::Operation::StableSort(s) => {
                                let temporary = scratch[scratch_indices[i]].as_ref().unwrap();
                                sort::record(
                                    &mut commands,
                                    c,
                                    i,
                                    s,
                                    &data,
                                    temporary.sorting.as_ref().unwrap(),
                                    temporary.arguments.as_ref(),
                                    &infos,
                                    &accelerations,
                                    &mut uniforms,
                                    self.partition_elements,
                                )?;
                            }
                            blanguage::ir::Operation::Map if stage.selection.is_some() => {
                                selection::record(
                                    &mut commands,
                                    c,
                                    i,
                                    &data,
                                    scratch[scratch_indices[i]].as_ref().unwrap(),
                                    &infos,
                                    &accelerations,
                                    &mut uniforms,
                                    self.partition_elements,
                                )?;
                            }
                            blanguage::ir::Operation::Map if stage.prefix.is_some() => {
                                prefix::dispatch(
                                    &commands,
                                    c,
                                    c.pipelines[i],
                                    prefix_set.unwrap(),
                                    scratch[scratch_indices[i]]
                                        .as_ref()
                                        .unwrap()
                                        .arguments
                                        .as_ref()
                                        .unwrap(),
                                    &mut uniforms,
                                    0,
                                );
                            }
                            blanguage::ir::Operation::Map => {
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
                            blanguage::ir::Operation::SegmentedReduce(r) => {
                                scan::record_segmented_reduce(
                                    &mut commands,
                                    c,
                                    [
                                        c.pipelines[i],
                                        c.tails[i].unwrap(),
                                        c.scan_adds[i].unwrap(),
                                        c.finishes[i].unwrap(),
                                    ],
                                    &r.values.fields,
                                    &data[r.values.fields.source],
                                    &scratch[scratch_indices[i]].as_ref().unwrap().buffers,
                                    scratch[scratch_indices[i]]
                                        .as_ref()
                                        .and_then(|s| s.arguments.as_ref()),
                                    &infos,
                                    &accelerations,
                                    &mut uniforms,
                                    self.partition_elements,
                                )?;
                            }
                            blanguage::ir::Operation::CustomSegmentedReduce(r) => {
                                scan::record_segmented_reduce(
                                    &mut commands,
                                    c,
                                    [
                                        c.pipelines[i],
                                        c.tails[i].unwrap(),
                                        c.scan_adds[i].unwrap(),
                                        c.finishes[i].unwrap(),
                                    ],
                                    &r.values.fields,
                                    &data[r.values.fields.source],
                                    &scratch[scratch_indices[i]].as_ref().unwrap().buffers,
                                    scratch[scratch_indices[i]]
                                        .as_ref()
                                        .and_then(|s| s.arguments.as_ref()),
                                    &infos,
                                    &accelerations,
                                    &mut uniforms,
                                    self.partition_elements,
                                )?;
                            }
                            blanguage::ir::Operation::ScanExclusive(r) => {
                                let add = c.scan_adds[i].unwrap();
                                let root_add = if r.needs_root_add(&c.program) {
                                    c.finishes[i].unwrap()
                                } else {
                                    add
                                };
                                scan::record(
                                    &mut commands,
                                    c,
                                    [c.pipelines[i], c.tails[i].unwrap(), add, root_add],
                                    &r.values,
                                    r.head_word.is_some(),
                                    &data[r.values.source],
                                    &data[r.values.target].allocation,
                                    &scratch[scratch_indices[i]].as_ref().unwrap().buffers,
                                    scratch[scratch_indices[i]]
                                        .as_ref()
                                        .and_then(|s| s.arguments.as_ref()),
                                    &infos,
                                    &accelerations,
                                    &mut uniforms,
                                    self.partition_elements,
                                    0,
                                )?;
                            }
                            blanguage::ir::Operation::CustomScan(r) => {
                                let fields = &r.values.fields;
                                let add = c.scan_adds[i].unwrap();
                                let root_add = if r.needs_root_add(&c.program) {
                                    c.finishes[i].unwrap()
                                } else {
                                    add
                                };
                                scan::record(
                                    &mut commands,
                                    c,
                                    [c.pipelines[i], c.tails[i].unwrap(), add, root_add],
                                    fields,
                                    r.head_word.is_some(),
                                    &data[fields.source],
                                    &data[fields.target].allocation,
                                    &scratch[scratch_indices[i]].as_ref().unwrap().buffers,
                                    scratch[scratch_indices[i]]
                                        .as_ref()
                                        .and_then(|s| s.arguments.as_ref()),
                                    &infos,
                                    &accelerations,
                                    &mut uniforms,
                                    self.partition_elements,
                                    0,
                                )?;
                            }
                            blanguage::ir::Operation::Compact(selection) => {
                                compact::record(
                                    &mut commands,
                                    c,
                                    i,
                                    selection,
                                    &data,
                                    scratch[scratch_indices[i]].as_ref().unwrap(),
                                    &infos,
                                    &accelerations,
                                    &mut uniforms,
                                    self.partition_elements,
                                )?;
                            }
                            blanguage::ir::Operation::Reduce(blanguage::ir::ReductionSpec {
                                fields: reduction,
                                ..
                            })
                            | blanguage::ir::Operation::CustomReduce(
                                blanguage::ir::CustomReduction {
                                    fields: reduction, ..
                                },
                            ) => {
                                let mut length = count;
                                let mut pass = 0;
                                loop {
                                    let groups = length.div_ceil(64).max(1);
                                    if pass > 0 {
                                        commands.barrier(0x800, 0x800, 0x40, 0x60);
                                    }
                                    let source = if pass == 0 {
                                        &data[reduction.source].allocation
                                    } else {
                                        &scratch[scratch_indices[i]].as_ref().unwrap().buffers
                                            [(pass - 1) % 2]
                                            .allocation
                                    };
                                    let target = if groups == 1 {
                                        &data[reduction.target].allocation
                                    } else {
                                        &scratch[scratch_indices[i]].as_ref().unwrap().buffers
                                            [pass % 2]
                                            .allocation
                                    };
                                    let mut pass_infos = infos.clone();
                                    pass_infos[reduction.source] = VkDescriptorBufferInfo {
                                        buffer: source.buffer,
                                        offset: 0,
                                        range: source.size,
                                    };
                                    pass_infos[reduction.target] = VkDescriptorBufferInfo {
                                        buffer: target.buffer,
                                        offset: 0,
                                        range: target.size,
                                    };
                                    let pass_set = descriptor_set(
                                        &mut commands,
                                        c,
                                        &pass_infos,
                                        &accelerations,
                                        None,
                                        scratch[scratch_indices[i]]
                                            .as_ref()
                                            .and_then(|s| s.arguments.as_ref())
                                            .map(|a| &a.allocation),
                                        None,
                                    )?;
                                    uniforms[2 + reduction.target] = groups;
                                    scan::dispatch(
                                        &commands,
                                        c,
                                        if pass == 0 {
                                            c.pipelines[i]
                                        } else {
                                            c.tails[i].unwrap()
                                        },
                                        pass_set,
                                        length,
                                        &mut uniforms,
                                        self.partition_elements,
                                        scratch[scratch_indices[i]]
                                            .as_ref()
                                            .and_then(|s| s.arguments.as_ref()),
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
                        (f.vkCmdWriteTimestamp)(
                            commands.command,
                            0x800,
                            commands.queries,
                            query + 1,
                        );
                        pending_reads.extend(stage.reads.iter().copied());
                        for b in &stage.writes {
                            pending.insert(*b);
                            last[*b] = Usage::Compute;
                            data[*b].owned_storage()?.producer = Usage::Compute;
                        }
                    }
                }
            }
        }
        commands.submit()?;
        let queries = (query_count != 0).then(|| QueryPool::take(&mut commands, query_count));
        let execution = Execution::new(
            ExecutionResources::Compute {
                commands,
                _compiled: c.clone(),
                _accelerations: owner.accelerations.clone(),
                _scratch: scratch,
                _queries: queries.clone(),
            },
            owner.dependencies,
        );
        for dataset in data.iter_mut() {
            dataset.last_use = Some(execution.clone());
        }
        owner.bindings.completed(&c.program);
        Ok(Submission {
            execution,
            bindings: owner.bindings,
            accelerations: owner.accelerations,
            queries,
        })
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
    pub fn submit(
        &self,
        bindings: impl Into<Bindings<DomainBindings<GpuDataset>>>,
        values: &[Value],
    ) -> Result<Submission> {
        self.code.submit(bindings, values)
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
// 数据本身持有最后使用点，输入容器无需维护另一套原生资源所属表。
struct InputOwner {
    dependencies: Vec<Rc<Execution>>,
    bindings: DomainBindings<GpuDataset>,
    accelerations: Vec<Acceleration>,
    textures: Vec<Texture>,
}
impl InputOwner {
    fn after(
        previous: impl IntoIterator<Item = Submission>,
        accelerations: Vec<Acceleration>,
        textures: Vec<Texture>,
        inner: &Rc<Inner>,
    ) -> Result<Self> {
        let mut owner = Self {
            dependencies: Vec::new(),
            bindings: Vec::new().into(),
            accelerations,
            textures,
        };
        for previous in previous {
            if !Rc::ptr_eq(inner, &previous.execution.point.inner) {
                return Err(error("跨提交衔接必须属于同一设备队列"));
            }
            let Submission {
                execution,
                bindings,
                accelerations: _,
                queries: _,
            } = previous;
            owner.dependencies.push(execution);
            owner.bindings.append(bindings);
        }
        Ok(owner)
    }
}
pub struct Submission {
    execution: Rc<Execution>,
    bindings: DomainBindings<GpuDataset>,
    accelerations: Vec<Acceleration>,
    queries: Option<Rc<QueryPool>>,
}
pub struct Completed {
    bindings: DomainBindings<GpuDataset>,
    pub accelerations: Vec<Acceleration>,
    pub timings: GpuTimings,
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
    pub fn into_parts(self) -> (DomainBindings<GpuDataset>, Vec<Acceleration>, GpuTimings) {
        (self.bindings, self.accelerations, self.timings)
    }
}
pub struct GpuTimings {
    pub elapsed_ms: f64,
    pub stage_ms: Vec<f64>,
}
impl Submission {
    pub fn is_complete(&self) -> Result<bool> {
        self.execution.is_complete()
    }
    /// 交还尚未等待的数据；物理存储继续持有真实最后使用点。
    pub fn into_bindings(self) -> DomainBindings<GpuDataset> {
        self.bindings
    }
    pub fn partition(self, groups: &[&[usize]]) -> Result<Vec<DomainBindings<GpuDataset>>> {
        self.into_bindings().partition(groups)
    }
    pub fn wait(mut self) -> Result<Completed> {
        self.execution.wait()?;
        let timings = match &self.queries {
            Some(queries) => queries.read()?,
            None => GpuTimings {
                elapsed_ms: 0.0,
                stage_ms: Vec::new(),
            },
        };
        for dataset in &mut self.bindings.datasets {
            dataset.last_use = None;
        }
        Ok(Completed {
            bindings: self.bindings,
            accelerations: self.accelerations,
            timings,
        })
    }
}
