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
fn mip_copies(dimensions: [u32; 3], levels: u32, texel_bytes: u64) -> (Vec<BufferImageCopy>, u64) {
    let [mut width, mut height, mut depth] = dimensions;
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
                depth,
            },
        });
        offset += width as u64 * height as u64 * depth as u64 * texel_bytes;
        width = (width / 2).max(1);
        height = (height / 2).max(1);
        depth = (depth / 2).max(1);
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
struct ImageFormatProperties {
    extent: VkExtent3D,
    mips: u32,
    layers: u32,
    samples: u32,
    resource_bytes: u64,
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
    dimensions: [u32; 3],
    mips: u32,
    address: [crate::TextureAddress; 3],
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
// 真实图像和生产完成点由同一个共享所有者持有；执行不反向持有输出图像。
struct Image {
    storage: Storage,
    produced: Rc<Execution>,
}
impl Drop for Image {
    fn drop(&mut self) {
        self.produced.point.finish_for_drop();
    }
}
/// 一个既有二维mip在原GPU数据集中的记录布局；每条记录是一份已编码texel。
#[derive(Clone, Copy, Debug)]
pub struct TextureCopyLevel {
    pub source: usize,
    pub offset: u64,
    pub row_stride: u32,
}
#[derive(Clone)]
pub struct Texture<const D: usize = 2>(Rc<Image>);
impl<const D: usize> Texture<D> {
    pub fn format(&self) -> crate::TextureFormat {
        self.0.storage.format
    }
    pub fn address(&self) -> [crate::TextureAddress; D] {
        std::array::from_fn(|i| self.0.storage.address[i])
    }
    pub fn dimensions(&self) -> [u32; D] {
        std::array::from_fn(|i| self.0.storage.dimensions[i])
    }
    pub fn mip_count(&self) -> u32 {
        self.0.storage.mips
    }
    pub fn allocated_bytes(&self) -> u64 {
        self.0.storage.allocated_bytes
    }
    /// 只观察本张图像的生产复制，不代表后续采样消费者已经完成。
    pub fn is_complete(&self) -> Result<bool> {
        self.0.produced.is_complete()
    }
    pub fn wait(&self) -> Result<()> {
        self.0.produced.wait()
    }
    pub(super) fn produced(&self) -> Rc<Execution> {
        self.0.produced.clone()
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
/// 提交输入保留原图像所有者；不同维度不会改变同一图像的身份。
#[derive(Clone)]
pub enum TextureInput { D2(Texture), D3(Texture<3>) }
impl From<Texture> for TextureInput { fn from(value: Texture) -> Self { Self::D2(value) } }
impl From<Texture<3>> for TextureInput { fn from(value: Texture<3>) -> Self { Self::D3(value) } }
impl TextureInput {
    pub(super) fn format(&self) -> crate::TextureFormat {
        match self { Self::D2(t) => t.format(), Self::D3(t) => t.format() }
    }
    pub fn dimension(&self) -> blanguage::ast::TextureDimension {
        match self { Self::D2(_) => blanguage::ast::TextureDimension::D2, Self::D3(_) => blanguage::ast::TextureDimension::D3 }
    }
    pub(super) fn produced(&self) -> Rc<Execution> {
        match self { Self::D2(t) => t.produced(), Self::D3(t) => t.produced() }
    }
    pub(super) fn belongs_to(&self, inner: &Rc<Inner>) -> bool {
        match self { Self::D2(t) => t.belongs_to(inner), Self::D3(t) => t.belongs_to(inner) }
    }
    pub(super) fn descriptor(&self) -> DescriptorImageInfo {
        match self { Self::D2(t) => t.descriptor(), Self::D3(t) => t.descriptor() }
    }
}
impl Device {
    pub fn upload_texture(&self, source: &crate::Texture2D) -> Result<Texture> {
        let [width, height] = source.dimensions();
        let [u, v] = source.address();
        self.upload_image([width, height, 1], [u, v, crate::TextureAddress::Clamp], source.format(),
            source.levels.iter().map(|m| &m.pixels))
    }
    pub fn upload_texture3d(&self, source: &crate::Texture3D) -> Result<Texture<3>> {
        self.upload_image(source.dimensions(), source.address(), source.format(),
            source.levels.iter().map(|m| &m.pixels))
    }
    /// 从同设备原GPU记录直接复制已有mip；记录stride就是format的真实texel字节数。
    /// 只在提交成功后更新源last_use，返回时不等待设备复制。
    pub fn copy_texture(&self, sources: &mut [GpuDataset], dimensions: [u32; 2],
        format: crate::TextureFormat, address: [crate::TextureAddress; 2],
        levels: &[TextureCopyLevel]) -> Result<Texture> {
        if dimensions.contains(&0) || levels.is_empty()
            || levels.len() > (32 - dimensions[0].max(dimensions[1]).leading_zeros()) as usize {
            return Err(error("纹理尺寸和mip链必须是非空的合法二维链"));
        }
        let mut size = dimensions;
        for level in levels {
            let source = sources.get(level.source).ok_or_else(|| error("纹理mip的GPU源下标越界"))?;
            if !Rc::ptr_eq(&source.allocation.inner, &self.inner) {
                return Err(error("纹理GPU源属于另一设备"));
            }
            if source.stride != format.texel_bytes() as u32 {
                return Err(error("纹理GPU源记录stride必须等于实际texel字节数"));
            }
            if level.row_stride < size[0] {
                return Err(error("纹理GPU源行步长小于本级宽度"));
            }
            let length = u64::from(size[1] - 1) * u64::from(level.row_stride) + u64::from(size[0]);
            if level.offset.checked_add(length).is_none_or(|end| end > u64::from(source.count)) {
                return Err(error("纹理mip超过原GPU源记录范围"));
            }
            size = size.map(|n| (n / 2).max(1));
        }
        let dependencies = Execution::prepare(levels.iter()
            .filter_map(|level| sources[level.source].dependency().cloned()).collect())?;
        let texture = self.create_image::<2>([dimensions[0], dimensions[1], 1],
            [address[0], address[1], crate::TextureAddress::Clamp], format, levels.len() as u32)?;
        let texture = self.write_image(texture, Vec::new(), dependencies, |commands, texture| {
            let source_stage = levels.iter().fold(0, |stages, level| stages | sources[level.source].producer.stage());
            let source_access = levels.iter().fold(0, |access, level| access | sources[level.source].producer.access());
            commands.barrier(source_stage, 0x1000, source_access, 0x800);
            let mut size = dimensions;
            for (mip, level) in levels.iter().enumerate() {
                let source = &sources[level.source];
                let page_records = u64::from(source.page_records());
                let mut row = 0;
                let mut column = 0;
                while row < size[1] {
                    let pixel = level.offset + u64::from(row) * u64::from(level.row_stride) + u64::from(column);
                    let page = (pixel / page_records) as usize;
                    let offset = pixel % page_records;
                    let available = page_records.min(u64::from(source.count) - page as u64 * page_records) - offset;
                    let (width, rows) = if column == 0 && available >= u64::from(size[0]) {
                        (size[0], u64::from(size[1] - row).min(
                            (available - u64::from(size[0])) / u64::from(level.row_stride) + 1) as u32)
                    } else {
                        (u64::from(size[0] - column).min(available) as u32, 1)
                    };
                    let copy = BufferImageCopy {
                        offset: offset * u64::from(source.stride),
                        row_length: if rows > 1 { level.row_stride } else { 0 },
                        image_height: 0,
                        subresource: Subresource { aspect: 1, mip: mip as u32, base_layer: 0, layers: 1 },
                        image_offset: [column as i32, row as i32, 0],
                        extent: VkExtent3D { width, height: rows, depth: 1 },
                    };
                    unsafe {
                        (self.inner.functions.vkCmdCopyBufferToImage)(
                            commands.command, source.pages[page].buffer, texture.image, 7, 1, &copy,
                        );
                    }
                    column += width;
                    if column == size[0] { column = 0; row += rows; }
                }
                size = size.map(|n| (n / 2).max(1));
            }
            // 原producer仍表示最后写；本次真实Transfer读必须先于后继源写。
            commands.barrier(0x1000, 0x1800, 0x800, 0x1040);
        })?;
        for level in levels {
            sources[level.source].last_use = Some(texture.produced());
        }
        Ok(texture)
    }
    fn upload_image<'a, const D: usize>(&self, dimensions: [u32; 3], address: [crate::TextureAddress; 3],
        format: crate::TextureFormat, levels: impl ExactSizeIterator<Item = &'a crate::TextureTexels>) -> Result<Texture<D>> {
        let texture = self.create_image::<D>(dimensions, address, format, levels.len() as u32)?;
        let (copies, bytes) = mip_copies(dimensions, texture.mips, format.texel_bytes() as u64);
        let staging = Allocation::new(self.inner.clone(), bytes, 1, 6, 1)?;
        let mapped = staging.mapping()?;
        for (pixels, copy) in levels.zip(&copies) {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    pixels.as_bytes().as_ptr(), mapped.cast::<u8>().add(copy.offset as usize),
                    pixels.as_bytes().len(),
                );
            }
        }
        let buffer = staging.buffer;
        self.write_image(texture, vec![staging], Vec::new(), |commands, texture| {
            unsafe {
                (self.inner.functions.vkCmdCopyBufferToImage)(
                    commands.command, buffer, texture.image, 7, copies.len() as u32, copies.as_ptr(),
                );
            }
        })
    }
    fn create_image<const D: usize>(&self, dimensions: [u32; 3], address: [crate::TextureAddress; 3],
        format: crate::TextureFormat, mips: u32) -> Result<Storage> {
        let inner = self.inner.clone();
        let native_format = match format {
            crate::TextureFormat::Rgba32Float => 109,
            crate::TextureFormat::Rgba8Unorm => 37,
            crate::TextureFormat::Rgba8Srgb => 43,
            crate::TextureFormat::R8Unorm => 9,
            crate::TextureFormat::Rg16Unorm => 77,
            crate::TextureFormat::Rgb10A2Unorm => 64,
            crate::TextureFormat::R11G11B10Float => 122,
        };
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
            let image_properties: unsafe extern "system" fn(
                VkPhysicalDevice, u32, u32, u32, u32, u32, *mut ImageFormatProperties,
            ) -> i32 = instance.library.get(instance.instance, c"vkGetPhysicalDeviceImageFormatProperties")?;
            let mut support: ImageFormatProperties = zero();
            check(image_properties(inner.physical, native_format, D as u32 - 1, 0, 7, 0, &mut support),
                "vkGetPhysicalDeviceImageFormatProperties")?;
            let maximum = [support.extent.width, support.extent.height, support.extent.depth];
            if dimensions.iter().zip(maximum).any(|(n, max)| *n > max)
                || mips > support.mips || support.samples & 1 == 0
            {
                return Err(error(format!("纹理尺寸、mip或采样数超出设备 image{D}D 格式能力")));
            }
            let mut texture = Storage {
                inner: inner.clone(),
                image: 0,
                memory: 0,
                view: 0,
                sampler: 0,
                allocated_bytes: 0,
                dimensions,
                mips,
                address,
                format,
            };
            let f = &inner.functions;
            let device = inner.owner.device;
            let mut ci: ImageCreateInfo = zero();
            ci.kind = 14;
            ci.image_type = D as u32 - 1;
            ci.format = native_format;
            ci.extent = VkExtent3D {
                width: dimensions[0],
                height: dimensions[1],
                depth: dimensions[2],
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
                view_type: D as u32 - 1,
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
                address_u: match texture.address[0] {
                    crate::TextureAddress::Clamp => 2,
                    crate::TextureAddress::Repeat => 0,
                },
                address_v: match texture.address[1] {
                    crate::TextureAddress::Clamp => 2,
                    crate::TextureAddress::Repeat => 0,
                },
                address_w: match texture.address[2] {
                    crate::TextureAddress::Clamp => 2,
                    crate::TextureAddress::Repeat => 0,
                },
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
            Ok(texture)
        }
    }
    fn write_image<const D: usize>(&self, texture: Storage, staging: Vec<Allocation>,
        dependencies: Vec<Rc<Execution>>, record: impl FnOnce(&Commands, &Storage)) -> Result<Texture<D>> {
        let f = &self.inner.functions;
        let commands = Commands::new(self.inner.clone())?;
        unsafe {
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
        }
        record(&commands, &texture);
        unsafe {
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
        }
        let produced = self.submit_transfer(commands, staging, dependencies)?;
        Ok(Texture(Rc::new(Image { storage: texture, produced })))
    }
    /// 显式同步读回真实图像的全部 mip；返回独立 CPU 存储并恢复采样布局。
    pub fn download_texture(&self, source: &Texture) -> Result<crate::Texture2D> {
        let levels = self.download_image(source, |[width, height, _], pixels| crate::TextureMip { width, height, pixels })?;
        Ok(crate::Texture2D { levels: levels.into(), address: source.address() })
    }
    /// 显式同步读回同一原生三维图像的全部体mip，恢复原采样布局。
    pub fn download_texture3d(&self, source: &Texture<3>) -> Result<crate::Texture3D> {
        let levels = self.download_image(source, |[width, height, depth], pixels| crate::TextureMip3D { width, height, depth, pixels })?;
        Ok(crate::Texture3D { levels: levels.into(), address: source.address() })
    }
    fn download_image<const D: usize, M>(&self, source: &Texture<D>,
        mut mip: impl FnMut([u32; 3], crate::TextureTexels) -> M) -> Result<Vec<M>> {
        if !source.belongs_to(&self.inner) {
            return Err(error("纹理属于另一设备"));
        }
        let texture = &source.0.storage;
        let (copies, bytes) = mip_copies(
            texture.dimensions,
            texture.mips,
            texture.format.texel_bytes() as u64,
        );
        let target = Allocation::new(self.inner.clone(), bytes, 2, 6, 1)?;
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
                let depth = copy.extent.depth;
                let count = width as usize * height as usize * depth as usize * texture.format.texel_bytes();
                let bytes =
                    unsafe { std::slice::from_raw_parts(mapped.add(copy.offset as usize), count) };
                let pixels = crate::TextureTexels::from_bytes(texture.format, bytes)?;
                levels.push(mip([width, height, depth], pixels));
            }
            Ok(())
        })??;
        Ok(levels)
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
