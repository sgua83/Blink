//! 不可变有类型纹理与独立资产；CPU描述只借用实际texel存储，不生成解码镜像。
use crate::{Result, error};
use std::{io::Write, path::Path, sync::Arc};
pub use blanguage::ast::TextureFormat;
fn read_format(value: u32) -> Result<TextureFormat> {
    TextureFormat::from_code(value).ok_or_else(|| error("未知BLTEX texel格式"))
}
pub enum TextureTexels {
    Rgba32Float(Vec<[f32; 4]>),
    Rgba8Unorm(Vec<[u8; 4]>),
    Rgba8Srgb(Vec<[u8; 4]>),
    R8Unorm(Vec<u8>),
    Rg16Unorm(Vec<[u16; 2]>),
    Rgb10A2Unorm(Vec<u32>),
}
impl From<Vec<[f32; 4]>> for TextureTexels {
    fn from(value: Vec<[f32; 4]>) -> Self {
        Self::Rgba32Float(value)
    }
}
impl TextureTexels {
    pub fn format(&self) -> TextureFormat {
        match self {
            Self::Rgba32Float(_) => TextureFormat::Rgba32Float,
            Self::Rgba8Unorm(_) => TextureFormat::Rgba8Unorm,
            Self::Rgba8Srgb(_) => TextureFormat::Rgba8Srgb,
            Self::R8Unorm(_) => TextureFormat::R8Unorm,
            Self::Rg16Unorm(_) => TextureFormat::Rg16Unorm,
            Self::Rgb10A2Unorm(_) => TextureFormat::Rgb10A2Unorm,
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        let (pixels, bytes): (*const u8, usize) = match self {
            Self::Rgba32Float(v) => (v.as_ptr().cast(), v.len() * 16),
            Self::Rgba8Unorm(v) | Self::Rgba8Srgb(v) => (v.as_ptr().cast(), v.len() * 4),
            Self::R8Unorm(v) => (v.as_ptr(), v.len()),
            Self::Rg16Unorm(v) => (v.as_ptr().cast(), v.len() * 4),
            Self::Rgb10A2Unorm(v) => (v.as_ptr().cast(), v.len() * 4),
        };
        unsafe { std::slice::from_raw_parts(pixels, bytes) }
    }
    pub fn from_bytes(format: TextureFormat, bytes: &[u8]) -> Result<Self> {
        if !bytes.len().is_multiple_of(format.texel_bytes()) {
            return Err(error("texel负载不满足格式字节布局"));
        }
        // 调用类型均为每种bit模式有效的数值数组；保留NaN载荷与所有原始位。
        fn copy<T: Copy>(bytes: &[u8]) -> Vec<T> {
            let count = bytes.len() / std::mem::size_of::<T>();
            let mut data = Vec::<T>::with_capacity(count);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    data.as_mut_ptr().cast(),
                    bytes.len(),
                );
                data.set_len(count);
            }
            data
        }
        Ok(match format {
            TextureFormat::Rgba32Float => Self::Rgba32Float(copy(bytes)),
            TextureFormat::Rgba8Unorm => Self::Rgba8Unorm(copy(bytes)),
            TextureFormat::Rgba8Srgb => Self::Rgba8Srgb(copy(bytes)),
            TextureFormat::R8Unorm => Self::R8Unorm(bytes.to_vec()),
            TextureFormat::Rg16Unorm => Self::Rg16Unorm(copy(bytes)),
            TextureFormat::Rgb10A2Unorm => Self::Rgb10A2Unorm(copy(bytes)),
        })
    }
}
pub struct TextureMip {
    pub width: u32,
    pub height: u32,
    pub pixels: TextureTexels,
}
#[derive(Clone)]
pub struct Texture2D {
    pub(crate) levels: Arc<[TextureMip]>,
    pub(crate) address: [TextureAddress; 2],
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextureAddress {
    #[default]
    Clamp,
    Repeat,
}
/// 原CPU资源的拥有输入；变体保留原不可变像素所有者。
#[derive(Clone)]
pub enum TextureInput {
    D2(Texture2D),
    D3(Texture3D),
}
impl From<Texture2D> for TextureInput { fn from(value: Texture2D) -> Self { Self::D2(value) } }
impl From<Texture3D> for TextureInput { fn from(value: Texture3D) -> Self { Self::D3(value) } }
impl TextureInput {
    pub fn dimension(&self) -> blanguage::ast::TextureDimension {
        match self { Self::D2(_) => blanguage::ast::TextureDimension::D2, Self::D3(_) => blanguage::ast::TextureDimension::D3 }
    }
    pub(crate) fn format(&self) -> TextureFormat {
        match self { Self::D2(t) => t.format(), Self::D3(t) => t.format() }
    }
    fn address_bits(&self) -> u32 {
        let address = match self { Self::D2(t) => &t.address[..], Self::D3(t) => &t.address[..] };
        address.iter().enumerate().fold(0, |bits, (axis, mode)| bits | (u32::from(*mode == TextureAddress::Repeat) << axis))
    }
}
#[repr(C)]
pub(crate) struct LevelDescriptor {
    width: u32,
    height: u32,
    depth: u32,
    pixels: *const u8,
}
#[repr(C)]
pub(crate) struct TextureDescriptor {
    levels: u32,
    address: u32,
    mips: *const LevelDescriptor,
    format: u32,
}
pub(crate) struct CpuTextures {
    _levels: Vec<Box<[LevelDescriptor]>>,
    descriptors: Vec<TextureDescriptor>,
}
impl CpuTextures {
    pub fn new(textures: &[TextureInput]) -> Self {
        let levels: Vec<Box<[LevelDescriptor]>> = textures.iter().map(|t| match t {
            TextureInput::D2(t) => t.levels.iter().map(|m| LevelDescriptor {
                width: m.width, height: m.height, depth: 1, pixels: m.pixels.as_bytes().as_ptr(),
            }).collect(),
            TextureInput::D3(t) => t.levels.iter().map(|m| LevelDescriptor {
                width: m.width, height: m.height, depth: m.depth, pixels: m.pixels.as_bytes().as_ptr(),
            }).collect(),
        }).collect();
        let descriptors = levels.iter().zip(textures).map(|(m, t)| TextureDescriptor {
            levels: m.len() as u32, address: t.address_bits(), mips: m.as_ptr(), format: t.format() as u32,
        }).collect();
        Self { _levels: levels, descriptors }
    }
    pub fn pointers(&self) -> impl Iterator<Item = *mut u32> {
        self.descriptors.iter().map(|d| (d as *const TextureDescriptor).cast_mut().cast())
    }
}
impl Texture2D {
    pub fn new(levels: Vec<TextureMip>) -> Result<Self> {
        if levels.is_empty() {
            return Err(error("纹理至少需要一个mip层"));
        }
        let format = levels[0].pixels.format();
        let mut expected = None;
        for level in &levels {
            if level.width == 0
                || level.height == 0
                || level.pixels.format() != format
                || level.pixels.as_bytes().len()
                    != texture_bytes([level.width, level.height, 1], format)?
            {
                return Err(error("纹理尺寸、格式或负载不满足原生采样ABI"));
            }
            if let Some((w, h)) = expected {
                if (level.width, level.height) != (w, h) {
                    return Err(error("mip尺寸必须逐级减半并至少保留一个texel"));
                }
            }
            expected = Some(((level.width / 2).max(1), (level.height / 2).max(1)));
        }
        let maximum = 32 - levels[0].width.max(levels[0].height).leading_zeros();
        if levels.len() > maximum as usize {
            return Err(error("mip层数超过基础尺寸的完整链"));
        }
        Ok(Self {
            levels: levels.into(),
            address: [TextureAddress::Clamp; 2],
        })
    }
    pub fn with_address(mut self, u: TextureAddress, v: TextureAddress) -> Self {
        self.address = [u, v];
        self
    }
    pub fn address(&self) -> [TextureAddress; 2] {
        self.address
    }
    pub fn format(&self) -> TextureFormat {
        self.levels[0].pixels.format()
    }
    pub fn dimensions(&self) -> [u32; 2] {
        [self.levels[0].width, self.levels[0].height]
    }
    pub fn mip_count(&self) -> u32 {
        self.levels.len() as u32
    }
    pub fn mip(&self, level: usize) -> Option<&TextureMip> {
        self.levels.get(level)
    }
    pub fn write(&self, path: &Path) -> Result<()> {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        f.write_all(b"BLTEX\0\0\0")?;
        f.write_all(&2u32.to_le_bytes())?;
        f.write_all(&(self.format() as u32).to_le_bytes())?;
        f.write_all(&self.mip_count().to_le_bytes())?;
        for mip in self.levels.iter() {
            f.write_all(&mip.width.to_le_bytes())?;
            f.write_all(&mip.height.to_le_bytes())?;
            f.write_all(mip.pixels.as_bytes())?;
        }
        f.flush()?;
        Ok(())
    }
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let mut at = 0usize;
        if take(&bytes, &mut at, 8)? != b"BLTEX\0\0\0" {
            return Err(error("不是BLTEX资产"));
        }
        let format = match word(&bytes, &mut at)? {
            1 => TextureFormat::Rgba32Float,
            2 => read_format(word(&bytes, &mut at)?)?,
            _ => return Err(error("不支持的BLTEX版本")),
        };
        let count = word(&bytes, &mut at)?;
        let mut levels = Vec::new();
        for _ in 0..count {
            let width = word(&bytes, &mut at)?;
            let height = word(&bytes, &mut at)?;
            let size = texture_bytes([width, height, 1], format)?;
            let data = take(&bytes, &mut at, size)?;
            levels.push(TextureMip {
                width,
                height,
                pixels: TextureTexels::from_bytes(format, data)?,
            });
        }
        if at != bytes.len() {
            return Err(error("纹理文件有未声明的尾部数据"));
        }
        Self::new(levels)
    }
}

// 两种维度共用同一资产字节读取；边界错误直接交回原调用者。
fn take<'a>(bytes: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = at.checked_add(n).ok_or_else(|| error("纹理文件大小溢出"))?;
    let data = bytes.get(*at..end).ok_or_else(|| error("纹理文件不完整"))?;
    *at = end;
    Ok(data)
}
fn word(bytes: &[u8], at: &mut usize) -> Result<u32> {
    Ok(u32::from_le_bytes(take(bytes, at, 4)?.try_into()?))
}
fn texture_bytes(dimensions: [u32; 3], format: TextureFormat) -> Result<usize> {
    dimensions.into_iter().try_fold(format.texel_bytes(), |bytes, extent|
        bytes.checked_mul(extent as usize).ok_or_else(|| error("纹理层大小溢出")))
}
/// x连续、其次y、最后z；pixels持有原格式的实际体数据。
pub struct TextureMip3D {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub pixels: TextureTexels,
}
/// 不可变三维体资产；克隆只共享同一原mip存储，不生成二维图集或解码镜像。
#[derive(Clone)]
pub struct Texture3D {
    pub(crate) levels: Arc<[TextureMip3D]>,
    pub(crate) address: [TextureAddress; 3],
}
impl Texture3D {
    pub fn new(levels: Vec<TextureMip3D>) -> Result<Self> {
        if levels.is_empty() {
            return Err(error("纹理至少需要一个mip层"));
        }
        let format = levels[0].pixels.format();
        let mut expected = None;
        for level in &levels {
            let dimensions = [level.width, level.height, level.depth];
            if dimensions.contains(&0) || level.pixels.format() != format
                || level.pixels.as_bytes().len() != texture_bytes(dimensions, format)?
            {
                return Err(error("三维纹理尺寸、格式或负载不满足原生图像布局"));
            }
            if let Some(next) = expected {
                if dimensions != next {
                    return Err(error("mip尺寸必须逐轴减半并至少保留一个texel"));
                }
            }
            expected = Some(dimensions.map(|n| (n / 2).max(1)));
        }
        let base = &levels[0];
        let maximum = 32 - base.width.max(base.height).max(base.depth).leading_zeros();
        if levels.len() > maximum as usize {
            return Err(error("mip层数超过基础尺寸的完整链"));
        }
        Ok(Self { levels: levels.into(), address: [TextureAddress::Clamp; 3] })
    }
    pub fn with_address(mut self, u: TextureAddress, v: TextureAddress, w: TextureAddress) -> Self {
        self.address = [u, v, w];
        self
    }
    pub fn address(&self) -> [TextureAddress; 3] { self.address }
    pub fn format(&self) -> TextureFormat { self.levels[0].pixels.format() }
    pub fn dimensions(&self) -> [u32; 3] {
        [self.levels[0].width, self.levels[0].height, self.levels[0].depth]
    }
    pub fn mip_count(&self) -> u32 { self.levels.len() as u32 }
    pub fn mip(&self, level: usize) -> Option<&TextureMip3D> { self.levels.get(level) }
    /// BLTEX3交接三维mip与原通道位；寻址配置属于使用方，不写入资产。
    pub fn write(&self, path: &Path) -> Result<()> {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        f.write_all(b"BLTEX\0\0\0")?;
        for word in [3u32, 3, self.format() as u32, self.mip_count()] {
            f.write_all(&word.to_le_bytes())?;
        }
        for level in self.levels.iter() {
            for extent in [level.width, level.height, level.depth] {
                f.write_all(&extent.to_le_bytes())?;
            }
            f.write_all(level.pixels.as_bytes())?;
        }
        f.flush()?;
        Ok(())
    }
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let mut at = 0;
        if take(&bytes, &mut at, 8)? != b"BLTEX\0\0\0" {
            return Err(error("不是BLTEX资产"));
        }
        if word(&bytes, &mut at)? != 3 {
            return Err(error("不支持的三维BLTEX版本"));
        }
        if word(&bytes, &mut at)? != 3 {
            return Err(error("BLTEX体资产需要三维图像"));
        }
        let format = read_format(word(&bytes, &mut at)?)?;
        let count = word(&bytes, &mut at)?;
        let mut levels = Vec::new();
        for _ in 0..count {
            let dimensions = [word(&bytes, &mut at)?, word(&bytes, &mut at)?, word(&bytes, &mut at)?];
            let data = take(&bytes, &mut at, texture_bytes(dimensions, format)?)?;
            levels.push(TextureMip3D { width: dimensions[0], height: dimensions[1], depth: dimensions[2],
                pixels: TextureTexels::from_bytes(format, data)? });
        }
        if at != bytes.len() {
            return Err(error("纹理文件有未声明的尾部数据"));
        }
        Self::new(levels)
    }
}
