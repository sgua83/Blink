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
    element_before: Vec<ElementFact>,
    element_after: Vec<ElementFact>,
    source: Option<Arc<SourceSite>>,
}
struct RangeContext {
    output_scopes: usize,
    view_model: view_contracts::Function,
    residency: residency::Inputs,
}

struct ExpandedDomain {
    program: Program,
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
        world.call(expand_domain(world, input, &parameters, Some(&mut uniform_types), &mut functions, &[], None)).await
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
    element_inputs: &[ElementFact],
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
        DomainInput::Parallel { length_requirements, prefix_requirements, .. } => {
            program.length_requirements.extend_from_slice(length_requirements);
            program.prefix_requirements.extend_from_slice(prefix_requirements);
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
    program.element_requirements.extend_from_slice(element_inputs);
    program.element_requirements.sort();
    program.element_requirements.dedup();
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
    program.element_guarantees = program.element_outputs(facts.element_facts(&program));
    Ok((ExpandedDomain { program, scopes, calls, outputs: facts.outputs, elements: facts.elements }, bound_contracts))
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
                        let output = append::clear(program, env, &args, d, statement.span(), facts,
                            input.operation_source(call_span, SourceSyntax::Expression, domain_path))?;
                        facts.outputs.push(output);
                        *boundary = false;
                    }
                    ExprKind::Call(name, args) if name == "stable_sort_by_key"
                        && world.resolve(name, &d.module, &d.imports, true).is_missing() =>
                    {
                        sort::compile(program, env, &args, d, statement.span(), facts, input.operation_source(call_span, SourceSyntax::Expression, domain_path))?;
                        *boundary = false;
                    }
                    ExprKind::Call(name, args) if name == "compact"
                        && world.resolve(name, &d.module, &d.imports, true).is_missing() =>
                    {
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
                        if callee_key.is_missing() {
                            if let Some(operator) = CollectiveOperator::from_reduction_source(name)
                                .filter(|_| explicit.is_none())
                            {
                                reduce::compile(program, env, operator, &args, d, statement.span(), facts, input.operation_source(call_span, SourceSyntax::Expression, domain_path))?;
                                *boundary = false;
                                break 'call;
                            }
                            if let Some(kind) =
                                custom_reduce::Kind::from_source(name).filter(|_| explicit.is_none())
                            {
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
                            if let Some(operator) =
                                CollectiveOperator::from_segmented_reduction_source(name)
                                    .filter(|_| explicit.is_none())
                            {
                                segmented_reduce::compile(
                                    program,
                                    env,
                                    operator,
                                    &args,
                                    d,
                                    statement.span(),
                                    facts,
                                    input.operation_source(call_span, SourceSyntax::Expression, domain_path),
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
                                    &args,
                                    d,
                                    statement.span(),
                                    facts,
                                    input.operation_source(call_span, SourceSyntax::Expression, domain_path),
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
                                    &args,
                                    d,
                                    statement.span(),
                                    facts,
                                    input.operation_source(call_span, SourceSyntax::Expression, domain_path),
                                )?;
                                *boundary = false;
                                break 'call;
                            }
                        }
                        let Some(key) = callee_key.unique() else {
                            return d
                                .source
                                .fail(statement.span(), "B109", format!("未知计算程序 {name}"));
                        };
                        let callee = &world.functions[&key];
                        if callee.execution != crate::execution::Requirement::Domain {
                            return d.source.fail(
                                statement.span(),
                                "B110",
                                "program 顶层只能调用计算程序或声明 parallel 阶段",
                            );
                        }
                        if args.len() != callee.function.params().len() {
                            return d
                                .source
                                .fail(statement.span(), "B111", "计算程序参数数量不匹配");
                        }
                        if program.target != callee.function.target()
                            && callee.function.target() != Target::Compute
                        {
                            return d
                                .source
                                .fail(statement.span(), "B112", "程序与子程序的设备目标不兼容");
                        }
                        let mut roots=Vec::new();
                        for arg in args.iter() {
                            roots.push(unsafe { world.call(stage_root(world,instance,env,program,&arg,domain_path)).await }?);
                        }
                        let actuals: Vec<_> = roots
                            .iter()
                            .map(|root| match root {
                                Root::Data(i, _) => generics::Actual {
                                    ty: Some(std::borrow::Cow::Borrowed(&program.buffers[*i].ty)),
                                    count: program.buffers[*i].count,
                                },
                                Root::Uniform(i) => generics::Actual::value(&program.uniforms[*i].ty),
                                Root::Literal(ty, ..) => generics::Actual::value(ty),
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
                        let explicit=match explicit {
                            Some(arguments)=>Some(unsafe { world.call(instance.bindings.arguments(world,arguments,
                                (&d.source,&d.module,&d.imports),statement.span())).await }?),
                            None=>None,
                        };
                        let instance =
                            unsafe { world.call(generics::specialize(world, &key, &actuals, explicit.as_deref(), (&d.source, &d.module, &d.imports), statement.span())).await }?;
                        let callee = &instance.definition;
                        let mut child = BTreeMap::new();
                        for ((p, arg), root) in instance.parameters(world).zip(args).zip(roots) {
                            let p=unsafe { world.call(p).await }?;
                            let expected = async || {
                                unsafe { world.call(world.ty(
                                    &p.ty,
                                    &callee.source,
                                    p.span,
                                    &callee.module,
                                    &callee.imports,
                                )).await }
                            };
                            let actual_value = match &root {
                                Root::Texture(i) => {
                                    let binding = &program.textures[*i];
                                    if p.access != Access::Read || !unsafe {
                                        world.call(texture::compatible(
                                            world, &p, binding, binding.count, (&callee.source, &callee.module, &callee.imports),
                                        )).await
                                    }? {
                                        return d.source.fail(
                                            arg.span(),
                                            "B239",
                                            "纹理资源与只读纹理参数的维度／形状／格式不匹配",
                                        );
                                    }
                                    None
                                }
                                Root::Acceleration(_) => {
                                    if p.ty.name != "acceleration"
                                        || p.ty.dataset
                                        || p.access != Access::Read
                                    {
                                        return d.source.fail(
                                            arg.span(),
                                            "B182",
                                            "加速资源的类型或访问权限不匹配",
                                        );
                                    }
                                    None
                                }
                                Root::Data(i, access) => {
                                    let ty = expected().await?;
                                    if !p.ty.dataset || ty != program.buffers[*i].ty {
                                        return d.source.fail(arg.span(), "B115", "数据集类型不匹配");
                                    }
                                    if p.ty.count.is_some()
                                        && unsafe { world.call(world.shape(
                                            &p.ty.count,
                                            &callee.source,
                                            p.span,
                                            &callee.module,
                                            &callee.imports,
                                        )).await }? != program.buffers[*i].count
                                    {
                                        return d.source.fail(
                                            arg.span(),
                                            "B167",
                                            "调用方没有声明子程序需要的数据形状",
                                        );
                                    }
                                    if (p.access.reads() && !access.reads())
                                        || (p.access.writes() && !access.writes())
                                    {
                                        return d.source.fail(
                                            arg.span(),
                                            "B116",
                                            "子程序请求了调用方没有的数据访问权限",
                                        );
                                    }
                                    None
                                }
                                Root::Uniform(i) => Some(&program.uniforms[*i].ty),
                                Root::Literal(ty, ..) => Some(ty),
                            };
                            if let Some(actual) = actual_value {
                                let ty = expected().await?;
                                if p.ty.dataset || *actual != ty {
                                    return d.source.fail(arg.span(), "B117", "标量参数类型不匹配");
                                }
                            }
                            child.insert(
                                p.name.to_owned(),
                                if let Root::Data(i, _) = root {
                                    Root::Data(i, p.access)
                                } else {
                                    root
                                },
                            );
                        }
                        let mut used = BTreeMap::new();
                        for p in &callee.function.params() {
                            if let Some(Root::Data(i, _)) = child.get(p.name()) {
                                if let Some(previous) = used.insert(*i, p.access()) {
                                    if (previous.writes() || p.access().writes())
                                        && !(previous == Access::Atomic && p.access() == Access::Atomic)
                                    {
                                        return d.source.fail(
                                            statement.span(),
                                            "B118",
                                            "同一数据集通过多个参数别名进入可写子程序",
                                        );
                                    }
                                }
                            }
                        }
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
                            let before = facts.relations();
                            let element_before = facts.element_facts(program);
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
                                stage: program.stages.len(),
                                instance: instance.clone(),
                                arguments: callee
                                    .function
                                    .params()
                                    .iter()
                                    .map(|p| child[p.name()].clone())
                                    .collect(),
                                write_regions,
                                element_before,
                                element_after: Vec::new(),
                                before,
                                after: Vec::new(),
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
                        if compiled {
                            calls.last_mut().unwrap().after = facts.relations();
                            calls.last_mut().unwrap().element_after = facts.element_facts(program);
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
    match arg.kind() {
        ExprKind::Name(n) => {
            if let Some(root) = env.get(n) {
                Ok(root.clone().reference_literal(operation_source(instance, arg.span(), SourceSyntax::Expression, domain_path)))
            } else if let Some(value) = instance.bindings.values.get(n) {
                Ok(Root::Literal(Ty::scalar(Scalar::U32), vec![Bits::from(*value)], None).reference_literal(operation_source(instance, arg.span(), SourceSyntax::Expression, domain_path)))
            } else if let Some(key) = world
                .resolve(n, &d.module, &d.imports, true).unique()
                .filter(|key| world.functions[key].function.kind() == FunctionKind::Constant)
            {
                let constant = unsafe { world.call(constant::evaluate(world, &key, &[])).await }?;
                let (ty, words, origin) = constant.value.clone();
                Ok(Root::Literal(ty, words, origin).reference_literal(operation_source(instance, arg.span(), SourceSyntax::Expression, domain_path)))
            } else {
                d.source
                    .fail(arg.span(), "B113", format!("未知参数或 const {n}"))
            }
        }
        ExprKind::StaticU32(bits) => Ok(Root::Literal(Ty::scalar(Scalar::U32), vec![Bits::from(bits)], None)),
        ExprKind::Number(n) => {
            let (ty, bits) = number(&d.source, arg.span(), n)?;
            Ok(Root::Literal(ty, vec![bits], None))
        }
        _ => unsafe { world.call(constant::stage_expression(world, instance, env, program, arg, None, domain_path)).await },
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
    let result=if result_name.name=="void"||result_name.is_never(){None}else{Some(unsafe { world.call(b.expected_storage(&result_name,d.function.span())).await }?)};
    function_inputs::return_inputs(&mut b, d.function.c_abi(), result.as_ref(), &mut parameters, &mut inputs);
    let (returned,written) = unsafe { world.call(b.function_body(instance, d.function.body().as_ref().unwrap())).await }?;
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
    iterators: Vec<Option<(String, usize)>>,
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
    async fn block(&mut self, body: &Block) -> Result<()> {
        let original = self.env.keys().cloned().collect::<BTreeSet<_>>();
        self.block_depth += 1;
        let mut flow = Flow::NEXT;
        for s in body {
            if !flow.continues() {
                break;
            }
            self.flow = Flow::NEXT;
            unsafe { self.world.call(self.statement(&s)).await }?;
            self.view_uses.after(s.span());
            flow = flow.then(self.flow);
        }
        self.flow = flow;
        self.block_depth -= 1;
        if flow.continues() {
            unsafe { self.world.call(self.release_application_data(Some(&original),view::Borrow::default())).await }?;
        }
        self.env.retain(|k, _| original.contains(k));
        Ok(())
    }
    async fn statement(&mut self, s: &Stmt) -> Result<()> {
        let world = self.world;
        unsafe { world.call(self.at_source(s.span(), SourceSyntax::Statement,
            async |b| unsafe { world.call(b.temporary_scope(s.span(), async |b| {
                unsafe { world.call(b.statement_syntax(s)) .await }?;
                Ok(((),view::Borrow::default()))
            })) .await })) .await }
    }
    async fn statement_syntax(&mut self, s: &Stmt) -> Result<()> {
        self.view_uses.before(s.span());
        match s.kind() {
            StmtKind::Declare(annotation, name) => {
                let annotation=unsafe { self.world.call(self.scope.bindings().type_name(self.world,&annotation,
                    (self.scope.source(),self.scope.module(),self.scope.imports()),s.span())).await }?;
                return unsafe { self.world.call(self.declare_statement(&annotation, name, s.span())).await };
            }
            StmtKind::Let(annotation, name, e) => {
                let annotation=match annotation.as_ref() {
                    Some(annotation)=>Some(unsafe { self.world.call(self.scope.bindings().type_name(self.world,annotation,
                        (self.scope.source(),self.scope.module(),self.scope.imports()),s.span())).await }?),
                    None=>None,
                };
                return unsafe { self.world.call(self.let_statement(&annotation, name, &e, s.span())).await };
            }
            StmtKind::Assign(lhs, op, rhs) => return unsafe { self.world.call(self.assign_statement(&lhs, op, &rhs, s.span())).await },
            StmtKind::Expr(e) => return unsafe { self.world.call(self.expression_statement(&e)).await },
            StmtKind::If(condition, yes_body, no_body) => {
                return unsafe { self.world.call(self.if_statement(&condition, &yes_body, &no_body, s.span())).await };
            }
            StmtKind::While(condition, body) => {
                return unsafe { self.world.call(self.while_statement(&condition, &body, s.span())).await };
            }
            StmtKind::For(name, start, end, body) => {
                return unsafe { self.world.call(self.for_statement(name, &start, &end, &body, s.span())).await };
            }
            StmtKind::Break | StmtKind::Continue => return unsafe { self.world.call(self.loop_control_statement(s)).await },
            StmtKind::Match(value, arms) => { return unsafe { self.world.call(self.match_returned(&value,&arms,s.span())).await }; }
            StmtKind::Return(value) => unsafe { self.world.call(self.return_statement(value.as_ref(), s.span())).await }?,
            StmtKind::Parallel(element,domain,body) => return unsafe { self.world.call(self.application_parallel(s,element,&domain,&body)).await },
            StmtKind::Phase => {
                if !self.application_entry() {
                    return self.fail(s.span(), "B163", "phase 需要整域同步位置，不能位于元素实例中");
                }
            }
            StmtKind::Seek(name, position) => return unsafe { self.world.call(self.seek_statement(name, &position, s.span())).await },
        }
        Ok(())
    }
    // 循环出口的资源交接只在本语句分析时存活。
    #[inline(never)]
    async fn loop_control_statement(&mut self, s: &Stmt) -> Result<()> {
        let breaking = matches!(s.kind(), StmtKind::Break);
        if self.loop_depth == 0 {
            return self.fail(s.span(), "B129", if breaking {
                "break 只能用于局部顺序循环"
            } else {
                "continue 只能用于局部顺序循环"
            });
        }
        unsafe { self.world.call(self.release_application_loop()).await }?;
        if breaking {
            self.emit(InstKind::Break);
            self.capture_break();
            self.flow = Flow::BREAK;
        } else {
            if let Some(Some((_, slot))) = self.iterators.last().cloned() {
                self.advance_iterator(slot)?;
            }
            unsafe { self.world.call(self.check_application_loop(s.span())).await }?;
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
    async fn place(&mut self, e: &Expr) -> Result<Option<Place>> {
        unsafe { self.world.call(self.at_source(e.span(), SourceSyntax::Place, async |b| unsafe { b.world.call(b.place_syntax(e)).await })).await }
    }
    async fn place_syntax(&mut self, e: &Expr) -> Result<Option<Place>> {
        self.require_user_projection(e)?;
        self.view_uses.before(e.span());
        match e.kind() {
            ExprKind::Name(name) => self.name_place(name, e.span()).map(Some),
            ExprKind::Specialize(name, explicit, args) if name == "unsafe_position" => {
                {
                let explicit=unsafe { self.world.call(self.scope.bindings().arguments(self.world,&explicit,
                    (self.scope.source(),self.scope.module(),self.scope.imports()),e.span())).await }?;
                unsafe { self.world.call(self.external_position(&explicit, &args, e.span())).await }
            }
            }
            ExprKind::Member(base, field) => unsafe { self.world.call(self.member_place(&base, field, e)).await },
            ExprKind::Index(base, index) => unsafe { self.world.call(self.index_place(&base, &index)).await },
            _ => self.fail(e.span(), "B138", "表达式不是可赋值位置"),
        }
    }
    async fn read_place(&mut self, p: &Place, span: Span, element_read_origin: Option<Arc<GeneratedOrigin>>) -> Result<Value> {
        unsafe { self.world.call(self.require_place_access(p,Access::Read,span)).await }?;
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
                let mut content_read=None;
                if let Some((target,borrow))=unsafe { self.world.call(self.reference_memory(p,span)).await }? {
                    self.check_view_requirement(view::Requirement::Address{borrow:borrow.clone(),layout:address::Layout::of(ty),written:None,writable:false,initialized:true,span})?;
                    content_read=Some((self.view_model.uses.len(),Arc::new(borrow.clone())));
                    self.check_view_requirement(view::Requirement::Memory{target,borrow,access:Some(Access::Read),written:None,snapshots:None,span,address_fields:Arc::from([])})?;
                }else{unsafe { self.world.call(self.require_initialized(p, span)).await }?;}
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
                let (mut proof, uniform, known) = unsafe { self.world.call(self.read_owned_proof(p, &regs, &[])).await }?;
                if !known && matches!(address,ReferenceAddress::Borrowed{..}) && let Some((memory,borrow))=content_read {
                    let snapshots:Arc<[view_contracts::ValueSnapshot]>=self.capture_address_values(memory,address::Capture::Read).into();
                    if let view::Requirement::Memory{snapshots:captured,..}=&mut self.view_model.uses[memory].requirement{*captured=Some(snapshots.clone());}
                    for (word,field) in ty.borrow_fields(){
                        if !field.is_address(){continue;}
                        let component=ty.components.iter().position(|(_,at)|*at==word).unwrap();
                        proof.set_view_borrow(component,view::Borrow{reads:view::Edges::from([view_contracts::ContentRead::Value{memory,word,borrow:borrow.clone(),snapshots:snapshots.clone()}]),..view::Borrow::default()});
                    }
                }
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
                unsafe { self.world.call(self.require_initialized(p, span)).await }?;
                let reads=self.dataset_effect(p,Access::Read,false,span)?;
                let mut value = self
                    .read_application_place(ty, *handle, *identity, *index, *stride, words, span, element_read_origin)?;
                let (proof, uniform, _) = unsafe { self.world.call(self.read_owned_proof(p, &value.regs, &reads)).await }?;
                value.constant = proof.known_value(ty);
                value.proof = proof;
                value.uniform = uniform;
                Ok(value)
            }
            Place::Indexed { .. } => unsafe { self.world.call(self.read_array_place(p, span)).await },
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
                let (proof, uniform, _) = unsafe { self.world.call(self.read_owned_proof(p, &regs, &[])).await }?;
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
                if let TileOrigin::Input { binding, .. } =
                    &self.tiles[*tile].source
                {
                    self.reads.insert(*binding);
                    self.cross_access.insert(*binding);
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
                let (proof, uniform, _) = unsafe { self.world.call(self.read_owned_proof(p, &regs, &[])).await }?;
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
                    unsafe { self.world.call(self.require_initialized(p, span)).await }?;
                }
                let mut regs = Vec::new();
                for (s, slot) in ty.components.iter().zip(slots) {
                    let r = self.reg(s.0);
                    self.emit(InstKind::LoadLocal(r, *slot));
                    regs.push(r);
                }
                let (proof, uniform, _) = unsafe { self.world.call(self.read_owned_proof(p, &regs, &[])).await }?;
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
                    unsafe { self.world.call(self.require_initialized(p, span)).await }?;
                }
                self.data_access(p, Access::Read, None);
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
                let proof = unsafe { self.world.call(self.read_owned_proof(p, &regs, &[])).await }?.0;
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
    async fn write_place(&mut self, p: &Place, value: Value, span: Span, local_origin: Option<Arc<GeneratedOrigin>>) -> Result<()> {
        unsafe { self.world.call(self.require_place_access(p,Access::Write,span)).await }?;
        if p.ty().contains_views() && matches!(p.storage(), Place::Element { .. } | Place::ApplicationElement { .. }) {
            return self.fail(span,"B185","持久dataset的view载荷不可写或独占借出；数值字段按原权限修改");
        }
        self.require_owned_writable(p,span)?;
        self.require_view_available(&value, &[], span)?;
        let ty = p.ty();
        self.require_same(ty, &value.ty, span)?;
        let stored=bounds::Proof::capture(&value);
        let written=if matches!(p,Place::Reference{..}){self.path_proof(ty,bounds::Proof::capture(&value)).numeric_facts()}else{bounds::Proof::default()};
        if let Some((target,borrow))=unsafe { self.world.call(self.reference_memory(p,span)).await }? {
            let memory=self.view_model.uses.len();
            let snapshots=if borrow.pending()&&!self.resolved_address_write(&borrow){Some(self.capture_address_values(memory,address::Capture::Write).into())}else{unsafe { self.world.call(self.write_address_values(&borrow,&[(borrow.clone(),Some(Access::Write),Some(written.clone()))])).await }?;None};
            self.check_view_requirement(view::Requirement::Memory{target,borrow,access:Some(Access::Write),written:Some(stored.clone()),snapshots,span,address_fields:p.ty().borrow_fields().into_iter().filter(|(_,field)|field.is_address()).map(|(word,_)|(word,p.ty().components.iter().position(|(_,at)|*at==word).unwrap())).collect()})?;
        }
        match p {
            Place::AtomicParameter(_)
            | Place::Reference { access: Access::Atomic, .. }
            | Place::ApplicationElement { access: Access::Atomic, .. } => {
                return self.fail(span, "B214", "原子借用只能通过显式原子操作修改");
            }
            Place::Reference { address, words, .. } => {
                unsafe { self.world.call(self.writable_storage(p, span, Some(written.clone()))).await }?;
                unsafe { self.world.call(self.assign_owned_proof(p, bounds::Proof::capture(&value), false)).await }?;
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
                sources,
                index,
                stride,
                words,
                ..
            } => {
                unsafe { self.world.call(self.writable_storage(p, span, Some(written))).await }?;
                self.dataset_effect(p,Access::Write,false,span)?;
                let proof = bounds::Proof::capture(&value);
                if self.iterators.last().is_some_and(Option::is_some) {
                    self.path = bounds::Clause::both(&self.path, &proof.output_aliases(&value.regs, &self.registers, &BTreeMap::new()));
                }
                self.write_application_place(
                    *handle, *identity, *index, *stride, words, value, span, None,
                )?;
                self.initialize(p);
                unsafe { self.world.call(self.assign_owned_proof(p, proof, false)).await }?;
                if self.loop_contexts.is_empty() {
                    unsafe { self.world.call(self.complete_application_writes(|_,data| sources.contains(&data.root.identity()), span)).await }?;
                }
                return Ok(());
            }
            Place::Indexed { .. } => return unsafe { self.world.call(self.write_array_place(p, value, span)).await },
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
                unsafe { self.world.call(self.assign_owned_proof(p, bounds::Proof::capture(&value), false)).await }?;
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
                self.data_access(p, Access::Write, None);
                self.writes.insert(*binding);
                unsafe { self.world.call(self.assign_owned_proof(p, bounds::Proof::capture(&value), false)).await }?;
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
    async fn expr(&mut self, e: &Expr) -> Result<Option<Value>> {
        let world = self.world;
        let mut value = unsafe { world.call(self.at_source(e.span(), SourceSyntax::Expression,
            async |b| unsafe { world.call(b.expr_syntax(e)) .await })) .await }?;
        if let Some(value) = &mut value { self.bound_value(value); }
        Ok(value)
    }
    async fn expr_syntax(&mut self, e: &Expr) -> Result<Option<Value>> {
        let world = self.world;
        self.view_uses.before(e.span());
        match e.kind() {
            ExprKind::FunctionRef(..) => self.fail(
                e.span(),
                "B236",
                "静态函数引用只能作为集合算法参数或 bind 的首参数，不能作为普通值",
            ),
            ExprKind::Cooperate(..) => {
                self.fail(e.span(), "B208", "cooperate 调用只能作为协作根块的局部声明")
            }
            ExprKind::StaticU32(bits) => self.constant_expression(Scalar::U32, Bits::from(bits)),
            ExprKind::Specialize(..) =>
                unsafe { world.call(source_call::specialized(self,e)).await },
            ExprKind::Number(n) => self.number_expression(n, e.span()),
            ExprKind::Bool(b) => self.constant_expression(Scalar::Bool, Bits::from(b)),
            ExprKind::Name(name) => unsafe { world.call(self.name_expr(name, e.span())) .await }.map(Some),
            ExprKind::Member(base, field) => {
                let result = unsafe { world.call(self.member_returned(e, &base, field)) .await }?;
                self.returned_expression(result, e.span())
            }
            ExprKind::Index(base, index) => unsafe { world.call(self.index_expr(&base, &index, e)) .await },
            ExprKind::Unary(op, a) => unsafe { world.call(self.unary_expr(op, &a, e.span())) .await },
            ExprKind::Binary(op, a, b) => unsafe { world.call(self.binary_expr(op, &a, &b, e.span())) .await },
            ExprKind::Select(c, a, b) => unsafe { world.call(self.selection(&c, &a, &b, e.span())) .await },
            ExprKind::Target(target, call) => {
                let result = unsafe { world.call(self.target_call(target, &call, e.span())) .await }?;
                self.returned_expression(result, e.span())
            }
            ExprKind::Construct(ty, args) => {
                let ty = unsafe { world.call(self.scope.bindings().type_name(world, &ty,
                    (self.scope.source(),self.scope.module(),self.scope.imports()),e.span())) .await }?;
                unsafe { world.call(self.record_expression(&ty, &args, e.span())) .await }
            },
            ExprKind::Call(..) | ExprKind::Invoke(..) => {
                let result = unsafe { world.call(self.evaluate_returned(e, None)) .await }?;
                self.returned_expression(result, e.span())
            }
        }
    }
    #[inline(never)]
    fn constant_expression(&mut self, scalar: Scalar, bits: Bits) -> Result<Option<Value>> {
        Ok(Some(self.constant(scalar, bits)))
    }
    #[inline(never)]
    fn returned_expression(&mut self, result: Option<returns::Returned>, span: Span) -> Result<Option<Value>> {
        if !self.flow.continues() {
            return Ok(None);
        }
        let value = self.ordinary_return(result, span)?;
        self.value_result(value, span).map(Some)
    }
    async fn function_from_address(&mut self,explicit:&[GenericArgument],count:usize,span:Span,argument_span:Span,
        argument:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Value>>)->Result<Option<Value>> {
            if self.program.target != Target::Cpu {
                return self.fail(span, "B265", "unsafe_function只解释本进程CPU函数地址");
            }
            let ([GenericArgument::Type(signature)], 1) = (explicit, count) else {
                return self.fail(span, "B153", "unsafe_function<cfn<...>>需要明确签名和一个address实参");
            };
            let ty = unsafe { self.world.call(self.world.ty(signature, self.scope.source(), span, self.scope.module(), self.scope.imports())).await }?;
            if !ty.is_c_function() { return self.fail(span, "B263", "unsafe_function需要cfn原生C签名"); }
            let Some(mut value) = argument(self).await? else { return Ok(None); };
            self.require_same(&Ty::builtin("address").unwrap(), &value.ty, argument_span)?;
            value.ty = ty;
            value.proof.retain_view_borrows(&value.ty);
            value.literal = false;
            Ok(Some(value))
    }
    #[inline(never)]
    fn number_expression(&mut self, n: &str, span: Span) -> Result<Option<Value>> {
        let (t, b) = number(self.scope.source(), span, n)?;
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
        let proof = bounds::Proof::bitcast(&value, target);
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
            proof,
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
            self.scope.source(), span)?;
        let a = if types.cast_left { self.cast(a, types.scalar) } else { a };
        let b = if types.cast_right { self.cast(b, types.scalar) } else { b };
        Ok((a, b))
    }
    #[inline(never)]
    fn binary(&mut self, op: &str, a: Value, b: Value, span: Span, generated_origin: Option<Arc<GeneratedOrigin>>) -> Result<Value> {
        let (ty, kind) = operators::binary(op, &a.ty, a.literal, &b.ty, b.literal,
            self.scope.source(), span)?;
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
            operators::Binary::Matrix(plan) => return Ok(self.matrix_binary(ty, plan, a, b)),
            operators::Binary::Numeric { operands, comparison, operation } => (operands, comparison, operation),
        };
        let (operands, comparison, operation) = operands;
        let literal = a.literal && b.literal && !operands.scalar.is_float() && !comparison;
        let a = if operands.cast_left { self.cast(a, operands.scalar) } else { a };
        let b = if operands.cast_right { self.cast(b, operands.scalar) } else { b };
        let scalar = operands.scalar;
        let divisors = if matches!(op, "/" | "%") && !scalar.is_float() {
            let path=self.residency_path();
            match bounds::integer_divisors(&b,&path) {
                Some(divisors)=>Some(divisors),
                None if self.view_model.pending_values()||self.loop_contexts.iter().any(|context|context.view_inputs.iter().any(|slot|matches!(slot,view::InputSlot::Value{identity:Some(_),..})))=>{
                    self.check_view_requirement(view::Requirement::Nonzero{value:Box::new(b.clone()),path,span})?;
                    Some(b.regs.iter().copied().map(IntegerDivisor::Value).collect())
                },
                None=>return self.fail(span,"B152",bounds::INTEGER_DIVISOR_MESSAGE),
            }
        } else {
            None
        };
        Ok(self.lower_numeric_binary(op, ty, a, b, scalar, comparison, operation, literal,
            divisors, generated_origin))
    }

    fn lower_numeric_binary(
        &mut self, op: &str, ty: Ty, a: Value, b: Value, scalar: Scalar,
        comparison: bool, operation: Option<BinaryOp>, literal: bool,
        divisors: Option<Vec<IntegerDivisor>>, generated_origin: Option<Arc<GeneratedOrigin>>,
    ) -> Value {
        let n = ty.components.len();
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
            bounds::arithmetic(self, op, &a, &b)
        };
        let known_comparison = if !constant_fold && comparison { proof.known_boolean_value(n) } else { None };
        let constant = constant.or_else(|| known_comparison.as_ref().map(|known| known.iter().map(|(bits, _)| *bits).collect()));
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
            if let Some(known) = known_comparison {
                let operation: Arc<str> = op.into();
                for (component_index, (inst, (result_bits, [false_possible, true_possible]))) in self.code[first..].iter_mut().zip(known).enumerate() {
                    let ai = component_index % a.regs.len();
                    let bi = component_index % b.regs.len();
                    let numeric = scalar.is_integer() || scalar.is_float();
                    inst.generated_origin = Some(Arc::new(GeneratedOrigin {
                        reason: GenerationReason::ComparisonKnownValue,
                        trigger: Some(GenerationAnchor::ComparisonKnownValue {
                            expression: inst.source_site.clone(), operation: operation.clone(), scalar,
                            left_ranges: numeric.then(|| a.component_range_intervals(ai)), right_ranges: numeric.then(|| b.component_range_intervals(bi)),
                            left_constant_present: a.constant.is_some(), right_constant_present: b.constant.is_some(), same_value: a.same_component(ai, &b, bi),
                            result_bits, false_possible, true_possible,
                            left_component_index: ai, left_component_count: a.regs.len(), right_component_index: bi, right_component_count: b.regs.len(),
                            component_index, component_count: n, retained: inst.generated_origin.clone(),
                        }),
                    }));
                }
            }
            value.literal = literal;
            value.proof = proof;
            return value;
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
            let instruction = self.code.len();
            if let Some(origin) = &generated_origin { self.code.push(Inst::generated(kind, origin.clone())); }
            else { self.emit(kind); }
            if scalar.is_float() {
                self.code[instruction].float_preserve =
                    proof.binary_requires_float_preserve(scalar, i, &a, &b, comparison);
            }
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
        value
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
            let proof = bounds::arithmetic(self, op, &a, &b);
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
            source: self.scope.source().clone(),
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
    async fn builtin_call(&mut self, name: &str, args: &Expressions, span: Span, record:Option<Option<Ty>>) -> Result<Option<Value>> {
        if name == "c_function" { return unsafe { self.world.call(self.c_function_value(args, span)).await }.map(Some); }
        if matches!(name, "Ok" | "Err") {
            return self.fail(
                span,
                "B269",
                "Ok／Err 需要 result<T,E> 上下文或两个显式类型实参",
            );
        }
        if matches!(name, "upload" | "download") {
            return unsafe { self.world.call(self.application_transfer(name, args, span)).await };
        }
        if name == "println" {
            return unsafe { self.world.call(self.println(args, span)).await };
        }
        if name == "texture_sample" {
            return unsafe { self.world.call(self.texture_sample(args, span)).await };
        }
        if name == "append" {
            return unsafe { self.world.call(self.append(args, span)).await };
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
        if let Some(value) = unsafe { self.world.call(self.subgroup(name,args.len(),span,async |builder,index|unsafe { builder.world.call(builder.expr(&args.at(index))).await },|index|args.at(index).span())).await } {
            return value;
        }
        if matches!(name, "trace_closest" | "trace_any") {
            return unsafe { self.world.call(self.ray_query_call(name, args, span)).await };
        }
        if name == "unsafe_read" {
            return unsafe { self.world.call(self.unsafe_read_call(args, span)).await };
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
            return unsafe { self.world.call(self.atomic_value(name, args, span)).await };
        }
        match record{
            Some(Some(ty))=>return unsafe{self.world.call(self.record_values(ty,args.len(),span,
                async |builder,index,expected|unsafe{builder.world.call(builder.expr_expected(&args.at(index),expected)).await})).await},
            Some(None)=>{},
            None=>if !self.world.resolve(name,self.scope.module(),self.scope.imports(),false).is_missing(){
                return unsafe{self.world.call(self.record_expression(&TypeName::named(name),args,span)).await};
            },
        }
        if name == "address_of" { return unsafe { self.world.call(self.position_address(args, span)).await }; }
        unsafe { self.world.call(self.builtin_values(name,span,async |builder| {
            let mut values=Vec::new();
            for arg in args.iter() {
                let Some(value)=unsafe { builder.world.call(builder.expr(&arg)).await }? else { return Ok(None); };
                values.push(value);
            }
            Ok(Some(values))
        })).await }
    }
    // The original value/IR owner consumes values from either actual input reader.
    // Argument evaluation stays lazy until this owner has selected a known operation.
    async fn builtin_values(&mut self,name:&str,span:Span,
        values:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Vec<Value>>>)->Result<Option<Value>> {
        if name == "function_address" {
            let Some(mut values) = values(self).await? else { return Ok(None); };
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
            let Some(mut values) = values(self).await? else { return Ok(None); };
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
            let Some(mut values) = values(self).await? else { return Ok(None); };
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
            let Some(values) = values(self).await? else { return Ok(None); };
            return self.value_constructor(ty, values, span).map(Some);
        }
        if matches!(name, "bitcast_u32" | "bitcast_f32" | "bitcast_u64" | "bitcast_f64") {
            let Some(values) = values(self).await? else { return Ok(None); };
            return self.bitcast(name, values, span).map(Some);
        }
        if matches!(
            name,
            "sqrt"
                | "abs"
                | "floor"
                | "ceil"
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
            let Some(values) = values(self).await? else { return Ok(None); };
            return self.math(name, values, span).map(Some);
        }
        if matches!(name, "transpose" | "determinant" | "inverse") {
            let Some(values) = values(self).await? else { return Ok(None); };
            return self.matrix_intrinsic(name, values, span).map(Some);
        }
        if name == "dot" {
            let Some(values) = values(self).await? else { return Ok(None); };
            return self.dot_value(values, span).map(Some);
        }
        if matches!(name, "length" | "normalize") {
            let Some(values) = values(self).await? else { return Ok(None); };
            return self
                .length_value(values, name == "normalize", span)
                .map(Some);
        }
        if name == "cross" {
            let Some(values) = values(self).await? else { return Ok(None); };
            return self.cross_value(values, span).map(Some);
        }
        if name == "clamp" {
            let Some(values) = values(self).await? else { return Ok(None); };
            if values.len() != 3 {
                return self.fail(span, "B153", "clamp 需要三个参数");
            }
            let a = self.math("max", vec![values[0].clone(), values[1].clone()], span)?;
            return self.math("min", vec![a, values[2].clone()], span).map(Some);
        }
        self.fail(span, "B158", format!("未知或未导出的函数或能力 {name}"))
    }
    #[inline(never)]
    async fn unary_expr(&mut self, op: &str, a: &Expr, span: Span) -> Result<Option<Value>> {
        if op == "-" && let ExprKind::Number(literal) = a.kind() {
            return self.negative_number(literal, span).map(Some);
        }
        let Some(value) = unsafe { self.world.call(self.expr(a)).await }? else { return Ok(None); };
        self.unary_value(op, value, span).map(Some)
    }
    fn negative_number(&mut self, literal: &str, span: Span) -> Result<Value> {
        let (ty, bits) = numeric_literal::number(self.scope.source(), span, literal, true)?;
        let scalar = ty.components[0].0;
        let mut value = self.constant_value(ty, vec![bits], None);
        value.literal = scalar.size() == 4;
        Ok(value)
    }
    fn unary_value(&mut self, op: &str, a: Value, span: Span) -> Result<Value> {
        let Some(operation) = operators::unary(op, &a.ty, self.scope.source(), span)? else {
            return Ok(a);
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
            return Ok(value);
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
        Ok(Value {
            proof,
            literal: a.literal,
            ty: a.ty,
            regs,
            uniform: a.uniform,
            constant: None,
        })
    }
    #[inline(never)]
    async fn binary_expr(&mut self, op: &str, a: &Expr, b: &Expr, span: Span) -> Result<Option<Value>> {
        if op == "&&" || op == "||" { return unsafe { self.world.call(self.short_circuit_expr(op,a,b,span)).await }; }
        let Some(a) = unsafe { self.world.call(self.expr(a)).await }? else { return Ok(None); };
        unsafe { self.world.call(self.binary_after_left(op,Box::new(a),b,span)).await }
    }
    #[inline(never)]
    async fn short_circuit_expr(&mut self, op: &str, a: &Expr, b: &Expr, span: Span) -> Result<Option<Value>> {
        let Some(condition) = unsafe { self.world.call(self.temporary_scope(a.span(), async |builder| Ok((unsafe { builder.world.call(builder.expr(a)).await }?, view::Borrow::default())))).await }? else {
            return Ok(None);
        };
        self.require_bool(&condition, a.span())?;
        unsafe { self.world.call(self.short_circuit_value(op, condition, b.span(),
            async |builder| unsafe { builder.world.call(builder.evaluate_returned(b, None)).await },
            async |builder, names| unsafe { builder.world.call(expression_assignments(builder, b, names)).await }, span)).await }
    }
    async fn short_circuit_value(
        &mut self, op: &str, condition: Value, right_span: Span,
        right: impl std::ops::AsyncFnOnce(&mut Self) -> Result<Option<returns::Returned>>,
        modified: impl std::ops::AsyncFnOnce(&Self, &mut BTreeSet<String>) -> Result<()>, span: Span,
    ) -> Result<Option<Value>> {
        let skipped = op == "||";
        let skip = async |builder: &mut Self| {
            let value = builder.constant(Scalar::Bool, Bits::from(skipped));
            Ok(Some(returns::Returned::Value(Box::new(value))))
        };
        let result = if op == "&&" {
            unsafe { self.world.call(self.selection_after_condition(condition, (right_span, right), (span, skip), modified, span)).await }?
        } else {
            unsafe { self.world.call(self.selection_after_condition(condition, (span, skip), (right_span, right), async |_, _| Ok(()), span)).await }?
        };
        if !self.flow.continues() { return Ok(None); }
        let value = self.ordinary_return(result, span)?;
        let value = self.value_result(value, span)?;
        self.require_bool(&value, span)?;
        Ok(Some(value))
    }
    #[inline(never)]
    async fn binary_after_left(&mut self, op: &str, a: Box<Value>, b: &Expr, span: Span) -> Result<Option<Value>> {
        let Some(b) = unsafe { self.world.call(self.expr(b)).await }? else { return Ok(None); };
        self.binary_operands(op,a,b,span)
    }
    #[inline(never)]
    fn binary_operands(&mut self, op: &str, a: Box<Value>, b: Value, span: Span) -> Result<Option<Value>> {
        self.binary(op,*a,b,span,None).map(Some)
    }
    #[inline(never)]
    async fn if_statement(
        &mut self,
        condition: &Expr,
        yes_body: &Block,
        no_body: &Block,
        span: Span,
    ) -> Result<()> {
        let Some(c) = unsafe { self.world.call(self.temporary_scope(condition.span(), async |builder| Ok((unsafe { builder.world.call(builder.expr(condition)).await }?, view::Borrow::default())))).await }? else {
            return Ok(());
        };
        self.require_bool(&c, condition.span())?;
        unsafe { self.world.call(self.conditional_blocks(c, async |b| unsafe { b.world.call(b.block(yes_body)).await }, async |b| unsafe { b.world.call(b.block(no_body)).await }, span)).await }
    }
    async fn conditional_blocks(
        &mut self,
        c: Value,
        yes_body: impl std::ops::AsyncFnOnce(&mut Self) -> Result<()>,
        no_body: impl std::ops::AsyncFnOnce(&mut Self) -> Result<()>,
        span: Span,
    ) -> Result<()> {
        let partition = Arc::new(bounds::Condition::of(&c));
        if let Some(truth) = partition.boolean_constant() {
            self.refine_bounds(&partition, truth);
            return if truth { yes_body(self).await } else { no_body(self).await };
        }
        self.remember_loop_control(&c);
        let environment = self.env.clone();
        let path = self.path.clone();
        let prefix = self.code.len();
        let init = self.application_initialization_state();
        let control = self.control_uniform;
        self.control_uniform &= c.uniform;
        self.refine_bounds(&partition, true);
        yes_body(self).await?;
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
        no_body(self).await?;
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
            let when=owned_presence::Presence::input(&c);
            self.join_initialization(&when,yes_init,no_init);
            self.join_environment_selected(&mut no_env,&when,&when.condition());
        } else {
            self.path = bounds::Clause::impossible();
            self.env = environment;
            self.join_initialization(&owned_presence::Presence::input(&c),yes_init,no_init);
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
    async fn while_statement(&mut self, condition: &Expr, body: &Block, span: Span) -> Result<()> {
        let initial = unsafe { self.world.call(self.application_initial_loop(body)).await }?;
        unsafe { self.world.call(self.while_loop(condition, body, initial, span)).await }
    }
    async fn while_loop(&mut self, condition: &Expr, body: &Block, initial: bool, span: Span) -> Result<()> {
        self.flow = Flow::NEXT;
        let environment = self.env.clone();
        self.enter_loop(&environment);
        self.configure_initial_loop(initial,None,None);
        self.prepare_view_loop();
        let writes=unsafe { self.world.call(self.loop_writes(Some(condition),body,None,initial)).await }?;
        self.loop_contexts.last_mut().unwrap().writes=writes;
        self.loop_contexts.last_mut().unwrap().outer=std::mem::take(&mut self.code);
        let Some(c) = unsafe { self.world.call(self.temporary_scope(condition.span(), async |builder| Ok((unsafe { builder.world.call(builder.expr(condition)).await }?, view::Borrow::default())))).await }? else {
            let outer=std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer);
            self.finish_view_condition()?;
            let code = std::mem::replace(&mut self.code, outer);
            self.code.extend(code);
            return Ok(());
        };
        self.require_bool(&c, condition.span())?;
        self.loop_contexts.last_mut().unwrap().control_source=view_contracts::LoopControl::capture(&c,&self.env,None);
        let partition = Arc::new(bounds::Condition::of(&c));
        let condition_prefix = self.code.len();
        let mut condition_env = self.env.clone();
        let condition_path = self.path.clone();
        let mut possible=!c.constant.as_ref().is_some_and(|bits|bits[0]==0);
        if possible {
            self.refine_bounds(&partition,true);
            if !initial {
                for(name,(low,high))in std::mem::take(&mut self.loop_contexts.last_mut().unwrap().writes.body_bounds) {
                    if let Some(Binding::Owned(owner))=self.env.get_mut(&name){
                        owner.proof.retain_loop_bounds(&bounds::Proof::unsigned(low,high),0,true,true);
                    }
                }
                possible=unsafe { self.world.call(self.retain_loop_bounds(Some(condition),body,&environment,&mut condition_env)).await }?;
            }
        }
        if !possible {
            self.env=condition_env;self.path=condition_path;
            let outer=std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer);
            self.finish_view_condition()?;
            self.refine_bounds(&partition,false);
            let condition_code=std::mem::replace(&mut self.code,outer);
            self.code.extend(condition_code);
            return Ok(());
        }
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
        unsafe { self.world.call(self.block(body)).await }?;
        let mut body_flow = self.flow;
        if body_flow.continues() {
            unsafe { self.world.call(self.check_application_loop(condition.span())).await }?;
        }
        self.iterators.pop();
        self.loop_depth -= 1;
        let mut code = self.code.split_off(condition_prefix);
        let condition_code = std::mem::take(&mut self.code);
        if initial {
            let (expanded, flow) =
                unsafe { self.world.call(self.expand_initial_loop(code, body_flow.continues(), async |builder, _| {
                    let initial=unsafe { builder.world.call(builder.next_initial_loop(body)).await }?;
                    unsafe { builder.world.call(builder.while_loop(condition, body, initial, span)).await }
                })).await }?;
            code = expanded;
            body_flow = body_flow.union(flow);
        }
        let body = code;
        let outer=std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer);
        let (next, _) = self.leave_loop(natural, &c, &condition_code, &body, span, false)?;
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
    async fn for_statement(
        &mut self,
        name: &str,
        start: &Expr,
        end: &Option<Expr>,
        body: &Block,
        span: Span,
    ) -> Result<()> {
        if self.env.contains_key(name) {
            return self.fail(span, "B125", "循环变量不能覆盖已有绑定");
        }
        let (begin, limit, data) = if let Some(end) = end {
            let Some(a) = unsafe { self.world.call(self.temporary_scope(start.span(), async |builder| Ok((unsafe { builder.world.call(builder.expr(start)).await }?, view::Borrow::default())))).await }? else {
                return Ok(());
            };
            let Some(b) = unsafe { self.world.call(self.temporary_scope(end.span(), async |builder| Ok((unsafe { builder.world.call(builder.expr(end)).await }?, view::Borrow::default())))).await }? else {
                return Ok(());
            };
            self.require_same(&Ty::scalar(Scalar::U32), &a.ty, start.span())?;
            self.require_same(&a.ty, &b.ty, start.span())?;
            (a, b, None)
        } else {
            let source = if let Some(value) = unsafe { self.world.call(self.application_reference(start)).await }? {
                Some(Binding::DataReference(value))
            } else if let ExprKind::Name(name) = start.kind() {
                self.env.get(name).cloned()
            } else {
                self.dataset_reference(start)?.map(Binding::Data)
            };
            let begin = self.constant(Scalar::U32, 0);
            match source {
                Some(Binding::DataReference(data)) => {
                    self.application_cpu(&data, start.span())?;
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
                            start.span(),
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
                    let Some(view) = unsafe { self.world.call(self.expr(start)).await }? else {
                        return Ok(());
                    };
                    if view.ty.view.is_none() {
                        return self.fail(
                            start.span(),
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
        let initial = unsafe { self.world.call(self.application_initial_loop(body)).await }?;
        let slot = self.local_slots(&Ty::scalar(Scalar::U32)).0[0];
        self.emit(InstKind::StoreLocal(slot, begin.regs[0]));
        let initialization = self.application_iteration_outputs(&begin, &limit, match &data {Some(CursorSource::ApplicationData(data))=>Some(&data.data),_=>None});
        let writes = unsafe { self.world.call(self.for_loop(
            ForLoop {
                name: name.to_string(),
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
        )).await }?;
        if self.flow.continues() {
            unsafe { self.world.call(self.complete_application_writes(|builder,data| writes.unknown_storage
                || writes.storage.iter().any(|written| written.includes_root(builder,data.root.regs[0]))
                || data.place.as_ref().is_some_and(|place| matches!(place.storage(),
                    Place::Local(_, slots, _) if slots.iter().any(|slot| writes.locals.contains(slot)))), span)).await }?;
        }
        Ok(())
    }
    async fn for_loop(
        &mut self,
        cursor_specification: ForLoop,
        body: &Block,
        span: Span,
        initial: bool,
        nonempty: bool,
        original_begin: &Value,
    ) -> Result<effects::LoopWrites> {
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
            return Ok(effects::LoopWrites::default());
        }
        if bounds::seeks(body) {
            initialization.clear();
        }
        let environment = self.env.clone();
        self.enter_loop(&environment);
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
            proof:if initial {bounds::Proof::capture(&begin)}else{bounds::for_cursor(&begin,&limit,bounds::seeks(body))},
            ..cursor.clone()
        };
        if !bounds::seeks(body) {
            self.refine_bounds(&Arc::new(bounds::Condition::comparison(Comparison::LessEqual, original_begin, &cursor)), true);
        }
        let binding = if let Some(domain) = data {
            match domain {
                CursorSource::ApplicationData(mut data) => {
                    // The cursor uses the same current loop resource as indexed access.
                    data.data.root.proof.set_view_borrow(0,data.root.proof.view_borrow(0).joined(self.data_borrow(&data.sources)));
                    let input=if !initial&&!bounds::seeks(body){unsafe { self.world.call(self.application_cursor_input(&data,&body_cursor)).await }?}else{bounds::Proof::default()};
                    Binding::Owned(OwnedBinding {
                        place: data.element(
                            &cursor,
                            (
                                cursor.minimum(),
                                cursor
                                    .maximum()
                                    .min(data.length.maximum().saturating_sub(1)),
                            ),
                        ),
                        proof: input,
                        uniform: false,
                        initialization: None,
                    })
                },
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
        let mut writes=unsafe { self.world.call(self.loop_writes(None,body,borrowed,initial)).await }?;
        writes.locals.insert(slot);
        self.loop_contexts.last_mut().unwrap().writes=writes;
        self.loop_contexts.last_mut().unwrap().control_source=view_contracts::LoopControl::capture(&test,&self.env,Some(body_cursor.clone()));
        self.capture_loop_cursor(body_cursor);
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
        if !initial {unsafe { self.world.call(self.retain_loop_bounds(None,body,&environment,&mut natural.env)).await }?;}
        let control = self.control_uniform;
        self.control_uniform &= begin.uniform && limit.uniform;
        self.loop_depth += 1;
        self.iterators.push(Some((name.clone(), slot)));
        unsafe { self.world.call(self.block(body)).await }?;
        let mut body_flow = self.flow;
        let body_reachable = body_flow.continues();
        if body_reachable {
            self.advance_iterator(slot)?;
            unsafe { self.world.call(self.check_application_loop(span)).await }?;
        }
        self.iterators.pop();
        self.loop_depth -= 1;
        if !initial && !bounds::seeks(body) {
            match &cursor_specification.data {
                Some(CursorSource::ApplicationData(data)) => unsafe { self.world.call(self.application_iteration_elements(&mut natural, Some(data), &cursor, &limit)).await }?,
                None => unsafe { self.world.call(self.application_iteration_elements(&mut natural, None, &cursor, &limit)).await }?,
                _ => {},
            }
        }
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
                unsafe { self.world.call(self.expand_initial_loop(code, body_reachable, async |builder, next| {
                    let mut cursor = specification.clone();
                    cursor.initialization =
                        builder.application_iteration_prefix(&initialization, iteration);
                    cursor.begin = next.unwrap();
                    let initial=unsafe { builder.world.call(builder.next_initial_loop(body)).await }?;
                    let nonempty=cursor.begin.maximum()<cursor.limit.minimum();
                    unsafe { builder.world.call(builder.for_loop(cursor, body, span, initial, nonempty, original_begin)).await }.map(|_| ())
                })).await }?;
            code = expanded;
            body_flow = body_flow.union(flow);
        }
        let body = code;
        let outer=std::mem::take(&mut self.loop_contexts.last_mut().unwrap().outer);
        let (next, writes) = self.leave_loop(natural, &test, &condition, &body, span, nonempty)?;
        self.flow = body_flow.loop_result(next);
        self.control_uniform = control;
        self.code = outer;
        self.emit(InstKind::Loop {
            condition,
            test: test.regs[0],
            body,
        });

        Ok(writes)
    }
    #[inline(never)]
    async fn let_statement(
        &mut self,
        annotation: &Option<TypeName>,
        name: &str,
        e: &Expr,
        span: Span,
    ) -> Result<()> {
        if self.env.contains_key(name) {
            return self.fail(span, "B125", format!("重复局部变量或覆盖参数 {name}"));
        }
        if let ExprKind::Call(call, args) = e.kind() {
            if matches!(call, "tile_publish" | "tile_atomic") {
                return unsafe { self.world.call(self.tile_publish(
                    annotation.is_some(),
                    name,
                    &args,
                    span,
                    call == "tile_atomic",
                )).await };
            }
        }
        let expected=match annotation.as_ref() {
            Some(ty)=>Some(unsafe { self.world.call(self.expected_storage(ty,span)).await }?),
            None=>None,
        };
        let result = if let ExprKind::Cooperate(call, explicit, args) = e.kind() {
            unsafe { self.world.call(self.cooperate(call, explicit.as_ref(), &args, e.span())).await }?
                .map(returns::Returned::value)
        } else if let ExprKind::Call(call, args) = e.kind() {
            if let Some(op) = TileOp::from_source(call) {
                Some(returns::Returned::value(
                    self.tile_collective(op, &args, span)?,
                ))
            } else {
                unsafe { self.world.call(self.evaluate_returned(e, expected.as_ref())).await }?
            }
        } else {
            unsafe { self.world.call(self.evaluate_returned(e, expected.as_ref())).await }?
        };
        if !self.flow.continues() {
            return Ok(());
        }
        let mut value = result.ok_or_else(|| Diagnostic {
            source: self.scope.source().clone(),
            span,
            code: "B228",
            message: "局部绑定需要值或拥有资源".into(),
        })?;
        if let Some(expected) = expected {
            unsafe { self.world.call(self.check_returned_type(&mut value, &expected, span)).await }?;
        }
        self.bind_returned(name, value, span)
    }
    fn bind_local_value(&mut self, name: &str, value: Value, span: Span) -> Result<()> {
        self.require_view_available(&value, &[], span)?;
        if self.env.contains_key(name) {
            return self.fail(span, "B125", format!("重复局部变量或覆盖参数 {name}"));
        }
        if !self.mutable_names.contains(name) && value.ty.contains_arrays() {
            let mut owner = self.materialize_array(value,None);
            owner.place = owner.place.read_only();
            owner.uniform &= self.control_uniform;
            self.env.insert(name.to_string(), Binding::Owned(owner));
            return Ok(());
        }
        if !self.mutable_names.contains(name) {
            let mut value = value;
            value.literal = false;
            value.uniform &= self.control_uniform;
            self.env.insert(name.to_string(), Binding::Value(value));
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
            name.to_string(),
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
    async fn member_place(&mut self, base: &Expr, field: &str, e: &Expr) -> Result<Option<Place>> {
        let Some(p) = unsafe { self.world.call(self.place(base)).await }? else {
            return Ok(None);
        };
        self.member_position(p,field,e.span())
    }
    fn member_position(&mut self,p:Place,field:&str,span:Span)->Result<Option<Place>> {
        if p.ty().view.is_some() {
            return self.fail(span, "B185", "view 元数据不可修改或按分量访问");
        }
        let (sub, components) = member_type(p.ty(), field, self.scope.source(), span)?;
        Ok(Some(p.project(sub, &components)))
    }
    async fn index_place(&mut self, base: &Expr, index: &Expr) -> Result<Option<Place>> {
        if let Some(data) = unsafe { self.world.call(self.application_reference(base)).await }? {
            return unsafe { self.world.call(self.application_index(data, index)).await };
        }
        if let ExprKind::Name(name) = base.kind()
            && let Some(Binding::Tile(view)) = self.env.get(name).cloned()
        {
            return unsafe { self.world.call(self.tile_place(view, index)).await };
        }
        let Some(view) = self.dataset_reference(base)? else {
            return unsafe { self.world.call(self.view_place(base, index)).await };
        };
        let binding = view.binding;
        let Some(i) = unsafe { self.world.call(self.expr(index)).await }? else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(Scalar::U32), &i.ty, index.span())?;
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
                index.span(),
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
                    index.span(),
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
    fn name_place(&mut self, name: &str, span: Span) -> Result<Place> {
        match self.env.get(name).cloned() {
            Some(Binding::AtomicBorrow(place)) => Ok(place),
            Some(Binding::Owned(owner)) => Ok(owner.place),
            Some(Binding::OwnedValue(owner))=>{
                if let Some(place)=owner.place(){return Ok(place);}
                let owner=self.owned_value_binding(owner,true);let place=owner.place().unwrap();self.env.insert(name.to_string(),Binding::OwnedValue(owner));Ok(place)
            },
            Some(Binding::Value(value)) if value.ty.contains_arrays() => {
                let mut owner = self.materialize_array(value,None);
                owner.place = owner.place.read_only();
                let place = owner.place.clone();
                self.env.insert(name.to_string(), Binding::Owned(owner));
                Ok(place)
            }
            Some(Binding::ViewElement { element, .. }) => Ok(element.place),
            _ => self.fail(span, "B134", format!("{name} 不是可赋值的元素或局部变量")),
        }
    }
    #[inline(never)]
    async fn declare_statement(
        &mut self,
        annotation: &TypeName,
        name: &str,
        span: Span,
    ) -> Result<()> {
        if self.env.contains_key(name) {
            return self.fail(span, "B125", format!("重复局部变量或覆盖参数 {name}"));
        }
        if annotation.dataset {
            return unsafe { self.world.call(self.application_declare(annotation, name, span)).await };
        }
        let ty = unsafe { self.world.call(self.world.ty(
            annotation,
            self.scope.source(),
            span,
            self.scope.module(),
            self.scope.imports(),
        )).await }?;
        if ty.owns_resources() {
            return unsafe { self.world.call(self.owned_value_declare(name, ty, span)).await };
        }
        let (slots, storage) = self.local_slots(&ty);
        self.env.insert(
            name.to_string(),
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
    async fn assign_statement(&mut self, lhs: &Expr, op: &str, rhs: &Expr, span: Span) -> Result<()> {
        if unsafe { self.world.call(self.read_expression_type(lhs)).await }?.is_some_and(|ty|ty.owns_resources()) {
            let mut element=lhs.clone();let mut projection=Vec::new();
            loop {match element.kind() {
                ExprKind::Index(base,index) if match unsafe { self.world.call(self.read_expression_type(&base)).await }?.and_then(|ty|ty.dataset) {
                    Some(reference)=>unsafe { self.world.call(self.expected_storage(&reference,base.span())).await }?.owns_resources(),
                    None=>false,
                }=>{
                    let Some(data)=unsafe { self.world.call(self.application_reference(&base)).await }?else{return Ok(())};
                    projection.reverse();return unsafe { self.world.call(self.assign_application_element(data,&index,&projection,op,rhs,span)).await };
                },
                ExprKind::Member(base,_)|ExprKind::Index(base,_)=>{projection.push(element);element=base;},
                _=>break,
            }}
        }
        if let ExprKind::Name(name) = lhs.kind()
            && let Some(Binding::ApplicationData(data)) = self.env.get(name).cloned()
        {
            return unsafe { self.world.call(self.application_assign(name, data, op, rhs, span)).await };
        }
        let Some(place) = unsafe { self.world.call(self.place(lhs)).await }? else {
            return Ok(());
        };
        if place.ty().owns_resources() {
            return unsafe { self.world.call(self.assign_owned_storage(place,None,op,rhs,span)).await };
        }
        let borrowed = matches!(place.storage(), Place::ApplicationElement { .. })
            .then(|| self.hold_binding(Binding::AtomicBorrow(place.clone())));
        let Some(mut value) = unsafe { self.world.call(self.expr_expected(rhs, Some(place.ty()))).await }? else {
            return Ok(());
        };
        if let Some(name) = borrowed {
            self.env.remove(&name);
        }
        if op != "=" {
            let current = unsafe { self.world.call(self.read_place(&place, lhs.span(), None)).await }?;
            value = self.binary(&op[..op.len() - 1], current, value, span,None)?;
        }
        self.view_uses.after(span);
        unsafe { self.world.call(self.write_place(&place, value, span, None)).await }?;

        Ok(())
    }
    #[inline(never)]
    async fn expression_statement(&mut self, e: &Expr) -> Result<()> {
        if let ExprKind::Call(name, args) = e.kind()
            && name == "atomic_store"
        {
            unsafe { self.world.call(self.atomic_store(&args, e.span())).await }?;
        } else if let ExprKind::Call(name, args) = e.kind()
            && name == "tile_barrier"
        {
            self.tile_barrier(&args, e.span())?;
        } else if let ExprKind::Cooperate(name, explicit, args) = e.kind() {
            unsafe { self.world.call(self.cooperate(name, explicit.as_ref(), &args, e.span())).await }?;
        } else {
            if let Some(value) = unsafe { self.world.call(self.evaluate_returned(e, None)).await }? {
                unsafe { self.world.call(self.drop_returned(value,&view::Borrow::default(),e.span())).await }?;
            }
        }

        Ok(())
    }
    fn advance_iterator(&mut self, slot: usize) -> Result<()> {
        let source=self.loop_cursor().unwrap();
        let one=self.constant(Scalar::U32,1);
        let value=self.binary("+",source,one,self.scope.span(),None)?;
        self.emit(InstKind::StoreLocal(slot,value.regs[0]));
        self.capture_loop_cursor(value);Ok(())
    }
    #[inline(never)]
    async fn seek_statement(&mut self, name: &str, position: &Expr, span: Span) -> Result<()> {
        let Some(Some((iterator, slot))) = self.iterators.last().cloned() else {
            return self.fail(span, "B164", "seek 需要当前有界 for 游标");
        };
        if iterator != *name {
            return self.fail(span, "B165", "seek 只能推进当前最内层游标");
        }
        let Some(value) = unsafe { self.world.call(self.temporary_scope(position.span(), async |builder| Ok((unsafe { builder.world.call(builder.expr(position)).await }?, view::Borrow::default())))).await }? else {
            return Ok(());
        };
        self.require_same(&Ty::scalar(Scalar::U32), &value.ty, position.span())?;
        unsafe { self.world.call(self.release_application_loop()).await }?;
        self.emit(InstKind::StoreLocal(slot, value.regs[0]));
        self.capture_loop_cursor(value);
        unsafe { self.world.call(self.check_application_loop(span)).await }?;
        self.emit(InstKind::Continue);
        self.flow = Flow::CONTINUE;

        Ok(())
    }
    async fn ray_query_call(&mut self, name: &str, args: &Expressions, span: Span) -> Result<Option<Value>> {
        if !matches!(args.len(), 5 | 6) {
            return self.fail(
                span,
                "B183",
                "光线查询需要 acceleration、origin、minimum、direction、maximum，以及可选的 bool cull_back_faces",
            );
        }
        let argument=args.at(0);
        let ExprKind::Name(scene) = argument.kind() else {
            return self.fail(span, "B183", "光线查询需要命名 acceleration 资源");
        };
        let Some(Binding::Acceleration(scene)) = self.env.get(scene).cloned() else {
            return self.fail(span, "B183", "光线查询需要 read acceleration 资源");
        };
        let Some(origin) = unsafe { self.world.call(self.expr(&args.at(1))).await }? else {
            return Ok(None);
        };
        let Some(minimum) = unsafe { self.world.call(self.expr(&args.at(2))).await }? else {
            return Ok(None);
        };
        let Some(direction) = unsafe { self.world.call(self.expr(&args.at(3))).await }? else {
            return Ok(None);
        };
        let Some(maximum) = unsafe { self.world.call(self.expr(&args.at(4))).await }? else {
            return Ok(None);
        };
        self.require_same(&Ty::vector(Scalar::F32, 3), &origin.ty, args.at(1).span())?;
        self.require_same(&Ty::scalar(Scalar::F32), &minimum.ty, args.at(2).span())?;
        self.require_same(&origin.ty, &direction.ty, args.at(3).span())?;
        self.require_same(&minimum.ty, &maximum.ty, args.at(4).span())?;
        let cull_back_faces = if args.len() == 6 {
            let arg = args.at(5);
            let Some(value) = unsafe { self.world.call(self.expr(&arg)).await }? else {
                return Ok(None);
            };
            self.require_same(&Ty::scalar(Scalar::Bool), &value.ty, arg.span())?;
            Some(value.regs[0])
        } else {
            None
        };
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
            cull_back_faces,
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
    async fn unsafe_read_call(&mut self, args: &Expressions, span: Span) -> Result<Option<Value>> {
        if args.len() != 2 {
            return self.fail(
                span,
                "B181",
                "unsafe_read 需要 read 数据集与 u32 索引；调用者负责外部索引有效性",
            );
        }
        if let ExprKind::Name(data) = &args.at(0).kind() {
            if let Some(data) = unsafe { self.world.call(self.application_value(data, span)).await }? {
                self.application_cpu(&data, span)?;
                let Some(index) = unsafe { self.world.call(self.expr(&args.at(1))).await }? else {
                    return Ok(None);
                };
                self.require_same(&Ty::scalar(Scalar::U32), &index.ty, args.at(1).span())?;
                return unsafe { self.world.call(self
                    .read_place(
                        &Self::application_element(
                            &data,
                            &index,
                            (index.minimum(), index.maximum()),
                        ),
                        span, None
                    )).await }
                    .map(Some);
            }
        }
        if let Some(view) = self.dataset_reference(&args.at(0))? {
            if !matches!(view.access, Access::Read | Access::InOut) {
                return self.fail(span, "B181", "unsafe_read 需要 read 或本阶段共享只读的 inout 数据集");
            }
            let Some(index) = unsafe { self.world.call(self.expr(&args.at(1))).await }? else { return Ok(None); };
            self.require_same(&Ty::scalar(Scalar::U32), &index.ty, args.at(1).span())?;
            return unsafe { self.world.call(self.read_place(&self.dataset_element(&view, index.regs[0], false), span, None)).await }.map(Some);
        }
        let Some(view) = unsafe { self.world.call(self.expr(&args.at(0))).await }? else {
            return Ok(None);
        };
        unsafe { self.world.call(self.unsafe_read_view(view,span,args.at(1).span(),async |builder|unsafe { builder.world.call(builder.expr(&args.at(1))).await })).await }
    }
    async fn unsafe_read_view(&mut self,view:Value,span:Span,index_span:Span,
        read_index:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Value>>)->Result<Option<Value>> {
        if view.ty.view.is_none() {
            return self.fail(span, "B181", "unsafe_read 需要 read dataset 或不可变 view");
        }
        let held = self.hold_binding(Binding::Value(view.clone()));
        let evaluated = read_index(self).await;
        self.env.remove(&held);
        let Some(index) = evaluated? else { return Ok(None); };
        self.require_same(&Ty::scalar(Scalar::U32), &index.ty, index_span)?;
        return unsafe { self.world.call(self
            .read_place(&Self::view_element(&view, index.regs[0]), span, None)).await }
            .map(Some);
    }
    fn ordinal_call(&mut self, args: &Expressions, span: Span) -> Result<Option<Value>> {
        if args.len() != 1 {
            return self.fail(span, "B168", "ordinal 需要一个有界游标");
        }
        let argument=args.at(0);
        let ExprKind::Name(name) = argument.kind() else {
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
    async fn member_expr(&mut self, base: &Expr, field: &str, e: &Expr) -> Result<Option<Value>> {
        if let Some(data) = unsafe { self.world.call(self.application_reference(base)).await }? {
            return if field == "length" {
                Ok(Some(data.length.clone()))
            } else {
                self.fail(e.span(), "B141", "dataset只公开只读length")
            };
        }
        if let ExprKind::Name(name) = base.kind() {
            if let Some(Binding::Texture(reference)) = self.env.get(name).cloned() {
                if field == "length"
                    && let Some(count) = reference.shape(self.program)
                {
                    return Ok(Some(self.constant(Scalar::U32, Bits::from(count))));
                }
                return self.fail(e.span(), "B239", "纹理资源数组只公开只读 length");
            }
            if let Some(Binding::Tile(view)) = self.env.get(name).cloned() {
                return match field {
                    "length" => Ok(Some(self.tile_length(&view))),
                    "lane" => Ok(Some(self.tile_lane())),
                    _ => self.fail(e.span(), "B141", "tile 只公开只读 length 与 lane"),
                };
            }
        }
        if field == "length" && let Some(view) = self.dataset_reference(base)? {
            return Ok(Some(self.data_length(&view)));
        }
        if self.is_place(base) {
            // 保留字段访问，避免为了读取一个字段先加载整个结构体。
            let Some(p) = unsafe { self.world.call(self.place(base)).await }? else {
                return Ok(None);
            };
            if p.ty().array.is_some() && field == "length" {
                return Ok(Some(self.constant(Scalar::U32, Bits::from(p.ty().values[0]))));
            }
            if p.ty().view.is_some() {
                if field == "length" {
                    let view = unsafe { self.world.call(self.read_place(&p, e.span(), None)).await }?;
                    return Ok(Some(Self::view_length(&view)));
                }
                return self.fail(e.span(), "B185", "view 只公开只读 length，不公开设备地址分量");
            }
            let (sub, components) = member_type(p.ty(), field, self.scope.source(), e.span())?;
            return unsafe { self.world.call(self
                .read_place(&p.project(sub, &components), e.span(), None)).await }
                .map(Some);
        }
        let Some(v) = unsafe { self.world.call(self.expr(base)).await }? else {
            return Ok(None);
        };
        self.member_value(v, field, e.span()).map(Some)
    }
    pub(super) fn member_value(&mut self, v: Value, field: &str, span: Span) -> Result<Value> {
        if v.ty.array.is_some() && field == "length" {
            return Ok(self.constant(Scalar::U32, Bits::from(v.ty.values[0])));
        }
        if v.ty.view.is_some() && field == "length" {
            return Ok(Self::view_length(&v));
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
        let resource = self.dataset_reference(base)?.is_some() || matches!(base.kind(), ExprKind::Name(name) if matches!(self.env.get(name), Some(Binding::Tile(_) | Binding::ApplicationData(_))));
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
    async fn name_expr(&mut self, name: &str, span: Span) -> Result<Value> {
        if let Some(value) = self.scope.bindings().values.get(name).copied() {
            return Ok(self.constant(Scalar::U32, Bits::from(value)));
        }
        match self.env.get(name).cloned() {
            Some(Binding::Value(v)) => Ok(v),
            Some(Binding::Owned(owner)) => unsafe { self.world.call(self.read_place(&owner.place, span, None)).await },
            Some(
                Binding::AtomicBorrow(_)
                | Binding::ViewElement { .. },
            ) => {
                let p = self.name_place(name, span)?;
                unsafe { self.world.call(self.read_place(&p, span, None)).await }
            }
            None => {
                let key = self
                    .world
                    .resolve(
                        name,
                        self.scope.module(),
                        self.scope.imports(),
                        true,
                    ).unique()
                    .filter(|key| self.world.functions[key].function.kind() == FunctionKind::Constant)
                    .ok_or_else(|| Diagnostic {
                        source: self.scope.source().clone(),
                        span: span,
                        code: "B145",
                        message: format!("未知值 {name}"),
                    })?;
                unsafe { self.world.call(self.module_constant(key)).await }
            }
            _ => self.fail(span, "B145", format!("未知值 {name}")),
        }
    }
    fn is_place(&self, e: &Expr) -> bool {
        match e.kind() {
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
            ExprKind::Member(b, _) => self.is_place(&b),
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
