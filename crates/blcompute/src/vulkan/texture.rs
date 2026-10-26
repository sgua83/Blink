//! 不可变 sampled image；实际命令持有纹理直到原生完成。
use super::*;
#[repr(C)]
pub(super) struct IndexingFeatures {
    pub(super) kind: u32,
    pub(super) next: *mut c_void,
    pub(super) capabilities: [u32; 20],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct Subresource {
    pub(super) aspect: u32,
    pub(super) mip: u32,
    pub(super) base_layer: u32,
    pub(super) layers: u32,
}
#[repr(C)]
pub(super) struct BufferImageCopy {
    pub(super) offset: u64,
    pub(super) row_length: u32,
    pub(super) image_height: u32,
    pub(super) subresource: Subresource,
    pub(super) image_offset: [i32; 3],
    pub(super) extent: VkExtent3D,
}
fn mip_copies(dimensions: [u32; 2], levels: u32, texel_bytes: u64) -> (Vec<BufferImageCopy>, u64) {
    let [mut width, mut height] = dimensions;
    let mut offset = 0u64;
    let mut copies = Vec::with_capacity(levels as usize);
    for mip in 0..levels {
        offset = (offset + 3) & !3;
        copies.push(BufferImageCopy {
            offset,
            row_length: 0,
            image_height: 0,
            subresource: Subresource {
                aspect: 1,
                mip,
                base_layer: 0,
                layers: 1,
            },
            image_offset: [0; 3],
            extent: VkExtent3D {
                width,
                height,
                depth: 1,
            },
        });
        offset += width as u64 * height as u64 * texel_bytes;
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    (copies, offset)
}
#[repr(C)]
pub(super) struct SubresourceRange {
    pub(super) aspect: u32,
    pub(super) base_mip: u32,
    pub(super) levels: u32,
    pub(super) base_layer: u32,
    pub(super) layers: u32,
}
#[repr(C)]
pub(super) struct ImageBarrier {
    pub(super) kind: u32,
    pub(super) next: *const c_void,
    pub(super) source_access: u32,
    pub(super) target_access: u32,
    pub(super) old_layout: u32,
    pub(super) new_layout: u32,
    pub(super) source_family: u32,
    pub(super) target_family: u32,
    pub(super) image: u64,
    pub(super) range: SubresourceRange,
}

#[repr(C)]
pub(super) struct ImageCreateInfo {
    kind: u32,
    next: *const c_void,
    flags: u32,
    image_type: u32,
    format: u32,
    extent: VkExtent3D,
    mips: u32,
    layers: u32,
    samples: u32,
    tiling: u32,
    usage: u32,
    sharing: u32,
    family_count: u32,
    families: *const u32,
    initial_layout: u32,
}
#[repr(C)]
pub(super) struct ImageViewCreateInfo {
    kind: u32,
    next: *const c_void,
    flags: u32,
    image: u64,
    view_type: u32,
    format: u32,
    components: [u32; 4],
    range: SubresourceRange,
}
#[repr(C)]
pub(super) struct SamplerCreateInfo {
    kind: u32,
    next: *const c_void,
    flags: u32,
    mag_filter: u32,
    min_filter: u32,
    mipmap_mode: u32,
    address_u: u32,
    address_v: u32,
    address_w: u32,
    mip_bias: f32,
    anisotropy: u32,
    max_anisotropy: f32,
    compare: u32,
    compare_op: u32,
    min_lod: f32,
    max_lod: f32,
    border: u32,
    unnormalized: u32,
}
#[repr(C)]
struct FormatProperties {
    linear: u32,
    optimal: u32,
    buffer: u32,
}
#[repr(C)]
pub(super) struct DescriptorImageInfo {
    sampler: u64,
    view: u64,
    layout: u32,
}
struct Storage {
    inner: Rc<Inner>,
    image: u64,
    memory: u64,
    view: u64,
    sampler: u64,
    allocated_bytes: u64,
    dimensions: [u32; 2],
    mips: u32,
    address: [crate::TextureAddress; 2],
    format: crate::TextureFormat,
}
impl Storage {
    fn range(&self) -> SubresourceRange {
        SubresourceRange {
            aspect: 1,
            base_mip: 0,
            levels: self.mips,
            base_layer: 0,
            layers: 1,
        }
    }
}
// 真实图像和上传完成点由同一个共享所有者持有；执行不反向持有输出图像。
struct Image {
    storage: Storage,
    uploaded: Rc<Execution>,
}
impl Drop for Image {
    fn drop(&mut self) {
        self.uploaded.point.finish_for_drop();
    }
}
#[derive(Clone)]
pub struct Texture(Rc<Image>);
impl Texture {
    pub fn format(&self) -> crate::TextureFormat {
        self.0.storage.format
    }
    pub fn address(&self) -> [crate::TextureAddress; 2] {
        self.0.storage.address
    }
    pub fn dimensions(&self) -> [u32; 2] {
        self.0.storage.dimensions
    }
    pub fn mip_count(&self) -> u32 {
        self.0.storage.mips
    }
    pub fn allocated_bytes(&self) -> u64 {
        self.0.storage.allocated_bytes
    }
    /// 只观察本张图像的上传，不代表后续采样消费者已经完成。
    pub fn is_complete(&self) -> Result<bool> {
        self.0.uploaded.is_complete()
    }
    pub fn wait(&self) -> Result<()> {
        self.0.uploaded.wait()
    }
    pub(super) fn produced(&self) -> Rc<Execution> {
        self.0.uploaded.clone()
    }
    pub(super) fn belongs_to(&self, inner: &Rc<Inner>) -> bool {
        Rc::ptr_eq(&self.0.storage.inner, inner)
    }
    pub(super) fn descriptor(&self) -> DescriptorImageInfo {
        DescriptorImageInfo {
            sampler: self.0.storage.sampler,
            view: self.0.storage.view,
            layout: 5,
        }
    }
}
impl Device {
    pub fn upload_texture(&self, source: &crate::Texture2D) -> Result<Texture> {
        let inner = self.inner.clone();
        let dimensions = source.dimensions();
        let format = source.format();
        let native_format = match format {
            crate::TextureFormat::Rgba32Float => 109,
            crate::TextureFormat::Rgba8Unorm => 37,
            crate::TextureFormat::Rgba8Srgb => 43,
            crate::TextureFormat::R8Unorm => 9,
        };
        if dimensions
            .iter()
            .any(|n| *n > inner.properties.limits.maxImageDimension2D)
        {
            return Err(error("纹理尺寸超出设备 image2D 能力"));
        }
        unsafe {
            let instance = &inner.owner._instance;
            let properties: unsafe extern "system" fn(
                VkPhysicalDevice,
                u32,
                *mut FormatProperties,
            ) = instance
                .library
                .get(instance.instance, c"vkGetPhysicalDeviceFormatProperties")?;
            let mut support: FormatProperties = zero();
            properties(inner.physical, native_format, &mut support);
            if support.optimal & 0xd001 != 0xd001 {
                return Err(error(format!(
                    "设备不支持{format:?} sampled image的线性过滤与双向传输"
                )));
            }
            let mut texture = Storage {
                inner: inner.clone(),
                image: 0,
                memory: 0,
                view: 0,
                sampler: 0,
                allocated_bytes: 0,
                dimensions,
                mips: source.mip_count(),
                address: source.address(),
                format,
            };
            let f = &inner.functions;
            let device = inner.owner.device;
            let mut ci: ImageCreateInfo = zero();
            ci.kind = 14;
            ci.image_type = 1;
            ci.format = native_format;
            ci.extent = VkExtent3D {
                width: dimensions[0],
                height: dimensions[1],
                depth: 1,
            };
            ci.mips = texture.mips;
            ci.layers = 1;
            ci.samples = 1;
            ci.usage = 7;
            check(
                (f.vkCreateImage)(device, &ci, null(), &mut texture.image),
                "vkCreateImage",
            )?;
            let mut required: VkMemoryRequirements = zero();
            (f.vkGetImageMemoryRequirements)(device, texture.image, &mut required);
            texture.allocated_bytes = required.size;
            let index = (0..inner.memory.memoryTypeCount)
                .find(|i| {
                    required.memoryTypeBits & (1 << i) != 0
                        && inner.memory.memoryTypes[*i as usize].propertyFlags & 1 != 0
                })
                .ok_or_else(|| error("纹理缺少 device-local 内存类型"))?;
            let ai = VkMemoryAllocateInfo {
                sType: 5,
                pNext: null(),
                allocationSize: required.size,
                memoryTypeIndex: index,
            };
            check(
                (f.vkAllocateMemory)(device, &ai, null(), &mut texture.memory),
                "vkAllocateMemory(image)",
            )?;
            check(
                (f.vkBindImageMemory)(device, texture.image, texture.memory, 0),
                "vkBindImageMemory",
            )?;
            let vi = ImageViewCreateInfo {
                kind: 15,
                next: null(),
                flags: 0,
                image: texture.image,
                view_type: 1,
                format: native_format,
                components: [0; 4],
                range: texture.range(),
            };
            check(
                (f.vkCreateImageView)(device, &vi, null(), &mut texture.view),
                "vkCreateImageView",
            )?;
            let si = SamplerCreateInfo {
                kind: 31,
                next: null(),
                flags: 0,
                mag_filter: 1,
                min_filter: 1,
                mipmap_mode: 1,
                address_u: match source.address[0] {
                    crate::TextureAddress::Clamp => 2,
                    crate::TextureAddress::Repeat => 0,
                },
                address_v: match source.address[1] {
                    crate::TextureAddress::Clamp => 2,
                    crate::TextureAddress::Repeat => 0,
                },
                address_w: 2,
                mip_bias: 0.0,
                anisotropy: 0,
                max_anisotropy: 1.0,
                compare: 0,
                compare_op: 7,
                min_lod: 0.0,
                max_lod: (texture.mips - 1) as f32,
                border: 0,
                unnormalized: 0,
            };
            check(
                (f.vkCreateSampler)(device, &si, null(), &mut texture.sampler),
                "vkCreateSampler",
            )?;
            let (copies, bytes) = mip_copies(dimensions, texture.mips, format.texel_bytes() as u64);
            let staging = Allocation::new(inner.clone(), bytes, 1, 6)?;
            let mut mapped = null_mut();
            check(
                (f.vkMapMemory)(device, staging.memory, 0, bytes, 0, &mut mapped),
                "vkMapMemory(texture)",
            )?;
            for (mip, copy) in source.levels.iter().zip(&copies) {
                std::ptr::copy_nonoverlapping(
                    mip.pixels.as_bytes().as_ptr(),
                    mapped.cast::<u8>().add(copy.offset as usize),
                    mip.pixels.as_bytes().len(),
                );
            }
            (f.vkUnmapMemory)(device, staging.memory);
            let commands = Commands::new(inner.clone())?;
            let barrier = ImageBarrier {
                kind: 45,
                next: null(),
                source_access: 0,
                target_access: 0x1000,
                old_layout: 0,
                new_layout: 7,
                source_family: u32::MAX,
                target_family: u32::MAX,
                image: texture.image,
                range: texture.range(),
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
            (f.vkCmdCopyBufferToImage)(
                commands.command,
                staging.buffer,
                texture.image,
                7,
                copies.len() as u32,
                copies.as_ptr(),
            );
            let barrier = ImageBarrier {
                kind: 45,
                next: null(),
                source_access: 0x1000,
                target_access: 0x20,
                old_layout: 7,
                new_layout: 5,
                source_family: u32::MAX,
                target_family: u32::MAX,
                image: texture.image,
                range: texture.range(),
            };
            (f.vkCmdPipelineBarrier)(
                commands.command,
                0x1000,
                0x800,
                0,
                0,
                null(),
                0,
                null(),
                1,
                (&barrier as *const ImageBarrier).cast(),
            );
            let uploaded = self.submit_transfer(commands, vec![staging], Vec::new())?;
            Ok(Texture(Rc::new(Image {
                storage: texture,
                uploaded,
            })))
        }
    }
    /// 显式同步读回真实图像的全部 mip；返回独立 CPU 存储并恢复采样布局。
    pub fn download_texture(&self, source: &Texture) -> Result<crate::Texture2D> {
        if !source.belongs_to(&self.inner) {
            return Err(error("纹理属于另一设备"));
        }
        let texture = &source.0.storage;
        let (copies, bytes) = mip_copies(
            texture.dimensions,
            texture.mips,
            texture.format.texel_bytes() as u64,
        );
        let target = Allocation::new(self.inner.clone(), bytes, 2, 6)?;
        let commands = Commands::new(self.inner.clone())?;
        unsafe {
            // 布局转换覆盖本队列此前的上传与采样，复制后恢复采样布局。
            let barrier = ImageBarrier {
                kind: 45,
                next: null(),
                source_access: 0x18000,
                target_access: 0x800,
                old_layout: 5,
                new_layout: 6,
                source_family: u32::MAX,
                target_family: u32::MAX,
                image: texture.image,
                range: texture.range(),
            };
            (self.inner.functions.vkCmdPipelineBarrier)(
                commands.command,
                0x10000,
                0x1000,
                0,
                0,
                null(),
                0,
                null(),
                1,
                (&barrier as *const ImageBarrier).cast(),
            );
            (self.inner.functions.vkCmdCopyImageToBuffer)(
                commands.command,
                texture.image,
                6,
                target.buffer,
                copies.len() as u32,
                copies.as_ptr(),
            );
            let barrier = ImageBarrier {
                source_access: 0x800,
                target_access: 0x20,
                old_layout: 6,
                new_layout: 5,
                ..barrier
            };
            (self.inner.functions.vkCmdPipelineBarrier)(
                commands.command,
                0x1000,
                0x800,
                0,
                0,
                null(),
                0,
                null(),
                1,
                (&barrier as *const ImageBarrier).cast(),
            );
        }
        commands.barrier(0x1000, 0x4000, 0x1000, 0x2000);
        let dependencies = Execution::prepare(vec![source.produced()])?;
        self.submit_transfer(commands, Vec::new(), dependencies)?
            .wait()?;
        let mut levels = Vec::with_capacity(copies.len());
        target.read_with(|mapped| -> Result<()> {
            for copy in &copies {
                let width = copy.extent.width;
                let height = copy.extent.height;
                let count = width as usize * height as usize * texture.format.texel_bytes();
                let bytes =
                    unsafe { std::slice::from_raw_parts(mapped.add(copy.offset as usize), count) };
                let pixels = crate::TextureTexels::from_bytes(texture.format, bytes)?;
                levels.push(crate::TextureMip {
                    width,
                    height,
                    pixels,
                });
            }
            Ok(())
        })??;
        Ok(crate::Texture2D {
            levels: levels.into(),
            address: source.address(),
        })
    }
}
impl Drop for Storage {
    fn drop(&mut self) {
        unsafe {
            let f = &self.inner.functions;
            let device = self.inner.owner.device;
            if self.sampler != 0 {
                (f.vkDestroySampler)(device, self.sampler, null());
            }
            if self.view != 0 {
                (f.vkDestroyImageView)(device, self.view, null());
            }
            if self.image != 0 {
                (f.vkDestroyImage)(device, self.image, null());
            }
            if self.memory != 0 {
                (f.vkFreeMemory)(device, self.memory, null());
            }
        }
    }
}
