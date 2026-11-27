//! 不可变有类型纹理与独立资产；CPU描述只借用实际texel存储，不生成解码镜像。
use crate::{Result, error};
use std::{
    ffi::c_void,
    fs::File,
    io::{ErrorKind, Read, Write},
    path::Path,
    sync::Arc,
};
pub use blanguage::ast::TextureFormat;
pub enum TextureTexels {
    Rgba32Float(Vec<[f32; 4]>),
    Rgba8Unorm(Vec<[u8; 4]>),
    Rgba8Srgb(Vec<[u8; 4]>),
    R8Unorm(Vec<u8>),
    Rg16Unorm(Vec<[u16; 2]>),
    Rgb10A2Unorm(Vec<u32>),
    R11G11B10Float(Vec<u32>),
    R16Float(Vec<u16>),
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
            Self::R11G11B10Float(_) => TextureFormat::R11G11B10Float,
            Self::R16Float(_) => TextureFormat::R16Float,
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        let (pixels, bytes): (*const u8, usize) = match self {
            Self::Rgba32Float(v) => (v.as_ptr().cast(), v.len() * 16),
            Self::Rgba8Unorm(v) | Self::Rgba8Srgb(v) => (v.as_ptr().cast(), v.len() * 4),
            Self::R8Unorm(v) => (v.as_ptr(), v.len()),
            Self::R16Float(v) => (v.as_ptr().cast(), v.len() * 2),
            Self::Rg16Unorm(v) => (v.as_ptr().cast(), v.len() * 4),
            Self::Rgb10A2Unorm(v) | Self::R11G11B10Float(v) => (v.as_ptr().cast(), v.len() * 4),
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
            TextureFormat::R11G11B10Float => Self::R11G11B10Float(copy(bytes)),
            TextureFormat::R16Float => Self::R16Float(copy(bytes)),
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
/// 同步CPU调用借用的实际描述；不取得像素或mip的释放权。
#[derive(Clone, Copy)]
pub struct CpuTextureBinding<'a> {
    dimension: blanguage::ast::TextureDimension,
    descriptor: &'a TextureDescriptor,
}
impl CpuTextureBinding<'_> {
    /// # Safety
    /// descriptor为声明维度的原有效CPU纹理描述，mip及像素保持至全部同步消费者结束。
    /// 描述满足现行纹理ABI；本借用不接收原资源的释放权。
    pub unsafe fn from_raw<'a>(dimension: blanguage::ast::TextureDimension, descriptor: *const u32) -> CpuTextureBinding<'a> {
        CpuTextureBinding { dimension, descriptor: unsafe { &*descriptor.cast::<TextureDescriptor>() } }
    }
    pub(crate) fn dimension(&self) -> blanguage::ast::TextureDimension { self.dimension }
    pub(crate) fn format_code(&self) -> u32 { self.descriptor.format }
    pub(crate) fn pointer(&self) -> *mut u32 { (self.descriptor as *const TextureDescriptor).cast_mut().cast() }
}
/// 原CPU纹理的同步描述拥有者；显式借用像素输入，不能早于其结束该借用。
pub struct CpuTextures<'a> {
    inputs: &'a [TextureInput],
    _levels: Vec<Box<[LevelDescriptor]>>,
    descriptors: Vec<TextureDescriptor>,
}
impl<'a> CpuTextures<'a> {
    pub fn new(textures: &'a [TextureInput]) -> Self {
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
        Self { inputs: textures, _levels: levels, descriptors }
    }
    pub fn bindings(&self) -> Vec<CpuTextureBinding<'_>> {
        self.descriptors.iter().zip(self.inputs).map(|(descriptor, input)|
            CpuTextureBinding { dimension: input.dimension(), descriptor }).collect()
    }
}
#[repr(C)]
struct MipExtent([u32; 3]);

unsafe extern "C" {
    #[link_name = "codec.type_layout::mip_level_count"]
    fn native_mip_level_count(base: MipExtent) -> u32;

    #[link_name = "codec.type_layout::mip_extent"]
    fn native_mip_extent(base: MipExtent, level: u32) -> MipExtent;
}

pub(crate) fn mip_level_count(base: [u32; 3]) -> usize {
    unsafe { native_mip_level_count(MipExtent(base)) as usize }
}

pub(crate) fn mip_extent(base: [u32; 3], level: u32) -> [u32; 3] {
    unsafe { native_mip_extent(MipExtent(base), level).0 }
}

struct MipLayout<const D: usize> {
    levels: usize,
    next: Option<[u32; 3]>,
}
impl<const D: usize> MipLayout<D> {
    fn new(levels: usize) -> Result<Self> {
        if levels == 0 {
            return Err(error("纹理至少需要一个mip层"));
        }
        Ok(Self { levels, next: None })
    }

    fn layout_error() -> &'static str {
        if D == 2 { "纹理尺寸、格式或负载不满足原生采样ABI" }
        else { "三维纹理尺寸、格式或负载不满足原生图像布局" }
    }

    fn accept(&mut self, dimensions: [u32; 3], format: TextureFormat) -> Result<usize> {
        if dimensions.contains(&0) {
            return Err(error(Self::layout_error()));
        }
        let bytes = dimensions.into_iter().try_fold(format.texel_bytes(), |bytes, extent|
            bytes.checked_mul(extent as usize).ok_or_else(|| error("纹理层大小溢出")))?;
        if let Some(expected) = self.next {
            if dimensions != expected {
                return Err(error(if D == 2 { "mip尺寸必须逐级减半并至少保留一个texel" }
                    else { "mip尺寸必须逐轴减半并至少保留一个texel" }));
            }
        } else if self.levels > mip_level_count(dimensions) {
            return Err(error("mip层数超过基础尺寸的完整链"));
        }
        self.next = Some(mip_extent(dimensions, 1));
        Ok(bytes)
    }

    fn check(&mut self, dimensions: [u32; 3], format: TextureFormat, pixels: &TextureTexels) -> Result<()> {
        if pixels.format() != format || pixels.as_bytes().len() != self.accept(dimensions, format)? {
            return Err(error(Self::layout_error()));
        }
        Ok(())
    }
}
impl Texture2D {
    pub fn new(levels: Vec<TextureMip>) -> Result<Self> {
        let mut layout = MipLayout::<2>::new(levels.len())?;
        let format = levels[0].pixels.format();
        for level in &levels {
            layout.check([level.width, level.height, 1], format, &level.pixels)?;
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
        write_bltex_header(&mut f, 2, self.format(), self.mip_count())?;
        for mip in self.levels.iter() {
            f.write_all(&mip.width.to_le_bytes())?;
            f.write_all(&mip.height.to_le_bytes())?;
            f.write_all(mip.pixels.as_bytes())?;
        }
        f.flush()?;
        Ok(())
    }
    pub fn read(path: &Path) -> Result<Self> {
        let mut input = TextureFile::open(path)?;
        let (format, count) = input.header(2)?;
        let mut layout = MipLayout::<2>::new(count as usize)?;
        let mut levels = Vec::new();
        for _ in 0..count {
            let [width, height] = input.words()?;
            let bytes = layout.accept([width, height, 1], format)?;
            levels.push(TextureMip {
                width,
                height,
                pixels: input.pixels(bytes, format)?,
            });
        }
        input.finish()?;
        Ok(Self { levels: levels.into(), address: [TextureAddress::Clamp; 2] })
    }
}

const BLTEX_MAX_HEADER: usize = 24;

#[repr(C)]
struct BltexPiece {
    bytes: [u8; 8],
    read: u32,
}

#[allow(dead_code)] // Produced by the B header decoder.
#[repr(u32)]
enum BltexStatus { Valid, Input, Magic, Version, Dimension }

#[repr(C)]
struct BltexHeader {
    status: BltexStatus,
    count: u32,
}

struct BltexInput<'a> {
    reader: &'a mut TextureFile,
    format: Option<TextureFormat>,
    error: Option<Box<dyn std::error::Error + Send + Sync>>,
}

unsafe extern "C" {
    #[link_name = "codec.value_format::decode_bltex_header"]
    fn decode_bltex_header(
        dimensions: u32, input: *mut c_void,
        next: unsafe extern "C" fn(*mut c_void, u32) -> BltexPiece,
        select_format: unsafe extern "C" fn(*mut c_void, u32) -> u32,
    ) -> BltexHeader;

    #[link_name = "codec.value_format::encode_bltex_header"]
    fn encode_bltex_header(
        bytes: *mut [u8; BLTEX_MAX_HEADER], dimensions: u32, format: u32, count: u32,
    ) -> usize;
}

unsafe extern "C" fn read_bltex_piece(input: *mut c_void, length: u32) -> BltexPiece {
    let input = unsafe { &mut *input.cast::<BltexInput<'_>>() };
    let mut bytes = [0; 8];
    match input.reader.read_exact(&mut bytes[..length as usize]) {
        Ok(()) => BltexPiece { bytes, read: 1 },
        Err(error) => {
            input.error = Some(error);
            BltexPiece { bytes, read: 0 }
        }
    }
}

unsafe extern "C" fn select_bltex_format(input: *mut c_void, code: u32) -> u32 {
    let input = unsafe { &mut *input.cast::<BltexInput<'_>>() };
    input.format = TextureFormat::from_code(code);
    if input.format.is_some() {
        return 1;
    }
    input.error = Some(error("未知BLTEX texel格式"));
    0
}

fn write_bltex_header(
    writer: &mut impl Write, dimensions: u32, format: TextureFormat, count: u32,
) -> std::io::Result<()> {
    let mut bytes = std::mem::MaybeUninit::<[u8; BLTEX_MAX_HEADER]>::uninit();
    let length = unsafe { encode_bltex_header(bytes.as_mut_ptr(), dimensions, format as u32, count) };
    writer.write_all(&unsafe { bytes.assume_init() }[..length])
}

struct TextureFile {
    file: File,
    remaining: Option<u64>,
}

impl TextureFile {
    fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let remaining = crate::disk_file_length(&file)?;
        Ok(Self { file, remaining })
    }

    fn header(&mut self, dimensions: u32) -> Result<(TextureFormat, u32)> {
        let mut input = BltexInput { reader: self, format: None, error: None };
        let header = unsafe {
            decode_bltex_header(dimensions, std::ptr::from_mut(&mut input).cast(),
                read_bltex_piece, select_bltex_format)
        };
        match header.status {
            BltexStatus::Valid => Ok((input.format.unwrap(), header.count)),
            BltexStatus::Input => Err(input.error.unwrap()),
            BltexStatus::Magic => Err(error("不是BLTEX资产")),
            BltexStatus::Version if dimensions == 3 => Err(error("不支持的三维BLTEX版本")),
            BltexStatus::Version => Err(error("不支持的BLTEX版本")),
            BltexStatus::Dimension => Err(error("BLTEX体资产需要三维图像")),
        }
    }

    fn read_exact(&mut self, output: &mut [u8]) -> Result<()> {
        match self.file.read_exact(output) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Err(error("纹理文件不完整")),
            Err(e) => return Err(e.into()),
        }
        if let Some(remaining) = &mut self.remaining {
            *remaining = remaining.saturating_sub(output.len() as u64);
        }
        Ok(())
    }

    fn words<const N: usize>(&mut self) -> Result<[u32; N]> {
        let mut words = [0u32; N];
        let bytes = unsafe {
            std::slice::from_raw_parts_mut(words.as_mut_ptr().cast(), std::mem::size_of_val(&words))
        };
        self.read_exact(bytes)?;
        Ok(words.map(u32::from_le))
    }

    fn pixels(&mut self, bytes: usize, format: TextureFormat) -> Result<TextureTexels> {
        if self.remaining.is_some_and(|remaining| remaining < bytes as u64) {
            return Err(error("纹理文件不完整"));
        }
        // Each format uses padding-free texels for which every bit pattern is valid.
        unsafe {
            Ok(match format {
                TextureFormat::Rgba32Float => TextureTexels::Rgba32Float(self.read_texels(bytes)?),
                TextureFormat::Rgba8Unorm => TextureTexels::Rgba8Unorm(self.read_texels(bytes)?),
                TextureFormat::Rgba8Srgb => TextureTexels::Rgba8Srgb(self.read_texels(bytes)?),
                TextureFormat::R8Unorm => TextureTexels::R8Unorm(self.read_texels(bytes)?),
                TextureFormat::Rg16Unorm => TextureTexels::Rg16Unorm(self.read_texels(bytes)?),
                TextureFormat::Rgb10A2Unorm => TextureTexels::Rgb10A2Unorm(self.read_texels(bytes)?),
                TextureFormat::R11G11B10Float => TextureTexels::R11G11B10Float(self.read_texels(bytes)?),
                TextureFormat::R16Float => TextureTexels::R16Float(self.read_texels(bytes)?),
            })
        }
    }

    // T must have no padding, accept every bit pattern, and divide bytes exactly.
    unsafe fn read_texels<T: Copy + Default>(&mut self, bytes: usize) -> Result<Vec<T>> {
        let stride = std::mem::size_of::<T>();
        let count = bytes / stride;
        // A stream has no measured extent. Grow the final storage as the input arrives.
        let chunk = if self.remaining.is_some() { count } else { 64 * 1024 / stride };
        let mut texels = Vec::new();
        while texels.len() < count {
            let start = texels.len();
            let length = (count - start).min(chunk);
            texels.try_reserve(length)?;
            texels.resize(start + length, T::default());
            let output = unsafe {
                std::slice::from_raw_parts_mut(texels[start..].as_mut_ptr().cast(), length * stride)
            };
            self.read_exact(output)?;
        }
        Ok(texels)
    }

    fn finish(mut self) -> Result<()> {
        match self.file.read_exact(&mut [0]) {
            Ok(()) => Err(error("纹理文件有未声明的尾部数据")),
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
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
        let mut layout = MipLayout::<3>::new(levels.len())?;
        let format = levels[0].pixels.format();
        for level in &levels {
            layout.check([level.width, level.height, level.depth], format, &level.pixels)?;
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
        write_bltex_header(&mut f, 3, self.format(), self.mip_count())?;
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
        let mut input = TextureFile::open(path)?;
        let (format, count) = input.header(3)?;
        let mut layout = MipLayout::<3>::new(count as usize)?;
        let mut levels = Vec::new();
        for _ in 0..count {
            let [width, height, depth] = input.words()?;
            let bytes = layout.accept([width, height, depth], format)?;
            levels.push(TextureMip3D {
                width, height, depth,
                pixels: input.pixels(bytes, format)?,
            });
        }
        input.finish()?;
        Ok(Self { levels: levels.into(), address: [TextureAddress::Clamp; 3] })
    }
}
