//! 原生执行只消费接口、访问依赖和调度布局；值函数与指令属于编译阶段。
use super::*;
use std::{collections::BTreeSet, ops::Deref};

/// A common field value in the original Scalar's ordered numeric encoding.
/// Binding and byte offset name the actual logical input, never an SSA value.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ElementRange {
    pub binding: usize,
    pub offset: u32,
    pub scalar: Scalar,
    pub intervals: Vec<(Bits, Bits)>,
    /// Fixed raw bits (mask, value), intersected with these ordered intervals.
    pub known_bits: (Bits, Bits),
}
impl ElementRange {
    pub fn within(&self, required: &Self) -> bool {
        let allowed = required.intervals.as_slice();
        let mut remaining = allowed;
        let mut lower = 0;
        self.intervals.iter().all(|&range| {
            if range.0 < lower {
                remaining = allowed;
            }
            lower = range.0;
            while let Some((&(start, end), rest)) = remaining.split_first()
                && end.max(start) < lower
            {
                remaining = rest;
            }
            self.scalar.covers_range(range, self.known_bits, remaining, required.known_bits)
        })
    }
    /// Intersect the two interval unions and fixed bits, keeping ordered, disjoint intervals.
    pub fn intersect(&mut self, other: &Self) {
        let conflict = (self.known_bits.1 ^ other.known_bits.1) & self.known_bits.0 & other.known_bits.0;
        self.known_bits = (self.known_bits.0 | other.known_bits.0, self.known_bits.1 | other.known_bits.1);
        if conflict != 0 || self.intervals.is_empty() || other.intervals.is_empty() {
            self.intervals.clear();
            return;
        }

        if !self.intervals.is_sorted() {
            self.intervals.sort_unstable();
        }
        let mut right = std::borrow::Cow::Borrowed(other.intervals.as_slice());
        if !right.is_sorted() {
            right.to_mut().sort_unstable();
        }

        let mut left = self.intervals.as_slice();
        let mut right = right.as_ref();
        let mut intervals: Vec<(Bits, Bits)> = Vec::new();
        while let (Some((&(a, b), rest_left)), Some((&(c, d), rest_right))) =
            (left.split_first(), right.split_first())
        {
            let (start, end) = (a.max(c), b.min(d));
            if start <= end {
                match intervals.last_mut() {
                    Some((_, upper)) if start <= upper.saturating_add(1) => *upper = (*upper).max(end),
                    _ => intervals.push((start, end)),
                }
            }
            if b <= d { left = rest_left; }
            if d <= b { right = rest_right; }
        }
        self.intervals = intervals;
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ElementFact {
    Range(ElementRange),
    Index { binding: usize, offset: u32, target: usize },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeviceRequirements {
    pub ray_query: bool, pub float64: bool,
    pub float32_preserve: bool, pub float64_preserve: bool,
    pub integer8: bool, pub integer16: bool,
    pub atomic64: bool, pub shared_atomic64: bool,
    pub sorting: bool,
    pub maximal_reconvergence: bool,
    pub texture_indexing: bool,
    pub position_fetch: bool,
    pub subgroup: SubgroupCapabilities,
}
pub trait ProgramInterface {
    fn name(&self) -> &str;
    fn target(&self) -> crate::ast::Target;
    fn buffers(&self) -> &[Buffer];
    fn accelerations(&self) -> &[String];
    fn textures(&self) -> &[TextureBinding];
    fn uniforms(&self) -> &[Uniform];
    fn uniform_bytes(&self) -> u32;
    fn prefix_requirements(&self) -> &[PrefixRelation];
    fn prefix_guarantees(&self) -> &[PrefixRelation];
    fn element_requirements(&self) -> &[ElementFact];
    fn element_guarantees(&self) -> &[ElementFact];
    fn requires_bindings(&self) -> bool {
        !self.prefix_requirements().is_empty() || !self.element_requirements().is_empty()
    }
    fn length_requirements(&self) -> &[(usize, usize)];
    fn same_length(&self, a: usize, b: usize) -> bool {
        let fixed = |a: usize, b: usize| a == b
            || self.buffers()[a].count.is_some() && self.buffers()[a].count == self.buffers()[b].count;
        let mut pending = vec![a];
        let mut visited = BTreeSet::new();
        while let Some(current) = pending.pop() {
            if fixed(current, b) { return true; }
            if !visited.insert(current) { continue; }
            for &(left, right) in self.length_requirements() {
                if fixed(current, left) { pending.push(right); }
                if fixed(current, right) { pending.push(left); }
            }
        }
        false
    }
    fn requirements(&self) -> DeviceRequirements;
}
macro_rules! interface_fields {
    ($vis:vis,) => {
        $vis fn name(&self) -> &str { &self.name }
        $vis fn target(&self) -> crate::ast::Target { self.target }
        $vis fn buffers(&self) -> &[Buffer] { &self.buffers }
        $vis fn accelerations(&self) -> &[String] { &self.accelerations }
        $vis fn textures(&self) -> &[TextureBinding] { &self.textures }
        $vis fn uniforms(&self) -> &[Uniform] { &self.uniforms }
        $vis fn uniform_bytes(&self) -> u32 { self.uniform_bytes }
        $vis fn prefix_requirements(&self) -> &[PrefixRelation] { &self.prefix_requirements }
        $vis fn prefix_guarantees(&self) -> &[PrefixRelation] { &self.prefix_guarantees }
        $vis fn element_requirements(&self) -> &[ElementFact] { &self.element_requirements }
        $vis fn element_guarantees(&self) -> &[ElementFact] { &self.element_guarantees }
        $vis fn length_requirements(&self) -> &[(usize, usize)] { &self.length_requirements }
    };
}
#[cfg(feature = "compiler")]
impl ProgramInterface for Program {
    interface_fields!(,);
    fn requirements(&self) -> DeviceRequirements {
        let (float64, integer8, integer16) = dataflow::numeric_requirements(self);
        DeviceRequirements {
            float64, integer8, integer16,
            ray_query: !self.accelerations.is_empty(),
            sorting: self.requires_sort(),
            texture_indexing: self.requires_texture_indexing(),
            position_fetch: self.requires_ray_position_fetch(),
            ..dataflow::program_code_requirements(self)
        }
    }
}
impl<T: Deref> ProgramInterface for T
where
    T::Target: ProgramInterface,
{
    fn name(&self) -> &str {
        self.deref().name()
    }
    fn target(&self) -> crate::ast::Target {
        self.deref().target()
    }
    fn buffers(&self) -> &[Buffer] {
        self.deref().buffers()
    }
    fn accelerations(&self) -> &[String] {
        self.deref().accelerations()
    }
    fn textures(&self) -> &[TextureBinding] {
        self.deref().textures()
    }
    fn uniforms(&self) -> &[Uniform] {
        self.deref().uniforms()
    }
    fn uniform_bytes(&self) -> u32 {
        self.deref().uniform_bytes()
    }
    fn prefix_requirements(&self) -> &[PrefixRelation] {
        self.deref().prefix_requirements()
    }
    fn prefix_guarantees(&self) -> &[PrefixRelation] {
        self.deref().prefix_guarantees()
    }
    fn element_requirements(&self) -> &[ElementFact] {
        self.deref().element_requirements()
    }
    fn element_guarantees(&self) -> &[ElementFact] {
        self.deref().element_guarantees()
    }
    fn length_requirements(&self) -> &[(usize, usize)] {
        self.deref().length_requirements()
    }
    fn requirements(&self) -> DeviceRequirements {
        self.deref().requirements()
    }
}
#[derive(Clone, Debug)]
pub struct ProgramLayout {
    pub write_regions: BTreeMap<usize, BTreeSet<(u32, u32)>>,
    pub name: String,
    pub target: crate::ast::Target,
    pub buffers: Vec<Buffer>,
    pub accelerations: Vec<String>,
    pub textures: Vec<TextureBinding>,
    pub uniforms: Vec<Uniform>,
    pub uniform_bytes: u32,
    pub prefix_requirements: Vec<PrefixRelation>,
    pub prefix_guarantees: Vec<PrefixRelation>,
    pub element_requirements: Vec<ElementFact>,
    pub element_guarantees: Vec<ElementFact>,
    pub length_requirements: Vec<(usize, usize)>,
    pub requirements: DeviceRequirements,
}
impl ProgramInterface for ProgramLayout {
    interface_fields!(,);
    fn requirements(&self) -> DeviceRequirements {
        self.requirements
    }
}
impl ProgramLayout {
    interface_fields!(pub,);
    pub fn requirements(&self) -> DeviceRequirements {
        self.requirements
    }
    pub fn texture_count(&self) -> usize {
        self.textures.iter().map(|t| t.elements() as usize).sum()
    }
    pub fn texture_offset(&self, binding: usize) -> usize {
        self.textures[..binding]
            .iter()
            .map(|t| t.elements() as usize)
            .sum()
    }
}
#[derive(Clone, Debug)]
pub struct ReductionLayout {
    pub fields: Reduction,
    pub ty: Ty,
}
impl custom_reduce::ReductionShape for ReductionLayout {
    fn value_type(&self) -> &Ty {
        &self.ty
    }
    fn fields(&self) -> &Reduction {
        &self.fields
    }
}
#[cfg(feature = "compiler")]
impl From<&CustomReduction> for ReductionLayout {
    fn from(value: &CustomReduction) -> Self {
        Self {
            fields: value.fields.clone(),
            ty: value.ty.clone(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct ExecutionStage {
    pub domain: usize,
    pub prefix: Option<PrefixCount>,
    pub selection: Option<Selection>,
    pub operation: Operation<ReductionLayout>,
    pub reads: Vec<usize>,
    pub writes: Vec<usize>,
    pub write_regions: BTreeMap<usize, BTreeSet<(u32, u32)>>,
    pub prefix_guarantees: Vec<PrefixRelation>,
    pub element_guarantees: Vec<ElementFact>,
    pub inputs: BTreeSet<(usize, u32)>,
    pub tile_scope: bool,
    pub width: u32,
    /// GPU各入口静态Workgroup声明总字节的峰值，由发射者交出；CPU映像不分配Workgroup。
    pub shared_bytes: u64,
}
impl ExecutionStage {
    #[cfg(feature = "compiler")]
    pub(crate) fn from_stage(stage: &Stage, inputs: BTreeSet<(usize, u32)>) -> Self {
        let operation = match &stage.operation {
            Operation::Map => Operation::Map,
            Operation::Reduce(r) => Operation::Reduce(r.clone()),
            Operation::CustomReduce(r) => Operation::CustomReduce(r.into()),
            Operation::CustomScan(r) => Operation::CustomScan(CustomScan {
                values: (&r.values).into(),
                head_word: r.head_word,
            }),
            Operation::CustomSegmentedReduce(r) => {
                Operation::CustomSegmentedReduce(CustomSegmentedReduction {
                    values: (&r.values).into(),
                    head_word: r.head_word,
                    count: r.count,
                })
            }
            Operation::SegmentedReduce(r) => Operation::SegmentedReduce(r.clone()),
            Operation::ScanExclusive(r) => Operation::ScanExclusive(r.clone()),
            Operation::Compact(c) => Operation::Compact(c.clone()),
            Operation::StableSort(s) => Operation::StableSort(s.clone()),
        };
        Self {
            domain: stage.domain,
            prefix: stage.prefix,
            selection: stage.selection,
            operation,
            reads: stage.reads.clone(),
            writes: stage.writes.clone(),
            write_regions: stage.write_regions.clone(),
            prefix_guarantees: stage.prefix_guarantees.clone(),
            element_guarantees: stage.element_guarantees.clone(),
            inputs,
            tile_scope: stage.has_tile_scope(),
            width: stage.workgroup_width(),
            shared_bytes: 0,
        }
    }

    pub fn has_tile_scope(&self) -> bool {
        self.tile_scope
    }
    pub fn workgroup_width(&self) -> u32 {
        self.width
    }
    pub fn shared_bytes(&self) -> u64 {
        self.shared_bytes
    }
    /// 借用本计划持有的实际输入字；集合不复制，寿命归原执行计划。
    pub fn input_words(&self) -> &BTreeSet<(usize, u32)> {
        &self.inputs
    }
}
#[derive(Clone, Debug)]
pub struct ExecutionPlan {
    pub interface: ProgramLayout,
    pub stages: Vec<ExecutionStage>,
    pub indirect_buffer: usize,
    pub auxiliary_buffers: usize,
    pub(crate) native_frame: Option<u32>,
}
impl Deref for ExecutionPlan {
    type Target = ProgramLayout;
    fn deref(&self) -> &Self::Target {
        &self.interface
    }
}
impl ExecutionPlan {
    /// 本次CPU范围私有Frame在原参数块中的真实字节位置；不属于用户uniform。
    pub fn native_frame(&self) -> Option<u32> { self.native_frame }
    pub fn stages(&self) -> &[ExecutionStage] {
        &self.stages
    }
    pub fn indirect_buffer(&self) -> usize {
        self.indirect_buffer
    }
    pub fn selection_buffer(&self) -> usize {
        self.indirect_buffer + 1
    }
    pub fn scan_source_buffer(&self) -> usize {
        self.selection_buffer() + 1
    }
    pub fn auxiliary_buffers(&self) -> usize {
        self.auxiliary_buffers
    }
    pub fn requires_sort(&self) -> bool {
        self.requirements().sorting
    }
    pub fn requires_ray_position_fetch(&self) -> bool {
        self.requirements().position_fetch
    }
    pub fn requires_maximal_reconvergence(&self) -> bool {
        self.requirements().maximal_reconvergence
    }
    pub fn requires_texture_indexing(&self) -> bool {
        self.requirements().texture_indexing
    }
    pub fn subgroup_capabilities(&self) -> SubgroupCapabilities {
        self.requirements().subgroup
    }
    #[cfg(feature = "compiler")]
    pub(crate) fn from_program(
        program: &Program, stages: Vec<ExecutionStage>, requirements: DeviceRequirements,
        buffers: ExecutionBuffers,
    ) -> Self {
        Self {
            interface: ProgramLayout {
                write_regions: program.write_regions(),
                name: program.name.clone(),
                target: program.target,
                buffers: program.buffers.clone(),
                accelerations: program.accelerations.clone(),
                textures: program.textures.clone(),
                uniforms: program.uniforms.clone(),
                uniform_bytes: program.uniform_bytes,
                prefix_requirements: program.prefix_requirements.clone(),
                prefix_guarantees: program.prefix_guarantees.clone(),
                element_requirements: program.element_requirements.clone(),
                element_guarantees: program.element_guarantees.clone(),
                length_requirements: program.length_requirements.clone(),
                requirements,
            },
            stages,
            indirect_buffer: buffers.indirect,
            auxiliary_buffers: buffers.auxiliary,
            native_frame: program.native_frame,
        }
    }
}
