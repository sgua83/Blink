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
}
crate::module::codec::record!(Contract {
    parameters,
    result,
    inputs,
    returned,
    written,
    requires,
    target,
    frame
});
struct Unit {
    function: Option<FunctionBody>,
    contract: Contract,
    module: String,
    public: bool,
}
fn declaration(
    world: &World,
    instance: &crate::generics::Instance,
    mut context: application::Context,
    interface: &[Parameter],
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
    let checked = checked_function(world, &instance, Some(context), interface)?;
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
    };
    crate::optimize::value_code(&mut function.body);
    context.functions.units.insert(
        instance.name.clone(),
        Some(Unit {
            function: Some(function),
            contract,
            module: instance.definition.module.clone(),
            public: instance.definition.public,
        }),
    );
    Ok(context)
}
pub(super) fn canonical(
    builder: &mut Builder<'_>,
    returned: Option<Returned>,
    inputs: &[Reg],
    written: Vec<Option<bounds::Proof>>,
) -> (Option<Returned>, Vec<Reg>, Vec<Option<bounds::Proof>>) {
    let bindings = inputs
        .iter()
        .enumerate()
        .map(|(index, reg)| {
            (
                ValueIdentity::Register(*reg),
                Value {
                    ty: Ty::scalar(Scalar::U32),
                    regs: vec![index],
                    proof: bounds::Proof::default(),
                    literal: false,
                    uniform: false,
                    constant: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let returned = returned.map(|model| {
        let storage = builder.returned_storage(&model);
        let value = Value {
            ty: storage.ty,
            regs: (inputs.len()..inputs.len() + storage.regs.len()).collect(),
            proof: bounds::Proof::default(),
            literal: false,
            uniform: false,
            constant: None,
        };
        let mut value = builder.template_returned(value, true);
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
        let context = self.application.take().unwrap();
        self.application = Some(declaration(self.world, instance, context, interface)?);
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
                    unit.contract.inputs.clone(),
                    unit.contract.returned.clone(),
                    unit.contract.written.clone(),
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
    module: Module,
    mut reachable: BTreeSet<String>,
    mut root: Option<&mut ValueCode>,
    owner: Option<&str>,
) -> Finalized {
    use crate::module::codec::Encode;
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
    result
}
pub(super) fn finish(module: Module, root: &mut ValueCode) -> Finalized {
    let roots = dependencies(root);
    finalize(module, roots, Some(root), None)
}

fn default_symbol(world: &World, instance: &crate::generics::Instance) -> Result<String> {
    fn states(ty: &Ty, out: &mut String) {
        if ty.dataset.is_some() {
            out.push('c');
        } else if ty.owns_resources() {
            out.push('(');
            if ty.result {
                for ty in &ty.arguments {
                    states(ty, out);
                }
            } else {
                for (_, _, ty) in &ty.fields {
                    if ty.owns_resources() {
                        states(ty, out);
                    } else {
                        out.push('-');
                    }
                }
            }
            out.push(')');
        }
    }
    let mut name = instance.name.clone();
    for (index, param) in instance.definition.function.params.iter().enumerate() {
        let ty = cooperative::parameter_type(world, param, &instance.definition)?;
        if !param.ty.dataset && matches!(param.access, Access::Write | Access::InOut) {
            name.push_str(&format!(
                "#r{index}{:?}",
                ty.components
                    .iter()
                    .map(|(_, offset)| offset / 4)
                    .collect::<Vec<_>>()
            ));
        } else if !param.ty.dataset && param.access == Access::Value && ty.owns_resources() {
            name.push_str(&format!("#s{index}"));
            states(&ty, &mut name);
        }
    }
    Ok(name)
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
        for mut instance in crate::generics::instances(world, key, d)? {
            if d.function.external.is_none()
                && d.function.kind == FunctionKind::Value
                && d.function.target != Target::Gpu
            {
                instance.name = default_symbol(world, &instance)?;
                if d.public {
                    roots.insert(instance.name.clone());
                }
                context = declaration(world, &instance, context, &[])?;
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
        let mut instance = super::generics::specialize(
            world,
            &key,
            &[],
            arguments.as_deref(),
            d,
            d.function.span,
        )?;
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
        instance.name = default_symbol(world, &instance)?;
        roots.insert(instance.name.clone());
        context = declaration(world, &instance, context, &[])?;
    }
    let finalized = finalize(context.functions, roots, None, Some(entry));
    let mut functions = finalized.functions;
    let resources = application::module_resources(&mut functions, context.resources);
    Ok(crate::module::Module {
        name: entry.into(),
        header: world.interfaces[entry].text.to_string(),
        functions,
        imports: finalized.imports,
        exports: finalized.exports,
        resources,
    })
}
