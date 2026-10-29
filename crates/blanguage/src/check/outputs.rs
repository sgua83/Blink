//! Declared output initialization is part of the original function contract.
//! Its predicates use input snapshots and the real returned value, never a
//! runtime initialization flag or a second resource owner.
use super::*;
use application::Ownership;
use owned_presence::Presence;
use returns::Returned;

pub(super) fn validate(
    builder: &Builder<'_>,
    definition: &Definition,
    contract: &FunctionContract,
) -> Result<()> {
    let ContractPredicate::Initialized { value, .. } = &contract.predicate else {
        return Ok(());
    };
    if contract.kind != ContractKind::Ensures
        || definition.function.kind != FunctionKind::Value
        || definition.function.target != Target::Cpu
    {
        return definition.source.fail(
            contract.span,
            "B225",
            "initialized 是普通CPU函数的 ensures 输出合同",
        );
    }
    let mut fields = value.split('.');
    let root = fields.next().unwrap();
    let Some(parameter) = definition
        .function
        .params
        .iter()
        .find(|p| p.name == root && matches!(p.access, Access::Write | Access::InOut))
    else {
        return definition.source.fail(
            contract.span,
            "B225",
            "initialized 需要实际 write／inout 参数或字段",
        );
    };
    let mut ty = cooperative::parameter_type(builder.world, parameter, definition)?;
    for field in fields {
        ty = builder.member(&ty, field, contract.span)?.0;
    }
    Ok(())
}

pub(super) fn declared(definition: &Definition, parameter: usize) -> bool {
    let name = &definition.function.params[parameter].name;
    definition.function.contracts.iter().any(|contract| {
        matches!(&contract.predicate,
        ContractPredicate::Initialized { value, .. } if value.split('.').next() == Some(name))
    })
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
    definition: &Definition,
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
        definition: &Definition,
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
                if name == "returned" || definition.function.params.iter().any(|p| p.name == *name)
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
        definition: &Definition,
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
                    let expected = builder.constant(scalar, discriminant);
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

pub(super) fn guards(
    builder: &mut Builder<'_>,
    definition: &Definition,
    parameter: usize,
    ty: &Ty,
    inputs: &BTreeMap<String, Value>,
    returned: Option<&Value>,
) -> Result<Vec<Presence>> {
    let name = &definition.function.params[parameter].name;
    let mut guards = vec![None::<Presence>; ty.components.len()];
    for contract in &definition.function.contracts {
        let ContractPredicate::Initialized { value, condition } = &contract.predicate else {
            continue;
        };
        let mut fields = value.split('.');
        if fields.next() != Some(name) {
            continue;
        }
        let mut selected = ty.clone();
        let mut components = (0..ty.components.len()).collect::<Vec<_>>();
        for field in fields {
            let (next, projection) = builder.member(&selected, field, contract.span)?;
            components = projection.iter().map(|i| components[*i]).collect();
            selected = next;
        }
        let when = Presence::input(&expression(
            builder,
            definition,
            condition,
            inputs,
            returned,
            contract.span,
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
    fn normalize(
        builder: &mut Builder<'_>,
        definition: &Definition,
        value: &OutputCondition,
        span: Span,
    ) -> Result<OutputCondition> {
        Ok(match value {
            OutputCondition::Name(name)
                if name != "returned"
                    && !definition.function.params.iter().any(|p| p.name == *name) =>
            {
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
                        message: format!("输出条件没有编译期常量 {name}"),
                    })?;
                if definition.public && !builder.world.functions[&key].public {
                    return definition.source.fail(
                        span,
                        "B225",
                        "公开输出条件的模块常量必须在接口中可见",
                    );
                }
                let (ty, bits) = constant_value(builder.world, &key)?;
                OutputCondition::Constant(OutputConstant { ty, bits })
            }
            OutputCondition::StaticU32(value) => OutputCondition::Constant(OutputConstant {
                ty: Ty::scalar(Scalar::U32),
                bits: vec![*value],
            }),
            OutputCondition::Member(base, field) => {
                let base = normalize(builder, definition, base, span)?;
                if let OutputCondition::Constant(value) = base {
                    let selected = if value.ty.has_variants() {
                        let Some(variant) = value.ty.variants.iter().find(|v| v.name == *field)
                        else {
                            return definition.source.fail(
                                span,
                                "B225",
                                "输出条件使用声明的变体名称",
                            );
                        };
                        builder.constant(
                            Scalar::Bool,
                            u32::from(value.bits[0] == variant.discriminant),
                        )
                    } else {
                        let value = builder.constant_value(value.ty, value.bits);
                        builder.member_value(value, field, span)?
                    };
                    OutputCondition::Constant(OutputConstant {
                        ty: selected.ty,
                        bits: selected.constant.unwrap(),
                    })
                } else {
                    OutputCondition::Member(Box::new(base), field.clone())
                }
            }
            OutputCondition::Unary(op, value) => OutputCondition::Unary(
                op.clone(),
                Box::new(normalize(builder, definition, value, span)?),
            ),
            OutputCondition::Binary(op, a, b) => OutputCondition::Binary(
                op.clone(),
                Box::new(normalize(builder, definition, a, span)?),
                Box::new(normalize(builder, definition, b, span)?),
            ),
            value => value.clone(),
        })
    }
    let mut predicates = BTreeSet::new();
    for contract in &definition.function.contracts {
        let mut predicate = contract.predicate.clone();
        if let ContractPredicate::Initialized { condition, .. } = &mut predicate {
            *condition = normalize(builder, definition, condition, contract.span)?;
        }
        predicates.insert((contract.kind, predicate));
    }
    Ok(predicates)
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
        fn index(builder: &mut Builder<'_>, index: ownership::ApplicationIndex) -> Value {
            match index {
                ownership::ApplicationIndex::Constant(value) => {
                    builder.constant(Scalar::U32, value)
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
        let changed = self.positions(place);
        let previous = std::mem::take(&mut self.initialized);
        for position in previous {
            let position = erase(self, &position, &changed, span)?;
            self.initialized.insert(position);
        }
        Ok(())
    }
    pub(super) fn reset_data_output(
        &mut self,
        data: &mut application::Data,
        span: Span,
    ) -> Result<()> {
        let scope = Position::Parameter(self.application_output_scope(), 0);
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
                    candidate.initialization =
                        Some(Position::selected(selected, scope.clone(), old));
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
                    owner.initialization = Some(scope.clone());
                    found = true;
                }
            }
        }
        if !found {
            return self.fail(span, "B229", "条件dataset输出没有可交接的原初始化所有者");
        }
        data.initialization = Some(scope);
        Ok(())
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
            if self.path.possible() && !self.application_field_initialized(data, *offset / 4) {
                return self.fail(span, "B229", "返回分支没有满足dataset字段的条件初始化合同");
            }
        }
        self.path = path;
        Ok(())
    }
    pub(super) fn initialize_data_output(&mut self, data: &application::Data, guards: &[Presence]) {
        if let Some(scope) = &data.initialization {
            for ((_, offset), when) in data.ty.components.iter().zip(guards) {
                self.initialized.insert(Position::selected(
                    when.clone(),
                    scope.application_word(None, *offset / 4),
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
