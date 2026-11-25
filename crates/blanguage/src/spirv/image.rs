//! 已编译GPU映像及页绑定ABI；编译器与执行器消费同一份表示。
use crate::ir::{ExecutionPlan, Operation};

/// GPU 存储页的最大字节数；每页只容纳完整记录，页表使用 64 位设备地址。
pub const PAGE_BYTES: u32 = 256 * 1024 * 1024;
/// 页表绑定头为页数组地址、物理stride和字段offset；GPU view仍指向页数组。
pub const BINDING_BYTES: u32 = 16;

/// u32 元素域按 64 路分组递归至单组所需的最大层数。
pub const PREFIX_LEVELS: usize = u32::BITS.div_ceil(6) as usize;

#[derive(Clone, Debug)]
pub struct ShaderCode {
    pub words: Vec<u32>,
    pub ranks: bool,
}
#[derive(Clone, Debug)]
pub struct StageCode {
    pub main: Vec<u32>,
    pub tail: Option<Vec<u32>>,
    pub scan_add: Option<Vec<u32>>,
    pub finish: Option<(String, Vec<u32>)>,
    pub dispatch: Option<Vec<u32>>,
    pub selector: Option<Vec<u32>>,
    /// Original sort or single-group segmented entries, followed by the
    /// terminal scan alternatives that the stage's physical shape can use.
    pub additional: Option<Vec<ShaderCode>>,
}

pub const SCAN_WIDTH: u32 = 64;
/// Vulkan dispatch dimensions followed by one shader-side element count.
pub const DISPATCH_RECORD_BYTES: usize = 16;
/// Byte offset of the histogram or the selected domain in a preparation pipeline.
pub const DISPATCH_CONTINUATION_SPEC_ID: u32 = 0;

pub fn sort_histogram_count(count: u32) -> u32 {
    count.div_ceil(SCAN_WIDTH).max(1) * 16
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum ScanShader {
    Summary,
    Terminal,
    Both,
}

pub(crate) fn scan_shaders<R>(operation: &Operation<R>, selected: bool, count: Option<u32>) -> [ScanShader; 2] {
    use ScanShader::*;
    let (root, count) = match operation {
        Operation::StableSort(_) => (true, count.map(sort_histogram_count)),
        Operation::ScanExclusive(_) | Operation::CustomScan(_) | Operation::Compact(_) => (true, count),
        Operation::Map if selected => (true, count),
        Operation::SegmentedReduce(_) | Operation::CustomSegmentedReduce(_) => (false, count),
        _ => return [Summary; 2],
    };
    let first = match (root, count) {
        (true, None) => Both,
        (true, Some(size)) if size <= SCAN_WIDTH => Terminal,
        _ => Summary,
    };
    let tail = match count {
        None => Both,
        Some(size) if size > SCAN_WIDTH * SCAN_WIDTH => Both,
        Some(size) if size > SCAN_WIDTH => Terminal,
        _ => Summary,
    };
    [first, tail]
}

pub enum TerminalScan<'a> {
    Canonical,
    Alternate(&'a ShaderCode),
}

pub struct AdditionalShaders<'a> {
    pub sort: &'a [ShaderCode],
    pub segmented_single_group: Option<&'a ShaderCode>,
    /// Exclusive scan of the original records and of an upper level, respectively.
    pub terminal_scans: [Option<TerminalScan<'a>>; 2],
}

impl StageCode {
    pub fn additional_shaders(&self, plan: &ExecutionPlan, stage: &crate::ir::ExecutionStage) -> AdditionalShaders<'_> {
        let mut shaders = self.additional.as_deref().unwrap_or_default().iter();
        let operation = &stage.operation;
        let count = plan.buffers()[stage.domain].count;
        let sort = if let Operation::StableSort(sort) = operation {
            let size = 19 + sort.keys.len();
            let original = shaders.as_slice().get(..size).unwrap_or_default();
            shaders = shaders.as_slice().get(size..).unwrap_or_default().iter();
            original
        } else {
            &[]
        };
        let segmented_single_group = if matches!(operation,
            Operation::SegmentedReduce(_) | Operation::CustomSegmentedReduce(_))
            && count.is_none_or(|size| size <= SCAN_WIDTH)
        {
            shaders.next()
        } else {
            None
        };

        let root = match operation {
            Operation::StableSort(_) => sort.get(16).map(|shader| shader.words.as_slice()),
            Operation::Map if stage.selection.is_some() => self.selector.as_deref(),
            _ => Some(self.main.as_slice()),
        };
        let tail = if sort.is_empty() {
            self.tail.as_deref()
        } else {
            sort.get(17).map(|shader| shader.words.as_slice())
        };
        let binding = (plan.buffers().len() + plan.accelerations().len() + plan.textures().len()) as u32;
        let [root_role, tail_role] = scan_shaders(operation, stage.selection.is_some(), count);
        let terminal_scans = [(root_role, root), (tail_role, tail)].map(|(role, canonical)| match role {
                ScanShader::Summary => None,
                ScanShader::Terminal => canonical.filter(|words| !declares_binding(words, binding))
                    .map(|_| TerminalScan::Canonical),
                ScanShader::Both => shaders.next().map(TerminalScan::Alternate),
            });
        AdditionalShaders { sort, segmented_single_group, terminal_scans }
    }
}

// This reads the compiled page-binding declaration used by the original pipeline.
// Older images declared the auxiliary slot even in a statically single-group scan.
fn declares_binding(words: &[u32], binding: u32) -> bool {
    let Some(mut instructions) = words.get(5..) else { return true };
    while let Some(&header) = instructions.first() {
        let width = (header >> 16) as usize;
        let Some((instruction, rest)) = instructions.split_at_checked(width) else { return true };
        match (header as u16, instruction) {
            (54, _) => break, // OpFunction ends module declarations.
            (71, [_, _, 33, slot]) if *slot == binding => return true, // OpDecorate Binding
            (_, []) => return true,
            _ => {}
        }
        instructions = rest;
    }
    false
}

#[derive(Clone, Debug)]
pub struct ProgramImage<P> {
    program: std::sync::Arc<ExecutionPlan>,
    marker: std::marker::PhantomData<fn() -> P>,
    stages: Vec<StageCode>,
}
impl<P> ProgramImage<P> {
    /// # Safety
    /// The original image must satisfy the entry requirements of `Q`.
    #[cfg(feature = "compiler")]
    pub(crate) unsafe fn with_entry_type<Q>(self) -> ProgramImage<Q> {
        let Self { program, marker: _, stages } = self;
        ProgramImage { program, marker: std::marker::PhantomData, stages }
    }

    /// 恢复本版本编译器输出的 SPIR-V 与执行布局。
    ///
    /// # Safety
    /// 必须来自本版本编译器的同一个 P 入口，包含完整未修改的 Shader、
    /// 调度、设备需求及前缀关系；不得由宿主输入构造或伪造条件模板的证明。
    #[doc(hidden)]
    pub unsafe fn from_compiler_output(program: impl Into<std::sync::Arc<ExecutionPlan>>, stages: Vec<StageCode>) -> Self {
        Self {
            program: program.into(),
            stages,
            marker: std::marker::PhantomData,
        }
    }
    pub fn program(&self) -> &ExecutionPlan {
        &self.program
    }
    pub fn shared_program(&self) -> std::sync::Arc<ExecutionPlan> {
        self.program.clone()
    }
    pub fn stages(&self) -> &[StageCode] {
        &self.stages
    }
    pub fn into_parts(self) -> (std::sync::Arc<ExecutionPlan>, Vec<StageCode>) {
        (self.program, self.stages)
    }
}
