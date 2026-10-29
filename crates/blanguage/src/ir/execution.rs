//! 原生执行只消费接口、访问依赖和调度布局；值函数与指令属于编译阶段。
use super::*;
use std::{collections::BTreeSet, ops::Deref};

#[derive(Clone, Copy, Debug, Default)]
pub struct DeviceRequirements {
    pub ray_query: bool, pub float64: bool,
    pub integer8: bool, pub integer16: bool,
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
    };
}
impl ProgramInterface for Program {
    interface_fields!(,);
    fn requirements(&self) -> DeviceRequirements {
        let mut requirements = DeviceRequirements {
            ray_query: !self.accelerations.is_empty(),
            sorting: self.requires_sort(),
            maximal_reconvergence: self.requires_maximal_reconvergence(),
            texture_indexing: self.requires_texture_indexing(),
            position_fetch: self.requires_ray_position_fetch(),
            subgroup: self.subgroup_capabilities(),
            ..DeviceRequirements::default()
        };
        for scalar in self.buffers.iter().flat_map(|b| &b.ty.components).map(|(s, _)| s)
            .chain(self.uniforms.iter().flat_map(|b| &b.ty.components).map(|(s, _)| s))
            .chain(self.stages.iter().flat_map(|s| {
                let custom=match &s.operation {Operation::CustomReduce(r)|Operation::CustomScan(CustomScan{values:r,..})|Operation::CustomSegmentedReduce(CustomSegmentedReduction{values:r,..})=>Some(r),_=>None};
                s.registers.iter().chain(s.locals.values().flat_map(|ty|ty.components.iter().map(|(s,_)|s))).chain(custom.into_iter().flat_map(|r|[&r.identity,&r.combine].into_iter().chain(&r.project)).flat_map(|body|body.registers.iter().chain(body.locals.values().flat_map(|ty|ty.components.iter().map(|(s,_)|s)))))
            })) {
            requirements.integer8 |= scalar.size() == 1;
            requirements.integer16 |= scalar.size() == 2;
            requirements.float64 |= *scalar == Scalar::F64;
        }
        requirements
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
    fn requirements(&self) -> DeviceRequirements {
        self.deref().requirements()
    }
}
#[derive(Clone, Debug)]
pub struct ProgramLayout {
    pub name: String,
    pub target: crate::ast::Target,
    pub buffers: Vec<Buffer>,
    pub accelerations: Vec<String>,
    pub textures: Vec<TextureBinding>,
    pub uniforms: Vec<Uniform>,
    pub uniform_bytes: u32,
    pub prefix_requirements: Vec<PrefixRelation>,
    pub prefix_guarantees: Vec<PrefixRelation>,
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
    pub inputs: BTreeSet<(usize, u32)>,
    pub tile_scope: bool,
    pub width: u32,
    pub shared_bytes: u64,
    pub float32_preserve: bool, pub float64_preserve: bool,
}
impl ExecutionStage {
    pub fn has_tile_scope(&self) -> bool {
        self.tile_scope
    }
    pub fn workgroup_width(&self) -> u32 {
        self.width
    }
    pub fn shared_bytes(&self) -> u64 {
        self.shared_bytes
    }
    pub fn requires_float32_preserve(&self) -> bool {
        self.float32_preserve
    }
    pub fn input_words(&self) -> BTreeSet<(usize, u32)> {
        self.inputs.clone()
    }
}
#[derive(Clone, Debug)]
pub struct ExecutionPlan {
    pub interface: ProgramLayout,
    pub stages: Vec<ExecutionStage>,
    pub indirect_buffer: usize,
    pub auxiliary_buffers: usize,
}
impl Deref for ExecutionPlan {
    type Target = ProgramLayout;
    fn deref(&self) -> &Self::Target {
        &self.interface
    }
}
impl ExecutionPlan {
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
    pub(crate) fn from_program(program: &Program) -> Self {
        let stages = program
            .stages
            .iter()
            .map(|stage| {
                let shared_bytes = stage.tile_bytes().max(match &stage.operation {
                    Operation::Reduce(r) => u64::from(r.ty.size.div_ceil(4)) * 64 * 4,
                    Operation::CustomReduce(r) => u64::from(r.ty.size.div_ceil(4)) * 64 * 4,
                    Operation::ScanExclusive(r) => r.summary_words() as u64 * 64 * 4,
                    Operation::SegmentedReduce(r) => r.summary_words() as u64 * 64 * 4,
                    Operation::CustomScan(r) => r.summary_words() as u64 * 64 * 4,
                    Operation::CustomSegmentedReduce(r) => r.summary_words() as u64 * 64 * 4,
                    _ => 0,
                });
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
                ExecutionStage {
                    domain: stage.domain,
                    prefix: stage.prefix,
                    selection: stage.selection,
                    operation,
                    reads: stage.reads.clone(),
                    writes: stage.writes.clone(),
                    inputs: stage.input_words().into_keys().collect(),
                    tile_scope: stage.has_tile_scope(),
                    width: stage.workgroup_width(),
                    shared_bytes,
                    float32_preserve: stage.requires_float_preserve(Scalar::F32), float64_preserve: stage.requires_float_preserve(Scalar::F64),
                }
            })
            .collect();
        Self {
            interface: ProgramLayout {
                name: program.name.clone(),
                target: program.target,
                buffers: program.buffers.clone(),
                accelerations: program.accelerations.clone(),
                textures: program.textures.clone(),
                uniforms: program.uniforms.clone(),
                uniform_bytes: program.uniform_bytes,
                prefix_requirements: program.prefix_requirements.clone(),
                prefix_guarantees: program.prefix_guarantees.clone(),
                requirements: program.requirements(),
            },
            stages,
            indirect_buffer: program.indirect_buffer(),
            auxiliary_buffers: program.auxiliary_buffers(),
        }
    }
}
