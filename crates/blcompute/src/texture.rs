//! 不可变有类型纹理与独立资产；CPU描述只借用实际texel存储，不生成解码镜像。
use crate::{Result, error};
use std::{io::Write, path::Path, sync::Arc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum TextureFormat {
    Rgba32Float = 0,
    Rgba8Unorm = 1,
    Rgba8Srgb = 2,
    R8Unorm = 3,
}
impl TextureFormat {
    pub fn texel_bytes(self) -> usize {
        match self {
            Self::Rgba32Float => 16,
            Self::Rgba8Unorm | Self::Rgba8Srgb => 4,
            Self::R8Unorm => 1,
        }
    }
    fn read(value: u32) -> Result<Self> {
        match value {
            0 => Ok(Self::Rgba32Float),
            1 => Ok(Self::Rgba8Unorm),
            2 => Ok(Self::Rgba8Srgb),
            3 => Ok(Self::R8Unorm),
            _ => Err(error("未知BLTEX texel格式")),
        }
    }
}
pub enum TextureTexels {
    Rgba32Float(Vec<[f32; 4]>),
    Rgba8Unorm(Vec<[u8; 4]>),
    Rgba8Srgb(Vec<[u8; 4]>),
    R8Unorm(Vec<u8>),
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
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Rgba32Float(v) => unsafe {
                std::slice::from_raw_parts(v.as_ptr().cast(), v.len() * 16)
            },
            Self::Rgba8Unorm(v) | Self::Rgba8Srgb(v) => unsafe {
                std::slice::from_raw_parts(v.as_ptr().cast(), v.len() * 4)
            },
            Self::R8Unorm(v) => v,
        }
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
#[repr(C)]
pub(crate) struct LevelDescriptor {
    width: u32,
    height: u32,
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
    pub fn new(textures: &[Texture2D]) -> Self {
        let levels: Vec<Box<[LevelDescriptor]>> = textures
            .iter()
            .map(|t| {
                t.levels
                    .iter()
                    .map(|m| LevelDescriptor {
                        width: m.width,
                        height: m.height,
                        pixels: m.pixels.as_bytes().as_ptr(),
                    })
                    .collect()
            })
            .collect();
        let descriptors = levels
            .iter()
            .zip(textures)
            .map(|(m, t)| TextureDescriptor {
                levels: m.len() as u32,
                address: u32::from(t.address[0] == TextureAddress::Repeat)
                    | (u32::from(t.address[1] == TextureAddress::Repeat) << 1),
                mips: m.as_ptr(),
                format: t.format() as u32,
            })
            .collect();
        Self {
            _levels: levels,
            descriptors,
        }
    }
    pub fn pointers(&self) -> impl Iterator<Item = *mut u32> {
        self.descriptors
            .iter()
            .map(|d| (d as *const TextureDescriptor).cast_mut().cast())
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
                || level.width > 1 << 24
                || level.height > 1 << 24
                || level.pixels.format() != format
                || level.pixels.as_bytes().len() as u64
                    != level.width as u64 * level.height as u64 * format.texel_bytes() as u64
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
        fn take<'a>(bytes: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8]> {
            let end = at.checked_add(n).ok_or_else(|| error("纹理文件大小溢出"))?;
            let data = bytes.get(*at..end).ok_or_else(|| error("纹理文件不完整"))?;
            *at = end;
            Ok(data)
        }
        fn word(bytes: &[u8], at: &mut usize) -> Result<u32> {
            Ok(u32::from_le_bytes(take(bytes, at, 4)?.try_into()?))
        }
        if take(&bytes, &mut at, 8)? != b"BLTEX\0\0\0" {
            return Err(error("不是BLTEX资产"));
        }
        let format = match word(&bytes, &mut at)? {
            1 => TextureFormat::Rgba32Float,
            2 => TextureFormat::read(word(&bytes, &mut at)?)?,
            _ => return Err(error("不支持的BLTEX版本")),
        };
        let count = word(&bytes, &mut at)?;
        let mut levels = Vec::new();
        for _ in 0..count {
            let width = word(&bytes, &mut at)?;
            let height = word(&bytes, &mut at)?;
            let size = (width as u64)
                .checked_mul(height as u64)
                .and_then(|n| n.checked_mul(format.texel_bytes() as u64))
                .ok_or_else(|| error("纹理层大小溢出"))?;
            let data = take(&bytes, &mut at, usize::try_from(size)?)?;
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
