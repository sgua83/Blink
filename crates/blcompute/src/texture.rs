//! 不可变浮点纹理与独立资产格式；CPU 描述只在本次原生调用期间借用实际像素。
use crate::{Result, error};
use std::{io::Write, path::Path, sync::Arc};
pub struct TextureMip {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 4]>,
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
    pixels: *const [f32; 4],
}
#[repr(C)]
pub(crate) struct TextureDescriptor {
    levels: u32,
    address: u32,
    mips: *const LevelDescriptor,
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
                        pixels: m.pixels.as_ptr(),
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
            return Err(error("纹理至少需要一个 mip 层"));
        }
        let mut expected = None;
        for level in &levels {
            if level.width == 0
                || level.height == 0
                || level.width > 1 << 24
                || level.height > 1 << 24
                || level.pixels.len() as u64 != level.width as u64 * level.height as u64
            {
                return Err(error("纹理尺寸或实际像素数量不满足原生采样 ABI"));
            }
            if let Some((w, h)) = expected {
                if (level.width, level.height) != (w, h) {
                    return Err(error("mip 尺寸必须逐级减半并至少保留一个 texel"));
                }
            }
            expected = Some(((level.width / 2).max(1), (level.height / 2).max(1)));
        }
        let maximum = 32 - levels[0].width.max(levels[0].height).leading_zeros();
        if levels.len() > maximum as usize {
            return Err(error("mip 层数超过基础尺寸的完整链"));
        }
        Ok(Self {
            levels: levels.into(),
            address: [TextureAddress::Clamp; 2],
        })
    }
    /// 采样模式属于当前只读资源视图；共享原像素，不重写图像或其文件格式。
    pub fn with_address(mut self, u: TextureAddress, v: TextureAddress) -> Self {
        self.address = [u, v];
        self
    }
    pub fn address(&self) -> [TextureAddress; 2] {
        self.address
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
        f.write_all(&1u32.to_le_bytes())?;
        f.write_all(&self.mip_count().to_le_bytes())?;
        for mip in self.levels.iter() {
            f.write_all(&mip.width.to_le_bytes())?;
            f.write_all(&mip.height.to_le_bytes())?;
            for pixel in &mip.pixels {
                for channel in pixel {
                    f.write_all(&channel.to_bits().to_le_bytes())?;
                }
            }
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
        if take(&bytes, &mut at, 8)? != b"BLTEX\0\0\0" || word(&bytes, &mut at)? != 1 {
            return Err(error("不支持的 BLTEX 资产格式"));
        }
        let count = word(&bytes, &mut at)?;
        let mut levels = Vec::new();
        for _ in 0..count {
            let width = word(&bytes, &mut at)?;
            let height = word(&bytes, &mut at)?;
            let size = (width as u64)
                .checked_mul(height as u64)
                .and_then(|n| n.checked_mul(16))
                .ok_or_else(|| error("纹理层大小溢出"))?;
            let data = take(&bytes, &mut at, usize::try_from(size)?)?;
            let pixels = data
                .chunks_exact(16)
                .map(|p| {
                    std::array::from_fn(|i| {
                        f32::from_bits(u32::from_le_bytes(p[i * 4..i * 4 + 4].try_into().unwrap()))
                    })
                })
                .collect();
            levels.push(TextureMip {
                width,
                height,
                pixels,
            });
        }
        if at != bytes.len() {
            return Err(error("纹理文件有未声明的尾部数据"));
        }
        Self::new(levels)
    }
}
