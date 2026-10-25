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
}
#[derive(Clone)]
pub struct Texture(Rc<Storage>);
impl Texture {
    pub fn dimensions(&self) -> [u32; 2] {
        self.0.dimensions
    }
    pub fn mip_count(&self) -> u32 {
        self.0.mips
    }
    pub fn allocated_bytes(&self) -> u64 {
        self.0.allocated_bytes
    }
    pub(super) fn belongs_to(&self, inner: &Rc<Inner>) -> bool {
        Rc::ptr_eq(&self.0.inner, inner)
    }
    pub(super) fn descriptor(&self) -> DescriptorImageInfo {
        DescriptorImageInfo {
            sampler: self.0.sampler,
            view: self.0.view,
            layout: 5,
        }
    }
}
impl Device {
    pub fn upload_texture(&self, source: &crate::Texture2D) -> Result<Texture> {
        let inner = self.inner.clone();
        let dimensions = source.dimensions();
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
            properties(inner.physical, 109, &mut support);
            if support.optimal & 0x9001 != 0x9001 {
                return Err(error("设备不支持 RGBA32F sampled image 的线性过滤与传输"));
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
            };
            let f = &inner.functions;
            let device = inner.owner.device;
            let mut ci: ImageCreateInfo = zero();
            ci.kind = 14;
            ci.image_type = 1;
            ci.format = 109;
            ci.extent = VkExtent3D {
                width: dimensions[0],
                height: dimensions[1],
                depth: 1,
            };
            ci.mips = texture.mips;
            ci.layers = 1;
            ci.samples = 1;
            ci.usage = 6;
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
            let range = || SubresourceRange {
                aspect: 1,
                base_mip: 0,
                levels: texture.mips,
                base_layer: 0,
                layers: 1,
            };
            let vi = ImageViewCreateInfo {
                kind: 15,
                next: null(),
                flags: 0,
                image: texture.image,
                view_type: 1,
                format: 109,
                components: [0; 4],
                range: range(),
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
                address_u: 2,
                address_v: 2,
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
            let bytes = source
                .levels
                .iter()
                .map(|m| m.pixels.len() as u64 * 16)
                .sum();
            let staging = Allocation::new(inner.clone(), bytes, 1, 6)?;
            let mut mapped = null_mut();
            check(
                (f.vkMapMemory)(device, staging.memory, 0, bytes, 0, &mut mapped),
                "vkMapMemory(texture)",
            )?;
            let mut offset = 0usize;
            let mut copies = Vec::new();
            for (level, mip) in source.levels.iter().enumerate() {
                std::ptr::copy_nonoverlapping(
                    mip.pixels.as_ptr().cast::<u8>(),
                    mapped.cast::<u8>().add(offset),
                    mip.pixels.len() * 16,
                );
                copies.push(BufferImageCopy {
                    offset: offset as u64,
                    row_length: 0,
                    image_height: 0,
                    subresource: Subresource {
                        aspect: 1,
                        mip: level as u32,
                        base_layer: 0,
                        layers: 1,
                    },
                    image_offset: [0; 3],
                    extent: VkExtent3D {
                        width: mip.width,
                        height: mip.height,
                        depth: 1,
                    },
                });
                offset += mip.pixels.len() * 16;
            }
            (f.vkUnmapMemory)(device, staging.memory);
            let mut commands = Commands::new(inner.clone())?;
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
                range: range(),
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
            commands.submit()?;
            commands.completion.wait()?;
            Ok(Texture(Rc::new(texture)))
        }
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
