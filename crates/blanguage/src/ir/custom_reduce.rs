//! 值函数只有输入／输出寄存器及私有局部值，不持有数据资源。
use super::*;
#[derive(Clone, Debug)]
pub struct ValueCode {
    #[cfg(feature = "compiler")]
    pub(crate) entry_origin: Option<std::sync::Arc<ValueCodeEntry>>,
    #[cfg(feature = "compiler")]
    pub(crate) generated_origin: Option<std::sync::Arc<GeneratedOrigin>>,
    pub registers: Vec<Scalar>,
    pub locals: BTreeMap<usize, Ty>,
    pub code: Vec<Inst>,
    pub inputs: Vec<Reg>,
    pub outputs: Vec<Reg>,
}
#[derive(Clone, Debug)]
pub struct CustomReduction {
    pub fields: Reduction,
    pub source_ty: Ty,
    pub ty: Ty,
    pub project: Option<ValueCode>,
    pub identity: ValueCode,
    pub combine: ValueCode,
}
#[derive(Clone, Debug)]
pub struct CustomScan<R = CustomReduction> {
    pub values: R,
    pub head_word: Option<u32>,
}
#[derive(Clone, Debug)]
pub struct CustomSegmentedReduction<R = CustomReduction> {
    pub values: R,
    pub head_word: u32,
    pub count: PrefixCount,
}
pub trait ReductionShape {
    fn value_type(&self) -> &Ty;
    fn fields(&self) -> &Reduction;
}
impl ReductionShape for CustomReduction {
    fn value_type(&self) -> &Ty {
        &self.ty
    }
    fn fields(&self) -> &Reduction {
        &self.fields
    }
}
impl<R: ReductionShape> CustomSegmentedReduction<R> {
    pub fn summary_words(&self) -> usize {
        self.values.value_type().size.div_ceil(4) as usize + 2
    }
}
impl<R: ReductionShape> CustomScan<R> {
    pub fn summary_words(&self) -> usize {
        self.values.value_type().size.div_ceil(4) as usize + usize::from(self.head_word.is_some())
    }
    pub fn needs_root_add(&self, program: &dyn ProgramInterface) -> bool {
        program.buffers()[self.values.fields().source]
            .count
            .is_none_or(|n| n > 64)
    }
}
#[cfg(feature = "compiler")]
pub struct ReductionKernel {
    pub stage: Stage,
    pub invariant_end: usize,
    pub(crate) invariants: std::collections::BTreeSet<Reg>,
    pub initialize_end: usize,
    pub project_end: usize,
    pub source: Vec<Reg>,
    pub projected: Vec<Reg>,
    pub initial: Vec<Reg>,
    pub left: Vec<Reg>,
    pub right: Vec<Reg>,
    pub result: Vec<Reg>,
}
#[cfg(feature = "compiler")]
impl CustomReduction {
    /// The original CPU and SPIR-V consumers share this B preparation.
    pub fn kernel(&self, stage: &Stage, project: bool, identity: bool) -> ReductionKernel {
        crate::optimize::reduction_kernel(self, stage, project, identity)
    }
}
