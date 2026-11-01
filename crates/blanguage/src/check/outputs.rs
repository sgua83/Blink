//! Declared output initialization is part of the original function contract.
//! Its predicates use input snapshots and the real returned value, never a
//! runtime initialization flag or a second resource owner.
use super::*;
use application::Ownership;
use owned_presence::Presence;
use returns::Returned;

#[derive(Clone, Copy)]
enum Clauses<'a> {
    Declaration(&'a [FunctionContract]),
    Signature(&'a [ContractPredicate], Span),
}
/// A borrowed view of the original declaration or bound function-value contract.
#[derive(Clone, Copy)]
pub(super) struct Contract<'a> {
    source: &'a crate::Source,
    module: &'a str,
    imports: &'a [String],
    public: bool,
    pub(super) parameters: &'a [Param],
    clauses: Clauses<'a>,
}
impl<'a> From<&'a Definition> for Contract<'a> {
    fn from(definition: &'a Definition) -> Self {
        Self { source: &definition.source, module: &definition.module,
            imports: &definition.imports, public: definition.public,
            parameters: &definition.function.params,
            clauses: Clauses::Declaration(&definition.function.contracts) }
    }
}
impl<'a> From<&'a Arc<Definition>> for Contract<'a> {
    fn from(definition: &'a Arc<Definition>) -> Self { definition.as_ref().into() }
}
impl<'a> Contract<'a> {
    pub(super) fn signature(source: &'a crate::Source, module: &'a str, imports: &'a [String],
        parameters: &'a [Param], outputs: &'a [ContractPredicate], span: Span) -> Self {
        Self { source, module, imports, public: false, parameters,
            clauses: Clauses::Signature(outputs, span) }
    }
    fn len(self) -> usize {
        match self.clauses { Clauses::Declaration(clauses) => clauses.len(),
            Clauses::Signature(clauses, _) => clauses.len() }
    }
    fn at(self, index: usize) -> (ContractKind, &'a ContractPredicate, Span) {
        match self.clauses {
            Clauses::Declaration(clauses) => {
                let clause = &clauses[index]; (clause.kind, &clause.predicate, clause.span)
            }
            Clauses::Signature(clauses, span) => (ContractKind::Ensures, &clauses[index], span),
        }
    }
}

pub(super) fn validate(world: &World, definition: &Definition) -> Result<()> {
    if !definition.function.contracts.iter()
        .any(|contract| matches!(contract.predicate, ContractPredicate::Initialized { .. })) {
        return Ok(());
    }
    let parameters = definition.function.params.iter().map(|param| {
        let ty = cooperative::parameter_type(world, param, definition)?;
        Ok(if param.ty.dataset {
            Ty::owned_dataset(ty, world.shape(&param.ty.count, &definition.source, param.span,
                &definition.module, &definition.imports)?)
        } else { ty })
    }).collect::<Result<Vec<_>>>()?;
    let result = &definition.function.result;
    let result = if result.name == "void" || result.is_never() { None } else {
        let ty = world.ty(result, &definition.source, definition.function.span,
            &definition.module, &definition.imports)?;
        Some(if result.dataset {
            Ty::owned_dataset(ty, world.shape(&result.count, &definition.source,
                definition.function.span, &definition.module, &definition.imports)?)
        } else { ty })
    };
    validate_named(world, definition, &parameters, result.as_ref())
}
pub(super) fn validate_named(world: &World, definition: &Definition,
    parameters: &[Ty], returned: Option<&Ty>,
) -> Result<()> {
    let input = |name: &str| definition.function.params.iter().enumerate()
        .find(|(_, param)| param.name == name)
        .map(|(index, param)| (param.access, parameters[index].clone()));
    for contract in &definition.function.contracts {
        let ContractPredicate::Initialized { value, condition } = &contract.predicate else { continue; };
        if contract.kind != ContractKind::Ensures || definition.function.kind != FunctionKind::Value
            || definition.function.target != Target::Cpu {
            return definition.source.fail(contract.span, "B225", "initialized 是普通CPU函数的 ensures 输出合同");
        }
        let condition = normalize(world, definition.into(), condition, contract.span)?;
        validate_output(value, &condition, &input, returned, &definition.source, contract.span)?;
    }
    Ok(())
}

pub(super) fn declared<'a>(definition: impl Into<Contract<'a>>, parameter: usize) -> bool {
    let definition = definition.into();
    let name = &definition.parameters[parameter].name;
    (0..definition.len()).any(|index| matches!(definition.at(index).1,
        ContractPredicate::Initialized { value, .. } if value.split('.').next() == Some(name)))
}

fn input_names(definition: &Definition) -> BTreeSet<String> {
    fn visit(condition: &OutputCondition, names: &mut BTreeSet<String>) {
        match condition {
            OutputCondition::Name(name) if name != "returned" => {
                names.insert(name.clone());
            }
            OutputCondition::Member(base, _) | OutputCondition::Unary(_, base) => {
                visit(base, names)
            }
            OutputCondition::Binary(_, a, b) => {
                visit(a, names);
                visit(b, names);
            }
            _ => {}
        }
    }
    let mut names = BTreeSet::new();
    for contract in &definition.function.contracts {
        if let ContractPredicate::Initialized { condition, .. } = &contract.predicate {
            visit(condition, &mut names);
        }
    }
    names
}
fn collect_inputs(
    builder: &mut Builder<'_>,
    definition: &Definition,
    env: &BTreeMap<String, Binding>,
    names: &BTreeSet<String>,
) -> Result<BTreeMap<String, Value>> {
    let mut inputs = BTreeMap::new();
    for parameter in &definition.function.params {
        if !names.contains(&parameter.name) {
            continue;
        }
        let Some(binding) = env.get(&parameter.name) else {
            continue;
        };
        let value = match binding {
            Binding::Value(value) if parameter.access == Access::Value => value.clone(),
            Binding::OwnedValue(value) if parameter.access == Access::Value => value.value.clone(),
            Binding::Owned(owner) if parameter.access == Access::Value => {
                builder.read_place(&owner.place, parameter.span)?
            }
            Binding::ApplicationData(data) => {
                builder.returned_storage(&Returned::Dataset(data.clone()))
            }
            Binding::DataReference(data)=>builder.returned_storage(&Returned::Dataset(data.data.clone())),
            _ => continue,
        };
        inputs.insert(parameter.name.clone(), value);
    }
    Ok(inputs)
}
pub(super) fn inputs(
    builder: &mut Builder<'_>,
    definition: &Definition,
    env: &BTreeMap<String, Binding>,
) -> Result<BTreeMap<String, Value>> {
    collect_inputs(builder, definition, env, &input_names(definition))
}
pub(super) fn entry_inputs(
    builder: &mut Builder<'_>,
    definition: &Definition,
) -> Result<BTreeMap<String, Value>> {
    let names = input_names(definition);
    // A mutable owned aggregate must retain the actual ABI input identities
    // before a field's first load or replacement. The original Proof owns them.
    for parameter in &definition.function.params {
        if parameter.access == Access::Value
            && names.contains(&parameter.name)
            && let Some(Binding::OwnedValue(value)) = builder.env.get_mut(&parameter.name)
        {
            value.value.proof = bounds::Proof::capture(&value.value);
        }
    }
    let env = builder
        .env
        .iter()
        .filter(|(name, _)| names.contains(*name))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    collect_inputs(builder, definition, &env, &names)
}

fn expression(
    builder: &mut Builder<'_>,
    definition: Contract<'_>,
    condition: &OutputCondition,
    inputs: &BTreeMap<String, Value>,
    returned: Option<&Value>,
    span: Span,
) -> Result<Value> {
    fn atom(values: &mut BTreeMap<String, Binding>, value: Value, span: Span) -> Expr {
        let name = format!("\0output-condition:{}", values.len());
        values.insert(name.clone(), Binding::Value(value));
        Expr {
            kind: ExprKind::Name(name),
            span,
        }
    }
    fn path(
        builder: &mut Builder<'_>,
        definition: Contract<'_>,
        value: &OutputCondition,
        inputs: &BTreeMap<String, Value>,
        returned: Option<&Value>,
        span: Span,
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
                if name == "returned" || definition.parameters.iter().any(|p| p.name == *name)
                {
                    return definition.source.fail(
                        span,
                        "B225",
                        "输出条件只引用按值输入快照、资源长度和实际 returned",
                    );
                }
                let key = builder
                    .world
                    .resolve(name, &definition.module, &definition.imports, true)
                    .filter(|key| {
                        builder.world.functions[key].function.kind == FunctionKind::Constant
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
                let (ty, bits) = constant_value(builder.world, &key)?;
                Ok(builder.constant_value(ty, bits))
            }
            OutputCondition::Member(base, field) => {
                let value = path(builder, definition, base, inputs, returned, span)?;
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
    fn convert(
        builder: &mut Builder<'_>,
        definition: Contract<'_>,
        condition: &OutputCondition,
        inputs: &BTreeMap<String, Value>,
        returned: Option<&Value>,
        values: &mut BTreeMap<String, Binding>,
        span: Span,
    ) -> Result<Expr> {
        let kind = match condition {
            OutputCondition::Name(_) => {
                return Ok(atom(
                    values,
                    path(builder, definition, condition, inputs, returned, span)?,
                    span,
                ));
            }
            OutputCondition::Member(base, field) => {
                let value = path(builder, definition, base, inputs, returned, span)?;
                if value.ty.has_variants() {
                    let Some(variant) = value.ty.variants.iter().find(|v| v.name == *field) else {
                        return builder.fail(span, "B225", "输出条件使用声明的返回变体名称");
                    };
                    let discriminant = variant.discriminant;
                    let tag = builder.value_component(&value, 0);
                    let scalar = tag.ty.components[0].0;
                    let expected = builder.constant(scalar, Bits::from(discriminant));
                    ExprKind::Binary(
                        "==".into(),
                        Box::new(atom(values, tag, span)),
                        Box::new(atom(values, expected, span)),
                    )
                } else {
                    return Ok(atom(
                        values,
                        path(builder, definition, condition, inputs, returned, span)?,
                        span,
                    ));
                }
            }
            OutputCondition::Number(number) => ExprKind::Number(number.clone()),
            OutputCondition::StaticU32(value) => ExprKind::StaticU32(*value),
            OutputCondition::Constant(value) => {
                return Ok(atom(
                    values,
                    builder.constant_value(value.ty.clone(), value.bits.clone()),
                    span,
                ));
            }
            OutputCondition::Bool(value) => ExprKind::Bool(*value),
            OutputCondition::Unary(op, value) => ExprKind::Unary(
                op.clone(),
                Box::new(convert(
                    builder, definition, value, inputs, returned, values, span,
                )?),
            ),
            OutputCondition::Binary(op, a, b) => ExprKind::Binary(
                op.clone(),
                Box::new(convert(
                    builder, definition, a, inputs, returned, values, span,
                )?),
                Box::new(convert(
                    builder, definition, b, inputs, returned, values, span,
                )?),
            ),
        };
        Ok(Expr { kind, span })
    }
    let mut values = BTreeMap::new();
    let expression = convert(
        builder,
        definition,
        condition,
        inputs,
        returned,
        &mut values,
        span,
    )?;
    let outer = std::mem::replace(&mut builder.env, values);
    let result = builder.expr(&expression);
    builder.env = outer;
    let value = result?.ok_or_else(|| Diagnostic {
        source: builder.definition.source.clone(),
        span,
        code: "B225",
        message: "输出条件必须产生bool值".into(),
    })?;
    builder.require_bool(&value, span)?;
    Ok(value)
}

pub(super) fn guards<'a>(
    builder: &mut Builder<'_>,
    definition: impl Into<Contract<'a>>,
    parameter: usize,
    ty: &Ty,
    inputs: &BTreeMap<String, Value>,
    returned: Option<&Value>,
) -> Result<Vec<Presence>> {
    let definition = definition.into();
    let name = &definition.parameters[parameter].name;
    let mut guards = vec![None::<Presence>; ty.components.len()];
    for index in 0..definition.len() {
        let (_, predicate, span) = definition.at(index);
        let ContractPredicate::Initialized { value, condition } = predicate else {
            continue;
        };
        let mut fields = value.split('.');
        if fields.next() != Some(name) {
            continue;
        }
        let mut selected = ty.clone();
        let mut components = (0..ty.components.len()).collect::<Vec<_>>();
        for field in fields {
            let (next, projection) = member_type(&selected, field, definition.source, span)?;
            components = projection.iter().map(|i| components[*i]).collect();
            selected = next;
        }
        let when = Presence::input(&expression(
            builder,
            definition,
            condition,
            inputs,
            returned,
            span,
        )?);
        for component in components {
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

pub(super) fn contract_predicates(
    builder: &mut Builder<'_>,
    definition: &Definition,
) -> Result<BTreeSet<(ContractKind, ContractPredicate)>> {
    let mut predicates = BTreeSet::new();
    for contract in &definition.function.contracts {
        let mut predicate = contract.predicate.clone();
        if let ContractPredicate::Initialized { condition, .. } = &mut predicate {
            *condition = normalize(builder.world, definition.into(), condition, contract.span)?;
        }
        predicates.insert((contract.kind, predicate));
    }
    Ok(predicates)
}

fn normalize(
    world: &World,
    definition: Contract<'_>,
    value: &OutputCondition,
    span: Span,
) -> Result<OutputCondition> {
    Ok(match value {
        OutputCondition::Name(name)
            if name != "returned" && !definition.parameters.iter().any(|p| p.name == *name) =>
        {
            let key = world.resolve(name, definition.module, definition.imports, true)
                .filter(|key| world.functions[key].function.kind == FunctionKind::Constant)
                .ok_or_else(|| Diagnostic {
                    source: definition.source.clone(), span, code: "B225",
                    message: format!("输出条件没有编译期常量 {name}"),
                })?;
            if definition.public && !world.functions[&key].public {
                return definition.source.fail(span, "B225", "公开输出条件的模块常量必须在接口中可见");
            }
            let (ty, bits) = constant_value(world, &key)?;
            OutputCondition::Constant(OutputConstant { ty, bits })
        }
        OutputCondition::StaticU32(value) => OutputCondition::Constant(OutputConstant {
            ty: Ty::scalar(Scalar::U32), bits: vec![Bits::from(*value)],
        }),
        OutputCondition::Member(base, field) => {
            let base = normalize(world, definition, base, span)?;
            if let OutputCondition::Constant(value) = base {
                let selected = if value.ty.has_variants() {
                    let Some(variant) = value.ty.variants.iter().find(|v| v.name == *field) else {
                        return definition.source.fail(span, "B225", "输出条件使用声明的变体名称");
                    };
                    OutputConstant { ty: Ty::scalar(Scalar::Bool),
                        bits: vec![Bits::from(value.bits[0] == Bits::from(variant.discriminant))] }
                } else if value.ty.array.is_some() && field == "length" {
                    OutputConstant { ty: Ty::scalar(Scalar::U32),
                        bits: vec![Bits::from(value.ty.values[0])] }
                } else {
                    let (ty, components) = member_type(&value.ty, field, definition.source, span)?;
                    OutputConstant { ty, bits: components.iter().map(|i| value.bits[*i]).collect() }
                };
                OutputCondition::Constant(selected)
            } else {
                OutputCondition::Member(Box::new(base), field.clone())
            }
        }
        OutputCondition::Unary(op, value) => OutputCondition::Unary(
            op.clone(), Box::new(normalize(world, definition, value, span)?)),
        OutputCondition::Binary(op, a, b) => OutputCondition::Binary(
            op.clone(), Box::new(normalize(world, definition, a, span)?),
            Box::new(normalize(world, definition, b, span)?)),
        value => value.clone(),
    })
}

pub(crate) fn bind_function_outputs(
    world: &World, parameters: &[(Access, TypeName)], outputs: &[ContractPredicate],
    source: &crate::Source, span: Span, module: &str, imports: &[String],
) -> Result<Vec<ContractPredicate>> {
    if outputs.is_empty() { return Ok(Vec::new()); }
    let parameters = parameters.iter().enumerate().map(|(index, (access, ty))|
        Param { name: format!("argument{index}"), ty: ty.clone(), access: *access, span }
    ).collect::<Vec<_>>();
    let contract = Contract::signature(source, module, imports, &parameters, outputs, span);
    let mut bound = BTreeSet::new();
    for predicate in outputs {
        let ContractPredicate::Initialized { value, condition } = predicate else {
            return source.fail(span, "B225", "cfn输出合同使用ensures initialized");
        };
        bound.insert(ContractPredicate::Initialized { value: value.clone(),
            condition: normalize(world, contract, condition, span)? });
    }
    Ok(bound.into_iter().collect())
}
fn condition_type(
    value: &OutputCondition, input: &impl Fn(&str) -> Option<(Access, Ty)>,
    returned: Option<&Ty>, source: &Source, span: Span,
) -> Result<(Ty, bool)> {
    fn path(value: &OutputCondition, input: &impl Fn(&str) -> Option<(Access, Ty)>,
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
                let ty = path(base, input, returned, source, span)?;
                if (ty.array.is_some() || ty.dataset.is_some()) && field == "length" {
                    return Ok(Ty::scalar(Scalar::U32));
                }
                if ty.has_variants() {
                    return if ty.variants.iter().any(|variant| variant.name == *field) {
                        Ok(Ty::scalar(Scalar::Bool))
                    } else { source.fail(span, "B225", "输出条件使用声明的返回变体名称") };
                }
                member_type(&ty, field, source, span).map(|(ty, _)| ty)
            }
            _ => source.fail(span, "B225", "输出条件的字段来自输入或returned"),
        }
    }
    Ok(match value {
        OutputCondition::Name(_) | OutputCondition::Member(..) =>
            (path(value, input, returned, source, span)?, false),
        OutputCondition::Number(text) => {
            let (ty, _) = number(source, span, text)?;
            let literal = ty.components[0].0.size() == 4;
            (ty, literal)
        }
        OutputCondition::StaticU32(_) => (Ty::scalar(Scalar::U32), false),
        OutputCondition::Constant(value) => (value.ty.clone(), false),
        OutputCondition::Bool(_) => (Ty::scalar(Scalar::Bool), false),
        OutputCondition::Unary(op, value) => {
            if op == "-" && let OutputCondition::Number(text) = value.as_ref() {
                let (ty, _) = operators::negative_number(source, span, text)?;
                let literal = ty.components[0].0.size() == 4;
                (ty, literal)
            } else {
                let (ty, literal) = condition_type(value, input, returned, source, span)?;
                operators::unary(op, &ty, source, span)?;
                (ty, literal)
            }
        }
        OutputCondition::Binary(op, a, b) => {
            let (a, a_literal) = condition_type(a, input, returned, source, span)?;
            let (b, b_literal) = condition_type(b, input, returned, source, span)?;
            if matches!(op.as_str(), "&&" | "||") {
                let boolean = Ty::scalar(Scalar::Bool);
                operators::same(&boolean, &a, source, span)?;
                operators::same(&boolean, &b, source, span)?;
                (boolean, false)
            } else {
                let (ty, kind) = operators::binary(op, &a, a_literal, &b, b_literal, source, span)?;
                let literal = matches!(kind, operators::Binary::Numeric { operands, comparison: false, .. }
                    if a_literal && b_literal && !operands.scalar.is_float());
                (ty, literal)
            }
        }
    })
}
fn validate_output(value: &str, condition: &OutputCondition,
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
    if let Some(element) = &ty.dataset { ty = element.as_ref().clone(); }
    for field in fields { ty = member_type(&ty, field, source, span)?.0; }
    let (condition, _) = condition_type(condition, input, returned, source, span)?;
    operators::same(&Ty::scalar(Scalar::Bool), &condition, source, span)
}

pub(crate) fn validate_function_outputs(
    parameters: &[CallParameter], result: Option<&Ty>, outputs: &[ContractPredicate],
    source: &crate::Source, span: Span,
) -> Result<()> {
    let input = |name: &str| parameters.iter().enumerate()
        .find(|(index, _)| name == format!("argument{index}"))
        .map(|(_, parameter)| match parameter {
            CallParameter::Value(ty) => (Access::Value, ty.clone()),
            CallParameter::Reference { ty, access } => (*access, ty.clone()),
        });
    for output in outputs {
        let ContractPredicate::Initialized { value, condition } = output else {
            return source.fail(span, "B225", "cfn输出合同使用ensures initialized");
        };
        validate_output(value, condition, &input, result, source, span)?;
    }
    Ok(())
}

fn resource_guard(ty: &Ty, offset: u32, guards: &[Presence]) -> Presence {
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
            if value.value.ty.has_variants() || value.value.ty.cleanup.is_some() {
                value.ownership = Ownership::Owned.restricted(&resource_guard(ty, offset, guards));
            }
            for (index, payload) in value.payloads.iter_mut().enumerate() {
                if let Some(payload) = payload {
                    let at = if value.value.ty.has_variants() {
                        ir::align_up(4, value.value.ty.align)
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
    fn forget_output_positions(&mut self,changed:&[Position],span:Span)->Result<()> {
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
                    Presence::input(&builder.binary("==", left, right, span)?)
                }
                (Position::Indexed(choices, index), other)
                | (other, Position::Indexed(choices, index)) => {
                    let mut all = Presence::None;
                    for (element, choice) in choices.iter().enumerate() {
                        all = all.or(Presence::Selected(*index, element)
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
                value => {
                    let mut when = Presence::None;
                    for position in changed {
                        when = when.or(overlap(builder, position, value, span)?);
                    }
                    Position::selected(when, Position::Initialized, value.clone())
                }
            })
        }
        let previous = std::mem::take(&mut self.initialized);
        for position in previous {
            let position = erase(self, &position, changed, span)?;
            self.initialized.insert(position);
        }
        Ok(())
    }
    pub(super) fn prepare_data_output(
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
                    self.assign_owned_place(&actual, Returned::Dataset(candidate));
                    found = true;
                }
            }
        } else {
            for binding in self.env.values_mut() {
                let paths = view::data_paths(binding)
                    .into_iter()
                    .filter(|(_, owner, _)| owner.handle.identity() == data.handle.identity())
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
    pub(super) fn check_data_output(
        &mut self,
        data: &application::Data,
        guards: &[Presence],
        span: Span,
    ) -> Result<()> {
        let path = self.path.clone();
        for ((_, offset), when) in data.ty.components.iter().zip(guards) {
            self.path = when.condition().assume(&path, true);
            if self.path.possible() && !self.application_field_initialized(data, *offset) {
                return self.fail(span, "B229", "返回分支没有满足dataset字段的条件初始化合同");
            }
        }
        self.path = path;
        Ok(())
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
        for (position, when) in self.positions(place).into_iter().zip(guards) {
            if self.owned_possible(when) {
                self.initialized.insert(Position::selected(
                    when.clone(),
                    position,
                    Position::Initialized,
                ));
            }
        }
    }

    pub(super) fn conditional_written_model(
        &mut self,
        name: &str,
        guards: &[Presence],
        span: Span,
    ) -> Result<Returned> {
        let binding = self.env[name].clone();
        let place = binding.stored_place().unwrap();
        let original_path = self.path.clone();
        let initialized = self.initialized.clone();
        for (component, when) in guards.iter().enumerate() {
            self.path = when.condition().assume(&original_path, true);
            if self.path.possible() {
                if let Binding::OwnedValue(value) = &binding {
                    let mut value = Returned::OwnedValue(value.clone());
                    self.resolve_owned_model(&mut value, None);
                    self.prepare_owned_read(&value);
                }
                let selected = place
                    .clone()
                    .project(Ty::scalar(place.ty().components[component].0), &[component]);
                if !self.is_initialized(&selected) {
                    return self.fail(
                        span,
                        "B229",
                        format!("返回分支没有满足 {name} 的条件输出初始化合同"),
                    );
                }
            }
            self.initialized = initialized.clone();
        }
        self.path = original_path.clone();
        let mut model = match binding {
            Binding::Owned(owner) => Returned::value(Value {
                ty: place.ty().clone(),
                regs: place
                    .ty()
                    .components
                    .iter()
                    .map(|(scalar, _)| self.reg(*scalar))
                    .collect(),
                proof: owner.proof,
                uniform: owner.uniform,
                literal: false,
                constant: None,
            }),
            Binding::OwnedValue(value) => Returned::OwnedValue(value),
            _ => unreachable!(),
        };
        fn resources(
            builder: &mut Builder<'_>,
            model: &Returned,
            root: &Ty,
            offset: u32,
            guards: &[Presence],
            path: &bounds::Clause,
            span: Span,
        ) -> Result<()> {
            match model {
                Returned::Dataset(data) => {
                    let when = resource_guard(root, offset, guards);
                    builder.path = when.condition().assume(path, true);
                    if builder.path.possible() {
                        let mut selected = model.clone();
                        builder.resolve_owned_model(&mut selected, None);
                        let Returned::Dataset(data) = selected else {
                            unreachable!()
                        };
                        if !data.ownership.available() {
                            return builder.fail(
                                span,
                                "B229",
                                "条件输出分支没有实际拥有所声明的资源",
                            );
                        }
                        builder.require_application_initialized(&data, span)?;
                    }
                    builder.path = when.condition().assume(path, false);
                    if builder.path.possible()
                        && builder.resolve_ownership(&data.ownership, None).may_own()
                    {
                        return builder.fail(
                            span,
                            "B229",
                            "未交付分支仍拥有输出资源；按实际返回关系交付，或由原所有者释放",
                        );
                    }
                }
                Returned::OwnedValue(value) => {
                    if value.value.ty.cleanup.is_some() {
                        let when=resource_guard(root,offset,guards);
                        builder.path=when.condition().assume(path,true);
                        if builder.path.possible()&&!builder.resolve_ownership(&value.ownership,None).available(){return builder.fail(span,"B229","条件输出分支没有实际构造用户拥有值");}
                        builder.path=when.condition().assume(path,false);
                        if builder.path.possible()&&builder.resolve_ownership(&value.ownership,None).may_own(){return builder.fail(span,"B229","未交付分支仍拥有用户资源；按实际返回关系交付或由原所有者释放");}
                    }
                    for (index, payload) in value.payloads.iter().enumerate() {
                        if let Some(payload) = payload {
                            let at = if value.value.ty.has_variants() {
                                ir::align_up(4, value.value.ty.align)
                            } else {
                                value.value.ty.aggregate_part(index).0
                            };
                            resources(builder, payload, root, offset + at, guards, path, span)?;
                        }
                    }
                }
                _ => {}
            }
            Ok(())
        }
        resources(self, &model, place.ty(), 0, guards, &original_path, span)?;
        self.path = original_path;
        fn absent(model: &mut Returned) {
            match model {
                Returned::Dataset(data) if data.ownership == Ownership::Moved => {
                    data.residency = residency::Residency::Absent
                }
                Returned::OwnedValue(value) => {
                    for payload in value.payloads.iter_mut().flatten() {
                        absent(payload);
                    }
                }
                _ => {}
            }
        }
        absent(&mut model);
        // These registers only name the public output storage. No load from an
        // uninitialized branch is emitted while constructing its contract.
        let registers = place
            .ty()
            .components
            .iter()
            .map(|(scalar, _)| self.reg(*scalar))
            .collect();
        native_function::rebind_model(&mut model, registers);
        Ok(model)
    }
}
