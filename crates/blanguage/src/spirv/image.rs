//! 已编译GPU映像及页绑定ABI；编译器与执行器消费同一份表示。
use crate::ir::{ExecutionPlan, Operation};
use std::{ffi::c_void, ptr};

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

#[derive(Clone, Copy)]
#[repr(u32)]
pub(crate) enum EntryOperation {
    Map,
    Reduce,
    Scan,
    Segmented,
    Compact,
    Sort,
    CustomScan,
    CustomSegmented,
    CustomReduce,
}

pub(super) fn entry_operation<R>(operation: &Operation<R>) -> (EntryOperation, *const std::ffi::c_void) {
    use EntryOperation::*;
    match operation {
        Operation::Map => (Map, std::ptr::null()),
        Operation::Reduce(r) => (Reduce, std::ptr::from_ref(r).cast()),
        Operation::ScanExclusive(r) => (Scan, std::ptr::from_ref(r).cast()),
        Operation::SegmentedReduce(r) => (Segmented, std::ptr::from_ref(r).cast()),
        Operation::Compact(r) => (Compact, std::ptr::from_ref(r).cast()),
        Operation::StableSort(r) => (Sort, std::ptr::from_ref(r).cast()),
        Operation::CustomScan(r) => (CustomScan, std::ptr::from_ref(r).cast()),
        Operation::CustomSegmentedReduce(r) => (CustomSegmented, std::ptr::from_ref(r).cast()),
        Operation::CustomReduce(r) => (CustomReduce, std::ptr::from_ref(r).cast()),
    }
}

impl<R> From<&Operation<R>> for EntryOperation {
    fn from(operation: &Operation<R>) -> Self {
        entry_operation(operation).0
    }
}

pub fn sort_histogram_count(count: u32) -> u32 {
    unsafe { histogram_count(count, SCAN_WIDTH) }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ScanShape {
    operation: EntryOperation,
    selected: u32,
    count: u32,
    fixed: u32,
    width: u32,
}

impl ScanShape {
    fn new(operation: EntryOperation, selected: bool, count: Option<u32>) -> Self {
        Self {
            operation, selected: u32::from(selected),
            count: count.unwrap_or_default(), fixed: u32::from(count.is_some()), width: SCAN_WIDTH,
        }
    }
}

#[cfg(feature = "compiler")]
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Returned by the B scan-role query.
#[repr(u32)]
pub(crate) enum ScanShader { Summary, Terminal, Both }

#[cfg(feature = "compiler")]
#[repr(C)]
struct ScanRoles { root: ScanShader, tail: ScanShader }

#[cfg(feature = "compiler")]
pub(crate) fn scan_shaders(operation: EntryOperation, selected: bool, count: Option<u32>) -> [ScanShader; 2] {
    let roles = unsafe { scan_roles(ScanShape::new(operation, selected, count)) };
    [roles.root, roles.tail]
}

#[repr(C)]
struct ShaderWords { data: *const u32, count: usize }

impl From<&[u32]> for ShaderWords {
    fn from(words: &[u32]) -> Self { Self { data: words.as_ptr(), count: words.len() } }
}

#[repr(C)]
struct StageShaders {
    shape: ScanShape,
    sort_keys: usize,
    additional_count: usize,
    main: ShaderWords,
    tail: ShaderWords,
    selector: ShaderWords,
    binding: u32,
}

#[allow(dead_code)] // Returned by the B entry query.
#[repr(u32)]
enum TerminalKind { Absent, Canonical, Alternate }

#[repr(C)]
struct TerminalEntry { index: usize, kind: TerminalKind }

#[repr(C)]
struct AdditionalEntries {
    sort: usize,
    single_group: usize,
    root: TerminalEntry,
    tail: TerminalEntry,
}

unsafe extern "C" {
    #[link_name = "spirv.scan_roles::histogram_count"]
    fn histogram_count(count: u32, width: u32) -> u32;
    #[cfg(feature = "compiler")]
    #[link_name = "spirv.scan_roles::scan_roles"]
    fn scan_roles(shape: ScanShape) -> ScanRoles;
    #[link_name = "spirv.scan_roles::additional_entries"]
    fn additional_entries(input: StageShaders, context: *const c_void,
        additional: unsafe extern "C" fn(*const c_void, usize) -> ShaderWords) -> AdditionalEntries;
}

unsafe extern "C" fn additional_words(context: *const c_void, index: usize) -> ShaderWords {
    let shaders = unsafe { *context.cast::<&[ShaderCode]>() };
    shaders[index].words.as_slice().into()
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
        let shaders = self.additional.as_deref().unwrap_or_default();
        let input = StageShaders {
            shape: ScanShape::new((&stage.operation).into(), stage.selection.is_some(), plan.buffers()[stage.domain].count),
            sort_keys: match &stage.operation {
                Operation::StableSort(sort) => sort.keys.len(),
                _ => 0,
            },
            additional_count: shaders.len(),
            main: self.main.as_slice().into(),
            tail: self.tail.as_deref().unwrap_or_default().into(),
            selector: self.selector.as_deref().unwrap_or_default().into(),
            binding: (plan.buffers().len() + plan.accelerations().len() + plan.textures().len()) as u32,
        };
        let entries = unsafe { additional_entries(input, ptr::from_ref(&shaders).cast(), additional_words) };
        AdditionalShaders {
            sort: &shaders[..entries.sort],
            segmented_single_group: shaders.get(entries.single_group),
            terminal_scans: [entries.root, entries.tail].map(|entry| match entry.kind {
                TerminalKind::Absent => None,
                TerminalKind::Canonical => Some(TerminalScan::Canonical),
                TerminalKind::Alternate => Some(TerminalScan::Alternate(&shaders[entry.index])),
            }),
        }
    }
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
