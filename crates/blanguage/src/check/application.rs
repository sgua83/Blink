//! 应用入口是一次普通 CPU 值函数调用，不伪造 dataset 或 parallel 域。
use super::cooperative::Argument;
use super::*;
#[derive(Default)]
pub(super) struct Context {
    pub frame: Option<Value>,
    pub resources: crate::application::Resources,
    pub functions: native_function::Module,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Ownership {
    Owned,
    Borrowed,
    Moved,
}
#[derive(Clone)]
pub(super) struct Data {
    pub handle: Value,
    pub slots: Option<[usize; 2]>,
    pub initialization: Option<usize>,
    pub ty: Ty,
    pub length: Value,
    pub ownership: Ownership,
    pub writable: bool,
    pub count: Option<u32>,
    pub residency: Option<Target>,
}
fn pointer_type() -> Ty {
    let mut ty = Ty::vector(Scalar::U32, 2);
    ty.name = "__blink_frame".into();
    ty
}
fn argument(value: Value) -> CallArgument {
    CallArgument::Value {
        ty: value.ty,
        values: value.regs,
    }
}
pub(super) fn runtime_call(
    name: &str,
    arguments: Vec<CallArgument>,
    result: Option<Ty>,
    values: Vec<Reg>,
) -> Inst {
    Inst::Call(Call {
        target: CallTarget::External(ExternalName {
            library: crate::application::RUNTIME_LIBRARY.into(),
            symbol: format!("blink_{name}"),
        }),
        arguments,
        result,
        values,
    })
}
pub(crate) fn compile_application(
    world: &World,
    instance: &crate::generics::Instance,
) -> Result<crate::application::Application> {
    let d = &instance.definition;
    validate_signature(d)?;
    let result = if d.function.result.name == "void" || d.function.result.is_never() {
        None
    } else {
        Some(world.ty(
            &d.function.result,
            &d.source,
            d.function.span,
            &d.module,
            &d.imports,
        )?)
    };
    if d.function.kind != FunctionKind::Value
        || d.function.external.is_some()
        || d.function.target != Target::Cpu
        || !d.function.params.is_empty()
        || !result.is_some_and(|result| {
            [Ty::scalar(Scalar::U32), Ty::scalar(Scalar::I32)].contains(&result)
        })
    {
        return d.source.fail(
            d.function.span,
            "B262",
            "应用入口需要无参数的 u32／i32 普通函数，显式 on cpu；返回值是进程退出码",
        );
    }
    let context = Program {
        name: instance.name.clone(),
        target: Target::Cpu,
        buffers: Vec::new(),
        accelerations: Vec::new(),
        textures: Vec::new(),
        uniforms: Vec::new(),
        uniform_bytes: 0,
        source_stages: 0,
        stages: Vec::new(),
        prefix_requirements: Vec::new(),
        prefix_guarantees: Vec::new(),
    };
    let mut builder = Builder::new(world, d, &context, 0);
    builder.application = Some(Context::default());
    builder.in_function = true;
    let result = builder.native_function(instance.clone(), Vec::new(), d.function.span)?;
    let result = builder.ordinary_return(result, d.function.span)?.unwrap();
    let mut app = builder.application.take().unwrap();
    let mut body = ValueCode {
        registers: builder.registers,
        locals: builder.locals,
        code: builder.code,
        inputs: app
            .frame
            .as_ref()
            .map(|frame| frame.regs.clone())
            .unwrap_or_default(),
        outputs: result.regs,
    };
    crate::optimize::value_code(&mut body);
    if let Some(frame) = &mut app.frame {
        frame.regs = std::mem::take(&mut body.inputs);
    }
    let finalized = native_function::finish(std::mem::take(&mut app.functions), &mut body);
    let mut functions = finalized.functions;
    let imports = finalized.imports;
    let resources = live_resources(&mut body, &mut functions, app);
    Ok(crate::application::Application {
        name: instance.name.clone(),
        body,
        functions,
        imports,
        resources,
    })
}

fn runtime_calls(code: &mut [Inst], visitor: &mut impl FnMut(&mut ExternalName)) {
    for instruction in code {
        match instruction {
            Inst::Call(call) => {
                if let CallTarget::External(name) = &mut call.target
                    && name.library == crate::application::RUNTIME_LIBRARY
                {
                    visitor(name)
                }
            }
            Inst::Scope(body) => runtime_calls(body, visitor),
            Inst::If { yes, no, .. } => {
                runtime_calls(yes, visitor);
                runtime_calls(no, visitor);
            }
            Inst::Loop {
                condition, body, ..
            } => {
                runtime_calls(condition, visitor);
                runtime_calls(body, visitor);
            }
            _ => {}
        }
    }
}
fn declarations(
    codes: &mut [&mut Vec<Inst>],
    resources: crate::application::Resources,
) -> crate::application::Resources {
    let mut names = BTreeSet::new();
    for code in codes.iter_mut() {
        runtime_calls(code, &mut |call| {
            names.insert(call.symbol.clone());
        });
    }
    fn retain<T>(
        items: Vec<T>,
        prefix: &str,
        names: &BTreeSet<String>,
        renames: &mut BTreeMap<String, String>,
    ) -> Vec<T> {
        let mut live = Vec::new();
        for (old, item) in items.into_iter().enumerate() {
            let name = format!("blink_{prefix}_{old}");
            if names.contains(&name) {
                renames.insert(name, format!("blink_{prefix}_{}", live.len()));
                live.push(item);
            }
        }
        live
    }
    let mut renames = BTreeMap::new();
    let types = retain(resources.types, "allocate", &names, &mut renames);
    let programs = retain(resources.programs, "run", &names, &mut renames);
    for code in codes.iter_mut() {
        runtime_calls(code, &mut |call| {
            if let Some(name) = renames.get(&call.symbol) {
                call.symbol = name.clone();
            }
        });
    }
    crate::application::Resources { types, programs }
}
pub(super) fn module_resources(
    functions: &mut [FunctionBody],
    resources: crate::application::Resources,
) -> Option<crate::application::Resources> {
    let mut used = false;
    for function in &mut *functions {
        runtime_calls(&mut function.body.code, &mut |_| used = true);
    }
    if !used {
        return None;
    }
    let mut codes = functions
        .iter_mut()
        .map(|function| &mut function.body.code)
        .collect::<Vec<_>>();
    Some(declarations(&mut codes, resources))
}
fn live_resources(
    body: &mut ValueCode,
    functions: &mut [FunctionBody],
    app: Context,
) -> Option<crate::application::Resources> {
    let frame = app.frame?;
    fn uses(code: &[Inst], registers: &[Reg]) -> bool {
        code.iter().any(|inst| {
            inst.operands().iter().any(|reg| registers.contains(reg))
                || match inst {
                    Inst::Scope(code) => uses(code, registers),
                    Inst::If { yes, no, .. } => uses(yes, registers) || uses(no, registers),
                    Inst::Loop {
                        condition, body, ..
                    } => uses(condition, registers) || uses(body, registers),
                    _ => false,
                }
        })
    }
    if !uses(&body.code, &frame.regs) {
        return None;
    }
    let mut codes = std::iter::once(&mut body.code)
        .chain(functions.iter_mut().map(|function| &mut function.body.code))
        .collect::<Vec<_>>();
    let resources = declarations(&mut codes, app.resources);
    body.code.insert(
        0,
        runtime_call(
            "create",
            Vec::new(),
            Some(frame.ty.clone()),
            frame.regs.clone(),
        ),
    );
    body.code.push(runtime_call(
        "destroy",
        vec![argument(frame)],
        None,
        Vec::new(),
    ));
    Some(resources)
}
impl Builder<'_> {
    // 独立检查 CPU 函数时，参数提供符号身份和真实接口形状；不创建物理存储。
    // 原生调用交接同一 Frame 的句柄与长度，物理资源仍由原帧拥有。
    pub(super) fn application_parameter(&mut self, binding: usize, access: Access) -> Result<Data> {
        if access == Access::Value {
            self.application_frame(self.definition.function.span)?;
        }
        let reg = self.reg(Scalar::U32);
        let handle = self.application_handle(reg);
        let length_reg = self.reg(Scalar::U32);
        let count = self.program.buffers[binding].count;
        let mut length = Value {
            ty: Ty::scalar(Scalar::U32),
            regs: vec![length_reg],
            proof: count
                .map(|n| bounds::Proof::unsigned(n, n))
                .unwrap_or_default(),
            literal: false,
            uniform: false,
            constant: count.map(|n| vec![n]),
        };
        length.proof = bounds::Proof::capture(&length).with_domains(Domains::from([
            IndexDomain::ApplicationLength(handle.identity()),
        ]));
        let initialization = if access == Access::Write {
            Some(self.application_output_scope())
        } else {
            None
        };
        Ok(Data {
            handle,
            slots: None,
            initialization,
            ty: self.program.buffers[binding].ty.clone(),
            length,
            count: self.program.buffers[binding].count,
            ownership: if access == Access::Value {
                Ownership::Owned
            } else {
                Ownership::Borrowed
            },
            writable: matches!(access, Access::Value | Access::InOut | Access::Write),
            residency: Some(Target::Cpu),
        })
    }
    pub(super) fn application_iteration_outputs(
        &self,
        begin: &Value,
        limit: &Value,
    ) -> Vec<(Data, Vec<u32>)> {
        let domains = limit.proof.domains(0);
        self.env.values().filter_map(|binding| {
            let Binding::ApplicationData(data) = binding else { return None; };
            let scope = data.initialization?;
            if !(domains.contains(&IndexDomain::ApplicationLength(data.handle.identity()))
                || limit.identity() == data.length.identity()
                || limit.minimum() == limit.maximum() && limit.minimum() == data.length.minimum() && limit.maximum() == data.length.maximum()) { return None; }
            let words = data.ty.components.iter().filter_map(|(_, offset)| {
                let word = offset / 4;
                let prefix = begin.minimum();
                (self.application_field_initialized(data, word) || begin.maximum() == prefix &&
                    self.initialized.iter().filter(|position| matches!(position,
                        Position::ApplicationElement(owner, ownership::ApplicationIndex::Constant(index), component)
                        if *owner == scope && *component == word && *index < prefix)).count() as u64 == u64::from(prefix)).then_some(word)
            }).collect();
            Some((data.clone(), words))
        }).collect()
    }
    pub(super) fn application_iteration_prefix(
        &self,
        outputs: &[(Data, Vec<u32>)],
        index: ownership::ApplicationIndex,
    ) -> Vec<(Data, Vec<u32>)> {
        outputs
            .iter()
            .map(|(data, words)| {
                (
                    data.clone(),
                    words
                        .iter()
                        .copied()
                        .filter(|word| {
                            flow::initialized_application_word(
                                &self.initialized,
                                data.initialization.unwrap(),
                                index,
                                *word,
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    }
    pub(super) fn initialize_application_words(&mut self, data: &Data, words: &[u32]) {
        let scope = data.initialization.unwrap();
        self.initialized.extend(
            words
                .iter()
                .map(|word| Position::Parameter(scope, *word as usize)),
        );
    }
    pub(super) fn application_empty_initialization(
        &self,
        mut initialized: BTreeSet<Position>,
    ) -> BTreeSet<Position> {
        for binding in self.env.values() {
            if let Binding::ApplicationData(data) = binding
                && let Some(scope) = data.initialization
                && data.length.maximum() == 0
            {
                initialized.extend(
                    data.ty
                        .components
                        .iter()
                        .map(|(_, offset)| Position::Parameter(scope, (offset / 4) as usize)),
                );
            }
        }
        initialized
    }
    pub(super) fn application_initialization_state(&self) -> BTreeSet<Position> {
        self.application_empty_initialization(self.initialized.clone())
    }
    pub(super) fn take_application_initialization_state(&mut self) -> BTreeSet<Position> {
        let initialized = std::mem::take(&mut self.initialized);
        self.application_empty_initialization(initialized)
    }
    fn initialize_application_domain(
        &mut self,
        interface: &crate::module::domain::Interface,
        data: &[Data],
    ) {
        let same_length = |a: &Data, b: &Data| {
            a.length.identity() == b.length.identity()
                || a.length.minimum() == a.length.maximum()
                    && a.length.minimum() == b.length.minimum()
                    && a.length.maximum() == b.length.maximum()
        };
        for output in &interface.outputs {
            let Some(scope) = data[output.binding].initialization else {
                continue;
            };
            match output.domain {
                Some(domain) if same_length(&data[output.binding], &data[domain]) => {
                    self.initialized.extend(
                        output
                            .words
                            .iter()
                            .map(|word| Position::Parameter(scope, *word as usize)),
                    );
                }
                None => self.initialized.extend(output.words.iter().map(|word| {
                    Position::ApplicationElement(
                        scope,
                        ownership::ApplicationIndex::Constant(0),
                        *word,
                    )
                })),
                _ => {}
            }
        }
    }
    pub(super) fn application_output_scope(&mut self) -> usize {
        self.output_scopes += 1;
        self.output_scopes
    }
    pub(super) fn application_initialized(&self, data: &Data) -> bool {
        data.ty
            .components
            .iter()
            .all(|(_, offset)| self.application_field_initialized(data, offset / 4))
    }
    pub(super) fn application_field_initialized(&self, data: &Data, word: u32) -> bool {
        self.application_field_initialized_in(data, word, &self.initialized)
    }
    pub(super) fn application_field_initialized_in(
        &self,
        data: &Data,
        word: u32,
        initialized: &BTreeSet<Position>,
    ) -> bool {
        let Some(scope) = data.initialization else {
            return true;
        };
        if initialized.contains(&Position::Parameter(scope, word as usize)) {
            return true;
        }
        let length = data.length.minimum();
        data.length.maximum() == length && initialized.iter().filter(|position| matches!(position,
            Position::ApplicationElement(owner, ownership::ApplicationIndex::Constant(index), component)
            if *owner == scope && *component == word && *index < length)).count() as u64 == u64::from(length)
    }
    pub(super) fn require_application_initialized(&self, data: &Data, span: Span) -> Result<()> {
        if !self.application_initialized(data) {
            return self.fail(
                span,
                "B142",
                "读取或 inout 借用了尚未完整初始化的 dataset；write 输出须先完整写入所需元素与字段",
            );
        }
        Ok(())
    }
    pub(super) fn initialize_application(&mut self, data: &Data) {
        if let Some(scope) = data.initialization {
            self.initialized.extend(
                data.ty
                    .components
                    .iter()
                    .map(|(_, offset)| Position::Parameter(scope, (offset / 4) as usize)),
            );
        }
    }
    pub(super) fn application_frame(&mut self, span: Span) -> Result<Value> {
        if self.application.is_none() {
            return self.fail(
                span,
                "B266",
                "动态数据与 CPU／GPU program 调用属于普通 CPU 应用主流程",
            );
        }
        if let Some(frame) = self.application.as_ref().unwrap().frame.clone() {
            return Ok(frame);
        }
        let ty = pointer_type();
        let frame = Value {
            proof: bounds::Proof::default(),
            literal: false,
            regs: ty.components.iter().map(|(s, _)| self.reg(*s)).collect(),
            ty,
            uniform: false,
            constant: None,
        };
        self.application.as_mut().unwrap().frame = Some(frame.clone());
        Ok(frame)
    }
    fn application_call(
        &mut self,
        name: &str,
        args: Vec<Value>,
        result: Option<Ty>,
        span: Span,
    ) -> Result<Option<Value>> {
        let frame = self.application_frame(span)?;
        let values: Vec<_> = result
            .iter()
            .flat_map(|ty| &ty.components)
            .map(|(s, _)| self.reg(*s))
            .collect();
        let arguments = std::iter::once(frame).chain(args).map(argument).collect();
        self.code.push(runtime_call(
            name,
            arguments,
            result.clone(),
            values.clone(),
        ));
        Ok(result.map(|ty| Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty,
            regs: values,
            uniform: false,
            constant: None,
        }))
    }
    fn application_release(&self, handle: Reg) -> Inst {
        let frame = self.application.as_ref().unwrap().frame.clone().unwrap();
        runtime_call(
            "release",
            vec![
                argument(frame),
                CallArgument::Value {
                    ty: Ty::scalar(Scalar::U32),
                    values: vec![handle],
                },
            ],
            None,
            Vec::new(),
        )
    }
    pub(super) fn release_application_handle(&mut self, handle: Reg) {
        self.code.push(self.application_release(handle));
    }
    pub(super) fn application_join_releases(
        &mut self,
        environment: &BTreeMap<String, Binding>,
        other: &BTreeMap<String, Binding>,
    ) -> Vec<Inst> {
        let old = std::mem::take(&mut self.code);
        for (name, binding) in environment {
            let a = match binding {
                Binding::ApplicationData(d) => Some(returns::Returned::Dataset(d.clone())),
                Binding::OwnedValue(v) => Some(returns::Returned::OwnedValue(v.clone())),
                _ => None,
            };
            let b = match other.get(name) {
                Some(Binding::ApplicationData(d)) => Some(returns::Returned::Dataset(d.clone())),
                Some(Binding::OwnedValue(v)) => Some(returns::Returned::OwnedValue(v.clone())),
                _ => None,
            };
            if let (Some(a), Some(b)) = (a, b) {
                self.join_owned_releases(a, &b, self.definition.function.span)
                    .unwrap();
            }
        }
        std::mem::replace(&mut self.code, old)
    }
    pub(super) fn application_drop(&mut self, data: &Data) -> Vec<Inst> {
        let mut code = Vec::new();
        let handle = if let Some(slots) = data.slots {
            let reg = self.reg(Scalar::U32);
            code.push(Inst::LoadLocal(reg, slots[0]));
            reg
        } else {
            data.handle.regs[0]
        };
        code.push(self.application_release(handle));
        code
    }
    pub(super) fn application_available(&self, data: &Data, span: Span) -> Result<()> {
        if data.ownership == Ownership::Moved {
            return self.fail(
                span,
                "B227",
                "dataset 尚未初始化或所有权已经移动，原绑定不能访问或再次转移",
            );
        }
        Ok(())
    }
    pub(super) fn application_handle_available(
        &self,
        identity: ValueIdentity,
        span: Span,
    ) -> Result<()> {
        let mut matching = self
            .env
            .values()
            .flat_map(|binding| match binding {
                Binding::ApplicationData(d) => vec![d],
                Binding::OwnedValue(v) => v.resources(),
                _ => Vec::new(),
            })
            .filter(|d| d.handle.proof.known_identity(0) == Some(identity))
            .peekable();
        if matching.peek().is_some() && matching.all(|data| data.ownership == Ownership::Moved) {
            return self.fail(
                span,
                "B227",
                "dataset 在当前表达式中已移动，先前借用或元素位置不能继续消费",
            );
        }
        Ok(())
    }
    pub(super) fn application_borrowed(&self, identity: ValueIdentity) -> bool {
        self.env.values().any(|binding| {
            let place = match binding {
                Binding::Owned(owner) => &owner.place,
                Binding::AtomicBorrow(place) => place,
                _ => return false,
            };
            matches!(place.storage(), Place::ApplicationElement { identity: owner, .. } if *owner == identity)
        })
    }
    pub(super) fn application_movable(&self, data: &Data, span: Span) -> Result<()> {
        if self.application_borrowed(data.handle.identity()) {
            return self.fail(
                span,
                "B227",
                "dataset 的元素仍被当前游标或位置借用，不能交接或覆盖整份所有权",
            );
        }
        Ok(())
    }
    fn application_handle(&self, reg: Reg) -> Value {
        let mut value = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![reg],
            uniform: false,
            constant: None,
        };
        value.proof = bounds::Proof::capture(&value);
        value
    }
    pub(super) fn application_value(&mut self, name: &str, span: Span) -> Result<Option<Data>> {
        let Some(Binding::ApplicationData(mut data)) = self.env.get(name).cloned() else {
            return Ok(None);
        };
        self.application_available(&data, span)?;
        data = self.refresh_application_data(data);
        if let Some(Binding::ApplicationData(source)) = self.env.get_mut(name) {
            source.handle.proof = data.handle.proof.clone();
            source.length.proof = data.length.proof.clone();
        }
        Ok(Some(data))
    }
    pub(super) fn refresh_application_data(&mut self, mut data: Data) -> Data {
        if let Some(slots) = data.slots {
            let handle = self.reg(Scalar::U32);
            let length = self.reg(Scalar::U32);
            self.code.push(Inst::LoadLocal(handle, slots[0]));
            self.code.push(Inst::LoadLocal(length, slots[1]));
            data.handle.regs = vec![handle];
            data.length.regs = vec![length];
            data.handle.proof = bounds::Proof::capture(&data.handle);
            data.length.proof = bounds::Proof::capture(&data.length).with_domains(Domains::from([
                IndexDomain::ApplicationLength(data.handle.identity()),
            ]));
            data.length.constant = data.length.proof.known_value(&data.length.ty);
        }
        data
    }
    pub(super) fn application_bind(&mut self, mut data: Data, mutable: bool) -> Data {
        data.slots = None;
        data.handle.proof = bounds::Proof::capture(&data.handle);
        data.length.proof = bounds::Proof::capture(&data.length);
        if mutable {
            let slots = [self.locals.len(), self.locals.len() + 1];
            self.locals.extend([Scalar::U32, Scalar::U32]);
            self.code
                .push(Inst::StoreLocal(slots[0], data.handle.regs[0]));
            self.code
                .push(Inst::StoreLocal(slots[1], data.length.regs[0]));
            data.slots = Some(slots);
        }
        data
    }
    pub(super) fn release_application_data(&mut self, keep: Option<&BTreeSet<String>>) {
        let bindings: Vec<_> = self
            .env
            .iter()
            .filter(|(name, _)| !keep.is_some_and(|names| names.contains(*name)))
            .filter_map(|(_, binding)| match binding {
                Binding::ApplicationData(data) if data.ownership == Ownership::Owned => {
                    Some(binding.clone())
                }
                Binding::OwnedValue(result) if result.ownership == Ownership::Owned => {
                    Some(binding.clone())
                }
                _ => None,
            })
            .collect();
        for binding in bindings {
            self.drop_binding(binding, self.definition.function.span)
                .unwrap();
        }
    }
    pub(super) fn application_data_type(
        &self,
        data: &mut Data,
        ty: &Ty,
        count: Option<u32>,
        span: Span,
    ) -> Result<()> {
        self.require_same(ty, &data.ty, span)?;
        if let Some(count) = count {
            if data.length.minimum() != count || data.length.maximum() != count {
                return self.fail(span, "B167", "dataset 实际长度未满足声明的固定形状");
            }
            data.count = Some(count);
        }
        Ok(())
    }
    pub(super) fn application_declare(
        &mut self,
        annotation: &TypeName,
        name: &str,
        span: Span,
    ) -> Result<()> {
        self.application_frame(span)?;
        let definition = self.definition.clone();
        let ty = self.world.ty(
            annotation,
            &definition.source,
            span,
            &definition.module,
            &definition.imports,
        )?;
        let count = self.world.shape(
            &annotation.count,
            &definition.source,
            span,
            &definition.module,
            &definition.imports,
        )?;
        let slots = [self.locals.len(), self.locals.len() + 1];
        self.locals.extend([Scalar::U32, Scalar::U32]);
        let handle = self.reg(Scalar::U32);
        let length = self.reg(Scalar::U32);
        // These are absent compile-time values. No native initialization or placeholder allocation is emitted.
        let value = |reg| Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![reg],
            uniform: false,
            constant: None,
        };
        self.env.insert(
            name.into(),
            Binding::ApplicationData(Data {
                handle: value(handle),
                length: value(length),
                slots: Some(slots),
                initialization: None,
                ty,
                count,
                ownership: Ownership::Moved,
                writable: true,
                residency: None,
            }),
        );
        Ok(())
    }
    pub(super) fn application_assign(
        &mut self,
        name: &str,
        target: Data,
        op: &str,
        rhs: &Expr,
        span: Span,
    ) -> Result<()> {
        if op != "=" {
            return self.fail(
                span,
                "B124",
                "dataset 赋值交接整份拥有资源，不支持数值复合运算",
            );
        }
        if target.ownership == Ownership::Borrowed {
            return self.fail(span, "B227", "借用的 dataset 不能替换其资源所有者");
        }
        let Some(mut value) = self.application_resource(rhs, span)? else {
            return Ok(());
        };
        self.application_data_type(&mut value, &target.ty, target.count, span)?;
        if matches!(&self.env[name], Binding::ApplicationData(data) if data.ownership == Ownership::Owned)
        {
            let old = self.application_value(name, span)?.unwrap();
            self.application_movable(&old, span)?;
            self.release_application_handle(old.handle.regs[0]);
        }
        let slots = target.slots.unwrap();
        self.code
            .push(Inst::StoreLocal(slots[0], value.handle.regs[0]));
        self.code
            .push(Inst::StoreLocal(slots[1], value.length.regs[0]));
        value.slots = Some(slots);
        value.count = target.count;
        value.ownership = Ownership::Owned;
        value.writable = true;
        value.handle.proof = bounds::Proof::capture(&value.handle);
        value.length.proof = bounds::Proof::capture(&value.length);
        self.env
            .insert(name.into(), Binding::ApplicationData(value));
        Ok(())
    }
    pub(super) fn application_resource(&mut self, expr: &Expr, span: Span) -> Result<Option<Data>> {
        self.application_frame(span)?;
        if let ExprKind::Call(name, args) = &expr.kind
            && name == "try"
        {
            let value = self.propagate_returned(args, span)?;
            if !self.flow.continues() {
                return Ok(None);
            }
            return match value {
                Some(returns::Returned::Dataset(data)) => Ok(Some(data)),
                _ => self.fail(span, "B224", "try成功载荷不是dataset"),
            };
        }
        if let ExprKind::Name(name) = &expr.kind {
            let Some(mut data) = self.application_value(name, span)? else {
                return self.fail(span, "B224", "dataset 移动需要应用拥有的实际资源");
            };
            if data.ownership != Ownership::Owned {
                return self.fail(span, "B227", "借用的 dataset 不能移动或作为拥有资源返回");
            }
            self.application_movable(&data, span)?;
            if let Some(Binding::ApplicationData(source)) = self.env.get_mut(name) {
                source.ownership = Ownership::Moved;
                source.residency = None;
            }
            data.slots = None;
            return Ok(Some(data));
        }
        let (name, explicit, args) = match &expr.kind {
            ExprKind::Specialize(name, _, _) if name == "dataset" => {
                return self.create_application_dataset(expr, span);
            }
            ExprKind::Call(name, args) => (name, None, args),
            ExprKind::Specialize(name, explicit, args) => (name, Some(explicit.as_slice()), args),
            _ => {
                return self.fail(
                    span,
                    "B266",
                    "dataset 资源由构造或返回拥有资源的 CPU 函数取得",
                );
            }
        };
        let Some(key) = self.world.resolve(
            name,
            &self.definition.module,
            &self.definition.imports,
            true,
        ) else {
            return self.fail(span, "B158", format!("未知或未导出的资源函数 {name}"));
        };
        let callee = &self.world.functions[&key];
        if !callee.function.result.dataset {
            return self.fail(span, "B224", "dataset 资源声明需要返回拥有 dataset 的函数");
        }
        let Some(arguments) = self.function_arguments(&key, args, span)? else {
            return Ok(None);
        };
        let instance = self.specialized_arguments(key, &arguments, explicit, span)?;
        let Some(returns::Returned::Dataset(data)) =
            self.inline_instance(instance, arguments, span)?
        else {
            return self.fail(span, "B224", "资源函数没有交付声明的 dataset 所有权");
        };
        Ok(Some(data))
    }
    fn create_application_dataset(&mut self, expr: &Expr, span: Span) -> Result<Option<Data>> {
        self.application_frame(span)?;
        let ExprKind::Specialize(constructor, explicit, args) = &expr.kind else {
            return self.fail(
                span,
                "B266",
                "动态 dataset 使用 dataset<T>(length) 创建独占存储",
            );
        };
        if constructor != "dataset" || !(1..=2).contains(&explicit.len()) || args.len() != 1 {
            return self.fail(
                span,
                "B266",
                "dataset<T>(length) 需要元素类型和一个 u32 长度",
            );
        }
        let GenericArgument::Type(element) = &explicit[0] else {
            return self.fail(span, "B266", "dataset<T> 需要元素类型");
        };
        if element.dataset || element.tile {
            return self.fail(
                span,
                "B266",
                "dataset 的元素是普通值，不是另一份 dataset 或 tile 所有权",
            );
        }
        let definition = self.definition.clone();
        let ty = self.world.ty(
            element,
            &definition.source,
            span,
            &definition.module,
            &definition.imports,
        )?;
        let count = if let Some(argument) = explicit.get(1) {
            let Some(shape) = argument.shape() else {
                return self.fail(span, "B266", "dataset<T,N> 的 N 需要静态 u32 形状");
            };
            self.world.shape(
                &Some(shape),
                &definition.source,
                span,
                &definition.module,
                &definition.imports,
            )?
        } else {
            None
        };
        if ty.owns_resources() {
            return self.fail(span, "B224", "dataset 元素必须是普通存储，不能复制拥有句柄");
        }
        if ty.contains_views() {
            return self.fail(
                span,
                "B266",
                "动态 dataset 的元素需要普通值；view 资源借用不能由零初始化存储创建",
            );
        }
        let Some(mut length) = self.expr(&args[0])? else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(Scalar::U32), &length.ty, args[0].span)?;
        if count.is_some_and(|count| length.minimum() != count || length.maximum() != count) {
            return self.fail(
                span,
                "B266",
                "固定形状 dataset 的创建长度必须与声明的 const 形状相同",
            );
        }
        let app = self.application.as_mut().unwrap();
        let index = app
            .resources
            .types
            .iter()
            .position(|item| *item == ty)
            .unwrap_or_else(|| {
                let index = app.resources.types.len();
                app.resources.types.push(ty.clone());
                index
            });
        let handle = self
            .application_call(
                &format!("allocate_{index}"),
                vec![length.clone()],
                Some(Ty::scalar(Scalar::U32)),
                span,
            )?
            .unwrap();
        let handle = self.application_handle(handle.regs[0]);
        length.proof = bounds::Proof::capture(&length).with_domains(Domains::from([
            IndexDomain::ApplicationLength(handle.identity()),
        ]));
        Ok(Some(Data {
            handle,
            slots: None,
            initialization: None,
            ty,
            length,
            count,
            ownership: Ownership::Owned,
            writable: true,
            residency: Some(Target::Cpu),
        }))
    }
    pub(super) fn application_element(
        data: &Data,
        index: &Value,
        index_range: (u32, u32),
    ) -> Place {
        Place::ApplicationElement {
            ty: data.ty.clone(),
            handle: data.handle.regs[0],
            identity: data.handle.identity(),
            initialization: data
                .initialization
                .map(|scope| (scope, ownership::ApplicationIndex::of(index))),
            index: index.regs[0],
            index_range,
            stride: ir::align_up(data.ty.size, data.ty.align),
            words: data
                .ty
                .components
                .iter()
                .map(|(_, offset)| offset / 4)
                .collect(),
            writable: data.writable,
        }
    }
    pub(super) fn application_index(&mut self, data: Data, index: &Expr) -> Result<Option<Place>> {
        self.application_cpu(&data, index.span)?;
        let Some(value) = self.expr(index)? else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(Scalar::U32), &value.ty, index.span)?;
        if !self.index_in(
            &value,
            IndexDomain::ApplicationData(data.handle.identity()),
            data.length.minimum(),
        ) {
            return self.fail(
                index.span,
                "B137",
                "dataset 索引需要由本资源的长度条件或有界游标证明",
            );
        }
        Ok(Some(Self::application_element(
            &data,
            &value,
            (value.minimum(), value.maximum()),
        )))
    }
    pub(super) fn application_address(
        &mut self,
        handle: Reg,
        identity: ValueIdentity,
        write: bool,
        span: Span,
    ) -> Result<[Reg; 2]> {
        self.application_handle_available(identity, span)?;
        if let Some(data) = self.env.values().find_map(|binding| match binding {
            Binding::ApplicationData(data)
                if data.handle.proof.known_identity(0) == Some(identity)
                    && data.ownership != Ownership::Moved =>
            {
                Some(data)
            }
            _ => None,
        }) {
            self.application_cpu(data, span)?;
        }
        let handle = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![handle],
            uniform: false,
            constant: None,
        };
        let write = self.constant(Scalar::U32, u32::from(write));
        let value = self
            .application_call("address", vec![handle, write], Some(pointer_type()), span)?
            .unwrap();
        Ok([value.regs[0], value.regs[1]])
    }
    pub(super) fn read_application_place(
        &mut self,
        ty: &Ty,
        handle: Reg,
        identity: ValueIdentity,
        index: Reg,
        stride: u32,
        words: &[u32],
        span: Span,
    ) -> Result<Value> {
        let address = self.application_address(handle, identity, false, span)?;
        let mut regs = Vec::new();
        for (&(scalar, _), &word) in ty.components.iter().zip(words) {
            let dst = self.reg(scalar);
            self.code.push(Inst::ViewLoad {
                dst,
                address,
                index,
                stride,
                word,
            });
            regs.push(dst);
        }
        Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: ty.clone(),
            regs,
            uniform: false,
            constant: None,
        })
    }
    pub(super) fn write_application_place(
        &mut self,
        handle: Reg,
        identity: ValueIdentity,
        index: Reg,
        stride: u32,
        words: &[u32],
        value: Value,
        span: Span,
    ) -> Result<()> {
        if words.iter().copied().collect::<BTreeSet<_>>().len() != words.len() {
            return self.fail(span, "B143", "赋值位置包含重复分量");
        }
        let address = self.application_address(handle, identity, true, span)?;
        let zero = self.constant(Scalar::U32, 0).regs[0];
        for (&word, value) in words.iter().zip(value.regs) {
            self.code.push(Inst::StoreIndexed {
                source: IndexedSource::View {
                    address,
                    index,
                    stride,
                },
                offset: zero,
                word,
                value,
            });
        }
        Ok(())
    }
    pub(super) fn target_call(
        &mut self,
        target: Target,
        call: &Expr,
        span: Span,
    ) -> Result<Option<returns::Returned>> {
        if self.application.is_none() {
            return self.fail(
                span,
                "B266",
                "target 调用由 CPU 主流程发起，元素实例与协作成员不能选择发起位置",
            );
        }
        let (name, explicit, args) = match &call.kind {
            ExprKind::Call(name, args) => (name, None, args),
            ExprKind::Specialize(name, explicit, args) => (name, Some(explicit.as_slice()), args),
            _ => {
                return self.fail(
                    span,
                    "B266",
                    "target 包围一次实际函数调用，例如 target(gpu, step(data))",
                );
            }
        };
        if target == Target::Cpu {
            return self.evaluate_returned(call, None);
        }
        let key = self
            .world
            .resolve(
                name,
                &self.definition.module,
                &self.definition.imports,
                true,
            )
            .ok_or_else(|| Diagnostic {
                source: self.definition.source.clone(),
                span,
                code: "B158",
                message: format!("未知或未导出的函数 {name}"),
            })?;
        if self.world.functions[&key].execution != crate::execution::Requirement::Domain {
            return self.fail(
                span,
                "B266",
                "当前 GPU target 消费整域函数；普通值调用的跨目标返回通道尚未实现",
            );
        }
        let Some(arguments) = self.function_arguments(&key, args, span)? else {
            return Ok(None);
        };
        let instance = self.specialized_arguments(key, &arguments, explicit, span)?;
        self.application_execute(instance, arguments, target, span)
    }
    pub(super) fn application_execute(
        &mut self,
        instance: crate::generics::Instance,
        prepared: Vec<Argument>,
        target: Target,
        span: Span,
    ) -> Result<Option<returns::Returned>> {
        self.application_frame(span)?;
        let callee = &instance.definition;
        if callee.function.target != Target::Compute && callee.function.target != target {
            return self.fail(span, "B266", "函数声明的目标能力与本次调用位置不一致");
        }
        let mut data = Vec::new();
        let mut uniforms = Vec::new();
        let mut held = Vec::new();
        for (parameter, value) in callee.function.params.iter().zip(prepared) {
            if parameter.ty.dataset {
                let value = match value {
                    Argument::ApplicationData(data) => data,
                    Argument::ScopedData { data, owner } => {
                        held.push(owner);
                        data
                    }
                    _ => return self.fail(span, "B224", "整域调用需要当前应用持有的实际 dataset"),
                };
                if parameter.access.writes() && !value.writable {
                    return self.fail(span, "B227", "只读 dataset 不能交给可写函数参数");
                }
                if value.residency != Some(target) {
                    return self.fail(
                        span,
                        "B266",
                        "实际存储与调用位置不一致；先显式 upload／download",
                    );
                }
                if data
                    .iter()
                    .zip(callee.function.params.iter().filter(|p| p.ty.dataset))
                    .any(|(existing, previous): (&Data, &Param)| {
                        existing.handle.identity() == value.handle.identity()
                            && (previous.access.writes() || parameter.access.writes())
                            && !(previous.access == Access::Atomic
                                && parameter.access == Access::Atomic)
                    })
                {
                    return self.fail(span, "B227", "同一存储的重复参数必须同为 read 或同为 atomic");
                }
                if parameter.access.reads() {
                    self.require_application_initialized(&value, span)?;
                }
                data.push(value);
            } else {
                let Argument::Value(value) = value else {
                    return self.fail(span, "B266", "当前整域统一参数需要普通值");
                };
                uniforms.push(value);
            }
        }
        for value in &data {
            self.application_handle_available(value.handle.identity(), span)?;
        }
        let interface = self.prepare_domain(&instance, target)?;
        for (value, buffer) in data.iter().zip(&interface.buffers) {
            self.require_same(&buffer.ty, &value.ty, span)?;
            if buffer.count.is_some() && buffer.count != value.count {
                return self.fail(span, "B167", "调用方没有交付函数需要的数据形状");
            }
            if buffer.access.writes() {
                self.invalidate_application_contents(value.handle.identity());
            }
        }
        for (value, uniform) in uniforms.iter().zip(&interface.uniforms) {
            self.require_same(&uniform.ty, &value.ty, span)?;
        }
        self.initialize_application_domain(&interface, &data);
        let frame = self.application_frame(span)?;
        let mut physical = vec![argument(frame)];
        let mut data = data.into_iter();
        let mut uniforms = uniforms.into_iter();
        for parameter in &callee.function.params {
            if parameter.ty.dataset {
                let value = data.next().unwrap();
                physical.push(argument(
                    self.returned_storage(&returns::Returned::Dataset(value)),
                ));
            } else {
                physical.push(argument(uniforms.next().unwrap()));
            }
        }
        self.call_domain(instance, target, physical);
        for name in held {
            self.finish_scoped_argument(&name, span)?;
        }
        Ok(Some(returns::Returned::Unit))
    }
    pub(super) fn application_parallel(&mut self, statement: &Stmt) -> Result<()> {
        if self.application.is_none() {
            return self.fail(
                statement.span,
                "B131",
                "并行域由主流程或整域函数发起；元素实例中的嵌套域尚未实现",
            );
        }
        let mut names = BTreeSet::new();
        captured_names(std::slice::from_ref(statement), &mut names);
        let mut params = Vec::new();
        let mut args = Vec::new();
        for name in names {
            let Some(binding) = self.env.get(&name).cloned() else {
                continue;
            };
            let expr = Expr {
                kind: ExprKind::Name(name.clone()),
                span: statement.span,
            };
            let (ty, access, arg) = if matches!(binding, Binding::ApplicationData(_)) {
                let data = self.application_reference(&expr)?.unwrap();
                let access = if !data.writable {
                    Access::Read
                } else if self.application_initialized(&data) {
                    Access::InOut
                } else {
                    Access::Write
                };
                let mut ty = data.ty.syntax();
                ty.dataset = true;
                ty.count = data.count.map(Shape::Literal);
                (ty, access, Argument::ApplicationData(data))
            } else {
                let value = self.name_expr(&name, &expr)?;
                (value.ty.syntax(), Access::Value, Argument::Value(value))
            };
            params.push(Param {
                name,
                ty,
                access,
                span: statement.span,
            });
            args.push(arg);
        }
        let name = format!(
            "{}@parallel{}:{:?}",
            self.calls.last().unwrap(),
            statement.span.start,
            params.iter().map(|p| (&p.ty, p.access)).collect::<Vec<_>>()
        );
        let function = Function {
            name: name.clone(),
            types: Vec::new(),
            values: Vec::new(),
            params,
            result: TypeName::named("void"),
            kind: FunctionKind::Value,
            target: Target::Cpu,
            contracts: Vec::new(),
            body: Some(vec![statement.clone()]),
            external: None,
            span: statement.span,
        };
        let definition = Arc::new(Definition {
            function,
            execution: crate::execution::Requirement::Domain,
            source: self.definition.source.clone(),
            module: self.definition.module.clone(),
            imports: self.definition.imports.clone(),
            public: false,
            compiled: None,
        });
        self.application_execute(
            crate::generics::Instance { name, definition },
            args,
            Target::Cpu,
            statement.span,
        )?;
        Ok(())
    }
    pub(super) fn application_cpu(&self, data: &Data, span: Span) -> Result<()> {
        if data.residency != Some(Target::Cpu) {
            return self.fail(
                span,
                "B266",
                "CPU 访问需要 CPU 存储；先显式 download，条件分支也须统一存储位置",
            );
        }
        Ok(())
    }
    pub(super) fn application_transfer(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        self.application_frame(span)?;
        if args.len() != 1 {
            return self.fail(span, "B266", "upload／download 需要一个拥有的 dataset 名称");
        }
        let Some(data) = self.application_reference(&args[0])? else {
            return self.fail(span, "B266", "upload／download 需要应用拥有的 dataset");
        };
        if data.ownership != Ownership::Owned {
            return self.fail(
                span,
                "B227",
                "只有资源所有者可以转移 CPU／GPU 存储，函数借用不能转移",
            );
        }
        let (source, target) = if name == "upload" {
            (Target::Cpu, Target::Gpu)
        } else {
            (Target::Gpu, Target::Cpu)
        };
        if data.residency != Some(source) {
            return self.fail(
                span,
                "B266",
                "存储位置与传输方向不一致；条件分支须分别完成显式传输",
            );
        }
        let value = data.handle.clone();
        self.application_call(name, vec![value], None, span)?;
        let update = &mut |alias: &mut Data| {
            if alias.ownership != Ownership::Moved
                && alias.handle.proof.known_identity(0) == Some(data.handle.identity())
            {
                alias.residency = Some(target);
            }
        };
        for binding in self.env.values_mut() {
            match binding {
                Binding::ApplicationData(alias) => update(alias),
                Binding::OwnedValue(owner) => owner.resources_mut(update),
                _ => {}
            }
        }
        Ok(None)
    }
    pub(super) fn println(&mut self, args: &[Expr], span: Span) -> Result<Option<Value>> {
        if self.program.target != Target::Cpu {
            return self.fail(span, "B260", "println 是 CPU 标准输出操作，要求显式 on cpu");
        }
        if args.len() != 1 {
            return self.fail(span, "B261", "println 需要一个 u32、i32、f32 或 bool 标量");
        }
        let Some(value) = self.expr(&args[0])? else {
            return Ok(None);
        };
        if ![
            Ty::scalar(Scalar::U32),
            Ty::scalar(Scalar::I32),
            Ty::scalar(Scalar::Bool),
            Ty::scalar(Scalar::F32),
        ]
        .contains(&value.ty)
        {
            return self.fail(
                args[0].span,
                "B261",
                "println 当前接受 u32、i32、f32 或 bool 标量",
            );
        }
        let dst = self.reg(Scalar::Bool);
        self.code.push(Inst::Print {
            dst,
            value: value.regs[0],
        });
        Ok(Some(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::Bool),
            regs: vec![dst],
            uniform: false,
            constant: None,
        }))
    }
}

// Only whole binding assignment needs mutable handle/length storage; payload writes do not.
pub(super) fn rebound(body: &[Stmt], name: &str) -> bool {
    fn root(expr: &Expr) -> Option<&str> {
        match &expr.kind {
            ExprKind::Name(name) => Some(name),
            ExprKind::Member(base, _) => root(base),
            _ => None,
        }
    }
    body.iter().any(|s| match &s.kind {
        StmtKind::Assign(lhs, _, _) => root(lhs) == Some(name),
        StmtKind::If(_, yes, no) | StmtKind::Match(_, _, yes, _, no) => {
            rebound(yes, name) || rebound(no, name)
        }
        StmtKind::For(_, _, _, body) | StmtKind::While(_, body) => rebound(body, name),
        _ => false,
    })
}

fn captured_names(body: &[Stmt], names: &mut BTreeSet<String>) {
    fn name(name: &str, bound: &BTreeSet<String>, names: &mut BTreeSet<String>) {
        if !bound.contains(name) {
            names.insert(name.to_owned());
        }
    }
    fn expr(e: &Expr, bound: &BTreeSet<String>, names: &mut BTreeSet<String>) {
        match &e.kind {
            ExprKind::Name(n) => name(n, bound, names),
            ExprKind::Call(_, args)
            | ExprKind::Specialize(_, _, args)
            | ExprKind::Construct(_, args)
            | ExprKind::Cooperate(_, _, args) => {
                for a in args {
                    expr(a, bound, names);
                }
            }
            ExprKind::Target(_, e) | ExprKind::Unary(_, e) | ExprKind::Member(e, _) => {
                expr(e, bound, names)
            }
            ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
                expr(a, bound, names);
                expr(b, bound, names);
            }
            ExprKind::Select(a, b, c) => {
                expr(a, bound, names);
                expr(b, bound, names);
                expr(c, bound, names);
            }
            _ => {}
        }
    }
    fn block(body: &[Stmt], bound: &BTreeSet<String>, names: &mut BTreeSet<String>) {
        let mut bound = bound.clone();
        for s in body {
            match &s.kind {
                StmtKind::Parallel(iterator, domain, body) => {
                    name(&domain.dataset, &bound, names);
                    if let Some(e) = &domain.count {
                        expr(e, &bound, names);
                    }
                    if let Some(e) = &domain.selection {
                        expr(e, &bound, names);
                    }
                    let mut nested = bound.clone();
                    nested.insert(iterator.clone());
                    block(body, &nested, names);
                }
                StmtKind::Let(_, local, e) => {
                    expr(e, &bound, names);
                    bound.insert(local.clone());
                }
                StmtKind::Declare(_, local) => {
                    bound.insert(local.clone());
                }
                StmtKind::Expr(e) | StmtKind::Return(Some(e)) | StmtKind::Seek(_, e) => {
                    expr(e, &bound, names)
                }
                StmtKind::Assign(a, _, b) => {
                    expr(a, &bound, names);
                    expr(b, &bound, names);
                }
                StmtKind::If(e, a, b) => {
                    expr(e, &bound, names);
                    block(a, &bound, names);
                    block(b, &bound, names);
                }
                StmtKind::Match(e, yes_name, yes, no_name, no) => {
                    expr(e, &bound, names);
                    for (binding, branch) in [(yes_name, yes), (no_name, no)] {
                        let mut nested = bound.clone();
                        nested.extend(binding.iter().cloned());
                        block(branch, &nested, names);
                    }
                }
                StmtKind::While(e, body) => {
                    expr(e, &bound, names);
                    block(body, &bound, names);
                }
                StmtKind::For(iterator, a, b, body) => {
                    expr(a, &bound, names);
                    if let Some(b) = b {
                        expr(b, &bound, names);
                    }
                    let mut nested = bound.clone();
                    nested.insert(iterator.clone());
                    block(body, &nested, names);
                }
                _ => {}
            }
        }
    }
    block(body, &BTreeSet::new(), names);
}

crate::module::codec::variants!(Ownership{0=>Owned,1=>Borrowed,2=>Moved});
crate::module::codec::record!(Data {
    handle,
    slots,
    initialization,
    ty,
    length,
    ownership,
    writable,
    count,
    residency
});
