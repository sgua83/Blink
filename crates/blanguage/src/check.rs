use crate::{
    Definition, World,
    ast::*,
    ir::{self, *},
};
mod append;
mod address;
mod array;
mod atomic;
mod bounds;
mod compact;
mod callback;
mod constant;
pub(crate) use constant::constant_value;
pub(crate) use constant::variant_discriminant;
mod application;
mod contracts;
mod cooperative;
mod custom_reduce;
mod effects;
pub(crate) mod external;
mod native_function;
mod residency;
pub(crate) use native_function::{check_domain, compile_module as compile_native_module};
mod flow;
pub(crate) mod generics;
mod matrix;
mod operators;
mod owned_records;
mod user_owned;
mod owned_borrows;
mod owned_presence;
mod owned_values;
mod ownership;
pub(crate) mod outputs;
mod prefix;
mod records;
mod reduce;
mod results;
mod returns;
mod scan;
mod segmented_reduce;
mod selection;
mod sort;
mod subgroup;
pub(crate) use application::compile_application;
mod texture;
mod tile;
mod view;
mod view_liveness;
mod view_contracts;
use flow::{Flow, LoopContext};
use ownership::Position;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) fn member_type(ty: &Ty, field: &str, source: &crate::Source, span: Span) -> Result<(Ty, Vec<usize>)> {
    if ty.has_variants() {
        return source.fail(
            span,
            "B269",
            "结果载荷通过 match 或 try 使用，不公开原始判别字／载荷字段",
        );
    }
    if ty.view.is_some() {
        return if field == "length" {
            Ok((Ty::scalar(Scalar::U32), vec![2]))
        } else {
            source.fail(span, "B185", "view 只公开只读 length，不公开设备地址分量")
        };
    }
    if let Some((_, offset, sub)) = ty.fields.iter().find(|(n, _, _)| n == field) {
        let indices = sub
            .components
            .iter()
            .map(|(_, o)| {
                ty.components
                    .iter()
                    .position(|(_, v)| *v == offset + o)
                    .unwrap()
            })
            .collect();
        return Ok(((**sub).clone(), indices));
    }
    if !ty.is_handle() && ty.array.is_none() && ty.fields.is_empty() && ty.components.len() > 1 && field.len() <= 4
    {
        let mut indices = Vec::new();
        for c in field.chars() {
            let i = match c {
                'x' | 'r' => 0,
                'y' | 'g' => 1,
                'z' | 'b' => 2,
                'w' | 'a' => 3,
                _ => return source.fail(span, "B139", format!("未知字段 {field}")),
            };
            if i >= ty.components.len() {
                return source.fail(span, "B140", "向量分量越界");
            }
            indices.push(i);
        }
        return Ok((Ty::vector(ty.components[0].0, indices.len()), indices));
    }
    source.fail(span, "B141", format!("{} 没有字段 {field}", ty.name))
}

#[derive(Clone)]
enum Root {
    Data(usize, Access),
    Acceleration(usize),
    Texture(usize),
    Uniform(Ty, u32),
    Literal(Ty, Vec<Bits>, Option<Arc<GeneratedOrigin>>),
}
impl Root {
    fn reference_literal(self, reference: Option<Arc<SourceSite>>) -> Self {
        match self {
            Self::Literal(ty, bits, retained) => Self::Literal(ty, bits, Some(Arc::new(GeneratedOrigin {
                reason: GenerationReason::StageConstantReference,
                trigger: Some(GenerationAnchor::StageConstantReference { reference, retained }),
            }))),
            root => root,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ValueIdentity {
    Register(Reg),
    // 资源的长度身份不随每次装载而变化；只存在于编译期。
    DataLength(usize),
    TileLength,
    // 原生位置参数入口的内容身份，与传递地址的机器寄存器分开。
    ParameterContent(usize, usize),
    // 资源叶输入与重叠载荷中的view值分开命名，不占机器参数。
    ResourceParameter(usize, usize),
    // Semantic storage input, separate from native pointer bits and parameter contents.
    ParameterStorage(usize),
    ElementComponent(usize),
    ElementParent(usize, usize),
    // Bound ordinal in the original Data.elements scope, never a machine register.
    ElementIndex(usize),
    DatasetContent(usize, usize, usize),
    // The numeric storage snapshot produced by this actual call, not a machine argument.
    MemoryValue(usize, usize),
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum IndexDomain {
    Data(usize),
    Fixed(u32),
    Tile,
    DataLength(usize),
    TileLength,
    View([ValueIdentity; 3]),
    ViewLength([ValueIdentity; 3]),
    ApplicationData(ValueIdentity),
    ApplicationLength(ValueIdentity),
    // 无符号整数关系；距离按数学整数解释，不进行机器回绕。
    Upper(ValueIdentity, u32), // value + distance <= limit
    Lower(ValueIdentity, u32), // value >= base + distance
    Room(ValueIdentity), // value + base <= u32::MAX
    Sum(ValueIdentity, ValueIdentity), // value == left + right，已证明不回绕
    Offset(ValueIdentity, u32), // value == base + amount，已证明不回绕
    Shifted(ValueIdentity, ValueIdentity, u32), // value + distance <= base + limit
    Computed(bounds::IntegerExpression),
    // Selected numeric snapshots inherit common bounds, never common value identity.
    CommonRange(ValueIdentity),
}
type Domains = BTreeSet<IndexDomain>;
#[derive(Clone)]
struct Value {
    proof: bounds::Proof,
    // 尚未由绑定、参数、返回值或显式构造固定类型的数值字面量表达式。
    literal: bool,
    ty: Ty,
    regs: Vec<Reg>,
    uniform: bool,
    constant: Option<Vec<Bits>>,
}
#[derive(Clone)]
enum Binding {
    DataReference(application::DataReference),
    ApplicationData(application::Data),
    OwnedValue(owned_values::OwnedValue),
    AtomicBorrow(Place),
    Owned(OwnedBinding),
    Value(Value),
    Data(DatasetView),
    Tile(TileView),
    ViewElement {
        view: Value,
        element: OwnedBinding,
    },
    Acceleration(usize),
    Texture(texture::TextureReference),
}
#[derive(Clone)]
struct OwnedBinding {
    place: Place,
    proof: bounds::Proof,
    uniform: bool,
    initialization: Option<Vec<Position>>,
}
#[derive(Clone)]
struct DatasetView {
    binding: usize,
    access: Access,
    whole: bool,
    projection: Vec<usize>,
    length: bounds::Proof,
}
#[derive(Clone)]
struct TileView {
    tile: usize,
    ty: Ty,
    words: Vec<u32>,
    length: bounds::Proof,
}
#[derive(Clone)]
enum CursorSource {
    ApplicationData(application::DataReference),
    Data(DatasetView),
    Tile(TileView),
    View(Value),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum TileWidth {
    Fixed(u32),
    Parameter,
}
#[derive(Clone)]
struct ForLoop {
    name: String,
    begin: Value,
    limit: Value,
    data: Option<CursorSource>,
    borrowed: Option<ValueIdentity>,
    slot: usize,
    initialization: Vec<(application::Data, Vec<u32>)>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum TileOrigin {
    Bound(TileSource),
    Parameter,
}
struct CheckedTile {
    source: TileOrigin,
    ty: Ty,
    width: TileWidth,
    access: Access,
}
#[derive(Clone, Eq)]
enum ReferenceAddress {
    Contiguous([Reg; 2]),
    Components(BTreeMap<u32, [Reg; 2]>),
    External { address: [Reg; 2], identity: address::Identity },
    Borrowed { address: [Reg;2], identity: address::Identity, borrow:view::Borrow, layout:address::Layout },
}
impl ReferenceAddress {
    fn component(&self, word: u32) -> ([Reg; 2], u32) {
        match self {
            Self::Contiguous(address) | Self::External { address, .. } | Self::Borrowed { address, .. } => (*address, word),
            Self::Components(addresses) => (addresses[&word], 0),
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
enum Place {
    Reference {
        ty: Ty,
        address: ReferenceAddress,
        scope: Option<usize>,
        words: Vec<u32>,
        access: Access,
    },
    ApplicationElement {
        ty: Ty,
        handle: Reg,
        identity: ValueIdentity,
        sources: Vec<ValueIdentity>,
        resource: view::Borrow,
        initialization: Option<(Position, ownership::ApplicationIndex)>,
        index: Reg,
        index_value: bounds::Operand,
        parent: Option<Box<Place>>,
        element_layout: address::Layout,
        index_range: (u32, u32),
        stride: application::DataStride,
        words: Vec<u32>,
        access: Access,
    },
    // 独立函数检查的符号位置；program 内联直接借用调用方的实际 Place。
    AtomicParameter(Ty),
    ViewElement {
        ty: Ty,
        source: [ValueIdentity; 3],
        address: [Reg; 2],
        index: Reg,
        stride: ElementStride,
        words: Vec<u32>,
    },
    TileElement {
        ty: Ty,
        tile: usize,
        index: Reg,
        words: Vec<u32>,
    },
    Local(Ty, Vec<usize>, std::ops::Range<usize>),
    ReadOnlyLocal(Ty, Vec<usize>, std::ops::Range<usize>),
    Indexed {
        ty: Ty,
        initialization: Vec<Position>,
        locations: Vec<Position>,
        base: Box<Place>,
        offset: Reg,
        words: Vec<u32>,
    },
    Element {
        ty: Ty,
        binding: usize,
        index: Reg,
        words: Vec<u32>,
        writable: bool,
    },
}

struct DomainCall {
    stage: usize,
    instance: crate::generics::Instance,
    arguments: Vec<Root>,
    write_regions: BTreeMap<usize, BTreeSet<(u32, u32)>>,
    before: Vec<PrefixRelation>,
    after: Vec<PrefixRelation>,
    source: Option<Arc<SourceSite>>,
}
struct ExpandedDomain {
    program: Program,
    calls: Vec<DomainCall>,
    outputs: Vec<crate::module::domain::Output>,
}
pub(crate) fn compile_program(
    world: &World,
    instance: &crate::generics::Instance,
) -> Result<Program> {
    let mut functions = native_function::Module::default();
    let expanded = expand_domain(world, instance, &mut functions)?;
    if !expanded.calls.is_empty() {
        return instance.definition.source.fail(
            instance.definition.function.span,
            "B270",
            "跨对象域使用原生模块或应用入口编译；单个内核映像不包含其他模块的实现",
        );
    }
    native_function::finish_program(functions, expanded.program)
}
fn expand_domain(world: &World, instance: &crate::generics::Instance, functions: &mut native_function::Module) -> Result<ExpandedDomain> {
    let name = &instance.name;
    let d = &instance.definition;
    if !kernel_interface(world, d)? {
        return d.source.fail(
            d.function.span,
            "B266",
            "算法对象接口以借用数据与普通统一值交付结果；普通函数的独立调用／资源 ABI 产物尚未实现",
        );
    }
    let mut buffers = Vec::new();
    let mut accelerations = Vec::new();
    let mut textures = Vec::new();
    let mut uniforms = Vec::new();
    let mut env = BTreeMap::new();
    let mut offset = 0;
    for p in &d.function.params {
        if p.ty.is_texture_resource() {
            if p.access != Access::Read {
                return d
                    .source
                    .fail(p.span, "B239", "texture2d／texture3d 必须声明 read 资源权限");
            }
            env.insert(p.name.clone(), Root::Texture(textures.len()));
            textures.push(TextureBinding {
                name: p.name.clone(),
                count: texture::shape(world, &p.ty, d, p.span)?,
                dimension: p.ty.texture_dimension().unwrap(),
            });
            continue;
        }
        if p.ty.is_acceleration() {
            if p.access != Access::Read || d.function.target != Target::Gpu {
                return d.source.fail(
                    p.span,
                    "B182",
                    "acceleration 必须是 GPU program 的 read 资源",
                );
            }
            env.insert(p.name.clone(), Root::Acceleration(accelerations.len()));
            accelerations.push(p.name.clone());
            continue;
        }
        let ty = world.ty(&p.ty, &d.source, p.span, &d.module, &d.imports)?;
        if ty.owns_resources() {
            return d.source.fail(
                p.span,
                "B224",
                "拥有结果属于CPU资源组织，不能成为program的普通存储或uniform",
            );
        }
        if ty.contains_views() && (!p.ty.dataset || !matches!(p.access, Access::Read | Access::InOut)) {
            return d.source.fail(
                p.span,
                "B185",
                "含view的整域根需要read／inout dataset；write／atomic或uniform不能构造引用图",
            );
        }
        if p.ty.dataset {
            if p.access == Access::Value {
                return d.source.fail(
                    p.span,
                    "B101",
                    "dataset 必须声明 read、write、inout 或 atomic",
                );
            }
            let binding = buffers.len();
            let stride = ir::align_up(ty.size, ty.align);
            buffers.push(Buffer {
                name: p.name.clone(),
                access: p.access,
                ty,
                stride,
                count: world.shape(&p.ty.count, &d.source, p.span, &d.module, &d.imports)?,
            });
            env.insert(p.name.clone(), Root::Data(binding, p.access));
        } else {
            if p.access != Access::Value {
                return d
                    .source
                    .fail(p.span, "B102", "标量参数按值传递；访问权限只用于 dataset");
            }
            uniforms.push(Uniform {
                name: p.name.clone(),
                ty,
                offset: 0,
            });
        }
    }
    // word 0 为阶段长度，word 1 为设备分区起点，其后是各数据集长度。
    offset += 4 * (2 + buffers.len() as u32);
    for u in &mut uniforms {
        offset = ir::align_up(offset, u.ty.align);
        u.offset = offset;
        offset += u.ty.size;
        env.insert(u.name.clone(), Root::Uniform(u.ty.clone(), u.offset));
    }
    offset = ir::align_up(offset, 4);
    if offset > world.worker.compiler().max_parameter_bytes() {
        return d.source.fail(
            d.function.span,
            "B103",
            format!("程序参数占 {offset} 字节，超过本次编译配置的 {} 字节参数上限；此处未查询实际设备能力", world.worker.compiler().max_parameter_bytes()),
        );
    }
    let mut program = Program {
        name: name.clone(),
        source_origin: d.source_origin(),
        generated_origin: None,
        target: d.function.target,
        buffers,
        accelerations,
        textures,
        uniforms,
        uniform_bytes: offset,
        source_stages: 0,
        stages: Vec::new(),
        functions: Arc::from([]),
        prefix_requirements: Vec::new(),
        prefix_guarantees: Vec::new(), length_requirements: Vec::new(), imports: BTreeMap::new(),
    };
    program.length_requirements = contracts::root_lengths(&env, d)?;
    program.prefix_requirements = prefix::relations(&program, &env, d, ContractKind::Requires)?;
    program.prefix_guarantees = prefix::relations(&program, &env, d, ContractKind::Ensures)?;
    let mut facts = prefix::Facts::new(&program.prefix_requirements);
    let mut calls = Vec::new();
    expand_program(
        world,
        instance,
        &mut env,
        &mut program,
        &mut Vec::new(),
        &mut false,
        &mut facts,
        &mut calls,
        instance.definition.syntax_context(Some(&instance.name)).1.domain_path.clone(),
        functions,
    )?;
    program.source_stages = program.stages.len();
    Ok(ExpandedDomain { program, calls, outputs: facts.outputs })
}
fn expand_program(
    world: &World,
    instance: &crate::generics::Instance,
    env: &mut BTreeMap<String, Root>,
    program: &mut Program,
    stack: &mut Vec<String>,
    boundary: &mut bool,
    facts: &mut prefix::Facts,
    calls: &mut Vec<DomainCall>,
    domain_path: Arc<[DomainSite]>,
    functions: &mut native_function::Module,
) -> Result<()> {
    let d = &instance.definition;
    let key = format!("{}::{}", d.module, d.function.name);
    if stack.contains(&key) {
        return d
            .source
            .fail(d.function.span, "B105", "计算程序递归调用没有静态阶段边界");
    }
    stack.push(key);
    expand_stages(
        world,
        instance,
        d.function.body.as_ref().ok_or_else(|| Diagnostic {
            source: d.source.clone(), span: d.function.span, code: "B270",
            message: "已编译整域入口没有可展开的实现体；CPU应用或CPU包装函数沿原生入口调用，域内部的已编译域组合尚未实现".into(),
        })?,
        env,
        program,
        stack,
        boundary,
        facts,
        calls,
        &domain_path,
        functions,
    )?;
    for contract in d
        .function
        .contracts
        .iter()
        .filter(|c| c.kind == ContractKind::Ensures)
    {
        let relation = prefix::relation(program, env, d, contract)?;
        if !facts.holds(relation) {
            return d.source.fail(
                contract.span,
                "B193",
                "ensures prefix 未由本函数当前生产证明或有效输入能力支持",
            );
        }
    }
    stack.pop();
    Ok(())
}
fn operation_source(instance: &crate::generics::Instance, span: Span, syntax: SourceSyntax, domain_path: &Arc<[DomainSite]>) -> Option<Arc<SourceSite>> {
    let (origin, context) = instance.definition.syntax_context(Some(&instance.name));
    let context = Arc::new(SourceContext { domain_path: domain_path.clone(), ..(*context).clone() });
    origin.map(|origin| Arc::new(SourceSite { origin, context, span, syntax }))
}
fn expand_stages(
    world: &World,
    instance: &crate::generics::Instance,
    body: &[Stmt],
    env: &mut BTreeMap<String, Root>,
    program: &mut Program,
    stack: &mut Vec<String>,
    boundary: &mut bool,
    facts: &mut prefix::Facts,
    calls: &mut Vec<DomainCall>,
    domain_path: &Arc<[DomainSite]>,
    functions: &mut native_function::Module,
) -> Result<bool> {
    let d = &instance.definition;
    for statement in body {
        for root in env.values_mut() {
            if let Root::Data(binding, access @ Access::Write) = root
                && program.buffers[*binding].ty.components.iter().all(|(_, offset)| facts.initialized(program, *binding, *offset))
            {
                *access = Access::InOut;
            }
        }
        let mut first_stage = program.stages.len();
        match &statement.kind {
            StmtKind::Return(None) => return Ok(true),
            StmtKind::Let(annotation, name, expression) => {
                if env.contains_key(name) {
                    return d.source.fail(statement.span, "B125", format!("重复局部变量或覆盖参数 {name}"));
                }
                let value = constant::stage_expression(
                    world, instance, env, program, expression, annotation.as_ref(), domain_path,
                )?;
                env.insert(name.clone(), value);
            }
            StmtKind::If(condition, yes, no) => {
                let Root::Literal(ty, words, evaluation) = stage_root(world, instance, env, program, condition, domain_path)? else {
                    return d.source.fail(condition.span, "B251", "阶段 if 条件必须是编译期 bool；动态条件放在 parallel 或CPU主流程中");
                };
                if ty != Ty::scalar(Scalar::Bool) {
                    return d.source.fail(condition.span, "B251", "阶段 if 条件必须是编译期 bool");
                }
                let selected = if words[0] == 0 { no } else { yes };
                let mut child = env.clone();
                let first_call = calls.len();
                let returned = expand_stages(world, instance, selected, &mut child, program, stack, boundary, facts, calls, domain_path, functions)?;
                program.generated_origin = Some(Arc::new(GeneratedOrigin {
                    reason: GenerationReason::StageIfSelection,
                    trigger: Some(GenerationAnchor::StageIfSelection {
                        condition: operation_source(instance, condition.span, SourceSyntax::Expression, domain_path),
                        value: words[0] != 0, evaluation, returned,
                        stages: program.stages[first_stage..].iter().map(|stage| OperationOrigin {
                            source_site: stage.operation_source.clone(), generated_origin: stage.generated_origin.clone(),
                        }).collect(),
                        calls: calls[first_call..].iter().map(|call| OperationOrigin {
                            source_site: call.source.clone(), generated_origin: None,
                        }).collect(),
                        retained: program.generated_origin.clone(),
                    }),
                }));
                if returned {
                    return Ok(true);
                }
                first_stage = program.stages.len();
            }
            StmtKind::For(name, first, Some(last), body) => {
                if env.contains_key(name) {
                    return d.source.fail(
                        statement.span,
                        "B125",
                        format!("重复阶段 for 计数器或覆盖参数 {name}"),
                    );
                }
                let bound = |expr: &Expr| -> Result<StageForBound> {
                    match stage_root(world, instance, env, program, expr, domain_path)? {
                        Root::Literal(ty, words, evaluation) if ty == Ty::scalar(Scalar::U32) => Ok(StageForBound {
                            expression: operation_source(instance, expr.span, SourceSyntax::Expression, domain_path),
                            value: words[0] as u32, evaluation,
                        }),
                        _ => d.source.fail(
                            expr.span,
                            "B251",
                            "program 阶段 for 的范围必须是编译期 u32；动态循环放在 parallel 中",
                        ),
                    }
                };
                let first = bound(first)?;
                let last = bound(last)?;
                let retained = program.generated_origin.take();
                let mut iterations = Vec::new();
                let mut returned = false;
                for index in first.value..last.value {
                    let first_stage = program.stages.len();
                    let first_call = calls.len();
                    let mut child = env.clone();
                    child.insert(
                        name.clone(),
                        Root::Literal(Ty::scalar(Scalar::U32), vec![Bits::from(index)], None),
                    );
                    returned = expand_stages(
                        world, instance, body, &mut child, program, stack, boundary, facts, calls, domain_path, functions,
                    )?;
                    iterations.push(StageForIteration {
                        index, returned,
                        stages: program.stages[first_stage..].iter().map(|stage| OperationOrigin {
                            source_site: stage.operation_source.clone(), generated_origin: stage.generated_origin.clone(),
                        }).collect(),
                        calls: calls[first_call..].iter().map(|call| OperationOrigin {
                            source_site: call.source.clone(), generated_origin: None,
                        }).collect(),
                        expansion: program.generated_origin.take(),
                    });
                    if returned { break; }
                }
                program.generated_origin = Some(Arc::new(GeneratedOrigin {
                    reason: GenerationReason::StageForExpansion,
                    trigger: Some(GenerationAnchor::StageForExpansion {
                        statement: operation_source(instance, statement.span, SourceSyntax::Statement, domain_path),
                        counter: name.as_str().into(), first, last, iterations, retained,
                    }),
                }));
                if returned {
                    return Ok(true);
                }
                first_stage = program.stages.len();
            }
            StmtKind::Parallel(name, syntax, body) => {
                let data = &syntax.dataset;
                let Some(Root::Data(domain, domain_access)) = env.get(data) else {
                    return d
                        .source
                        .fail(statement.span, "B106", format!("{data} 不是数据集参数"));
                };
                let prefix = syntax
                    .count
                    .as_ref()
                    .map(|expr| prefix::resolve(program, env, *domain, expr, d, facts))
                    .transpose()?;
                let selection = syntax
                    .selection
                    .as_ref()
                    .map(|expr| {
                        let (binding, word, access) =
                            collective_field(program, env, expr, d, Scalar::Bool)?;
                        if !matches!(access, Access::Read | Access::InOut)
                            && !(access == Access::Write && facts.initialized(program, binding, word))
                        {
                            return d.source.fail(
                                expr.span,
                                "B170",
                                "where 谓词必须是只读 bool 字段，或当前 inout 独占域的 bool 字段",
                            );
                        }
                        if let (Some(domain_count), Some(flags_count)) = (
                            program.buffers[*domain].count,
                            program.buffers[binding].count,
                        ) && flags_count < domain_count
                        {
                            return d.source.fail(
                                expr.span,
                                "B187",
                                "where 谓词容量不能小于原始域",
                            );
                        }
                        Ok(Selection { binding, word })
                    })
                    .transpose()?;
                let mut b = Builder::new(world, d, program, *domain, Some(&instance.name))?;
                b.source_context = Arc::new(SourceContext { domain_path: domain_path.clone(), ..(*b.source_context).clone() });
                b.functions = std::mem::take(functions);
                b.roots(env);
                let operation_source = operation_source(instance, statement.span, SourceSyntax::Statement, domain_path);
                let index = b.reg(Scalar::U32);
                b.code.push(Inst::generated(InstKind::Index(index), Arc::new(GeneratedOrigin {
                    reason: GenerationReason::ParallelIndex, trigger: operation_source.clone().map(GenerationAnchor::Syntax),
                })));
                b.global_index = index;
                b.refine_parallel_domain();
                if b.env.contains_key(name) {
                    return d
                        .source
                        .fail(statement.span, "B107", "迭代变量不能覆盖参数");
                }
                let ty = program.buffers[*domain].ty.clone();
                let words = ty.components.iter().map(|x| x.1).collect();
                let element = Binding::Owned(OwnedBinding {
                        place: Place::Element {
                            ty,
                            binding: *domain,
                            index,
                            words,
                            writable: matches!(domain_access, Access::InOut | Access::Write),
                        },
                        proof: if *domain_access == Access::Atomic { bounds::Proof::default() }
                            else { facts.elements.get(domain).cloned().unwrap_or_default() },
                        uniform: false,
                        initialization: None,
                    });
                // 输入内容与前序阶段的真实输出共同提供当前元素初值。
                for (binding, access) in &b.view_access {
                    b.initialized.extend(program.buffers[*binding].ty.components.iter()
                        .filter(|(_, offset)| access.reads() || facts.initialized(program, *binding, *offset))
                        .map(|(_, offset)| Position::Element(*binding, *offset)));
                }
                b.env.insert(name.clone(), element);
                if let Some(limit) = &prefix {
                    b.reads.insert(limit.binding);
                    b.cross_access.insert(limit.binding);
                }
                b.prefix = prefix;
                let inputs = b.tile_inputs(body)?;
                b.block(&body[inputs..])?;
                b.parallel_access(statement.span)?;
                let appended: Vec<_> = b
                    .appends
                    .iter()
                    .map(|(domain, count)| PrefixRelation {
                        domain: *domain,
                        count: *count,
                    })
                    .collect();
                for relation in &appended {
                    prefix::prove(
                        facts,
                        relation.domain,
                        relation.count,
                        &d.source,
                        statement.span,
                    )?;
                }
                if prefix.is_none() && selection.is_none() {
                    for &binding in &b.writes {
                        if b.view_access[&binding] == Access::Write
                            && !b.appends.iter().any(|(output, count)| *output == binding || count.binding == binding)
                        {
                            let words: Vec<_> = program.buffers[binding].ty.components.iter()
                                .filter(|(_, offset)| b.initialized.contains(&Position::Element(binding, *offset)) && !facts.initialized(program, binding, *offset))
                                .map(|(_, offset)| *offset).collect();
                            if !words.is_empty() {
                                facts.outputs.push(crate::module::domain::Output { binding, domain: Some(*domain), words });
                            }
                        }
                    }
                }
                let tiles = b.lower_tiles(statement.span)?;
                if selection.is_some() && !tiles.is_empty() {
                    return d.source.fail(
                        statement.span,
                        "B250",
                        "where 稀疏域尚未定义 tile 邻接与发布语义",
                    );
                }
                if let Some(selection) = selection {
                    b.reads.insert(selection.binding);
                    b.cross_access.insert(selection.binding);
                }
                let element_ranges = if prefix.is_none() && selection.is_none()
                    && b.view_access[domain] != Access::Atomic
                    && !b.appends.iter().any(|(output, count)| output == domain || count.binding == *domain)
                {
                    b.env.get_mut(name).and_then(Binding::proof_storage)
                        .map(|(proof, _, _)| proof.scalar_ranges())
                } else { None };
                let write_regions = std::mem::take(&mut b.memory_regions).into_iter()
                    .filter_map(|(binding, (_, written))| (!written.is_empty()).then_some((binding, written))).collect();
                *functions = std::mem::take(&mut b.functions);
                let stage = Stage {
                    domain: *domain,
                    prefix,
                    selection,
                    operation: Operation::Map,
                    operation_source,
                    generated_origin: None,
                    tiles,
                    boundary_before: std::mem::take(boundary),
                    registers: b.registers,
                    locals: b.locals,
                    code: b.code,
                    reads: b.reads.into_iter().collect(),
                    writes: b.writes.into_iter().collect(),
                    write_regions,
                    cross_access: b.cross_access.into_iter().collect(),
                };
                for (&binding, written) in &stage.write_regions {
                    for &(start, end) in written { facts.written(program, binding, start, end); }
                }
                if let Some(proof) = element_ranges {
                    facts.elements.insert(*domain, proof);
                }
                for relation in appended {
                    facts.grant(relation);
                }
                program.stages.push(stage);
            }
            StmtKind::Expr(Expr {
                kind: ExprKind::Call(name, args),
                span: call_span,
                ..
            }) if name == "clear_prefix"
                && world.resolve(name, &d.module, &d.imports, true).is_none() =>
            {
                let output = append::clear(program, env, args, d, statement.span, facts,
                    operation_source(instance, *call_span, SourceSyntax::Expression, domain_path))?;
                facts.outputs.push(output);
                *boundary = false;
            }
            StmtKind::Expr(Expr {
                kind: ExprKind::Call(name, args),
                span: call_span,
                ..
            }) if name == "stable_sort_by_key"
                && world.resolve(name, &d.module, &d.imports, true).is_none() =>
            {
                sort::compile(program, env, args, d, statement.span, facts, operation_source(instance, *call_span, SourceSyntax::Expression, domain_path))?;
                *boundary = false;
            }
            StmtKind::Expr(Expr {
                kind: ExprKind::Call(name, args),
                span: call_span,
                ..
            }) if name == "compact"
                && world.resolve(name, &d.module, &d.imports, true).is_none() =>
            {
                compact::compile(program, env, args, d, statement.span, facts, operation_source(instance, *call_span, SourceSyntax::Expression, domain_path))?;
                *boundary = false;
            }
            StmtKind::Expr(Expr {
                kind: ExprKind::Call(name, args) | ExprKind::Specialize(name, _, args),
                span: call_span,
                ..
            }) => 'call: {
                let explicit = if let StmtKind::Expr(Expr {
                    kind: ExprKind::Specialize(_, types, _),
                    ..
                }) = &statement.kind
                {
                    Some(types.as_slice())
                } else {
                    None
                };
                let callee_key = world.resolve(name, &d.module, &d.imports, true);
                if callee_key.is_none() {
                    if let Some(operator) = CollectiveOperator::from_reduction_source(name)
                        .filter(|_| explicit.is_none())
                    {
                        reduce::compile(program, env, operator, args, d, statement.span, facts, operation_source(instance, *call_span, SourceSyntax::Expression, domain_path))?;
                        *boundary = false;
                        break 'call;
                    }
                    if let Some(kind) =
                        custom_reduce::Kind::from_source(name).filter(|_| explicit.is_none())
                    {
                        custom_reduce::compile(
                            world,
                            program,
                            env,
                            args,
                            instance,
                            statement.span,
                            facts,
                            name.ends_with("_map"),
                            kind,
                            domain_path.clone(),
                            functions,
                            operation_source(instance, *call_span, SourceSyntax::Expression, domain_path),
                        )?;
                        *boundary = false;
                        break 'call;
                    }
                    if let Some(operator) =
                        CollectiveOperator::from_segmented_reduction_source(name)
                            .filter(|_| explicit.is_none())
                    {
                        segmented_reduce::compile(
                            program,
                            env,
                            operator,
                            args,
                            d,
                            statement.span,
                            facts,
                            operation_source(instance, *call_span, SourceSyntax::Expression, domain_path),
                        )?;
                        *boundary = false;
                        break 'call;
                    }
                    if let Some(operator) =
                        CollectiveOperator::from_scan_source(name).filter(|_| explicit.is_none())
                    {
                        scan::compile(
                            program,
                            env,
                            operator,
                            false,
                            args,
                            d,
                            statement.span,
                            facts,
                            operation_source(instance, *call_span, SourceSyntax::Expression, domain_path),
                        )?;
                        *boundary = false;
                        break 'call;
                    }
                    if let Some(operator) = CollectiveOperator::from_segmented_scan_source(name)
                        .filter(|_| explicit.is_none())
                    {
                        scan::compile(
                            program,
                            env,
                            operator,
                            true,
                            args,
                            d,
                            statement.span,
                            facts,
                            operation_source(instance, *call_span, SourceSyntax::Expression, domain_path),
                        )?;
                        *boundary = false;
                        break 'call;
                    }
                }
                let Some(key) = callee_key else {
                    return d
                        .source
                        .fail(statement.span, "B109", format!("未知计算程序 {name}"));
                };
                let callee = &world.functions[&key];
                if callee.execution != crate::execution::Requirement::Domain {
                    return d.source.fail(
                        statement.span,
                        "B110",
                        "program 顶层只能调用计算程序或声明 parallel 阶段",
                    );
                }
                if args.len() != callee.function.params.len() {
                    return d
                        .source
                        .fail(statement.span, "B111", "计算程序参数数量不匹配");
                }
                if program.target != callee.function.target
                    && callee.function.target != Target::Compute
                {
                    return d
                        .source
                        .fail(statement.span, "B112", "程序与子程序的设备目标不兼容");
                }
                let roots = args
                    .iter()
                    .map(|arg| stage_root(world, instance, env, program, arg, domain_path))
                    .collect::<Result<Vec<_>>>()?;
                let actuals: Vec<_> = roots
                    .iter()
                    .map(|root| match root {
                        Root::Data(i, _) => generics::Actual {
                            ty: Some(std::borrow::Cow::Borrowed(&program.buffers[*i].ty)),
                            count: program.buffers[*i].count,
                        },
                        Root::Uniform(ty, _) | Root::Literal(ty, ..) => generics::Actual::value(ty),
                        Root::Acceleration(_) => generics::Actual {
                            ty: None,
                            count: None,
                        },
                        Root::Texture(i) => generics::Actual {
                            ty: None,
                            count: program.textures[*i].count,
                        },
                    })
                    .collect();
                let (caller_origin, caller_context) = d.syntax_context(Some(&instance.name));
                let instance =
                    generics::specialize(world, &key, &actuals, explicit, (&d.source, &d.module, &d.imports), statement.span)?;
                let callee = &instance.definition;
                let mut child = BTreeMap::new();
                for ((p, arg), root) in callee.function.params.iter().zip(args).zip(roots) {
                    let expected = || {
                        world.ty(
                            &p.ty,
                            &callee.source,
                            p.span,
                            &callee.module,
                            &callee.imports,
                        )
                    };
                    match &root {
                        Root::Texture(i) => {
                            if !p.ty.is_texture_resource()
                                || p.access != Access::Read
                                || p.ty.texture_dimension() != Some(program.textures[*i].dimension)
                                || texture::shape(world, &p.ty, callee, p.span)?
                                    != program.textures[*i].count
                            {
                                return d.source.fail(
                                    arg.span,
                                    "B239",
                                    "纹理资源与只读纹理参数的维度／形状不匹配",
                                );
                            }
                        }
                        Root::Acceleration(_) => {
                            if p.ty.name != "acceleration"
                                || p.ty.dataset
                                || p.access != Access::Read
                            {
                                return d.source.fail(
                                    arg.span,
                                    "B182",
                                    "加速资源的类型或访问权限不匹配",
                                );
                            }
                        }
                        Root::Data(i, access) => {
                            let ty = expected()?;
                            if !p.ty.dataset || ty != program.buffers[*i].ty {
                                return d.source.fail(arg.span, "B115", "数据集类型不匹配");
                            }
                            if p.ty.count.is_some()
                                && world.shape(
                                    &p.ty.count,
                                    &callee.source,
                                    p.span,
                                    &callee.module,
                                    &callee.imports,
                                )? != program.buffers[*i].count
                            {
                                return d.source.fail(
                                    arg.span,
                                    "B167",
                                    "调用方没有声明子程序需要的数据形状",
                                );
                            }
                            if (p.access.reads() && !access.reads())
                                || (p.access.writes() && !access.writes())
                            {
                                return d.source.fail(
                                    arg.span,
                                    "B116",
                                    "子程序请求了调用方没有的数据访问权限",
                                );
                            }
                        }
                        Root::Uniform(t, _) | Root::Literal(t, ..) => {
                            let ty = expected()?;
                            if p.ty.dataset || *t != ty {
                                return d.source.fail(arg.span, "B117", "标量参数类型不匹配");
                            }
                        }
                    }
                    child.insert(
                        p.name.clone(),
                        if let Root::Data(i, _) = root {
                            Root::Data(i, p.access)
                        } else {
                            root
                        },
                    );
                }
                let mut used = BTreeMap::new();
                for p in &callee.function.params {
                    if let Some(Root::Data(i, _)) = child.get(&p.name) {
                        if let Some(previous) = used.insert(*i, p.access) {
                            if (previous.writes() || p.access.writes())
                                && !(previous == Access::Atomic && p.access == Access::Atomic)
                            {
                                return d.source.fail(
                                    statement.span,
                                    "B118",
                                    "同一数据集通过多个参数别名进入可写子程序",
                                );
                            }
                        }
                    }
                }
                for (left, right) in contracts::root_lengths(&child, callee)? {
                    if !program.same_length(left, right) {
                        return d.source.fail(statement.span, "B226", "整域调用前未证明same_length数据集长度关系");
                    }
                }
                let required = prefix::relations(program, &child, callee, ContractKind::Requires)?;
                for relation in &required {
                    prefix::prove(
                        facts,
                        relation.domain,
                        relation.count,
                        &d.source,
                        statement.span,
                    )?;
                }
                let compiled = callee.compiled.is_some();
                let mut child_path = domain_path.to_vec();
                child_path.push(DomainSite {
                    origin: caller_origin.clone(), instance: caller_context.instance.clone(),
                    span: *call_span, target: Arc::from(instance.name.as_str()),
                    transition: if compiled { DomainTransition::Compiled } else { DomainTransition::Expanded },
                });
                let child_path: Arc<[DomainSite]> = child_path.into();
                if compiled {
                    use crate::module::codec::Decode;
                    let target = if callee.function.target == Target::Gpu { Target::Gpu } else { Target::Cpu };
                    let symbol = crate::module::domain::symbol(&instance.name, target);
                    let bytes = callee.compiled.as_ref().unwrap().get(&symbol).ok_or_else(|| Diagnostic {
                            source: d.source.clone(), span: statement.span, code: "B270",
                            message: format!("缺少已编译整域入口合同 {symbol}"),
                        })?;
                    let contract = native_function::Contract::from_bytes(bytes).map_err(|error| Diagnostic {
                        source: d.source.clone(), span: statement.span, code: "B270", message: error.to_string(),
                    })?.domain.ok_or_else(|| Diagnostic {
                        source: d.source.clone(), span: statement.span, code: "B270", message: format!("入口 {symbol} 不是整域合同"),
                    })?;
                    let bindings: Vec<_> = callee.function.params.iter().filter_map(|p| match child[&p.name] {
                        Root::Data(binding, _) => Some(binding), _ => None,
                    }).collect();
                    let before = facts.relations();
                    let mut write_regions = BTreeMap::new();
                    for (logical, written) in contract.write_regions {
                        write_regions.entry(bindings[logical]).or_insert_with(BTreeSet::new).extend(written);
                    }
                    for (&binding, written) in &write_regions {
                        for &(start, end) in written { facts.written(program, binding, start, end); }
                    }
                    facts.outputs.extend(contract.outputs.into_iter().map(|mut output| {
                        output.binding = bindings[output.binding];
                        output.domain = output.domain.map(|binding| bindings[binding]);
                        output
                    }));
                    calls.push(DomainCall {
                        stage: program.stages.len(),
                        instance: instance.clone(),
                        arguments: callee
                            .function
                            .params
                            .iter()
                            .map(|p| child[&p.name].clone())
                            .collect(),
                        write_regions,
                        before,
                        after: Vec::new(),
                        source: caller_origin.map(|origin| Arc::new(SourceSite {
                            origin, context: Arc::new(SourceContext { domain_path: child_path, ..(*caller_context).clone() }),
                            span: *call_span, syntax: SourceSyntax::Expression,
                        })),
                    });
                    *boundary = true;
                } else {
                    facts.enter(&required);
                    expand_program(
                        world, &instance, &mut child, program, stack, boundary, facts, calls, child_path, functions,
                    )?;
                    facts.leave();
                }
                first_stage = program.stages.len();
                for relation in prefix::relations(program, &child, callee, ContractKind::Ensures)? {
                    facts.grant(relation);
                }
                if compiled {
                    calls.last_mut().unwrap().after = facts.relations();
                }
            }
            StmtKind::Phase => {
                *boundary = true;
            }
            _ => {
                return d.source.fail(
                    statement.span,
                    "B119",
                    "program 顶层表达阶段；局部控制流放在 parallel 内",
                );
            }
        }
        let mut add = |binding, domain, first, ty: &Ty| facts.outputs.push(crate::module::domain::Output {
            binding,
            domain,
            words: ty.components.iter().map(|(_, offset)| first + offset).collect(),
        });
        for stage in &program.stages[first_stage..] {
            match &stage.operation {
                Operation::Reduce(v) => add(v.fields.target, None, v.fields.target_word, &v.ty),
                Operation::CustomReduce(v) => add(v.fields.target, None, v.fields.target_word, &v.ty),
                Operation::ScanExclusive(v) if stage.prefix.is_none() => add(
                    v.values.target, Some(v.values.source), v.values.target_word, &v.ty,
                ),
                Operation::CustomScan(v) if stage.prefix.is_none() => add(
                    v.values.fields.target, Some(v.values.fields.source),
                    v.values.fields.target_word, &v.values.ty,
                ),
                Operation::StableSort(v) if stage.prefix.is_none() => {
                    add(v.target, Some(v.source), 0, &program.buffers[v.target].ty);
                }
                Operation::Compact(v) => add(v.count, None, v.count_word, &Ty::scalar(Scalar::U32)),
                Operation::SegmentedReduce(v) => add(
                    v.count.binding, None, v.count.word, &Ty::scalar(Scalar::U32),
                ),
                Operation::CustomSegmentedReduce(v) => add(
                    v.count.binding, None, v.count.word, &Ty::scalar(Scalar::U32),
                ),
                _ => {}
            }
        }
    }
    Ok(false)
}
fn stage_root(
    world: &World,
    instance: &crate::generics::Instance,
    env: &BTreeMap<String, Root>,
    program: &Program,
    arg: &Expr,
    domain_path: &Arc<[DomainSite]>,
) -> Result<Root> {
    let d = &instance.definition;
    match &arg.kind {
        ExprKind::Name(n) => {
            if let Some(root) = env.get(n) {
                Ok(root.clone().reference_literal(operation_source(instance, arg.span, SourceSyntax::Expression, domain_path)))
            } else if let Some(key) = world
                .resolve(n, &d.module, &d.imports, true)
                .filter(|key| world.functions[key].function.kind == FunctionKind::Constant)
            {
                let (ty, words, origin) = constant::evaluate(world, &key, &[])?;
                Ok(Root::Literal(ty, words, origin).reference_literal(operation_source(instance, arg.span, SourceSyntax::Expression, domain_path)))
            } else {
                d.source
                    .fail(arg.span, "B113", format!("未知参数或 const {n}"))
            }
        }
        ExprKind::StaticU32(bits) => Ok(Root::Literal(Ty::scalar(Scalar::U32), vec![Bits::from(*bits)], None)),
        ExprKind::Number(n) => {
            let (ty, bits) = number(&d.source, arg.span, n)?;
            Ok(Root::Literal(ty, vec![bits], None))
        }
        _ => constant::stage_expression(world, instance, env, program, arg, None, domain_path),
    }
}
fn collective_field(
    program: &Program,
    env: &BTreeMap<String, Root>,
    expr: &Expr,
    d: &Definition,
    scalar: Scalar,
) -> Result<(usize, u32, Access)> {
    let (binding, word, access, ty) = collective_value_field(program, env, expr, d)?;
    if ty != Ty::scalar(scalar) {
        return d
            .source
            .fail(expr.span, "B175", format!("集合运算字段需要 {scalar:?}"));
    }
    Ok((binding, word, access))
}
fn collective_value_field(
    program: &Program,
    env: &BTreeMap<String, Root>,
    expr: &Expr,
    d: &Definition,
) -> Result<(usize, u32, Access, Ty)> {
    let mut fields = Vec::new();
    let mut base = expr;
    while let ExprKind::Member(parent, field) = &base.kind {
        fields.push(field.as_str());
        base = parent;
    }
    if fields.is_empty() {
        return d
            .source
            .fail(expr.span, "B172", "集合运算参数必须是 dataset.field");
    }
    let ExprKind::Name(name) = &base.kind else {
        return d
            .source
            .fail(expr.span, "B172", "集合运算参数必须是命名数据集字段");
    };
    let Some(Root::Data(binding, access)) = env.get(name) else {
        return d.source.fail(expr.span, "B173", "未知集合运算数据集");
    };
    fields.reverse();
    let Some((offset, ty)) = program.buffers[*binding].ty.field(&fields.join(".")) else {
        return d.source.fail(expr.span, "B174", "未知集合运算字段");
    };
    Ok((*binding, offset, *access, ty.clone()))
}
pub(crate) fn validate_signature(d: &Definition) -> Result<()> {
    if d.function.external.is_some() {
        return external::validate_signature(d);
    }
    if d.execution == crate::execution::Requirement::Domain {
        return Ok(());
    }
    if (d.function.result.dataset || d.function.result.mentions_owned())
        && !(d.function.kind == FunctionKind::Value && d.function.target == Target::Cpu)
    {
        return d.source.fail(
            d.function.span,
            "B224",
            "拥有的 dataset 返回属于普通 CPU 应用函数，借用与并行实例不转移资源所有权",
        );
    }
    if d.function.kind == FunctionKind::Cooperative && !d.function.params.iter().any(|p| p.ty.tile)
    {
        return d.source.fail(
            d.function.span,
            "B207",
            "cooperative 接口需要至少一个 tile<T> 借用来声明块范围",
        );
    }
    for p in &d.function.params {
        if p.ty.mentions_owned()
            && !p.ty.dataset
            && !(d.function.kind == FunctionKind::Value
                && d.function.target == Target::Cpu
                && matches!(p.access,Access::Value|Access::Read))
        {
            return d
                .source
                .fail(p.span, "B224", "拥有结果参数属于CPU函数的按值交接");
        }
        if p.ty.is_acceleration() {
            if p.access != Access::Read || d.function.target != Target::Gpu {
                return d
                    .source
                    .fail(p.span, "B182", "acceleration 必须是 GPU 函数的 read 资源");
            }
            continue;
        }
        if p.ty.dataset
            && !matches!(p.access, Access::Read | Access::Atomic)
            && !(d.function.kind == FunctionKind::Value
                && (matches!(p.access, Access::Write | Access::InOut)
                    || d.function.target == Target::Cpu && p.access == Access::Value))
        {
            return d.source.fail(
                p.span,
                "B120",
                "普通函数dataset参数支持read／write／inout／atomic；仅CPU应用函数可按值取得所有权",
            );
        }
        if p.ty.is_texture_resource() && p.access != Access::Read {
            return d
                .source
                .fail(p.span, "B239", "texture2d／texture3d 必须是 read 借用参数");
        }
        if p.ty.tile
            && (d.function.kind != FunctionKind::Cooperative
                || !matches!(p.access, Access::Value | Access::Atomic))
        {
            return d.source.fail(
                p.span,
                "B207",
                "tile<T> 只能是 cooperative 的只读或 atomic 借用参数",
            );
        }
    }
    Ok(())
}
pub(crate) fn check_function(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let name = &instance.name;
    let d = &instance.definition;
    if d.function.kind == FunctionKind::Constant {
        return constant_value(world, name).map(|_| ());
    }
    if d.function.kind == FunctionKind::Value && d.function.target == Target::Cpu {
        return native_function::check(world, instance);
    }
    checked_function(world, instance, None, native_function::Module::default()).map(|_| ())
}
struct CheckedFunction {
    body: FunctionBody,
    returned: Option<returns::Returned>,
    inputs: Vec<Reg>,
    written: Vec<Option<returns::Returned>>,
    contracts: BTreeSet<(ContractKind, ContractPredicate)>,
    residency: residency::Inputs,
    view_model: view_contracts::Function,
    initialization:Option<Vec<address::Initialization>>,
    functions: native_function::Module,
}
fn checked_function(
    world: &World,
    instance: &crate::generics::Instance,
    application: Option<application::Context>,
    functions: native_function::Module,
) -> Result<Box<CheckedFunction>> {
    let name = &instance.name;
    let d = &instance.definition;
    let program = function_program(world, instance)?;
    let mut b = Box::new(Builder::new(world, d, &program, 0, Some(&instance.name))?);
    b.in_function = true;
    b.application = application;
    b.functions = functions;
    if b.application.is_some() { b.view_uses = view_liveness::State::new(world, d); }
    let (parameters, inputs, output_contracts) = function_parameters(&mut b, name)?;
    let (returned,written) = b.function_body(d.function.body.as_ref().unwrap())?;
    finish_checked_function(b, name, parameters, inputs, output_contracts, returned, written)
}

#[inline(never)]
fn function_program(world: &World, instance: &crate::generics::Instance) -> Result<Box<Program>> {
    let name = &instance.name;
    let d = &instance.definition;
    // CPU 参数是实际调用 ABI；设备参数仍使用原并行域的接口身份。
    let mut buffers = Vec::new();
    for p in &d.function.params {
        if p.ty.dataset {
            let ty = cooperative::parameter_type(world, p, d)?;
            buffers.push(Buffer {
                name: p.name.clone(),
                access: p.access,
                stride: ir::align_up(ty.size, ty.align),
                ty,
                count: world.shape(&p.ty.count, &d.source, p.span, &d.module, &d.imports)?,
            });
        }
    }
    let program = Program {
        name: name.clone(),
        source_origin: d.source_origin(),
        generated_origin: None,
        target: d.function.target,
        uniform_bytes: 8 + 4 * buffers.len() as u32,
        buffers,
        accelerations: d
            .function
            .params
            .iter()
            .filter(|p| p.ty.is_acceleration())
            .map(|p| p.name.clone())
            .collect(),
        textures: d
            .function
            .params
            .iter()
            .filter(|p| p.ty.is_texture_resource())
            .map(|p| {
                Ok(TextureBinding {
                    name: p.name.clone(),
                    count: texture::shape(world, &p.ty, d, p.span)?,
                dimension: p.ty.texture_dimension().unwrap(),
                })
            })
            .collect::<Result<Vec<_>>>()?,
        uniforms: Vec::new(),
        source_stages: 0,
        stages: Vec::new(),
        functions: Arc::from([]),
        prefix_requirements: Vec::new(),
        prefix_guarantees: Vec::new(), length_requirements: Vec::new(), imports: BTreeMap::new(),
    };
    Ok(Box::new(program))
}

// 参数准备与输出打包不在下一层函数体分析期间保留临时载荷。
#[inline(never)]
fn function_parameters(
    b: &mut Builder<'_>,
    name: &str,
) -> Result<(Vec<CallParameter>, Vec<Reg>, BTreeSet<(ContractKind, ContractPredicate)>)> {
    let world = b.world;
    let definition = b.definition.clone();
    let d = &definition;
    let mut inputs = Vec::new();
    let mut parameters = Vec::new();
    if b.application.is_some() {
        let frame = b.application_frame(d.function.span)?;
        parameters.push(CallParameter::value(frame.ty).with_origin(Some(Arc::new(ParameterOrigin::Frame))));
        inputs.extend(frame.regs);
    }
    b.calls.push(name.into());
    if d.function.kind == FunctionKind::Cooperative {
        b.cooperative_root = Some(0);
    }
    let mut data_index = 0;
    let mut texture_index = 0;
    let mut acceleration_index = 0;
    for (parameter_index, p) in d.function.params.iter().enumerate() {
        let (generated, site) = match &d.syntax_origin {
            crate::frontend::DefinitionSyntax::Source => (false, b.source_at(p.span, SourceSyntax::Parameter)),
            crate::frontend::DefinitionSyntax::Generated(site) => (true, site.clone()),
            crate::frontend::DefinitionSyntax::CompiledInterface => (false, None),
        };
        let origin = (!matches!(d.syntax_origin, crate::frontend::DefinitionSyntax::CompiledInterface)).then(|| Arc::new(ParameterOrigin::Parameter {
            index: parameter_index, name: p.name.clone(), instance: Arc::from(name), generated, site,
        }));
        if p.ty.is_acceleration() {
            b.env
                .insert(p.name.clone(), Binding::Acceleration(acceleration_index));
            acceleration_index += 1;
            continue;
        }
        if p.ty.is_texture_resource() {
            let address = b.reg(Scalar::U64);
            parameters.push(CallParameter::value(Ty::scalar(Scalar::U64)).with_origin(origin.clone()));
            inputs.push(address);
            let mut resource = texture::TextureReference::binding(texture_index);
            resource.source.parameter = Some(address);
            b.env.insert(p.name.clone(), Binding::Texture(resource));
            texture_index += 1;
            continue;
        }
        if p.ty.dataset {
            let binding = if d.function.kind == FunctionKind::Value
                && d.function.target == Target::Cpu {
                {
                    let mut data = b.application_parameter(data_index, parameter_index, p.access)?;
                    if p.access!=Access::Value {
                        let parameter_ty=Builder::borrowed_dataset_parameter_type(p.access,d.execution);
                        let stride=b.reg(parameter_ty.components[2].0);let offset=b.reg(Scalar::U32);
                        let value=|reg,scalar|Value{ty:Ty::scalar(scalar),regs:vec![reg],proof:bounds::Proof::default(),literal:false,uniform:false,constant:None};
                        let stride_scalar=parameter_ty.components[2].0;
                        parameters.push(CallParameter::value(parameter_ty).with_origin(origin.clone()));
                        inputs.extend([data.root.regs[0],data.length.regs[0],stride,offset]);
                        data.layout=application::DataLayout::Parameter{parameter:parameter_index,stride:value(stride,stride_scalar),offset:value(offset,Scalar::U32)};
                        Binding::ApplicationData(data)
                    }else{
                        parameters.push(CallParameter::value(Ty::owned_dataset(data.ty.clone(),data.count)).with_origin(origin.clone()));
                        inputs.extend([data.root.regs[0],data.length.regs[0]]);
                        let data=b.application_bind(data,application::rebound(d.function.body.as_ref().unwrap(),&p.name));
                        Binding::ApplicationData(data)
                    }
                }
            } else {
                b.view_access.insert(data_index, p.access);
                if p.access == Access::InOut {
                    b.initialized.extend(b.program.buffers[data_index].ty.components.iter().map(|(_,word)|Position::Element(data_index,*word)));
                }
                Binding::Data(b.dataset_view(data_index,p.access,true))
            };
            b.env.insert(p.name.clone(), binding);
            data_index += 1;
            continue;
        }
        if p.ty.tile {
            let ty = cooperative::parameter_type(world, p, d)?;
            let tile = b.tiles.len();
            b.tiles.push(CheckedTile {
                source: TileOrigin::Parameter,
                ty,
                width: TileWidth::Parameter,
                access: p.access,
            });
            b.bind_tile(&p.name, tile);
            continue;
        }
        let ty=cooperative::parameter_type(world,p,d)?;
        if p.access==Access::Read {
            if ty.owns_resources() {
                let regs=ty.components.iter().map(|(scalar,_)|b.reg(*scalar)).collect::<Vec<_>>();
                inputs.extend(&regs);parameters.push(CallParameter::value(ty.clone()).with_origin(origin.clone()));
                let proof=bounds::Proof::default().view_parameters(&ty,&[],|i|ValueIdentity::Register(regs[i]));
                let mut value=b.incoming_owned_value(Value{ty,regs,proof,literal:false,uniform:false,constant:None});
                value.borrow();residency::parameter(&mut value,parameter_index);
                b.bind_owned_value(&p.name,value,p.span)?;continue;
            }
        }
        if p.access == Access::Atomic && b.application.is_none() {
            b.env.insert(
                p.name.clone(),
                Binding::AtomicBorrow(Place::AtomicParameter(ty)),
            );
            continue;
        }
        if p.access != Access::Value {
            let place = if b.application.is_some() {
                let parts = if d.function.c_abi { vec![ty.clone()] } else { native_function::reference_parameters(&ty) };
                let mut addresses = Vec::new();
                for part in &parts {
                    let address = [b.reg(Scalar::U32), b.reg(Scalar::U32)];
                    inputs.extend(address);
                    addresses.push(address);
                    parameters.push(CallParameter::reference(part.clone(), p.access).with_origin(origin.clone()));
                }
                let address = if addresses.len() == 1 {
                    ReferenceAddress::Contiguous(addresses[0])
                } else {
                    ReferenceAddress::Components(ty.components.iter().map(|(_, offset)| *offset).zip(addresses).collect())
                };
                Place::Reference {
                    words: ty.components.iter().map(|(_, offset)| *offset).collect(),
                    ty,
                    address,
                    scope: Some(parameter_index),
                    access: p.access,
                }
            } else {
                let (slots, storage) = b.local_slots(&ty);
                if p.access == Access::Read { Place::ReadOnlyLocal(ty, slots, storage) }
                else { Place::Local(ty, slots, storage) }
            };
            if p.access.reads() {
                b.initialize(&place);
            }
            if p.access == Access::Atomic {
                b.env.insert(p.name.clone(), Binding::AtomicBorrow(place));
                continue;
            }
            if place.ty().owns_resources(){
                let ty=place.ty().clone();let regs=ty.components.iter().map(|(scalar,_)|b.reg(*scalar)).collect();
                let proof=if p.access==Access::InOut{bounds::Proof::parameter_content(parameter_index,ty.components.len()).view_parameters(&ty,&[],|i|ValueIdentity::ParameterContent(parameter_index,i))}else{bounds::Proof::default()};
                let value=Value{ty,regs,proof,uniform:false,literal:false,constant:None};
                let returns::Returned::OwnedValue(mut value)=b.template_returned(value,p.access==Access::InOut)else{unreachable!()};
                if p.access==Access::InOut{residency::parameter(&mut value,parameter_index);}
                value.place=Some(place);b.env.insert(p.name.clone(),Binding::OwnedValue(value));continue;
            }
            b.env.insert(
                p.name.clone(),
                Binding::Owned(OwnedBinding {
                    proof: if p.access.reads() && b.application.is_some() {
                        bounds::Proof::parameter_content(
                            parameter_index,
                            place.ty().components.len(),
                        ).view_parameters(place.ty(),&[], |i| ValueIdentity::ParameterContent(parameter_index,i))
                    } else {
                        bounds::Proof::default()
                    },
                    place,
                    uniform: false,
                    initialization: None,
                }),
            );
            continue;
        }
        let regs: Vec<_> = ty.components.iter().map(|(s, _)| b.reg(*s)).collect();
        if b.application.is_some() {
            inputs.extend(&regs);
            parameters.push(CallParameter::value(ty.clone()).with_origin(origin.clone()));
        }
        if ty.owns_resources() {
            let proof=bounds::Proof::default().view_parameters(&ty,&[],|i|ValueIdentity::Register(regs[i]));
            let mut result = b.incoming_owned_value(Value {
                ty,
                regs,
                proof,
                literal: false,
                uniform: false,
                constant: None,
            });
            residency::parameter(&mut result, parameter_index);
            b.bind_owned_value(&p.name, result, p.span)?;
            continue;
        }
        b.env.insert(
            p.name.clone(),
            Binding::Value(Value {
                proof: bounds::Proof::default().view_parameters(&ty,&[], |i| ValueIdentity::Register(regs[i])),
                literal: false,
                ty,
                regs,
                uniform: false,
                constant: None,
            }),
        );
    }
    application::bind_parameter_elements(b,d);
    let environment=std::mem::take(&mut b.env);
    let requirements=contracts::resolve(b,d,&environment);b.env=environment;
    let requirements=requirements?;
    contracts::assume(&mut b.env, &mut b.path, &requirements);
    b.materialize_array_bindings();
    let output_contracts=outputs::contract_predicates(b,d)?;
    Ok((parameters, inputs, output_contracts))
}

#[inline(never)]
fn finish_checked_function(
    mut b: Box<Builder<'_>>,
    name: &String,
    parameters: Vec<CallParameter>,
    inputs: Vec<Reg>,
    output_contracts: BTreeSet<(ContractKind, ContractPredicate)>,
    returned: Option<returns::Returned>,
    written: Vec<Option<returns::Returned>>,
) -> Result<Box<CheckedFunction>> {
    let definition = b.definition.clone();
    let d = &definition;
    let output = returned.as_ref().map(|value| b.returned_storage(value));
    let result = if d.function.result.name == "void" || d.function.result.is_never() {
        None
    } else {
        Some(b.expected_storage(&d.function.result, d.function.span)?)
    };
    let (returned, semantic_inputs, written, initialization) =
        native_function::canonical(&mut b, returned, &inputs, written);
    let body = FunctionBody {
        name: name.clone(),
        source_origin: d.source_origin(),
        parameters,
        result,
        body: ValueCode {
            entry_origin: None,
            generated_origin: None,
            registers: b.registers,
            locals: b.locals,
            code: b.code,
            inputs: inputs.clone(),
            outputs: output.map(|v| v.regs).unwrap_or_default(),
        },
    };
    Ok(Box::new(CheckedFunction {
        body,
        returned,
        inputs: semantic_inputs,
        written,
        contracts: output_contracts,
        residency: b.residency,
        view_model: b.view_model,
        initialization,
        functions: b.functions,
    }))
}
fn number(source: &Source, span: Span, n: &str) -> Result<(Ty, Bits)> {
    if !n.starts_with("0x") && let Some(text) = n.strip_suffix("f64") {
        let value = text.parse::<f64>().map_err(|_| Diagnostic {
            source: source.clone(), span, code: "B121",
            message: format!("非法 f64 常量 {n}"),
        })?;
        if !value.is_finite() {
            return source.fail(span, "B122", "浮点常量超出有限 f64 范围");
        }
        return Ok((Ty::scalar(Scalar::F64), value.to_bits()));
    }
    if let Some(text) = n.strip_suffix("u64") {
        let bits = if let Some(hex) = text.strip_prefix("0x") {
            u64::from_str_radix(hex, 16)
        } else {
            text.parse::<u64>()
        }.map_err(|_| Diagnostic {
            source: source.clone(), span, code: "B123",
            message: format!("非法 u64 常量 {n}"),
        })?;
        return Ok((Ty::scalar(Scalar::U64), bits));
    }
    if !n.starts_with("0x")
        && (n.contains('.') || n.contains('e') || n.contains('E') || n.ends_with('f'))
    {
        let value = n
            .trim_end_matches('f')
            .parse::<f32>()
            .map_err(|_| Diagnostic {
                source: source.clone(),
                span,
                code: "B121",
                message: format!("非法 f32 常量 {n}"),
            })?;
        if !value.is_finite() {
            return source.fail(span, "B122", "浮点常量超出有限 f32 范围");
        }
        Ok((Ty::scalar(Scalar::F32), Bits::from(value.to_bits())))
    } else {
        let text = n.trim_end_matches('u');
        let bits = if let Some(hex) = text.strip_prefix("0x") {
            u32::from_str_radix(hex, 16)
        } else {
            text.parse::<u32>()
        }
        .map_err(|_| Diagnostic {
            source: source.clone(),
            span,
            code: "B123",
            message: format!("非法 u32 常量 {n}"),
        })?;
        Ok((Ty::scalar(Scalar::U32), Bits::from(bits)))
    }
}
struct Builder<'a> {
    residency: residency::Inputs,
    view_model: view_contracts::Function,
    application: Option<application::Context>,
    functions: native_function::Module,
    world: &'a World,
    definition: Arc<Definition>,
    program: &'a Program,
    domain: usize,
    element_domains: BTreeSet<usize>,
    env: BTreeMap<String, Binding>,
    path: bounds::Clause,
    registers: Vec<Scalar>,
    locals: BTreeMap<usize, Ty>,
    code: Vec<Inst>,
    source_definition: Option<Arc<SourceOrigin>>,
    source_context: Arc<SourceContext>,
    source_site: Option<Arc<SourceSite>>,
    global_index: Reg,
    reads: BTreeSet<usize>,
    writes: BTreeSet<usize>,
    cross_access: BTreeSet<usize>,
    memory_regions: BTreeMap<usize, (BTreeSet<(bool, u32, u32)>, BTreeSet<(u32, u32)>)>,
    view_access: BTreeMap<usize, Access>,
    initialized: BTreeSet<Position>,
    output_scopes: usize,
    control_uniform: bool,
    loop_depth: usize,
    block_depth: usize,
    in_function: bool,
    flow: Flow,
    loop_contexts: Vec<LoopContext>,
    return_context: Option<returns::ReturnContext>,
    cooperative_root: Option<usize>,
    calls: Vec<String>,
    iterators: Vec<Option<(String, usize)>>,
    mutable_names: BTreeSet<String>,
    view_uses: view_liveness::State,
    tiles: Vec<CheckedTile>,
    appends: BTreeMap<usize, PrefixCount>,
    prefix: Option<PrefixCount>,
}
impl<'a> Builder<'a> {
    #[inline(never)]
    fn new(
        world: &'a World,
        definition: &Arc<Definition>,
        program: &'a Program,
        domain: usize,
        source_instance: Option<&str>,
    ) -> Result<Self> {
        let mut mutable_names = BTreeSet::new();
        assigned_names(
            world,
            definition,
            definition.function.body.as_ref().unwrap(),
            &mut mutable_names,
        )?;
        let (source_definition, source_context) = definition.syntax_context(source_instance);
        Ok(Self {
            residency: residency::Inputs::default(),
            view_model: view_contracts::Function::default(),
            application: None,
            functions: native_function::Module::default(),
            world,
            definition: definition.clone(),
            program,
            domain,
            element_domains: BTreeSet::from([domain]),
            env: BTreeMap::new(),
            path: bounds::Clause::default(),
            registers: Vec::new(),
            locals: BTreeMap::new(),
            code: Vec::new(),
            source_definition,
            source_context,
            source_site: None,
            global_index: usize::MAX,
            reads: BTreeSet::new(),
            writes: BTreeSet::new(),
            cross_access: BTreeSet::new(),
            memory_regions: BTreeMap::new(),
            view_access: BTreeMap::new(),
            initialized: BTreeSet::new(),
            // Reference parameters already own 0..params.len(); dataset and
            // inline output origins share this allocator after that range.
            output_scopes: definition.function.params.len(),
            control_uniform: true,
            loop_depth: 0,
            block_depth: 0,
            in_function: false,
            flow: Flow::NEXT,
            loop_contexts: Vec::new(),
            return_context: None,
            cooperative_root: None,
            calls: Vec::new(),
            iterators: Vec::new(),
            mutable_names,
            view_uses: view_liveness::State::default(),
            tiles: Vec::new(),
            appends: BTreeMap::new(),
            prefix: None,
        })
    }
    fn emit(&mut self, kind: InstKind) {
        self.code.push(Inst { source_site: self.source_site.clone(), ..Inst::new(kind) });
    }
    fn source_at(&self, span: Span, syntax: SourceSyntax) -> Option<Arc<SourceSite>> {
        self.source_definition.as_ref().map(|origin| Arc::new(SourceSite {
            origin: origin.clone(), context: self.source_context.clone(), span, syntax,
        }))
    }
    fn at_source<T>(&mut self, span: Span, syntax: SourceSyntax, work: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let site = self.source_at(span, syntax);
        let previous = std::mem::replace(&mut self.source_site, site);
        let result = work(self);
        self.source_site = previous;
        result
    }
    fn data_access(&mut self, place: &Place, access: Access) {
        let Place::Element { index, .. } = place.storage() else { return; };
        let cross = *index != self.global_index;
        for ((scalar, _), position) in place.ty().components.iter().zip(place.data_positions()) {
            let mut pending = vec![&position];
            while let Some(position) = pending.pop() {
                match position {
                    Position::Element(binding, word) => {
                        let (reads, writes) = self.memory_regions.entry(*binding).or_default();
                        if access.reads() { reads.insert((cross, *word, *word + scalar.size())); }
                        if access.writes() { writes.insert((*word, *word + scalar.size())); }
                    }
                    Position::Indexed(choices, _) => pending.extend(choices.iter()),
                    _ => {}
                }
            }
        }
    }
    fn parallel_access(&self, span: Span) -> Result<()> {
        for (binding, (reads, writes)) in &self.memory_regions {
            if !reads.is_empty() && self.appends.iter().any(|(output, count)| output == binding || count.binding == *binding) {
                return self.fail(span, "B221", "append 的共享输出／计数不能在同一阶段作为普通共享输入读取");
            }
            let mut writes = writes.iter().filter(|(start, end)| start < end).peekable();
            for read in reads.iter().filter(|(cross, start, end)| *cross && start < end) {
                while writes.peek().is_some_and(|write| write.1 <= read.1) { writes.next(); }
                if writes.peek().is_some_and(|write| write.0 < read.2) {
                    return self.fail(span, "B166", format!(
                        "同一 parallel 阶段不能共享读取并写入 {} 的其他元素中的重叠位置",
                        self.program.buffers[*binding].name,
                    ));
                }
            }
        }
        Ok(())
    }
    fn roots(&mut self, env: &BTreeMap<String, Root>) {
        for (name, root) in env {
            match root {
                Root::Texture(i) => {
                    self.env.insert(
                        name.clone(),
                        Binding::Texture(texture::TextureReference::binding(*i)),
                    );
                }
                Root::Acceleration(i) => {
                    self.env.insert(name.clone(), Binding::Acceleration(*i));
                }
                Root::Data(i, access) => {
                    let view = self.dataset_view(*i,*access,false);
                    self.env.insert(name.clone(), Binding::Data(view));
                    self.view_access.insert(*i, *access);
                }
                Root::Uniform(ty, offset) => {
                    let mut regs = Vec::new();
                    for (s, o) in &ty.components {
                        let r = self.reg(*s);
                        self.emit(InstKind::Uniform(r, offset + o));
                        regs.push(r);
                    }
                    self.env.insert(
                        name.clone(),
                        Binding::Value(Value {
                            proof: bounds::Proof::default(),
                            literal: false,
                            ty: ty.clone(),
                            regs,
                            uniform: true,
                            constant: None,
                        }),
                    );
                }
                Root::Literal(ty, bits, origin) => {
                    let value = self.evaluated_constant(ty.clone(), bits.clone(), origin.clone());
                    self.env.insert(name.clone(), Binding::Value(value));
                }
            }
        }
    }
    fn fail<T>(&self, span: Span, code: &'static str, message: impl Into<String>) -> Result<T> {
        self.definition.source.fail(span, code, message)
    }
    fn reg(&mut self, ty: Scalar) -> Reg {
        let r = self.registers.len();
        self.registers.push(ty);
        r
    }
    fn constant(&mut self, s: Scalar, bits: Bits) -> Value {
        self.constant_with_origin(s, bits, None)
    }
    fn constant_with_origin(&mut self, s: Scalar, bits: Bits, generated_origin: Option<Arc<GeneratedOrigin>>) -> Value {
        let bits = s.truncate(bits);
        let r = self.reg(s);
        let kind = InstKind::Constant(r, s, bits);
        if let Some(origin) = generated_origin { self.code.push(Inst::generated(kind, origin)); }
        else { self.emit(kind); }
        Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(s),
            regs: vec![r],
            uniform: true,
            constant: Some(vec![bits]),
        }
    }
    fn require_same(&self, a: &Ty, b: &Ty, span: Span) -> Result<()> {
        operators::same(a, b, &self.definition.source, span)
    }
    fn require_bool(&self, v: &Value, span: Span) -> Result<()> {
        self.require_same(&Ty::scalar(Scalar::Bool), &v.ty, span)
    }
    fn block(&mut self, body: &[Stmt]) -> Result<()> {
        let original = self.env.keys().cloned().collect::<BTreeSet<_>>();
        self.block_depth += 1;
        let mut flow = Flow::NEXT;
        for s in body {
            if !flow.continues() {
                break;
            }
            self.flow = Flow::NEXT;
            self.statement(s)?;
            self.view_uses.after(s.span);
            flow = flow.then(self.flow);
        }
        self.flow = flow;
        self.block_depth -= 1;
        if flow.continues() {
            self.release_application_data(Some(&original),view::Borrow::default())?;
        }
        self.env.retain(|k, _| original.contains(k));
        Ok(())
    }
    fn statement(&mut self, s: &Stmt) -> Result<()> {
        self.at_source(s.span, SourceSyntax::Statement, |b| b.temporary_scope(s.span, |b| {b.statement_syntax(s)?;Ok(((),view::Borrow::default()))}))
    }
    fn statement_syntax(&mut self, s: &Stmt) -> Result<()> {
        self.view_uses.before(s.span);
        match &s.kind {
            StmtKind::Declare(annotation, name) => {
                return self.declare_statement(annotation, name, s.span);
            }
            StmtKind::Let(annotation, name, e) => {
                return self.let_statement(annotation, name, e, s.span);
            }
            StmtKind::Assign(lhs, op, rhs) => return self.assign_statement(lhs, op, rhs, s.span),
            StmtKind::Expr(e) => return self.expression_statement(e),
            StmtKind::If(condition, yes_body, no_body) => {
                return self.if_statement(condition, yes_body, no_body, s.span);
            }
            StmtKind::While(condition, body) => {
                return self.while_statement(condition, body, s.span);
            }
            StmtKind::For(name, start, end, body) => {
                return self.for_statement(name, start, end, body, s.span);
            }
            StmtKind::Break | StmtKind::Continue => return self.loop_control_statement(s),
            StmtKind::Match(value, arms) => { return self.match_returned(value,arms,s.span); }
            StmtKind::Return(value) => self.return_statement(value.as_ref(), s.span)?,
            StmtKind::Parallel(..) => return self.application_parallel(s),
            StmtKind::Phase => {
                if self.application.is_none() {
                    return self.fail(s.span, "B163", "phase 需要整域同步位置，不能位于元素实例中");
                }
            }
            StmtKind::Seek(name, position) => return self.seek_statement(name, position, s.span),
        }
        Ok(())
    }
    // 循环出口的资源交接只在本语句分析时存活。
    #[inline(never)]
    fn loop_control_statement(&mut self, s: &Stmt) -> Result<()> {
        let breaking = matches!(&s.kind, StmtKind::Break);
        if self.loop_depth == 0 {
            return self.fail(s.span, "B129", if breaking {
                "break 只能用于局部顺序循环"
            } else {
                "continue 只能用于局部顺序循环"
            });
        }
        self.release_application_loop()?;
        if breaking {
            self.emit(InstKind::Break);
            self.capture_break();
            self.flow = Flow::BREAK;
        } else {
            if let Some(Some((_, slot))) = self.iterators.last().cloned() {
                let index = self.reg(Scalar::U32);
                self.emit(InstKind::LoadLocal(index, slot));
                self.advance_iterator(index, slot)?;
            }
            self.check_application_loop(s.span)?;
            self.emit(InstKind::Continue);
            self.flow = Flow::CONTINUE;
        }
        Ok(())
    }
    fn value_result(&self, result: Option<Value>, span: Span) -> Result<Value> {
        match result {
            Some(value) => Ok(value),
            None => self.fail(span, "B228", "此处需要普通值，void 调用没有返回值"),
        }
    }
    fn place(&mut self, e: &Expr) -> Result<Option<Place>> {
        self.at_source(e.span, SourceSyntax::Place, |b| b.place_syntax(e))
    }
    fn place_syntax(&mut self, e: &Expr) -> Result<Option<Place>> {
        self.require_user_projection(e)?;
        self.view_uses.before(e.span);
        match &e.kind {
            ExprKind::Name(name) => self.name_place(name, e).map(Some),
            ExprKind::Specialize(name, explicit, args) if name == "unsafe_position" => {
                self.external_position(explicit, args, e.span)
            }
            ExprKind::Member(base, field) => self.member_place(base, field, e),
            ExprKind::Index(base, index) => self.index_place(base, index),
            _ => self.fail(e.span, "B138", "表达式不是可赋值位置"),
        }
    }
    fn read_place(&mut self, p: &Place, span: Span, element_read_origin: Option<Arc<GeneratedOrigin>>) -> Result<Value> {
        if p.ty().owns_resources() {
            return self.fail(
                span,
                "B224",
                "拥有资源需要移动、字段借用或match交接，不能按普通值复制",
            );
        }
        match p {
            Place::AtomicParameter(_)
            | Place::Reference { access: Access::Atomic, .. }
            | Place::ApplicationElement { access: Access::Atomic, .. } => {
                self.fail(span, "B214", "原子借用只能通过显式原子操作读取")
            }
            Place::Reference {
                ty, address, words, ..
            } => {
                if let Some(borrow)=self.reference_position_borrow(p,span)? {
                    self.check_view_requirement(view::Requirement::Address{borrow,layout:address::Layout::of(ty),writes:false,initialized:true,span})?;
                }else{self.require_initialized(p, span)?;}
                let zero = self.constant(Scalar::U32, 0).regs[0];
                let mut regs = Vec::new();
                for ((scalar, _), word) in ty.components.iter().zip(words) {
                    let (source, word) = address.source_at(*word, zero);
                    let dst = self.reg(*scalar);
                    self.emit(InstKind::LoadIndexed {
                        dst,
                        source,
                        offset: zero,
                        word,
                    });
                    regs.push(dst);
                }
                let (mut proof, uniform) = self.read_owned_proof(p, &regs);
                if matches!(address, ReferenceAddress::External { .. }) {
                    proof = proof.with_external_borrows(ty);
                }
                Ok(Value {
                    proof,
                    literal: false,
                    ty: ty.clone(),
                    regs,
                    uniform,
                    constant: None,
                })
            }
            Place::ApplicationElement {
                ty,
                handle,
                identity,
                index,
                stride,
                words,
                ..
            } => {
                self.require_initialized(p, span)?;
                self.dataset_effect(p,Access::Read,false,span)?;
                let mut value = self
                    .read_application_place(ty, *handle, *identity, *index, *stride, words, span, element_read_origin)?;
                let (proof, uniform) = self.read_owned_proof(p, &value.regs);
                value.constant = proof.known_value(ty);
                value.proof = proof;
                value.uniform = uniform;
                Ok(value)
            }
            Place::Indexed { .. } => self.read_array_place(p, span),
            Place::ViewElement {
                ty,
                address,
                index,
                stride,
                words,
                ..
            } => {
                let mut regs = Vec::new();
                for ((scalar, _), word) in ty.components.iter().zip(words) {
                    let dst = self.reg(*scalar);
                    self.emit(InstKind::ViewLoad {
                        dst,
                        address: *address,
                        index: *index,
                        stride: *stride,
                        word: *word,
                    });
                    regs.push(dst);
                }
                let (proof, uniform) = self.read_owned_proof(p, &regs);
                Ok(Value {
                    proof,
                    literal: false,
                    ty: ty.clone(),
                    regs,
                    uniform,
                    constant: None,
                })
            }
            Place::TileElement {
                ty,
                tile,
                index,
                words,
            } => {
                if self.tiles[*tile].access == Access::Atomic {
                    return self.fail(span, "B214", "atomic tile 元素只能通过显式原子操作读取");
                }
                if let TileOrigin::Bound(TileSource::Input { binding, .. }) =
                    self.tiles[*tile].source
                {
                    self.reads.insert(binding);
                    self.cross_access.insert(binding);
                }
                let mut regs = Vec::new();
                for ((scalar, _), word) in ty.components.iter().zip(words) {
                    let dst = self.reg(*scalar);
                    self.emit(InstKind::TileLoad {
                        dst,
                        tile: *tile,
                        index: *index,
                        word: *word,
                    });
                    regs.push(dst);
                }
                let (proof, uniform) = self.read_owned_proof(p, &regs);
                Ok(Value {
                    proof,
                    literal: false,
                    ty: ty.clone(),
                    regs,
                    uniform,
                    constant: None,
                })
            }
            Place::Local(ty, slots, ..) | Place::ReadOnlyLocal(ty, slots, ..) => {
                if matches!(p, Place::Local(..)) {
                    self.require_initialized(p, span)?;
                }
                let mut regs = Vec::new();
                for (s, slot) in ty.components.iter().zip(slots) {
                    let r = self.reg(s.0);
                    self.emit(InstKind::LoadLocal(r, *slot));
                    regs.push(r);
                }
                let (proof, uniform) = self.read_owned_proof(p, &regs);
                let constant = proof.known_value(ty);
                Ok(Value {
                    proof,
                    literal: false,
                    ty: ty.clone(),
                    regs,
                    uniform,
                    constant,
                })
            }
            Place::Element {
                ty,
                binding,
                index,
                words,
                ..
            } => {
                if self.view_access[binding] == Access::Atomic {
                    return self.fail(span, "B214", "atomic 数据集元素只能通过显式原子操作读取");
                }
                if self.view_access[binding].writes() {
                    self.require_initialized(p, span)?;
                }
                self.data_access(p, Access::Read);
                self.reads.insert(*binding);
                if *index != self.global_index {
                    self.cross_access.insert(*binding);
                }
                let mut regs = Vec::new();
                for ((s, _), word) in ty.components.iter().zip(words) {
                    let dst = self.reg(*s);
                    self.emit(InstKind::Load {
                        dst,
                        binding: *binding,
                        index: *index,
                        word: *word,
                    });
                    regs.push(dst);
                }
                let proof = self.read_owned_proof(p, &regs).0;
                Ok(Value {
                    proof,
                    literal: false,
                    ty: ty.clone(),
                    regs,
                    uniform: false,
                    constant: None,
                })
            }
        }
    }
    fn write_place(&mut self, p: &Place, value: Value, span: Span, local_origin: Option<Arc<GeneratedOrigin>>) -> Result<()> {
        if p.ty().contains_views() && matches!(p.storage(), Place::Element { .. } | Place::ApplicationElement { .. }) {
            return self.fail(span,"B185","持久dataset的view载荷不可写或独占借出；数值字段按原权限修改");
        }
        self.require_owned_writable(p,span)?;
        self.require_view_available(&value, &[], span)?;
        let ty = p.ty();
        self.require_same(ty, &value.ty, span)?;
        if let Some(borrow)=self.reference_position_borrow(p,span)? {self.write_address_values(&borrow);}
        match p {
            Place::AtomicParameter(_)
            | Place::Reference { access: Access::Atomic, .. }
            | Place::ApplicationElement { access: Access::Atomic, .. } => {
                return self.fail(span, "B214", "原子借用只能通过显式原子操作修改");
            }
            Place::Reference { address, words, .. } => {
                self.writable_storage(p, span)?;
                self.assign_owned_proof(p, bounds::Proof::capture(&value), false);
                self.initialize(p);
                let zero = self.constant(Scalar::U32, 0).regs[0];
                for (word, value) in words.iter().zip(value.regs) {
                    let (source, word) = address.source_at(*word, zero);
                    self.emit(InstKind::StoreIndexed {
                        source,
                        offset: zero,
                        word,
                        value,
                    });
                }
            }
            Place::ApplicationElement {
                handle,
                identity,
                index,
                stride,
                words,
                ..
            } => {
                self.writable_storage(p, span)?;
                self.dataset_effect(p,Access::Write,false,span)?;
                let proof = bounds::Proof::capture(&value);
                self.write_application_place(
                    *handle, *identity, *index, *stride, words, value, span, None,
                )?;
                self.initialize(p);
                self.assign_owned_proof(p, proof, false);
                return Ok(());
            }
            Place::Indexed { .. } => return self.write_array_place(p, value, span),
            Place::ReadOnlyLocal(..) => {
                return self.fail(span, "B144", "按值参数或不可变局部值不可写");
            }
            Place::ViewElement { .. } => return self.fail(span, "B185", "view 的元素不可写"),
            Place::TileElement { tile, .. } => {
                if self.tiles[*tile].access == Access::Atomic {
                    return self.fail(span, "B214", "atomic tile 元素只能通过显式原子操作修改");
                }
                return self.fail(span, "B202", "tile 值不可写；只修改当前独占的全局元素");
            }
            Place::Local(_, slots, ..) => {
                if slots.iter().copied().collect::<BTreeSet<_>>().len() != slots.len() {
                    return self.fail(span, "B143", "赋值位置包含重复分量");
                }
                self.assign_owned_proof(p, bounds::Proof::capture(&value), false);
                self.initialize(p);
                for (slot, value) in slots.iter().zip(value.regs) {
                    let kind=InstKind::StoreLocal(*slot,value);
                    if let Some(origin)=&local_origin {self.code.push(Inst::generated(kind,origin.clone()));}
                    else {self.emit(kind);}
                }
            }
            Place::Element {
                binding,
                index,
                words,
                writable,
                ..
            } => {
                if self.view_access[binding] == Access::Atomic {
                    return self.fail(span, "B214", "atomic 数据集元素只能通过显式原子操作修改");
                }
                let b = &self.program.buffers[*binding];
                if !*writable || !self.view_access[binding].writes() {
                    return self.fail(
                        span,
                        "B144",
                        format!("没有 {} 当前元素的独占修改权", b.name),
                    );
                }
                if words.iter().copied().collect::<BTreeSet<_>>().len() != words.len() {
                    return self.fail(span, "B143", "赋值位置包含重复分量");
                }
                self.data_access(p, Access::Write);
                self.writes.insert(*binding);
                self.assign_owned_proof(p, bounds::Proof::capture(&value), false);
                for (word, value) in words.iter().zip(value.regs) {
                    self.emit(InstKind::Store {
                        binding: *binding,
                        index: *index,
                        word: *word,
                        value,
                    });
                }
                self.initialize(p);
            }
        }
        Ok(())
    }
    fn expr(&mut self, e: &Expr) -> Result<Option<Value>> {
        self.at_source(e.span, SourceSyntax::Expression, |b| b.expr_syntax(e))
    }
    fn expr_syntax(&mut self, e: &Expr) -> Result<Option<Value>> {
        self.view_uses.before(e.span);
        match &e.kind {
            ExprKind::FunctionRef(..) => self.fail(
                e.span,
                "B236",
                "静态函数引用只能作为集合算法参数或 bind 的首参数，不能作为普通值",
            ),
            ExprKind::Cooperate(..) => {
                self.fail(e.span, "B208", "cooperate 调用只能作为协作根块的局部声明")
            }
            ExprKind::StaticU32(bits) => self.constant_expression(Scalar::U32, Bits::from(*bits)),
            ExprKind::Specialize(name, explicit, args) => self.specialized_expression(e, name, explicit, args),
            ExprKind::Number(n) => self.number_expression(n, e.span),
            ExprKind::Bool(b) => self.constant_expression(Scalar::Bool, Bits::from(*b)),
            ExprKind::Name(name) => self.name_expression(name, e),
            ExprKind::Member(base, field) => {
                let result = self.member_returned(e, base, field)?;
                self.returned_expression(result, e.span)
            }
            ExprKind::Index(base, index) => self.index_expr(base, index, e),
            ExprKind::Unary(op, a) => self.unary_expr(op, a, e.span),
            ExprKind::Binary(op, a, b) => self.binary_expr(op, a, b, e.span),
            ExprKind::Select(c, a, b) => self.selection(c, a, b, e.span),
            ExprKind::Target(target, call) => {
                let result = self.target_call(*target, call, e.span)?;
                self.returned_expression(result, e.span)
            }
            ExprKind::Construct(ty, args) => self.record_expression(ty, args, e.span),
            ExprKind::Call(..) | ExprKind::Invoke(..) => {
                let result = self.evaluate_returned(e, None)?;
                self.returned_expression(result, e.span)
            }
        }
    }
    #[inline(never)]
    fn constant_expression(&mut self, scalar: Scalar, bits: Bits) -> Result<Option<Value>> {
        Ok(Some(self.constant(scalar, bits)))
    }
    #[inline(never)]
    fn name_expression(&mut self, name: &String, e: &Expr) -> Result<Option<Value>> {
        self.name_expr(name, e).map(Some)
    }
    #[inline(never)]
    fn returned_expression(&mut self, result: Option<returns::Returned>, span: Span) -> Result<Option<Value>> {
        if !self.flow.continues() {
            return Ok(None);
        }
        let value = self.ordinary_return(result, span)?;
        self.value_result(value, span).map(Some)
    }
    #[inline(never)]
    fn specialized_expression(&mut self, e: &Expr, name: &str, explicit: &[GenericArgument], args: &[Expr]) -> Result<Option<Value>> {
        if name == "unsafe_function" {
            if self.program.target != Target::Cpu {
                return self.fail(e.span, "B265", "unsafe_function只解释本进程CPU函数地址");
            }
            let ([GenericArgument::Type(signature)], [arg]) = (explicit, args) else {
                return self.fail(e.span, "B153", "unsafe_function<cfn<...>>需要明确签名和一个address实参");
            };
            let ty = self.world.ty(signature, &self.definition.source, e.span, &self.definition.module, &self.definition.imports)?;
            if !ty.is_c_function() { return self.fail(e.span, "B263", "unsafe_function需要cfn原生C签名"); }
            let Some(mut value) = self.expr(arg)? else { return Ok(None); };
            self.require_same(&Ty::builtin("address").unwrap(), &value.ty, arg.span)?;
            value.ty = ty;
            value.proof.retain_view_borrows(&value.ty);
            value.literal = false;
            return Ok(Some(value));
        }
        if name == "unsafe_position" {
            let Some(place) = self.external_position(explicit, args, e.span)? else { return Ok(None); };
            return self.read_place(&place, e.span, None).map(Some);
        }
        if name == "view" { return self.create_application_view(explicit, args, e.span); }
        if self.variant_constructor_type(name,Some(explicit),None,e.span)?.is_some() {
            let value=self.evaluate_returned(e,None)?;return self.ordinary_return(value,e.span);
        }
        let value = self.specialized_call(name, explicit, args, e.span)?;
        if !self.flow.continues() {
            return Ok(None);
        }
        self.value_result(value, e.span).map(Some)
    }
    #[inline(never)]
    fn number_expression(&mut self, n: &str, span: Span) -> Result<Option<Value>> {
        let (t, b) = number(&self.definition.source, span, n)?;
        let scalar = t.components[0].0;
        let mut value = self.literal(scalar, b);
        value.literal = scalar.size() == 4;
        Ok(Some(value))
    }
    fn bitcast(&mut self, name: &str, mut args: Vec<Value>, span: Span) -> Result<Value> {
        if args.len() != 1 {
            return self.fail(span, "B153", format!("{name} 需要一个参数"));
        }
        let (source, target) = match name {
            "bitcast_u32" => (Scalar::F32, Scalar::U32),
            "bitcast_f32" => (Scalar::U32, Scalar::F32),
            "bitcast_u64" => (Scalar::F64, Scalar::U64),
            _ => (Scalar::U64, Scalar::F64),
        };
        let value = args.remove(0);
        let n = value.regs.len();
        if !(1..=if source.size() == 8 { 1 } else { 4 }).contains(&n) || value.ty != Ty::vector(source, n) {
            return self.fail(span, "B154", format!("{name} 需要对应位宽的数值标量；32位也接受向量"));
        }
        let ty = Ty::vector(target, n);
        if let Some(bits) = value.constant {
            let first = self.code.len();
            let mut result = self.constant_value(ty, bits,None);
            for (component_index, (inst, bits)) in self.code[first..].iter_mut()
                .zip(result.constant.iter().flatten()).enumerate() {
                inst.generated_origin = Some(Arc::new(GeneratedOrigin {
                    reason: GenerationReason::BitcastConstantFold,
                    trigger: Some(GenerationAnchor::BitcastConstantFold {
                        expression: inst.source_site.clone(), source_scalar: source, target_scalar: target,
                        bits: *bits, component_index, component_count: n,
                    }),
                }));
            }
            result.literal = false;
            return Ok(result);
        }
        let mut regs = Vec::with_capacity(n);
        for a in value.regs {
            let dst = self.reg(target);
            self.emit(InstKind::Unary {
                dst,
                op: UnaryOp::Bitcast,
                a,
            });
            regs.push(dst);
        }
        Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty,
            regs,
            uniform: value.uniform,
            constant: None,
        })
    }
    fn cast(&mut self, v: Value, s: Scalar) -> Value {
        let constant = v.constant.as_ref().and_then(|values| {
            values.iter().enumerate().map(|(i, bits)| constant::cast(v.ty.components[i].0, s, *bits)).collect::<Option<Vec<_>>>()
        });
        let proof = bounds::Proof::cast(&v, s);
        let mut regs = Vec::new();
        let n = v.regs.len();
        if v.ty.components.iter().any(|(source, _)| *source != s) && let Some(bits) = constant {
            let first = self.code.len();
            let mut value = self.constant_value(Ty::vector(s, n), bits,None);
            for (component_index, (inst, (input_bits, result_bits))) in self.code[first..].iter_mut()
                .zip(v.constant.iter().flatten().zip(value.constant.iter().flatten())).enumerate() {
                let source_scalar = v.ty.components[component_index].0;
                if source_scalar != s {
                    inst.generated_origin = Some(Arc::new(GeneratedOrigin {
                        reason: GenerationReason::CastConstantFold,
                        trigger: Some(GenerationAnchor::CastConstantFold {
                            expression: inst.source_site.clone(), source_scalar, target_scalar: s,
                            input_bits: *input_bits, result_bits: *result_bits,
                            component_index, component_count: n,
                        }),
                    }));
                }
            }
            value.proof = proof;
            return value;
        }
        for src in v.regs {
            if self.registers[src] == s {
                regs.push(src);
            } else {
                let dst = self.reg(s);
                self.emit(InstKind::Cast { dst, src });
                regs.push(dst);
            }
        }
        Value {
            proof,
            literal: false,
            ty: Ty::vector(s, n),
            regs,
            uniform: v.uniform,
            constant,
        }
    }
    fn numeric_operands(&mut self, a: Value, b: Value, span: Span) -> Result<(Value, Value)> {
        let types = operators::numeric(&a.ty, a.literal, &b.ty, b.literal,
            &self.definition.source, span)?;
        let a = if types.cast_left { self.cast(a, types.scalar) } else { a };
        let b = if types.cast_right { self.cast(b, types.scalar) } else { b };
        Ok((a, b))
    }
    #[inline(never)]
    fn binary(&mut self, op: &str, a: Value, b: Value, span: Span, generated_origin: Option<Arc<GeneratedOrigin>>) -> Result<Value> {
        let (ty, kind) = operators::binary(op, &a.ty, a.literal, &b.ty, b.literal,
            &self.definition.source, span)?;
        let operands = match kind {
            operators::Binary::Identity => {
                let mut a = a; let mut b = b;
                a.ty = Ty::scalar(Scalar::U64); b.ty = Ty::scalar(Scalar::U64);
                return self.binary(op, a, b, span,generated_origin.clone());
            }
            operators::Binary::Handle => {
                let mut a = a; let mut b = b;
                a.ty = Ty::vector(Scalar::U32, 2);
                b.ty = Ty::vector(Scalar::U32, 2);
                let bits = self.binary("^", a, b, span,generated_origin.clone())?;
                let word = |component| Value {
                    ty: Ty::scalar(Scalar::U32),
                    regs: vec![bits.regs[component]],
                    proof: bits.projected_proof(&[component]),
                    literal: false, uniform: bits.uniform,
                    constant: bits.constant.as_ref().map(|values| vec![values[component]]),
                };
                let difference = self.binary("|", word(0), word(1), span,generated_origin.clone())?;
                let zero = self.constant(Scalar::U32, 0);
                return self.binary(op, difference, zero, span,generated_origin.clone());
            }
            operators::Binary::Matrix(kind) => return self.matrix_binary(op, a, b, kind, span),
            operators::Binary::Numeric { operands, comparison, operation } => (operands, comparison, operation),
        };
        let (operands, comparison, operation) = operands;
        let literal = a.literal && b.literal && !operands.scalar.is_float() && !comparison;
        let a = if operands.cast_left { self.cast(a, operands.scalar) } else { a };
        let b = if operands.cast_right { self.cast(b, operands.scalar) } else { b };
        let n = ty.components.len();
        let scalar = operands.scalar;
        let divisors = if matches!(op, "/" | "%") && !scalar.is_float() {
            let path=self.residency_path();
            match bounds::integer_divisors(&b,&path) {
                Some(divisors)=>Some(divisors),
                None if self.view_model.pending_values()=>{
                    self.check_view_requirement(view::Requirement::Nonzero{value:Box::new(b.clone()),path,span})?;
                    Some(b.regs.iter().copied().map(IntegerDivisor::Value).collect())
                },
                None=>return self.fail(span,"B152",bounds::INTEGER_DIVISOR_MESSAGE),
            }
        } else {
            None
        };
        let constant = a
            .constant
            .as_ref()
            .zip(b.constant.as_ref())
            .and_then(|(a, b)| {
                (0..n)
                    .map(|i| {
                        let left = a[i % a.len()];
                        let right = b[i % b.len()];
                        if let Some(divisors) = &divisors {
                            match divisors[i % divisors.len()] {
                                IntegerDivisor::Constant(divisor) => {
                                    Some(constant::divide(scalar, left, divisor, op == "%"))
                                }
                                IntegerDivisor::Value(_) => None,
                            }
                        } else {
                            constant::binary(operation.unwrap(), scalar, left, right)
                        }
                    })
                    .collect::<Option<Vec<_>>>()
            });
        let constant_fold = constant.is_some();
        let proof = if let Some(BinaryOp::Compare(op)) = operation {
            bounds::Proof::comparison(op, &a, &b)
        } else {
            bounds::arithmetic(op, &a, &b)
        };
        let constant = constant.or_else(|| {
            if comparison {
                proof.known_value(&ty)
            } else {
                None
            }
        });
        if let Some(bits) = constant {
            let first = self.code.len();
            let mut value = self.constant_value(ty, bits,generated_origin);
            if constant_fold {
                let left = a.constant.as_deref().unwrap();
                let right = b.constant.as_deref().unwrap();
                let operation: Arc<str> = op.into();
                for (component_index, (inst, result_bits)) in self.code[first..].iter_mut()
                    .zip(value.constant.iter().flatten()).enumerate() {
                    inst.generated_origin = Some(Arc::new(GeneratedOrigin {
                        reason: GenerationReason::BinaryConstantFold,
                        trigger: Some(GenerationAnchor::BinaryConstantFold {
                            expression: inst.source_site.clone(), operation: operation.clone(), scalar,
                            result_scalar: value.ty.components[component_index].0,
                            left_bits: left[component_index % left.len()], right_bits: right[component_index % right.len()],
                            result_bits: *result_bits,
                            left_component_index: component_index % left.len(), left_component_count: left.len(),
                            right_component_index: component_index % right.len(), right_component_count: right.len(),
                            component_index, component_count: value.regs.len(), retained: inst.generated_origin.clone(),
                        }),
                    }));
                }
            }
            value.literal = literal;
            value.proof = proof;
            return Ok(value);
        }
        let mut regs = Vec::new();
        for i in 0..n {
            let dst = self.reg(if comparison { Scalar::Bool } else { scalar });
            let ar = a.regs[i % a.regs.len()];
            let br = b.regs[i % b.regs.len()];
            let kind = if let Some(divisors) = &divisors {
                let divisor = divisors[i % divisors.len()];
                if scalar.is_signed()
                    && matches!(divisor, IntegerDivisor::Constant(d) if d.get() == scalar.truncate(Bits::MAX))
                {
                    if op == "/" {
                        InstKind::Unary { dst, op: UnaryOp::SignedNegate, a: ar }
                    } else {
                        InstKind::Constant(dst, scalar, 0)
                    }
                } else {
                    InstKind::IntegerDivide {
                        result: if op == "/" { IntegerDivisionResult::Quotient(dst) }
                            else { IntegerDivisionResult::Remainder(dst) },
                        src: ar, divisor,
                    }
                }
            } else {
                InstKind::Binary { dst, op: operation.unwrap(), a: ar, b: br }
            };
            if let Some(origin) = &generated_origin { self.code.push(Inst::generated(kind, origin.clone())); }
            else { self.emit(kind); }
            regs.push(dst);
        }
        let mut value = Value {
            proof,
            literal,
            ty,
            regs,
            uniform: a.uniform && b.uniform,
            constant: None,
        };
        self.observe_integer_binary(op, &a, &b, &mut value);
        Ok(value)
    }
    fn math(&mut self, op: &str, mut args: Vec<Value>, span: Span) -> Result<Value> {
        let arity = if matches!(op, "min" | "max" | "pow") {
            2
        } else {
            1
        };
        if args.len() != arity {
            return self.fail(span, "B153", format!("{op} 需要 {arity} 个参数"));
        }
        if matches!(op, "min" | "max")
            && args.iter().all(|a| {
                a.ty.is_numeric() && a.ty.components[0].0.is_integer()
            })
        {
            let mut args = args.into_iter();
            let (a, b) = self.numeric_operands(args.next().unwrap(), args.next().unwrap(), span)?;
            let n = a.regs.len().max(b.regs.len());
            let scalar = a.ty.components[0].0;
            let ty = Ty::vector(scalar, n);
            let proof = bounds::arithmetic(op, &a, &b);
            let literal = a.literal && b.literal;
            if let Some(known) = proof.known_integer_value(&ty) {
                let first = self.code.len();
                let mut value = self.constant_value(ty, known.iter().map(|(bits, _)| *bits).collect(),None);
                for (component_index, (inst, (result_bits, basis))) in self.code[first..].iter_mut().zip(known).enumerate() {
                    let ai = component_index % a.regs.len();
                    let bi = component_index % b.regs.len();
                    inst.generated_origin = Some(Arc::new(GeneratedOrigin {
                        reason: GenerationReason::MinmaxKnownValue,
                        trigger: Some(GenerationAnchor::MinmaxKnownValue {
                            expression: inst.source_site.clone(), operation: op.into(), scalar,
                            left_ranges: a.component_range_intervals(ai), right_ranges: b.component_range_intervals(bi),
                            left_constant_present: a.constant.is_some(), right_constant_present: b.constant.is_some(),
                            same_value: a.same_component(ai, &b, bi), result_bits, basis,
                            left_component_index: ai, left_component_count: a.regs.len(),
                            right_component_index: bi, right_component_count: b.regs.len(),
                            component_index, component_count: n,
                        }),
                    }));
                }
                value.proof = proof;
                value.literal = literal;
                return Ok(value);
            }
            let comparison = self.binary(
                if op == "min" { "<" } else { ">" },
                a.clone(),
                b.clone(),
                span,
                None,
            )?;
            let mut regs = Vec::with_capacity(n);
            for i in 0..n {
                let dst = self.reg(scalar);
                self.emit(InstKind::Select {
                    dst,
                    condition: comparison.regs[i],
                    a: a.regs[i % a.regs.len()],
                    b: b.regs[i % b.regs.len()],
                });
                regs.push(dst);
            }
            return Ok(Value {
                proof,
                literal,
                ty,
                regs,
                uniform: a.uniform && b.uniform,
                constant: None,
            });
        }
        if args.len() == 2 {
            let b = args.pop().unwrap();
            let a = args.pop().unwrap();
            let (a,b) = self.numeric_operands(a,b,span)?;
            args = vec![a,b];
        }
        let scalar = args[0].ty.components[0].0;
        let n = args.iter().map(|v| v.regs.len()).max().unwrap();
        for a in &args {
            if !a.ty.components[0].0.is_float() || a.ty.components[0].0 != scalar
                || !a.ty.fields.is_empty()
                || a.ty.array.is_some()
                || (a.regs.len() != 1 && a.regs.len() != n)
            {
                return self.fail(span, "B154", format!("{op} 需要相同浮点类型；f32 向量须维度相容"));
            }
        }
        let uniform = args.iter().all(|v| v.uniform);
        let operation = MathOp::from_source(op).ok_or_else(|| Diagnostic {
            source: self.definition.source.clone(),
            span,
            code: "B179",
            message: format!("未知数学运算 {op}"),
        })?;
        let mut regs = Vec::new();
        for i in 0..n {
            let dst = self.reg(scalar);
            self.emit(InstKind::Math {
                dst,
                op: operation,
                args: args.iter().map(|a| a.regs[i % a.regs.len()]).collect(),
            });
            regs.push(dst);
        }
        Ok(Value {
            proof: bounds::math(operation, &args),
            literal: false,
            ty: Ty::vector(scalar, n),
            regs,
            uniform,
            constant: None,
        })
    }
    fn builtin_call(&mut self, name: &str, args: &[Expr], span: Span) -> Result<Option<Value>> {
        if name == "c_function" { return self.c_function_value(args, span).map(Some); }
        if matches!(name, "Ok" | "Err") {
            return self.fail(
                span,
                "B269",
                "Ok／Err 需要 result<T,E> 上下文或两个显式类型实参",
            );
        }
        if matches!(name, "upload" | "download") {
            return self.application_transfer(name, args, span);
        }
        if name == "println" {
            return self.println(args, span);
        }
        if name == "texture_sample" {
            return self.texture_sample(args, span);
        }
        if name == "append" {
            return self.append(args, span);
        }
        if TileOp::from_source(name).is_some() {
            return self.fail(
                span,
                "B205",
                "tile 集合运算只能作为 parallel 根块的局部变量声明",
            );
        }
        if name == "tile_publish" {
            return self.fail(
                span,
                "B203",
                "tile_publish 只能作为 parallel 根块的 let 发布声明",
            );
        }
        if name == "tile_input" {
            return self.fail(
                span,
                "B198",
                "tile_input 只能作为 parallel 开头的 let 输入声明，不能创建普通值或在控制流中构造",
            );
        }
        if let Some(value) = self.subgroup(name, args, span) {
            return value;
        }
        if matches!(name, "trace_closest" | "trace_any") {
            return self.ray_query_call(name, args, span);
        }
        if name == "unsafe_read" {
            return self.unsafe_read_call(args, span);
        }
        if name == "ordinal" {
            return self.ordinal_call(args, span);
        }
        if name == "index" {
            if !args.is_empty() || self.in_function {
                return self.fail(
                    span,
                    "B155",
                    "index() 只能在当前 program 的 parallel 域中使用",
                );
            }
            return Ok(Some(Value {
                proof: self.path_proof(
                    &Ty::scalar(Scalar::U32),
                    self.data_index_proof(self.domain)
                        .with_domains(Domains::from([IndexDomain::Data(self.domain)])).with_identity(ValueIdentity::Register(self.global_index)),
                ),
                literal: false,
                ty: Ty::scalar(Scalar::U32),
                regs: vec![self.global_index],
                uniform: false,
                constant: None,
            }));
        }
        if matches!(
            name,
            "bind" | "barrier" | "group_barrier" | "compact" | "stable_sort_by_key" | "ddx" | "ddy"
        ) || custom_reduce::Kind::from_source(name).is_some()
            || CollectiveOperator::from_scan_source(name).is_some()
            || CollectiveOperator::from_segmented_scan_source(name).is_some()
            || CollectiveOperator::from_reduction_source(name).is_some()
            || CollectiveOperator::from_segmented_reduction_source(name).is_some()
        {
            return self.fail(
                span,
                "B156",
                format!("{name} 的作用域／能力语义尚未实现，当前编译器明确拒绝"),
            );
        }
        if name == "atomic_store" {
            return self.fail(span, "B218", "atomic_store 是语句，没有可绑定的返回值");
        }
        if name == "tile_barrier" {
            return self.fail(span, "B218", "tile_barrier 是语句，没有可绑定的返回值");
        }
        if matches!(
            name,
            "atomic_load"
                | "atomic_add"
                | "atomic_sub"
                | "atomic_exchange"
                | "atomic_compare_exchange"
                | "atomic_min"
                | "atomic_max"
                | "atomic_and"
                | "atomic_or"
                | "atomic_xor"
        ) {
            return self.atomic_value(name, args, span);
        }
        if self
            .world
            .resolve(
                name,
                &self.definition.module,
                &self.definition.imports,
                false,
            )
            .is_some()
        {
            return self.record_expression(&TypeName::named(name), args, span);
        }
        if name == "address_of" { return self.position_address(args, span); }
        let values = args.iter().map(|arg| self.expr(arg));
        if name == "function_address" {
            let Some(mut values) = values.collect::<Result<Option<Vec<_>>>>()? else { return Ok(None); };
            if self.program.target != Target::Cpu { return self.fail(span, "B265", "function_address只取得本进程CPU函数地址"); }
            if values.len() != 1 { return self.fail(span, "B153", "function_address需要一个cfn实参"); }
            let mut value = values.pop().unwrap();
            if !value.ty.is_c_function() { return self.fail(span, "B263", "function_address需要cfn原生C签名值"); }
            value.ty = Ty::builtin("address").unwrap();
            value.proof = value.proof.with_external_borrows(&value.ty);
            value.literal = false;
            return Ok(Some(value));
        }
        if name == "unsafe_handle" {
            let Some(mut values) = values.collect::<Result<Option<Vec<_>>>>()? else { return Ok(None); };
            if values.len() != 1 {
                return self.fail(span, "B153", "unsafe_handle需要一个usize实参");
            }
            let value = values.pop().unwrap();
            self.require_same(&Ty::scalar(Scalar::U64), &value.ty, span)?;
            let low = self.cast(value.clone(), Scalar::U32);
            let shift = self.constant(Scalar::U64, 32);
            let high = self.binary(">>", value, shift, span,None)?;
            let high = self.cast(high, Scalar::U32);
            let constant = low.constant.as_ref().zip(high.constant.as_ref())
                .map(|(low, high)| vec![low[0], high[0]]);
            return Ok(Some(Value {
                ty: Ty::builtin("handle").unwrap(),
                regs: vec![low.regs[0], high.regs[0]],
                proof: bounds::Proof::default(),
                literal: false,
                uniform: low.uniform && high.uniform,
                constant,
            }));
        }
        if matches!(name, "unsafe_address" | "address_bits") {
            let Some(mut values) = values.collect::<Result<Option<Vec<_>>>>()? else { return Ok(None); };
            if self.program.target != Target::Cpu {
                return self.fail(span, "B265", "CPU本地地址只能在on cpu位置解释或取得位模式");
            }
            if values.len() != 1 {
                return self.fail(span, "B153", format!("{name}需要一个实参"));
            }
            let address = Ty::builtin("address").unwrap();
            let integer = Ty::scalar(Scalar::U64);
            let (input, output) = if name == "unsafe_address" {
                (integer, address)
            } else {
                (address, integer)
            };
            let mut value = values.pop().unwrap();
            self.require_same(&input, &value.ty, span)?;
            value.ty = output;
            if name == "unsafe_address" {
                value.proof = value.proof.with_external_borrows(&value.ty);
            } else {
                value.proof.retain_view_borrows(&value.ty);
            }
            value.literal = false;
            return Ok(Some(value));
        }
        if let Some(ty) = Ty::builtin(name) {
            let Some(values) = values.collect::<Result<Option<Vec<_>>>>()? else { return Ok(None); };
            return self.value_constructor(ty, values, span).map(Some);
        }
        if matches!(name, "bitcast_u32" | "bitcast_f32" | "bitcast_u64" | "bitcast_f64") {
            let Some(values) = values.collect::<Result<Option<Vec<_>>>>()? else { return Ok(None); };
            return self.bitcast(name, values, span).map(Some);
        }
        if matches!(
            name,
            "sqrt"
                | "abs"
                | "floor"
                | "sin"
                | "cos"
                | "exp"
                | "exp2"
                | "log"
                | "log2"
                | "min"
                | "max"
                | "pow"
        ) {
            let Some(values) = values.collect::<Result<Option<Vec<_>>>>()? else { return Ok(None); };
            return self.math(name, values, span).map(Some);
        }
        if matches!(name, "transpose" | "determinant" | "inverse") {
            let Some(values) = values.collect::<Result<Option<Vec<_>>>>()? else { return Ok(None); };
            return self.matrix_intrinsic(name, values, span).map(Some);
        }
        if name == "dot" {
            let Some(values) = values.collect::<Result<Option<Vec<_>>>>()? else { return Ok(None); };
            return self.dot_value(values, span).map(Some);
        }
        if matches!(name, "length" | "normalize") {
            let Some(values) = values.collect::<Result<Option<Vec<_>>>>()? else { return Ok(None); };
            return self
                .length_value(values, name == "normalize", span)
                .map(Some);
        }
        if name == "cross" {
            let Some(values) = values.collect::<Result<Option<Vec<_>>>>()? else { return Ok(None); };
            return self.cross_value(values, span).map(Some);
        }
        if name == "clamp" {
            let Some(values) = values.collect::<Result<Option<Vec<_>>>>()? else { return Ok(None); };
            if values.len() != 3 {
                return self.fail(span, "B153", "clamp 需要三个参数");
            }
            let a = self.math("max", vec![values[0].clone(), values[1].clone()], span)?;
            return self.math("min", vec![a, values[2].clone()], span).map(Some);
        }
        self.fail(span, "B158", format!("未知或未导出的函数或能力 {name}"))
    }
    #[inline(never)]
    fn unary_expr(&mut self, op: &String, a: &Expr, span: Span) -> Result<Option<Value>> {
        if op == "-" && let ExprKind::Number(literal) = &a.kind {
            let (ty, bits) = operators::negative_number(&self.definition.source, span, literal)?;
            let scalar = ty.components[0].0;
            let mut value = self.constant_value(ty, vec![bits],None);
            value.literal = scalar.size() == 4;
            return Ok(Some(value));
        }
        let Some(a) = self.expr(a)? else { return Ok(None); };
        let Some(operation) = operators::unary(op, &a.ty, &self.definition.source, span)? else {
            return Ok(Some(a));
        };
        let scalar = a.ty.components[0].0;
        let constant = a.constant.as_ref().map(|values| {
            values
                .iter()
                .map(|bits| constant::unary(operation, scalar, *bits))
                .collect()
        });
        let proof = bounds::unary(operation, &a);
        if let Some(bits) = constant {
            let first = self.code.len();
            let mut value = self.constant_value(a.ty, bits,None);
            for (component_index, (inst, (input_bits, result_bits))) in self.code[first..].iter_mut()
                .zip(a.constant.iter().flatten().zip(value.constant.iter().flatten())).enumerate() {
                inst.generated_origin = Some(Arc::new(GeneratedOrigin {
                    reason: GenerationReason::UnaryConstantFold,
                    trigger: Some(GenerationAnchor::UnaryConstantFold {
                        expression: inst.source_site.clone(), operation, scalar,
                        input_bits: *input_bits, result_bits: *result_bits,
                        component_index, component_count: value.regs.len(),
                    }),
                }));
            }
            value.literal = a.literal;
            value.proof = proof;
            return Ok(Some(value));
        }
        let mut regs = Vec::new();
        for x in a.regs {
            let dst = self.reg(self.registers[x]);
            self.emit(InstKind::Unary {
                dst,
                op: operation,
                a: x,
            });
            regs.push(dst);
        }
        Ok(Some(Value {
            proof,
            literal: a.literal,
            ty: a.ty,
            regs,
            uniform: a.uniform,
            constant: None,
        }))
    }
    #[inline(never)]
    fn binary_expr(&mut self, op: &String, a: &Expr, b: &Expr, span: Span) -> Result<Option<Value>> {
        if op == "&&" || op == "||" { return self.short_circuit_expr(op,a,b,span); }
        let Some(a) = self.expr(a)? else { return Ok(None); };
        self.binary_after_left(op,Box::new(a),b,span)
    }
    #[inline(never)]
    fn short_circuit_expr(&mut self, op: &String, a: &Expr, b: &Expr, span: Span) -> Result<Option<Value>> {
        let skipped = Expr {
            kind: ExprKind::Bool(op == "||"),
            span,
        };
        let (yes, no) = if op == "&&" {
            (b, &skipped)
        } else {
            (&skipped, b)
        };
        let result = self.selection(a, yes, no, span)?;
        if let Some(value) = &result {
            self.require_bool(value, span)?;
        }
        Ok(result)
    }
    #[inline(never)]
    fn binary_after_left(&mut self, op: &String, a: Box<Value>, b: &Expr, span: Span) -> Result<Option<Value>> {
        let Some(b) = self.expr(b)? else { return Ok(None); };
        self.binary_operands(op,a,b,span)
    }
    #[inline(never)]
    fn binary_operands(&mut self, op: &str, a: Box<Value>, b: Value, span: Span) -> Result<Option<Value>> {
        self.binary(op,*a,b,span,None).map(Some)
    }
    #[inline(never)]
    fn if_statement(
        &mut self,
        condition: &Expr,
        yes_body: &[Stmt],
        no_body: &[Stmt],
        span: Span,
    ) -> Result<()> {
        let Some(c) = self.temporary_scope(condition.span, |builder| Ok((builder.expr(condition)?, view::Borrow::default())))? else {
            return Ok(());
        };
        self.require_bool(&c, condition.span)?;
        self.conditional_blocks(c, |b| b.block(yes_body), |b| b.block(no_body), span)
    }
    fn conditional_blocks(
        &mut self,
        c: Value,
        yes_body: impl FnOnce(&mut Self) -> Result<()>,
        no_body: impl FnOnce(&mut Self) -> Result<()>,
        span: Span,
    ) -> Result<()> {
        let partition = Arc::new(bounds::Condition::of(&c));
        if let Some(truth) = partition.boolean_constant() {
            self.refine_bounds(&partition, truth);
            return if truth { yes_body(self) } else { no_body(self) };
        }
        self.remember_loop_control(&c);
        let environment = self.env.clone();
        let path = self.path.clone();
        let prefix = self.code.len();
        let init = self.application_initialization_state();
        let control = self.control_uniform;
        self.control_uniform &= c.uniform;
        self.refine_bounds(&partition, true);
        yes_body(self)?;
        let yes = self.code.split_off(prefix);
        let yes_init = self.take_application_initialization_state();
        let yes_env = self.env.clone();
        let yes_path = self.path.clone();
        let yes_flow = self.flow;
        let yes_reachable = yes_flow.continues();
        self.flow = Flow::NEXT;
        self.initialized = init;
        self.env = environment.clone();
        self.path = path;
        self.refine_bounds(&partition, false);
        no_body(self)?;
        let no = self.code.split_off(prefix);
        let no_init = self.take_application_initialization_state();
        let no_flow = self.flow;
        let no_reachable = no_flow.continues();
        let mut no_env = self.env.clone();
        let no_path = self.path.clone();
        if yes_reachable && !no_reachable {
            self.path = yes_path;
            self.env = yes_env;
            self.initialized = yes_init;
        } else if !yes_reachable && no_reachable {
            self.path = no_path;
            self.env = no_env;
            self.initialized = no_init;
        } else if yes_reachable && no_reachable {
            self.path = bounds::Clause::either(&yes_path, &no_path);
            self.view_uses.after(span);
            self.env = yes_env;
            self.join_initialization(&c, yes_init, no_init);
            self.join_environment_selected(&mut no_env, &owned_presence::Presence::input(&c));
        } else {
            self.path = bounds::Clause::impossible();
            self.env = environment;
            self.join_initialization(&c, yes_init, no_init);
        }
        self.flow = yes_flow.union(no_flow);
        self.control_uniform = control && (c.uniform || (yes_reachable && no_reachable));
        self.emit(InstKind::If {
            condition: c.regs[0],
            yes,
            no,
        });

        Ok(())
    }
    fn while_statement(&mut self, condition: &Expr, body: &[Stmt], span: Span) -> Result<()> {
        let initial = self.application_initial_loop(body)?;
        self.while_loop(condition, body, initial, span)
    }
    fn while_loop(&mut self, condition: &Expr, body: &[Stmt], initial: bool, span: Span) -> Result<()> {
        self.flow = Flow::NEXT;
        let environment = self.env.clone();
        let writes=self.loop_writes(Some(condition),body,None,initial)?;
        self.enter_loop(&environment);
        self.loop_contexts.last_mut().unwrap().writes=writes;
        self.configure_initial_loop(initial,None,None);
        self.prepare_view_loop();
        self.loop_contexts.last_mut().unwrap().outer=std::mem::take(&mut self.code);
        let Some(c) = self.temporary_scope(condition.span, |builder| Ok((builder.expr(condition)?, view::Borrow::default())))? else {
            let outer=std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer);
            self.finish_view_condition()?;
            let code = std::mem::replace(&mut self.code, outer);
            self.code.extend(code);
            return Ok(());
        };
        self.require_bool(&c, condition.span)?;
        self.loop_contexts.last_mut().unwrap().control_source=view_contracts::LoopControl::capture(&c,&self.env,None);
        let partition = Arc::new(bounds::Condition::of(&c));
        let condition_code = std::mem::take(&mut self.code);
        if c.constant.as_ref().is_some_and(|bits| bits[0] == 0) {
            let outer=std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer);
            self.finish_view_condition()?;
            self.refine_bounds(&partition, false);
            self.code = outer;
            self.code.extend(condition_code);
            return Ok(());
        }
        self.code = condition_code;
        let condition_prefix = self.code.len();
        let mut condition_env = self.env.clone();
        let condition_path = self.path.clone();
        self.refine_bounds(&partition, true);
        if !initial { self.retain_loop_bounds(Some(condition), body, &environment, &mut condition_env)?; }
        let body_env = std::mem::replace(&mut self.env, condition_env);
        let body_path = std::mem::replace(&mut self.path, condition_path);
        let natural = if c.constant.as_ref().is_some_and(|bits| bits[0] != 0) {
            None
        } else {
            self.refine_bounds(&partition, false);
            Some(self.loop_state())
        };
        self.env = body_env;
        self.path = body_path;
        let control = self.control_uniform;
        self.control_uniform &= c.uniform;
        self.loop_depth += 1;
        self.iterators.push(None);
        self.block(body)?;
        let mut body_flow = self.flow;
        if body_flow.continues() {
            self.check_application_loop(condition.span)?;
        }
        self.iterators.pop();
        self.loop_depth -= 1;
        let mut code = self.code.split_off(condition_prefix);
        let condition_code = std::mem::take(&mut self.code);
        if initial {
            let (expanded, flow) =
                self.expand_initial_loop(code, body_flow.continues(), |builder, _| {
                    let initial=builder.next_initial_loop(body)?;
                    builder.while_loop(condition, body, initial, span)
                })?;
            code = expanded;
            body_flow = body_flow.union(flow);
        }
        let body = code;
        let outer=std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer);
        let next = self.leave_loop(natural, &c, &body, span, false)?;
        self.flow = body_flow.loop_result(next);
        self.control_uniform = control;
        self.code = outer;
        self.emit(InstKind::Loop {
            condition: condition_code,
            test: c.regs[0],
            body,
        });

        Ok(())
    }
    #[inline(never)]
    fn for_statement(
        &mut self,
        name: &String,
        start: &Expr,
        end: &Option<Expr>,
        body: &[Stmt],
        span: Span,
    ) -> Result<()> {
        if self.env.contains_key(name) {
            return self.fail(span, "B125", "循环变量不能覆盖已有绑定");
        }
        let (begin, limit, data) = if let Some(end) = end {
            let Some(a) = self.temporary_scope(start.span, |builder| Ok((builder.expr(start)?, view::Borrow::default())))? else {
                return Ok(());
            };
            let Some(b) = self.temporary_scope(end.span, |builder| Ok((builder.expr(end)?, view::Borrow::default())))? else {
                return Ok(());
            };
            self.require_same(&Ty::scalar(Scalar::U32), &a.ty, start.span)?;
            self.require_same(&a.ty, &b.ty, start.span)?;
            (a, b, None)
        } else {
            let source = if let Some(value) = self.application_reference(start)? {
                Some(Binding::DataReference(value))
            } else if let ExprKind::Name(name) = &start.kind {
                self.env.get(name).cloned()
            } else {
                self.dataset_reference(start)?.map(Binding::Data)
            };
            let begin = self.constant(Scalar::U32, 0);
            match source {
                Some(Binding::DataReference(data)) => {
                    self.application_cpu(&data, start.span)?;
                    (
                        begin,
                        data.length.clone(),
                        Some(CursorSource::ApplicationData(data)),
                    )
                }
                Some(Binding::Tile(view)) => (
                    begin,
                    self.tile_length(&view),
                    Some(CursorSource::Tile(view)),
                ),
                Some(Binding::Data(view)) => {
                    if !matches!(view.access, Access::Read | Access::InOut | Access::Atomic) {
                        return self.fail(
                            start.span,
                            "B128",
                            "只读顺序遍历需要 read／inout 或 atomic 数据集；atomic 游标仍须显式原子访问",
                        );
                    }
                    (
                        begin,
                        self.data_length(&view),
                        Some(CursorSource::Data(view)),
                    )
                }
                _ => {
                    let Some(view) = self.expr(start)? else {
                        return Ok(());
                    };
                    if view.ty.view.is_none() {
                        return self.fail(
                            start.span,
                            "B127",
                            "顺序遍历需要 dataset、tile 或不可变 view",
                        );
                    }
                    (
                        begin,
                        Self::view_length(&view),
                        Some(CursorSource::View(view)),
                    )
                }
            }
        };
        if begin.minimum() >= limit.maximum() {
            return Ok(());
        }
        let nonempty = begin.maximum() < limit.minimum();
        let borrowed = match &data {
            Some(CursorSource::ApplicationData(data)) => Some(data.root.identity()),
            _ => None,
        };
        let initial = self.application_initial_loop(body)?;
        let slot = self.local_slots(&Ty::scalar(Scalar::U32)).0[0];
        self.emit(InstKind::StoreLocal(slot, begin.regs[0]));
        let initialization = self.application_iteration_outputs(&begin, &limit, match &data {Some(CursorSource::ApplicationData(data))=>Some(&data.data),_=>None});
        self.for_loop(
            ForLoop {
                name: name.clone(),
                begin: begin.clone(),
                limit,
                data,
                borrowed,
                slot,
                initialization,
            },
            body,
            span,
            initial,
            nonempty,
            &begin,
        )
    }
    fn for_loop(
        &mut self,
        cursor_specification: ForLoop,
        body: &[Stmt],
        span: Span,
        initial: bool,
        nonempty: bool,
        original_begin: &Value,
    ) -> Result<()> {
        self.flow = Flow::NEXT;
        let ForLoop {
            name,
            begin,
            limit,
            data,
            borrowed,
            slot,
            mut initialization,
        } = cursor_specification.clone();
        if begin.minimum() >= limit.maximum() || begin.identity() == limit.identity() {
            for (data, words) in initialization {
                self.initialize_application_words(&data, &words);
            }
            return Ok(());
        }
        if bounds::seeks(body) {
            initialization.clear();
        }
        let environment = self.env.clone();
        let mut writes=self.loop_writes(None,body,borrowed,initial)?;
        writes.locals.insert(slot);
        self.enter_loop(&environment);
        self.loop_contexts.last_mut().unwrap().writes=writes;
        self.configure_initial_loop(initial, Some(name.clone()), Some(begin.clone()));
        self.prepare_view_loop();
        self.loop_contexts.last_mut().unwrap().outer=std::mem::take(&mut self.code);
        let index = self.reg(Scalar::U32);
        self.emit(InstKind::LoadLocal(index, slot));
        let cursor = Value {
            proof: if initial {
                bounds::Proof::capture(&begin)
            } else {
                bounds::Proof::default()
            },
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![index],
            uniform: begin.uniform && limit.uniform,
            constant: if initial {
                begin.constant.clone()
            } else {
                None
            },
        };
        let test = self.binary("<", cursor.clone(), limit.clone(), span,None)?;
        let condition = std::mem::take(&mut self.code);
        let body_cursor=Value {
            proof:if initial {bounds::Proof::capture(&begin)}else{bounds::for_cursor(&begin,&limit,body)},
            ..cursor.clone()
        };
        if !bounds::seeks(body) {
            self.refine_bounds(&Arc::new(bounds::Condition::comparison(Comparison::LessEqual, original_begin, &cursor)), true);
        }
        let binding = if let Some(domain) = data {
            match domain {
                CursorSource::ApplicationData(data) => Binding::Owned(OwnedBinding {
                    place: data.element(
                        &cursor,
                        (
                            cursor.minimum(),
                            cursor
                                .maximum()
                                .min(data.length.maximum().saturating_sub(1)),
                        ),
                    ),
                    proof: bounds::Proof::default(),
                    uniform: false,
                    initialization: None,
                }),
                CursorSource::Data(view) => {
                    Binding::Owned(OwnedBinding {
                        place: self.dataset_element(&view, index, false),
                        proof: bounds::Proof::default(),
                        uniform: false,
                        initialization: None,
                    })
                },
                CursorSource::Tile(view) => {
                    let place = Place::TileElement {
                        ty: view.ty,
                        tile: view.tile,
                        index,
                        words: view.words,
                    };
                    let width = place.ty().components.len();
                    let mut proof = bounds::Proof::default();
                    for (component, (scalar, _)) in place.ty().components.iter().enumerate() {
                        proof.assign(&[component], width, bounds::Proof::default()
                            .with_identity(ValueIdentity::Register(self.reg(*scalar))));
                    }
                    Binding::Owned(OwnedBinding { place, proof, uniform: false, initialization: None })
                },
                CursorSource::View(view) => {
                    let place = Self::view_element(&view, index);
                    let width = place.ty().components.len();
                    let mut proof = bounds::Proof::default();
                    // 同一只读行的内容身份从游标建立时存在，不随分支是否先装载而改变。
                    for (component, (scalar, _)) in place.ty().components.iter().enumerate() {
                        proof.assign(&[component], width, bounds::Proof::default()
                            .with_identity(ValueIdentity::Register(self.reg(*scalar))));
                    }
                    Binding::ViewElement {
                        element: OwnedBinding { place, proof, uniform: false, initialization: None },
                        view,
                    }
                },
            }
        } else {
            Binding::Value(body_cursor.clone())
        };
        self.env.insert(name.clone(), binding);
        self.loop_contexts.last_mut().unwrap().control_source=view_contracts::LoopControl::capture(&test,&self.env,Some(body_cursor));
        let iteration = match self.env.get(&name) {
            Some(Binding::Value(value)) => ownership::ApplicationIndex::of(value),
            _ => ownership::ApplicationIndex::of(&cursor),
        };
        let condition_env = self.env.clone();
        let condition_path = self.path.clone();
        let partition = Arc::new(bounds::Condition::of(&test));
        self.refine_bounds(&partition, false);
        let mut natural = self.loop_state();
        self.env = condition_env;
        self.path = condition_path;
        self.refine_bounds(&partition, true);
        if !initial {self.retain_loop_bounds(None,body,&environment,&mut natural.env)?;}
        let control = self.control_uniform;
        self.control_uniform &= begin.uniform && limit.uniform;
        self.loop_depth += 1;
        self.iterators.push(Some((name.clone(), slot)));
        self.block(body)?;
        let mut body_flow = self.flow;
        let body_reachable = body_flow.continues();
        if body_reachable {
            self.advance_iterator(index, slot)?;
            self.check_application_loop(span)?;
        }
        self.iterators.pop();
        self.loop_depth -= 1;
        let mut code = std::mem::take(&mut self.code);
        self.initialize_iteration_exit(&mut natural, &initialization, iteration, initial);
        self.initialize_array_iteration_exit(
            &mut natural,
            &begin,
            &limit,
            iteration,
            initial,
            body,
        );
        self.defer_iteration_exit(&mut natural,&begin,&limit,&initialization,iteration,initial,body);
        let natural = (!nonempty || (!initial && body_flow.iterates())).then_some(natural);
        if initial {
            let specification = cursor_specification.clone();
            let (expanded, flow) =
                self.expand_initial_loop(code, body_reachable, |builder, next| {
                    let mut cursor = specification.clone();
                    cursor.initialization =
                        builder.application_iteration_prefix(&initialization, iteration);
                    cursor.begin = next.unwrap();
                    let initial=builder.next_initial_loop(body)?;
                    let nonempty=cursor.begin.maximum()<cursor.limit.minimum();
                    builder.for_loop(cursor, body, span, initial, nonempty, original_begin)
                })?;
            code = expanded;
            body_flow = body_flow.union(flow);
        }
        let body = code;
        let outer=std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer);
        let next = self.leave_loop(natural, &test, &body, span, nonempty)?;
        self.flow = body_flow.loop_result(next);
        self.control_uniform = control;
        self.code = outer;
        self.emit(InstKind::Loop {
            condition,
            test: test.regs[0],
            body,
        });

        Ok(())
    }
    #[inline(never)]
    fn let_statement(
        &mut self,
        annotation: &Option<TypeName>,
        name: &String,
        e: &Expr,
        span: Span,
    ) -> Result<()> {
        if self.env.contains_key(name) {
            return self.fail(span, "B125", format!("重复局部变量或覆盖参数 {name}"));
        }
        if let ExprKind::Call(call, args) = &e.kind {
            if matches!(call.as_str(), "tile_publish" | "tile_atomic") {
                return self.tile_publish(
                    annotation.is_some(),
                    name,
                    args,
                    span,
                    call == "tile_atomic",
                );
            }
        }
        let expected = annotation
            .as_ref()
            .map(|ty| self.expected_storage(ty, span))
            .transpose()?;
        let result = if let ExprKind::Cooperate(call, explicit, args) = &e.kind {
            self.cooperate(call, explicit.as_deref(), args, e.span)?
                .map(returns::Returned::value)
        } else if let ExprKind::Call(call, args) = &e.kind {
            if let Some(op) = TileOp::from_source(call) {
                Some(returns::Returned::value(
                    self.tile_collective(op, args, span)?,
                ))
            } else {
                self.evaluate_returned(e, expected.as_ref())?
            }
        } else {
            self.evaluate_returned(e, expected.as_ref())?
        };
        if !self.flow.continues() {
            return Ok(());
        }
        let mut value = result.ok_or_else(|| Diagnostic {
            source: self.definition.source.clone(),
            span,
            code: "B228",
            message: "局部绑定需要值或拥有资源".into(),
        })?;
        if let Some(expected) = expected {
            self.check_returned_type(&mut value, &expected, span)?;
        }
        self.bind_returned(name, value, span)
    }
    fn bind_local_value(&mut self, name: &String, value: Value, span: Span) -> Result<()> {
        self.require_view_available(&value, &[], span)?;
        if self.env.contains_key(name) {
            return self.fail(span, "B125", format!("重复局部变量或覆盖参数 {name}"));
        }
        if !self.mutable_names.contains(name) && value.ty.contains_arrays() {
            let mut owner = self.materialize_array(value,None);
            owner.place = owner.place.read_only();
            owner.uniform &= self.control_uniform;
            self.env.insert(name.clone(), Binding::Owned(owner));
            return Ok(());
        }
        if !self.mutable_names.contains(name) {
            let mut value = value;
            value.literal = false;
            value.uniform &= self.control_uniform;
            self.env.insert(name.clone(), Binding::Value(value));
            return Ok(());
        }
        let (slots, storage) = self.local_slots(&value.ty);
        for (slot, r) in slots.iter().zip(&value.regs) {
            self.emit(InstKind::StoreLocal(*slot, *r));
        }
        let proof = bounds::Proof::capture(&value);
        let place = Place::Local(value.ty, slots, storage);
        self.initialize(&place);
        self.env.insert(
            name.clone(),
            Binding::Owned(OwnedBinding {
                proof,
                place,
                uniform: value.uniform && self.control_uniform,
                initialization: None,
            }),
        );

        Ok(())
    }
    fn value_constructor(&mut self, ty: Ty, mut values: Vec<Value>, span: Span) -> Result<Value> {
        if ty.is_c_function() {
            if values.is_empty() { return Ok(self.constant_value(ty, vec![0],None)); }
            if values.len() == 1 && values[0].ty == ty {
                let mut value = values.pop().unwrap(); value.literal = false; return Ok(value);
            }
            return self.fail(span, "B154", "cfn构造只接受空值或同签名函数值；原生地址解释须显式unsafe_function");
        }
        if values.iter().any(|value| value.ty.is_c_function()) {
            return self.fail(span, "B154", "C函数地址不能转换为数值或其他身份；地址解释须显式function_address");
        }
        if ty.is_address() {
            if values.is_empty() {
                return Ok(self.constant_value(ty, vec![0],None));
            }
            if values.len() == 1 && values[0].ty == ty {
                let mut value = values.pop().unwrap();
                value.literal = false;
                return Ok(value);
            }
            return self.fail(span, "B154", "address构造只接受空值或同型地址；整数解释须显式unsafe_address");
        }
        if values.iter().any(|value| value.ty.is_address()) {
            return self.fail(span, "B154", "address不能隐式转换为数值；位模式须显式address_bits");
        }
        if ty.is_handle() {
            if values.is_empty() {
                return Ok(self.constant_value(ty, vec![0, 0],None));
            }
            if values.len() == 1 && values[0].ty == ty {
                let mut value = values.pop().unwrap();
                value.literal = false;
                return Ok(value);
            }
            return self.fail(span, "B154", "handle构造只接受空值或同类型身份；外部身份解释须显式unsafe_handle");
        }
        if values.iter().any(|value| value.ty.is_handle()) {
            return self.fail(span, "B154", "不透明handle不能转换为数值或公开其载荷");
        }
        if values.iter().any(|value| value.ty.has_variants()) {
            return self.fail(span, "B269", "结果值须先通过 match 或 try 取得对应载荷");
        }
        if values.iter().any(|value| value.ty.enumeration) {
            if values.len() != 1 || ty != Ty::scalar(values[0].ty.components[0].0) {
                return self.fail(
                    span,
                    "B268",
                    "枚举值只直接取回其声明的整数表示；其他数值转换在该整数上显式进行",
                );
            }
            return Ok(self.cast(values.pop().unwrap(), ty.components[0].0));
        }
        if ty.matrix_shape().is_some() {
            return self.matrix_construct(ty, values, span);
        }
        if values.iter().any(|v| v.ty.array.is_some()) {
            return self.fail(span, "B238", "数组不能转换成数值标量／向量；先取得数组元素");
        }
        if values.iter().any(|v| v.ty.matrix_shape().is_some()) {
            return self.fail(span, "B237", "矩阵不能转换成数值标量／向量；先取得行字段");
        }
        if values.iter().any(|v| v.ty.contains_views()) {
            return self.fail(span, "B185", "view 不能转换成数值或重解释设备地址");
        }
        if values.len() == 1 && values[0].ty == ty {
            let mut value = values.pop().unwrap();
            value.literal = false;
            return Ok(value);
        }
        let mut flat = Vec::new();
        let mut constant = Some(Vec::new());
        let mut proof = bounds::Proof::default();
        let uniform = values.iter().all(|v| v.uniform);
        for v in values {
            let v = self.cast(v, ty.components[0].0);
            if ty.components.len() == 1 {
                proof = v.proof.clone();
            } else {
                proof.append(&v);
            }
            constant = constant.zip(v.constant).map(|(mut all, part)| {
                all.extend(part);
                all
            });
            flat.extend(v.regs);
        }
        if flat.len() == 1 && ty.components.len() > 1 {
            proof.splat(ty.components.len());
            flat = vec![flat[0]; ty.components.len()];
            constant = constant.map(|v| vec![v[0]; ty.components.len()]);
        }
        if flat.len() != ty.components.len() {
            return self.fail(span, "B157", "类型构造的分量数不匹配");
        }
        return Ok(Value {
            proof,
            literal: false,
            ty,
            regs: flat,
            uniform,
            constant,
        });
    }
    fn dot_value(&mut self, values: Vec<Value>, span: Span) -> Result<Value> {
        if values.len() != 2 {
            return self.fail(span, "B153", "dot 需要两个向量");
        }
        self.require_same(&values[0].ty, &values[1].ty, span)?;
        if values[0].ty.components[0].0 != Scalar::F32 || !values[0].ty.is_numeric() {
            return self.fail(span, "B154", "dot 需要 float 向量");
        }
        let p = self.binary("*", values[0].clone(), values[1].clone(), span,None)?;
        let mut r = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::F32),
            regs: vec![p.regs[0]],
            uniform: p.uniform,
            constant: None,
        };
        for x in p.regs.iter().skip(1) {
            r = self.binary(
                "+",
                r,
                Value {
                    proof: bounds::Proof::default(),
                    literal: false,
                    ty: Ty::scalar(Scalar::F32),
                    regs: vec![*x],
                    uniform: p.uniform,
                    constant: None,
                },
                span,
                None,
            )?;
        }
        return Ok(r);
    }
    fn length_value(
        &mut self,
        mut values: Vec<Value>,
        normalize: bool,
        span: Span,
    ) -> Result<Value> {
        if values.len() != 1 {
            return self.fail(span, "B153", "length／normalize 需要一个向量");
        }
        let v = values.remove(0);
        if v.ty.components[0].0 != Scalar::F32 || !v.ty.fields.is_empty() || v.ty.array.is_some() {
            return self.fail(span, "B154", "需要 float 向量");
        }
        let p = self.binary("*", v.clone(), v.clone(), span,None)?;
        let mut r = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::F32),
            regs: vec![p.regs[0]],
            uniform: p.uniform,
            constant: None,
        };
        for x in p.regs.iter().skip(1) {
            r = self.binary(
                "+",
                r,
                Value {
                    proof: bounds::Proof::default(),
                    literal: false,
                    ty: Ty::scalar(Scalar::F32),
                    regs: vec![*x],
                    uniform: p.uniform,
                    constant: None,
                },
                span,
                None,
            )?;
        }
        let length = self.math("sqrt", vec![r], span)?;
        if !normalize {
            Ok(length)
        } else {
            self.binary("/", v, length, span,None)
        }
    }
    fn cross_value(&mut self, values: Vec<Value>, span: Span) -> Result<Value> {
        if values.len() != 2 {
            return self.fail(span, "B153", "cross 需要两个 float3");
        }
        for v in &values {
            self.require_same(&Ty::vector(Scalar::F32, 3), &v.ty, span)?;
        }
        let a = &values[0];
        let b = &values[1];
        let mut regs = Vec::new();
        for (i, j) in [(1, 2), (2, 0), (0, 1)] {
            let value = |r, u| Value {
                proof: bounds::Proof::default(),
                literal: false,
                ty: Ty::scalar(Scalar::F32),
                regs: vec![r],
                uniform: u,
                constant: None,
            };
            let x = self.binary(
                "*",
                value(a.regs[i], a.uniform),
                value(b.regs[j], b.uniform),
                span,
                None,
            )?;
            let y = self.binary(
                "*",
                value(a.regs[j], a.uniform),
                value(b.regs[i], b.uniform),
                span,
                None,
            )?;
            regs.push(self.binary("-", x, y, span,None)?.regs[0]);
        }
        return Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::vector(Scalar::F32, 3),
            regs,
            uniform: a.uniform && b.uniform,
            constant: None,
        });
    }
    fn member_place(&mut self, base: &Expr, field: &String, e: &Expr) -> Result<Option<Place>> {
        let Some(p) = self.place(base)? else {
            return Ok(None);
        };
        if p.ty().view.is_some() {
            return self.fail(e.span, "B185", "view 元数据不可修改或按分量访问");
        }
        let (sub, components) = member_type(p.ty(), field, &self.definition.source, e.span)?;
        Ok(Some(p.project(sub, &components)))
    }
    fn index_place(&mut self, base: &Expr, index: &Expr) -> Result<Option<Place>> {
        if let Some(data) = self.application_reference(base)? {
            return self.application_index(data, index);
        }
        if let ExprKind::Name(name) = &base.kind
            && let Some(Binding::Tile(view)) = self.env.get(name).cloned()
        {
            return self.tile_place(view, index);
        }
        let Some(view) = self.dataset_reference(base)? else {
            return self.view_place(base, index);
        };
        let binding = view.binding;
        let Some(i) = self.expr(index)? else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(Scalar::U32), &i.ty, index.span)?;
        let current = i.regs[0] == self.global_index;
        let same_shape = self.program.same_length(binding, self.domain);
        if !self.index_in(
            &i,
            IndexDomain::Data(binding),
            self.program.buffers[binding].count.unwrap_or(0),
            None,
        ) && !(current && (binding == self.domain || same_shape))
        {
            return self.fail(
                index.span,
                "B137",
                "索引有效性无法静态证明；使用有界游标、同域 index()、有效长度条件或形状约束",
            );
        }
        if matches!(view.access, Access::Write | Access::InOut) {
            if self
                .appends
                .iter()
                .any(|(output, count)| *output == binding || count.binding == binding)
            {
                return self.fail(
                    index.span,
                    "B221",
                    "append 的共享输出／计数不能在同一阶段借为普通独占元素",
                );
            }
            self.element_domains.insert(binding);
        }
        Ok(Some(self.dataset_element(
            &view, i.regs[0], view.access.writes() && (view.whole || current),
        )))
    }
    fn name_place(&mut self, name: &String, e: &Expr) -> Result<Place> {
        match self.env.get(name).cloned() {
            Some(Binding::AtomicBorrow(place)) => Ok(place),
            Some(Binding::Owned(owner)) => Ok(owner.place),
            Some(Binding::OwnedValue(owner))=>{
                if let Some(place)=owner.place(){return Ok(place);}
                let owner=self.owned_value_binding(owner,true);let place=owner.place().unwrap();self.env.insert(name.clone(),Binding::OwnedValue(owner));Ok(place)
            },
            Some(Binding::Value(value)) if value.ty.contains_arrays() => {
                let mut owner = self.materialize_array(value,None);
                owner.place = owner.place.read_only();
                let place = owner.place.clone();
                self.env.insert(name.clone(), Binding::Owned(owner));
                Ok(place)
            }
            Some(Binding::ViewElement { element, .. }) => Ok(element.place),
            _ => self.fail(e.span, "B134", format!("{name} 不是可赋值的元素或局部变量")),
        }
    }
    #[inline(never)]
    fn declare_statement(
        &mut self,
        annotation: &TypeName,
        name: &String,
        span: Span,
    ) -> Result<()> {
        if self.env.contains_key(name) {
            return self.fail(span, "B125", format!("重复局部变量或覆盖参数 {name}"));
        }
        if annotation.dataset {
            return self.application_declare(annotation, name, span);
        }
        let ty = self.world.ty(
            annotation,
            &self.definition.source,
            span,
            &self.definition.module,
            &self.definition.imports,
        )?;
        if ty.owns_resources() {
            return self.owned_value_declare(name, ty, span);
        }
        let (slots, storage) = self.local_slots(&ty);
        self.env.insert(
            name.clone(),
            Binding::Owned(OwnedBinding {
                place: Place::Local(ty, slots, storage),
                proof: bounds::Proof::default(),
                uniform: false,
                initialization: None,
            }),
        );

        Ok(())
    }
    #[inline(never)]
    fn assign_statement(&mut self, lhs: &Expr, op: &String, rhs: &Expr, span: Span) -> Result<()> {
        if self.read_expression_type(lhs).is_some_and(|ty|ty.owns_resources()) {
            let mut element=lhs;let mut projection=Vec::new();
            loop {match &element.kind {
                ExprKind::Index(base,index) if self.read_expression_type(base).is_some_and(|ty|ty.dataset.as_ref().is_some_and(|element|element.owns_resources()))=>{
                    let Some(data)=self.application_reference(base)?else{return Ok(())};
                    projection.reverse();return self.assign_application_element(data,index,&projection,op,rhs,span);
                },
                ExprKind::Member(base,_)|ExprKind::Index(base,_)=>{projection.push(element);element=base;},
                _=>break,
            }}
        }
        if self.owned_element_type(lhs).is_some_and(|ty|ty.owns_resources()) {
            let Some(place)=self.place(lhs)?else{return Ok(())};
            return self.assign_owned_storage(place,op,rhs,span);
        }
        if let ExprKind::Name(name) = &lhs.kind
            && let Some(Binding::ApplicationData(data)) = self.env.get(name).cloned()
        {
            return self.application_assign(name, data, op, rhs, span);
        }
        let Some(place) = self.place(lhs)? else {
            return Ok(());
        };
        let borrowed = matches!(place.storage(), Place::ApplicationElement { .. })
            .then(|| self.hold_binding(Binding::AtomicBorrow(place.clone())));
        let Some(mut value) = self.expr_expected(rhs, Some(place.ty()))? else {
            return Ok(());
        };
        if let Some(name) = borrowed {
            self.env.remove(&name);
        }
        if op != "=" {
            let current = self.read_place(&place, lhs.span, None)?;
            value = self.binary(&op[..op.len() - 1], current, value, span,None)?;
        }
        self.view_uses.after(span);
        self.write_place(&place, value, span, None)?;

        Ok(())
    }
    #[inline(never)]
    fn expression_statement(&mut self, e: &Expr) -> Result<()> {
        if let ExprKind::Call(name, args) = &e.kind
            && name == "atomic_store"
        {
            self.atomic_store(args, e.span)?;
        } else if let ExprKind::Call(name, args) = &e.kind
            && name == "tile_barrier"
        {
            self.tile_barrier(args, e.span)?;
        } else if let ExprKind::Cooperate(name, explicit, args) = &e.kind {
            self.cooperate(name, explicit.as_deref(), args, e.span)?;
        } else {
            if let Some(value) = self.evaluate_returned(e, None)? {
                self.drop_returned(value,&view::Borrow::default(),e.span)?;
            }
        }

        Ok(())
    }
    fn advance_iterator(&mut self, index: Reg, slot: usize) -> Result<()> {
        let mut source = self.loop_cursor().unwrap_or(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![index],
            uniform: false,
            constant: None,
        });
        source.regs = vec![index];
        let one = self.constant(Scalar::U32, 1);
        let value = self.binary("+", source, one, self.definition.function.span,None)?;
        self.emit(InstKind::StoreLocal(slot, value.regs[0]));
        self.capture_loop_cursor(value);
        Ok(())
    }
    #[inline(never)]
    fn seek_statement(&mut self, name: &String, position: &Expr, span: Span) -> Result<()> {
        let Some(Some((iterator, slot))) = self.iterators.last().cloned() else {
            return self.fail(span, "B164", "seek 需要当前有界 for 游标");
        };
        if iterator != *name {
            return self.fail(span, "B165", "seek 只能推进当前最内层游标");
        }
        let Some(value) = self.temporary_scope(position.span, |builder| Ok((builder.expr(position)?, view::Borrow::default())))? else {
            return Ok(());
        };
        self.require_same(&Ty::scalar(Scalar::U32), &value.ty, position.span)?;
        self.release_application_loop()?;
        self.emit(InstKind::StoreLocal(slot, value.regs[0]));
        self.capture_loop_cursor(value);
        self.check_application_loop(span)?;
        self.emit(InstKind::Continue);
        self.flow = Flow::CONTINUE;

        Ok(())
    }
    fn ray_query_call(&mut self, name: &str, args: &[Expr], span: Span) -> Result<Option<Value>> {
        if args.len() != 5 {
            return self.fail(
                span,
                "B183",
                "光线查询需要 acceleration、origin、minimum、direction、maximum",
            );
        }
        let ExprKind::Name(scene) = &args[0].kind else {
            return self.fail(span, "B183", "光线查询需要命名 acceleration 资源");
        };
        let Some(Binding::Acceleration(scene)) = self.env.get(scene).cloned() else {
            return self.fail(span, "B183", "光线查询需要 read acceleration 资源");
        };
        let Some(origin) = self.expr(&args[1])? else {
            return Ok(None);
        };
        let Some(minimum) = self.expr(&args[2])? else {
            return Ok(None);
        };
        let Some(direction) = self.expr(&args[3])? else {
            return Ok(None);
        };
        let Some(maximum) = self.expr(&args[4])? else {
            return Ok(None);
        };
        self.require_same(&Ty::vector(Scalar::F32, 3), &origin.ty, args[1].span)?;
        self.require_same(&Ty::scalar(Scalar::F32), &minimum.ty, args[2].span)?;
        self.require_same(&origin.ty, &direction.ty, args[3].span)?;
        self.require_same(&minimum.ty, &maximum.ty, args[4].span)?;
        let ty = if name == "trace_any" {
            Ty::scalar(Scalar::Bool)
        } else {
            Ty::builtin("RayHit").unwrap()
        };
        let regs: Vec<_> = ty.components.iter().map(|(s, _)| self.reg(*s)).collect();
        let result = if name == "trace_any" {
            RayResult::Any(regs[0])
        } else {
            RayResult::Closest(regs.as_slice().try_into().unwrap())
        };
        self.emit(InstKind::RayQuery {
            scene,
            origin: origin.regs.as_slice().try_into().unwrap(),
            direction: direction.regs.as_slice().try_into().unwrap(),
            minimum: minimum.regs[0],
            maximum: maximum.regs[0],
            result,
        });
        return Ok(Some(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty,
            regs,
            uniform: false,
            constant: None,
        }));
    }
    fn unsafe_read_call(&mut self, args: &[Expr], span: Span) -> Result<Option<Value>> {
        if args.len() != 2 {
            return self.fail(
                span,
                "B181",
                "unsafe_read 需要 read 数据集与 u32 索引；调用者负责外部索引有效性",
            );
        }
        if let ExprKind::Name(data) = &args[0].kind {
            if let Some(data) = self.application_value(data, span)? {
                self.application_cpu(&data, span)?;
                let Some(index) = self.expr(&args[1])? else {
                    return Ok(None);
                };
                self.require_same(&Ty::scalar(Scalar::U32), &index.ty, args[1].span)?;
                return self
                    .read_place(
                        &Self::application_element(
                            &data,
                            &index,
                            (index.minimum(), index.maximum()),
                        ),
                        span, None
                    )
                    .map(Some);
            }
        }
        if let Some(view) = self.dataset_reference(&args[0])? {
            if !matches!(view.access, Access::Read | Access::InOut) {
                return self.fail(span, "B181", "unsafe_read 需要 read 或本阶段共享只读的 inout 数据集");
            }
            let Some(index) = self.expr(&args[1])? else { return Ok(None); };
            self.require_same(&Ty::scalar(Scalar::U32), &index.ty, args[1].span)?;
            return self.read_place(&self.dataset_element(&view, index.regs[0], false), span, None).map(Some);
        }
        let Some(view) = self.expr(&args[0])? else {
            return Ok(None);
        };
        if view.ty.view.is_none() {
            return self.fail(span, "B181", "unsafe_read 需要 read dataset 或不可变 view");
        }
        let held = self.hold_binding(Binding::Value(view.clone()));
        let evaluated = self.expr(&args[1]);
        self.env.remove(&held);
        let Some(index) = evaluated? else { return Ok(None); };
        self.require_same(&Ty::scalar(Scalar::U32), &index.ty, args[1].span)?;
        return self
            .read_place(&Self::view_element(&view, index.regs[0]), span, None)
            .map(Some);
    }
    fn ordinal_call(&mut self, args: &[Expr], span: Span) -> Result<Option<Value>> {
        if args.len() != 1 {
            return self.fail(span, "B168", "ordinal 需要一个有界游标");
        }
        let ExprKind::Name(name) = &args[0].kind else {
            return self.fail(span, "B168", "ordinal 需要命名游标");
        };
        let (domain, index, proof) = match self.env.get(name).cloned() {
            Some(Binding::Owned(OwnedBinding {
                place:
                    Place::ApplicationElement {
                        identity,
                        index,
                        index_range,
                        ..
                    },
                ..
            })) => (
                IndexDomain::ApplicationData(identity),
                index,
                bounds::Proof::unsigned(index_range.0, index_range.1),
            ),
            Some(Binding::Owned(OwnedBinding {
                place: Place::Element { binding, index, .. },
                ..
            })) => (
                IndexDomain::Data(binding),
                index,
                self.data_index_proof(binding),
            ),
            Some(Binding::Owned(OwnedBinding {
                place: Place::TileElement { index, .. }, ..
            })) => (IndexDomain::Tile, index, self.tile_index_proof()),
            Some(Binding::ViewElement {
                view,
                element: OwnedBinding { place: Place::ViewElement { index, .. }, .. },
            }) => {
                let proof = bounds::Proof::unsigned(
                    0,
                    Self::view_length(&view).maximum().saturating_sub(1),
                );
                (IndexDomain::View(view.view_identity()), index, proof)
            }
            _ => return self.fail(span, "B168", "ordinal 只能用于元素游标"),
        };
        return Ok(Some(Value {
            proof: self.path_proof(&Ty::scalar(Scalar::U32), proof.with_domains(Domains::from([domain])).with_identity(ValueIdentity::Register(index))),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![index],
            uniform: false,
            constant: None,
        }));
    }
    fn member_expr(&mut self, base: &Expr, field: &String, e: &Expr) -> Result<Option<Value>> {
        if let Some(data) = self.application_reference(base)? {
            return if field == "length" {
                Ok(Some(data.length.clone()))
            } else {
                self.fail(e.span, "B141", "dataset只公开只读length")
            };
        }
        if let ExprKind::Name(name) = &base.kind {
            if let Some(Binding::Texture(reference)) = self.env.get(name).cloned() {
                if field == "length"
                    && let Some(count) = reference.shape(self.program)
                {
                    return Ok(Some(self.constant(Scalar::U32, Bits::from(count))));
                }
                return self.fail(e.span, "B239", "纹理资源数组只公开只读 length");
            }
            if let Some(Binding::Tile(view)) = self.env.get(name).cloned() {
                return match field.as_str() {
                    "length" => Ok(Some(self.tile_length(&view))),
                    "lane" => Ok(Some(self.tile_lane())),
                    _ => self.fail(e.span, "B141", "tile 只公开只读 length 与 lane"),
                };
            }
        }
        if field == "length" && let Some(view) = self.dataset_reference(base)? {
            return Ok(Some(self.data_length(&view)));
        }
        if self.is_place(base) {
            // 保留字段访问，避免为了读取一个字段先加载整个结构体。
            let Some(p) = self.place(base)? else {
                return Ok(None);
            };
            if p.ty().array.is_some() && field == "length" {
                return Ok(Some(self.constant(Scalar::U32, Bits::from(p.ty().values[0]))));
            }
            if p.ty().view.is_some() {
                if field == "length" {
                    let view = self.read_place(&p, e.span, None)?;
                    return Ok(Some(Self::view_length(&view)));
                }
                return self.fail(e.span, "B185", "view 只公开只读 length，不公开设备地址分量");
            }
            let (sub, components) = member_type(p.ty(), field, &self.definition.source, e.span)?;
            return self
                .read_place(&p.project(sub, &components), e.span, None)
                .map(Some);
        }
        let Some(v) = self.expr(base)? else {
            return Ok(None);
        };
        self.member_value(v, field, e.span).map(Some)
    }
    pub(super) fn member_value(&mut self, v: Value, field: &String, span: Span) -> Result<Value> {
        if v.ty.array.is_some() && field == "length" {
            return Ok(self.constant(Scalar::U32, Bits::from(v.ty.values[0])));
        }
        if v.ty.view.is_some() && field == "length" {
            return Ok(Self::view_length(&v));
        }
        let (ty, indices) = member_type(&v.ty, field, &self.definition.source, span)?;
        Ok(Value {
            proof: v.projected_proof(&indices),
            literal: false,
            ty,
            regs: indices.iter().map(|i| v.regs[*i]).collect(),
            uniform: v.uniform,
            constant: v.constant.map(|b| indices.iter().map(|i| b[*i]).collect()),
        })
    }
    #[inline(never)]
    fn index_expr(&mut self, base: &Expr, index: &Expr, e: &Expr) -> Result<Option<Value>> {
        let resource = self.dataset_reference(base)?.is_some() || matches!(&base.kind, ExprKind::Name(name) if matches!(self.env.get(name), Some(Binding::Tile(_) | Binding::ApplicationData(_))));
        if !resource && !self.is_place(base) {
            let returned = self.owned_array_index(e, base, index)?;
            return self.ordinary_return(returned, e.span);
        }
        let Some(p) = self.place(e)? else {
            return Ok(None);
        };
        self.read_place(&p, e.span, None).map(Some)
    }
    #[inline(never)]
    fn name_expr(&mut self, name: &String, e: &Expr) -> Result<Value> {
        match self.env.get(name).cloned() {
            Some(Binding::Value(v)) => Ok(v),
            Some(Binding::Owned(owner)) => self.read_place(&owner.place, e.span, None),
            Some(
                Binding::AtomicBorrow(_)
                | Binding::ViewElement { .. },
            ) => {
                let p = self.name_place(name, e)?;
                self.read_place(&p, e.span, None)
            }
            None => {
                let key = self
                    .world
                    .resolve(
                        name,
                        &self.definition.module,
                        &self.definition.imports,
                        true,
                    )
                    .filter(|key| self.world.functions[key].function.kind == FunctionKind::Constant)
                    .ok_or_else(|| Diagnostic {
                        source: self.definition.source.clone(),
                        span: e.span,
                        code: "B145",
                        message: format!("未知值 {name}"),
                    })?;
                self.module_constant(key)
            }
            _ => self.fail(e.span, "B145", format!("未知值 {name}")),
        }
    }
    fn is_place(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Name(n) => {
                matches!(
                    self.env.get(n),
                    Some(
                        Binding::AtomicBorrow(_)
                            | Binding::ViewElement { .. }
                            | Binding::Owned(_)
                            | Binding::OwnedValue(_)
                    )
                )
            }
            ExprKind::Member(b, _) => self.is_place(b),
            ExprKind::Index(..) => true,
            ExprKind::Specialize(name, _, _) if name == "unsafe_position" => true,
            _ => false,
        }
    }
}
use effects::{assigned_names, expression_assignments};

pub(crate) fn stage_intrinsic(name: &str) -> bool {
    matches!(name, "clear_prefix" | "stable_sort_by_key" | "compact")
        || CollectiveOperator::from_reduction_source(name).is_some()
        || CollectiveOperator::from_segmented_reduction_source(name).is_some()
        || CollectiveOperator::from_scan_source(name).is_some()
        || CollectiveOperator::from_segmented_scan_source(name).is_some()
        || custom_reduce::Kind::from_source(name).is_some()
}

pub(crate) fn kernel_interface(world: &World, d: &Definition) -> Result<bool> {
    let result = &d.function.result;
    if result.name != "void"
        || result.dataset
        || result.tile
        || result.view
        || !result.arguments.is_empty()
    {
        return Ok(false);
    }
    for param in &d.function.params {
        if param.ty.dataset {
            if param.access == Access::Value {
                return Ok(false);
            }
        } else if !param.ty.is_acceleration() && !param.ty.is_texture_resource() {
            if param.access != Access::Value
                || world
                    .ty(&param.ty, &d.source, param.span, &d.module, &d.imports)?
                    .owns_resources()
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

crate::module::codec::record!(Value {
    proof,
    literal,
    ty,
    regs,
    uniform,
    constant
});
crate::module::codec::variants!(ValueIdentity{0=>Register(reg),1=>DataLength(binding),2=>TileLength,3=>ParameterContent(parameter,component),4=>ResourceParameter(parameter,resource),5=>ParameterStorage(parameter),6=>ElementComponent(component),7=>ElementParent(depth,component),8=>DatasetContent(parameter,resource,component),9=>MemoryValue(call,component),10=>ElementIndex(depth)});
crate::module::codec::variants!(IndexDomain{0=>Data(binding),1=>Fixed(count),2=>Tile,3=>DataLength(binding),4=>TileLength,5=>View(ids),6=>ViewLength(ids),7=>ApplicationData(id),8=>ApplicationLength(id),9=>Upper(limit,distance),10=>Room(base),11=>Sum(left,right),12=>Shifted(base,limit,distance),13=>Offset(base,amount),14=>Computed(expression),15=>Lower(base,distance),16=>CommonRange(source)});


// 已编译符号的空合同表示没有B函数合同；非空合同由原所有者交接规范身份。
pub(crate) fn canonical_function_contract(bytes: &[u8]) -> crate::module::codec::Result<Vec<u8>> {
    use crate::module::codec::{Decode, Encode};
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    Ok(native_function::Contract::from_bytes(bytes)?.bytes())
}
