//! 普通 CPU 函数拥有独立调用帧；编译中的符号声明截断递归展开。
use super::*;
use cooperative::Argument;
use returns::Returned;

pub(super) enum Parameter {
    Value,
    Reference(Vec<u32>),
    Owned(Returned),
}
fn resource_states(value: &Returned, states: &mut String) {
    match value {
        Returned::Dataset(data) => states.push(match data.residency {
            Some(Target::Cpu) => 'c',
            Some(Target::Gpu) => 'g',
            _ => 'u',
        }),
        Returned::OwnedValue(value) => {
            states.push('(');
            for item in &value.payloads {
                if let Some(item) = item {
                    resource_states(item, states);
                } else {
                    states.push('-');
                }
            }
            states.push(')');
        }
        _ => {}
    }
}
fn parameter_symbol(name: &str, parameters: &[Parameter]) -> String {
    let mut symbol = name.to_owned();
    for (i, parameter) in parameters.iter().enumerate() {
        match parameter {
            Parameter::Reference(words) => symbol.push_str(&format!("#r{i}{words:?}")),
            Parameter::Owned(value) => {
                symbol.push_str(&format!("#s{i}"));
                resource_states(value, &mut symbol);
            }
            _ => {}
        }
    }
    symbol
}
pub(super) fn parameter_resources(value: &mut owned_values::OwnedValue, origin: &Returned) {
    let Returned::OwnedValue(origin) = origin else {
        unreachable!()
    };
    fn copy(value: &mut Returned, origin: &Returned) {
        match (value, origin) {
            (Returned::Dataset(value), Returned::Dataset(origin)) => {
                value.residency = origin.residency
            }
            (Returned::OwnedValue(value), Returned::OwnedValue(origin)) => {
                for (value, origin) in value.payloads.iter_mut().zip(&origin.payloads) {
                    if let (Some(value), Some(origin)) = (value, origin) {
                        copy(value, origin);
                    }
                }
            }
            _ => {}
        }
    }
    for (value, origin) in value.payloads.iter_mut().zip(&origin.payloads) {
        if let (Some(value), Some(origin)) = (value, origin) {
            copy(value, origin);
        }
    }
}
#[derive(Default)]
pub(super) struct Module {
    units: BTreeMap<String, Option<Unit>>,
}
struct Unit {
    function: FunctionBody,
    inputs: Vec<Reg>,
    returned: Option<Returned>,
    written: Vec<Option<bounds::Proof>>,
}
pub(super) fn check(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let mut context = application::Context::default();
    context.functions.units.insert(instance.name.clone(), None);
    checked_function(world, instance, Some(context), &[]).map(|_| ())
}
impl Builder<'_> {
    fn native_declaration(
        &mut self,
        instance: &crate::generics::Instance,
        interface: &[Parameter],
    ) -> Result<()> {
        if self
            .application
            .as_ref()
            .unwrap()
            .functions
            .units
            .contains_key(&instance.name)
        {
            return Ok(());
        }
        let outer = self.application.as_mut().unwrap();
        outer.functions.units.insert(instance.name.clone(), None);
        let context = application::Context {
            frame: None,
            resources: std::mem::take(&mut outer.resources),
            functions: std::mem::take(&mut outer.functions),
        };
        let mut instance = instance.clone();
        if instance.definition.function.target == Target::Compute {
            Arc::make_mut(&mut instance.definition).function.target = Target::Cpu;
        }
        let checked = checked_function(self.world, &instance, Some(context), interface)?;
        let mut context = checked.application.unwrap();
        let mut function = checked.body;
        crate::optimize::value_code(&mut function.body);
        context.functions.units.insert(
            instance.name.clone(),
            Some(Unit {
                function,
                inputs: checked.inputs,
                returned: checked.returned,
                written: checked.written,
            }),
        );
        let outer = self.application.as_mut().unwrap();
        outer.functions = context.functions;
        outer.resources = context.resources;
        Ok(())
    }
    pub(super) fn native_function(
        &mut self,
        mut instance: crate::generics::Instance,
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
            .map(|reg| {
                Some(Value {
                    ty: Ty::scalar(Scalar::U32),
                    regs: vec![*reg],
                    proof: bounds::Proof::default(),
                    literal: false,
                    uniform: false,
                    constant: None,
                })
            })
            .collect::<Vec<_>>();
        let mut env = BTreeMap::new();
        let mut interface = Vec::new();
        let mut written = Vec::new();
        let mut written_data = Vec::new();
        let mut held = Vec::new();
        for (index, (p, arg)) in d.function.params.iter().zip(args).enumerate() {
            let ty = cooperative::parameter_type(self.world, p, d)?;
            let arg = match arg {
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
                    interface.push(Parameter::Value);
                    append_actual(&mut actual, &value);
                    arguments.push(CallArgument::Value {
                        ty: value.ty.clone(),
                        values: value.regs.clone(),
                    });
                    env.insert(p.name.clone(), Binding::Value(value));
                }
                Argument::Exclusive(place) if matches!(p.access, Access::Write | Access::InOut) => {
                    self.require_same(&ty, place.ty(), span)?;
                    let (address, layout) = self.call_reference(&place, p.access, span)?;
                    interface.push(Parameter::Reference(layout));
                    arguments.push(CallArgument::Reference {
                        ty,
                        access: p.access,
                        address,
                    });
                    // Pointer input identities are private to the callee; they do not alias caller scalar registers.
                    actual.extend([None, None]);
                    env.insert(
                        p.name.clone(),
                        Binding::Owned(self.borrow_owned(place.clone())),
                    );
                    written.push((index, place));
                }
                Argument::ApplicationData(data) if p.ty.dataset => {
                    interface.push(Parameter::Value);
                    self.application_cpu(&data, span)?;
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
                Argument::OwnedValue(value) if p.access == Access::Value && ty.owns_resources() => {
                    self.require_same(&ty, &value.value.ty, span)?;
                    interface.push(Parameter::Owned(Returned::OwnedValue(value.clone())));
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
        instance.name = parameter_symbol(&instance.name, &interface);
        self.native_declaration(&instance, &interface)?;
        let result = if d.function.result.name == "void" {
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
        let prototype = self.application.as_ref().unwrap().functions.units[&instance.name]
            .as_ref()
            .map(|unit| {
                (
                    unit.inputs.clone(),
                    unit.returned.clone(),
                    unit.written.clone(),
                )
            });
        let (bindings, model, writes) = if let Some((inputs, model, writes)) = prototype {
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
        } else {
            unknown_resources(&mut returned);
        }
        Ok(Some(returned))
    }
}
fn append_actual(actual: &mut Vec<Option<Value>>, value: &Value) {
    for (i, (scalar, _)) in value.ty.components.iter().enumerate() {
        actual.push(Some(Value {
            ty: Ty::scalar(*scalar),
            regs: vec![value.regs[i]],
            proof: value.projected_proof(&[i]),
            literal: false,
            uniform: value.uniform,
            constant: value.constant.as_ref().map(|v| vec![v[i]]),
        }));
    }
}
fn translate_value(value: &mut Value, model: &Value, bindings: &BTreeMap<ValueIdentity, Value>) {
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
    bindings: &BTreeMap<ValueIdentity, Value>,
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
            value.residency = model.residency;
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
fn unknown_resources(value: &mut Returned) {
    match value {
        Returned::Dataset(data) => data.residency = None,
        Returned::OwnedValue(value) => value.resources_mut(&mut |data| data.residency = None),
        _ => {}
    }
}

pub(super) fn finish(module: Module, root: &mut ValueCode) -> Vec<FunctionBody> {
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
    let mut functions = module
        .units
        .into_iter()
        .map(|(name, unit)| (name, unit.unwrap().function))
        .collect::<BTreeMap<_, _>>();
    let mut reachable = dependencies(root);
    loop {
        let mut next = reachable.clone();
        for name in &reachable {
            next.extend(dependencies(&mut functions.get_mut(name).unwrap().body));
        }
        if next == reachable {
            break;
        }
        reachable = next;
    }
    functions.retain(|name, _| reachable.contains(name));
    let mut frame = BTreeSet::new();
    for (name, function) in &mut functions {
        calls(&mut function.body.code, &mut |call| {
            if matches!(&call.target,CallTarget::External(name) if name.library==crate::application::RUNTIME_LIBRARY)
            {
                frame.insert(name.clone());
            }
        });
    }
    loop {
        let mut next = frame.clone();
        for (name, function) in &mut functions {
            if dependencies(&mut function.body)
                .iter()
                .any(|callee| frame.contains(callee))
            {
                next.insert(name.clone());
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
    prune(root, &frame);
    for (name, function) in &mut functions {
        prune(&mut function.body, &frame);
        if !frame.contains(name) {
            function.parameters.remove(0);
            function.body.inputs.drain(..2);
        }
        crate::optimize::value_code(&mut function.body);
    }
    functions.into_values().collect()
}
