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
    pub(crate) fn type_name(
        self: &std::sync::Arc<Self>, world: &World, source_type: &crate::syntax::Type,
        context: (&Source, &str, &[String]), span: Span,
    ) -> Result<TypeName> {
        let (source, module, imports) = context;
        validate_type_uses(source_type.clone(), self, source, span)?;
        let arguments = self.arguments(world, &source_type.arguments(), context, span)?;
        let count = source_type.count().map(|shape|
            world.source_shape(&shape, self, source, span, module, imports).map(Shape::Literal)).transpose()?;
        let mut result = if let Some(actual) = self.types.get(source_type.name()) {
            actual.clone()
        } else {
            TypeName {
                name: source_type.name().into(), canonical: source_type.canonical(), arguments,
                dataset: false, view: false, tile: false, count: None,
                contracts: source_type.contracts().map(|contracts|
                    Box::new(contracts.iter().map(|predicate| bound_predicate(&predicate, &self.values)).collect())),
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
    pub(crate) fn arguments(
        self: &std::sync::Arc<Self>, world: &World,
        arguments: &crate::syntax::List<crate::syntax::wire::ArgumentNode>,
        context: (&Source, &str, &[String]), span: Span,
    ) -> Result<Vec<GenericArgument>> {
        let (source, module, imports) = context;
        arguments.iter().map(|argument| match argument.kind() {
            crate::syntax::ArgumentKind::Type(ty) => {
                let mut argument = GenericArgument::Type(self.type_name(world, &ty, context, span)?);
                crate::value_parameters::substitute_argument(&mut argument, &self.values, source)?;
                Ok(argument)
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
        predicate: bound_predicate(&predicate, values), span: contract.span() })
}
fn bound_predicate(predicate: &crate::syntax::Predicate, values: &BTreeMap<String, u32>) -> ContractPredicate {
    use crate::syntax::PredicateKind as K;
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
                bound_condition(interval.lower(), values),
                bound_condition(interval.upper(), values))).collect() },
        K::SameLength { left, right } => ContractPredicate::SameLength { left: left.into(), right: right.into() },
        K::Initialized { value, condition } => ContractPredicate::Initialized {
            value: value.into(), condition: bound_condition(condition, values) },
        K::Cleanup { value, condition } => ContractPredicate::Cleanup {
            value: value.into(), condition: bound_condition(condition, values) },
    }
}
fn bound_condition(condition: crate::syntax::Condition, values: &BTreeMap<String, u32>) -> OutputCondition {
    use crate::syntax::ConditionKind as K;
    match condition.kind() {
        K::Name(name) => match values.get(name) {
            Some(value) => OutputCondition::StaticU32(*value),
            None => OutputCondition::Name(name.into()),
        },
        K::Number(value) => OutputCondition::Number(value.into()),
        K::StaticU32(value) => OutputCondition::StaticU32(value),
        K::Bool(value) => OutputCondition::Bool(value),
        K::Member(base, field) => OutputCondition::Member(Box::new(bound_condition(base, values)), field.into()),
        K::Unary(op, value) => OutputCondition::Unary(op.into(), Box::new(bound_condition(value, values))),
        K::Binary(op, left, right) => OutputCondition::Binary(op.into(),
            Box::new(bound_condition(left, values)), Box::new(bound_condition(right, values))),
    }
}
pub(crate) fn instantiate(
    world:&World,
    base:&str,
    definition:&std::sync::Arc<crate::Definition>,
    arguments:Vec<TypeName>,
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
    let names=arguments.iter().map(|ty| {
        if *ty==TypeName::named("void"){return Ok("void".to_owned());}
        let actual=world.ty(ty,&definition.source,function.span(),&definition.module,&definition.imports)?;
        Ok(if ty.dataset {
            Ty::owned_dataset(actual.syntax(),world.shape(&ty.count,&definition.source,function.span(),&definition.module,&definition.imports)?).name
        }else{actual.name})
    }).collect::<Result<Vec<_>>>()?;
    let types=function.types().iter().zip(arguments).map(|(parameter,ty)|(parameter.name().to_owned(),ty)).collect();
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
    validate_type_uses(function.result(),&bindings,&definition.source,function.span())?;
    for parameter in function.params().iter(){
        validate_type_uses(parameter.ty(),&bindings,&definition.source,parameter.span())?;
    }
    if let Some(body)=function.body(){validate_block_uses(body,&bindings,&definition.source)?;}
    let instance=Instance{name:key(base,names.iter().map(String::as_str),values),definition:definition.clone(),bindings:std::sync::Arc::new(bindings)};
    if function.c_abi(){crate::check::external::check_declaration(world,&instance)?;}
    world.check_interface_types(definition,&instance.bindings)?;
    Ok(instance)
}
// Retain the original static-use rules while removing the AST writeback.
// The actual type/value binding is read by the instance's semantic consumers.
fn validate_type_uses(ty:crate::syntax::Type,bindings:&Bindings,source:&Source,span:Span)->Result<()> {
    validate_argument_uses(ty.arguments(),bindings,source,span)?;
    if let Some(actual)=bindings.types.get(ty.name()) {
        if !ty.arguments().is_empty(){
            return source.fail(span,"B232","类型参数已代表完整具体类型，不能再附加类型实参");
        }
        if (ty.dataset()||ty.view()||ty.tile())&&(actual.dataset||actual.view||actual.tile){
            return source.fail(span,"B232","资源包装的元素需要普通存储类型，不能再次包装资源");
        }
    }
    if let Some(shape)=ty.count(){validate_shape_uses(shape,bindings,source)?;}
    Ok(())
}
fn validate_argument_uses(arguments:crate::syntax::List<crate::syntax::wire::ArgumentNode>,bindings:&Bindings,source:&Source,span:Span)->Result<()> {
    for argument in arguments {
        match argument.kind(){
            crate::syntax::ArgumentKind::Type(ty)=>validate_type_uses(ty,bindings,source,span)?,
            crate::syntax::ArgumentKind::Value(shape)=>validate_shape_uses(shape,bindings,source)?,
        }
    }
    Ok(())
}
fn validate_shape_uses(shape:crate::syntax::Shape,bindings:&Bindings,source:&Source)->Result<()> {
    if let crate::syntax::ShapeKind::Expression(expression)=shape.kind(){validate_expr_uses(expression,bindings,source)?;}
    Ok(())
}
fn validate_reference_uses(name:&str,arguments:crate::syntax::List<crate::syntax::wire::ArgumentNode>,bindings:&Bindings,source:&Source,span:Span)->Result<()> {
    if bindings.types.contains_key(name){return source.fail(span,"B232","类型参数已代表完整类型，不能再附加实参");}
    if bindings.values.contains_key(name){return source.fail(span,"B235","const 参数不是可调用函数");}
    validate_argument_uses(arguments,bindings,source,span)
}
fn validate_expressions(expressions:crate::syntax::List<crate::syntax::wire::ExprNode>,bindings:&Bindings,source:&Source)->Result<()> {
    for expression in expressions{validate_expr_uses(expression,bindings,source)?;}Ok(())
}
pub(crate) fn validate_expr_uses(expression:crate::syntax::Expr,bindings:&Bindings,source:&Source)->Result<()> {
    use crate::syntax::ExprKind as K;
    match expression.kind(){
        K::Invoke(callee,args)=>{validate_expr_uses(callee,bindings,source)?;validate_expressions(args,bindings,source)?;},
        K::Construct(ty,args)=>{validate_type_uses(ty,bindings,source,expression.span())?;validate_expressions(args,bindings,source)?;},
        K::Call(name,args)=>{
            if bindings.values.contains_key(name){return source.fail(expression.span(),"B235","const 参数不是可调用函数");}
            validate_expressions(args,bindings,source)?;
        },
        K::Specialize(name,explicit,args)|K::Cooperate(name,Some(explicit),args)=>{
            validate_reference_uses(name,explicit,bindings,source,expression.span())?;validate_expressions(args,bindings,source)?;
        },
        K::FunctionRef(name,explicit)=>validate_reference_uses(name,explicit,bindings,source,expression.span())?,
        K::Cooperate(name,None,args)=>{
            if bindings.values.contains_key(name){return source.fail(expression.span(),"B235","const 参数不是可调用函数");}
            validate_expressions(args,bindings,source)?;
        },
        K::Target(_,base)|K::Member(base,_)|K::Unary(_,base)=>validate_expr_uses(base,bindings,source)?,
        K::Index(a,b)|K::Binary(_,a,b)=>{validate_expr_uses(a,bindings,source)?;validate_expr_uses(b,bindings,source)?;},
        K::Select(a,b,c)=>{validate_expr_uses(a,bindings,source)?;validate_expr_uses(b,bindings,source)?;validate_expr_uses(c,bindings,source)?;},
        K::Name(_)|K::Number(_)|K::StaticU32(_)|K::Bool(_)=>{},
    }
    Ok(())
}
fn validate_block_uses(body:crate::syntax::List<crate::syntax::wire::StmtNode>,bindings:&Bindings,source:&Source)->Result<()> {
    use crate::syntax::StmtKind as K;
    for statement in body {
        let kind=statement.kind();
        match &kind {
            K::Declare(_,name)|K::Let(_,name,_)|K::For(name,_,_,_)|K::Parallel(name,_,_)
                if bindings.values.contains_key(*name)=>{
                return source.fail(statement.span(),"B235","局部绑定不能覆盖本函数的 const 参数");
            },
            _=>{},
        }
        match kind {
            K::Declare(ty,_)=>validate_type_uses(ty,bindings,source,statement.span())?,
            K::Let(annotation,_,expression)=>{
                if let Some(ty)=annotation{validate_type_uses(ty,bindings,source,statement.span())?;}
                validate_expr_uses(expression,bindings,source)?;
            },
            K::Assign(a,_,b)=>{validate_expr_uses(a,bindings,source)?;validate_expr_uses(b,bindings,source)?;},
            K::Expr(e)|K::Seek(_,e)|K::Return(Some(e))=>validate_expr_uses(e,bindings,source)?,
            K::If(c,a,b)=>{validate_expr_uses(c,bindings,source)?;validate_block_uses(a,bindings,source)?;validate_block_uses(b,bindings,source)?;},
            K::Match(value,arms)=>{
                validate_expr_uses(value,bindings,source)?;for arm in arms{validate_block_uses(arm.body(),bindings,source)?;}
            },
            K::While(c,body)=>{validate_expr_uses(c,bindings,source)?;validate_block_uses(body,bindings,source)?;},
            K::For(_,a,b,body)=>{
                validate_expr_uses(a,bindings,source)?;if let Some(b)=b{validate_expr_uses(b,bindings,source)?;}validate_block_uses(body,bindings,source)?;
            },
            K::Parallel(_,domain,body)=>{
                if let Some(e)=domain.count(){validate_expr_uses(e,bindings,source)?;}
                if let Some(e)=domain.selection(){validate_expr_uses(e,bindings,source)?;}
                validate_block_uses(body,bindings,source)?;
            },
            K::Return(None)|K::Break|K::Continue|K::Phase=>{},
        }
    }
    Ok(())
}
pub(crate) fn validate_declaration(definition: &crate::Definition) -> Result<()> {
    let f = &definition.function;
    crate::check::validate_signature(definition)?;
    validate_parameters(&f.types(), f.name(), &definition.source, f.span())?;
    crate::value_parameters::validate(&f.values(), f.name(), f.params().iter(), &definition.source, f.span())
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
pub(crate) fn validate_parameters(
    types: &crate::syntax::List<crate::syntax::wire::TypeParameterNode>,
    owner: &str,
    source: &Source,
    span: Span,
) -> Result<()> {
    for parameter in types {
        if crate::builtin_types::Builtin::recognize(parameter.name()).is_present()
            || matches!(
                parameter.name(),
                "void"
                    | "never"
                    | "dataset"
                    | "view"
                    | "tile"
                    | "array"
                    | "result"
                    | "acceleration"
                    | "texture2d" | "texture3d"
            )
            || parameter.name() == owner
        {
            return source.fail(span, "B231", "类型参数不能覆盖内置类型或当前声明名");
        }
    }
    Ok(())
}
pub(crate) fn for_each_combination(
    world: &World,
    types: &crate::syntax::List<crate::syntax::wire::TypeParameterNode>,
    values: &crate::syntax::List<crate::syntax::wire::ValueParameterNode>,
    (source, span, module, imports): (&Source, Span, &str, &[String]),
    mut consume: impl FnMut(Vec<TypeName>, &[u32]) -> Result<()>,
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
pub(crate) fn substitute_type(
    ty: &mut TypeName,
    replacements: &BTreeMap<String, TypeName>,
    source: &Source,
    span: Span,
) -> Result<()> {
    for argument in &mut ty.arguments {
        match argument {
            GenericArgument::Type(ty)=>substitute_type(ty,replacements,source,span)?,
            GenericArgument::Value(Shape::Expression(expression))=>substitute_expr(expression,replacements,&BTreeMap::new(),source)?,
            _=>{},
        }
    }
    if let Some(actual) = replacements.get(&ty.name) {
        if !ty.arguments.is_empty() {
            return source.fail(
                span,
                "B232",
                "类型参数已代表完整具体类型，不能再附加类型实参",
            );
        }
        let (dataset, view, tile, count) = (ty.dataset, ty.view, ty.tile, ty.count.clone());
        *ty = actual.clone();
        if dataset || view || tile {
            if ty.dataset || ty.view || ty.tile {return source.fail(span,"B232","资源包装的元素需要普通存储类型，不能再次包装资源");}
            ty.dataset = dataset;
            ty.view = view;
            ty.tile = tile;
            ty.count = count;
        }
    }
    if let Some(Shape::Expression(expression))=&mut ty.count {
        substitute_expr(expression,replacements,&BTreeMap::new(),source)?;
    }
    Ok(())
}
fn substitute_reference(
    name: &str,
    explicit: &mut [GenericArgument],
    types: &BTreeMap<String, TypeName>,
    values: &BTreeMap<String, u32>,
    source: &Source,
    span: Span,
) -> Result<()> {
    if types.contains_key(name) {
        return source.fail(span, "B232", "类型参数已代表完整类型，不能再附加实参");
    }
    if values.contains_key(name) {
        return source.fail(span, "B235", "const 参数不是可调用函数");
    }
    for arg in explicit {
        if let GenericArgument::Type(ty) = arg {
            substitute_type(ty, types, source, span)?;
        }
        crate::value_parameters::substitute_argument(arg, values,source)?;
    }
    Ok(())
}
pub(crate) fn substitute_expr(
    expr: &mut Expr,
    types: &BTreeMap<String, TypeName>,
    values: &BTreeMap<String, u32>,
    source: &Source,
) -> Result<()> {
    match &mut expr.kind {
        ExprKind::Name(name) if values.contains_key(name) => {
            expr.kind = ExprKind::StaticU32(values[name]);
        }
        ExprKind::Invoke(callee, args) => {
            substitute_expr(callee, types, values, source)?;
            for arg in args { substitute_expr(arg, types, values, source)?; }
        }
        ExprKind::Construct(ty, args) => {
            substitute_type(ty, types, source, expr.span)?;
            crate::value_parameters::substitute_type(ty, values,source)?;
            for arg in args {
                substitute_expr(arg, types, values, source)?;
            }
        }
        ExprKind::Call(name, args) => {
            if values.contains_key(name) {
                return source.fail(expr.span, "B235", "const 参数不是可调用函数");
            }
            for arg in args.iter_mut() {
                substitute_expr(arg, types, values, source)?;
            }
            if let Some(actual) = types.get(name) {
                if actual.arguments.is_empty()
                    && crate::builtin_types::Builtin::recognize(&actual.name).is_scalar()
                {
                    name.clone_from(&actual.name);
                } else {
                    expr.kind = ExprKind::Construct(actual.clone(), std::mem::take(args));
                }
            }
        }
        ExprKind::Specialize(name, explicit, args)
        | ExprKind::Cooperate(name, Some(explicit), args) => {
            substitute_reference(name, explicit, types, values, source, expr.span)?;
            for arg in args {
                substitute_expr(arg, types, values, source)?;
            }
        }
        ExprKind::FunctionRef(name, explicit) => {
            substitute_reference(name, explicit, types, values, source, expr.span)?;
        }
        ExprKind::Cooperate(name, None, args) => {
            if values.contains_key(name) {
                return source.fail(expr.span, "B235", "const 参数不是可调用函数");
            }
            for arg in args {
                substitute_expr(arg, types, values, source)?;
            }
        }
        ExprKind::Target(_,base) | ExprKind::Member(base, _) | ExprKind::Unary(_, base) => {
            substitute_expr(base, types, values, source)?
        }
        ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
            substitute_expr(a, types, values, source)?;
            substitute_expr(b, types, values, source)?;
        }
        ExprKind::Select(a, b, c) => {
            substitute_expr(a, types, values, source)?;
            substitute_expr(b, types, values, source)?;
            substitute_expr(c, types, values, source)?;
        }
        _ => {}
    }
    Ok(())
}
