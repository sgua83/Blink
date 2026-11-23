use crate::{
    Definition, World,
    ast::*,
    syntax::{Expr, ExprKind, Stmt, StmtKind, MatchArm, ParallelDomain, Block, Expressions, MatchArms},
    ir::{self, *},
};
mod append;
mod address;
mod array;
mod atomic;
mod collective;
mod bounds;
pub(crate) use bounds::Proof;
mod compact;
mod callback;
mod constant;
mod source_call;
mod call_aliases;
mod call_arguments;
mod call_execution;
pub(crate) use constant::evaluate as evaluate_constant;
pub(crate) use constant::variant_discriminant;
mod application;
mod contracts;
mod cooperative;
mod custom_reduce;
mod effects;
pub(crate) mod external;
mod native_function;
mod residency;
pub(crate) use native_function::{check_domain, compile_module as compile_native_module, compiled_task_signature};
mod flow;
pub(crate) mod generics;
mod matrix;
mod operators;
mod numeric_literal;
mod owned_records;
mod user_owned;
mod owned_borrows;
mod owned_presence;
mod owned_values;
mod dataset_drop;
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
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) fn member_type(ty: &Ty, field: &str, source: &Source, span: Span) -> Result<(Ty, Vec<usize>)> {
    let mut indices = Vec::new();
    let selected = select_member(ty, field, source, span, Some(&mut indices))?;
    Ok((selected.into_owned(), indices))
}

pub(crate) fn member_type_only<'a>(
    ty: Cow<'a, Ty>, field: &str, source: &Source, span: Span,
) -> Result<Cow<'a, Ty>> {
    match ty {
        Cow::Borrowed(ty) => select_member(ty, field, source, span, None),
        Cow::Owned(ty) => select_member(&ty, field, source, span, None)
            .map(|selected| Cow::Owned(selected.into_owned())),
    }
}

fn select_member<'a>(
    ty: &'a Ty, field: &str, source: &Source, span: Span, indices: Option<&mut Vec<usize>>,
) -> Result<Cow<'a, Ty>> {
    use crate::ir::SourceMember;
    match crate::ir::source_member(ty, field) {
        SourceMember::Length => {
            if let Some(indices) = indices {
                indices.push(2);
            }
            Ok(Cow::Owned(Ty::scalar(Scalar::U32)))
        }
        SourceMember::Declared { ty: selected, offset } => {
            if let Some(indices) = indices {
                indices.extend(selected.components.iter().map(|(_, component)| {
                    ty.components.iter().position(|(_, position)| *position == offset + component).unwrap()
                }));
            }
            Ok(Cow::Borrowed(selected))
        }
        SourceMember::Swizzle { lanes, count } => {
            if let Some(indices) = indices {
                indices.extend(lanes[..count as usize].iter().map(|&lane| lane as usize));
            }
            Ok(Cow::Owned(Ty::vector(ty.components[0].0, count as usize)))
        }
        SourceMember::Variant => source.fail(span, "B269",
            "结果载荷通过 match 或 try 使用，不公开原始判别字／载荷字段"),
        SourceMember::HiddenView => source.fail(span, "B185", "view 只公开只读 length，不公开设备地址分量"),
        SourceMember::UnknownComponent => source.fail(span, "B139", format!("未知字段 {field}")),
        SourceMember::ComponentBounds => source.fail(span, "B140", "向量分量越界"),
        SourceMember::Missing => source.fail(span, "B141", format!("{} 没有字段 {field}", ty.name)),
    }
}

#[derive(Clone)]
enum Root {
    Data(usize, Access),
    Acceleration(usize),
    Texture(usize),
    Uniform(usize),
    Literal(Ty, Vec<Bits>, Option<Arc<GeneratedOrigin>>),
}
impl Root {
    fn reference_literal(self, reference: impl FnOnce() -> Option<Arc<SourceSite>>) -> Self {
        match self {
            Self::Literal(ty, bits, retained) => Self::Literal(ty, bits, Some(Arc::new(GeneratedOrigin {
                reason: GenerationReason::StageConstantReference,
                trigger: Some(GenerationAnchor::StageConstantReference { reference: reference(), retained }),
            }))),
            root => root,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TaskCompletionSource {
    Input(Reg),
    Returned(Reg,usize),
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
    LoopValue(usize, usize),
    TaskResult(usize),
    TaskCompletion(Reg, usize),
    TaskWorkCompletion(u32,u32),
    // 普通调用的实际任务返回句柄所确定的来源分支。
    TaskControl(Reg),
    // 普通返回实际交接的任务来源，按原结果分量命名。
    TaskReturnSource(usize),
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
    CommonRange(ValueIdentity,Scalar,Scalar,Scalar,Scalar),
    KnownBits(Scalar,Bits,Bits),
    Floating(bounds::FloatingBound),
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
enum TileOrigin {
    Input { binding: usize, word: u32, ty: Ty },
    Published(Value),
    Parameter(Ty),
}
struct CheckedTile {
    source: TileOrigin,
    width: TileWidth,
    access: Access,
}
impl CheckedTile {
    fn ty(&self) -> &Ty {
        match &self.source {
            TileOrigin::Input { ty, .. } | TileOrigin::Parameter(ty) => ty,
            TileOrigin::Published(value) => &value.ty,
        }
    }
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
        index_expression: Option<bounds::IntegerExpression>,
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
        resource: view::Borrow,
        element_layout: address::Layout,
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
    prefix_guarantees: Vec<PrefixRelation>,
    prefix_requirements: Vec<PrefixRelation>,
    element_guarantees: Vec<ElementFact>,
    element_requirements: Vec<ElementFact>,
    source: Option<Arc<SourceSite>>,
}
struct RangeContext {
    output_scopes: usize,
    view_model: view_contracts::Function,
    residency: residency::Inputs,
}

struct ExpandedDomain {
    program: Program,
    write_regions: BTreeMap<usize, BTreeSet<(u32, u32)>>,
    scopes: BTreeMap<usize, RangeContext>,
    calls: Vec<DomainCall>,
    outputs: Vec<crate::module::domain::Output>,
    elements: BTreeMap<usize, bounds::Proof>,
}
pub(crate) async fn compile_program(
    world: &World,
    instance: &crate::generics::Instance,
    signature: Option<KernelSignature>,
) -> Result<Program> {
    let input = DomainInput::Declaration { instance, signature: None };
    let KernelSignature { parameters, uniform_types } = match signature {
        Some(signature) => signature,
        None => unsafe { world.call(kernel_signature(world, instance)).await }?
            .ok_or_else(|| invalid_kernel_interface(input))?,
    };
    let mut uniform_types = uniform_types.into_iter();
    let mut functions = native_function::Module::default();
    let (expanded, _) = unsafe {
        world.call(expand_domain(world, input, &parameters, Some(&mut uniform_types), &mut functions, None)).await
    }?;
    if !expanded.calls.is_empty() {
        return instance.definition.source.fail(
            instance.definition.function.span(),
            "B270",
            "跨对象域使用原生模块或应用入口编译；单个内核映像不包含其他模块的实现",
        );
    }
    unsafe { world.call(native_function::finish_program(world, functions, expanded.program, expanded.scopes)).await }
}
#[derive(Clone, Copy)]
enum DomainInput<'a> {
    Declaration {
        instance: &'a crate::generics::Instance,
        signature: Option<&'a KernelSignature>,
    },
    Parallel {
        owner: &'a crate::generics::Instance,
        name: &'a str,
        parameters: &'a Arc<[Param]>,
        length_requirements: &'a [(usize, usize)],
        prefix_requirements: &'a [PrefixRelation],
        element_requirements: &'a [ElementFact],
        statement: &'a Stmt,
        element: &'a str,
        domain: &'a ParallelDomain,
        body: &'a Block,
        site: &'a Option<Arc<SourceSite>>,
    },
}
impl<'a> DomainInput<'a> {
    fn owner(self)->&'a crate::generics::Instance {
        match self {Self::Declaration { instance, .. }|Self::Parallel{owner:instance,..}=>instance}
    }
    fn name(self)->&'a str {
        match self {Self::Declaration { instance, .. }=>&instance.name,Self::Parallel{name,..}=>name}
    }
    fn span(self)->Span {
        match self {Self::Declaration { instance, .. }=>instance.definition.function.span(),Self::Parallel{statement,..}=>statement.span()}
    }
    fn target(self)->Target {
        match self {Self::Declaration { instance, .. }=>instance.definition.function.target(),Self::Parallel{..}=>Target::Cpu}
    }
    async fn parameters(self, world: &World) -> Result<Cow<'a, [Param]>> {
        match self {
            Self::Declaration { signature: Some(signature), .. } => Ok(Cow::Borrowed(&signature.parameters)),
            Self::Declaration { instance, signature: None } => {
                let mut parameters = Vec::new();
                for parameter in instance.parameters(world) {
                    parameters.push(unsafe { world.call(parameter).await }?);
                }
                Ok(Cow::Owned(parameters))
            }
            Self::Parallel { parameters, .. } => Ok(Cow::Borrowed(parameters)),
        }
    }
    fn source_origin(self)->Option<SourceOrigin> {
        match self {Self::Declaration { instance, .. }=>instance.definition.source_origin(),Self::Parallel{..}=>None}
    }
    fn syntax_context(self)->(Option<Arc<SourceOrigin>>,Arc<SourceContext>) {
        match self {
            Self::Declaration { instance, .. }=>instance.definition.syntax_context(Some(&instance.name)),
            Self::Parallel{site:Some(site),..}=>(Some(site.origin.clone()),site.context.clone()),
            Self::Parallel{site:None,..}=>(None,Arc::new(SourceContext{instance:None,inlined_at:Box::default(),domain_path:Arc::from([])})),
        }
    }
    fn operation_source(self,span:Span,syntax:SourceSyntax,path:&Arc<[DomainSite]>)->Option<Arc<SourceSite>> {
        let(origin,context)=self.syntax_context();
        origin.map(|origin|Arc::new(SourceSite{origin,context:Arc::new(SourceContext{domain_path:path.clone(),..(*context).clone()}),span,syntax}))
    }
}
fn invalid_kernel_interface(input: DomainInput<'_>) -> Diagnostic {
    Diagnostic {
        source: input.owner().definition.source.clone(),
        span: input.span(),
        code: "B266",
        message: "算法对象接口以借用数据与普通统一值交付结果；普通函数的独立调用／资源 ABI 产物尚未实现".into(),
    }
}

async fn expand_domain(
    world: &World,
    input: DomainInput<'_>,
    parameters: &[Param],
    mut uniform_types: Option<&mut dyn Iterator<Item = Ty>>,
    functions: &mut native_function::Module,
    mut entry: Option<&mut native_function::DomainEntry>,
) -> Result<(ExpandedDomain, Vec<FunctionContract>)> {
    let name = input.name();
    let d = &input.owner().definition;
    let mut buffers = Vec::new();
    let mut accelerations = Vec::new();
    let mut textures = Vec::new();
    let mut uniforms = Vec::new();
    let mut env = BTreeMap::new();
    let gpu = input.target() == Target::Gpu;
    let qualified = uniform_types.is_some();
    for p in parameters {
        let texture = p.ty.texture_type();
        let parameter = function_inputs::domain_binding(p, texture.is_some(), gpu, qualified);
        if parameter.form == function_inputs::Parameter::Texture {
            parameter.check_access(&d.source, p.span)?;
            env.insert(p.name.to_owned(), Root::Texture(textures.len()));
            textures.push(unsafe {
                world.call(texture::binding(world, p, texture.unwrap(), (&d.source, &d.module, &d.imports))).await
            }?);
            if let Some(entry) = entry.as_deref_mut() { entry.texture(); }
            continue;
        }
        if parameter.form == function_inputs::Parameter::Acceleration {
            parameter.check_access(&d.source, p.span)?;
            env.insert(p.name.to_owned(), Root::Acceleration(accelerations.len()));
            accelerations.push(p.name.to_owned());
            continue;
        }
        let dataset = parameter.form == function_inputs::Parameter::Dataset;
        let ty = if !dataset && let Some(types) = uniform_types.as_deref_mut() {
            types.next().unwrap()
        } else {
            let ty = unsafe { world.call(world.ty(&p.ty, &d.source, p.span, &d.module, &d.imports)).await }?;
            if ty.owns_resources() {
                return d.source.fail(
                    p.span,
                    "B224",
                    "拥有结果属于CPU资源组织，不能成为program的普通存储或uniform",
                );
            }
            ty
        };
        if parameter.view_check != 0 && ty.contains_views() {
            return d.source.fail(
                p.span,
                "B185",
                "含view的整域根需要read／inout dataset；write／atomic或uniform不能构造引用图",
            );
        }
        parameter.check_access(&d.source, p.span)?;
        if dataset {
            let binding = buffers.len();
            let stride = ir::align_up(ty.size, ty.align);
            buffers.push(Buffer {
                name: p.name.to_owned(),
                access: p.access,
                ty,
                stride,
                count: unsafe { world.call(world.shape(&p.ty.count, &d.source, p.span, &d.module, &d.imports)).await }?,
            });
            env.insert(p.name.to_owned(), Root::Data(binding, p.access));
            if let Some(entry) = entry.as_deref_mut() { entry.dataset(); }
        } else {
            if let Some(entry) = entry.as_deref_mut() { entry.uniform(&ty); }
            uniforms.push(Uniform {
                name: p.name.to_owned(),
                ty,
                offset: 0,
            });
        }
    }
    let offset = function_inputs::layout_uniforms(buffers.len() as u32, &mut uniforms, &mut env);
    if offset > world.worker.compiler().max_parameter_bytes() {
        return d.source.fail(
            input.span(),
            "B103",
            format!("程序参数占 {offset} 字节，超过本次编译配置的 {} 字节参数上限；此处未查询实际设备能力", world.worker.compiler().max_parameter_bytes()),
        );
    }
    let mut program = Program {
        name: name.to_owned(),
        source_origin: input.source_origin(),
        generated_origin: None,
        target: input.target(),
        buffers,
        accelerations,
        textures,
        uniforms,
        uniform_bytes: offset,
        native_frame: None,
        resources: None,
        source_stages: 0,
        stages: Vec::new(),
        functions: Arc::from([]),
        prefix_requirements: Vec::new(),
        prefix_guarantees: Vec::new(), element_requirements: Vec::new(), element_guarantees: Vec::new(), length_requirements: Vec::new(), imports: BTreeMap::new(),
    };
    let (bound_contracts, guarantees) = match input {
        DomainInput::Parallel { length_requirements, prefix_requirements, element_requirements, .. } => {
            program.length_requirements.extend_from_slice(length_requirements);
            program.prefix_requirements.extend_from_slice(prefix_requirements);
            program.element_requirements.extend_from_slice(element_requirements);
            (Vec::new(), Vec::new())
        }
        DomainInput::Declaration { instance, .. } => {
            let contracts: Vec<_> = instance.contracts().collect();
            program.length_requirements = contracts::root_lengths(&env, d, &contracts)?;
            program.prefix_requirements = prefix::requirements(&program, &env, d, &contracts)?;
            let guarantees = prefix::bindings(&program, &env, d, &contracts, ContractKind::Ensures)
                .collect::<Result<Vec<_>>>()?;
            program.element_requirements = unsafe { world.call(prefix::element_requirements(world, &program.buffers, &env, instance, &contracts)).await }?;
            (contracts, guarantees)
        }
    };
    let mut facts = prefix::Facts::new(&program.prefix_requirements);
    facts.assume_elements(&program);
    let mut calls = Vec::new();
    let path=input.syntax_context().1.domain_path.clone();
    let mut scopes = BTreeMap::new();
    match input {
        DomainInput::Declaration { instance, .. }=>unsafe { world.call(expand_program(
            world,instance,&mut env,&mut program,&mut Vec::new(),&mut false,&mut facts,&mut calls,path,functions,&mut scopes)).await }?,
        DomainInput::Parallel{statement,..}=>{
            unsafe { world.call(expand_stages(world,input,std::iter::once(statement.clone()),&mut env,&mut program,
                &mut Vec::new(),&mut false,&mut facts,&mut calls,&path,functions,&mut scopes)).await }?;
        }
    }
    program.source_stages = program.stages.len();
    program.prefix_guarantees = prefix::canonical_relations(prefix::proved_guarantees(
        &program, d, &facts, guarantees.into_iter().map(Ok),
    )?);
    let write_regions = program.write_regions();
    let last_writes;
    let output_writes = if let Some(call) = calls.last() {
        last_writes = Program::stage_write_regions(&program.stages[call.stage..]);
        &last_writes
    } else {
        &write_regions
    };
    program.element_guarantees = facts.element_facts(&program, Some(output_writes));
    Ok((ExpandedDomain { program, write_regions, scopes, calls, outputs: facts.outputs, elements: facts.elements }, bound_contracts))
}
async fn expand_program(
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
    scopes: &mut BTreeMap<usize, RangeContext>,
) -> Result<()> {
    let d = &instance.definition;
    let key = format!("{}::{}", d.module, d.function.name());
    if stack.contains(&key) {
        return d
            .source
            .fail(d.function.span(), "B105", "计算程序递归调用没有静态阶段边界");
    }
    stack.push(key);
    unsafe { world.call(expand_stages(
        world,
        DomainInput::Declaration { instance, signature: None },
        d.function.body().as_ref().ok_or_else(|| Diagnostic {
            source: d.source.clone(), span: d.function.span(), code: "B270",
            message: "已编译整域入口没有可展开的实现体；CPU应用或CPU包装函数沿原生入口调用，域内部的已编译域组合尚未实现".into(),
        })?.iter(),
        env,
        program,
        stack,
        boundary,
        facts,
        calls,
        &domain_path,
        functions,
        scopes,
    )).await }?;
    stack.pop();
    Ok(())
}
fn operation_source(instance: &crate::generics::Instance, span: Span, syntax: SourceSyntax, domain_path: &Arc<[DomainSite]>) -> Option<Arc<SourceSite>> {
    let (origin, context) = instance.definition.syntax_context(Some(&instance.name));
    let context = Arc::new(SourceContext { domain_path: domain_path.clone(), ..(*context).clone() });
    origin.map(|origin| Arc::new(SourceSite { origin, context, span, syntax }))
}
fn begin_segment(program: &Program, facts: &prefix::Facts, calls: &mut [DomainCall]) {
    if let Some(call) = calls.last_mut() && call.stage == program.stages.len() {
        call.prefix_requirements = facts.relations();
        call.element_requirements = facts.element_facts(program, None);
    }
}

async fn expand_stages(
    world: &World,
    input: DomainInput<'_>,
    body: impl Iterator<Item=Stmt>,
    env: &mut BTreeMap<String, Root>,
    program: &mut Program,
    stack: &mut Vec<String>,
    boundary: &mut bool,
    facts: &mut prefix::Facts,
    calls: &mut Vec<DomainCall>,
    domain_path: &Arc<[DomainSite]>,
    functions: &mut native_function::Module,
    scopes: &mut BTreeMap<usize, RangeContext>,
) -> Result<bool> {
    let instance=input.owner();
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
        match statement.kind() {
            StmtKind::Return(None) => return Ok(true),
            StmtKind::Let(annotation, name, expression) => {
                if env.contains_key(name) {
                    return d.source.fail(statement.span(), "B125", format!("重复局部变量或覆盖参数 {name}"));
                }
                let annotation=match annotation.as_ref() {
                    Some(ty)=>Some(unsafe { world.call(instance.bindings.type_name(world,ty,
                        (&d.source,&d.module,&d.imports),statement.span())).await }?),
                    None=>None,
                };
                let value = unsafe { world.call(constant::stage_expression(
                    world, instance, env, program, &expression, annotation.as_ref(), domain_path,
                )).await }?;
                env.insert(name.to_string(), value);
            }
            StmtKind::If(condition, yes, no) => {
                let Root::Literal(ty, words, evaluation) = unsafe { world.call(stage_root(world, instance, env, program, &condition, domain_path)).await }? else {
                    return d.source.fail(condition.span(), "B251", "阶段 if 条件必须是编译期 bool；动态条件放在 parallel 或CPU主流程中");
                };
                if ty != Ty::scalar(Scalar::Bool) {
                    return d.source.fail(condition.span(), "B251", "阶段 if 条件必须是编译期 bool");
                }
                let selected = if words[0] == 0 { no } else { yes };
                let mut child = env.clone();
                let first_call = calls.len();
                let returned = unsafe { world.call(expand_stages(world, input, selected.iter(), &mut child, program, stack, boundary, facts, calls, domain_path, functions, scopes)).await }?;
                program.generated_origin = Some(Arc::new(GeneratedOrigin {
                    reason: GenerationReason::StageIfSelection,
                    trigger: Some(GenerationAnchor::StageIfSelection {
                        condition: input.operation_source(condition.span(), SourceSyntax::Expression, domain_path),
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
                        statement.span(),
                        "B125",
                        format!("重复阶段 for 计数器或覆盖参数 {name}"),
                    );
                }
                let bound = async |expr: &Expr| -> Result<StageForBound> {
                    match unsafe { world.call(stage_root(world, instance, env, program, expr, domain_path)).await }? {
                        Root::Literal(ty, words, evaluation) if ty == Ty::scalar(Scalar::U32) => Ok(StageForBound {
                            expression: input.operation_source(expr.span(), SourceSyntax::Expression, domain_path),
                            value: words[0] as u32, evaluation,
                        }),
                        _ => d.source.fail(
                            expr.span(),
                            "B251",
                            "program 阶段 for 的范围必须是编译期 u32；动态循环放在 parallel 中",
                        ),
                    }
                };
                let first = bound(&first).await?;
                let last = bound(&last).await?;
                let retained = program.generated_origin.take();
                let mut iterations = Vec::new();
                let mut returned = false;
                for index in first.value..last.value {
                    let first_stage = program.stages.len();
                    let first_call = calls.len();
                    let mut child = env.clone();
                    child.insert(
                        name.to_string(),
                        Root::Literal(Ty::scalar(Scalar::U32), vec![Bits::from(index)], None),
                    );
                    returned = unsafe { world.call(expand_stages(
                        world, input, body.iter(), &mut child, program, stack, boundary, facts, calls, domain_path, functions, scopes,
                    )).await }?;
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
                        statement: input.operation_source(statement.span(), SourceSyntax::Statement, domain_path),
                        counter: name.into(), first, last, iterations, retained,
                    }),
                }));
                if returned {
                    return Ok(true);
                }
                first_stage = program.stages.len();
            }
            StmtKind::Parallel(name, syntax, body) => {
                begin_segment(program, facts, calls);
                let data = &syntax.dataset();
                let Some(Root::Data(domain, domain_access)) = env.get(*data) else {
                    return d
                        .source
                        .fail(statement.span(), "B106", format!("{data} 不是数据集参数"));
                };
                let prefix = syntax
                    .count()
                    .as_ref()
                    .map(|expr| prefix::resolve(program, env, *domain, expr, d, facts))
                    .transpose()?;
                let selection = syntax
                    .selection()
                    .as_ref()
                    .map(|expr| {
                        let (binding, word, access) =
                            collective_field(program, env, expr, d, Scalar::Bool)?;
                        if !matches!(access, Access::Read | Access::InOut)
                            && !(access == Access::Write && facts.initialized(program, binding, word))
                        {
                            return d.source.fail(
                                expr.span(),
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
                                expr.span(),
                                "B187",
                                "where 谓词容量不能小于原始域",
                            );
                        }
                        Ok(Selection { binding, word })
                    })
                    .transpose()?;
                let mut b = unsafe { world.call(Builder::domain(world, input, program, *domain)).await }?;
                if program.target == Target::Cpu {
                    let frame = vec![b.reg(Scalar::U32), b.reg(Scalar::U32)];
                    b.application = Some(application::Context { frame: Some(frame), range: true });
                }
                b.element_facts = Some(&facts.elements);
                b.prefix_facts = Some(&facts.scopes.last().unwrap().0);
                b.source_context = Arc::new(SourceContext { domain_path: domain_path.clone(), ..(*b.source_context).clone() });
                b.functions = std::mem::take(functions);
                b.roots(env);
                let operation_source = input.operation_source(statement.span(), SourceSyntax::Statement, domain_path);
                let index = b.reg(Scalar::U32);
                b.code.push(Inst::generated(InstKind::Index(index), Arc::new(GeneratedOrigin {
                    reason: GenerationReason::ParallelIndex, trigger: operation_source.clone().map(GenerationAnchor::Syntax),
                })));
                b.global_index = index;
                b.refine_parallel_domain();
                if b.env.contains_key(name) {
                    return d
                        .source
                        .fail(statement.span(), "B107", "迭代变量不能覆盖参数");
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
                b.env.insert(name.to_string(), element);
                // 同一次迭代的其他普通写目标与命名游标共用原位置事实和合流。
                for (&binding, &access) in &b.view_access {
                    if binding != *domain && matches!(access, Access::Write | Access::InOut)
                        && program.same_length(binding, *domain)
                    {
                        let ty = program.buffers[binding].ty.clone();
                        let words = ty.components.iter().map(|x| x.1).collect();
                        b.env.insert(format!("\0{binding}"), Binding::Owned(OwnedBinding {
                            place: Place::Element { ty, binding, index, words, writable: true },
                            proof: facts.elements.get(&binding).cloned().unwrap_or_default(),
                            uniform: false,
                            initialization: None,
                        }));
                    }
                }
                if let Some(limit) = &prefix {
                    b.reads.insert(limit.binding);
                    b.cross_access.insert(limit.binding);
                }
                b.prefix = prefix;
                let inputs = unsafe { world.call(b.tile_inputs(&body)).await }?;
                unsafe { world.call(b.block(&body.slice(inputs..))).await }?;
                b.parallel_access(statement.span())?;
                let appended: Vec<_> = b
                    .appends
                    .iter()
                    .map(|(domain, count)| PrefixRelation { domain_region: None,
                        domain: *domain,
                        count: *count,
                    })
                    .collect();
                for relation in &appended {
                    prefix::prove(
                        facts,
                        program,
                        relation.domain,
                        relation.count,
                        &d.source,
                        statement.span(),
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
                let tiles = b.lower_tiles(statement.span())?;
                if selection.is_some() && !tiles.is_empty() {
                    return d.source.fail(
                        statement.span(),
                        "B250",
                        "where 稀疏域尚未定义 tile 邻接与发布语义",
                    );
                }
                if let Some(selection) = selection {
                    b.reads.insert(selection.binding);
                    b.cross_access.insert(selection.binding);
                }
                let range_frame = b.application.as_ref().and_then(|context| context.frame.as_ref())
                    .filter(|frame| {
                        let mut used = false;
                        crate::optimize::visit(&b.code, &mut |inst| {
                            used |= !inst.all_operands(&mut |reg| !frame.contains(&reg));
                        });
                        used
                    })
                    .map(|frame| {
                        let offset = program.native_frame.unwrap_or_else(|| ir::align_up(program.uniform_bytes, 8));
                        b.code.splice(0..0, [
                            Inst::new(InstKind::Uniform(frame[0], offset)),
                            Inst::new(InstKind::Uniform(frame[1], offset + 4)),
                        ]);
                        offset
                    });
                let memory_regions = std::mem::take(&mut b.memory_regions);
                let write_regions = memory_regions.iter()
                    .filter_map(|(&binding, (_, written))| (!written.is_empty())
                        .then(|| (binding, written.keys().copied().collect()))).collect();
                *functions = std::mem::take(&mut b.functions);
                let (elements, access, appends) = (b.env, b.view_access, b.appends);
                let mut stage = Stage {
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
                    prefix_guarantees: appended,
                    element_guarantees: Vec::new(),
                    cross_access: b.cross_access.into_iter().collect(),
                };
                let mut scoped = range_frame.is_some();
                crate::optimize::visit(&stage.code, &mut |inst| {
                    scoped |= matches!(&inst.kind,
                        InstKind::Call(Call { failure: Some(FailureContinuation::Scope(_)), .. })
                        | InstKind::Cleanup { failure: Some(_), .. });
                });
                if scoped {
                    scopes.insert(program.stages.len(), RangeContext {
                        output_scopes: b.output_scopes,
                        view_model: b.view_model,
                        residency: b.residency,
                    });
                }
                if let Some(offset) = range_frame {
                    program.native_frame = Some(offset);
                    program.uniform_bytes = offset + 8;
                }
                for (&binding, written) in &stage.write_regions {
                    let output = if access[&binding] == Access::Atomic {
                        facts.elements.get(&binding).map(|old| {
                            let ty = &program.buffers[binding].ty;
                            let mut output = old.clone();
                            for (component, &(scalar, offset)) in ty.components.iter().enumerate() {
                                let mut value = old.project(&[component], ty.components.len());
                                for (&(start, end), stored) in &memory_regions[&binding].1 {
                                    if u64::from(offset) < u64::from(end)
                                        && u64::from(start) < u64::from(offset) + u64::from(scalar.size())
                                    {
                                        if start == offset && u64::from(end) == u64::from(offset) + u64::from(scalar.size())
                                            && let Some(stored) = stored
                                        {
                                            value.join(stored, 1, None);
                                        } else {
                                            value.forget_value(&[0], 1);
                                        }
                                    }
                                }
                                output.assign_elements(&[component], ty.components.len(), &value);
                            }
                            output
                        })
                    } else if prefix.is_none() && selection.is_none()
                        && access[&binding] != Access::Atomic
                        && !appends.iter().any(|(target, count)| *target == binding || count.binding == binding)
                    {
                        elements.values().find_map(|value| match value {
                            Binding::Owned(OwnedBinding { place: Place::Element { binding: target, index: current, ty, .. }, proof, .. })
                                if *target == binding && *current == index && *ty == program.buffers[binding].ty =>
                                Some(proof.element_facts(|target| bounds::Proof::default()
                                    .with_domains(Domains::from([IndexDomain::Data(target)])))),
                            _ => None,
                        })
                    } else { None };
                    for &(start, end) in written { facts.written(program, binding, start, end, output.as_ref()); }
                    if let Some(output) = output {
                        stage.element_guarantees.extend(output.element_guarantees(binding, &program.buffers[binding].ty,
                            |offset, bytes| written.iter().any(|&(start, end)|
                                u64::from(offset) < u64::from(end) && u64::from(start) < u64::from(offset) + u64::from(bytes))));
                    }
                }
                for &relation in &stage.prefix_guarantees {
                    facts.grant(relation);
                }
                program.stages.push(stage);
            }
            StmtKind::Expr(expression) => {
                let call_span = expression.span();
                match expression.kind() {
                    ExprKind::Call(name, args) if name == "clear_prefix"
                        && world.resolve(name, &d.module, &d.imports, true).is_missing() =>
                    {
                        begin_segment(program, facts, calls);
                        let output = append::clear(program, env, &args, d, statement.span(), facts,
                            input.operation_source(call_span, SourceSyntax::Expression, domain_path))?;
                        facts.outputs.push(output);
                        *boundary = false;
                    }
                    ExprKind::Call(name, args) if name == "stable_sort_by_key"
                        && world.resolve(name, &d.module, &d.imports, true).is_missing() =>
                    {
                        begin_segment(program, facts, calls);
                        sort::compile(program, env, &args, d, statement.span(), facts, input.operation_source(call_span, SourceSyntax::Expression, domain_path))?;
                        *boundary = false;
                    }
                    ExprKind::Call(name, args) if name == "compact"
                        && world.resolve(name, &d.module, &d.imports, true).is_missing() =>
                    {
                        begin_segment(program, facts, calls);
                        compact::compile(program, env, &args, d, statement.span(), facts, input.operation_source(call_span, SourceSyntax::Expression, domain_path))?;
                        *boundary = false;
                    }
                    ExprKind::Call(name, args) | ExprKind::Specialize(name, _, args) => 'call: {
                        let arguments = if let ExprKind::Specialize(_, arguments, _) = expression.kind() {
                            Some(arguments)
                        } else {
                            None
                        };
                        let explicit = arguments.as_ref();
                        let callee_key = world.resolve(name, &d.module, &d.imports, true);
                        if callee_key.is_missing() && explicit.is_none() {
                            use collective::Form;
                            let builtin = collective::from_source(name);
                            let stage = match builtin.form {
                                Form::Reduce => Some(reduce::compile(
                                    program, env, builtin.operation, &args, d, statement.span(), facts,
                                    input.operation_source(call_span, SourceSyntax::Expression, domain_path),
                                )),
                                Form::SegmentedReduce => Some(segmented_reduce::compile(
                                    program, env, builtin.operation, &args, d, statement.span(), facts,
                                    input.operation_source(call_span, SourceSyntax::Expression, domain_path),
                                )),
                                Form::Scan | Form::SegmentedScan => Some(scan::compile(
                                    program, env, builtin.operation, matches!(builtin.form, Form::SegmentedScan),
                                    &args, d, statement.span(), facts,
                                    input.operation_source(call_span, SourceSyntax::Expression, domain_path),
                                )),
                                _ => None,
                            };
                            if let Some(stage) = stage {
                                stage?;
                                *boundary = false;
                                break 'call;
                            }
                            if let Some(kind) =
                                custom_reduce::Kind::from_source(name)
                            {
                                begin_segment(program, facts, calls);
                                unsafe { world.call(custom_reduce::compile(
                                    world,
                                    program,
                                    env,
                                    &args,
                                    instance,
                                    statement.span(),
                                    facts,
                                    name.ends_with("_map"),
                                    kind,
                                    domain_path.clone(),
                                    functions,
                                    input.operation_source(call_span, SourceSyntax::Expression, domain_path),
                                )).await }?;
                                *boundary = false;
                                break 'call;
                            }
                        }
                        let Some(key) = callee_key.unique() else {
                            return d
                                .source
                                .fail(statement.span(), "B109", format!("未知计算程序 {name}"));
                        };
                        let (instance, mut child) = unsafe { world.call(call_execution::domain_call(
                            world, instance, env, program, &key, args, explicit, statement.span(), domain_path,
                        )).await }?;
                        let callee = &instance.definition;
                        let bound_contracts: Vec<_> = instance.contracts().collect();
                        for (left, right) in contracts::root_lengths(&child, callee, &bound_contracts)? {
                            if !program.same_length(left, right) {
                                return d.source.fail(statement.span(), "B226", "整域调用前未证明same_length数据集长度关系");
                            }
                        }
                        let required = prefix::requirements(program, &child, callee, &bound_contracts)?;
                        let elements = unsafe { world.call(prefix::element_requirements(world, &program.buffers, &child, &instance, &bound_contracts)).await }?;
                        facts.prove_elements(program, &elements, &d.source, statement.span())?;
                        for relation in &required {
                            prefix::prove(
                                facts,
                                program,
                                relation.domain,
                                relation.count,
                                &d.source,
                                statement.span(),
                            )?;
                        }
                        let compiled = callee.compiled.is_some();
                        let (caller_origin, caller_context) = d.syntax_context(Some(&input.owner().name));
                        let mut child_path = domain_path.to_vec();
                        child_path.push(DomainSite {
                            origin: caller_origin.clone(), instance: caller_context.instance.clone(),
                            span: call_span, target: Arc::from(instance.name.as_str()),
                            transition: if compiled { DomainTransition::Compiled } else { DomainTransition::Expanded },
                        });
                        let child_path: Arc<[DomainSite]> = child_path.into();
                        let guarantees = if compiled {
                            use crate::module::codec::Decode;
                            let target = if callee.function.target() == Target::Gpu { Target::Gpu } else { Target::Cpu };
                            let symbol = crate::module::domain::symbol(&instance.name, target);
                            let bytes = callee.compiled.as_ref().unwrap().get(&symbol).ok_or_else(|| Diagnostic {
                                    source: d.source.clone(), span: statement.span(), code: "B270",
                                    message: format!("缺少已编译整域入口合同 {symbol}"),
                                })?;
                            let contract = native_function::Contract::from_bytes(bytes).map_err(|error| Diagnostic {
                                source: d.source.clone(), span: statement.span(), code: "B270", message: error.to_string(),
                            })?.domain.ok_or_else(|| Diagnostic {
                                source: d.source.clone(), span: statement.span(), code: "B270", message: format!("入口 {symbol} 不是整域合同"),
                            })?;
                            let bindings: Vec<_> = callee.function.params().iter().filter_map(|p| match child[p.name()] {
                                Root::Data(binding, _) => Some(binding), _ => None,
                            }).collect();
                            let stage = program.stages.len();
                            let segment_start = calls.last().map_or(0, |call| call.stage);
                            let (prefix_guarantees, element_guarantees) = if segment_start == stage {
                                (Vec::new(), Vec::new())
                            } else {
                                let writes = Program::stage_write_regions(&program.stages[segment_start..]);
                                (facts.relations(), facts.element_facts(program, Some(&writes)))
                            };
                            let mut write_regions = BTreeMap::new();
                            for (logical, written) in contract.write_regions {
                                let binding = bindings[logical];
                                let output = contract.elements.get(&logical).map(|proof|
                                    proof.element_facts(|target| bounds::Proof::default()
                                        .with_domains(Domains::from([IndexDomain::Data(bindings[target])]))));
                                for &(start, end) in &written {
                                    facts.written(program, binding, start, end, output.as_ref());
                                }
                                write_regions.entry(binding).or_insert_with(BTreeSet::new).extend(written);
                            }
                            facts.outputs.extend(contract.outputs.into_iter().map(|mut output| {
                                output.binding = bindings[output.binding];
                                output.domain = output.domain.map(|binding| bindings[binding]);
                                output
                            }));
                            calls.push(DomainCall {
                                stage,
                                instance: instance.clone(),
                                arguments: callee
                                    .function
                                    .params()
                                    .iter()
                                    .map(|p| child[p.name()].clone())
                                    .collect(),
                                write_regions,
                                element_guarantees,
                                element_requirements: Vec::new(),
                                prefix_guarantees,
                                prefix_requirements: Vec::new(),
                                source: caller_origin.map(|origin| Arc::new(SourceSite {
                                    origin, context: Arc::new(SourceContext { domain_path: child_path, ..(*caller_context).clone() }),
                                    span: call_span, syntax: SourceSyntax::Expression,
                                })),
                            });
                            *boundary = true;
                            let mut guarantees = contract.prefix_guarantees;
                            for relation in &mut guarantees {
                                relation.domain = bindings[relation.domain];
                                relation.count.binding = bindings[relation.count.binding];
                                if relation.domain == relation.count.binding {
                                    return d.source.fail(
                                        call_span,
                                        "B192",
                                        "前缀域约束需要独立形状 1 资源中的 u32 计数字段",
                                    );
                                }
                            }
                            guarantees
                        } else {
                            facts.enter(&required);
                            unsafe { world.call(expand_program(
                                world, &instance, &mut child, program, stack, boundary, facts, calls, child_path, functions, scopes,
                            )).await }?;
                            let guarantees = prefix::proved_guarantees(
                                program, callee, facts,
                                prefix::bindings(program, &child, callee, &bound_contracts, ContractKind::Ensures),
                            )?;
                            facts.leave();
                            guarantees
                        };
                        first_stage = program.stages.len();
                        for relation in guarantees {
                            facts.grant(relation);
                        }
                    }
                    _ => return d.source.fail(statement.span(), "B119",
                        "program 顶层表达阶段；局部控制流放在 parallel 内"),
                }
            }
            StmtKind::Phase => {
                *boundary = true;
            }
            _ => {
                return d.source.fail(
                    statement.span(),
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
async fn stage_root(
    world: &World,
    instance: &crate::generics::Instance,
    env: &BTreeMap<String, Root>,
    program: &Program,
    arg: &Expr,
    domain_path: &Arc<[DomainSite]>,
) -> Result<Root> {
    let d = &instance.definition;
    let source = arg.source_view();
    let mut cursor = SourceValueCursor { phase: 0 };
    let mut facts = StageRootFacts { present: 0, constant: 0 };
    let mut name = "";
    let mut bound = None;
    let mut value = None;
    let mut key = None;
    loop {
        let step = unsafe { stage_root_step(cursor, arg.signature_node(), StageRootFacts { ..facts }) };
        cursor = step.cursor;
        match step.need {
            StageRootNeed::Binding => {
                name = source.text(step.spelling);
                bound = env.get(name);
                facts.present = u32::from(bound.is_some());
            }
            StageRootNeed::Bound => return Ok(bound.unwrap().clone()
                .reference_literal(|| operation_source(instance, arg.span(), SourceSyntax::Expression, domain_path))),
            StageRootNeed::Scope => {
                value = instance.bindings.values.get(name);
                facts.present = u32::from(value.is_some());
            }
            StageRootNeed::ScopeValue => return Ok(Root::Literal(Ty::scalar(Scalar::U32), vec![Bits::from(*value.unwrap())], None)
                .reference_literal(|| operation_source(instance, arg.span(), SourceSyntax::Expression, domain_path))),
            StageRootNeed::Resolve => {
                key = world.resolve(name, &d.module, &d.imports, true).unique();
                facts.present = u32::from(key.is_some());
            }
            StageRootNeed::FunctionKind => facts.constant = u32::from(
                world.functions[key.as_ref().unwrap()].function.kind() == FunctionKind::Constant),
            StageRootNeed::Constant => {
                let key = key.unwrap();
                let constant = unsafe { world.call(constant::evaluate(world, &key, &[])).await }?;
                let (ty, words, origin) = constant.value.clone();
                return Ok(Root::Literal(ty, words, origin)
                    .reference_literal(|| operation_source(instance, arg.span(), SourceSyntax::Expression, domain_path)));
            }
            StageRootNeed::Unknown => return d.source.fail(arg.span(), "B113", format!("未知参数或 const {name}")),
            StageRootNeed::StaticU32 => return Ok(Root::Literal(Ty::scalar(Scalar::U32), vec![Bits::from(step.bits)], None)),
            StageRootNeed::Number => {
                let (ty, bits) = number(&d.source, arg.span(), source.text(step.spelling))?;
                return Ok(Root::Literal(ty, vec![bits], None));
            }
            StageRootNeed::Expression => return unsafe {
                world.call(constant::stage_expression(world, instance, env, program, arg, None, domain_path)).await
            },
        }
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
            .fail(expr.span(), "B175", format!("集合运算字段需要 {scalar:?}"));
    }
    Ok((binding, word, access))
}
fn collective_root<'a>(
    program: &'a Program, env: &BTreeMap<String, Root>, name: &str, d: &Definition, span: Span,
) -> Result<(usize, Access, &'a Ty)> {
    let Some(Root::Data(binding, access)) = env.get(name) else {
        return d.source.fail(span, "B173", "未知集合运算数据集");
    };
    Ok((*binding, *access, &program.buffers[*binding].ty))
}
fn collective_projection<'a, R>(
    expr: &Expr, d: &Definition, span: Span,
    root: impl FnOnce(&Expr) -> Result<(usize, u32, Access, &'a Ty, R)>,
) -> Result<(usize, u32, Access, &'a Ty, R)> {
    match expr.kind() {
        ExprKind::Member(base, field) => {
            let (binding, word, access, ty, result) = collective_projection(&base, d, span, root)?;
            let Some((offset, ty)) = ty.field(field) else {
                return d.source.fail(span, "B174", "未知集合运算字段");
            };
            Ok((binding, word + offset, access, ty, result))
        }
        _ => root(expr),
    }
}
fn collective_value_field(
    program: &Program, env: &BTreeMap<String, Root>, expr: &Expr, d: &Definition,
) -> Result<(usize, u32, Access, Ty)> {
    if !matches!(expr.kind(), ExprKind::Member(..)) {
        return d.source.fail(expr.span(), "B172", "集合运算参数必须是 dataset.field");
    }
    let (binding, word, access, ty, ()) = collective_projection(expr, d, expr.span(), |base| {
        let ExprKind::Name(name) = base.kind() else {
            return d.source.fail(expr.span(), "B172", "集合运算参数必须是命名数据集字段");
        };
        let (binding, access, ty) = collective_root(program, env, name, d, expr.span())?;
        Ok((binding, 0, access, ty, ()))
    })?;
    Ok((binding, word, access, ty.clone()))
}
use crate::syntax::wire::DeclaredArgumentForm;

fn declared_argument_form(ty: &crate::syntax::Type) -> DeclaredArgumentForm {
    unsafe { crate::syntax::wire::declared_argument_form(ty.signature_node()) }
}

pub(crate) fn validate_signature(d: &Definition) -> Result<()> {
    use crate::syntax::wire::{SignatureErrorKind as Kind, signature_error};

    let domain = u32::from(d.execution == crate::execution::Requirement::Domain);
    let error = unsafe { signature_error(d.function.signature_node(), domain) };
    let (code, message) = match error.kind {
        Kind::Valid => return Ok(()),
        Kind::CAbi => ("B263", "extern C 需要 on cpu 普通C签名，只接收普通值及 read／write／inout 位置；from导入是无正文的固定符号，B定义可实例化，输入可声明 requires nonzero，输出可声明 ensures initialized，不透明内存效果可声明 ensures memory"),
        Kind::OwnedResult => ("B224", "拥有的 dataset 返回属于普通 CPU 应用函数，借用与并行实例不转移资源所有权"),
        Kind::MissingTile => ("B207", "cooperative 接口需要至少一个 tile<T> 借用来声明块范围"),
        Kind::OwnedParameter => ("B224", "拥有聚合参数只支持普通CPU函数"),
        Kind::Acceleration => ("B182", "acceleration 必须是 GPU 函数的 read 资源"),
        Kind::DatasetAccess => ("B120", "普通函数dataset参数支持read／write／inout／atomic；仅CPU应用函数可按值取得所有权"),
        Kind::TextureAccess => ("B239", "texture2d／texture3d 必须是 read 借用参数"),
        Kind::TileAccess => ("B207", "tile<T> 只能是 cooperative 的只读或 atomic 借用参数"),
    };
    let span = Span { start: error.span.start as usize, end: error.span.end as usize };
    d.source.fail(span, code, message)
}

pub(crate) async fn check_function(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let name = &instance.name;
    let d = &instance.definition;
    if d.function.kind() == FunctionKind::Constant {
        return unsafe { world.call(evaluate_constant(world, name, &[])).await }.map(|_| ());
    }
    if d.function.kind() == FunctionKind::Value && d.function.target() == Target::Cpu {
        return unsafe { world.call(native_function::check(world, instance)).await };
    }
    unsafe { world.call(checked_function(world, instance, d.function.target(), None, native_function::Module::default())).await }.map(|_| ())
}
struct CheckedContext {
    program: Arc<Program>,
    output_scopes: usize,
    task_completions: BTreeMap<TaskCompletionSource, Vec<(Ty, [Reg; 2])>>,
}
struct CheckedFunction {
    context: CheckedContext,
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
mod function_inputs;
use function_inputs::{function_parameters, function_program};

async fn checked_function(
    world: &World,
    instance: &crate::generics::Instance,
    target: Target,
    application: Option<application::Context>,
    functions: native_function::Module,
) -> Result<Box<CheckedFunction>> {
    let _required_scope = bounds::required_scope();
    let name = &instance.name;
    let d = &instance.definition;
    let mut bound_parameters = Vec::new();
    let mut program=unsafe { world.call(function_program(world,name,instance.parameters(world),
        (&d.source,&d.module,&d.imports,d.function.kind()),target,d.source_origin(), |parameter| bound_parameters.push(parameter))).await }?;
    let roots=program.buffers.iter().enumerate().map(|(index,b)|(b.name.clone(),Root::Data(index,b.access))).collect();
    program.prefix_requirements=instance.contracts().filter(|c|c.kind==ContractKind::Requires&&matches!(c.predicate,ContractPredicate::Prefix{..}))
        .map(|c|prefix::relation(&program,&roots,d,&c)).collect::<Result<_>>()?;
    let program=Arc::new(program);
    let mut b = Box::new(unsafe { world.call(Builder::new(world, instance, &program, 0, Some(&instance.name))).await }?);
    b.in_function = true;
    b.application = application;
    b.functions = functions;
    if b.application.is_some() { b.view_uses = view_liveness::State::new(world, instance); }
    let (mut parameters, mut inputs, output_contracts) = unsafe { world.call(function_parameters(&mut b, instance, name, bound_parameters)).await }?;
    let result_name=unsafe { world.call(instance.bindings.type_name(world,&d.function.result(),(&d.source,&d.module,&d.imports),d.function.span())).await }?;
    let layout = if result_name.name == "void" || result_name.is_never() {
        None
    } else {
        Some(unsafe { world.call(b.storage_layout(&result_name, d.function.span())).await }?)
    };
    let result = layout.as_ref().map(|layout| layout.storage().into_owned());
    function_inputs::return_inputs(&mut b, d.function.c_abi(), result.as_ref(), &mut parameters, &mut inputs);
    let (returned,written) = unsafe { world.call(b.function_body(instance, &result_name, layout, d.function.body().as_ref().unwrap())).await }?;
    unsafe { world.call(finish_checked_function(b,program.clone(),name,d.function.span(),d.source_origin(),result,parameters,inputs,output_contracts,returned,written)).await }
}

#[inline(never)]
async fn finish_checked_function(
    mut b: Box<Builder<'_>>,
    program: Arc<Program>,
    name: &str,
    span: Span,
    source_origin:Option<SourceOrigin>,
    result:Option<Ty>,
    parameters: Vec<CallParameter>,
    inputs: Vec<Reg>,
    output_contracts: BTreeSet<(ContractKind, ContractPredicate)>,
    mut returned: Option<returns::Returned>,
    mut written: Vec<Option<returns::Returned>>,
) -> Result<Box<CheckedFunction>> {
    for value in returned.iter_mut().chain(written.iter_mut().flatten()){unsafe { b.world.call(b.normalize_element_facts(value,span)).await }?;}
    let outputs = returned.as_ref().map(returns::Returned::registers).unwrap_or_default();
    let (returned, semantic_inputs, written, initialization) =
        unsafe { b.world.call(native_function::canonical(&mut b, returned, &inputs, &parameters, written)).await }?;
    let body = FunctionBody {
        fallible: false,
        name: name.to_string(),
        source_origin,
        parameters,
        result,
        body: ValueCode {
            entry_origin: None,
            generated_origin: None,
            registers: b.registers,
            locals: b.locals,
            code: b.code,
            inputs,
            outputs,
        },
    };
    Ok(Box::new(CheckedFunction {
        context: CheckedContext { program, output_scopes: b.output_scopes, task_completions: b.task_completions },
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
    numeric_literal::number(source, span, n, false)
}
// 原检查事实只保持到该Call的真实失败出口，不进入资源ABI或执行映像。
#[derive(Clone)]
pub(crate) enum FailureContinuation {
    Return,
    Scope(Box<FailureScope>),
    Cleanup(usize),
}
#[derive(Clone)]
pub struct FailureScope {
    env: BTreeMap<String, Binding>,
    path: bounds::Clause,
    initialized: BTreeSet<Position>,
    scope: LexicalScope,
    source_definition: Option<Arc<SourceOrigin>>,
    source_context: Arc<SourceContext>,
    source_site: Option<Arc<SourceSite>>,
    mutable_names: BTreeSet<String>,
    frame: Option<Vec<Reg>>,
    domain: usize,
    control_uniform: bool,
    calls: Vec<String>,
    consumed: Vec<Place>,
    task_outputs: Vec<native_function::TaskCompletionOutput>,
}
impl std::fmt::Debug for FailureScope {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str("FailureScope")}
}
impl std::fmt::Debug for FailureContinuation {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        match self {
            Self::Return=>f.write_str("Return"),
            Self::Scope(_)=>f.write_str("Scope"),
            Self::Cleanup(slot)=>f.debug_tuple("Cleanup").field(slot).finish(),
        }
    }
}
#[derive(Clone,Copy)]
enum PendingError { Group(usize), Exit(usize) }
impl PendingError {
    fn slot(self)->usize {match self {Self::Group(slot)|Self::Exit(slot)=>slot}}
}
// A generated domain reads its real captured signature and original statement.
// It is not another source declaration or an AST owned by the checker.
#[derive(Clone)]
enum LexicalScope {
    Function(crate::generics::Instance),
    Parallel {
        owner: crate::generics::Instance,
        parameters: Arc<[Param]>,
        statement: Stmt,
        site: Option<Arc<SourceSite>>,
    },
    Values {
        source: Source, module: String, imports: Vec<String>,
        bindings: Arc<crate::generics::Bindings>, span: Span,
        parameters:Arc<[Param]>,source_definition:Option<Arc<SourceOrigin>>,source_context:Arc<SourceContext>,
    },
}
enum ScopeParameter<'a> {
    Declaration(&'a crate::generics::Instance, crate::syntax::Param),
    Bound(&'a Param),
}
impl ScopeParameter<'_> {
    fn name(&self)->&str {match self {Self::Declaration(_,p)=>p.name(),Self::Bound(p)=>&p.name}}
    fn access(&self)->Access {match self {Self::Declaration(_,p)=>p.access(),Self::Bound(p)=>p.access}}
    fn span(&self)->Span {match self {Self::Declaration(_,p)=>p.span(),Self::Bound(p)=>p.span}}
    async fn bound(&self,world:&World)->Result<Param> {
        match self {Self::Declaration(instance,p)=>unsafe { world.call(instance.parameter(world,p.clone())).await },Self::Bound(p)=>Ok((*p).clone())}
    }
}
impl LexicalScope {
    fn function(&self) -> Option<&crate::generics::Instance> {
        match self {Self::Function(instance)=>Some(instance),Self::Parallel{..}|Self::Values{..}=>None}
    }
    fn source(&self)->&Source {
        match self {Self::Function(instance)|Self::Parallel{owner:instance,..}=>&instance.definition.source,Self::Values{source,..}=>source}
    }
    fn module(&self)->&str {
        match self {Self::Function(instance)|Self::Parallel{owner:instance,..}=>&instance.definition.module,Self::Values{module,..}=>module}
    }
    fn imports(&self)->&[String] {
        match self {Self::Function(instance)|Self::Parallel{owner:instance,..}=>&instance.definition.imports,Self::Values{imports,..}=>imports}
    }
    fn bindings(&self)->&Arc<crate::generics::Bindings> {
        match self {Self::Function(instance)|Self::Parallel{owner:instance,..}=>&instance.bindings,Self::Values{bindings,..}=>bindings}
    }
    fn span(&self)->Span {
        match self {Self::Function(instance)=>instance.definition.function.span(),Self::Parallel{statement,..}=>statement.span(),Self::Values{span,..}=>*span}
    }
    fn parameters(&self)->impl Iterator<Item=ScopeParameter<'_>> {
        let declaration=self.function().map(|instance|(instance,instance.definition.function.params()));
        let bound=match self {Self::Parallel{parameters,..}|Self::Values{parameters,..}=>Some(parameters.as_ref()),_=>None};
        declaration.into_iter().flat_map(|(instance,parameters)|parameters.into_iter().map(move|p|ScopeParameter::Declaration(instance,p)))
            .chain(bound.into_iter().flat_map(|parameters|parameters.iter().map(ScopeParameter::Bound)))
    }
    fn body(&self)->Option<Block> {
        self.function().and_then(|instance|instance.definition.function.body())
    }
    fn execution(&self)->crate::execution::Requirement {
        match self {Self::Function(instance)=>instance.definition.execution,Self::Parallel{..}=>crate::execution::Requirement::Domain,
            Self::Values{..}=>crate::execution::Requirement::Invocation}
    }
    fn syntax_context(&self,instance:Option<&str>)->(Option<Arc<SourceOrigin>>,Arc<SourceContext>) {
        match self {
            Self::Function(function)=>function.definition.syntax_context(instance),
            Self::Parallel{site:Some(site),..}=>(Some(site.origin.clone()),site.context.clone()),
            Self::Values{source_definition,source_context,..}=>(source_definition.clone(),source_context.clone()),
            Self::Parallel{site:None,..}=>(None,Arc::new(SourceContext{
                instance:None,inlined_at:Box::default(),domain_path:Arc::from([])})),
        }
    }
}

struct Builder<'a> {
    residency: residency::Inputs,
    view_model: view_contracts::Function,
    application: Option<application::Context>,
    pending_error: Option<PendingError>,
    task_completions: BTreeMap<TaskCompletionSource, Vec<(Ty, [Reg; 2])>>,
    task_return_source: Option<[Reg;2]>,
    functions: native_function::Module,
    world: &'a World,
    scope: LexicalScope,
    program: &'a Program,
    element_facts: Option<&'a BTreeMap<usize, bounds::Proof>>,
    prefix_facts: Option<&'a BTreeSet<PrefixRelation>>,
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
    memory_regions: BTreeMap<usize, (BTreeSet<(bool, u32, u32)>, BTreeMap<(u32, u32), Option<Box<bounds::Proof>>>)>,
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
    iterators: Vec<Option<(String, usize, Value)>>,
    mutable_names: BTreeSet<String>,
    view_uses: view_liveness::State,
    tiles: Vec<CheckedTile>,
    appends: BTreeMap<usize, PrefixCount>,
    prefix: Option<PrefixCount>,
}
impl<'a> Builder<'a> {
    fn task_completion(&self, source:TaskCompletionSource, result:&Ty) -> Option<[Reg;2]> {
        self.task_completions.get(&source)?.iter().find(|(ty,_)|ty==result).map(|(_,address)|*address)
    }
    fn task_completion_destination(
        &mut self, task:Reg, result:&Ty, sources:&view::Borrow,
    ) -> Option<([Reg;2],Option<Reg>)> {
        if sources.inputs.entries().any(|(_,when)|self.path.overlaps(&when))
            || sources.calls.entries().any(|(_,when)|self.path.overlaps(&when))
            || sources.reads.entries().any(|(_,when)|self.path.overlaps(&when)) {
            return None;
        }
        let mut destinations=BTreeMap::new();
        for (source,_) in sources.sources.entries().filter(|(_,when)|self.path.overlaps(when)) {
            let source=match source {
                view::Source::Parameter(ValueIdentity::Register(input),_)=>TaskCompletionSource::Input(*input),
                view::Source::TaskOutput{projection:view::TaskProjection::Returned(component,
                    Some(ValueIdentity::TaskCompletion(parent,_))),..}=>TaskCompletionSource::Returned(*parent,*component),
                _=>return None,
            };
            destinations.insert(source,self.task_completion(source,result)?);
        }
        let (last,mut destination)=destinations.pop_last()?;
        if destinations.is_empty() {
            return Some((destination,Some(destination[0])));
        }
        if !matches!(last,TaskCompletionSource::Input(_)) {return None;}
        for (source,address) in destinations {
            let TaskCompletionSource::Input(candidate)=source else {return None;};
            let selected=self.reg(Scalar::Bool);
            self.emit(InstKind::Binary {
                dst:selected,op:BinaryOp::Compare(Comparison::Equal),a:task,b:candidate,
            });
            destination=std::array::from_fn(|part| {
                let dst=self.reg(Scalar::U32);
                self.emit(InstKind::Select{dst,condition:selected,a:address[part],b:destination[part]});
                dst
            });
        }
        Some((destination,None))
    }
    fn task_completion_layout(&self,result:&Ty,span:Span)->Result<Ty> {
        result.task_completion().ok_or_else(||Diagnostic{
            source:self.scope.source().clone(),span,code:"B224",message:"任务完成布局过大".into(),
        })
    }
    #[inline(never)]
    async fn new(
        world: &'a World,
        instance: &crate::generics::Instance,
        program: &'a Program,
        domain: usize,
        source_instance: Option<&str>,
    ) -> Result<Self> {
        let scope = LexicalScope::Function(instance.clone());
        let definition = &instance.definition;
        let mut mutable_names = BTreeSet::new();
        unsafe { world.call(assigned_names(
            world,
            &scope,
            definition.function.body().as_ref().unwrap(),
            &mut mutable_names,
        )).await }?;
        let (source_definition, source_context) = definition.syntax_context(source_instance);
        Ok(Self::from_checked(world,&scope,program,domain,source_definition,source_context,mutable_names))
    }
    async fn domain(world:&'a World,input:DomainInput<'_>,program:&'a Program,domain_index:usize)->Result<Self> {
        match input {
            DomainInput::Declaration { instance, .. }=>unsafe { world.call(Self::new(world,instance,program,domain_index,Some(&instance.name))).await },
            DomainInput::Parallel{owner,parameters,statement,element,domain,body,site,..}=>{
                let scope=LexicalScope::Parallel{owner:owner.clone(),parameters:(*parameters).clone(),statement:statement.clone(),site:(*site).clone()};
                let mut mutable_names=BTreeSet::new();
                unsafe { world.call(effects::assigned_parallel(world,&scope,element,domain,body,&mut mutable_names)).await }?;
                let(origin,context)=input.syntax_context();
                Ok(Self::from_checked(world,&scope,program,domain_index,origin,context,mutable_names))
            }
        }
    }
    fn expression(
        world: &'a World, program: &'a Program, source: &Source, module: &str, imports: &[String],
        bindings: Arc<crate::generics::Bindings>, span: Span,
    ) -> Self {
        let scope = LexicalScope::Values {
            source: source.clone(), module: module.into(), imports: imports.to_vec(), bindings, span,
            parameters:Arc::from([]),source_definition:None,
            source_context:Arc::new(SourceContext{instance:None,inlined_at:Box::default(),domain_path:Arc::from([])}),
        };
        let (source_definition, source_context) = scope.syntax_context(None);
        Self::from_checked(world, &scope, program, 0, source_definition, source_context, BTreeSet::new())
    }
    fn from_checked(
        world: &'a World,
        scope: &LexicalScope,
        program: &'a Program,
        domain: usize,
        source_definition: Option<Arc<SourceOrigin>>,
        source_context: Arc<SourceContext>,
        mutable_names: BTreeSet<String>,
    )->Self {
        Self {
            residency: residency::Inputs::default(),
            view_model: view_contracts::Function::default(),
            application: None,
            pending_error: None,
            task_completions: BTreeMap::new(),
            task_return_source: None,
            functions: native_function::Module::default(),
            world,
            scope: scope.clone(),
            program,
            element_facts: None,
            prefix_facts: None,
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
            output_scopes: scope.parameters().count(),
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
        }
    }
    fn application_entry(&self) -> bool {
        self.application.as_ref().is_some_and(|context| !context.range)
    }
    fn failure_scope(&self,consumed:Vec<Place>)->Option<Box<FailureScope>> {
        let owns=self.env.values().any(|binding|match binding {
            Binding::ApplicationData(data)=>data.ownership.may_own() || data.owns_write_elements()
                || data.returns_loan_on_error(),
            Binding::OwnedValue(value)=>value.ownership.may_own(),
            _=>false,
        });
        if !owns && !consumed.iter().any(|place| self.borrowed_element_storage(place)) {
            return None;
        }
        Some(Box::new(FailureScope {
            env:self.env.clone(),path:self.path.clone(),initialized:self.initialized.clone(),
            scope:self.scope.clone(),source_definition:self.source_definition.clone(),
            source_context:self.source_context.clone(),source_site:self.source_site.clone(),
            mutable_names:self.mutable_names.clone(),frame:self.application.as_ref().unwrap().frame.clone(),
            domain:self.domain,control_uniform:self.control_uniform,calls:self.calls.clone(),consumed,
            task_outputs:Vec::new(),
        }))
    }
    fn failure_continuation(&self,consumed:Vec<Place>)->Option<FailureContinuation> {
        Some(if let Some(error)=self.pending_error {FailureContinuation::Cleanup(error.slot())}
        else {match self.failure_scope(consumed) {
            Some(scope)=>FailureContinuation::Scope(scope),None=>FailureContinuation::Return,
        }})
    }
    async fn cleanup_group(&mut self,consumed:Vec<Place>,work:impl std::ops::AsyncFnOnce(&mut Self)->Result<()>)->Result<()> {
        if self.pending_error.is_some()||self.application.is_none(){return work(self).await;}
        let slot=self.local_slots(&Ty::scalar(Scalar::U64)).0[0];
        let outer=std::mem::take(&mut self.code);
        self.pending_error=Some(PendingError::Group(slot));
        let result=work(self).await;
        let body=std::mem::replace(&mut self.code,outer);
        self.pending_error=None;
        result?;
        let failure=self.failure_scope(consumed);
        // On the normal edge the original group status stayed zero throughout.
        let mut statuses=Vec::new();
        crate::optimize::visit(&body,&mut |inst|match &inst.kind {
            InstKind::LoadLocal(value,source) if *source==slot => statuses.push(*value),
            InstKind::Call(call) if matches!(&call.failure,Some(FailureContinuation::Cleanup(target))if *target==slot) => {
                statuses.extend(call.error);
            }
            _=>{}
        });
        self.path=bounds::Clause::both(&self.path,&bounds::Clause::zero_results(statuses));
        self.emit(InstKind::Cleanup{slot,body,failure});
        Ok(())
    }
    fn emit(&mut self, kind: InstKind) {
        self.code.push(Inst { source_site: self.source_site.clone(), ..Inst::new(kind) });
    }
    fn source_at(&self, span: Span, syntax: SourceSyntax) -> Option<Arc<SourceSite>> {
        self.source_definition.as_ref().map(|origin| Arc::new(SourceSite {
            origin: origin.clone(), context: self.source_context.clone(), span, syntax,
        }))
    }
    async fn at_source<T>(&mut self, span: Span, syntax: SourceSyntax, work: impl std::ops::AsyncFnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let site = self.source_at(span, syntax);
        let previous = std::mem::replace(&mut self.source_site, site);
        let result = work(self).await;
        self.source_site = previous;
        result
    }
    fn data_access(&mut self, place: &Place, access: Access, output: Option<&bounds::Proof>) {
        let Place::Element { index, .. } = place.storage() else { return; };
        let cross = *index != self.global_index;
        for (component, ((scalar, _), position)) in place.ty().components.iter().zip(place.data_positions()).enumerate() {
            let mut pending = vec![&position];
            while let Some(position) = pending.pop() {
                match position {
                    Position::Element(binding, word) => {
                        let (reads, writes) = self.memory_regions.entry(*binding).or_default();
                        if access.reads() { reads.insert((cross, *word, *word + scalar.size())); }
                        if access.writes() {
                            let stored = output.map(|proof| Box::new(proof.project(&[component], place.ty().components.len())));
                            writes.entry((*word, *word + scalar.size()))
                                .and_modify(|value| match (value.as_deref_mut(), stored.as_deref()) {
                                    (Some(value), Some(stored)) => value.join(stored, 1, None),
                                    _ => *value = None,
                                }).or_insert(stored);
                        }
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
            let mut writes = writes.keys().filter(|(start, end)| start < end).peekable();
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
                Root::Uniform(i) => {
                    let program = self.program;
                    let uniform = &program.uniforms[*i];
                    let mut regs = Vec::new();
                    for (s, o) in &uniform.ty.components {
                        let r = self.reg(*s);
                        self.emit(InstKind::Uniform(r, uniform.offset + o));
                        regs.push(r);
                    }
                    self.env.insert(
                        name.clone(),
                        Binding::Value(Value {
                            proof: bounds::Proof::default(),
                            literal: false,
                            ty: uniform.ty.clone(),
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
        self.scope.source().fail(span, code, message)
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
        operators::same(a, b, self.scope.source(), span)
    }
    fn require_bool(&self, v: &Value, span: Span) -> Result<()> {
        self.require_same(&Ty::scalar(Scalar::Bool), &v.ty, span)
    }
    async fn block(&mut self,body:&Block)->Result<()> {
        let mut cursor=unsafe {lexical_start()};let mut flow=Flow::NEXT;let mut original=None;let mut current=None;
        loop {let step=unsafe {lexical_block_step(cursor,body.len()as u64,u32::from(flow.continues()))};cursor=step.cursor;match step.need {
            LexicalNeed::Enter=>{original=Some(self.env.keys().cloned().collect::<BTreeSet<_>>());self.block_depth+=1;},
            LexicalNeed::Statement=>{let statement=body.at(cursor.index as usize);self.flow=Flow::NEXT;unsafe {self.world.call(self.statement(&statement)).await}?;current=Some(statement);},
            LexicalNeed::After=>{let statement=current.take().unwrap();self.view_uses.after(statement.span());flow=flow.then(self.flow);},
            LexicalNeed::Finish=>{self.flow=flow;self.block_depth-=1;},
            LexicalNeed::Release=>{unsafe {self.world.call(self.release_application_data(original.as_ref(),view::Borrow::default())).await}?;},
            LexicalNeed::Retain=>self.env.retain(|name,_|original.as_ref().unwrap().contains(name)),
            LexicalNeed::Complete=>return Ok(()),
            LexicalNeed::Next=>{},
        }}
    }
    async fn statement(&mut self, s: &Stmt) -> Result<()> {
        let world = self.world;
        unsafe { world.call(self.at_source(s.span(), SourceSyntax::Statement,
            async |b| unsafe { world.call(b.temporary_scope(s.span(), async |b| {
                unsafe { world.call(b.statement_syntax(s)) .await }?;
                Ok(((),view::Borrow::default()))
            })) .await })) .await }
    }
    async fn statement_syntax(&mut self,s:&Stmt)->Result<()> {
        self.view_uses.before(s.span());let source=s.source_view();let mut cursor=SourceValueCursor {phase:0};
        let mut facts=SourceStatementFacts {present:0,entry:0};let mut annotation=None;let mut condition=None;let mut initial=None;
        loop {let step=unsafe {source_statement_step(cursor,s.signature_node(),SourceStatementFacts {..facts})};cursor=step.cursor;match step.need {
            SourceStatementNeed::Bind=>{let ty=s.child::<crate::syntax::wire::TypeNode>(step.first);annotation=Some(unsafe {self.world.call(self.scope.bindings().type_name(self.world,&ty,(self.scope.source(),self.scope.module(),self.scope.imports()),s.span())).await}?);},
            SourceStatementNeed::Declare=>return unsafe {self.world.call(self.declare_statement(annotation.as_ref().unwrap(),source.text(step.name),s.span())).await},
            SourceStatementNeed::Let=>{let value=s.child(step.second);return unsafe {self.world.call(self.let_statement(&annotation,source.text(step.name),&value,s.span())).await};},
            SourceStatementNeed::Assign=>{let left=s.child(step.first);let right=s.child(step.second);return unsafe {self.world.call(self.assign_statement(&left,source.text(step.operation),&right,s.span())).await};},
            SourceStatementNeed::Expression=>{let value=s.child(step.first);return unsafe {self.world.call(self.expression_statement(&value)).await};},
            SourceStatementNeed::Condition=>{let expr:Expr=s.child(step.first);condition=unsafe {self.world.call(self.temporary_scope(expr.span(),async |builder|Ok((unsafe {builder.world.call(builder.expr(&expr)).await}?,view::Borrow::default())))).await}?;facts.present=u32::from(condition.is_some());},
            SourceStatementNeed::RequireBool=>{let expr:Expr=s.child(step.first);self.require_bool(condition.as_ref().unwrap(),expr.span())?;},
            SourceStatementNeed::Conditional=>{let yes=s.list::<crate::syntax::wire::StmtNode>(step.body);let no=s.list::<crate::syntax::wire::StmtNode>(step.other);return unsafe {self.world.call(self.conditional_blocks(condition.take().unwrap(),async |builder|unsafe {builder.world.call(builder.block(&yes)).await},async |builder|unsafe {builder.world.call(builder.block(&no)).await},s.span())).await};},
            SourceStatementNeed::InitialLoop=>{let body=s.list::<crate::syntax::wire::StmtNode>(step.body);initial=Some(unsafe {self.world.call(self.application_initial_loop(&body)).await}?);},
            SourceStatementNeed::While=>{let expr=s.child(step.first);let body=s.list::<crate::syntax::wire::StmtNode>(step.body);return unsafe {self.world.call(self.while_loop(&expr,&body,initial.take().unwrap(),s.span())).await};},
            SourceStatementNeed::For=>{let start=s.child(step.first);let end=(step.present!=0).then(||s.child::<crate::syntax::wire::ExprNode>(step.second));let body=s.list::<crate::syntax::wire::StmtNode>(step.body);return unsafe {self.world.call(self.for_statement(source.text(step.name),&start,&end,&body,s.span())).await};},
            SourceStatementNeed::LoopControl=>return unsafe {self.world.call(self.loop_control_statement(s)).await},
            SourceStatementNeed::Match=>{let value=s.child(step.first);let arms=s.list::<crate::syntax::wire::ArmNode>(step.body);return unsafe {self.world.call(self.match_returned(&value,&arms,s.span())).await};},
            SourceStatementNeed::Return=>{let value=(step.present!=0).then(||s.child::<crate::syntax::wire::ExprNode>(step.first));return unsafe {self.world.call(self.return_statement(value.as_ref(),s.span())).await};},
            SourceStatementNeed::Parallel=>{let domain=s.child::<crate::syntax::wire::ParallelNode>(step.first);let body=s.list::<crate::syntax::wire::StmtNode>(step.body);return unsafe {self.world.call(self.application_parallel(s,source.text(step.name),&domain,&body)).await};},
            SourceStatementNeed::Entry=>facts.entry=u32::from(self.application_entry()),
            SourceStatementNeed::BadPhase=>return self.fail(s.span(),"B163","phase 需要整域同步位置，不能位于元素实例中"),
            SourceStatementNeed::Seek=>{let position=s.child(step.first);return unsafe {self.world.call(self.seek_statement(source.text(step.name),&position,s.span())).await};},
            SourceStatementNeed::Complete=>return Ok(()),
        }}
    }
    fn value_result(&self, result: Option<Value>, span: Span) -> Result<Value> {
        match result {
            Some(value) => Ok(value),
            None => self.fail(span, "B228", "此处需要普通值，void 调用没有返回值"),
        }
    }
    async fn place(&mut self, e: &Expr) -> Result<Option<Place>> {
        unsafe { self.world.call(self.at_source(e.span(), SourceSyntax::Place, async |b| unsafe { b.world.call(b.place_syntax(e)).await })).await }
    }
    async fn place_syntax(&mut self, e: &Expr) -> Result<Option<Place>> {
        self.require_user_projection(e)?;self.view_uses.before(e.span());
        let mut cursor=SourceValueCursor {phase:0};let mut present=0;let mut explicit=None;let mut place=None;
        loop {let step=unsafe {source_place_step(cursor,e.signature_node(),present)};cursor=step.cursor;match step.need {
            SourcePlaceNeed::Name=>return self.name_place(e.source_view().text(step.spelling),e.span()).map(Some),
            SourcePlaceNeed::Bind=>{let types=e.list(step.types);explicit=Some(unsafe {self.world.call(self.scope.bindings().arguments(self.world,&types,(self.scope.source(),self.scope.module(),self.scope.imports()),e.span())).await}?);},
            SourcePlaceNeed::External=>{let args=e.list(step.arguments);return unsafe {self.world.call(self.external_position(explicit.as_ref().unwrap(),&args,e.span())).await};},
            SourcePlaceNeed::Parent=>{let base=e.child(step.first);place=unsafe {self.world.call(self.place(&base)).await}?;present=u32::from(place.is_some());},
            SourcePlaceNeed::Project=>return self.member_position(place.take().unwrap(),e.source_view().text(step.spelling),e.span()),
            SourcePlaceNeed::Index=>{let base=e.child(step.first);let index=e.child(step.second);return unsafe {self.world.call(self.index_place(&base,&index)).await};},
            SourcePlaceNeed::Bad=>return self.fail(e.span(),"B138","表达式不是可赋值位置"),
            SourcePlaceNeed::Empty=>return Ok(None),
        }}
    }
    async fn read_place(&mut self,p:&Place,span:Span,mut element_read_origin:Option<Arc<GeneratedOrigin>>)->Result<Value> {
        let mut cursor=unsafe {lexical_start()};let mut facts=ReadPlaceFacts {owns:0,kind:0,access:0,reference:0,memory:0,known:0,input:0,current:0,field_address:0,components:0,words:0,fields:0};
        let mut memory=None;let mut content_read=None;let mut snapshots=None;let mut shared=None;let mut fields=Vec::new();let mut zero=None;let mut regs=Vec::new();let mut reads=Vec::new();let mut proof=None;let mut uniform=false;let mut value=None;
        loop {let step=unsafe {read_place_step(cursor,ReadPlaceFacts {..facts})};cursor=step.cursor;match step.need {
            ReadPlaceNeed::Access=>{unsafe {self.world.call(self.require_place_access(p,Access::Read,span)).await}?;},
            ReadPlaceNeed::Owns=>facts.owns=u32::from(p.ty().owns_resources()),
            ReadPlaceNeed::Kind=>{facts.components=p.ty().components.len()as u64;match p {
                Place::Reference {address,words,access,..}=>{facts.kind=0;facts.access=*access as u32;facts.reference=match address{ReferenceAddress::Borrowed {..}=>1,ReferenceAddress::External {..}=>2,_=>0};facts.words=words.len()as u64;},
                Place::ApplicationElement {words,access,..}=>{facts.kind=1;facts.access=*access as u32;facts.words=words.len()as u64;},
                Place::Indexed {..}=>facts.kind=2,
                Place::ViewElement {words,..}=>{facts.kind=3;facts.words=words.len()as u64;},
                Place::TileElement {tile,words,..}=>{facts.kind=4;facts.access=self.tiles[*tile].access as u32;facts.input=u32::from(matches!(self.tiles[*tile].source,TileOrigin::Input {..}));facts.words=words.len()as u64;},
                Place::Local(_,slots,_)=>{facts.kind=5;facts.words=slots.len()as u64;},
                Place::ReadOnlyLocal(_,slots,_)=>{facts.kind=6;facts.words=slots.len()as u64;},
                Place::Element {binding,index,words,..}=>{facts.kind=7;facts.access=self.view_access[binding] as u32;facts.current=u32::from(*index==self.global_index);facts.words=words.len()as u64;},
                Place::AtomicParameter(_)=>facts.kind=8,
            }},
            ReadPlaceNeed::OwnedError=>return self.fail(span,"B224","拥有资源需要移动、字段借用或match交接，不能按普通值复制"),
            ReadPlaceNeed::AtomicError=>return self.fail(span,"B214","原子借用只能通过显式原子操作读取"),
            ReadPlaceNeed::AtomicTileError=>return self.fail(span,"B214","atomic tile 元素只能通过显式原子操作读取"),
            ReadPlaceNeed::AtomicDataError=>return self.fail(span,"B214","atomic 数据集元素只能通过显式原子操作读取"),
            ReadPlaceNeed::Array=>return unsafe {self.world.call(self.read_array_place(p,span)).await},
            ReadPlaceNeed::ReferenceMemory=>{memory=unsafe {self.world.call(self.reference_memory(p,span)).await}?;facts.memory=u32::from(memory.is_some());},
            ReadPlaceNeed::Address=>{let(_,borrow)=memory.as_ref().unwrap();self.check_view_requirement(view::Requirement::Address {borrow:borrow.clone(),layout:address::Layout::of(p.ty()),written:None,writable:false,initialized:true,span})?;},
            ReadPlaceNeed::RememberRead=>content_read=Some((self.view_model.uses.len(),memory.as_ref().unwrap().1.clone())),
            ReadPlaceNeed::Memory=>{let(target,borrow)=memory.take().unwrap();self.check_view_requirement(view::Requirement::Memory {target,borrow,access:Some(Access::Read),written:None,snapshots:None,span,address_fields:Arc::from([])})?;},
            ReadPlaceNeed::Initialize=>{unsafe {self.world.call(self.require_initialized(p,span)).await}?;},
            ReadPlaceNeed::Zero=>zero=Some(self.constant(Scalar::U32,0).regs[0]),
            ReadPlaceNeed::LoadReference=>{let Place::Reference {ty,address,words,..}=p else{unsafe {std::hint::unreachable_unchecked()}};let i=cursor.index as usize;let(source,word)=address.source_at(words[i],zero.unwrap());let dst=self.reg(ty.components[i].0);self.emit(InstKind::LoadIndexed {dst,source,offset:zero.unwrap(),word});regs.push(dst);},
            ReadPlaceNeed::LoadView=>{let Place::ViewElement {ty,address,index,stride,words,..}=p else{unsafe {std::hint::unreachable_unchecked()}};let i=cursor.index as usize;let dst=self.reg(ty.components[i].0);self.emit(InstKind::ViewLoad {dst,address:*address,index:*index,stride:*stride,word:words[i]});regs.push(dst);},
            ReadPlaceNeed::LoadTile=>{let Place::TileElement {ty,tile,index,words}=p else{unsafe {std::hint::unreachable_unchecked()}};let i=cursor.index as usize;let dst=self.reg(ty.components[i].0);self.emit(InstKind::TileLoad {dst,tile:*tile,index:*index,word:words[i]});regs.push(dst);},
            ReadPlaceNeed::LoadLocal=>{let(Place::Local(ty,slots,_)|Place::ReadOnlyLocal(ty,slots,_))=p else{unsafe {std::hint::unreachable_unchecked()}};let i=cursor.index as usize;let dst=self.reg(ty.components[i].0);self.emit(InstKind::LoadLocal(dst,slots[i]));regs.push(dst);},
            ReadPlaceNeed::LoadData=>{let Place::Element {ty,binding,index,words,..}=p else{unsafe {std::hint::unreachable_unchecked()}};let i=cursor.index as usize;let dst=self.reg(ty.components[i].0);self.emit(InstKind::Load {dst,binding:*binding,index:*index,word:words[i]});regs.push(dst);},
            ReadPlaceNeed::Proof=>{let(actual,is_uniform,known)=unsafe {self.world.call(self.read_owned_proof(p,&regs,&[])).await}?;proof=Some(actual);uniform=is_uniform;facts.known=u32::from(known);},
            ReadPlaceNeed::Snapshot=>{let(memory,borrow)=content_read.take().unwrap();let captured:Arc<[view_contracts::ValueSnapshot]>=self.capture_address_values(memory,address::Capture::Read).into();if let view::Requirement::Memory {snapshots,..}=&mut self.view_model.uses[memory].requirement{*snapshots=Some(captured.clone());}snapshots=Some((memory,borrow,captured));},
            ReadPlaceNeed::ShareBorrow=>{let(memory,borrow,captured)=snapshots.take().unwrap();shared=Some((memory,Arc::new(borrow),captured));},
            ReadPlaceNeed::BorrowFields=>{fields=p.ty().borrow_fields();facts.fields=fields.len()as u64;},
            ReadPlaceNeed::BorrowField=>facts.field_address=u32::from(fields[cursor.part as usize].1.is_address()),
            ReadPlaceNeed::SetBorrow=>{let word=fields[cursor.part as usize].0;let component=p.ty().components.iter().position(|(_,at)|*at==word).unwrap();let(memory,borrow,captured)=shared.as_ref().unwrap();proof.as_mut().unwrap().set_view_borrow(component,view::Borrow {reads:view::Edges::from([view_contracts::ContentRead::Value {memory:*memory,word,borrow:borrow.clone(),snapshots:captured.clone()}]),..view::Borrow::default()});},
            ReadPlaceNeed::ExternalProof=>proof=Some(proof.take().unwrap().with_external_borrows(p.ty())),
            ReadPlaceNeed::DatasetEffect=>reads=self.dataset_effect(p,Access::Read,false,span)?,
            ReadPlaceNeed::ApplicationRead=>{let Place::ApplicationElement {ty,handle,identity,index,stride,words,..}=p else{unsafe {std::hint::unreachable_unchecked()}};value=Some(self.read_application_place(ty,*handle,*identity,*index,*stride,words,span,element_read_origin.take())?);},
            ReadPlaceNeed::ApplicationProof=>{let(actual,is_uniform,_)=unsafe {self.world.call(self.read_owned_proof(p,&value.as_ref().unwrap().regs,&reads)).await}?;proof=Some(actual);uniform=is_uniform;},
            ReadPlaceNeed::TileInput=>{let Place::TileElement {tile,..}=p else{unsafe {std::hint::unreachable_unchecked()}};let TileOrigin::Input {binding,..}=&self.tiles[*tile].source else{unsafe {std::hint::unreachable_unchecked()}};self.reads.insert(*binding);self.cross_access.insert(*binding);},
            ReadPlaceNeed::DataAccess=>{let Place::Element {binding,..}=p else{unsafe {std::hint::unreachable_unchecked()}};self.data_access(p,Access::Read,None);self.reads.insert(*binding);},
            ReadPlaceNeed::DataCross=>{let Place::Element {binding,..}=p else{unsafe {std::hint::unreachable_unchecked()}};self.cross_access.insert(*binding);},
            ReadPlaceNeed::FinishApplication=>{let mut value=value.take().unwrap();let proof=proof.take().unwrap();value.constant=proof.known_value(p.ty());value.proof=proof;value.uniform=uniform;return Ok(value);},
            ReadPlaceNeed::Finish=>{let proof=proof.take().unwrap();let constant=if step.flags&1!=0{proof.known_value(p.ty())}else{None};return Ok(Value {proof,literal:false,ty:p.ty().clone(),regs,uniform:step.flags&2!=0&&uniform,constant});},
        }}
    }
    async fn write_place(&mut self,p:&Place,value:Value,span:Span,local_origin:Option<Arc<GeneratedOrigin>>)->Result<()> {
        let mut cursor=unsafe {lexical_start()};let mut facts=WritePlaceFacts {views:0,persistent:0,kind:0,access:0,writable:0,memory:0,pending:0,resolved:0,field_address:0,unique:0,iterator:0,loops_empty:0,words:0,values:0,fields:0};
        let mut value=Some(value);let mut written=None;let mut memory=None;let mut snapshots=None;let mut fields=Vec::new();let mut address_fields=Vec::new();let mut unique=BTreeSet::new();let mut proof=None;let mut zero=None;
        loop {let step=unsafe {write_place_step(cursor,WritePlaceFacts {..facts})};cursor=step.cursor;match step.need {
            WritePlaceNeed::Access=>{unsafe {self.world.call(self.require_place_access(p,Access::Write,span)).await}?;},
            WritePlaceNeed::ViewType=>{facts.views=u32::from(p.ty().contains_views());facts.persistent=u32::from(matches!(p.storage(),Place::Element {..}|Place::ApplicationElement {..}));},
            WritePlaceNeed::ViewPayloadError=>return self.fail(span,"B185","持久dataset的view载荷不可写或独占借出；数值字段按原权限修改"),
            WritePlaceNeed::Owned=>self.require_owned_writable(p,span)?,
            WritePlaceNeed::ViewAvailable=>self.require_view_available(value.as_ref().unwrap(),&[],span)?,
            WritePlaceNeed::Same=>self.require_same(p.ty(),&value.as_ref().unwrap().ty,span)?,
            WritePlaceNeed::Kind=>{facts.values=value.as_ref().unwrap().regs.len()as u64;match p {
                Place::Reference {words,access,..}=>{facts.kind=0;facts.access=*access as u32;facts.words=words.len()as u64;},
                Place::ApplicationElement {words,access,..}=>{facts.kind=1;facts.access=*access as u32;facts.words=words.len()as u64;},
                Place::Indexed {..}=>facts.kind=2,
                Place::ViewElement {..}=>facts.kind=3,
                Place::TileElement {tile,..}=>{facts.kind=4;facts.access=self.tiles[*tile].access as u32;},
                Place::Local(_,slots,_)=>{facts.kind=5;facts.words=slots.len()as u64;},
                Place::ReadOnlyLocal(..)=>facts.kind=6,
                Place::Element {binding,words,writable,..}=>{facts.kind=7;facts.access=self.view_access[binding] as u32;facts.words=words.len()as u64;facts.writable=u32::from(*writable);},
                Place::AtomicParameter(_)=>facts.kind=8,
            }},
            WritePlaceNeed::NumericProof=>written=Some(self.path_proof(p.ty(),bounds::Proof::capture(value.as_ref().unwrap())).numeric_facts()),
            WritePlaceNeed::ReferenceMemory=>{memory=unsafe {self.world.call(self.reference_memory(p,span)).await}?;facts.memory=u32::from(memory.is_some());},
            WritePlaceNeed::Pending=>facts.pending=u32::from(memory.as_ref().unwrap().1.pending()),
            WritePlaceNeed::Resolved=>facts.resolved=u32::from(self.resolved_address_write(&memory.as_ref().unwrap().1)),
            WritePlaceNeed::Snapshot=>snapshots=Some(self.capture_address_values(self.view_model.uses.len(),address::Capture::Write).into()),
            WritePlaceNeed::AddressWrite=>{let(_,borrow)=memory.as_ref().unwrap();unsafe {self.world.call(self.write_address_values(borrow,&[(borrow.clone(),Some(Access::Write),written.clone())])).await}?;},
            WritePlaceNeed::BorrowFields=>{fields=p.ty().borrow_fields();facts.fields=fields.len()as u64;},
            WritePlaceNeed::BorrowField=>facts.field_address=u32::from(fields[cursor.part as usize].1.is_address()),
            WritePlaceNeed::AddField=>{let word=fields[cursor.part as usize].0;address_fields.push((word,p.ty().components.iter().position(|(_,at)|*at==word).unwrap()));},
            WritePlaceNeed::Memory=>{let(target,borrow)=memory.take().unwrap();self.check_view_requirement(view::Requirement::Memory {target,borrow,access:Some(Access::Write),written:Some(bounds::Proof::capture(value.as_ref().unwrap())),snapshots:snapshots.take(),span,address_fields:std::mem::take(&mut address_fields).into()})?;},
            WritePlaceNeed::AtomicError=>return self.fail(span,"B214","原子借用只能通过显式原子操作修改"),
            WritePlaceNeed::AtomicTileError=>return self.fail(span,"B214","atomic tile 元素只能通过显式原子操作修改"),
            WritePlaceNeed::AtomicDataError=>return self.fail(span,"B214","atomic 数据集元素只能通过显式原子操作修改"),
            WritePlaceNeed::Array=>return unsafe {self.world.call(self.write_array_place(p,value.take().unwrap(),span)).await},
            WritePlaceNeed::ReadOnlyError=>return self.fail(span,"B144","按值参数或不可变局部值不可写"),
            WritePlaceNeed::ViewError=>return self.fail(span,"B185","view 的元素不可写"),
            WritePlaceNeed::TileError=>return self.fail(span,"B202","tile 值不可写；只修改当前独占的全局元素"),
            WritePlaceNeed::WriteError=>{let Place::Element {binding,..}=p else{unsafe {std::hint::unreachable_unchecked()}};return self.fail(span,"B144",format!("没有 {} 当前元素的独占修改权",self.program.buffers[*binding].name));},
            WritePlaceNeed::Unique=>{let i=cursor.part as usize;let position=match p {Place::Local(_,slots,_)=>slots[i],Place::Element {words,..}=>words[i]as usize,_=>unsafe {std::hint::unreachable_unchecked()}};facts.unique=u32::from(unique.insert(position));},
            WritePlaceNeed::DuplicateError=>return self.fail(span,"B143","赋值位置包含重复分量"),
            WritePlaceNeed::Writable=>{unsafe {self.world.call(self.writable_storage(p,span,written.take())).await}?;},
            WritePlaceNeed::AssignProof=>{unsafe {self.world.call(self.assign_owned_proof(p,bounds::Proof::capture(value.as_ref().unwrap()),false)).await}?;},
            WritePlaceNeed::Initialize=>self.initialize(p),
            WritePlaceNeed::Zero=>zero=Some(self.constant(Scalar::U32,0).regs[0]),
            WritePlaceNeed::DatasetEffect=>{self.dataset_effect(p,Access::Write,false,span)?;},
            WritePlaceNeed::CaptureProof=>{proof=Some(bounds::Proof::capture(value.as_ref().unwrap()));facts.iterator=u32::from(self.iterators.last().is_some_and(Option::is_some));},
            WritePlaceNeed::Alias=>self.path=bounds::Clause::both(&self.path,&proof.as_ref().unwrap().output_aliases(&value.as_ref().unwrap().regs,&self.registers,&BTreeMap::new())),
            WritePlaceNeed::ApplicationWrite=>{let Place::ApplicationElement {handle,identity,index,stride,words,..}=p else{unsafe {std::hint::unreachable_unchecked()}};self.write_application_place(*handle,*identity,*index,*stride,words,value.take().unwrap(),span,None)?;},
            WritePlaceNeed::AssignApplicationProof=>{unsafe {self.world.call(self.assign_owned_proof(p,proof.take().unwrap(),false)).await}?;facts.loops_empty=u32::from(self.loop_contexts.is_empty());},
            WritePlaceNeed::CompleteWrites=>{let Place::ApplicationElement {sources,..}=p else{unsafe {std::hint::unreachable_unchecked()}};unsafe {self.world.call(self.complete_application_writes(|_,data|sources.contains(&data.root.identity()),span)).await}?;},
            WritePlaceNeed::DataAccess=>{let Place::Element {binding,..}=p else{unsafe {std::hint::unreachable_unchecked()}};self.data_access(p,Access::Write,None);self.writes.insert(*binding);},
            WritePlaceNeed::StoreReference=>{let Place::Reference {address,words,..}=p else{unsafe {std::hint::unreachable_unchecked()}};let i=cursor.index as usize;let(source,word)=address.source_at(words[i],zero.unwrap());self.emit(InstKind::StoreIndexed {source,offset:zero.unwrap(),word,value:value.as_ref().unwrap().regs[i]});},
            WritePlaceNeed::StoreLocal=>{let Place::Local(_,slots,_)=p else{unsafe {std::hint::unreachable_unchecked()}};let i=cursor.index as usize;let kind=InstKind::StoreLocal(slots[i],value.as_ref().unwrap().regs[i]);if let Some(origin)=&local_origin{self.code.push(Inst::generated(kind,origin.clone()));}else{self.emit(kind);}},
            WritePlaceNeed::StoreData=>{let Place::Element {binding,index,words,..}=p else{unsafe {std::hint::unreachable_unchecked()}};let i=cursor.index as usize;self.emit(InstKind::Store {binding:*binding,index:*index,word:words[i],value:value.as_ref().unwrap().regs[i]});},
            WritePlaceNeed::Complete=>return Ok(()),
        }}
    }
    async fn expr(&mut self, e: &Expr) -> Result<Option<Value>> {
        let world = self.world;
        let mut value = unsafe { world.call(self.at_source(e.span(), SourceSyntax::Expression,
            async |b| unsafe { world.call(b.expr_syntax(e)) .await })) .await }?;
        if let Some(value) = &mut value { self.bound_value(value); }
        Ok(value)
    }
    async fn expr_syntax(&mut self, e: &Expr) -> Result<Option<Value>> {
        let world=self.world;let source=e.source_view();self.view_uses.before(e.span());
        let mut cursor=SourceValueCursor {phase:0};let mut present=0;
        let mut first:Option<Box<Value>>=None;let mut second=None;let mut ty=None;let mut returned=None;
        loop {let step=unsafe {source_value_step(cursor,e.signature_node(),present)};cursor=step.cursor;match step.need {
            SourceValueNeed::BadFunction=>return self.fail(e.span(),"B236","静态函数引用只能作为集合算法参数或 bind 的首参数，不能作为普通值"),
            SourceValueNeed::BadCooperate=>return self.fail(e.span(),"B208","cooperate 调用只能作为协作根块的局部声明"),
            SourceValueNeed::StaticU32=>return self.constant_expression(Scalar::U32,Bits::from(step.bits)),
            SourceValueNeed::Bool=>return self.constant_expression(Scalar::Bool,Bits::from(step.bits)),
            SourceValueNeed::Number=>return self.number_expression(source.text(step.spelling),e.span()),
            SourceValueNeed::Name=>return unsafe {world.call(self.name_expr(source.text(step.spelling),e.span())).await}.map(Some),
            SourceValueNeed::Specialized=>return unsafe {world.call(source_call::specialized(self,e)).await},
            SourceValueNeed::Member=>{let base=e.child(step.first);returned=unsafe {world.call(self.member_returned(e,&base,source.text(step.spelling))).await}?;},
            SourceValueNeed::Index=>{let base=e.child(step.first);let index=e.child(step.second);return unsafe {world.call(self.index_expr(&base,&index,e)).await};},
            SourceValueNeed::Select=>{let condition=e.child(step.first);let yes=e.child(step.second);let no=e.child(step.third);return unsafe {world.call(self.selection(&condition,&yes,&no,e.span())).await};},
            SourceValueNeed::Target=>{let call=e.child(step.first);returned=unsafe {world.call(self.target_call(step.target,&call,e.span())).await}?;},
            SourceValueNeed::ConstructType=>{let name=e.child(step.first);ty=Some(unsafe {world.call(self.scope.bindings().type_name(world,&name,(self.scope.source(),self.scope.module(),self.scope.imports()),e.span())).await}?);},
            SourceValueNeed::Construct=>{let args=e.list(step.arguments);return unsafe {world.call(self.record_expression(ty.as_ref().unwrap(),&args,e.span())).await};},
            SourceValueNeed::Returned=>returned=unsafe {world.call(self.evaluate_returned(e,None)).await}?,
            SourceValueNeed::FinishReturned=>return self.returned_expression(returned.take(),e.span(),false),
            SourceValueNeed::Negative=>return self.negative_number(source.text(step.spelling),e.span()).map(Some),
            SourceValueNeed::Operand|SourceValueNeed::Left=>{let operand=e.child(step.first);first=unsafe {world.call(self.expr(&operand)).await}?.map(Box::new);present=u32::from(first.is_some());},
            SourceValueNeed::Unary=>return self.unary_value(source.text(step.spelling),*first.take().unwrap(),e.span()).map(Some),
            SourceValueNeed::Right=>{let operand=e.child(step.second);second=unsafe {world.call(self.expr(&operand)).await}?;present=u32::from(second.is_some());},
            SourceValueNeed::Binary=>return self.binary(source.text(step.spelling),*first.take().unwrap(),second.take().unwrap(),e.span(),None).map(Some),
            SourceValueNeed::Condition=>{let condition:Expr=e.child(step.first);first=unsafe {world.call(self.temporary_scope(condition.span(),async |builder|Ok((unsafe {builder.world.call(builder.expr(&condition)).await}?,view::Borrow::default())))).await}?.map(Box::new);present=u32::from(first.is_some());},
            SourceValueNeed::RequireCondition=>{let condition=e.child::<crate::syntax::wire::ExprNode>(step.first);self.require_bool(first.as_ref().unwrap(),condition.span())?;},
            SourceValueNeed::ShortCircuit=>{let right:Expr=e.child(step.second);return unsafe {world.call(self.short_circuit_value(source.text(step.spelling),*first.take().unwrap(),right.span(),
                async |builder|unsafe {builder.world.call(builder.evaluate_returned(&right,None)).await},
                async |builder,names|unsafe {builder.world.call(expression_assignments(builder,&right,names)).await},e.span())).await};},
            SourceValueNeed::Empty=>return Ok(None),
        }}
    }
    #[inline(never)]
    fn constant_expression(&mut self, scalar: Scalar, bits: Bits) -> Result<Option<Value>> {
        Ok(Some(self.constant(scalar, bits)))
    }
    #[inline(never)]
    fn returned_expression(&mut self, mut result: Option<returns::Returned>, span: Span, boolean: bool) -> Result<Option<Value>> {
        let mut cursor=SourceValueCursor {phase:0};let mut ordinary=None;let mut value=None;
        loop {let step=unsafe {returned_value_step(cursor,u32::from(self.flow.continues()),u32::from(boolean))};cursor=step.cursor;match step.need {
            ReturnedValueNeed::Empty=>return Ok(None),
            ReturnedValueNeed::Ordinary=>ordinary=Some(self.ordinary_return(result.take(),span)?),
            ReturnedValueNeed::Value=>value=Some(self.value_result(ordinary.take().unwrap(),span)?),
            ReturnedValueNeed::Bool=>self.require_bool(value.as_ref().unwrap(),span)?,
            ReturnedValueNeed::Complete=>return Ok(value.take()),
        }}
    }
    async fn function_from_address(&mut self,explicit:&[GenericArgument],count:usize,span:Span,argument_span:Span,
        argument:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Value>>)->Result<Option<Value>>{
        let mut cursor=FunctionFromAddressCursor{phase:0};let mut facts=FunctionFromAddressFacts{cpu:u32::from(self.program.target==Target::Cpu),explicit_type:u32::from(matches!(explicit,[GenericArgument::Type(_)])),count:count as u64,function:0,present:0};
        let mut argument=Some(argument);let mut ty=None;let mut value=None;
        loop{let step=unsafe{function_from_address_step(cursor,FunctionFromAddressFacts{..facts})};cursor=step.cursor;match step.need{
            FunctionFromAddressNeed::BadTarget=>return self.fail(span,"B265","unsafe_function只解释本进程CPU函数地址"),
            FunctionFromAddressNeed::BadArguments=>return self.fail(span,"B153","unsafe_function<cfn<...>>需要明确签名和一个address实参"),
            FunctionFromAddressNeed::Type=>{let[GenericArgument::Type(signature)]=explicit else{unsafe{std::hint::unreachable_unchecked()}};let actual=unsafe{self.world.call(self.world.ty(signature,self.scope.source(),span,self.scope.module(),self.scope.imports())).await}?;facts.function=u32::from(actual.is_c_function());ty=Some(actual);},
            FunctionFromAddressNeed::BadSignature=>return self.fail(span,"B263","unsafe_function需要cfn原生C签名"),
            FunctionFromAddressNeed::Argument=>{value=argument.take().unwrap()(self).await?;facts.present=u32::from(value.is_some());},
            FunctionFromAddressNeed::Empty=>return Ok(None),
            FunctionFromAddressNeed::RequireAddress=>self.require_same(&Ty::builtin("address").unwrap(),&value.as_ref().unwrap().ty,argument_span)?,
            FunctionFromAddressNeed::Retype=>{let value=value.as_mut().unwrap();value.ty=ty.take().unwrap();value.proof.retain_view_borrows(&value.ty);value.literal=false;},
            FunctionFromAddressNeed::Complete=>return Ok(value.take()),
        }}
    }
    #[inline(never)]
    fn number_expression(&mut self, n: &str, span: Span) -> Result<Option<Value>> {
        let (t, b) = number(self.scope.source(), span, n)?;
        let scalar = t.components[0].0;
        let mut value = self.literal(scalar, b);
        value.literal = scalar.size() == 4;
        Ok(Some(value))
    }
    fn cast(&mut self,value:Value,target:Scalar)->Value { self.cast_value(value,target) }
    fn numeric_operands(&mut self, a: Value, b: Value, span: Span) -> Result<(Value, Value)> {
        let types = operators::numeric(&a.ty, a.literal, &b.ty, b.literal,
            self.scope.source(), span)?;
        let a = if types.cast_left { self.cast(a, types.scalar) } else { a };
        let b = if types.cast_right { self.cast(b, types.scalar) } else { b };
        Ok((a, b))
    }
    #[inline(never)]
    fn binary(&mut self, op: &str, a: Value, b: Value, span: Span, generated_origin: Option<Arc<GeneratedOrigin>>) -> Result<Value> {
        self.source_binary_value(op,a,b,span,generated_origin)
    }

    fn lower_numeric_binary(
        &mut self, op: &str, ty: Ty, a: &Value, b: &Value, scalar: Scalar,
        comparison: bool, operation: Option<BinaryOp>, literal: bool,
        divisors: Option<Vec<IntegerDivisor>>, generated_origin: Option<Arc<GeneratedOrigin>>,
    ) -> Value {
        self.numeric_binary_value(op,ty,a,b,scalar,comparison,operation,literal,divisors,generated_origin)
    }
    fn math(&mut self,name:&str,signature:operators::MathSignature,args:Vec<Value>,span:Span)->Result<Value> {
        self.math_value(name,signature,args,span)
    }
    fn lower_math(&mut self,operation:MathOp,args:Vec<Value>,scalar:Scalar,width:usize)->Value {
        self.floating_math_value(operation,args,scalar,width)
    }
    async fn builtin_call(&mut self, name: &str, args: &Expressions, span: Span, mut record:Option<Option<Ty>>) -> Result<Option<Value>> {
        let mut cursor=BuiltinCallCursor{phase:0,collective:collective::Form::Unknown,index:0};
        let mut facts=BuiltinCallFacts{present:0,record:match &record{None=>0,Some(None)=>1,Some(Some(_))=>2},in_function:u32::from(self.in_function),count:args.len()};
        let mut subgroup=None;
        loop{let step=unsafe{builtin_call_step(cursor,BuiltinCallFacts{..facts},name.as_ptr(),name.len())};cursor=step.cursor;match step.need{
            BuiltinCallNeed::CFunction=>return unsafe{self.world.call(self.c_function_value(args,span)).await}.map(Some),
            BuiltinCallNeed::ResultContext=>return self.fail(span,"B269","Ok／Err 需要 result<T,E> 上下文或两个显式类型实参"),
            BuiltinCallNeed::Transfer=>return unsafe{self.world.call(self.application_transfer(name,args,span)).await},
            BuiltinCallNeed::Println=>return unsafe{self.world.call(self.println(args,span)).await},
            BuiltinCallNeed::TextureSample=>return unsafe{self.world.call(self.texture_sample(args,span)).await},
            BuiltinCallNeed::Append=>return unsafe{self.world.call(self.append(args,span)).await},
            BuiltinCallNeed::TileCollective=>return self.fail(span,"B205","tile 集合运算只能作为 parallel 根块的局部变量声明"),
            BuiltinCallNeed::TilePublish=>return self.fail(span,"B203","tile_publish 只能作为 parallel 根块的 let 发布声明"),
            BuiltinCallNeed::TileInput=>return self.fail(span,"B198","tile_input 只能作为 parallel 开头的 let 输入声明，不能创建普通值或在控制流中构造"),
            BuiltinCallNeed::Subgroup=>{subgroup=unsafe{self.world.call(self.subgroup(name,args.len(),span,async |builder,index|unsafe{builder.world.call(builder.expr(&args.at(index))).await},|index|args.at(index).span())).await};facts.present=u32::from(subgroup.is_some());},
            BuiltinCallNeed::SubgroupResult=>return subgroup.take().unwrap(),
            BuiltinCallNeed::RayQueryAny|BuiltinCallNeed::RayQueryClosest=>return unsafe{self.world.call(self.ray_query_call(args,span,matches!(step.need,BuiltinCallNeed::RayQueryAny))).await},
            BuiltinCallNeed::UnsafeRead=>return unsafe{self.world.call(self.unsafe_read_call(args,span)).await},
            BuiltinCallNeed::Ordinal=>return self.ordinal_call(args,span),
            BuiltinCallNeed::IndexScope=>return self.fail(span,"B155","index() 只能在当前 program 的 parallel 域中使用"),
            BuiltinCallNeed::Index=>return Ok(Some(Value{
                proof:self.path_proof(&Ty::scalar(Scalar::U32),self.data_index_proof(self.domain)
                    .with_domains(Domains::from([IndexDomain::Data(self.domain)])).with_identity(ValueIdentity::Register(self.global_index))),
                literal:false,ty:Ty::scalar(Scalar::U32),regs:vec![self.global_index],uniform:false,constant:None,
            })),
            BuiltinCallNeed::Unsupported=>return self.fail(span,"B156",format!("{name} 的作用域／能力语义尚未实现，当前编译器明确拒绝")),
            BuiltinCallNeed::AtomicStore=>return self.fail(span,"B218","atomic_store 是语句，没有可绑定的返回值"),
            BuiltinCallNeed::Atomic=>return unsafe{self.world.call(self.atomic_operation(step.atomic,name,args,span)).await},
            BuiltinCallNeed::TileBarrier=>return self.fail(span,"B218","tile_barrier 是语句，没有可绑定的返回值"),
            BuiltinCallNeed::RecordValues=>return unsafe{self.world.call(self.record_values(record.take().unwrap().unwrap(),args.len(),span,
                async |builder,index,expected|unsafe{builder.world.call(builder.expr_expected(&args.at(index),expected)).await})).await},
            BuiltinCallNeed::RecordLookup=>facts.present=u32::from(!self.world.resolve(name,self.scope.module(),self.scope.imports(),false).is_missing()),
            BuiltinCallNeed::RecordExpression=>return unsafe{self.world.call(self.record_expression(&TypeName::named(name),args,span)).await},
            BuiltinCallNeed::AddressOf=>return unsafe{self.world.call(self.position_address(args,span)).await},
            BuiltinCallNeed::Values=>return unsafe{self.world.call(self.builtin_values(name,span,async |builder|{
                let mut cursor=step.cursor;let mut facts=BuiltinCallFacts{present:0,record:0,in_function:0,count:args.len()};
                let mut values=Vec::new();let mut value=None;
                loop{let step=unsafe{builtin_call_step(cursor,BuiltinCallFacts{..facts},name.as_ptr(),name.len())};cursor=step.cursor;match step.need{
                    BuiltinCallNeed::Argument=>{value=unsafe{builder.world.call(builder.expr(&args.at(step.cursor.index))).await}?;facts.present=u32::from(value.is_some());},
                    BuiltinCallNeed::ArgumentValue=>values.push(value.take().unwrap()),
                    BuiltinCallNeed::ArgumentsComplete=>return Ok(Some(values)),
                    BuiltinCallNeed::Empty=>return Ok(None),
                    _=>unsafe{std::hint::unreachable_unchecked()},
                }}
            })).await},
            _=>unsafe{std::hint::unreachable_unchecked()},
        }}
    }
    // The original value/IR owner consumes values from either actual input reader.
    // Argument evaluation stays lazy until this owner has selected a known operation.
    async fn builtin_values(&mut self,name:&str,span:Span,
        values:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Vec<Value>>>)->Result<Option<Value>>{
        let mut cursor=unsafe{builtin_value_start(name.as_ptr(),name.len()as u64)};let mut facts=BuiltinValueFacts{present:0,cpu:u32::from(self.program.target==Target::Cpu),count:0,function:0,low_known:0,high_known:0,low_uniform:0,high_uniform:0};
        let mut values=Some(values);let mut arguments=None;let mut value:Option<Value>=None;let mut ty=None;let mut bitcast=None;let mut math=None;let mut matrix=None;let mut geometry=None;let mut low=None;let mut high=None;let mut last=None;
        loop{let step=unsafe{builtin_value_step(cursor,BuiltinValueFacts{..facts},name.as_ptr(),name.len()as u64)};cursor=step.cursor;match step.need{
            BuiltinValueNeed::Type=>{ty=Ty::builtin(name);facts.present=u32::from(ty.is_some());},
            BuiltinValueNeed::Bitcast=>{let actual=operators::BitcastSignature::recognize(name);facts.present=u32::from(actual.is_present());bitcast=Some(actual);},
            BuiltinValueNeed::Math=>{let actual=operators::MathSignature::recognize(name);facts.present=u32::from(actual.is_present());math=Some(actual);},
            BuiltinValueNeed::Matrix=>{let actual=matrix::MatrixBuiltin::recognize(name);facts.present=u32::from(actual!=matrix::MatrixBuiltin::Unknown);matrix=Some(actual);},
            BuiltinValueNeed::Geometry=>{let actual=operators::GeometryBuiltin::recognize(name);facts.present=u32::from(actual!=operators::GeometryBuiltin::Unknown);geometry=Some(actual);},
            BuiltinValueNeed::Values=>{arguments=values.take().unwrap()(self).await?;facts.present=u32::from(arguments.is_some());facts.count=arguments.as_ref().map_or(0,|v|v.len()as u64);},
            BuiltinValueNeed::Empty=>return Ok(None),
            BuiltinValueNeed::Unknown=>return self.fail(span,"B158",format!("未知或未导出的函数或能力 {name}")),
            BuiltinValueNeed::Constructor=>return self.value_constructor(ty.take().unwrap(),arguments.take().unwrap(),span).map(Some),
            BuiltinValueNeed::InvokeBitcast=>return self.bitcast_value(name,bitcast.take().unwrap(),arguments.take().unwrap(),span).map(Some),
            BuiltinValueNeed::InvokeMath=>return self.math(name,math.take().unwrap(),arguments.take().unwrap(),span).map(Some),
            BuiltinValueNeed::InvokeMatrix=>return self.matrix_intrinsic(matrix.take().unwrap(),arguments.take().unwrap(),span).map(Some),
            BuiltinValueNeed::InvokeGeometry=>return self.geometry_value(geometry.take().unwrap(),arguments.take().unwrap(),span).map(Some),
            BuiltinValueNeed::BadFunctionTarget=>return self.fail(span,"B265","function_address只取得本进程CPU函数地址"),
            BuiltinValueNeed::BadAddressTarget=>return self.fail(span,"B265","CPU本地地址只能在on cpu位置解释或取得位模式"),
            BuiltinValueNeed::BadFunctionCount=>return self.fail(span,"B153","function_address需要一个cfn实参"),
            BuiltinValueNeed::BadHandleCount=>return self.fail(span,"B153","unsafe_handle需要一个usize实参"),
            BuiltinValueNeed::BadAddressCount=>return self.fail(span,"B153",format!("{name}需要一个实参")),
            BuiltinValueNeed::BadClampCount=>return self.fail(span,"B153","clamp 需要三个参数"),
            BuiltinValueNeed::Pop=>value=arguments.as_mut().unwrap().pop(),
            BuiltinValueNeed::FunctionKind=>facts.function=u32::from(value.as_ref().unwrap().ty.is_c_function()),
            BuiltinValueNeed::BadFunctionKind=>return self.fail(span,"B263","function_address需要cfn原生C签名值"),
            BuiltinValueNeed::FunctionToAddress|BuiltinValueNeed::RawToAddress=>{let mut actual=value.take().unwrap();actual.ty=Ty::builtin("address").unwrap();actual.proof=actual.proof.with_external_borrows(&actual.ty);actual.literal=false;value=Some(actual);},
            BuiltinValueNeed::RequireU64=>self.require_same(&Ty::scalar(Scalar::U64),&value.as_ref().unwrap().ty,span)?,
            BuiltinValueNeed::RequireAddress=>self.require_same(&Ty::builtin("address").unwrap(),&value.as_ref().unwrap().ty,span)?,
            BuiltinValueNeed::HandleLow=>{let actual=self.cast(value.as_ref().unwrap().clone(),Scalar::U32);facts.low_known=u32::from(actual.constant.is_some());facts.low_uniform=u32::from(actual.uniform);low=Some(actual);},
            BuiltinValueNeed::HandleHigh=>{let shift=self.constant(Scalar::U64,32);let shifted=self.binary(">>",value.take().unwrap(),shift,span,None)?;let actual=self.cast(shifted,Scalar::U32);facts.high_known=u32::from(actual.constant.is_some());facts.high_uniform=u32::from(actual.uniform);high=Some(actual);},
            BuiltinValueNeed::HandlePackage=>{let low=low.take().unwrap();let high=high.take().unwrap();let constant=(step.flags&1!=0).then(||vec![low.constant.as_ref().unwrap()[0],high.constant.as_ref().unwrap()[0]]);value=Some(Value{ty:Ty::builtin("handle").unwrap(),regs:vec![low.regs[0],high.regs[0]],proof:bounds::Proof::default(),literal:false,uniform:step.flags&2!=0,constant});},
            BuiltinValueNeed::AddressToBits=>{let value=value.as_mut().unwrap();value.ty=Ty::scalar(Scalar::U64);value.proof.retain_view_borrows(&value.ty);value.literal=false;},
            BuiltinValueNeed::ClampMax=>{let mut source=arguments.take().unwrap().into_iter();let a=source.next().unwrap();let b=source.next().unwrap();last=source.next();value=Some(self.math("max",operators::MathSignature::recognize("max"),vec![a,b],span)?);},
            BuiltinValueNeed::ClampMin=>value=Some(self.math("min",operators::MathSignature::recognize("min"),vec![value.take().unwrap(),last.take().unwrap()],span)?),
            BuiltinValueNeed::Complete=>return Ok(value.take()),
        }}
    }
    fn negative_number(&mut self, literal: &str, span: Span) -> Result<Value> {
        let (ty, bits) = numeric_literal::number(self.scope.source(), span, literal, true)?;
        let scalar = ty.components[0].0;
        let mut value = self.constant_value(ty, vec![bits], None);
        value.literal = scalar.size() == 4;
        Ok(value)
    }
    fn unary_value(&mut self,name:&str,value:Value,span:Span)->Result<Value> {
        self.lower_unary_value(name,value,span)
    }
    async fn short_circuit_value(
        &mut self, op: &str, condition: Value, right_span: Span,
        right: impl std::ops::AsyncFnOnce(&mut Self) -> Result<Option<returns::Returned>>,
        modified: impl std::ops::AsyncFnOnce(&Self, &mut BTreeSet<String>) -> Result<()>, span: Span,
    ) -> Result<Option<Value>> {
        let plan=unsafe {short_circuit_plan(SourceOperation {data:op.as_ptr(),length:op.len()as u64})};
        let skip=async |builder:&mut Self|Ok(Some(returns::Returned::Value(Box::new(builder.constant(Scalar::Bool,Bits::from(plan.skipped))))));
        let result=match plan.order {
            ShortCircuitOrder::RightYes=>unsafe {self.world.call(self.selection_after_condition(condition,(right_span,right),(span,skip),modified,span)).await}?,
            ShortCircuitOrder::RightNo=>unsafe {self.world.call(self.selection_after_condition(condition,(span,skip),(right_span,right),async |_,_|Ok(()),span)).await}?,
        };
        self.returned_expression(result,span,true)
    }
    fn value_constructor(&mut self,ty:Ty,values:Vec<Value>,span:Span)->Result<Value> {
        self.construct_value(operators::ConstructEntry::Value,ty,values,span)
    }
    fn member_position(&mut self,p:Place,field:&str,span:Span)->Result<Option<Place>> {
        match unsafe {member_position(u32::from(p.ty().view.is_some()))} {
            MemberPositionNeed::BadView=>self.fail(span,"B185","view 元数据不可修改或按分量访问"),
            MemberPositionNeed::Project=>{let(sub,components)=member_type(p.ty(),field,self.scope.source(),span)?;Ok(Some(p.project(sub,&components)))},
        }
    }
    async fn index_place(&mut self,base:&Expr,index:&Expr)->Result<Option<Place>> {
        let mut cursor=SourceValueCursor {phase:0};let mut facts=IndexPositionFacts {present:0,bounded:0,current:0,same_domain:0,same_shape:0,access:0,whole:0,appending:0};
        let mut data=None;let mut tile=None;let mut view=None;let mut value=None;
        loop {let step=unsafe {index_position_step(cursor,base.signature_node(),IndexPositionFacts {..facts})};cursor=step.cursor;match step.need {
            IndexPositionNeed::Application=>{data=unsafe {self.world.call(self.application_reference(base)).await}?;facts.present=u32::from(data.is_some());},
            IndexPositionNeed::ApplicationIndex=>return unsafe {self.world.call(self.application_index(data.take().unwrap(),index)).await},
            IndexPositionNeed::TileBinding=>{tile=match self.env.get(base.source_view().text(step.name)){Some(Binding::Tile(tile))=>Some(tile.clone()),_=>None};facts.present=u32::from(tile.is_some());},
            IndexPositionNeed::TileIndex=>return unsafe {self.world.call(self.tile_place(tile.take().unwrap(),index)).await},
            IndexPositionNeed::Dataset=>{view=self.dataset_reference(base)?;facts.present=u32::from(view.is_some());if let Some(view)=&view{facts.access=view.access as u32;facts.whole=u32::from(view.whole);}},
            IndexPositionNeed::View=>return unsafe {self.world.call(self.view_place(base,index)).await},
            IndexPositionNeed::Index=>{value=unsafe {self.world.call(self.expr(index)).await}?;facts.present=u32::from(value.is_some());},
            IndexPositionNeed::Type=>self.require_same(&Ty::scalar(Scalar::U32),&value.as_ref().unwrap().ty,index.span())?,
            IndexPositionNeed::Bounds=>{let view=view.as_ref().unwrap();let value=value.as_ref().unwrap();facts.current=u32::from(value.regs[0]==self.global_index);facts.same_domain=u32::from(view.binding==self.domain);facts.same_shape=u32::from(self.program.same_length(view.binding,self.domain));facts.bounded=u32::from(self.index_in(value,IndexDomain::Data(view.binding),self.program.buffers[view.binding].count.unwrap_or(0),None));},
            IndexPositionNeed::BadBounds=>return self.fail(index.span(),"B137","索引有效性无法静态证明；使用有界游标、同域 index()、有效长度条件或形状约束"),
            IndexPositionNeed::Append=>{let mut entries=self.appends.iter();facts.appending=unsafe {position_append(view.as_ref().unwrap().binding as u64,&mut entries,next_position_append)};},
            IndexPositionNeed::BadAppend=>return self.fail(index.span(),"B221","append 的共享输出／计数不能在同一阶段借为普通独占元素"),
            IndexPositionNeed::Domain=>{self.element_domains.insert(view.as_ref().unwrap().binding);},
            IndexPositionNeed::Element=>return Ok(Some(self.dataset_element(view.as_ref().unwrap(),value.as_ref().unwrap().regs[0],step.writable!=0))),
            IndexPositionNeed::Empty=>return Ok(None),
        }}
    }
    fn name_place(&mut self,name:&str,span:Span)->Result<Place> {
        let kind=named_binding_kind(self.env.get(name));
        let mut cursor=SourceValueCursor {phase:0};let mut facts=NamePositionFacts {kind,present:0,arrays:0};let mut place=None;
        loop {let step=unsafe {name_position_step(cursor,NamePositionFacts {..facts})};cursor=step.cursor;match step.need {
            NamePositionNeed::Atomic=>{let Some(Binding::AtomicBorrow(place))=self.env.get(name)else{unsafe {std::hint::unreachable_unchecked()}};return Ok(place.clone());},
            NamePositionNeed::Owned=>{let Some(Binding::Owned(owner))=self.env.get(name)else{unsafe {std::hint::unreachable_unchecked()}};return Ok(owner.place.clone());},
            NamePositionNeed::ViewElement=>{let Some(Binding::ViewElement {element,..})=self.env.get(name)else{unsafe {std::hint::unreachable_unchecked()}};return Ok(element.place.clone());},
            NamePositionNeed::InspectOwned=>{let Some(Binding::OwnedValue(owner))=self.env.get(name)else{unsafe {std::hint::unreachable_unchecked()}};place=owner.place();facts.present=u32::from(place.is_some());},
            NamePositionNeed::ExistingOwned=>return Ok(place.take().unwrap()),
            NamePositionNeed::MaterializeOwned=>{let(key,Binding::OwnedValue(owner))=self.env.remove_entry(name).unwrap()else{unsafe {std::hint::unreachable_unchecked()}};let owner=self.owned_value_binding(owner,true);let place=owner.place().unwrap();self.env.insert(key,Binding::OwnedValue(owner));return Ok(place);},
            NamePositionNeed::ArrayType=>{let Some(Binding::Value(value))=self.env.get(name)else{unsafe {std::hint::unreachable_unchecked()}};facts.arrays=u32::from(value.ty.contains_arrays());},
            NamePositionNeed::MaterializeArray=>{let(key,Binding::Value(value))=self.env.remove_entry(name).unwrap()else{unsafe {std::hint::unreachable_unchecked()}};let mut owner=self.materialize_array(value,None);owner.place=owner.place.read_only();let place=owner.place.clone();self.env.insert(key,Binding::Owned(owner));return Ok(place);},
            NamePositionNeed::Bad=>return self.fail(span,"B134",format!("{name} 不是可赋值的元素或局部变量")),
        }}
    }
    fn advance_iterator(&mut self, slot: usize) -> Result<Value> {
        let source=self.loop_cursor().unwrap();
        let one=self.constant(Scalar::U32,1);
        let mut value=self.binary("+",source,one,self.scope.span(),None)?;
        let (_, _, limit) = self.iterators.last().unwrap().as_ref().unwrap();
        value.proof.mark_domain(0, IndexDomain::Upper(limit.identity(), 0));
        self.emit(InstKind::StoreLocal(slot,value.regs[0]));
        Ok(value)
    }
    async fn ray_query_call(&mut self,args:&Expressions,span:Span,any:bool)->Result<Option<Value>>{
        let mut cursor=SourceValueCursor{phase:0};let mut facts=RayQueryFacts{count:args.len()as u64,any:u32::from(any),present:0};
        let mut first=None;let mut scene=None;let mut origin=None;let mut minimum=None;let mut direction=None;let mut maximum=None;let mut cull=None;let mut cull_back_faces=None;let mut ty=None;let mut regs:Option<Vec<Reg>>=None;let mut result=None;
        loop{let step=unsafe{ray_query_step(cursor,RayQueryFacts{..facts})};cursor=step.cursor;match step.need{
            RayQueryNeed::BadArguments=>return self.fail(span,"B183","光线查询需要 acceleration、origin、minimum、direction、maximum，以及可选的 bool cull_back_faces"),
            RayQueryNeed::Name=>{let argument=args.at(0);facts.present=u32::from(matches!(argument.kind(),ExprKind::Name(_)));first=Some(argument);},
            RayQueryNeed::MissingName=>return self.fail(span,"B183","光线查询需要命名 acceleration 资源"),
            RayQueryNeed::Binding=>{let argument=first.take().unwrap();let ExprKind::Name(name)=argument.kind()else{unsafe{std::hint::unreachable_unchecked()}};scene=match self.env.get(name){Some(Binding::Acceleration(scene))=>Some(*scene),_=>None};facts.present=u32::from(scene.is_some());},
            RayQueryNeed::BadSource=>return self.fail(span,"B183","光线查询需要 read acceleration 资源"),
            RayQueryNeed::Origin=>{origin=unsafe{self.world.call(self.expr(&args.at(1))).await}?;facts.present=u32::from(origin.is_some());},
            RayQueryNeed::Minimum=>{minimum=unsafe{self.world.call(self.expr(&args.at(2))).await}?;facts.present=u32::from(minimum.is_some());},
            RayQueryNeed::Direction=>{direction=unsafe{self.world.call(self.expr(&args.at(3))).await}?;facts.present=u32::from(direction.is_some());},
            RayQueryNeed::Maximum=>{maximum=unsafe{self.world.call(self.expr(&args.at(4))).await}?;facts.present=u32::from(maximum.is_some());},
            RayQueryNeed::Empty=>return Ok(None),
            RayQueryNeed::OriginType=>self.require_same(&Ty::vector(Scalar::F32,3),&origin.as_ref().unwrap().ty,args.at(1).span())?,
            RayQueryNeed::MinimumType=>self.require_same(&Ty::scalar(Scalar::F32),&minimum.as_ref().unwrap().ty,args.at(2).span())?,
            RayQueryNeed::DirectionType=>self.require_same(&origin.as_ref().unwrap().ty,&direction.as_ref().unwrap().ty,args.at(3).span())?,
            RayQueryNeed::MaximumType=>self.require_same(&minimum.as_ref().unwrap().ty,&maximum.as_ref().unwrap().ty,args.at(4).span())?,
            RayQueryNeed::Cull=>{cull=unsafe{self.world.call(self.expr(&args.at(5))).await}?;facts.present=u32::from(cull.is_some());},
            RayQueryNeed::CullType=>{let value=cull.take().unwrap();self.require_same(&Ty::scalar(Scalar::Bool),&value.ty,args.at(5).span())?;cull_back_faces=Some(value.regs[0]);},
            RayQueryNeed::AnyType=>ty=Some(Ty::scalar(Scalar::Bool)),
            RayQueryNeed::ClosestType=>ty=Some(Ty::builtin("RayHit").unwrap()),
            RayQueryNeed::Registers=>regs=Some(ty.as_ref().unwrap().components.iter().map(|(scalar,_)|self.reg(*scalar)).collect()),
            RayQueryNeed::AnyResult=>result=Some(RayResult::Any(regs.as_ref().unwrap()[0])),
            RayQueryNeed::ClosestResult=>result=Some(RayResult::Closest(regs.as_ref().unwrap().as_slice().try_into().unwrap())),
            RayQueryNeed::Emit=>self.emit(InstKind::RayQuery{scene:scene.unwrap(),origin:origin.as_ref().unwrap().regs.as_slice().try_into().unwrap(),direction:direction.as_ref().unwrap().regs.as_slice().try_into().unwrap(),minimum:minimum.as_ref().unwrap().regs[0],maximum:maximum.as_ref().unwrap().regs[0],cull_back_faces,result:result.take().unwrap()}),
            RayQueryNeed::Value=>return Ok(Some(Value{proof:bounds::Proof::default(),literal:false,ty:ty.take().unwrap(),regs:regs.take().unwrap(),uniform:false,constant:None})),
        }}
    }
    async fn unsafe_read_call(&mut self, args: &Expressions, span: Span) -> Result<Option<Value>> {
        let storage=args.source_view().storage();let arguments=args.source_slice();let mut cursor=SourceValueCursor {phase:0};
        let mut facts=UnsafeReadFacts {present:0,access:0};let mut data=None;let mut dataset=None;let mut value=None;let mut index=None;
        loop {let step=unsafe {unsafe_read_step(cursor,storage,arguments,UnsafeReadFacts {..facts})};cursor=step.cursor;match step.need {
            UnsafeReadNeed::BadArguments=>return self.fail(span,"B181","unsafe_read 需要 read 数据集与 u32 索引；调用者负责外部索引有效性"),
            UnsafeReadNeed::Application=>{data=unsafe {self.world.call(self.application_reference(&args.at(0))).await}?;facts.present=u32::from(data.is_some());},
            UnsafeReadNeed::ApplicationCpu=>self.application_cpu(data.as_ref().unwrap(),span)?,
            UnsafeReadNeed::Dataset=>{dataset=self.dataset_reference(&args.at(0))?;facts.present=u32::from(dataset.is_some());facts.access=dataset.as_ref().map_or(0,|view|view.access as u32);},
            UnsafeReadNeed::BadAccess=>return self.fail(span,"B181","unsafe_read 需要 read 或本阶段共享只读的 inout 数据集"),
            UnsafeReadNeed::Value=>{value=unsafe {self.world.call(self.expr(&args.at(0))).await}?;facts.present=u32::from(value.is_some());},
            UnsafeReadNeed::Index=>{index=unsafe {self.world.call(self.expr(&args.at(1))).await}?;facts.present=u32::from(index.is_some());},
            UnsafeReadNeed::IndexType=>self.require_same(&Ty::scalar(Scalar::U32),&index.as_ref().unwrap().ty,args.at(1).span())?,
            UnsafeReadNeed::ApplicationRead=>{let index=index.as_ref().unwrap();return unsafe {self.world.call(self.read_place(&data.as_ref().unwrap().element(index,(index.minimum(),index.maximum())),span,None)).await}.map(Some);},
            UnsafeReadNeed::DatasetRead=>return unsafe {self.world.call(self.read_place(&self.dataset_element(dataset.as_ref().unwrap(),index.as_ref().unwrap().regs[0],false),span,None)).await}.map(Some),
            UnsafeReadNeed::ViewRead=>return unsafe {self.world.call(self.unsafe_read_view(value.take().unwrap(),span,args.at(1).span(),async |builder|unsafe {builder.world.call(builder.expr(&args.at(1))).await})).await},
            UnsafeReadNeed::Empty=>return Ok(None),
        }}
    }
    async fn unsafe_read_view(&mut self,view:Value,span:Span,index_span:Span,
        read_index:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Value>>)->Result<Option<Value>> {
        let mut cursor=SourceValueCursor {phase:0};let mut facts=UnsafeViewFacts {view:u32::from(view.ty.view.is_some()),present:0};
        let mut read_index=Some(read_index);let mut held=None;let mut evaluated=None;let mut index=None;
        loop {let step=unsafe {unsafe_view_step(cursor,UnsafeViewFacts {..facts})};cursor=step.cursor;match step.need {
            UnsafeViewNeed::BadView=>return self.fail(span,"B181","unsafe_read 需要 read dataset 或不可变 view"),
            UnsafeViewNeed::Hold=>held=Some(self.hold_binding(Binding::Value(view.clone()))),
            UnsafeViewNeed::Index=>evaluated=Some(read_index.take().unwrap()(self).await),
            UnsafeViewNeed::Release=>{self.env.remove(held.as_ref().unwrap());},
            UnsafeViewNeed::Result=>{index=evaluated.take().unwrap()?;facts.present=u32::from(index.is_some());},
            UnsafeViewNeed::IndexType=>self.require_same(&Ty::scalar(Scalar::U32),&index.as_ref().unwrap().ty,index_span)?,
            UnsafeViewNeed::Read=>return unsafe {self.world.call(self.read_place(&Self::view_element(&view,index.as_ref().unwrap().regs[0]),span,None)).await}.map(Some),
            UnsafeViewNeed::Empty=>return Ok(None),
        }}
    }
    fn ordinal_call(&mut self, args: &Expressions, span: Span) -> Result<Option<Value>> {
        let source=args.source_view();let mut cursor=SourceValueCursor {phase:0};
        let mut facts=OrdinalFacts {binding:0,place:0,minimum:0,maximum:0};let mut binding:Option<&Binding>=None;
        let (domain,index,proof)=loop {
            let step=unsafe {ordinal_step(cursor,source.storage(),args.source_slice(),OrdinalFacts {..facts})};cursor=step.cursor;
            match (step.need,binding) {
                (OrdinalNeed::BadArguments,_)=>return self.fail(span,"B168","ordinal 需要一个有界游标"),
                (OrdinalNeed::BadName,_)=>return self.fail(span,"B168","ordinal 需要命名游标"),
                (OrdinalNeed::BadCursor,_)=>return self.fail(span,"B168","ordinal 只能用于元素游标"),
                (OrdinalNeed::Binding,_)=>{
                    binding=self.env.get(source.text(step.name));facts.binding=named_binding_kind(binding);
                    let place=match binding {Some(Binding::Owned(value))=>Some(&value.place),Some(Binding::ViewElement {element,..})=>Some(&element.place),_=>None};
                    facts.place=match place {Some(Place::ApplicationElement {index_range,..})=>{facts.minimum=index_range.0;facts.maximum=index_range.1;1},Some(Place::ViewElement {..})=>3,Some(Place::TileElement {..})=>4,Some(Place::Element {..})=>7,_=>0};
                    if let Some(Binding::ViewElement {view,..})=binding {facts.maximum=Self::view_length(view).maximum();}
                },
                (OrdinalNeed::Application,Some(Binding::Owned(OwnedBinding {place:Place::ApplicationElement {identity,index,..},..})))=>break (IndexDomain::ApplicationData(*identity),*index,bounds::Proof::unsigned(step.minimum,step.maximum)),
                (OrdinalNeed::Dataset,Some(Binding::Owned(OwnedBinding {place:Place::Element {binding,index,..},..})))=>break (IndexDomain::Data(*binding),*index,self.data_index_proof(*binding)),
                (OrdinalNeed::Tile,Some(Binding::Owned(OwnedBinding {place:Place::TileElement {index,..},..})))=>break (IndexDomain::Tile,*index,self.tile_index_proof()),
                (OrdinalNeed::View,Some(Binding::ViewElement {view,element:OwnedBinding {place:Place::ViewElement {index,..},..}}))=>break (IndexDomain::View(view.view_identity()),*index,bounds::Proof::unsigned(step.minimum,step.maximum)),
                _=>unsafe {std::hint::unreachable_unchecked()},
            }
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
    pub(super) fn member_value(&mut self, v: Value, field: &str, span: Span) -> Result<Value> {
        match unsafe{member_value_need(u32::from(v.ty.array.is_some()),u32::from(v.ty.view.is_some()),field.as_ptr(),field.len())}{
            MemberValueNeed::ArrayLength=>return Ok(self.constant(Scalar::U32,Bits::from(v.ty.values[0]))),
            MemberValueNeed::ViewLength=>return Ok(Self::view_length(&v)),
            MemberValueNeed::Project=>{},
        }
        let (ty, indices) = member_type(&v.ty, field, self.scope.source(), span)?;
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
    async fn index_expr(&mut self, base: &Expr, index: &Expr, e: &Expr) -> Result<Option<Value>> {
        let resource = self.is_dataset_reference(base)? || matches!(base.kind(), ExprKind::Name(name) if matches!(self.env.get(name), Some(Binding::Tile(_) | Binding::ApplicationData(_))));
        if !resource && !self.is_place(base) {
            let returned = unsafe { self.world.call(self.owned_array_index(e, base, index)).await }?;
            return self.ordinary_return(returned, e.span());
        }
        let Some(p) = unsafe { self.world.call(self.place(e)).await }? else {
            return Ok(None);
        };
        unsafe { self.world.call(self.read_place(&p, e.span(), None)).await }.map(Some)
    }
    #[inline(never)]
    async fn name_expr(&mut self,name:&str,span:Span)->Result<Value> {
        let mut cursor=SourceValueCursor {phase:0};let mut facts=NameValueFacts {present:0,kind:0,constant:0};let mut scope_value=None;let mut place=None;let mut key=None;
        loop {let step=unsafe {name_value_step(cursor,NameValueFacts {..facts})};cursor=step.cursor;match step.need {
            NameValueNeed::Scope=>{scope_value=self.scope.bindings().values.get(name).copied();facts.present=u32::from(scope_value.is_some());},
            NameValueNeed::ScopeValue=>return Ok(self.constant(Scalar::U32,Bits::from(scope_value.unwrap()))),
            NameValueNeed::Binding=>facts.kind=named_binding_kind(self.env.get(name)),
            NameValueNeed::Value=>{let Some(Binding::Value(value))=self.env.get(name)else{unsafe {std::hint::unreachable_unchecked()}};return Ok(value.clone());},
            NameValueNeed::OwnedPlace=>{let Some(Binding::Owned(owner))=self.env.get(name)else{unsafe {std::hint::unreachable_unchecked()}};place=Some(owner.place.clone());},
            NameValueNeed::NamedPlace=>place=Some(self.name_place(name,span)?),
            NameValueNeed::Read=>return unsafe {self.world.call(self.read_place(place.as_ref().unwrap(),span,None)).await},
            NameValueNeed::Resolve=>{key=self.world.resolve(name,self.scope.module(),self.scope.imports(),true).unique();facts.present=u32::from(key.is_some());},
            NameValueNeed::FunctionKind=>facts.constant=u32::from(self.world.functions[key.as_ref().unwrap()].function.kind()==FunctionKind::Constant),
            NameValueNeed::ModuleConstant=>return unsafe {self.world.call(self.module_constant(key.take().unwrap())).await},
            NameValueNeed::Unknown=>return self.fail(span,"B145",format!("未知值 {name}")),
        }}
    }
    fn is_place(&self,e:&Expr)->bool {
        let context=SourcePlaceQuery {builder:self,source:e.source_view()};
        unsafe {source_is_place(e.signature_node(),SourcePlaceQueryHost {context:&context,binding:query_named_binding})!=0}
    }

}
use effects::{assigned_names, expression_assignments};

pub(crate) fn stage_intrinsic(name: &str) -> bool {
    matches!(name, "clear_prefix" | "stable_sort_by_key" | "compact")
        || collective::from_source(name).is_stage()
        || custom_reduce::Kind::from_source(name).is_some()
}

pub(crate) struct KernelSignature {
    parameters: Vec<Param>,
    uniform_types: Vec<Ty>,
}

pub(crate) fn kernel_interface<'a>(
    world: &'a World,
    instance: &'a crate::generics::Instance,
) -> impl std::future::Future<Output = Result<bool>> + 'a {
    kernel_parameters(world, instance, declared_kernel_parameters(instance), |_, _| {})
}

pub(crate) async fn kernel_signature(
    world: &World,
    instance: &crate::generics::Instance,
) -> Result<Option<KernelSignature>> {
    let mut signature = KernelSignature { parameters: Vec::new(), uniform_types: Vec::new() };
    let eligible = unsafe {
        world.call(kernel_parameters(world, instance, declared_kernel_parameters(instance), |param, layout| {
            signature.parameters.push(param.into_owned());
            signature.uniform_types.extend(layout);
        })).await
    }?;
    Ok(eligible.then_some(signature))
}

fn declared_kernel_parameters(instance: &crate::generics::Instance) -> impl Iterator<Item = ScopeParameter<'_>> {
    instance.definition.function.params().into_iter().map(|parameter| ScopeParameter::Declaration(instance, parameter))
}

async fn kernel_parameters<'a>(
    world: &World,
    instance: &crate::generics::Instance,
    parameters: impl Iterator<Item = ScopeParameter<'a>>,
    mut retain: impl FnMut(Cow<'a, Param>, Option<Ty>),
) -> Result<bool> {
    let d=&instance.definition;
    let result=unsafe { world.call(instance.bindings.type_name(world,&d.function.result(),
        (&d.source,&d.module,&d.imports),d.function.span())).await }?;
    if !function_inputs::domain_result(&result) {
        return Ok(false);
    }
    for parameter in parameters {
        let param = match parameter {
            ScopeParameter::Declaration(instance, parameter) =>
                Cow::Owned(unsafe { world.call(instance.parameter(world, parameter)).await }?),
            ScopeParameter::Bound(parameter) => Cow::Borrowed(parameter),
        };
        let layout = match function_inputs::domain_parameter(&param) {
            function_inputs::DomainParameter::Accept => None,
            function_inputs::DomainParameter::Reject => return Ok(false),
            function_inputs::DomainParameter::PlainLayout => {
                let ty = unsafe { world.call(world
                    .ty(&param.ty, &d.source, param.span, &d.module, &d.imports)).await }?;
                if ty.owns_resources() {
                    return Ok(false);
                }
                Some(ty)
            }
        };
        retain(param, layout);
    }
    Ok(true)
}





// 已编译符号的空合同表示没有B函数合同；非空合同由原所有者交接规范身份。
pub(crate) fn read_function_contract(bytes: &[u8], same_producer: bool) -> crate::module::codec::Result<Vec<u8>> {
    use crate::module::codec::{Decode, Encode};
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let contract = native_function::Contract::from_bytes(bytes)?;
    if same_producer {
        drop(contract);
        Ok(bytes.to_vec())
    } else {
        Ok(contract.bytes())
    }
}

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(Value);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        proof: Slot,
        literal: Slot,
        ty: Slot,
        regs: Slot,
        uniform: Slot,
        constant: Slot,
    }
    #[repr(C)]
    struct Writer {
        proof: WriteCallback,
        literal: WriteCallback,
        ty: WriteCallback,
        regs: WriteCallback,
        uniform: WriteCallback,
        constant: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        proof: ReadCallback,
        literal: ReadCallback,
        ty: ReadCallback,
        regs: ReadCallback,
        uniform: ReadCallback,
        constant: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_value"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_value"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Value {
        fn empty() -> Self {
            Self {
                proof: <bounds::Proof as Native>::empty(),
                literal: <bool as Native>::empty(),
                ty: <Ty as Native>::empty(),
                regs: <Vec<Reg> as Native>::empty(),
                uniform: <bool as Native>::empty(),
                constant: <Option<Vec<Bits>> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        proof: contracts::address(&self.proof).cast_mut(),
                        literal: contracts::address(&self.literal).cast_mut(),
                        ty: contracts::address(&self.ty).cast_mut(),
                        regs: contracts::address(&self.regs).cast_mut(),
                        uniform: contracts::address(&self.uniform).cast_mut(),
                        constant: contracts::address(&self.constant).cast_mut(),
                    },
                    Writer {
                        proof: contracts::write_value::<bounds::Proof>,
                        literal: contracts::write_value::<bool>,
                        ty: contracts::write_value::<Ty>,
                        regs: contracts::write_value::<Vec<Reg>>,
                        uniform: contracts::write_value::<bool>,
                        constant: contracts::write_value::<Option<Vec<Bits>>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        proof: contracts::slot(&mut output.proof),
                        literal: contracts::slot(&mut output.literal),
                        ty: contracts::slot(&mut output.ty),
                        regs: contracts::slot(&mut output.regs),
                        uniform: contracts::slot(&mut output.uniform),
                        constant: contracts::slot(&mut output.constant),
                    },
                    Reader {
                        proof: contracts::read_value::<bounds::Proof>,
                        literal: contracts::read_value::<bool>,
                        ty: contracts::read_value::<Ty>,
                        regs: contracts::read_value::<Vec<Reg>>,
                        uniform: contracts::read_value::<bool>,
                        constant: contracts::read_value::<Option<Vec<Bits>>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

#[repr(C)]#[derive(Clone,Copy)]struct BuiltinCallCursor{phase:u32,collective:collective::Form,index:usize}
#[repr(C)]struct BuiltinCallFacts{present:u32,record:u32,in_function:u32,count:usize}
#[repr(u32)]enum BuiltinCallNeed{CFunction,ResultContext,Transfer,Println,TextureSample,Append,TileCollective,TilePublish,TileInput,Subgroup,SubgroupResult,RayQueryAny,UnsafeRead,Ordinal,IndexScope,Index,Unsupported,AtomicStore,Atomic,TileBarrier,RecordValues,RecordLookup,RecordExpression,AddressOf,Values,Argument,ArgumentValue,Empty,ArgumentsComplete,RayQueryClosest}
#[repr(C)]struct BuiltinCallStep{cursor:BuiltinCallCursor,need:BuiltinCallNeed,atomic:atomic::AtomicBuiltin}
unsafe extern "C"{#[link_name="bootstrap.operator_types::builtin_call_step"]fn builtin_call_step(cursor:BuiltinCallCursor,facts:BuiltinCallFacts,bytes:*const u8,length:usize)->BuiltinCallStep;}

#[repr(u32)]#[derive(Clone,Copy)]enum BuiltinValueKind{Other,FunctionAddress,UnsafeHandle,UnsafeAddress,AddressBits,Type,Bitcast,Math,Matrix,Geometry,Clamp}
#[repr(C)]#[derive(Clone,Copy)]struct BuiltinValueCursor{phase:u32,kind:BuiltinValueKind}
#[repr(C)]struct BuiltinValueFacts{present:u32,cpu:u32,count:u64,function:u32,low_known:u32,high_known:u32,low_uniform:u32,high_uniform:u32}
#[repr(u32)]enum BuiltinValueNeed{Type,Bitcast,Math,Matrix,Geometry,Values,Empty,Unknown,Constructor,InvokeBitcast,InvokeMath,InvokeMatrix,InvokeGeometry,BadFunctionTarget,BadAddressTarget,BadFunctionCount,BadHandleCount,BadAddressCount,BadClampCount,Pop,FunctionKind,BadFunctionKind,FunctionToAddress,RequireU64,HandleLow,HandleHigh,HandlePackage,RequireAddress,RawToAddress,AddressToBits,ClampMax,ClampMin,Complete}
#[repr(C)]struct BuiltinValueStep{cursor:BuiltinValueCursor,need:BuiltinValueNeed,flags:u32}
#[repr(C)]#[derive(Clone,Copy)]struct FunctionFromAddressCursor{phase:u32}
#[repr(C)]struct FunctionFromAddressFacts{cpu:u32,explicit_type:u32,count:u64,function:u32,present:u32}
#[repr(u32)]enum FunctionFromAddressNeed{BadTarget,BadArguments,Type,BadSignature,Argument,Empty,RequireAddress,Retype,Complete}
#[repr(C)]struct FunctionFromAddressStep{cursor:FunctionFromAddressCursor,need:FunctionFromAddressNeed}
unsafe extern "C"{
 #[link_name="bootstrap.operator_types::builtin_value_start"]fn builtin_value_start(bytes:*const u8,length:u64)->BuiltinValueCursor;
 #[link_name="bootstrap.operator_types::builtin_value_step"]fn builtin_value_step(cursor:BuiltinValueCursor,facts:BuiltinValueFacts,bytes:*const u8,length:u64)->BuiltinValueStep;
 #[link_name="bootstrap.operator_types::function_from_address_step"]fn function_from_address_step(cursor:FunctionFromAddressCursor,facts:FunctionFromAddressFacts)->FunctionFromAddressStep;
}


#[repr(C)]#[derive(Clone,Copy)]struct SourceValueCursor {phase:u32}
#[repr(C)]struct StageRootFacts {present:u32,constant:u32}
#[repr(u32)]enum StageRootNeed {Binding,Bound,Scope,ScopeValue,Resolve,FunctionKind,Constant,Unknown,StaticU32,Number,Expression}
#[repr(C)]struct StageRootStep {cursor:SourceValueCursor,need:StageRootNeed,spelling:crate::syntax::wire::Text,bits:u32}
unsafe extern "C" {#[link_name="bootstrap.function_body::stage_root_step"]fn stage_root_step(cursor:SourceValueCursor,source:crate::syntax::wire::SignatureNode,facts:StageRootFacts)->StageRootStep;}
#[repr(u32)]enum SourceValueNeed {BadFunction,BadCooperate,StaticU32,Bool,Number,Name,Specialized,Member,Index,Select,Target,ConstructType,Construct,Returned,FinishReturned,Negative,Operand,Unary,Left,Right,Binary,Condition,RequireCondition,ShortCircuit,Empty}
#[repr(C)]struct SourceValueStep {cursor:SourceValueCursor,need:SourceValueNeed,spelling:crate::syntax::wire::Text,first:u64,second:u64,third:u64,arguments:crate::syntax::wire::Slice,bits:u32,target:Target}
#[repr(u32)]enum ReturnedValueNeed {Empty,Ordinary,Value,Bool,Complete}
#[repr(C)]struct ReturnedValueStep {cursor:SourceValueCursor,need:ReturnedValueNeed}
#[repr(C)]struct SourceOperation {data:*const u8,length:u64}
#[repr(u32)]enum ShortCircuitOrder {RightYes,RightNo}
#[repr(C)]struct ShortCircuitPlan {order:ShortCircuitOrder,skipped:u32}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::source_value_step"]fn source_value_step(cursor:SourceValueCursor,source:crate::syntax::wire::SignatureNode,present:u32)->SourceValueStep;
 #[link_name="bootstrap.function_body::returned_value_step"]fn returned_value_step(cursor:SourceValueCursor,continuing:u32,boolean:u32)->ReturnedValueStep;
 #[link_name="bootstrap.function_body::short_circuit_plan"]fn short_circuit_plan(operation:SourceOperation)->ShortCircuitPlan;
}


#[repr(u32)]enum SourcePlaceNeed {Name,Bind,External,Parent,Project,Index,Bad,Empty}
#[repr(C)]struct SourcePlaceStep {cursor:SourceValueCursor,need:SourcePlaceNeed,spelling:crate::syntax::wire::Text,first:u64,second:u64,types:crate::syntax::wire::Slice,arguments:crate::syntax::wire::Slice}
#[repr(u32)]enum MemberPositionNeed {Project,BadView}
#[repr(u32)]enum MemberValueNeed {ArrayLength,ViewLength,Project}
#[repr(C)]struct NamePositionFacts {kind:u32,present:u32,arrays:u32}
#[repr(u32)]enum NamePositionNeed {Atomic,Owned,ViewElement,InspectOwned,ExistingOwned,MaterializeOwned,ArrayType,MaterializeArray,Bad}
#[repr(C)]struct NamePositionStep {cursor:SourceValueCursor,need:NamePositionNeed}
#[repr(C)]struct IndexPositionFacts {present:u32,bounded:u32,current:u32,same_domain:u32,same_shape:u32,access:u32,whole:u32,appending:u32}
#[repr(u32)]enum IndexPositionNeed {Application,ApplicationIndex,TileBinding,TileIndex,Dataset,View,Index,Type,Bounds,BadBounds,Append,BadAppend,Domain,Element,Empty}
#[repr(C)]struct IndexPositionStep {cursor:SourceValueCursor,need:IndexPositionNeed,name:crate::syntax::wire::Text,writable:u32}
#[repr(C)]struct PositionAppend {present:u32,output:u64,count:u64,word:u32}
#[repr(C)]struct ViewPositionFacts {placed:u32,present:u32,dataset:u32,indexed:u32,continuing:u32,kind:u32,array:u32}
#[repr(u32)]enum ViewPositionNeed {Parent,DatasetType,IndexedType,Reference,ApplicationIndex,Read,Returned,Kind,OwnedArray,HoldDataset,HoldOwned,Ordinary,ArrayType,Materialize,ArrayPlace,View,Empty}
#[repr(C)]struct ViewPositionStep {cursor:SourceValueCursor,need:ViewPositionNeed}
type PositionAppendIter<'a>=std::collections::btree_map::Iter<'a,usize,PrefixCount>;
unsafe extern "C" fn next_position_append(entries:*mut PositionAppendIter<'_>)->PositionAppend {
 match unsafe {&mut *entries}.next(){Some((output,count))=>PositionAppend {present:1,output:*output as u64,count:count.binding as u64,word:count.word},None=>PositionAppend {present:0,output:0,count:0,word:0}}
}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::source_place_step"]fn source_place_step(cursor:SourceValueCursor,source:crate::syntax::wire::SignatureNode,present:u32)->SourcePlaceStep;
 #[link_name="bootstrap.function_body::member_position"]fn member_position(view:u32)->MemberPositionNeed;
 #[link_name="bootstrap.function_body::member_value_need"]fn member_value_need(array:u32,view:u32,bytes:*const u8,length:usize)->MemberValueNeed;
 #[link_name="bootstrap.function_body::name_position_step"]fn name_position_step(cursor:SourceValueCursor,facts:NamePositionFacts)->NamePositionStep;
 #[link_name="bootstrap.function_body::index_position_step"]fn index_position_step(cursor:SourceValueCursor,source:crate::syntax::wire::SignatureNode,facts:IndexPositionFacts)->IndexPositionStep;
 #[link_name="bootstrap.function_body::position_append"]fn position_append(binding:u64,context:*mut PositionAppendIter<'_>,next:unsafe extern "C" fn(*mut PositionAppendIter<'_>)->PositionAppend)->u32;
 #[link_name="bootstrap.function_body::view_position_step"]fn view_position_step(cursor:SourceValueCursor,facts:ViewPositionFacts)->ViewPositionStep;
}


#[repr(C)]struct LexicalCursor {phase:u32,flags:u32,index:u64,part:u64}
#[repr(u32)]enum LexicalNeed {Enter,Statement,After,Finish,Release,Retain,Complete,Next}
#[repr(C)]struct LexicalStep {cursor:LexicalCursor,need:LexicalNeed}
#[repr(C)]struct SourceStatementFacts {present:u32,entry:u32}
#[repr(u32)]enum SourceStatementNeed {Bind,Declare,Let,Assign,Expression,Condition,RequireBool,Conditional,InitialLoop,While,For,LoopControl,Match,Return,Parallel,Entry,BadPhase,Seek,Complete}
#[repr(C)]struct SourceStatementStep {cursor:SourceValueCursor,need:SourceStatementNeed,name:crate::syntax::wire::Text,operation:crate::syntax::wire::Text,first:u64,second:u64,body:crate::syntax::wire::Slice,other:crate::syntax::wire::Slice,present:u32}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::start"]fn lexical_start()->LexicalCursor;
 #[link_name="bootstrap.function_body::lexical_block_step"]fn lexical_block_step(cursor:LexicalCursor,count:u64,continuing:u32)->LexicalStep;
 #[link_name="bootstrap.function_body::source_statement_step"]fn source_statement_step(cursor:SourceValueCursor,source:crate::syntax::wire::SignatureNode,facts:SourceStatementFacts)->SourceStatementStep;
}


fn named_binding_kind(binding:Option<&Binding>)->u32 {
    match binding {None=>0,Some(Binding::AtomicBorrow(_))=>1,Some(Binding::Owned(_))=>2,Some(Binding::OwnedValue(_))=>3,Some(Binding::Value(_))=>4,Some(Binding::ViewElement {..})=>5,Some(_)=>6}
}
#[repr(C)]struct ReadPlaceFacts {owns:u32,kind:u32,access:u32,reference:u32,memory:u32,known:u32,input:u32,current:u32,field_address:u32,components:u64,words:u64,fields:u64}
#[repr(u32)]enum ReadPlaceNeed {Access,Owns,Kind,OwnedError,AtomicError,AtomicTileError,AtomicDataError,Array,ReferenceMemory,Address,RememberRead,Memory,Initialize,Zero,LoadReference,LoadView,LoadTile,LoadLocal,LoadData,Proof,Snapshot,BorrowFields,BorrowField,SetBorrow,ExternalProof,DatasetEffect,ApplicationRead,ApplicationProof,TileInput,DataAccess,DataCross,FinishApplication,Finish,ShareBorrow}
#[repr(C)]struct ReadPlaceStep {cursor:LexicalCursor,need:ReadPlaceNeed,flags:u32}
#[repr(C)]struct NameValueFacts {present:u32,kind:u32,constant:u32}
#[repr(u32)]enum NameValueNeed {Scope,ScopeValue,Binding,Value,OwnedPlace,NamedPlace,Read,Resolve,FunctionKind,ModuleConstant,Unknown}
#[repr(C)]struct NameValueStep {cursor:SourceValueCursor,need:NameValueNeed}
struct SourcePlaceQuery<'a,'b,'c> {builder:&'a Builder<'b>,source:crate::syntax::SourceView<'c>}
#[repr(C)]struct SourcePlaceQueryHost<'a,'b,'c> {context:*const SourcePlaceQuery<'a,'b,'c>,binding:unsafe extern "C" fn(*const SourcePlaceQuery<'a,'b,'c>,crate::syntax::wire::Text)->u32}
unsafe extern "C" fn query_named_binding(context:*const SourcePlaceQuery<'_,'_,'_>,name:crate::syntax::wire::Text)->u32 {
    let context=unsafe {&*context};named_binding_kind(context.builder.env.get(context.source.text(name)))
}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::read_place_step"]fn read_place_step(cursor:LexicalCursor,facts:ReadPlaceFacts)->ReadPlaceStep;
 #[link_name="bootstrap.function_body::name_value_step"]fn name_value_step(cursor:SourceValueCursor,facts:NameValueFacts)->NameValueStep;
 #[link_name="bootstrap.function_body::source_is_place"]fn source_is_place(source:crate::syntax::wire::SignatureNode,host:SourcePlaceQueryHost<'_,'_,'_>)->u32;
}

#[repr(C)]struct WritePlaceFacts {views:u32,persistent:u32,kind:u32,access:u32,writable:u32,memory:u32,pending:u32,resolved:u32,field_address:u32,unique:u32,iterator:u32,loops_empty:u32,words:u64,values:u64,fields:u64}
#[repr(u32)]enum WritePlaceNeed {Access,ViewType,ViewPayloadError,Owned,ViewAvailable,Same,Kind,NumericProof,ReferenceMemory,Pending,Resolved,Snapshot,AddressWrite,BorrowFields,BorrowField,AddField,Memory,AtomicError,AtomicTileError,AtomicDataError,Array,ReadOnlyError,ViewError,TileError,WriteError,Unique,DuplicateError,Writable,AssignProof,Initialize,Zero,DatasetEffect,CaptureProof,Alias,ApplicationWrite,AssignApplicationProof,CompleteWrites,DataAccess,StoreReference,StoreLocal,StoreData,Complete}
#[repr(C)]struct WritePlaceStep {cursor:LexicalCursor,need:WritePlaceNeed}
unsafe extern "C" {#[link_name="bootstrap.function_body::write_place_step"]fn write_place_step(cursor:LexicalCursor,facts:WritePlaceFacts)->WritePlaceStep;}

#[repr(C)]struct UnsafeReadFacts {present:u32,access:u32}
#[repr(u32)]enum UnsafeReadNeed {BadArguments,Application,ApplicationCpu,Dataset,BadAccess,Value,Index,IndexType,ApplicationRead,DatasetRead,ViewRead,Empty}
#[repr(C)]struct UnsafeReadStep {cursor:SourceValueCursor,need:UnsafeReadNeed}
#[repr(C)]struct UnsafeViewFacts {view:u32,present:u32}
#[repr(u32)]enum UnsafeViewNeed {BadView,Hold,Index,Release,Result,IndexType,Read,Empty}
#[repr(C)]struct UnsafeViewStep {cursor:SourceValueCursor,need:UnsafeViewNeed}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::unsafe_read_step"]fn unsafe_read_step(cursor:SourceValueCursor,source:crate::syntax::wire::StorageView,arguments:crate::syntax::wire::Slice,facts:UnsafeReadFacts)->UnsafeReadStep;
 #[link_name="bootstrap.function_body::unsafe_view_step"]fn unsafe_view_step(cursor:SourceValueCursor,facts:UnsafeViewFacts)->UnsafeViewStep;
}

#[repr(C)]struct OrdinalFacts {binding:u32,place:u32,minimum:u32,maximum:u32}
#[repr(u32)]enum OrdinalNeed {BadArguments,BadName,Binding,BadCursor,Application,Dataset,Tile,View}
#[repr(C)]struct OrdinalStep {cursor:SourceValueCursor,need:OrdinalNeed,name:crate::syntax::wire::Text,minimum:u32,maximum:u32}
unsafe extern "C" {#[link_name="bootstrap.function_body::ordinal_step"]fn ordinal_step(cursor:SourceValueCursor,source:crate::syntax::wire::StorageView,arguments:crate::syntax::wire::Slice,facts:OrdinalFacts)->OrdinalStep;}

#[repr(C)]struct RayQueryFacts{count:u64,any:u32,present:u32}
#[repr(u32)]enum RayQueryNeed{BadArguments,Name,MissingName,Binding,BadSource,Origin,Minimum,Direction,Maximum,Empty,OriginType,MinimumType,DirectionType,MaximumType,Cull,CullType,AnyType,ClosestType,Registers,AnyResult,ClosestResult,Emit,Value}
#[repr(C)]struct RayQueryStep{cursor:SourceValueCursor,need:RayQueryNeed}
unsafe extern "C"{#[link_name="bootstrap.function_body::ray_query_step"]fn ray_query_step(cursor:SourceValueCursor,facts:RayQueryFacts)->RayQueryStep;}
