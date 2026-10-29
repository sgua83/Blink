//! 编译程序的设备执行、数据存储与生命周期所有者。
pub mod application;
mod bindings;
pub mod cpu;
pub mod ffi;
mod texture;
mod views;
pub use texture::{Texture2D, TextureAddress, TextureFormat, TextureMip, TextureTexels};
pub mod vulkan;
pub use bindings::{DomainBindings, DomainStorage};
use blanguage::ir::{ExecutionPlan, ProgramInterface, Scalar, Ty};
pub use views::{ReadDataset, ViewBinding};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub(crate) fn error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    std::io::Error::other(message.into()).into()
}

// 模块资源 C ABI 的标量：u32 标签与 C union 载荷；所有权仍由实际调用者保持。
#[repr(C, u32)]
#[derive(Clone, Copy, Debug)]
pub enum Value {
    F32(f32) = blanguage::resource::F32,
    U32(u32) = blanguage::resource::U32,
    I32(i32) = blanguage::resource::I32,
    Bool(bool) = blanguage::resource::BOOL,
    U64(u64) = blanguage::resource::U64,
    I64(i64) = blanguage::resource::I64, F64(f64) = blanguage::resource::F64,
    U8(u8) = blanguage::resource::U8,
    I8(i8) = blanguage::resource::I8,
    U16(u16) = blanguage::resource::U16,
    I16(i16) = blanguage::resource::I16,
}
impl Value {
    fn scalar(self) -> Scalar {
        match self {
            Self::F32(_) => Scalar::F32,
            Self::U32(_) => Scalar::U32,
            Self::I32(_) => Scalar::I32,
            Self::Bool(_) => Scalar::Bool,
            Self::U64(_) => Scalar::U64,
            Self::I64(_) => Scalar::I64, Self::F64(_) => Scalar::F64,
            Self::U8(_) => Scalar::U8,
            Self::I8(_) => Scalar::I8,
            Self::U16(_) => Scalar::U16,
            Self::I16(_) => Scalar::I16,
        }
    }
    fn bits(self) -> u64 {
        match self {
            Self::F32(v) => u64::from(v.to_bits()),
            Self::U32(v) => u64::from(v),
            Self::I32(v) => u64::from(v as u32),
            Self::Bool(v) => u64::from(v),
            Self::U64(v) => v,
            Self::I64(v) => v as u64, Self::F64(v) => v.to_bits(),
            Self::U8(v) => u64::from(v),
            Self::I8(v) => u64::from(v as u8),
            Self::U16(v) => u64::from(v),
            Self::I16(v) => u64::from(v as u16),
        }
    }
    fn write_words(self, words: &mut [u32], offset: usize) {
        let bits = self.bits();
        let word = offset / 4;
        let scalar = self.scalar();
        if scalar.size() < 4 {
            let shift = (offset % 4) * 8;
            let mask = (scalar.bit_mask() as u32) << shift;
            words[word] = words[word] & !mask | (bits as u32) << shift;
        } else {
            words[word] = bits as u32;
            if scalar.size() == 8 {
                words[word + 1] = (bits >> 32) as u32;
            }
        }
    }
}
pub struct Dataset {
    pub(crate) ty: Ty,
    pub(crate) stride: u32,
    pub(crate) count: u32,
    pub(crate) words: Vec<u32>,
    pub(crate) views: views::Views<ReadDataset>,
}
impl Dataset {
    /// 接收已经打包的记录存储，避免在上传前再次复制整个数据集。
    pub fn from_words(
        program: &dyn ProgramInterface,
        binding: usize,
        words: Vec<u32>,
    ) -> Result<Self> {
        let b = program
            .buffers()
            .get(binding)
            .ok_or_else(|| error("数据绑定编号越界"))?;
        let bytes = words.len() * 4;
        if bytes % b.stride as usize != 0 {
            return Err(error("打包数据不是完整记录"));
        }
        Ok(Self {
            ty: b.ty.clone(),
            stride: b.stride,
            count: u32::try_from(bytes / b.stride as usize)?,
            words,
            views: views::Views::default(),
        })
    }
    pub fn new(program: &dyn ProgramInterface, binding: usize, count: u32) -> Result<Self> {
        let b = program
            .buffers()
            .get(binding)
            .ok_or_else(|| error("数据绑定编号越界"))?;
        Self::with_type(b.ty.clone(), count)
    }
    pub fn for_view(
        program: &dyn ProgramInterface,
        binding: usize,
        field: &str,
        count: u32,
    ) -> Result<Self> {
        let b = program
            .buffers()
            .get(binding)
            .ok_or_else(|| error("数据绑定编号越界"))?;
        let (_, view) =
            b.ty.field(field)
                .ok_or_else(|| error(format!("未知 view 字段 {field}")))?;
        let ty = view
            .view
            .as_deref()
            .ok_or_else(|| error(format!("{field} 不是 view")))?;
        Self::with_type(ty.clone(), count)
    }
    pub(crate) fn with_type(ty: Ty, count: u32) -> Result<Self> {
        let stride = blanguage::ir::align_up(ty.size, ty.align);
        let bytes = (stride as usize)
            .checked_mul(count as usize)
            .ok_or_else(|| error("数据集长度溢出"))?;
        let mut words = Vec::new();
        words.try_reserve_exact(bytes.div_ceil(4))?;
        words.resize(bytes.div_ceil(4), 0);
        Ok(Self {
            ty,
            stride,
            count,
            words,
            views: views::Views::default(),
        })
    }
    /// BLDATA v1 保存编译布局下的完整记录；类型来自程序，不从文件生成另一份 schema。
    pub fn read_data(
        program: &dyn ProgramInterface,
        binding: usize,
        path: &std::path::Path,
    ) -> Result<Self> {
        use std::io::Read;
        let mut input = std::io::BufReader::new(std::fs::File::open(path)?);
        let mut header = [0u8; 24];
        input.read_exact(&mut header)?;
        if &header[..8] != b"BLDATA\0\0" {
            return Err(error("不是 BLDATA 数据"));
        }
        let word = |i| u32::from_le_bytes(header[i..i + 4].try_into().unwrap());
        if word(8) != 1 {
            return Err(error("不支持的 BLDATA 版本"));
        }
        let mut data = Self::new(program, binding, word(12))?;
        if word(16) != data.stride {
            return Err(error("BLDATA 步长与编译布局不一致"));
        }
        let length = data.stride as usize * data.count as usize;
        for (index, word) in data.words.iter_mut().enumerate() {
            let mut bytes = [0; 4];
            input.read_exact(&mut bytes[..(length - index * 4).min(4)])?;
            *word = u32::from_le_bytes(bytes);
        }
        let mut trailing = [0u8; 1];
        if input.read(&mut trailing)? != 0 {
            return Err(error("BLDATA 包含未声明的记录"));
        }
        Ok(data)
    }
    pub fn write_data(&self, path: &std::path::Path, steps: u32) -> Result<()> {
        use std::io::Write;
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        file.write_all(b"BLDATA\0\0")?;
        for word in [1, self.count, self.stride, steps] {
            file.write_all(&word.to_le_bytes())?;
        }
        let length = self.stride as usize * self.count as usize;
        for (index, word) in self.words.iter().enumerate() {
            file.write_all(&word.to_le_bytes()[..(length - index * 4).min(4)])?;
        }
        file.flush()?;
        Ok(())
    }
    pub fn words(&self) -> &[u32] {
        &self.words
    }
    /// 取得原始可写存储会放弃已绑定视图；执行前须通过 with_views 显式重新绑定。
    pub fn words_mut(&mut self) -> &mut [u32] {
        self.views.clear();
        &mut self.words
    }
    pub fn count(&self) -> u32 {
        self.count
    }
    pub fn stride(&self) -> u32 {
        self.stride
    }
    pub fn field_offset(&self, name: &str) -> Result<u32> {
        self.ty
            .field(name)
            .map(|(offset, _)| offset)
            .ok_or_else(|| error(format!("未知字段 {name}")))
    }
    /// 按声明布局写入记录的单标量字段；与words_mut一样使原view绑定失效。
    pub fn set_scalar(&mut self, index: u32, name: &str, value: Value) -> Result<()> {
        if index >= self.count {
            return Err(error("数据集记录下标越界"));
        }
        let stride = self.stride as usize;
        let start = index as usize * stride;
        let (offset, ty) = self.ty
            .field_at(name, &self.words[start / 4..(start + stride).div_ceil(4)])
            .ok_or_else(|| error(format!("未知或未激活的字段 {name}")))?;
        let scalar = value.scalar();
        if ty.components.as_slice() != [(scalar, 0)] || ty.size != scalar.size() {
            return Err(error(format!("字段 {name} 的标量类型与值不匹配")));
        }
        value.write_words(self.words_mut(), start + offset as usize);
        Ok(())
    }
}
pub(crate) fn arguments<'a>(
    program: &ExecutionPlan,
    shapes: impl ExactSizeIterator<Item = (&'a Ty, u32, u32)>,
    values: &[Value],
) -> Result<Vec<u32>> {
    if shapes.len() != program.buffers().len() {
        return Err(error("设备调用的数据集数量不匹配"));
    }
    let mut words = vec![0; program.uniform_bytes() as usize / 4];
    for (i, ((ty, stride, count), expected)) in shapes.zip(program.buffers()).enumerate() {
        if *ty != expected.ty || stride != expected.stride {
            return Err(error(format!(
                "绑定 {} 的数据布局与程序不一致",
                expected.name
            )));
        }
        if expected.count.is_some_and(|n| n != count) {
            return Err(error(format!(
                "绑定 {} 的元素数不满足程序形状 {:?}",
                expected.name, expected.count
            )));
        }
        words[2 + i] = count;
    }
    let length = |binding: usize| words[2 + binding];
    for stage in program.stages() {
        if let Some(selection) = stage.selection
            && length(selection.binding) < length(stage.domain)
        {
            return Err(error("where 实际谓词容量不能小于原始域容量"));
        }
        let sources: std::collections::BTreeSet<_> = stage
            .input_words()
            .into_iter()
            .map(|(source, _)| source)
            .filter(|source| *source != stage.domain)
            .collect();
        for source in sources {
            if length(source) < length(stage.domain) {
                return Err(error("tile 实际源容量不能小于阶段域容量"));
            }
        }
        if let blanguage::ir::Operation::Compact(c) = &stage.operation {
            if length(c.source) != length(c.flags) || length(c.target) < length(c.source) {
                return Err(error(
                    "compact 实际输入／谓词长度或输出容量不满足并行写入要求",
                ));
            }
        }
        if let blanguage::ir::Operation::StableSort(s) = &stage.operation {
            if length(s.target) < length(s.source) {
                return Err(error("排序的实际输出容量不足以容纳输入域"));
            }
        }
        if let blanguage::ir::Operation::ScanExclusive(r) = &stage.operation {
            let r = &r.values;
            if (stage.prefix.is_none() && length(r.source) != length(r.target))
                || (stage.prefix.is_some() && length(r.target) < length(r.source))
            {
                return Err(error("扫描的实际输出容量不满足输入域"));
            }
        }
        if let blanguage::ir::Operation::CustomScan(r) = &stage.operation
            && length(r.values.fields.target) < length(r.values.fields.source)
        {
            return Err(error("自定义扫描的实际输出容量不足以容纳输入域"));
        }
        if let blanguage::ir::Operation::CustomSegmentedReduce(r) = &stage.operation
            && length(r.values.fields.target) < length(r.values.fields.source)
        {
            return Err(error("自定义分段归约的实际输出容量不足以容纳输入段"));
        }
        if let blanguage::ir::Operation::SegmentedReduce(r) = &stage.operation {
            let fields = &r.values.fields;
            if length(fields.target) < length(fields.source) {
                return Err(error("分段归约的实际目标容量不足以容纳输入段"));
            }
        }
    }
    let expected: usize = program
        .uniforms()
        .iter()
        .map(|u| u.ty.components.len())
        .sum();
    if values.len() != expected {
        return Err(error(format!(
            "需要 {expected} 个标量参数，实际 {}",
            values.len()
        )));
    }
    let mut at = 0;
    for uniform in program.uniforms() {
        for (scalar, offset) in &uniform.ty.components {
            let value = values[at];
            if value.scalar() != *scalar {
                return Err(error(format!("参数 {} 的分量类型不匹配", uniform.name)));
            }
            value.write_words(&mut words, (uniform.offset + offset) as usize);
            at += 1;
        }
    }
    Ok(words)
}
