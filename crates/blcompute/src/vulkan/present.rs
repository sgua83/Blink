use super::*;
use std::{collections::VecDeque, rc::Weak, task::Poll};

#[derive(Default)]
pub(super) struct MaintenanceExtensions {
    khr: bool,
    ext: bool,
}
impl MaintenanceExtensions {
    pub(super) fn enable(library: &Library, extensions: &mut Vec<*const c_char>) -> Result<Self> {
        let enumerate: unsafe extern "system" fn(*const c_char, *mut u32, *mut ExtensionProperties) -> i32 =
            unsafe { library.get(null_mut(), c"vkEnumerateInstanceExtensionProperties")? };
        let mut count = 0;
        check(
            unsafe { enumerate(null(), &mut count, null_mut()) },
            "vkEnumerateInstanceExtensionProperties(count)",
        )?;
        let mut available: Vec<ExtensionProperties> = (0..count).map(|_| zero()).collect();
        check(
            unsafe { enumerate(null(), &mut count, available.as_mut_ptr()) },
            "vkEnumerateInstanceExtensionProperties",
        )?;
        let available = &available[..count as usize];
        let has = |name| available.iter().any(|e| unsafe { CStr::from_ptr(e.name.as_ptr()) } == name);
        if !has(c"VK_KHR_get_surface_capabilities2") {
            return Ok(Self::default());
        }
        let khr = has(c"VK_KHR_surface_maintenance1");
        let ext = has(c"VK_EXT_surface_maintenance1");
        if khr || ext {
            extensions.push(c"VK_KHR_get_surface_capabilities2".as_ptr());
        }
        if khr {
            extensions.push(c"VK_KHR_surface_maintenance1".as_ptr());
        }
        if ext {
            extensions.push(c"VK_EXT_surface_maintenance1".as_ptr());
        }
        Ok(Self { khr, ext })
    }

    pub(super) fn device_extension(&self, available: &[ExtensionProperties]) -> Option<&'static CStr> {
        [
            (self.khr, c"VK_KHR_swapchain_maintenance1"),
            (self.ext, c"VK_EXT_swapchain_maintenance1"),
        ]
        .into_iter()
        .find_map(|(enabled, name)| {
            (enabled && available.iter().any(|e| unsafe { CStr::from_ptr(e.name.as_ptr()) } == name))
                .then_some(name)
        })
    }
}

#[repr(C)]
pub(super) struct MaintenanceFeatures {
    pub kind: u32,
    pub next: *mut c_void,
    pub enabled: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowDpiAwarenessContext(window: *mut c_void) -> *mut c_void;
    fn SetThreadDpiAwarenessContext(context: *mut c_void) -> *mut c_void;
}
// WSI必须在该真实窗口的DPI上下文调用；离开操作后归还调用线程的原上下文。
struct WindowDpi(*mut c_void);
impl WindowDpi {
    fn enter(context: *mut c_void) -> Result<Self> {
        let previous = unsafe { SetThreadDpiAwarenessContext(context) };
        if previous.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(previous))
    }
}
impl Drop for WindowDpi {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct Extent2D {
    width: u32,
    height: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Capabilities {
    min_count: u32,
    max_count: u32,
    current: Extent2D,
    min: Extent2D,
    max: Extent2D,
    max_layers: u32,
    transforms: u32,
    current_transform: u32,
    composite: u32,
    usage: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Format {
    format: u32,
    color_space: u32,
}
#[repr(C)]
struct Win32SurfaceInfo {
    kind: u32,
    next: *const c_void,
    flags: u32,
    instance: *mut c_void,
    window: *mut c_void,
}
#[repr(C)]
pub(super) struct SwapchainCreateInfo {
    kind: u32,
    next: *const c_void,
    flags: u32,
    surface: u64,
    min_count: u32,
    format: u32,
    color_space: u32,
    extent: Extent2D,
    layers: u32,
    usage: u32,
    sharing: u32,
    family_count: u32,
    families: *const u32,
    transform: u32,
    composite: u32,
    present_mode: u32,
    clipped: u32,
    old: u64,
}
#[repr(C)]
pub(super) struct SemaphoreCreateInfo {
    kind: u32,
    next: *const c_void,
    flags: u32,
}
#[repr(C)]
pub(super) struct PresentInfo {
    kind: u32,
    next: *const c_void,
    wait_count: u32,
    wait: *const u64,
    chain_count: u32,
    chains: *const u64,
    indices: *const u32,
    results: *mut i32,
}
#[repr(C)]
struct PresentFenceInfo {
    kind: u32,
    next: *const c_void,
    chain_count: u32,
    fences: *const VkFence,
}
type DestroySurface = unsafe extern "system" fn(VkInstance, u64, *const c_void);

/// 两者均成功取得并提交图像；Suboptimal保留WSI报告的非最优表面事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentStatus {
    Optimal,
    Suboptimal,
}
impl PresentStatus {
    fn read(code: i32, operation: &str) -> Result<Self> {
        match code {
            0 => Ok(Self::Optimal),
            1000001003 => Ok(Self::Suboptimal),
            _ => Err(error(format!("{operation} 返回 VkResult {code}"))),
        }
    }
}

enum Acquisition {
    Idle,
    Acquired,
    CopyPending(Rc<Execution>),
    Failed(Option<Weak<Execution>>),
}

enum PresentCompletion {
    NoPresent,
    Unfenced,
    Fenced(VecDeque<VkFence>),
}

struct PresentCopy {
    index: u32,
    acquired: PresentStatus,
    execution: Rc<Execution>,
}

pub struct Swapchain {
    window_dpi: *mut c_void,
    extent: Extent2D,
    inner: Rc<Inner>,
    functions: PresentFunctions,
    surface: u64,
    destroy_surface: DestroySurface,
    chain: u64,
    images: Vec<u64>,
    acquired: u64,
    rendered: Vec<u64>,
    acquisition: Acquisition,
    presentation: PresentCompletion,
}
impl Device {
    /// 窗口必须在 Swapchain 析构之后销毁；调用者拥有原生窗口生命周期。
    /// 设备通过 Device::with_presentation 创建，明确启用窗口呈现扩展。
    pub unsafe fn swapchain(
        &self,
        instance: *mut c_void,
        window: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<Swapchain> {
        self.inner.active_sequence()?;
        unsafe {
            let window_dpi = GetWindowDpiAwarenessContext(window);
            if window_dpi.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            let _dpi = WindowDpi::enter(window_dpi)?;
            width
                .checked_mul(height)
                .ok_or_else(|| error("呈现图像超出当前 dataset 的 u32 寻址能力"))?;
            let owner = &self.inner.owner._instance;
            let vk_instance = owner.instance;
            let library = &owner.library;
            let get = library.get(vk_instance, c"vkGetDeviceProcAddr")?;
            let functions = PresentFunctions::load(self.inner.owner.device, get)?;
            let create: unsafe extern "system" fn(
                VkInstance,
                *const Win32SurfaceInfo,
                *const c_void,
                *mut u64,
            ) -> i32 = library.get(vk_instance, c"vkCreateWin32SurfaceKHR")?;
            let destroy_surface: DestroySurface =
                library.get(vk_instance, c"vkDestroySurfaceKHR")?;
            let support: unsafe extern "system" fn(VkPhysicalDevice, u32, u64, *mut u32) -> i32 =
                library.get(vk_instance, c"vkGetPhysicalDeviceSurfaceSupportKHR")?;
            let capabilities: unsafe extern "system" fn(
                VkPhysicalDevice,
                u64,
                *mut Capabilities,
            ) -> i32 = library.get(vk_instance, c"vkGetPhysicalDeviceSurfaceCapabilitiesKHR")?;
            let formats: unsafe extern "system" fn(
                VkPhysicalDevice,
                u64,
                *mut u32,
                *mut Format,
            ) -> i32 = library.get(vk_instance, c"vkGetPhysicalDeviceSurfaceFormatsKHR")?;
            let mut out = Swapchain {
                window_dpi,
                extent: Extent2D { width, height },
                inner: self.inner.clone(),
                functions,
                surface: 0,
                destroy_surface,
                chain: 0,
                images: Vec::new(),
                acquired: 0,
                rendered: Vec::new(),
                acquisition: Acquisition::Idle,
                presentation: if self.inner.present_fences {
                    PresentCompletion::Fenced(VecDeque::new())
                } else {
                    PresentCompletion::NoPresent
                },
            };
            let info = Win32SurfaceInfo {
                kind: 1000009000,
                next: null(),
                flags: 0,
                instance,
                window,
            };
            check(
                create(vk_instance, &info, null(), &mut out.surface),
                "vkCreateWin32SurfaceKHR",
            )?;
            let mut supported = 0;
            check(
                support(
                    self.inner.physical,
                    self.inner.family,
                    out.surface,
                    &mut supported,
                ),
                "vkGetPhysicalDeviceSurfaceSupportKHR",
            )?;
            if supported == 0 {
                return Err(error("选定计算队列不能呈现到目标窗口"));
            }
            let mut caps = zero();
            check(
                capabilities(self.inner.physical, out.surface, &mut caps),
                "vkGetPhysicalDeviceSurfaceCapabilitiesKHR",
            )?;
            let wanted = Extent2D { width, height };
            if (caps.current.width != u32::MAX
                && (caps.current.width != wanted.width || caps.current.height != wanted.height))
                || wanted.width < caps.min.width
                || wanted.height < caps.min.height
                || wanted.width > caps.max.width
                || wanted.height > caps.max.height
            {
                return Err(error(format!(
                    "窗口表面不是要求的 {width}x{height}，实际 {}x{}",
                    caps.current.width, caps.current.height
                )));
            }
            if caps.usage & 2 == 0 || caps.composite & 1 == 0 {
                return Err(error("表面缺少传输目标或不透明合成能力"));
            }
            let mut count = 0;
            check(
                formats(self.inner.physical, out.surface, &mut count, null_mut()),
                "vkGetPhysicalDeviceSurfaceFormatsKHR(count)",
            )?;
            let mut available = vec![zero::<Format>(); count as usize];
            check(
                formats(
                    self.inner.physical,
                    out.surface,
                    &mut count,
                    available.as_mut_ptr(),
                ),
                "vkGetPhysicalDeviceSurfaceFormatsKHR",
            )?;
            if !available
                .iter()
                .any(|f| f.format == 44 && f.color_space == 0)
            {
                return Err(error("目标表面需要 BGRA8_UNORM 与 SRGB_NONLINEAR"));
            }
            let count = if caps.max_count == 0 {
                caps.min_count + 1
            } else {
                (caps.min_count + 1).min(caps.max_count)
            };
            let info = SwapchainCreateInfo {
                kind: 1000001000,
                next: null(),
                flags: 0,
                surface: out.surface,
                min_count: count,
                format: 44,
                color_space: 0,
                extent: wanted,
                layers: 1,
                usage: 2,
                sharing: 0,
                family_count: 0,
                families: null(),
                transform: caps.current_transform,
                composite: 1,
                present_mode: 2,
                clipped: 1,
                old: 0,
            };
            let f = &out.functions;
            let device = self.inner.owner.device;
            let result = (f.vkCreateSwapchainKHR)(device, &info, null(), &mut out.chain);
            check(self.inner.observe_device_loss(result), "vkCreateSwapchainKHR")?;
            let mut count = 0;
            check(
                (f.vkGetSwapchainImagesKHR)(device, out.chain, &mut count, null_mut()),
                "vkGetSwapchainImagesKHR(count)",
            )?;
            out.images.resize(count as usize, 0);
            check(
                (f.vkGetSwapchainImagesKHR)(device, out.chain, &mut count, out.images.as_mut_ptr()),
                "vkGetSwapchainImagesKHR",
            )?;
            let info = SemaphoreCreateInfo {
                kind: 9,
                next: null(),
                flags: 0,
            };
            check(
                (f.vkCreateSemaphore)(device, &info, null(), &mut out.acquired),
                "vkCreateSemaphore(acquire)",
            )?;
            for _ in 0..count {
                let mut semaphore = 0;
                check(
                    (f.vkCreateSemaphore)(device, &info, null(), &mut semaphore),
                    "vkCreateSemaphore(present)",
                )?;
                out.rendered.push(semaphore);
            }
            Ok(out)
        }
    }
}
impl Swapchain {
    /// 同步呈现；返回前已提交的源图像拷贝结束对借用的使用。
    pub fn present(&mut self, display: &GpuDataset) -> Result<PresentStatus> {
        self.ready_for_acquire(true)?;
        let _dpi = WindowDpi::enter(self.window_dpi)?;
        let Poll::Ready(copy) = self.submit_copy(display, u64::MAX)? else {
            return Err(error("无限等待的呈现图像获取未完成"));
        };
        let presented = self.queue_present(copy.index, copy.acquired);
        let copied = copy.execution.wait();
        // The borrowed display must outlive the copy, including a nonterminal wait error.
        drop(copy.execution);
        let status = presented?;
        self.acquisition = Acquisition::Idle;
        copied?;
        Ok(status)
    }
    /// 只在原acquire信号量已消费且交换链图像可用时提交；Pending不提交或改写输入。
    /// Ready表示已排入呈现；拷贝沿display的真实最后使用点完成，不表示已经上屏。
    /// 已入队的拷贝在呈现报错时同样更新display的最后使用点。
    pub fn try_present(&mut self, display: &mut GpuDataset) -> Result<Poll<PresentStatus>> {
        if !self.ready_for_acquire(false)? {
            return Ok(Poll::Pending);
        }
        let _dpi = WindowDpi::enter(self.window_dpi)?;
        let Poll::Ready(copy) = self.submit_copy(display, 0)? else {
            return Ok(Poll::Pending);
        };
        display.last_use = Some(copy.execution.clone());
        let status = self.queue_present(copy.index, copy.acquired)?;
        self.acquisition = Acquisition::CopyPending(copy.execution);
        Ok(Poll::Ready(status))
    }
    fn ready_for_acquire(&mut self, wait: bool) -> Result<bool> {
        match &self.acquisition {
            Acquisition::Idle => Ok(true),
            Acquisition::CopyPending(previous) => {
                if wait {
                    previous.wait()?;
                } else if !previous.is_complete()? {
                    return Ok(false);
                }
                self.acquisition = Acquisition::Idle;
                Ok(true)
            }
            Acquisition::Acquired | Acquisition::Failed(_) => Err(error("交换链在取得图像后呈现失败，须结束并重新创建")),
        }
    }
    fn submit_copy(
        &mut self,
        display: &GpuDataset,
        timeout: u64,
    ) -> Result<Poll<PresentCopy>> {
        if !Rc::ptr_eq(&self.inner, &display.allocation.inner)
            || display.count != self.extent.width * self.extent.height
            || display.stride != 4
        {
            return Err(error(
                "呈现输入必须是同设备上与交换链尺寸一致的完整 BGRA8 数据",
            ));
        }
        self.inner.active_sequence()?;
        unsafe {
            let f = &self.inner.functions;
            let d = self.inner.owner.device;
            let mut index = 0;
            let result = (self.functions.vkAcquireNextImageKHR)(
                d,
                self.chain,
                timeout,
                self.acquired,
                0,
                &mut index,
            );
            let result = self.inner.observe_device_loss(result);
            if result == 1 || result == 2 {
                return Ok(Poll::Pending);
            }
            let acquired = PresentStatus::read(result, "vkAcquireNextImageKHR")?;
            self.acquisition = Acquisition::Acquired;
            let previous = Execution::prepare_one(display.dependency())?;
            let commands = Commands::new(self.inner.clone())?;
            commands.barrier(
                display.producer.stage(),
                0x1000,
                display.producer.access(),
                0x800,
            );
            let image = self.images[index as usize];
            let range = || SubresourceRange {
                aspect: 1,
                base_mip: 0,
                levels: 1,
                base_layer: 0,
                layers: 1,
            };
            let barrier = ImageBarrier {
                kind: 45,
                next: null(),
                source_access: 0,
                target_access: 0x1000,
                old_layout: 0,
                new_layout: 7,
                source_family: u32::MAX,
                target_family: u32::MAX,
                image,
                range: range(),
            };
            (f.vkCmdPipelineBarrier)(
                commands.command,
                1,
                0x1000,
                0,
                0,
                null(),
                0,
                null(),
                1,
                (&barrier as *const ImageBarrier).cast(),
            );
            commands.copy_dataset_to_image(
                display, image, 0, [self.extent.width, self.extent.height], 0, self.extent.width,
            );
            let barrier = ImageBarrier {
                kind: 45,
                next: null(),
                source_access: 0x1000,
                target_access: 0,
                old_layout: 7,
                new_layout: 1000001002,
                source_family: u32::MAX,
                target_family: u32::MAX,
                image,
                range: range(),
            };
            (f.vkCmdPipelineBarrier)(
                commands.command,
                0x1000,
                0x2000,
                0,
                0,
                null(),
                0,
                null(),
                1,
                (&barrier as *const ImageBarrier).cast(),
            );
            let signal = self.rendered[index as usize];
            let execution = Execution::submit_with(
                ExecutionResources::Present { commands },
                previous.cloned().map_or(Dependencies::Empty, Dependencies::Single),
                &[self.acquired],
                &[0x1000],
                &[signal],
            )?;
            // The accepted copy owns the wait on the acquire semaphore.
            self.acquisition = Acquisition::Failed(
                (!matches!(self.presentation, PresentCompletion::Unfenced))
                    .then(|| Rc::downgrade(&execution)),
            );
            Ok(Poll::Ready(PresentCopy { index, acquired, execution }))
        }
    }
    fn queue_present(&mut self, index: u32, acquired: PresentStatus) -> Result<PresentStatus> {
        let fence = match &mut self.presentation {
            PresentCompletion::Fenced(pending) => take_present_fence(&self.inner, pending)?,
            PresentCompletion::NoPresent | PresentCompletion::Unfenced => 0,
        };
        let fence_info = PresentFenceInfo {
            kind: 1000275001,
            next: null(),
            chain_count: 1,
            fences: &fence,
        };
        let info = PresentInfo {
            kind: 1000001001,
            next: if fence != 0 { (&fence_info as *const PresentFenceInfo).cast() } else { null() },
            wait_count: 1,
            wait: &self.rendered[index as usize],
            chain_count: 1,
            chains: &self.chain,
            indices: &index,
            results: null_mut(),
        };
        let result = unsafe { (self.functions.vkQueuePresentKHR)(self.inner.queue, &info) };
        let result = self.inner.observe_device_loss(result);
        // These rejection results still enqueue the wait on the copy signal.
        if matches!(result, 0 | 1000001003 | -1000000000 | -1000001004 | -1000255000 | -1000208000) {
            if let PresentCompletion::Fenced(pending) = &mut self.presentation {
                pending.push_back(fence);
            } else {
                self.presentation = PresentCompletion::Unfenced;
            }
            self.acquisition = Acquisition::Failed(None);
        } else if fence != 0 {
            destroy_present_fence(&self.inner, fence);
        }
        let queued = PresentStatus::read(result, "vkQueuePresentKHR")?;
        if acquired == PresentStatus::Suboptimal {
            Ok(acquired)
        } else {
            Ok(queued)
        }
    }
}

fn take_present_fence(inner: &Inner, pending: &mut VecDeque<VkFence>) -> Result<VkFence> {
    if let Some(&oldest) = pending.front() {
        let device = inner.owner.device;
        let functions = &inner.functions;
        let status = unsafe { (functions.vkGetFenceStatus)(device, oldest) };
        if status != 1 {
            check(inner.observe_device_loss(status), "vkGetFenceStatus(present)")?;
            pending.pop_front();
            let reset = unsafe { (functions.vkResetFences)(device, 1, &oldest) };
            if let Err(error) = check(inner.observe_device_loss(reset), "vkResetFences(present)") {
                destroy_present_fence(inner, oldest);
                return Err(error);
            }
            return Ok(oldest);
        }
    }
    pending.try_reserve(1)?;
    inner.create_fence()
}

fn wait_present_fence(inner: &Inner, fence: VkFence) {
    let result = unsafe {
        (inner.functions.vkWaitForFences)(inner.owner.device, 1, &fence, 1, u64::MAX)
    };
    if !matches!(inner.observe_device_loss(result), 0 | -4) {
        eprintln!("呈现资源终止：vkWaitForFences={result}");
        std::process::abort();
    }
}

fn destroy_present_fence(inner: &Inner, fence: VkFence) {
    unsafe { (inner.functions.vkDestroyFence)(inner.owner.device, fence, null()); }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        let needs_idle = matches!(self.presentation, PresentCompletion::Unfenced);
        unsafe {
            let f = &self.functions;
            let d = self.inner.owner.device;
            if matches!(self.acquisition, Acquisition::Acquired) && !self.inner.queue_lost() {
                // No copy reached the queue. Finish the outstanding WSI signal before destroying it.
                let stage = 1;
                let submit = VkSubmitInfo {
                    sType: 4,
                    waitSemaphoreCount: 1,
                    pWaitSemaphores: &self.acquired,
                    pWaitDstStageMask: &stage,
                    ..zero()
                };
                if !needs_idle {
                    let completion = self.inner.next_submission().and_then(|next| {
                        Completion::submit(self.inner.clone(), next, Some(&submit))
                    });
                    match completion {
                        Ok(completion) => completion.finish_for_drop(),
                        Err(error) if !self.inner.queue_lost() => {
                            eprintln!("窗口获取信号量无法结束：{error}");
                            std::process::abort();
                        }
                        Err(_) => {}
                    }
                } else {
                    let result = self.inner.submit_queue(0, Some(&submit));
                    if !matches!(result, 0 | -4) {
                        eprintln!("窗口获取信号量无法结束：vkQueueSubmit={result}");
                        std::process::abort();
                    }
                }
            }
            if needs_idle {
                self.inner.wait_idle_for_release();
            } else {
                if let PresentCompletion::Fenced(pending) = &mut self.presentation {
                    // The last present fence covers earlier presents to this chain on the same queue.
                    if !self.inner.queue_lost()
                        && let Some(&last) = pending.back()
                    {
                        wait_present_fence(&self.inner, last);
                    }
                    for fence in pending.drain(..) {
                        destroy_present_fence(&self.inner, fence);
                    }
                }
                if let Acquisition::Failed(Some(copy)) = &self.acquisition
                    && let Some(copy) = copy.upgrade()
                {
                    copy.point.finish_for_drop();
                }
            }
            for s in &self.rendered {
                (f.vkDestroySemaphore)(d, *s, null());
            }
            if self.acquired != 0 {
                (f.vkDestroySemaphore)(d, self.acquired, null());
            }
            if self.chain != 0 {
                (f.vkDestroySwapchainKHR)(d, self.chain, null());
            }
            if self.surface != 0 {
                (self.destroy_surface)(self.inner.owner._instance.instance, self.surface, null());
            }
        }
    }
}
