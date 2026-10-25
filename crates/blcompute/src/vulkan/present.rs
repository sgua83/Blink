use super::*;

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
type DestroySurface = unsafe extern "system" fn(VkInstance, u64, *const c_void);

pub struct Swapchain {
    extent: Extent2D,
    inner: Rc<Inner>,
    functions: PresentFunctions,
    surface: u64,
    destroy_surface: DestroySurface,
    chain: u64,
    images: Vec<u64>,
    acquired: u64,
    rendered: Vec<u64>,
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
        unsafe {
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
                extent: Extent2D { width, height },
                inner: self.inner.clone(),
                functions,
                surface: 0,
                destroy_surface,
                chain: 0,
                images: Vec::new(),
                acquired: 0,
                rendered: Vec::new(),
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
            check(
                (f.vkCreateSwapchainKHR)(device, &info, null(), &mut out.chain),
                "vkCreateSwapchainKHR",
            )?;
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
    pub fn present(&mut self, display: &GpuDataset) -> Result<()> {
        if !Rc::ptr_eq(&self.inner, &display.allocation.inner)
            || display.count != self.extent.width * self.extent.height
            || display.stride != 4
        {
            return Err(error(
                "呈现输入必须是同设备上与交换链尺寸一致的完整 BGRA8 数据",
            ));
        }
        unsafe {
            let f = &self.inner.functions;
            let d = self.inner.owner.device;
            let mut index = 0;
            check(
                (self.functions.vkAcquireNextImageKHR)(
                    d,
                    self.chain,
                    u64::MAX,
                    self.acquired,
                    0,
                    &mut index,
                ),
                "vkAcquireNextImageKHR",
            )?;
            let mut commands = Commands::new(self.inner.clone())?;
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
            let width = u64::from(self.extent.width);
            let mut pixel = 0u64;
            for page in &display.pages {
                let end = pixel + page.size / 4;
                let mut offset = 0u64;
                while pixel < end {
                    let x = pixel % width;
                    let remaining = end - pixel;
                    let (copy_width, rows) = if x == 0 && remaining >= width {
                        (width, remaining / width)
                    } else {
                        ((width - x).min(remaining), 1)
                    };
                    let copy = BufferImageCopy {
                        offset,
                        row_length: 0,
                        image_height: 0,
                        subresource: Subresource {
                            aspect: 1,
                            mip: 0,
                            base_layer: 0,
                            layers: 1,
                        },
                        image_offset: [x as i32, (pixel / width) as i32, 0],
                        extent: VkExtent3D {
                            width: copy_width as u32,
                            height: rows as u32,
                            depth: 1,
                        },
                    };
                    (f.vkCmdCopyBufferToImage)(commands.command, page.buffer, image, 7, 1, &copy);
                    let copied = copy_width * rows;
                    pixel += copied;
                    offset += copied * 4;
                }
            }
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
            commands.submit_with(&[self.acquired], &[0x1000], &[signal])?;
            let info = PresentInfo {
                kind: 1000001001,
                next: null(),
                wait_count: 1,
                wait: &signal,
                chain_count: 1,
                chains: &self.chain,
                indices: &index,
                results: null_mut(),
            };
            check(
                (self.functions.vkQueuePresentKHR)(self.inner.queue, &info),
                "vkQueuePresentKHR",
            )?;
            commands.wait()?;
        }
        Ok(())
    }
}
impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            let f = &self.functions;
            let d = self.inner.owner.device;
            let result = (f.vkDeviceWaitIdle)(d);
            if result != 0 {
                eprintln!("窗口呈现资源终止：vkDeviceWaitIdle={result}");
                if result != -4 {
                    std::process::abort();
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
