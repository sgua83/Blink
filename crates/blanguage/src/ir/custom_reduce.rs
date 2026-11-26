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
pub struct Kernel<S> {
    pub stage: S,
    pub invariant_end: usize,
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
pub type ReductionKernel = Kernel<Stage>;

#[cfg(feature = "compiler")]
pub(crate) type PreparedReductionKernel = Kernel<ReductionBody>;

#[cfg(feature = "compiler")]
#[derive(Default)]
pub(crate) struct ReductionBody {
    pub(crate) registers: Vec<Scalar>,
    pub(crate) locals: BTreeMap<usize, Ty>,
    pub(crate) code: Vec<Inst>,
}

#[cfg(feature = "compiler")]
unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::reduction_prefix_definitions"]
    fn reduction_prefix_definitions(code: super::dataflow::Span, ir: super::dataflow::Host, definitions: *mut u8);
}

#[cfg(feature = "compiler")]
fn prefix_definitions(code: &[Inst], registers: usize) -> Vec<u8> {
    let mut definitions = vec![0; registers];
    unsafe {
        reduction_prefix_definitions(super::dataflow::Span::slice(code), super::dataflow::HOST, definitions.as_mut_ptr());
    }
    definitions
}

#[cfg(feature = "compiler")]
impl ReductionKernel {
    pub(crate) fn prefix_definitions(&self) -> Vec<u8> {
        prefix_definitions(&self.stage.code[..self.invariant_end], self.stage.registers.len())
    }
}

#[cfg(feature = "compiler")]
impl PreparedReductionKernel {
    pub(crate) fn prefix_definitions(&self) -> Vec<u8> {
        prefix_definitions(&self.stage.code[..self.invariant_end], self.stage.registers.len())
    }
}

#[cfg(feature = "compiler")]
impl CustomReduction {
    /// The original CPU and SPIR-V consumers share this B preparation.
    pub fn kernel(&self, stage: &Stage, project: bool, identity: bool) -> ReductionKernel {
        let mut inputs = Vec::new();
        let PreparedReductionKernel {
            stage: body, invariant_end, initialize_end, project_end, source, projected,
            initial, left, right, result,
        } = crate::optimize::reduction_kernel(self, project, identity, Some(&mut inputs));
        let generated_origin = if inputs.is_empty() {
            stage.generated_origin.clone()
        } else {
            Some(std::sync::Arc::new(GeneratedOrigin {
                reason: GenerationReason::LocalReadForward,
                trigger: Some(GenerationAnchor::ReductionInputForward {
                    retained: stage.generated_origin.clone(), inputs,
                }),
            }))
        };
        let stage = Stage {
            domain: stage.domain, prefix: stage.prefix, selection: stage.selection,
            operation: stage.operation.clone(), operation_source: stage.operation_source.clone(),
            generated_origin, boundary_before: stage.boundary_before,
            registers: body.registers, locals: body.locals, code: body.code,
            reads: stage.reads.clone(), writes: stage.writes.clone(), write_regions: stage.write_regions.clone(),
            prefix_guarantees: stage.prefix_guarantees.clone(), element_guarantees: stage.element_guarantees.clone(),
            cross_access: stage.cross_access.clone(), tiles: Vec::new(),
        };
        ReductionKernel { stage, invariant_end, initialize_end, project_end, source, projected, initial, left, right, result }
    }
}
