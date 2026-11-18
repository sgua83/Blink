//! Declared output initialization is part of the original function contract.
//! Its predicates use input snapshots and the real returned value, never a
//! runtime initialization flag or a second resource owner.
use super::*;
use application::Ownership;
use owned_presence::Presence;
use returns::Returned;
#[path="outputs/contract_condition.rs"]
mod contract_condition;
#[path="outputs/written.rs"]
mod written;
use contract_condition::{add_input_names,bind_constants,condition_type,positional_index};
use std::borrow::Cow;

#[derive(Clone, Copy)]
enum Clauses<'a> {
    Declaration(&'a crate::generics::Instance),
    Signature(usize, &'a [ContractPredicate], Span),
    Native(&'a [Param], &'a [FunctionContract], &'a str, Span),
}
/// Read source clauses with their real instance bindings, or a concrete cfn signature.
#[derive(Clone, Copy)]
pub(super) struct Contract<'a> {
    pub(super) source: &'a crate::Source,
    pub(super) module: &'a str,
    pub(super) imports: &'a [String],
    public: bool,
    clauses: Clauses<'a>,
}
impl<'a> From<&'a crate::generics::Instance> for Contract<'a> {
    fn from(instance: &'a crate::generics::Instance) -> Self {
        let definition=&instance.definition;
        Self { source:&definition.source, module:&definition.module,
            imports:&definition.imports, public:definition.public,
            clauses:Clauses::Declaration(instance) }
    }
}
impl<'a> Contract<'a> {
    pub(super) fn signature(source:&'a crate::Source,module:&'a str,imports:&'a [String],
        argument_count:usize,outputs:&'a [ContractPredicate],span:Span)->Self {
        Self { source,module,imports,public:false,clauses:Clauses::Signature(argument_count,outputs,span) }
    }
    pub(super) fn declaration_origin(self)->Option<SourceDeclaration>{
        match self.clauses {
            Clauses::Declaration(instance)=>instance.definition.declaration_origin(),
            Clauses::Signature(..)|Clauses::Native(..)=>None,
        }
    }
    pub(super) fn native(scope:&'a LexicalScope,parameters:&'a [Param],contracts:&'a [FunctionContract],name:&'a str,span:Span)->Self {
        Self{source:scope.source(),module:scope.module(),imports:scope.imports(),public:false,
            clauses:Clauses::Native(parameters,contracts,name,span)}
    }
    pub(super) fn parameters(self)->impl Iterator<Item=ScopeParameter<'a>> {
        let(declaration,bound)=match self.clauses {
            Clauses::Declaration(instance)=>(Some(instance),None),
            Clauses::Native(parameters,..)=>(None,Some(parameters)),
            // An indirect call supplies its actual captured values separately.
            Clauses::Signature(..)=>(None,None),
        };
        declaration.into_iter().flat_map(|instance|instance.definition.function.params().into_iter()
            .map(move|parameter|ScopeParameter::Declaration(instance,parameter)))
            .chain(bound.into_iter().flat_map(|parameters|parameters.iter().map(ScopeParameter::Bound)))
    }
    pub(super) fn span(self)->Span {
        match self.clauses {Clauses::Declaration(instance)=>instance.definition.function.span(),
            Clauses::Signature(_,_,span)|Clauses::Native(_,_,_,span)=>span}
    }
    pub(super) fn name(self)->Option<&'a str> {
        match self.clauses {Clauses::Declaration(instance)=>Some(instance.definition.function.name()),
            Clauses::Native(_,_,name,_)=>Some(name),Clauses::Signature(..)=>None}
    }
    pub(super) async fn validate_source(self,world:&World)->Result<()> {
        if let Clauses::Declaration(instance)=self.clauses
            && !instance.definition.function.c_abi(){unsafe { world.call(validate(world,instance)).await }?;}
        Ok(())
    }
    pub(super) fn contracts(self)->impl Iterator<Item=FunctionContract>+'a {
        let(declaration,signature,native)=match self.clauses {
            Clauses::Declaration(instance)=>(Some(instance),None,None),
            Clauses::Signature(_,clauses,span)=>(None,Some((clauses,span)),None),
            Clauses::Native(_,clauses,..)=>(None,None,Some(clauses)),
        };
        declaration.into_iter().flat_map(|instance|instance.contracts())
            .chain(signature.into_iter().flat_map(|(clauses,span)|clauses.iter().map(move|predicate|{
                let kind=if matches!(predicate,ContractPredicate::Nonzero(_)){ContractKind::Requires}else{ContractKind::Ensures};
                FunctionContract{kind,predicate:predicate.clone(),span}
            }))).chain(native.into_iter().flat_map(|clauses|clauses.iter().cloned()))
    }
    pub(super) fn c_call_inputs(self, returns: bool) -> (Option<Access>, BTreeSet<String>) {
        let mut names = BTreeSet::new();
        let memory = external_memory(self.contracts().filter_map(|contract| {
            match (contract.kind, &contract.predicate) {
                (ContractKind::Requires, ContractPredicate::Nonzero(path)) => {
                    names.insert(path.split('.').next().unwrap().to_owned());
                }
                (ContractKind::Ensures, ContractPredicate::Initialized {condition,..}) if returns => {
                    add_input_names(condition, &mut names);
                }
                _ => {}
            }
            (contract.kind==ContractKind::Ensures).then_some(contract.predicate)
        }));
        (memory, names)
    }
    pub(super) fn nonzero_inputs(self)->impl Iterator<Item=(String,Span)>+'a {
        self.contracts().filter_map(|contract|match (contract.kind,contract.predicate) {
            (ContractKind::Requires,ContractPredicate::Nonzero(path))=>Some((path,contract.span)),
            _=>None,
        })
    }
    pub(super) fn parameter_name(self,index:usize)->String {
        match self.clauses {
            Clauses::Declaration(instance)=>instance.definition.function.params().at(index).name().into(),
            Clauses::Signature(..)=>format!("argument{index}"),
            Clauses::Native(parameters,..)=>parameters[index].name.clone(),
        }
    }
    fn condition_scope(self) -> ConditionScope<'a> {
        let parameters = match self.clauses {
            Clauses::Declaration(instance) => ConditionParameters::Declaration(instance),
            Clauses::Native(parameters,..) => ConditionParameters::Native(parameters),
            Clauses::Signature(count,..) => ConditionParameters::Captured(count),
        };
        ConditionScope { source: self.source, module: self.module, imports: self.imports,
            public: self.public, parameters }
    }
}

#[derive(Clone, Copy)]
enum ConditionParameters<'a> {
    Declaration(&'a crate::generics::Instance),
    Native(&'a [Param]),
    Positional(&'a [(Access, TypeName)]),
    Captured(usize),
}

#[derive(Clone, Copy)]
struct ConditionScope<'a> {
    source: &'a Source,
    module: &'a str,
    imports: &'a [String],
    public: bool,
    parameters: ConditionParameters<'a>,
}
impl<'a> ConditionScope<'a> {
    fn has_parameter(self, name: &str) -> bool {
        match self.parameters {
            ConditionParameters::Declaration(instance) => {
                instance.definition.function.params().iter().any(|parameter| parameter.name()==name)
            }
            ConditionParameters::Native(parameters) => parameters.iter().any(|parameter| parameter.name==name),
            ConditionParameters::Positional(parameters) => positional_index(name) < parameters.len(),
            ConditionParameters::Captured(count) => positional_index(name) < count,
        }
    }

    async fn parameter_type(self, world: &World, name: &str, span: Span) -> Result<Option<Cow<'a, TypeName>>> {
        match self.parameters {
            ConditionParameters::Declaration(instance) => {
                let Some(parameter) = instance.definition.function.params().iter().find(|parameter| parameter.name()==name)
                    else { return Ok(None); };
                let ty = unsafe {
                    world.call(instance.bindings.type_name(world, &parameter.ty(), (self.source,self.module,self.imports), span)).await
                }?;
                Ok(Some(Cow::Owned(ty)))
            }
            ConditionParameters::Native(parameters) => {
                Ok(parameters.iter().find(|parameter| parameter.name==name).map(|parameter| Cow::Borrowed(&parameter.ty)))
            }
            ConditionParameters::Positional(parameters) => {
                Ok(parameters.get(positional_index(name)).map(|(_, ty)| Cow::Borrowed(ty)))
            }
            ConditionParameters::Captured(_) => Ok(None),
        }
    }
}

pub(super) fn external_memory<P:std::borrow::Borrow<ContractPredicate>>(predicates:impl IntoIterator<Item=P>)->Option<Access> {
    let(mut reads,mut writes)=(true,true);
    for predicate in predicates {if let ContractPredicate::Memory(access)=predicate.borrow() {
        reads&=access.is_some_and(|access|access.reads());
        writes&=access.is_some_and(|access|access.writes());
    }}
    match(reads,writes){(true,true)=>Some(Access::InOut),(true,false)=>Some(Access::Read),(false,true)=>Some(Access::Write),(false,false)=>None}
}

pub(super) async fn validate(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let definition=&instance.definition;
    if !instance.contracts()
        .any(|contract| matches!(contract.predicate, ContractPredicate::Initialized { .. })) {
        return Ok(());
    }
    let mut parameters=Vec::new();
    for param in instance.parameters(world) {
        let param=unsafe { world.call(param).await }?;
        let ty=unsafe { world.call(cooperative::parameter_type(world,&param,
            (&definition.source,&definition.module,&definition.imports,definition.function.kind()),Target::Cpu)).await }?;
        parameters.push(if param.ty.dataset {
            Ty::owned_dataset(ty.syntax(),unsafe { world.call(world.shape(&param.ty.count,&definition.source,
                param.span,&definition.module,&definition.imports)).await }?)
        } else { ty });
    }
    let result_name = unsafe { world.call(instance.bindings.type_name(world,&definition.function.result(),
        (&definition.source,&definition.module,&definition.imports),definition.function.span())).await }?;
    let result = if result_name.name == "void" || result_name.is_never() { None } else {
        let ty = unsafe { world.call(world.ty(&result_name, &definition.source, definition.function.span(),
            &definition.module, &definition.imports)).await }?;
        Some(if result_name.dataset {
            Ty::owned_dataset(ty.syntax(), unsafe { world.call(world.shape(&result_name.count, &definition.source,
                definition.function.span(), &definition.module, &definition.imports)).await }?)
        } else { ty })
    };
    unsafe { world.call(validate_named(world, instance, |index| &parameters[index], &result_name, result.as_ref())).await }
}
pub(super) async fn validate_named<'a>(world: &World, instance: &crate::generics::Instance,
    parameter_type: impl Fn(usize) -> &'a Ty, result_name: &TypeName, returned: Option<&Ty>,
) -> Result<()> {
    let definition=&instance.definition;
    let input = |name: &str| definition.function.params().iter().enumerate()
        .find(|(_, param)| param.name() == name)
        .map(|(index, param)| (param.access(), parameter_type(index).clone()));
    for contract in instance.contracts() {
        if definition.function.c_abi() && contract.kind == ContractKind::Requires
            && let ContractPredicate::Nonzero(path) = &contract.predicate {
            contracts::validate_nonzero_input(path, &input, &definition.source, contract.span)?;
            continue;
        }
        let ContractPredicate::Initialized { value, condition } = &contract.predicate else { continue; };
        if contract.kind != ContractKind::Ensures || definition.function.kind() != FunctionKind::Value {
            return definition.source.fail(contract.span, "B225", "initialized 是普通函数的 ensures 输出合同");
        }
        let condition = unsafe { world.call(normalize(world, Contract::from(instance).condition_scope(), result_name, condition, contract.span)).await }?;
        unsafe { world.call(validate_output(world,value, &condition, &input, returned, &definition.source, contract.span)).await }?;
    }
    Ok(())
}

pub(super) fn declared<'a>(definition: impl Into<Contract<'a>>, parameter: usize) -> bool {
    let definition = definition.into();
    let name = definition.parameter_name(parameter);
    definition.contracts().any(|contract| matches!(contract.predicate,
        ContractPredicate::Initialized { value, .. } if value.split('.').next() == Some(name.as_str())))
}

fn output_input_names(definition:Contract<'_>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for contract in definition.contracts() {
        if let ContractPredicate::Initialized { condition, .. } = &contract.predicate {
            add_input_names(condition, &mut names);
        }
    }
    names
}
async fn collect_inputs(
    builder: &mut Builder<'_>,
    definition: Contract<'_>,
    env: &BTreeMap<String, Binding>,
    names: &BTreeSet<String>,
) -> Result<BTreeMap<String, Value>> {
    let mut inputs = BTreeMap::new();
    for parameter in definition.parameters() {
        if !names.contains(parameter.name()) {
            continue;
        }
        let Some(binding) = env.get(parameter.name()) else {
            continue;
        };
        let value = match binding {
            Binding::Value(value) if parameter.access() == Access::Value => value.clone(),
            Binding::OwnedValue(value) if parameter.access() == Access::Value => value.value.clone(),
            Binding::Owned(owner) if parameter.access() == Access::Value => {
                unsafe { builder.world.call(builder.read_place(&owner.place, parameter.span(), None)).await }?
            }
            Binding::Data(data)=>{
                inputs.insert(format!("{}.length",parameter.name()),builder.data_length(data));
                continue;
            }
            Binding::ApplicationData(data) => {
                builder.returned_storage(&Returned::Dataset(data.clone()))
            }
            Binding::DataReference(data)=>builder.returned_storage(&Returned::Dataset(data.data.clone())),
            _ => continue,
        };
        inputs.insert(parameter.name().to_owned(), value);
    }
    Ok(inputs)
}
pub(super) async fn inputs<'a>(
    builder: &mut Builder<'_>,
    definition: impl Into<Contract<'a>>,
    env: &BTreeMap<String, Binding>,
) -> Result<BTreeMap<String, Value>> {
    let definition=definition.into();
    unsafe { builder.world.call(collect_inputs(builder,definition,env,&output_input_names(definition))).await }
}
pub(super) async fn condition_inputs(
    builder: &mut Builder<'_>,
    definition: Contract<'_>,
    env: &BTreeMap<String, Binding>,
    condition: &OutputCondition,
) -> Result<BTreeMap<String, Value>> {
    let mut names = BTreeSet::new();
    add_input_names(condition, &mut names);
    unsafe { builder.world.call(collect_inputs(builder, definition, env, &names)).await }
}

async fn capture_current_inputs(
    builder: &mut Builder<'_>,
    definition: Contract<'_>,
    names: &BTreeSet<String>,
) -> Result<BTreeMap<String, Value>> {
    let env = builder.env.iter()
        .filter(|(name, _)| names.contains(*name))
        .map(|(name, binding)| (name.clone(), binding.clone()))
        .collect();
    unsafe { builder.world.call(collect_inputs(builder, definition, &env, names)).await }
}
pub(super) async fn current_inputs<'a>(
    builder: &mut Builder<'_>,
    definition: impl Into<Contract<'a>>,
) -> Result<BTreeMap<String, Value>> {
    let definition = definition.into();
    unsafe { builder.world.call(capture_current_inputs(builder, definition, &output_input_names(definition))).await }
}
pub(super) async fn entry_inputs(
    builder: &mut Builder<'_>,
    instance: &crate::generics::Instance,
) -> Result<BTreeMap<String, Value>> {
    let names = output_input_names(instance.into());
    // A mutable owned aggregate must retain the actual ABI input identities
    // before a field's first load or replacement. The original Proof owns them.
    for parameter in &instance.definition.function.params() {
        if parameter.access() == Access::Value
            && names.contains(parameter.name())
            && let Some(Binding::OwnedValue(value)) = builder.env.get_mut(parameter.name())
        {
            value.value.proof = bounds::Proof::capture(&value.value);
        }
    }
    unsafe { builder.world.call(capture_current_inputs(builder, instance.into(), &names)).await }
}

pub(super) async fn expression(
    builder: &mut Builder<'_>,
    definition: Contract<'_>,
    condition: &OutputCondition,
    inputs: &BTreeMap<String, Value>,
    returned: Option<&Value>,
    span: Span,
    expected: Option<&Ty>,
) -> Result<Value> {
    async fn path(
        builder: &mut Builder<'_>,
        definition: Contract<'_>,
        value: &OutputCondition,
        inputs: &BTreeMap<String, Value>,
        returned: Option<&Value>,
        span: Span,
        terminal: bool,
    ) -> Result<Value> {
        match value {
            OutputCondition::Name(name) => {
                let value = if name == "returned" {
                    returned
                } else {
                    inputs.get(name)
                };
                if let Some(value) = value {
                    return Ok(value.clone());
                }
                if name == "returned" || definition.condition_scope().has_parameter(name)
                {
                    return definition.source.fail(
                        span,
                        "B225",
                        "输出条件只引用按值输入快照、资源长度和实际 returned",
                    );
                }
                let key = builder
                    .world
                    .resolve(name, &definition.module, &definition.imports, true).unique()
                    .filter(|key| {
                        builder.world.functions[key].function.kind() == FunctionKind::Constant
                    })
                    .ok_or_else(|| Diagnostic {
                        source: definition.source.clone(),
                        span,
                        code: "B225",
                        message: format!("输出条件没有输入或编译期常量 {name}"),
                    })?;
                if definition.public && !builder.world.functions[&key].public {
                    return definition.source.fail(
                        span,
                        "B225",
                        "公开输出条件的模块常量必须在接口中可见",
                    );
                }
                let constant = unsafe { builder.world.call(constant::evaluate(builder.world, &key, &[])).await }?;
                let (ty, bits, origin) = constant.value.clone();
                Ok(builder.evaluated_constant(ty, bits, origin))
            }
            OutputCondition::Member(base, field) => {
                if field=="length"&&let OutputCondition::Name(name)=base.as_ref()
                    &&let Some(value)=inputs.get(&format!("{name}.length")) {return Ok(value.clone());}
                let value = unsafe {
                    builder.world.call(path(builder, definition, base, inputs, returned, span, false)).await
                }?;
                if terminal && value.ty.has_variants() {
                    let Some(discriminant) = value.ty.declared_variant(field) else {
                        return builder.fail(span, "B225", "输出条件使用声明的返回变体名称");
                    };
                    let tag = builder.value_component(&value, 0);
                    let scalar = tag.ty.components[0].0;
                    let expected = builder.constant(scalar, Bits::from(discriminant));
                    return builder.binary("==", tag, expected, span, None);
                }
                if value.ty.dataset.is_some() && field == "length" {
                    return Ok(builder.value_component(&value, 1));
                }
                builder.member_value(value, field, span)
            }
            _ => builder.fail(
                span,
                "B225",
                "输出条件的字段选择来自输入或 returned 的声明布局",
            ),
        }
    }
    async fn lower(
        builder: &mut Builder<'_>, definition: Contract<'_>, condition: &OutputCondition,
        inputs: &BTreeMap<String, Value>, returned: Option<&Value>, span: Span,
    ) -> Result<Option<Value>> {
        let mut value = match condition {
            OutputCondition::Name(_) | OutputCondition::Member(..) => Some(unsafe {
                builder.world.call(path(builder, definition, condition, inputs, returned, span, true)).await
            }?),
            OutputCondition::Number(number) => builder.number_expression(number, span)?,
            OutputCondition::StaticU32(value) => Some(builder.constant(Scalar::U32, Bits::from(*value))),
            OutputCondition::Constant(value) => Some(builder.evaluated_constant(
                value.ty.clone(), value.bits.clone(), value.generated_origin.clone())),
            OutputCondition::Bool(value) => Some(builder.constant(Scalar::Bool, Bits::from(*value))),
            OutputCondition::Unary(op, value) => {
                if op == "-" && let OutputCondition::Number(number) = value.as_ref() {
                    Some(builder.negative_number(number, span)?)
                } else {
                    let Some(value) = unsafe { builder.world.call(lower(builder, definition, value, inputs, returned, span)).await }? else { return Ok(None); };
                    Some(builder.unary_value(op, value, span)?)
                }
            }
            OutputCondition::Binary(op, left, right) => {
                let Some(left) = unsafe { builder.world.call(lower(builder, definition, left, inputs, returned, span)).await }? else { return Ok(None); };
                if op == "&&" || op == "||" {
                    builder.require_bool(&left, span)?;
                    unsafe { builder.world.call(builder.short_circuit_value(op, left, span, async |builder| {
                        Ok(unsafe { builder.world.call(lower(builder, definition, right, inputs, returned, span)).await }?
                            .map(|value| returns::Returned::Value(Box::new(value))))
                    }, async |_, _| Ok(()), span)).await }?
                } else {
                    let Some(right) = unsafe { builder.world.call(lower(builder, definition, right, inputs, returned, span)).await }? else { return Ok(None); };
                    Some(builder.binary(op, left, right, span, None)?)
                }
            }
        };
        if let Some(value) = &mut value { builder.bound_value(value); }
        Ok(value)
    }
    let declaration = definition.declaration_origin().map(Arc::new);
    let first = builder.code.len();
    let previous_definition = builder.source_definition.take();
    let previous_context = builder.source_context.clone();
    let previous_site = builder.source_site.take();
    match definition.clauses {
        Clauses::Declaration(declaration) if !builder.scope.function().is_some_and(|instance| std::ptr::eq(declaration, instance)) => {
            (builder.source_definition, builder.source_context) = declaration.definition.syntax_context(Some(&declaration.name));
        }
        Clauses::Declaration(_) | Clauses::Signature(..) | Clauses::Native(..) => {
            builder.source_definition = previous_definition.clone();
        }
    }
    builder.source_site = builder.source_at(span, SourceSyntax::Expression);
    let result = async {
        let result = unsafe { builder.world.call(lower(builder, definition, condition, inputs, returned, span)).await };
        let value = result?.ok_or_else(|| Diagnostic {
            source: builder.scope.source().clone(),
            span,
            code: "B225",
            message: "输出条件必须产生bool值".into(),
        })?;
        if let Some(expected) = expected { builder.require_same(expected, &value.ty, span)?; }
        Ok(value)
    }.await;
    if let Some(declaration) = declaration {
        for instruction in &mut builder.code[first..] {
            let retained = OperationOrigin::from_instruction(instruction);
            instruction.generated_origin = Some(Arc::new(GeneratedOrigin {
                reason: GenerationReason::ContractCondition,
                trigger: Some(GenerationAnchor::ContractCondition { declaration: declaration.clone(), span, retained }),
            }));
        }
    }
    builder.source_definition = previous_definition;
    builder.source_context = previous_context;
    builder.source_site = previous_site;
    result
}

pub(super) async fn guards<'a>(
    builder: &mut Builder<'_>,
    definition: impl Into<Contract<'a>>,
    parameter: usize,
    ty: &Ty,
    inputs: &BTreeMap<String, Value>,
    returned: Option<&Value>,
) -> Result<Vec<Presence>> {
    let definition = definition.into();
    let clauses=output_clauses(definition,parameter,ty)?;
    unsafe { builder.world.call(evaluate_guards(builder,definition,ty,clauses.iter().map(|(parts,condition,span)|(parts.as_slice(),condition,*span)),inputs,returned)).await }
}

type GuardClause = (Vec<usize>, OutputCondition, Span);

fn output_clauses(
    definition:Contract<'_>, parameter:usize, ty:&Ty,
)->Result<Vec<GuardClause>> {
    let name = definition.parameter_name(parameter);
    let mut clauses = Vec::new();
    for contract in definition.contracts() {
        let (predicate, span) = (contract.predicate, contract.span);
        let ContractPredicate::Initialized { value, condition } = predicate else {
            continue;
        };
        let mut fields = value.split('.');
        if fields.next() != Some(name.as_str()) {
            continue;
        }
        let mut selected = ty.clone();
        let mut components = (0..ty.components.len()).collect::<Vec<_>>();
        for field in fields {
            let (next, projection) = member_type(&selected, field, definition.source, span)?;
            components = projection.iter().map(|i| components[*i]).collect();
            selected = next;
        }
        clauses.push((components, condition, span));
    }
    Ok(clauses)
}

async fn evaluate_guards<'a>(
    builder:&mut Builder<'_>, definition:Contract<'_>, ty:&Ty,
    clauses:impl IntoIterator<Item=(&'a [usize],&'a OutputCondition,Span)>, inputs:&BTreeMap<String,Value>, returned:Option<&Value>,
)->Result<Vec<Presence>> {
    let mut guards = vec![None::<Presence>; ty.components.len()];
    for (components,condition,span) in clauses {
        let when = Presence::input(&unsafe { builder.world.call(expression(
            builder,
            definition,
            condition,
            inputs,
            returned,
            span,
            Some(&Ty::scalar(Scalar::Bool)),
        )).await }?);
        for &component in components {
            guards[component] = Some(
                guards[component]
                    .take()
                    .map_or_else(|| when.clone(), |old| old.or(when.clone())),
            );
        }
    }
    Ok(guards
        .into_iter()
        .map(|when| when.unwrap_or(Presence::All))
        .collect())
}


// The task keeps the original output predicate and the actual input words it names.
#[derive(Clone)]
pub(super) struct TaskGuard {
    pub target:view::Borrow,
    pub ty:Ty,
    clauses:Vec<(Vec<usize>,OutputCondition)>,
    result:Ty,
    model:Option<Returned>,
    arguments:BTreeMap<ValueIdentity,bounds::Proof>,
    returned:Vec<Reg>,
    inputs:BTreeMap<String,(Ty,Vec<Option<bounds::Operand>>)>,
}

impl Ord for TaskGuard {
    fn cmp(&self,other:&Self)->std::cmp::Ordering {
        use crate::module::codec::Encode;
        self.bytes().cmp(&other.bytes())
    }
}
impl PartialOrd for TaskGuard {
    fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering> {Some(self.cmp(other))}
}
impl PartialEq for TaskGuard {
    fn eq(&self,other:&Self)->bool {self.cmp(other).is_eq()}
}
impl Eq for TaskGuard {}

impl TaskGuard {
    pub(super) async fn new(
        builder:&Builder<'_>, definition:Contract<'_>, parameter:usize,
        ty:&Ty, target:view::Borrow, inputs:&BTreeMap<String,Value>, result:&Ty, span:Span,
        model:Option<Returned>, arguments:&BTreeMap<ValueIdentity,bounds::Proof>, returned:&[Reg],
    )->Result<Self> {
        let mut clauses=Vec::new();
        let mut names = BTreeSet::new();
        for (components,condition,span) in output_clauses(definition,parameter,ty)? {
            let condition=unsafe { builder.world.call(bind_constants(builder.world,definition.condition_scope(),&condition,span)).await }?;
            add_input_names(&condition, &mut names);
            clauses.push((components,condition));
        }
        let inputs=inputs.iter()
            .filter(|(name,_)| names.contains(name.split_once('.').map_or(name.as_str(), |(root,_)| root)))
            .map(|(name,value)| {
            let words=value.regs.iter().enumerate().map(|(at,&reg)| Some(match &value.constant {
                Some(bits)=>bounds::Operand::Constant(value.ty.components[at].0,bits[at]),
                None=>bounds::Operand::Value(ValueIdentity::Register(reg)),
            })).collect();
            (name.clone(),(value.ty.clone(),words))
        }).collect();
        Ok(Self{target,ty:ty.clone(),clauses,result:result.clone(),model,
            arguments:arguments.clone(),returned:returned.to_vec(),inputs})
    }

    pub(super) fn frame_output_type(&self) -> Option<&Ty> {
        (!self.clauses.is_empty() && self.ty.contains_dataset()).then_some(&self.ty)
    }

    pub(super) fn map_inputs(&mut self, map:impl Fn(bounds::Operand)->Option<bounds::Operand>) {
        for (_,words) in self.inputs.values_mut() {
            for word in words {
                *word=word.and_then(&map);
            }
        }
    }

    pub(super) fn map_arguments(&mut self,map:impl Fn(&bounds::Proof)->bounds::Proof) {
        for argument in self.arguments.values_mut(){*argument=map(argument);}
    }

    pub(super) async fn written_value(
        &self,builder:&mut Builder<'_>,returned:Option<&Value>,
    )->Result<Returned> {
        // The ordinary written model names the output. Its consumers load the actual place.
        let regs=self.ty.components.iter().map(|(scalar,_)|builder.reg(*scalar)).collect();
        let value=Value{ty:self.ty.clone(),regs,proof:bounds::Proof::default(),
            uniform:false,literal:false,constant:None};
        let mut output=unsafe { builder.world.call(builder.template_returned(value,true)).await }?;
        if let Some(model)=&self.model {
            let mut bindings=self.arguments.clone();
            if let Some(returned)=returned {
                let proof=bounds::Proof::capture(returned);
                for (component,source) in self.returned.iter().enumerate() {
                    bindings.insert(ValueIdentity::Register(*source),proof.project(&[component],returned.regs.len()));
                }
            }
            let actual=builder.returned_storage(&output);
            let model_proof=bounds::Proof::capture(&builder.returned_storage(model));
            for (component,(source,target)) in model.registers().into_iter().zip(&actual.regs).enumerate() {
                let proof=bounds::Proof::default().with_identity(ValueIdentity::Register(*target));
                bindings.insert(ValueIdentity::Register(source),proof.clone());
                if let Some(id)=model_proof.known_identity(component) {
                    bindings.entry(id).or_insert(proof);
                }
            }
            native_function::translate_returned(&mut output,model,&bindings,false);
        }
        residency::visit(&mut output,&mut residency::Input::new(0),
            &mut |data,_|data.residency=residency::Residency::cpu());
        Ok(output)
    }

    pub(super) fn result_at_call(
        &self,builder:&mut Builder<'_>,returned:&[Option<ValueIdentity>],
        bindings:&BTreeMap<ValueIdentity,bounds::Proof>,
    )->Option<Value> {
        let operands=returned.iter().zip(&self.result.components).map(|(source,(scalar,_))| {
            let proof=bindings.get(source.as_ref()?)?;
            proof.known_identity(0).map(bounds::Operand::Value).or_else(||
                proof.known_value(&Ty::scalar(*scalar)).map(|bits|bounds::Operand::Constant(*scalar,bits[0])))
        }).collect::<Option<Vec<_>>>()?;
        if operands.len()!=self.result.components.len(){return None;}
        let mut regs=Vec::with_capacity(operands.len());
        for operand in operands {
            match operand {
                bounds::Operand::Constant(scalar,bits)=>regs.push(builder.constant(scalar,bits).regs[0]),
                bounds::Operand::Value(ValueIdentity::Register(reg))=>regs.push(reg),
                _=>return None,
            }
        }
        Some(Value{ty:self.result.clone(),regs,proof:bounds::Proof::default(),
            uniform:false,literal:false,constant:None})
    }

    pub(super) async fn guards(
        &self, builder:&mut Builder<'_>, returned:Option<&Value>, span:Span,
        map:impl Fn(bounds::Operand)->Option<bounds::Operand>,
    )->Result<Vec<Presence>> {
        let source=builder.scope.source().clone();
        let definition=Contract::signature(&source,"",&[],0,&[],span);
        let mut inputs=BTreeMap::new();
        for (name,(ty,words)) in &self.inputs {
            let mut regs=Vec::with_capacity(words.len());
            for word in words {
                match word.and_then(&map) {
                    Some(bounds::Operand::Constant(scalar,bits))=>regs.push(builder.constant(scalar,bits).regs[0]),
                    Some(bounds::Operand::Value(ValueIdentity::Register(reg)))=>regs.push(reg),
                    _=>return builder.fail(span,"B224","任务条件输出所引用的实际输入快照未交接"),
                }
            }
            inputs.insert(name.clone(),Value{ty:ty.clone(),regs,proof:bounds::Proof::default(),
                uniform:false,literal:false,constant:None});
        }
        let clauses=self.clauses.iter().map(|(parts,condition)|(parts.as_slice(),condition,span));
        unsafe { builder.world.call(evaluate_guards(builder,definition,&self.ty,clauses,&inputs,returned)).await }
    }
}

pub(super) async fn contract_predicates(
    builder: &mut Builder<'_>,
    instance: &crate::generics::Instance,
) -> Result<BTreeSet<(ContractKind, ContractPredicate)>> {
    let definition = &instance.definition;
    let mut predicates = BTreeSet::new();
    for contract in instance.contracts() {
        let mut predicate = contract.predicate;
        if let ContractPredicate::Initialized { condition, .. }|ContractPredicate::Cleanup { condition, .. } = &mut predicate {
            *condition = unsafe { builder.world.call(normalize(builder.world, Contract::from(instance).condition_scope(), &unsafe { builder.world.call(instance.bindings.type_name(builder.world,&definition.function.result(),
                (&definition.source,&definition.module,&definition.imports),definition.function.span())).await }?, condition, contract.span)).await }?;
        }
        if let ContractPredicate::Range { intervals, .. } = &mut predicate {
            *intervals = unsafe { builder.world.call(range_constants(builder.world,instance.into(),&LexicalScope::Function(instance.clone()),None,intervals,contract.span)).await }?
                .into_iter().map(|(lower, upper)| (OutputCondition::Constant(lower), OutputCondition::Constant(upper))).collect();
        }
        predicates.insert((contract.kind, predicate));
    }
    Ok(predicates)
}

pub(super) async fn range_constants(
    world: &World, definition: Contract<'_>, lexical:&LexicalScope, scalar: Option<Scalar>,
    intervals: &[(OutputCondition, OutputCondition)], span: Span,
) -> Result<Vec<(OutputConstant, OutputConstant)>> {
    let context=constant::constant_context(lexical.module());
    let(source_definition,source_context)=lexical.syntax_context(None);
    let mut builder=Builder::from_checked(world,lexical,&context,0,source_definition,source_context,BTreeSet::new());
    builder.in_function = true;
    let mut endpoint = async |value: &OutputCondition| -> Result<OutputConstant> {
        let value = unsafe { world.call(bind_constants(world, definition.condition_scope(), value, span)).await }?;
        let first = builder.code.len();
        let value = unsafe { world.call(expression(&mut builder, definition, &value, &BTreeMap::new(), None, span, None)).await }?;
        if !value.ty.is_numeric() || value.ty.components.len() != 1
            || value.ty.components[0].0 == Scalar::Bool
            || scalar.is_some_and(|scalar| value.ty != Ty::scalar(scalar))
            || value.constant.is_none()
            || builder.code[first..].iter().any(|inst| !matches!(&inst.kind, InstKind::Constant(..)))
        {
            return definition.source.fail(span, "B225", "range端点必须完全编译期求值，且为选中分量同类型的数值标量");
        }
        Ok(OutputConstant { ty: value.ty, bits: value.constant.unwrap(),
            generated_origin: builder.code.last().and_then(|inst| inst.generated_origin.clone()) })
    };
    let mut endpoints=Vec::new();
    for (lower,upper) in intervals {
        let lower=endpoint(lower).await?;
        let upper=endpoint(upper).await?;
        endpoints.push((lower,upper));
    }
    Ok(endpoints)
}

async fn normalize(world: &World, definition: ConditionScope<'_>, returned: &TypeName,
    value: &OutputCondition, span: Span,
) -> Result<OutputCondition> {
    let value = unsafe { world.call(bind_constants(world, definition, value, span)).await }?;
    let path = async |value: &OutputCondition| {
        fn components<'a>(value: &'a OutputCondition, fields: &mut Vec<&'a str>) -> Option<&'a str> {
            match value {
                OutputCondition::Name(name) => Some(name),
                OutputCondition::Member(base, field) => { let name = components(base, fields)?; fields.push(field); Some(name) }
                _ => None,
            }
        }
        let mut fields = Vec::new();
        let name = components(value, &mut fields).ok_or_else(|| Diagnostic {
            source: definition.source.clone(), span, code: "B225", message: "输出条件的字段来自输入或returned".into(),
        })?;
        if name == "returned" && (returned.name == "void" || returned.is_never()) {
            return definition.source.fail(span, "B225", "无返回值的函数没有returned输入");
        }
        let syntax = if name == "returned" { Some(Cow::Borrowed(returned)) }
            else { unsafe { world.call(definition.parameter_type(world,name,span)).await }? };
        let syntax = syntax.ok_or_else(|| Diagnostic { source: definition.source.clone(), span, code: "B225",
            message: format!("输出条件没有输入{name}") })?;
        unsafe { world.call(world.output_path_type(&syntax, &fields, definition.source, span, definition.module, definition.imports)).await }
    };
    unsafe { world.call(contract_condition::identity(world,&value,&path, definition.source, span)).await }
}

pub(crate) async fn bind_function_contracts(
    world: &World, result: &TypeName, parameters: &[(Access, TypeName)], outputs: &[ContractPredicate],
    source: &crate::Source, span: Span, module: &str, imports: &[String],
) -> Result<Vec<ContractPredicate>> {
    if outputs.is_empty() { return Ok(Vec::new()); }
    let contract = ConditionScope { source, module, imports, public: false,
        parameters: ConditionParameters::Positional(parameters) };
    let mut bound = BTreeSet::new();
    for predicate in outputs {
        match predicate {
            ContractPredicate::Nonzero(path) => { bound.insert(ContractPredicate::Nonzero(path.clone())); }
            ContractPredicate::Memory(access) => { bound.insert(ContractPredicate::Memory(*access)); }
            ContractPredicate::Initialized { value, condition } => {
                bound.insert(ContractPredicate::Initialized { value: value.clone(),
                    condition: unsafe { world.call(normalize(world, contract, result, condition, span)).await }? });
            }
            _ => return source.fail(span, "B225", "cfn合同使用requires nonzero、ensures initialized及ensures memory"),
        }
    }
    Ok(bound.into_iter().collect())
}
fn typed_path(value: &OutputCondition, input: &impl Fn(&str) -> Option<(Access, Ty)>,
    returned: Option<&Ty>, source: &Source, span: Span) -> Result<Ty> {
    match value {
        OutputCondition::Name(name) if name == "returned" => returned.cloned().ok_or_else(|| Diagnostic {
            source: source.clone(), span, code: "B225", message: "无返回值的函数没有returned输入".into(),
        }),
        OutputCondition::Name(name) => {
            let Some((access, ty)) = input(name) else {
                return source.fail(span, "B225", format!("输出条件没有输入{name}"));
            };
            if access == Access::Value || ty.dataset.is_some() { Ok(ty) }
            else { source.fail(span, "B225", "输出条件只引用按值输入快照、资源长度和实际returned") }
        }
        OutputCondition::Member(base, field) => {
            let ty = typed_path(base, input, returned, source, span)?;
            if (ty.array.is_some() || ty.dataset.is_some()) && field == "length" {
                return Ok(Ty::scalar(Scalar::U32));
            }
            if ty.has_variants() {
                return if ty.declared_variant(field).is_some() {
                    Ok(Ty::scalar(Scalar::Bool))
                } else { source.fail(span, "B225", "输出条件使用声明的返回变体名称") };
            }
            member_type_only(&ty, field, source, span)
        }
        _ => source.fail(span, "B225", "输出条件的字段来自输入或returned"),
    }
}
async fn validate_output(world:&World,value: &str, condition: &OutputCondition,
    input: &impl Fn(&str) -> Option<(Access, Ty)>, returned: Option<&Ty>,
    source: &Source, span: Span,
) -> Result<()> {
    let mut fields = value.split('.');
    let root = fields.next().unwrap();
    let Some((Access::Write | Access::InOut, mut ty)) = input(root) else {
        return source.fail(span, "B225", "initialized需要实际write／inout参数或字段");
    };
    // Dataset output positions describe initialized elements; input/return
    // conditions consume the original handle/length storage type.
    if let Some(reference)=&ty.dataset {ty=unsafe { world.call(world.ty(reference,source,span,"",&[])).await }?;}
    for field in fields { ty = member_type_only(&ty, field, source, span)?; }
    let path = async |value: &OutputCondition| typed_path(value, input, returned, source, span);
    let (condition, _) = unsafe { world.call(condition_type(world,condition,&path, source, span)).await }?;
    operators::same(&Ty::scalar(Scalar::Bool), &condition, source, span)
}

pub(crate) async fn validate_function_contracts(
    world:&World,parameters: &[CallParameter], result: Option<&Ty>, outputs: &[ContractPredicate],
    source: &crate::Source, span: Span,
) -> Result<()> {
    let input = |name: &str| parameters.get(positional_index(name))
        .map(|parameter| match &parameter.kind {
            CallParameterKind::Value(ty) => (Access::Value, ty.clone()),
            CallParameterKind::Reference { ty, access } => (*access, ty.clone()),
        });
    for output in outputs {
        match output {
            ContractPredicate::Nonzero(path) => contracts::validate_nonzero_input(path, &input, source, span)?,
            ContractPredicate::Initialized { value, condition } => unsafe { world.call(validate_output(world,value, condition, &input, result, source, span)).await }?,
            ContractPredicate::Memory(_) => {},
            _ => return source.fail(span, "B225", "cfn合同使用requires nonzero、ensures initialized及ensures memory"),
        }
    }
    Ok(())
}

pub(super) fn resource_guard(ty: &Ty, offset: u32, guards: &[Presence]) -> Presence {
    guards[ty
        .components
        .iter()
        .position(|(_, at)| *at == offset)
        .unwrap()]
    .clone()
}

pub(super) fn apply_ownership(model: &mut Returned, ty: &Ty, offset: u32, guards: &[Presence]) {
    match model {
        Returned::Dataset(data) => {
            data.ownership = Ownership::Owned.restricted(&resource_guard(ty, offset, guards))
        }
        Returned::OwnedValue(value) => {
            if value.value.ty.has_owned_presence() {
                value.ownership = Ownership::Owned.restricted(&resource_guard(ty, offset, guards));
            }
            for (index, payload) in value.payloads.iter_mut().enumerate() {
                if let Some(payload) = payload {
                    let at = if value.value.ty.has_variants() {
                        value.value.ty.variant_payload_offset()
                    } else {
                        value.value.ty.aggregate_part(index).0
                    };
                    apply_ownership(payload, ty, offset + at, guards);
                }
            }
        }
        _ => {}
    }
}

impl Builder<'_> {
    pub(super) fn forget_output_initialization(&mut self, place: &Place, span: Span) -> Result<()> {
        let changed=self.positions(place);self.forget_output_positions(&changed,span)
    }
    pub(super) fn forget_output_positions(&mut self,changed:&[Position],span:Span)->Result<()> {
        fn index(builder: &mut Builder<'_>, index: ownership::ApplicationIndex) -> Value {
            match index {
                ownership::ApplicationIndex::Constant(value) => {
                    builder.constant(Scalar::U32, Bits::from(value))
                }
                ownership::ApplicationIndex::Value(id, reg) => Value {
                    ty: Ty::scalar(Scalar::U32),
                    regs: vec![reg],
                    proof: bounds::Proof::default().with_identity(id),
                    uniform: false,
                    literal: false,
                    constant: None,
                },
            }
        }
        fn overlap(
            builder: &mut Builder<'_>,
            a: &Position,
            b: &Position,
            span: Span,
        ) -> Result<Presence> {
            if a == b {
                return Ok(Presence::All);
            }
            Ok(match (a, b) {
                (Position::Parameter(a,word),Position::ApplicationElement(b,_,other)) if a==b && *word==*other as usize=>Presence::All,
                (Position::Indexed(left, i), Position::Indexed(right, j)) if left == right => {
                    let left = index(builder, *i);
                    let right = index(builder, *j);
                    Presence::input(&builder.binary("==", left, right, span,None)?)
                }
                (Position::Indexed(choices, index), other)
                | (other, Position::Indexed(choices, index)) => {
                    let mut all = Presence::None;
                    for (element, choice) in choices.iter().enumerate() {
                        all = all.or(Presence::indices(Comparison::Equal,*index, ownership::ApplicationIndex::Constant(element as u32))
                            .and(overlap(builder, choice, other, span)?));
                    }
                    all
                }
                (Position::Conditional(when, a, b), other)
                | (other, Position::Conditional(when, a, b)) => when
                    .as_ref()
                    .clone()
                    .and(overlap(builder, a, other, span)?)
                    .or(when
                        .as_ref()
                        .clone()
                        .not()
                        .and(overlap(builder, b, other, span)?)),
                (Position::Both(a, b), other) | (other, Position::Both(a, b)) => {
                    overlap(builder, a, other, span)?.or(overlap(builder, b, other, span)?)
                }
                _ => Presence::None,
            })
        }
        fn changed_at(builder:&mut Builder<'_>,value:&Position,changed:&[Position],span:Span)->Result<Presence> {
            let mut when=Presence::None;
            for position in changed {
                when=when.or(overlap(builder,position,value,span)?);
            }
            Ok(when)
        }
        fn erase(
            builder: &mut Builder<'_>,
            value: &Position,
            changed: &[Position],
            span: Span,
        ) -> Result<Position> {
            Ok(match value {
                Position::Conditional(when, a, b) => Position::selected(
                    when.as_ref().clone(),
                    erase(builder, a, changed, span)?,
                    erase(builder, b, changed, span)?,
                ),
                Position::Both(a, b) => Position::both(
                    erase(builder, a, changed, span)?,
                    erase(builder, b, changed, span)?,
                ),
                Position::Initialization(effect)
                    if let address::InitializationEffect::Position(fact)=effect.as_ref()
                    && let Some(parts)=fact.position_parts(&builder.path) =>
                {
                    let mut surviving=Position::Initialized;
                    for (mut part,position) in parts {
                        if let Some(position)=position {
                            let retained=changed_at(builder,&position,changed,span)?.not().condition();
                            part.when=part.when.logical(retained,false);
                        }
                        if part.when.active() {
                            surviving=Position::both(surviving,Position::Initialization(Arc::new(
                                address::InitializationEffect::Position(part),
                            )));
                        }
                    }
                    surviving
                },
                value => Position::selected(changed_at(builder,value,changed,span)?,Position::Initialized,value.clone()),
            })
        }
        let previous = std::mem::take(&mut self.initialized);
        for position in previous {
            let position = erase(self, &position, changed, span)?;
            self.initialized.insert(position);
        }
        Ok(())
    }
    pub(super) async fn prepare_data_output(
        &mut self,
        data: &mut application::Data,
        span: Span,
    ) -> Result<()> {
        fn materialize(value:&Position,scope:&mut impl FnMut()->Position)->Option<Position>{
            match value {
                Position::Initialized=>Some(scope()),
                Position::Conditional(_,a,b)|Position::Both(a,b)=>{
                    let left=materialize(a,scope);let right=materialize(b,scope);
                    if left.is_none()&&right.is_none(){return None;}
                    let left=left.unwrap_or_else(||a.as_ref().clone());let right=right.unwrap_or_else(||b.as_ref().clone());
                    Some(match value{Position::Conditional(when,..)=>Position::selected(when.as_ref().clone(),left,right),_=>Position::both(left,right)})
                },
                Position::Indexed(choices,index)=>{
                    let changed=choices.iter().map(|value|materialize(value,scope)).collect::<Vec<_>>();
                    if changed.iter().all(Option::is_none){return None;}
                    Some(Position::Indexed(choices.iter().zip(changed).map(|(old,new)|new.unwrap_or_else(||old.clone())).collect(),*index))
                },
                _=>None,
            }
        }
        let old=data.initialization.clone().unwrap_or(Position::Initialized);
        let mut created=None;
        let initialization=materialize(&old,&mut||created.get_or_insert_with(||Position::Parameter(self.application_output_scope(),0)).clone());
        let Some(initialization)=initialization else{return Ok(());};
        let scope=created.unwrap();
        let mut found = false;
        if let Some(place) = &data.place {
            let root = place.owned_positions().unwrap()[0].clone();
            for (_, _, model) in self.owned_place_candidates(place) {
                if let Returned::Dataset(mut candidate) = model {
                    let actual = candidate.place.as_ref().unwrap().clone();
                    let selected = Presence::selected(&root, &actual.owned_positions().unwrap()[0]);
                    let old = candidate
                        .initialization
                        .clone()
                        .unwrap_or(Position::Initialized);
                    let scoped=materialize(&old,&mut||scope.clone()).unwrap_or_else(||old.clone());
                    candidate.initialization =
                        Some(Position::selected(selected, scoped, old));
                    unsafe { self.world.call(self.assign_owned_place(&actual, Returned::Dataset(candidate))).await }?;
                    found = true;
                }
            }
        } else {
            for binding in self.env.values_mut() {
                let paths = view::data_paths(binding)
                    .into_iter()
                    .filter(|(_, owner, _)| owner.root.identity() == data.root.identity())
                    .map(|(path, _, _)| path)
                    .collect::<Vec<_>>();
                for path in paths {
                    let owner = view::data_at_mut(binding, &path).unwrap();
                    let old=owner.initialization.clone().unwrap_or(Position::Initialized);
                    owner.initialization=Some(materialize(&old,&mut||scope.clone()).unwrap_or(old));
                    found = true;
                }
            }
        }
        if !found {
            return self.fail(span, "B229", "条件dataset输出没有可交接的原初始化所有者");
        }
        self.initialized.extend(data.ty.components.iter().map(|(_,word)|scope.application_word(None,*word)));
        data.initialization = Some(initialization);
        Ok(())
    }
    pub(super) fn forget_data_output(&mut self,data:&application::DataReference,span:Span)->Result<()> {
        let scope=data.initialization.as_ref().unwrap();
        let changed=data.words().into_iter().map(|word|scope.application_word(None,word)).collect::<Vec<_>>();
        self.forget_output_positions(&changed,span)
    }
    pub(super) fn initialize_data_output(&mut self, data: &application::DataReference, guards: &[Presence]) {
        if let Some(scope) = &data.initialization {
            for (offset, when) in data.words().into_iter().zip(guards) {
                self.initialized.insert(Position::selected(
                    when.clone(),
                    scope.application_word(None, offset),
                    Position::Initialized,
                ));
            }
        }
    }
    pub(super) fn initialize_output(&mut self, place: &Place, guards: &[Presence]) {
        self.initialize_output_positions(self.positions(place),guards);
    }

    pub(super) fn initialize_output_positions(&mut self, positions:Vec<Position>, guards:&[Presence]) {
        for (position, when) in positions.into_iter().zip(guards) {
            if self.owned_possible(when) {
                self.initialized.insert(Position::selected(
                    when.clone(),
                    position,
                    Position::Initialized,
                ));
            }
        }
    }


}

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(TaskGuard);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        target: Slot,
        ty: Slot,
        clauses: Slot,
        result: Slot,
        model: Slot,
        arguments: Slot,
        returned: Slot,
        inputs: Slot,
    }
    #[repr(C)]
    struct Writer {
        target: WriteCallback,
        ty: WriteCallback,
        clauses: WriteCallback,
        result: WriteCallback,
        model: WriteCallback,
        arguments: WriteCallback,
        returned: WriteCallback,
        inputs: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        target: ReadCallback,
        ty: ReadCallback,
        clauses: ReadCallback,
        result: ReadCallback,
        model: ReadCallback,
        arguments: ReadCallback,
        returned: ReadCallback,
        inputs: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_task_guard"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_task_guard"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for TaskGuard {
        fn empty() -> Self {
            Self {
                target: <view::Borrow as Native>::empty(),
                ty: <Ty as Native>::empty(),
                clauses: <Vec<(Vec<usize>, crate::ast::OutputCondition)> as Native>::empty(),
                result: <Ty as Native>::empty(),
                model: <Option<Returned> as Native>::empty(),
                arguments: <BTreeMap<ValueIdentity, bounds::Proof> as Native>::empty(),
                returned: <Vec<Reg> as Native>::empty(),
                inputs: <BTreeMap<String, (Ty, Vec<Option<bounds::Operand>>)> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        target: contracts::address(&self.target).cast_mut(),
                        ty: contracts::address(&self.ty).cast_mut(),
                        clauses: contracts::address(&self.clauses).cast_mut(),
                        result: contracts::address(&self.result).cast_mut(),
                        model: contracts::address(&self.model).cast_mut(),
                        arguments: contracts::address(&self.arguments).cast_mut(),
                        returned: contracts::address(&self.returned).cast_mut(),
                        inputs: contracts::address(&self.inputs).cast_mut(),
                    },
                    Writer {
                        target: contracts::write_value::<view::Borrow>,
                        ty: contracts::write_value::<Ty>,
                        clauses: contracts::write_value::<
                            Vec<(Vec<usize>, crate::ast::OutputCondition)>,
                        >,
                        result: contracts::write_value::<Ty>,
                        model: contracts::write_value::<Option<Returned>>,
                        arguments: contracts::write_value::<BTreeMap<ValueIdentity, bounds::Proof>>,
                        returned: contracts::write_value::<Vec<Reg>>,
                        inputs: contracts::write_value::<
                            BTreeMap<String, (Ty, Vec<Option<bounds::Operand>>)>,
                        >,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        target: contracts::slot(&mut output.target),
                        ty: contracts::slot(&mut output.ty),
                        clauses: contracts::slot(&mut output.clauses),
                        result: contracts::slot(&mut output.result),
                        model: contracts::slot(&mut output.model),
                        arguments: contracts::slot(&mut output.arguments),
                        returned: contracts::slot(&mut output.returned),
                        inputs: contracts::slot(&mut output.inputs),
                    },
                    Reader {
                        target: contracts::read_value::<view::Borrow>,
                        ty: contracts::read_value::<Ty>,
                        clauses: contracts::read_value::<
                            Vec<(Vec<usize>, crate::ast::OutputCondition)>,
                        >,
                        result: contracts::read_value::<Ty>,
                        model: contracts::read_value::<Option<Returned>>,
                        arguments: contracts::read_value::<BTreeMap<ValueIdentity, bounds::Proof>>,
                        returned: contracts::read_value::<Vec<Reg>>,
                        inputs: contracts::read_value::<
                            BTreeMap<String, (Ty, Vec<Option<bounds::Operand>>)>,
                        >,
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
