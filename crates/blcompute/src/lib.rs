//! 编译程序的设备执行、数据存储与生命周期所有者。
pub mod application;
mod bindings;
pub mod cpu;
pub mod ffi;
mod texture;
mod views;
pub use texture::{Texture2D, TextureMip};
pub mod vulkan;
pub use bindings::{DomainBindings, DomainStorage};
use blanguage::ir::{ExecutionPlan, ProgramInterface, Scalar, Ty};
pub use views::{ReadDataset, ViewBinding};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub(crate) fn error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    std::io::Error::other(message.into()).into()
}

#[derive(Clone, Copy, Debug)]
pub enum Value {
    F32(f32),
    U32(u32),
    I32(i32),
    Bool(bool),
}
impl Value {
    fn scalar(self) -> Scalar {
        match self {
            Self::F32(_) => Scalar::F32,
            Self::U32(_) => Scalar::U32,
            Self::I32(_) => Scalar::I32,
            Self::Bool(_) => Scalar::Bool,
        }
    }
    fn bits(self) -> u32 {
        match self {
            Self::F32(v) => v.to_bits(),
            Self::U32(v) => v,
            Self::I32(v) => v as u32,
            Self::Bool(v) => v as u32,
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
        let stride_words = b.stride as usize / 4;
        if words.len() % stride_words != 0 {
            return Err(error("打包数据不是完整记录"));
        }
        Ok(Self {
            ty: b.ty.clone(),
            stride: b.stride,
            count: u32::try_from(words.len() / stride_words)?,
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
        words.try_reserve_exact(bytes / 4)?;
        words.resize(bytes / 4, 0);
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
        for chunk in data.words.chunks_mut(16384) {
            let mut bytes = vec![0u8; chunk.len() * 4];
            input.read_exact(&mut bytes)?;
            for (word, raw) in chunk.iter_mut().zip(bytes.chunks_exact(4)) {
                *word = u32::from_le_bytes(raw.try_into().unwrap());
            }
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
        for chunk in self.words.chunks(16384) {
            let bytes: Vec<_> = chunk.iter().flat_map(|w| w.to_le_bytes()).collect();
            file.write_all(&bytes)?;
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
}
pub(crate) fn arguments(
    program: &ExecutionPlan,
    shapes: &[(&Ty, u32, u32)],
    values: &[Value],
) -> Result<Vec<u32>> {
    if shapes.len() != program.buffers().len() {
        return Err(error("设备调用的数据集数量不匹配"));
    }
    let mut words = vec![0; program.uniform_bytes() as usize / 4];
    for (i, ((ty, stride, count), expected)) in shapes.iter().zip(program.buffers()).enumerate() {
        if **ty != expected.ty || *stride != expected.stride {
            return Err(error(format!(
                "绑定 {} 的数据布局与程序不一致",
                expected.name
            )));
        }
        if expected.count.is_some_and(|n| n != *count) {
            return Err(error(format!(
                "绑定 {} 的元素数不满足程序形状 {:?}",
                expected.name, expected.count
            )));
        }
        words[2 + i] = *count;
    }
    for stage in program.stages() {
        if let Some(selection) = stage.selection
            && shapes[selection.binding].2 < shapes[stage.domain].2
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
            if shapes[source].2 < shapes[stage.domain].2 {
                return Err(error("tile 实际源容量不能小于阶段域容量"));
            }
        }
        if let blanguage::ir::Operation::Compact(c) = &stage.operation {
            if shapes[c.source].2 != shapes[c.flags].2 || shapes[c.target].2 < shapes[c.source].2 {
                return Err(error(
                    "compact 实际输入／谓词长度或输出容量不满足并行写入要求",
                ));
            }
        }
        if let blanguage::ir::Operation::StableSort(s) = &stage.operation {
            if shapes[s.target].2 < shapes[s.source].2 {
                return Err(error("排序的实际输出容量不足以容纳输入域"));
            }
        }
        if let blanguage::ir::Operation::ScanExclusive(r) = &stage.operation {
            let r = &r.values;
            if (stage.prefix.is_none() && shapes[r.source].2 != shapes[r.target].2)
                || (stage.prefix.is_some() && shapes[r.target].2 < shapes[r.source].2)
            {
                return Err(error("扫描的实际输出容量不满足输入域"));
            }
        }
        if let blanguage::ir::Operation::CustomScan(r) = &stage.operation
            && shapes[r.values.fields.target].2 < shapes[r.values.fields.source].2
        {
            return Err(error("自定义扫描的实际输出容量不足以容纳输入域"));
        }
        if let blanguage::ir::Operation::CustomSegmentedReduce(r) = &stage.operation
            && shapes[r.values.fields.target].2 < shapes[r.values.fields.source].2
        {
            return Err(error("自定义分段归约的实际输出容量不足以容纳输入段"));
        }
        if let blanguage::ir::Operation::SegmentedReduce(r) = &stage.operation {
            let fields = &r.values.fields;
            if shapes[fields.target].2 < shapes[fields.source].2 {
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
            words[((uniform.offset + offset) / 4) as usize] = value.bits();
            at += 1;
        }
    }
    Ok(words)
}
