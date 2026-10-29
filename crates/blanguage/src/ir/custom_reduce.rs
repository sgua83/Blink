//! 值函数只有输入／输出寄存器及私有局部值，不持有数据资源。
use super::*;
#[derive(Clone, Debug)]
pub struct ValueCode {
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
impl CustomReduction {
    /// 两后端在编译期使用同一寄存器／局部值重定位；源 Program 保持不可变。
    pub fn kernel(&self, stage: &Stage, project: bool, identity: bool) -> ReductionKernel {
        let mut stage = stage.clone();
        stage.registers = if identity {
            self.identity.registers.clone()
        } else {
            Vec::new()
        };
        stage.locals = if identity {
            self.identity.locals.clone()
        } else {
            BTreeMap::new()
        };
        stage.code = if identity {
            self.identity.code.clone()
        } else {
            Vec::new()
        };
        let initialize_end = stage.code.len();
        let projection = self.project.as_ref().filter(|_| project).map(|value| {
            let reg = stage.registers.len();
            let slot = stage.locals.last_key_value().map_or(0, |(end, _)| *end);
            let mut code = value.code.clone();
            crate::optimize::map(&mut code, &|r| r + reg, &|s| s + slot);
            stage.code.extend(code);
            stage.registers.extend(&value.registers);
            stage.locals.extend(value.locals.iter().map(|(end, ty)| (end + slot, ty.clone())));
            (
                value.inputs.iter().map(|r| r + reg).collect(),
                value.outputs.iter().map(|r| r + reg).collect(),
            )
        });
        let project_end = stage.code.len();
        let reg = stage.registers.len();
        let slot = stage.locals.last_key_value().map_or(0, |(end, _)| *end);
        let mut combine = self.combine.code.clone();
        crate::optimize::map(&mut combine, &|r| r + reg, &|s| s + slot);
        stage.code.extend(combine);
        stage.registers.extend(&self.combine.registers);
        stage.locals.extend(self.combine.locals.iter().map(|(end, ty)| (end + slot, ty.clone())));
        let k = self.ty.components.len();
        let right: Vec<_> = self.combine.inputs[k..].iter().map(|r| r + reg).collect();
        let (source, projected) = projection.unwrap_or_else(|| (right.clone(), right.clone()));
        let mut kernel = ReductionKernel {
            stage,
            invariant_end: 0,
            invariants: std::collections::BTreeSet::new(),
            initialize_end,
            project_end,
            source,
            projected,
            initial: if identity {
                self.identity.outputs.clone()
            } else {
                Vec::new()
            },
            left: self.combine.inputs[..k].iter().map(|r| r + reg).collect(),
            right,
            result: self.combine.outputs.iter().map(|r| r + reg).collect(),
        };
        crate::optimize::prepare_reduction(&mut kernel);
        kernel
    }
}
