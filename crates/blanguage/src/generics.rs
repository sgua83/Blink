//! 从声明绑定具体类型与静态值；具体实例沿原入口检查和 IR 执行。
use crate::{World, ast::*, ir::Ty};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn is_open(types:&crate::syntax::List<crate::syntax::wire::TypeParameterNode>,values:&crate::syntax::List<crate::syntax::wire::ValueParameterNode>)->bool {
    types.iter().any(|p|p.family().is_none())||values.iter().any(|p|p.choices().is_none())
}
#[repr(C)]
#[derive(Clone, Copy)]
struct NameBytes { bytes: *const u8, length: u64 }
impl NameBytes {
    fn new(text: &str) -> Self { Self { bytes: text.as_ptr(), length: text.len() as u64 } }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct InstanceName {
    base: NameBytes,
    types: *const NameBytes,
    type_count: u64,
    values: *const u32,
    value_count: u64,
}
unsafe extern "C" {
    #[link_name = "bootstrap.instance_domain::key_size"]
    fn instance_key_size(name: InstanceName) -> u64;
    #[link_name = "bootstrap.instance_domain::write_key"]
    fn write_instance_key(name: InstanceName, output: *mut u8);
    #[link_name = "bootstrap.instance_domain::matches_key"]
    fn matches_instance_key(name: InstanceName, actual: NameBytes) -> bool;
}

fn with_instance_name<'a, T>(
    base: &str,
    types: impl IntoIterator<Item = &'a str>,
    values: &[u32],
    consume: impl FnOnce(InstanceName) -> T,
) -> T {
    let types = types.into_iter().map(NameBytes::new).collect::<Vec<_>>();
    consume(InstanceName {
        base: NameBytes::new(base),
        types: types.as_ptr(),
        type_count: types.len() as u64,
        values: values.as_ptr(),
        value_count: values.len() as u64,
    })
}

pub(crate) fn key<'a>(base: &str, types: impl IntoIterator<Item = &'a str>, values: &[u32]) -> String {
    with_instance_name(base, types, values, |name| {
        // B initializes the final UTF-8 bytes while the input is still borrowed.
        let length = unsafe { instance_key_size(name) } as usize;
        let mut bytes = Vec::with_capacity(length);
        unsafe {
            write_instance_key(name, bytes.as_mut_ptr());
            bytes.set_len(length);
            String::from_utf8_unchecked(bytes)
        }
    })
}

pub(crate) fn record_key_matches(base: &str, actual: &Ty) -> bool {
    with_instance_name(base, actual.arguments.iter().map(|ty| ty.name.as_str()), &actual.values, |name| {
        // B compares against the existing name without producing another string.
        unsafe { matches_instance_key(name, NameBytes::new(&actual.name)) }
    })
}
#[derive(Default)]
pub(crate) struct Bindings {
    pub types:BTreeMap<String,TypeName>,
    pub values:BTreeMap<String,u32>,
}

// A bound call reads the original instance; it does not rewrite its source node.
pub(crate) enum TypeCall<'a> { Builtin(&'a str), Construct(&'a TypeName) }
impl Bindings {
    pub(crate) fn type_call(&self,name:&str)->Option<TypeCall<'_>> {
        let actual=self.types.get(name)?;
        Some(if actual.arguments.is_empty()&&crate::builtin_types::Builtin::recognize(&actual.name).is_scalar() {
            TypeCall::Builtin(&actual.name)
        }else{TypeCall::Construct(actual)})
    }
    fn use_names(&self) -> Vec<crate::syntax::wire::BoundName> {
        self.types.iter().map(|(name, ty)| crate::syntax::wire::BoundName {
            bytes: name.as_ptr(), length: name.len() as u64, kind: 1,
            wrapped: u32::from(ty.dataset || ty.view || ty.tile),
        }).chain(self.values.keys().map(|name| crate::syntax::wire::BoundName {
            bytes: name.as_ptr(), length: name.len() as u64, kind: 2, wrapped: 0,
        })).collect()
    }
    fn use_result(error: crate::syntax::wire::UseError, source: &Source) -> Result<()> {
        if error.code == 0 { return Ok(()); }
        let (code, message) = [
            ("B232", "类型参数已代表完整具体类型，不能再附加类型实参"),
            ("B232", "资源包装的元素需要普通存储类型，不能再次包装资源"),
            ("B232", "类型参数已代表完整类型，不能再附加实参"),
            ("B235", "const 参数不是可调用函数"),
            ("B235", "局部绑定不能覆盖本函数的 const 参数"),
        ][error.code as usize - 1];
        source.fail(Span { start: error.span.start as usize, end: error.span.end as usize }, code, message)
    }
    fn check_type(&self, ty: &crate::syntax::Type, source: &Source, span: Span) -> Result<()> {
        let span = crate::syntax::wire::Span { start: span.start as u64, end: span.end as u64 };
        Self::use_result(ty.bound_uses(&self.use_names(), span), source)
    }
    fn check_function(&self, function: &crate::syntax::Function, source: &Source) -> Result<()> {
        Self::use_result(function.bound_uses(&self.use_names()), source)
    }
    pub(crate) fn check_expression(&self, expression: &crate::syntax::Expr, source: &Source) -> Result<()> {
        Self::use_result(expression.bound_uses(&self.use_names()), source)
    }
    pub(crate) fn type_name(
        self: &std::sync::Arc<Self>, world: &World, source_type: &crate::syntax::Type,
        context: (&Source, &str, &[String]), span: Span,
    ) -> Result<TypeName> {
        self.check_type(source_type, context.0, span)?;
        self.bound_type_name(world, source_type, context, span)
    }
    pub(crate) fn arguments(
        self: &std::sync::Arc<Self>, world: &World,
        arguments: &crate::syntax::List<crate::syntax::wire::ArgumentNode>,
        context: (&Source, &str, &[String]), span: Span,
    ) -> Result<Vec<GenericArgument>> {
        self.bind_arguments(world, arguments, context, span, |ty| self.type_name(world, ty, context, span))
    }
    fn bound_type_name(
        self: &std::sync::Arc<Self>, world: &World, source_type: &crate::syntax::Type,
        context: (&Source, &str, &[String]), span: Span,
    ) -> Result<TypeName> {
        let (source, module, imports) = context;
        let arguments = self.bind_arguments(world, &source_type.arguments(), context, span,
            |ty| self.bound_type_name(world, ty, context, span))?;
        let count = source_type.count().map(|shape|
            world.source_shape(&shape, self, source, span, module, imports).map(Shape::Literal)).transpose()?;
        let mut result = if let Some(actual) = self.types.get(source_type.name()) {
            actual.clone()
        } else {
            let contracts = source_type.contracts().map(|contracts| {
                let parameters = arguments.len().saturating_sub(1);
                let value_of = |name: &str| {
                    let value = *self.values.get(name)?;
                    let positional = name.strip_prefix("argument")
                        .and_then(|index| index.parse::<usize>().ok())
                        .is_some_and(|index| index < parameters && name == format!("argument{index}"));
                    (!positional).then_some(value)
                };
                Box::new(contracts.iter().map(|predicate| bound_predicate(&predicate, value_of)).collect())
            });
            TypeName {
                name: source_type.name().into(), canonical: source_type.canonical(), arguments,
                dataset: false, view: false, tile: false, count: None,
                contracts,
            }
        };
        if source_type.dataset() || source_type.view() || source_type.tile() {
            result.dataset = source_type.dataset();
            result.view = source_type.view();
            result.tile = source_type.tile();
            result.count = count;
        } else if !self.types.contains_key(source_type.name()) {
            result.count = count;
        }
        Ok(result)
    }
    fn bind_arguments(
        self: &std::sync::Arc<Self>, world: &World,
        arguments: &crate::syntax::List<crate::syntax::wire::ArgumentNode>,
        context: (&Source, &str, &[String]), span: Span,
        mut bind_type: impl FnMut(&crate::syntax::Type) -> Result<TypeName>,
    ) -> Result<Vec<GenericArgument>> {
        let (source, module, imports) = context;
        arguments.iter().map(|argument| match argument.kind() {
            crate::syntax::ArgumentKind::Type(ty) => {
                if ty.arguments().is_empty() && !ty.dataset() && !ty.view() && !ty.tile()
                    && let Some(&value) = self.values.get(ty.name())
                {
                    return Ok(GenericArgument::Value(Shape::Literal(value)));
                }
                Ok(GenericArgument::Type(bind_type(&ty)?))
            }
            crate::syntax::ArgumentKind::Value(shape) => {
                Ok(GenericArgument::Value(Shape::Literal(world.source_shape(&shape, self, source, span, module, imports)?)))
            }
        }).collect()
    }
}

#[derive(Clone)]
pub(crate) struct Instance {
    pub name:String,
    pub definition:std::sync::Arc<crate::Definition>,
    pub bindings:std::sync::Arc<Bindings>,
}
impl Instance {
    pub(crate) fn parameter(&self, world:&World, parameter:&crate::syntax::Param)->Result<Param>{
        let definition=&self.definition;
        Ok(Param{name:parameter.name().into(),access:parameter.access(),span:parameter.span(),
            ty:self.bindings.type_name(world,&parameter.ty(),
                (&definition.source,&definition.module,&definition.imports),parameter.span())?})
    }
    pub(crate) fn parameters<'a>(&'a self,world:&'a World)->impl Iterator<Item=Result<Param>>+'a{
        self.definition.function.params().into_iter().map(move|parameter|self.parameter(world,&parameter))
    }
    pub(crate) fn contracts(&self) -> impl Iterator<Item = FunctionContract> + '_ {
        self.definition.function.contracts().iter()
            .filter_map(|contract| bound_contract(contract, &self.bindings.values))
    }
}
// Source clauses stay in B storage. These are the actual instance's semantic
// contract values consumed by checking and the compiled module interface.
fn bound_contract(contract: crate::syntax::Contract, values: &BTreeMap<String, u32>) -> Option<FunctionContract> {
    let predicate = contract.predicate();
    if contract.kind() == ContractKind::Requires
        && let crate::syntax::PredicateKind::Nonzero(name) = predicate.kind()
        && values.contains_key(name)
    {
        // instantiate has already checked this static requirement.
        return None;
    }
    Some(FunctionContract { kind: contract.kind(),
        predicate: bound_predicate(&predicate, |name| values.get(name).copied()), span: contract.span() })
}
fn bound_predicate(
    predicate: &crate::syntax::Predicate, value_of: impl Fn(&str) -> Option<u32> + Copy,
) -> ContractPredicate {
    use crate::syntax::PredicateKind as K;
    let output_value = |name: &str| if name == "returned" { None } else { value_of(name) };
    match predicate.kind() {
        K::Prefix { data, count, field } => ContractPredicate::Prefix {
            data: data.into(), count: count.into(), field: field.into() },
        K::Nonzero(value) => ContractPredicate::Nonzero(value.into()),
        K::Nonempty(value) => ContractPredicate::Nonempty(value.into()),
        K::Contiguous(value) => ContractPredicate::Contiguous(value.into()),
        K::Memory(access) => ContractPredicate::Memory(access),
        K::Index { value, resource } => ContractPredicate::Index { value: value.into(), resource: resource.into() },
        K::Range { value, intervals } => ContractPredicate::Range { value: value.into(),
            intervals: intervals.iter().map(|interval| (
                bound_condition(interval.lower(), value_of),
                bound_condition(interval.upper(), value_of))).collect() },
        K::SameLength { left, right } => ContractPredicate::SameLength { left: left.into(), right: right.into() },
        K::Initialized { value, condition } => ContractPredicate::Initialized {
            value: value.into(), condition: bound_condition(condition, output_value) },
        K::Cleanup { value, condition } => ContractPredicate::Cleanup {
            value: value.into(), condition: bound_condition(condition, output_value) },
    }
}
fn bound_condition(condition: crate::syntax::Condition, value_of: impl Fn(&str) -> Option<u32> + Copy) -> OutputCondition {
    use crate::syntax::ConditionKind as K;
    match condition.kind() {
        K::Name(name) => match value_of(name) {
            Some(value) => OutputCondition::StaticU32(value),
            None => OutputCondition::Name(name.into()),
        },
        K::Number(value) => OutputCondition::Number(value.into()),
        K::StaticU32(value) => OutputCondition::StaticU32(value),
        K::Bool(value) => OutputCondition::Bool(value),
        K::Member(base, field) => OutputCondition::Member(Box::new(bound_condition(base, value_of)), field.into()),
        K::Unary(op, value) => OutputCondition::Unary(op.into(), Box::new(bound_condition(value, value_of))),
        K::Binary(op, left, right) => OutputCondition::Binary(op.into(),
            Box::new(bound_condition(left, value_of)), Box::new(bound_condition(right, value_of))),
    }
}
#[derive(Clone)]
pub(crate) struct TypeArgument {
    pub name: String,
    pub syntax: TypeName,
}

pub(crate) fn instantiate(
    world:&World,
    base:&str,
    definition:&std::sync::Arc<crate::Definition>,
    arguments: Vec<TypeArgument>,
    values:&[u32],
)->Result<Instance> {
    let function=&definition.function;
    if function.kind()!=FunctionKind::Constant&&function.body().is_some()
        &&let Some(inputs)=&world.build_declarations {
        inputs.borrow_mut().insert(crate::build::Declaration::Implementation(base.into()));
    }
    if function.external().is_some()&&let Some(inputs)=&world.build_declarations {
        inputs.borrow_mut().insert(crate::build::Declaration::External(base.into()));
    }
    if function.types().is_empty()&&function.values().is_empty() {
        return Ok(Instance{name:base.into(),definition:definition.clone(),
            bindings:std::sync::Arc::new(Bindings::default())});
    }
    let name = key(base, arguments.iter().map(|argument| argument.name.as_str()), values);
    let types = function.types().iter().zip(arguments)
        .map(|(parameter, argument)| (parameter.name().to_owned(), argument.syntax))
        .collect();
    let value_map=function.values().iter().zip(values).map(|(parameter,value)|(parameter.name().to_owned(),*value)).collect();
    let bindings=Bindings{types,values:value_map};
    for contract in function.contracts().iter() {
        if contract.kind()==ContractKind::Requires
            &&let crate::syntax::PredicateKind::Nonzero(name)=contract.predicate().kind()
            &&let Some(value)=bindings.values.get(name)
            &&*value==0 {
            return definition.source.fail(contract.span(),"B226",
                format!("const 参数 {name}=0 不满足 requires nonzero({name})"));
        }
    }
    bindings.check_function(function, &definition.source)?;
    let instance=Instance{name,definition:definition.clone(),bindings:std::sync::Arc::new(bindings)};
    if function.c_abi(){crate::check::external::check_declaration(world,&instance)?;}
    world.check_interface_types(definition,&instance.bindings)?;
    Ok(instance)
}
pub(crate) fn validate_declaration(definition: &crate::Definition) -> Result<()> {
    let f = &definition.function;
    crate::check::validate_signature(definition)?;
    let error = unsafe { crate::syntax::wire::function_parameter_error(f.signature_node()) };
    parameter_diagnostic(error, &definition.source, f.span())
}
pub(crate) fn instances(
    world: &World,
    base: &str,
    definition: &std::sync::Arc<crate::Definition>,
) -> Result<Vec<Instance>> {
    let f = &definition.function;
    let mut result = Vec::new();
    for_each_combination(
        world,
        &f.types(),
        &f.values(),
        (&definition.source, f.span(), &definition.module, &definition.imports),
        |arguments, values| {
            result.push(instantiate(world, base, definition, arguments, values)?);
            Ok(())
        },
    )?;
    Ok(result)
}
pub(crate) fn validate_record_parameters(record: &crate::syntax::Record, source: &Source) -> Result<()> {
    let error = unsafe { crate::syntax::wire::record_parameter_error(record.signature_node()) };
    parameter_diagnostic(error, source, record.span())
}

fn parameter_diagnostic(error: crate::syntax::wire::ParameterError, source: &Source, span: Span) -> Result<()> {
    use crate::syntax::wire::ParameterError;
    match error {
        ParameterError::Valid => Ok(()),
        ParameterError::TypeName => source.fail(span, "B231", "类型参数不能覆盖内置类型或当前声明名"),
        ParameterError::EmptyConstDomain => source.fail(span, "B235", "const 参数至少需要一个允许的 u32 值"),
        ParameterError::ConstName => source.fail(span, "B235", "const 参数不能覆盖内置名称、当前声明名或普通参数"),
    }
}

pub(crate) fn for_each_combination(
    world: &World,
    types: &crate::syntax::List<crate::syntax::wire::TypeParameterNode>,
    values: &crate::syntax::List<crate::syntax::wire::ValueParameterNode>,
    (source, span, module, imports): (&Source, Span, &str, &[String]),
    mut consume: impl FnMut(Vec<TypeArgument>, &[u32]) -> Result<()>,
) -> Result<()> {
    let mut domains = Vec::with_capacity(types.len());
    for parameter in types {
        let members = match parameter.family() {
            Some(family) => world.family_arguments(&family, source, span, module, imports, &mut BTreeSet::new())?,
            None => Vec::new(),
        };
        domains.push(members);
    }
    let sizes: Vec<_> = domains.iter().map(|domain| domain.len() as u64).collect();
    values.instance_combinations(&sizes).try_for_each(|indices, row| {
        let arguments = domains.iter().zip(indices)
            .map(|(domain, &index)| domain[index as usize].clone())
            .collect();
        consume(arguments, row)
    })
}
