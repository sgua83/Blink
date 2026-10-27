//! 普通 CPU 函数拥有独立调用帧；编译中的符号声明截断递归展开。
use super::*;
use cooperative::Argument;
use returns::Returned;

// Only source vectors admit a different component placement with the same type.
// Aggregate fields and arrays retain their declared layout.
pub(super) fn reference_parameters(ty: &Ty) -> Vec<Ty> {
    if ty.array.is_none()
        && ty.fields.is_empty()
        && ty.view.is_none()
        && ty.dataset.is_none()
        && !ty.result
        && !ty.enumeration
        && ty.components.len() > 1
    {
        ty.components
            .iter()
            .map(|(scalar, _)| Ty::scalar(*scalar))
            .collect()
    } else {
        vec![ty.clone()]
    }
}
#[derive(Default)]
pub(super) struct Module {
    units: BTreeMap<String, Option<Unit>>,
}
#[derive(Clone)]
pub(crate) struct Contract {
    parameters: Vec<CallParameter>,
    result: Option<Ty>,
    inputs: Vec<Reg>,
    returned: Option<Returned>,
    written: Vec<Option<bounds::Proof>>,
    requires: BTreeSet<(ContractKind, ContractPredicate)>,
    target: Target,
    frame: bool,
    domain: Option<crate::module::domain::Interface>,
    residency: residency::Requirements,
}
crate::module::codec::record!(Contract {
    parameters,
    result,
    inputs,
    returned,
    written,
    requires,
    target,
    frame,
    domain,
    residency
});
struct Unit {
    function: Option<FunctionBody>,
    contract: Contract,
    module: String,
    public: bool,
    residency_calls: Vec<residency::Call>,
    residency_uses: Vec<residency::Use>,
}
fn declaration(
    world: &World,
    instance: &crate::generics::Instance,
    mut context: application::Context,
) -> Result<application::Context> {
    if context.functions.units.contains_key(&instance.name) {
        return Ok(context);
    }
    if let Some(definitions) = &instance.definition.compiled {
        use crate::module::codec::Decode;
        let bytes = definitions.get(&instance.name).ok_or_else(|| Diagnostic {
            source: instance.definition.source.clone(),
            span: instance.definition.function.span,
            code: "B270",
            message: format!("已编译模块没有原生实例 {}", instance.name),
        })?;
        let contract = Contract::from_bytes(bytes).map_err(|error| Diagnostic {
            source: instance.definition.source.clone(),
            span: instance.definition.function.span,
            code: "B270",
            message: format!("模块函数接口错误：{error}"),
        })?;
        context.functions.units.insert(
            instance.name.clone(),
            Some(Unit {
                function: None,
                contract,
                module: instance.definition.module.clone(),
                public: instance.definition.public,
                residency_calls: Vec::new(),
                residency_uses: Vec::new(),
            }),
        );
        return Ok(context);
    }
    context.functions.units.insert(instance.name.clone(), None);
    let frame = context.frame.take();
    let mut instance = instance.clone();
    if instance.definition.function.target == Target::Compute {
        Arc::make_mut(&mut instance.definition).function.target = Target::Cpu;
    }
    let checked = checked_function(world, &instance, Some(context))?;
    let residency::Inputs {
        required,
        calls,
        uses,
    } = checked.residency;
    let mut context = checked.application.unwrap();
    context.frame = frame;
    let mut function = checked.body;
    let contract = Contract {
        parameters: function.parameters.clone(),
        result: function.result.clone(),
        inputs: checked.inputs,
        returned: checked.returned,
        written: checked.written,
        requires: crate::contract_signature(&instance.definition.function)
            .into_iter()
            .map(|(kind, predicate)| (kind, predicate.clone()))
            .collect(),
        target: instance.definition.function.target,
        frame: false,
        domain: None,
        residency: required,
    };
    crate::optimize::value_code(&mut function.body);
    context.functions.units.insert(
        instance.name.clone(),
        Some(Unit {
            function: Some(function),
            contract,
            module: instance.definition.module.clone(),
            public: instance.definition.public,
            residency_calls: calls,
            residency_uses: uses,
        }),
    );
    Ok(context)
}
// 合同中的值编号属于接口，不进入实际IR寄存器表或产生转换指令。
fn rebind_model(value: &mut Returned, registers: Vec<Reg>) {
    match value {
        Returned::Unit => {}
        Returned::Value(value) => value.regs = registers,
        Returned::Dataset(data) => {
            data.handle.regs = vec![registers[0]];
            data.length.regs = vec![registers[1]];
            data.slots = None;
            data.initialization = None;
        }
        Returned::OwnedValue(value) => {
            value.value.regs = registers;
            value.slots = None;
            for (index, payload) in value.payloads.iter_mut().enumerate() {
                if let Some(payload) = payload {
                    let (offset, ty) = if value.value.ty.result {
                        (
                            ir::align_up(4, value.value.ty.align),
                            &value.value.ty.arguments[index],
                        )
                    } else {
                        value.value.ty.aggregate_part(index)
                    };
                    let registers = ty
                        .components
                        .iter()
                        .map(|(_, at)| {
                            let component = value
                                .value
                                .ty
                                .components
                                .iter()
                                .position(|(_, word)| *word == offset + at)
                                .unwrap();
                            value.value.regs[component]
                        })
                        .collect();
                    rebind_model(payload, registers);
                }
            }
        }
    }
}
pub(super) fn canonical(
    builder: &mut Builder<'_>,
    returned: Option<Returned>,
    inputs: &[Reg],
    written: Vec<Option<bounds::Proof>>,
) -> (Option<Returned>, Vec<Reg>, Vec<Option<bounds::Proof>>) {
    let mut bindings = inputs
        .iter()
        .enumerate()
        .map(|(index, reg)| {
            (
                ValueIdentity::Register(*reg),
                bounds::Proof::default().with_identity(ValueIdentity::Register(index)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (parameter, p) in builder.definition.function.params.iter().enumerate() {
        if p.access == Access::InOut
            && let Some(Binding::Owned(owner)) = builder.env.get(&p.name)
        {
            for component in 0..owner.place.ty().components.len() {
                let identity = ValueIdentity::ParameterContent(parameter, component);
                bindings.insert(identity, bounds::Proof::default().with_identity(identity));
            }
        }
    }
    let count = returned.as_ref().map_or(0, |model| model.registers().len());
    let models = returned.map(|model| {
        let mut value = model.clone();
        rebind_model(&mut value, (inputs.len()..inputs.len() + count).collect());
        for (source, target) in model.registers().into_iter().zip(value.registers()) {
            bindings
                .entry(ValueIdentity::Register(source))
                .or_insert_with(|| {
                    bounds::Proof::default().with_identity(ValueIdentity::Register(target))
                });
        }
        (value, model)
    });
    let mut next = inputs.len() + count;
    for proof in written.iter().flatten() {
        for component in 0..proof.width() {
            if let Some(identity) = proof.known_identity(component) {
                bindings.entry(identity).or_insert_with(|| {
                    bounds::Proof::default().with_identity(ValueIdentity::Register(next))
                });
            }
            next += 1;
        }
    }
    let returned = models.map(|(mut value, model)| {
        translate_returned(&mut value, &model, &bindings);
        value
    });
    let written = written
        .into_iter()
        .map(|proof| proof.map(|proof| proof.at_call(&bindings)))
        .collect();
    (returned, (0..inputs.len()).collect(), written)
}
pub(super) fn check(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let mut context = declaration(world, instance, application::Context::default())?;
    resolve_residencies(&mut context.functions)
}
impl Builder<'_> {
    fn native_declaration(&mut self, instance: &crate::generics::Instance) -> Result<()> {
        let context = self.application.take().unwrap();
        self.application = Some(declaration(self.world, instance, context)?);
        Ok(())
    }
    pub(super) fn native_function(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<Returned>> {
        let d = &instance.definition;
        if d.function.target == Target::Gpu {
            return self.fail(span, "B161", "普通 CPU 调用不能改变函数声明的实际目标");
        }
        let frame = self.application_frame(span)?;
        let mut arguments = vec![CallArgument::Value {
            ty: frame.ty.clone(),
            values: frame.regs.clone(),
        }];
        let mut actual = frame
            .regs
            .iter()
            .map(|reg| Some(bounds::Proof::default().with_identity(ValueIdentity::Register(*reg))))
            .collect::<Vec<_>>();
        let mut env = BTreeMap::new();
        let mut locations = residency::Arguments::new();
        let mut written = Vec::new();
        let mut written_data = Vec::new();
        let mut held = Vec::new();
        let mut contents = BTreeMap::new();
        for (index, (p, arg)) in d.function.params.iter().zip(args).enumerate() {
            let ty = cooperative::parameter_type(self.world, p, d)?;
            let arg = match arg {
                Argument::BorrowedData(reference) => {
                    if p.access.writes() {
                        for source in &reference.sources {
                            self.invalidate_application_contents(*source);
                        }
                    }
                    Argument::ApplicationData(reference.data)
                }
                Argument::ScopedData { data, owner } => {
                    held.push(owner);
                    Argument::ApplicationData(data)
                }
                other => other,
            };
            match arg {
                Argument::Value(mut value) if p.access == Access::Value && !p.ty.dataset => {
                    self.require_same(&ty, &value.ty, span)?;
                    value.literal = false;
                    append_actual(&mut actual, &value);
                    arguments.push(CallArgument::Value {
                        ty: value.ty.clone(),
                        values: value.regs.clone(),
                    });
                    env.insert(p.name.clone(), Binding::Value(value));
                }
                Argument::Exclusive(place) if matches!(p.access, Access::Write | Access::InOut) => {
                    self.require_same(&ty, place.ty(), span)?;
                    for (part, address) in self.native_references(&place, p.access, span)? {
                        arguments.push(CallArgument::Reference {
                            ty: part,
                            access: p.access,
                            address,
                        });
                        // Actual pointer inputs are private to the callee, never caller scalar facts.
                        actual.extend([None, None]);
                    }
                    if p.access == Access::InOut {
                        // Name unread entry contents without loading them. Unused virtual registers
                        // are removed by value_code before native register allocation.
                        let contents = ty
                            .components
                            .iter()
                            .map(|(scalar, _)| self.reg(*scalar))
                            .collect::<Vec<_>>();
                        self.read_owned_proof(&place, &contents);
                    }
                    let owner = self.borrow_owned(place.clone());
                    if p.access == Access::InOut {
                        let width = ty.components.len();
                        for component in 0..width {
                            contents.insert(
                                ValueIdentity::ParameterContent(index, component),
                                owner.proof.project(&[component], width),
                            );
                        }
                    }
                    env.insert(p.name.clone(), Binding::Owned(owner));
                    written.push((index, place));
                }
                Argument::ApplicationData(data) if p.ty.dataset => {
                    residency::argument(&mut locations, index, &data);
                    self.require_same(&ty, &data.ty, span)?;
                    if !matches!(
                        p.access,
                        Access::Value | Access::Read | Access::Write | Access::InOut
                    ) {
                        return self.fail(
                            span,
                            "B224",
                            "CPU dataset 参数需要拥有交接或 read／write／inout 借用",
                        );
                    }
                    if p.access.writes() && !data.writable {
                        return self.fail(span, "B227", "只读 dataset 不能提升为可写借用");
                    }
                    if p.access != Access::Write {
                        self.require_application_initialized(&data, span)?;
                    }
                    if p.ty.count.is_some()
                        && self.world.shape(
                            &p.ty.count,
                            &d.source,
                            p.span,
                            &d.module,
                            &d.imports,
                        )? != data.count
                    {
                        return self.fail(span, "B167", "调用方没有声明函数需要的数据形状");
                    }
                    if p.access == Access::Write {
                        written_data.push(data.clone());
                    }
                    if matches!(p.access, Access::Write | Access::InOut) {
                        self.invalidate_application_contents(data.handle.identity());
                    }
                    let value = self.returned_storage(&Returned::Dataset(data.clone()));
                    append_actual(&mut actual, &value);
                    arguments.push(CallArgument::Value {
                        ty: value.ty,
                        values: value.regs,
                    });
                    env.insert(p.name.clone(), Binding::ApplicationData(data));
                }
                Argument::OwnedValue(mut value)
                    if p.access == Access::Value && ty.owns_resources() =>
                {
                    self.require_same(&ty, &value.value.ty, span)?;
                    residency::owned_argument(&mut locations, index, &mut value);
                    append_actual(&mut actual, &value.value);
                    arguments.push(CallArgument::Value {
                        ty: value.value.ty.clone(),
                        values: value.value.regs.clone(),
                    });
                    env.insert(p.name.clone(), Binding::OwnedValue(value));
                }
                _ => {
                    return self.fail(
                        span,
                        "B207",
                        "普通 CPU 调用需要匹配的实际值、拥有资源或独占位置",
                    );
                }
            }
        }
        let requirements = contracts::resolve(self, d, &env)?;
        contracts::prove(self, &requirements, span)?;
        self.native_declaration(&instance)?;
        if !locations.is_empty() {
            let call = residency::Call {
                function: instance.name.clone(),
                arguments: locations.clone(),
                source: self.definition.source.clone(),
                span,
            };
            if let Some(unit) =
                self.application.as_ref().unwrap().functions.units[&instance.name].as_ref()
            {
                self.residency.apply(&unit.contract.residency, &call)?;
            }
            self.residency.calls.push(call);
        }
        let result = if d.function.result.name == "void" || d.function.result.is_never() {
            None
        } else {
            let previous = std::mem::replace(&mut self.definition, d.clone());
            let ty = self.expected_storage(&d.function.result, span);
            self.definition = previous;
            Some(ty?)
        };
        let values = result
            .iter()
            .flat_map(|ty| &ty.components)
            .map(|(scalar, _)| self.reg(*scalar))
            .collect::<Vec<_>>();
        self.code.push(Inst::Call(Call {
            target: CallTarget::Defined(instance.name.clone()),
            arguments,
            result: result.clone(),
            values: values.clone(),
        }));
        if d.function.result.is_never() {
            self.flow = Flow::STOP;
            return Ok(None);
        }
        let prototype = self.application.as_ref().unwrap().functions.units[&instance.name]
            .as_ref()
            .map(|unit| {
                (
                    unit.contract.inputs.clone(),
                    unit.contract.returned.clone(),
                    unit.contract.written.clone(),
                )
            });
        let (mut bindings, model, writes) = if let Some((inputs, model, writes)) = prototype {
            (
                inputs
                    .into_iter()
                    .zip(actual)
                    .filter_map(|(reg, value)| {
                        value.map(|value| (ValueIdentity::Register(reg), value))
                    })
                    .collect::<BTreeMap<_, _>>(),
                model,
                writes,
            )
        } else {
            (BTreeMap::new(), None, Vec::new())
        };
        bindings.extend(contents);
        if let Some(model) = &model {
            for (source, target) in model.registers().into_iter().zip(&values) {
                bindings.insert(
                    ValueIdentity::Register(source),
                    bounds::Proof::default().with_identity(ValueIdentity::Register(*target)),
                );
            }
        }
        for (index, place) in &written {
            if let Some(Some(proof)) = writes.get(*index) {
                for (component, (scalar, _)) in place.ty().components.iter().enumerate() {
                    if let Some(identity) = proof.known_identity(component) {
                        bindings.entry(identity).or_insert_with(|| {
                            let reg = self.reg(*scalar);
                            bounds::Proof::default().with_identity(ValueIdentity::Register(reg))
                        });
                    }
                }
            }
        }
        for (index, place) in written {
            let proof = writes
                .get(index)
                .and_then(Option::as_ref)
                .map(|proof| proof.at_call(&bindings))
                .unwrap_or_default();
            self.assign_owned_proof(&place, proof, false);
            self.initialize(&place);
        }
        for data in written_data {
            self.initialize_application(&data);
        }
        for name in held {
            self.finish_scoped_argument(&name, span)?;
        }
        let Some(ty) = result else {
            return Ok(Some(Returned::Unit));
        };
        let value = Value {
            ty,
            regs: values,
            proof: bounds::Proof::default(),
            literal: false,
            uniform: false,
            constant: None,
        };
        let mut returned = self.template_returned(value, true);
        if let Some(model) = model {
            translate_returned(&mut returned, &model, &bindings);
            residency::returned(&mut returned, &locations);
        } else {
            residency::output(&mut returned, &instance.name, &locations);
        }
        Ok(Some(returned))
    }
}
fn append_actual(actual: &mut Vec<Option<bounds::Proof>>, value: &Value) {
    let proof = bounds::Proof::capture(value);
    for i in 0..value.regs.len() {
        actual.push(Some(proof.project(&[i], value.regs.len())));
    }
}
fn translate_value(
    value: &mut Value,
    model: &Value,
    bindings: &BTreeMap<ValueIdentity, bounds::Proof>,
) {
    value.proof = bounds::Proof::capture(model).at_call(bindings);
    value.constant = model
        .constant
        .clone()
        .or_else(|| value.proof.known_value(&value.ty));
    value.uniform = false;
}
fn translate_returned(
    value: &mut Returned,
    model: &Returned,
    bindings: &BTreeMap<ValueIdentity, bounds::Proof>,
) {
    match (value, model) {
        (Returned::Value(value), Returned::Value(model)) => translate_value(value, model, bindings),
        (Returned::Dataset(value), Returned::Dataset(model)) => {
            translate_value(&mut value.handle, &model.handle, bindings);
            translate_value(&mut value.length, &model.length, bindings);
            value.handle.proof = bounds::Proof::capture(&value.handle);
            value.length.proof =
                bounds::Proof::capture(&value.length).with_domains(Domains::from([
                    IndexDomain::ApplicationLength(value.handle.identity()),
                ]));
            value.residency = model.residency.clone();
            value.count = model.count;
        }
        (Returned::OwnedValue(value), Returned::OwnedValue(model)) => {
            translate_value(&mut value.value, &model.value, bindings);
            for (value, model) in value.payloads.iter_mut().zip(&model.payloads) {
                match (value.as_mut(), model.as_ref()) {
                    (Some(value), Some(model)) => translate_returned(value, model, bindings),
                    _ => {
                        *value = None;
                    }
                }
            }
        }
        _ => {}
    }
}
// 函数合同沿真实资源调用关系收敛，递归参数置换也消费同一输入条件。
fn resolve_residencies(module: &mut Module) -> Result<()> {
    let outputs = residency::solve_outputs(module.units.iter().map(|(name, unit)| {
        (
            name.as_str(),
            unit.as_ref().unwrap().contract.returned.as_ref(),
        )
    }));
    loop {
        let known = module
            .units
            .iter()
            .map(|(name, unit)| {
                (
                    name.clone(),
                    unit.as_ref().unwrap().contract.residency.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut changed = false;
        for unit in module.units.values_mut() {
            let unit = unit.as_mut().unwrap();
            let mut inputs = residency::Inputs {
                required: unit.contract.residency.clone(),
                calls: Vec::new(),
                uses: Vec::new(),
            };
            for call in &unit.residency_calls {
                inputs.apply(
                    &known[&call.function],
                    &residency::resolved_call(call, &outputs),
                )?;
            }
            inputs.apply_uses(&unit.residency_uses, &outputs, false)?;
            changed |= inputs.required != unit.contract.residency;
            unit.contract.residency = inputs.required;
        }
        if !changed {
            break;
        }
    }
    for unit in module.units.values_mut() {
        let unit = unit.as_mut().unwrap();
        let mut inputs = residency::Inputs {
            required: unit.contract.residency.clone(),
            calls: Vec::new(),
            uses: Vec::new(),
        };
        inputs.apply_uses(&unit.residency_uses, &outputs, true)?;
        if let Some(returned) = &mut unit.contract.returned {
            if !residency::resolve_returned(returned, &outputs) {
                unit.contract.returned = None;
            }
        }
    }
    Ok(())
}
fn calls(code: &mut [Inst], visit: &mut impl FnMut(&mut Call)) {
    for inst in code {
        match inst {
            Inst::Call(call) => visit(call),
            Inst::Scope(body) => calls(body, visit),
            Inst::If { yes, no, .. } => {
                calls(yes, visit);
                calls(no, visit);
            }
            Inst::Loop {
                condition, body, ..
            } => {
                calls(condition, visit);
                calls(body, visit);
            }
            _ => {}
        }
    }
}
fn dependencies(body: &mut ValueCode) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    calls(&mut body.code, &mut |call| {
        if let CallTarget::Defined(name) = &call.target {
            names.insert(name.clone());
        }
    });
    names
}
pub(super) struct Finalized {
    pub functions: Vec<FunctionBody>,
    pub imports: BTreeMap<String, Vec<u8>>,
    pub exports: BTreeMap<String, Vec<u8>>,
}
fn finalize(
    mut module: Module,
    mut reachable: BTreeSet<String>,
    mut root: Option<&mut ValueCode>,
    owner: Option<&str>,
) -> Result<Finalized> {
    use crate::module::codec::Encode;
    resolve_residencies(&mut module)?;
    let mut units = module
        .units
        .into_iter()
        .map(|(name, unit)| (name, unit.unwrap()))
        .collect::<BTreeMap<_, _>>();
    loop {
        let mut next = reachable.clone();
        for name in &reachable {
            if let Some(function) = &mut units.get_mut(name).unwrap().function {
                next.extend(dependencies(&mut function.body));
            }
        }
        if next == reachable {
            break;
        }
        reachable = next;
    }
    units.retain(|name, _| reachable.contains(name));
    let mut frame = BTreeSet::new();
    for (name, unit) in &mut units {
        if unit.contract.frame {
            frame.insert(name.clone());
        }
        if let Some(function) = &mut unit.function {
            calls(&mut function.body.code, &mut |call| {
                if matches!(&call.target,CallTarget::External(name) if name.library==crate::application::RUNTIME_LIBRARY)
                {
                    frame.insert(name.clone());
                }
            });
        }
    }
    loop {
        let mut next = frame.clone();
        for (name, unit) in &mut units {
            if let Some(function) = &mut unit.function {
                if dependencies(&mut function.body)
                    .iter()
                    .any(|callee| frame.contains(callee))
                {
                    next.insert(name.clone());
                }
            }
        }
        if next == frame {
            break;
        }
        frame = next;
    }
    fn prune(body: &mut ValueCode, frame: &BTreeSet<String>) {
        calls(&mut body.code, &mut |call| {
            if matches!(&call.target,CallTarget::Defined(name) if !frame.contains(name)) {
                call.arguments.remove(0);
            }
        });
    }
    if let Some(body) = &mut root {
        prune(body, &frame);
    }
    let mut result = Finalized {
        functions: Vec::new(),
        imports: BTreeMap::new(),
        exports: BTreeMap::new(),
    };
    for (name, mut unit) in units {
        unit.contract.frame = frame.contains(&name);
        let local = unit.function.is_some() && owner.is_none_or(|owner| unit.module == owner);
        if local {
            let mut function = unit.function.take().unwrap();
            prune(&mut function.body, &frame);
            if !frame.contains(&name) {
                function.parameters.remove(0);
                function.body.inputs.drain(..2);
            }
            crate::optimize::value_code(&mut function.body);
            result.functions.push(function);
            if owner.is_some() && unit.public {
                result.exports.insert(name, unit.contract.bytes());
            }
        } else {
            result.imports.insert(name, unit.contract.bytes());
        }
    }
    Ok(result)
}
pub(super) fn finish(module: Module, root: &mut ValueCode) -> Result<Finalized> {
    let roots = dependencies(root);
    finalize(module, roots, Some(root), None)
}

pub(crate) fn compile_module(
    world: &World,
    entry: &str,
    requests: &[String],
) -> Result<crate::module::Module> {
    let mut context = application::Context::default();
    let mut roots = BTreeSet::new();
    for (key, d) in &world.functions {
        crate::generics::validate_declaration(d)?;
        if d.module != entry || d.compiled.is_some() {
            continue;
        }
        for instance in crate::generics::instances(world, key, d)? {
            if d.execution == crate::execution::Requirement::Domain
                && super::kernel_interface(world, &instance.definition)?
            {
                roots.extend(domain_declaration(world, &instance, &mut context)?);
            } else if d.function.external.is_none()
                && d.function.kind == FunctionKind::Value
                && d.function.target != Target::Gpu
            {
                if d.public {
                    roots.insert(instance.name.clone());
                }
                context = declaration(world, &instance, context)?;
            } else if d.execution == crate::execution::Requirement::Domain {
                compile_program(world, &instance)?;
            } else {
                check_function(world, &instance)?;
            }
        }
    }
    for request in requests {
        let source = Source {
            path: "<module-export>".into(),
            text: request.clone().into(),
        };
        let (name, arguments) = crate::parser::entry(source.clone())?;
        let key = world
            .resolve(&name, entry, &[], true)
            .filter(|key| world.functions[key].module == entry && world.functions[key].public)
            .ok_or_else(|| Diagnostic {
                source: source.clone(),
                span: Span::default(),
                code: "B270",
                message: format!("没有本模块公开导出 {name}"),
            })?;
        let d = &world.functions[&key];
        let instance = super::generics::specialize(
            world,
            &key,
            &[],
            arguments.as_deref(),
            d,
            d.function.span,
        )?;
        if d.execution == crate::execution::Requirement::Domain
            && super::kernel_interface(world, &instance.definition)?
        {
            roots.extend(domain_declaration(world, &instance, &mut context)?);
            continue;
        }
        if d.function.external.is_some()
            || d.function.kind != FunctionKind::Value
            || d.function.target == Target::Gpu
        {
            return d.source.fail(
                d.function.span,
                "B270",
                "此普通原生模块入口需要CPU适用的普通函数",
            );
        }
        roots.insert(instance.name.clone());
        context = declaration(world, &instance, context)?;
    }
    let finalized = finalize(context.functions, roots, None, Some(entry))?;
    let mut functions = finalized.functions;
    let resources = application::module_resources(&mut functions, context.resources);
    Ok(crate::module::Module {
        name: entry.into(),
        header: world.interfaces[entry].text.to_string(),
        requirements: world
            .functions
            .iter()
            .filter(|(_, d)| d.module == entry && d.public)
            .map(|(name, d)| (name.clone(), d.execution))
            .collect(),
        functions,
        imports: finalized.imports,
        exports: finalized.exports,
        resources,
    })
}

fn domain_at(
    world: &World,
    instance: &crate::generics::Instance,
    context: &mut application::Context,
    target: Target,
) -> Result<crate::module::domain::Interface> {
    let name = crate::module::domain::symbol(&instance.name, target);
    if !context.functions.units.contains_key(&name) {
        if instance.definition.compiled.is_some() {
            let mut reference = instance.clone();
            reference.name = name.clone();
            *context = declaration(world, &reference, std::mem::take(context))?;
        } else {
            build_domain(world, instance, context, target)?;
        }
    }
    context.functions.units[&name]
        .as_ref()
        .unwrap()
        .contract
        .domain
        .clone()
        .ok_or_else(|| Diagnostic {
            source: instance.definition.source.clone(),
            span: instance.definition.function.span,
            code: "B270",
            message: "模块入口缺少整域合同".into(),
        })
}
impl Builder<'_> {
    pub(super) fn prepare_domain(
        &mut self,
        instance: &crate::generics::Instance,
        target: Target,
    ) -> Result<crate::module::domain::Interface> {
        let mut context = self.application.take().unwrap();
        let interface = domain_at(self.world, instance, &mut context, target)?;
        self.application = Some(context);
        Ok(interface)
    }
    pub(super) fn call_domain(
        &mut self,
        instance: crate::generics::Instance,
        target: Target,
        arguments: Vec<CallArgument>,
    ) {
        self.code.push(Inst::Call(Call {
            target: CallTarget::Defined(crate::module::domain::symbol(&instance.name, target)),
            arguments,
            result: None,
            values: Vec::new(),
        }));
    }
}
pub(crate) fn check_domain(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    domain_declaration(world, instance, &mut application::Context::default())?;
    Ok(())
}
fn domain_declaration(
    world: &World,
    instance: &crate::generics::Instance,
    context: &mut application::Context,
) -> Result<BTreeSet<String>> {
    let targets: &[Target] = match instance.definition.function.target {
        Target::Cpu => &[Target::Cpu],
        Target::Gpu => &[Target::Gpu],
        Target::Compute => &[Target::Cpu, Target::Gpu],
    };
    let mut roots = BTreeSet::new();
    for &target in targets {
        domain_at(world, instance, context, target)?;
        if instance.definition.public {
            roots.insert(crate::module::domain::symbol(&instance.name, target));
        }
    }
    Ok(roots)
}
fn build_domain(
    world: &World,
    instance: &crate::generics::Instance,
    context: &mut application::Context,
    target: Target,
) -> Result<()> {
    let d = &instance.definition;
    let expanded = expand_domain(world, instance)?;
    let mut signature = expanded.program;
    if !signature.accelerations.is_empty() || !signature.textures.is_empty() {
        return d.source.fail(
            d.function.span,
            "B270",
            "B应用的独立整域入口尚未交接纹理或加速结构；原生宿主算法接口继续使用已有产物",
        );
    }
    let stages = std::mem::take(&mut signature.stages);
    let mut frame = Ty::vector(Scalar::U32, 2);
    frame.name = "__blink_frame".into();
    let mut registers = vec![Scalar::U32, Scalar::U32];
    let mut parameters = vec![CallParameter::Value(frame.clone())];
    let frame = CallArgument::Value {
        ty: frame,
        values: vec![0, 1],
    };
    let mut data = Vec::new();
    let mut uniform_values = Vec::new();
    let mut uniform_regs = BTreeMap::new();
    let mut ui = 0;
    for param in &d.function.params {
        let ty = cooperative::parameter_type(world, param, d)?;
        let first = registers.len();
        if param.ty.dataset {
            registers.extend([Scalar::U32, Scalar::U32]);
            let count = world.shape(
                &param.ty.count,
                &d.source,
                param.span,
                &d.module,
                &d.imports,
            )?;
            parameters.push(CallParameter::Value(Ty::owned_dataset(ty, count)));
            data.push([first, first + 1]);
        } else {
            registers.extend(ty.components.iter().map(|(s, _)| *s));
            parameters.push(CallParameter::Value(ty.clone()));
            for (i, (scalar, offset)) in ty.components.iter().enumerate() {
                uniform_regs.insert(signature.uniforms[ui].offset + offset, first + i);
                uniform_values.push(CallArgument::Value {
                    ty: Ty::scalar(*scalar),
                    values: vec![first + i],
                });
            }
            ui += 1;
        }
    }
    let inputs: Vec<_> = (0..registers.len()).collect();
    let mut code = Vec::new();
    let mut interface = crate::module::domain::Interface::new(&signature, target);
    interface.outputs.clear();
    let mut start = 0;
    let mut required = signature.prefix_requirements.clone();
    let split = !expanded.calls.is_empty();
    let mut part = 0;
    for call in expanded.calls {
        emit_domain_part(
            &signature,
            &stages[start..call.stage],
            &required,
            &call.before,
            split,
            &mut part,
            context,
            target,
            &frame,
            &data,
            &uniform_values,
            &mut code,
            &mut interface,
        );
        let callee = domain_at(world, &call.instance, context, target)?;
        let mut arguments = vec![frame.clone()];
        let bindings = call
            .arguments
            .iter()
            .filter_map(|root| {
                if let Root::Data(i, _) = root {
                    Some(*i)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for root in &call.arguments {
            arguments.push(match root {
                Root::Data(i, _) => CallArgument::Value {
                    ty: Ty::owned_dataset(
                        signature.buffers[*i].ty.clone(),
                        signature.buffers[*i].count,
                    ),
                    values: data[*i].to_vec(),
                },
                Root::Uniform(ty, offset) => CallArgument::Value {
                    ty: ty.clone(),
                    values: ty
                        .components
                        .iter()
                        .map(|(_, at)| uniform_regs[&(offset + at)])
                        .collect(),
                },
                Root::Literal(ty, words) => {
                    let values = ty
                        .components
                        .iter()
                        .zip(words)
                        .map(|((scalar, _), word)| {
                            let reg = registers.len();
                            registers.push(*scalar);
                            code.push(Inst::Constant(reg, *scalar, *word));
                            reg
                        })
                        .collect();
                    CallArgument::Value {
                        ty: ty.clone(),
                        values,
                    }
                }
                _ => {
                    return d
                        .source
                        .fail(d.function.span, "B270", "该Frame入口尚未交接此资源种类");
                }
            });
        }
        code.push(Inst::Call(Call {
            target: CallTarget::Defined(crate::module::domain::symbol(&call.instance.name, target)),
            arguments,
            result: None,
            values: Vec::new(),
        }));
        interface
            .outputs
            .extend(callee.outputs.into_iter().map(|mut output| {
                output.binding = bindings[output.binding];
                output.domain = output.domain.map(|i| bindings[i]);
                output
            }));
        start = call.stage;
        required = call.after;
    }
    emit_domain_part(
        &signature,
        &stages[start..],
        &required,
        &signature.prefix_guarantees,
        split,
        &mut part,
        context,
        target,
        &frame,
        &data,
        &uniform_values,
        &mut code,
        &mut interface,
    );
    interface.outputs.sort();
    interface.outputs.dedup();
    let name = crate::module::domain::symbol(&instance.name, target);
    let body = ValueCode {
        registers,
        locals: Vec::new(),
        inputs: inputs.clone(),
        outputs: Vec::new(),
        code,
    };
    let function = FunctionBody {
        name: name.clone(),
        parameters: parameters.clone(),
        result: None,
        body,
    };
    let contract = Contract {
        parameters,
        result: None,
        inputs,
        returned: None,
        written: vec![None; d.function.params.len()],
        requires: crate::contract_signature(&d.function)
            .into_iter()
            .map(|(kind, predicate)| (kind, predicate.clone()))
            .collect(),
        target: Target::Cpu,
        frame: false,
        domain: Some(interface),
        residency: BTreeMap::new(),
    };
    context.functions.units.insert(
        name,
        Some(Unit {
            function: Some(function),
            contract,
            module: d.module.clone(),
            public: d.public,
            residency_calls: Vec::new(),
            residency_uses: Vec::new(),
        }),
    );
    Ok(())
}
fn emit_domain_part(
    signature: &Program,
    stages: &[Stage],
    required: &[PrefixRelation],
    guaranteed: &[PrefixRelation],
    split: bool,
    part: &mut usize,
    context: &mut application::Context,
    target: Target,
    frame: &CallArgument,
    data: &[[Reg; 2]],
    uniforms: &[CallArgument],
    code: &mut Vec<Inst>,
    interface: &mut crate::module::domain::Interface,
) {
    if stages.is_empty() {
        return;
    }
    let mut program = signature.clone();
    if split {
        program.name = format!("{}@part{}", signature.name, *part);
    }
    *part += 1;
    program.stages = stages.to_vec();
    program.source_stages = stages.len();
    program.prefix_requirements = required.to_vec();
    program.prefix_guarantees = guaranteed.to_vec();
    crate::optimize::optimize(&mut program);
    interface
        .outputs
        .extend(crate::module::domain::Interface::new(&program, target).outputs);
    let index = context.resources.programs.len();
    context.resources.programs.push((target, program));
    let mut arguments = vec![frame.clone()];
    arguments.extend(data.iter().map(|regs| CallArgument::Value {
        ty: Ty::scalar(Scalar::U32),
        values: vec![regs[0]],
    }));
    arguments.extend_from_slice(uniforms);
    code.push(application::runtime_call(
        &format!("run_{index}"),
        arguments,
        None,
        Vec::new(),
    ));
}
