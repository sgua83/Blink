//! 应用入口是一次普通 CPU 值函数调用，不伪造 dataset 或 parallel 域。
use super::*;
#[derive(Default)]
pub(super) struct Context {
    pub frame: Option<Value>,
    pub resources: crate::application::Resources,
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
fn argument(value: Value) -> ExternalArgument {
    ExternalArgument::Value {
        ty: value.ty,
        values: value.regs,
    }
}
fn runtime_call(
    name: &str,
    arguments: Vec<ExternalArgument>,
    result: Option<Ty>,
    values: Vec<Reg>,
) -> Inst {
    Inst::ExternalCall(ExternalCall {
        function: ExternalName {
            library: crate::application::RUNTIME_LIBRARY.into(),
            symbol: format!("blink_{name}"),
        },
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
    let result = if d.function.result.name == "void" {
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
    builder.calls.push(instance.name.clone());
    let requirements = contracts::resolve(&builder, d, &builder.env)?;
    contracts::assume(&mut builder.env, &requirements);
    let result = builder.function_body(d.function.body.as_ref().unwrap())?;
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
    let resources = live_resources(&mut body, app);
    Ok(crate::application::Application {
        name: instance.name.clone(),
        body,
        resources,
    })
}
fn runtime_calls(code: &mut [Inst], visitor: &mut impl FnMut(&mut ExternalCall)) {
    for instruction in code {
        match instruction {
            Inst::ExternalCall(call)
                if call.function.library == crate::application::RUNTIME_LIBRARY =>
            {
                visitor(call)
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
fn live_resources(body: &mut ValueCode, app: Context) -> Option<crate::application::Resources> {
    let frame = app.frame?;
    let mut names = BTreeSet::new();
    runtime_calls(&mut body.code, &mut |call| {
        names.insert(call.function.symbol.clone());
    });
    if names.is_empty() {
        return None;
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
    let types = retain(app.resources.types, "allocate", &names, &mut renames);
    let programs = retain(app.resources.programs, "run", &names, &mut renames);
    runtime_calls(&mut body.code, &mut |call| {
        if let Some(name) = renames.get(&call.function.symbol) {
            call.function.symbol = name.clone();
        }
    });
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
    Some(crate::application::Resources { types, programs })
}
impl Builder<'_> {
    // 独立检查 CPU 函数时，参数提供符号身份和真实接口形状；不创建物理存储。
    // 实际内联交接或借用调用方的 ApplicationData，物理资源仍由原帧拥有。
    pub(super) fn application_parameter(&mut self, binding: usize, access: Access) -> Result<Data> {
        if access == Access::Value {
            self.application_frame(self.definition.function.span)?;
        }
        let reg = self.reg(Scalar::U32);
        let handle = self.application_handle(reg);
        let view = self.dataset_view(binding);
        let mut length = self.data_length(&view);
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
    fn initialize_application_program(&mut self, program: &Program, data: &[Data]) {
        let same_length = |a: &Data, b: &Data| {
            a.length.identity() == b.length.identity()
                || a.length.minimum() == a.length.maximum()
                    && a.length.minimum() == b.length.minimum()
                    && a.length.maximum() == b.length.maximum()
        };
        for stage in &program.stages {
            match &stage.operation {
                Operation::Map => {
                    if stage.prefix.is_none() && stage.selection.is_none() {
                        for binding in &stage.writes {
                            if program.buffers[*binding].access == Access::Write
                                && same_length(&data[*binding], &data[stage.domain])
                            {
                                self.initialize_application(&data[*binding]);
                            }
                        }
                    }
                }
                Operation::Reduce(values) => self.initialize_application_reduction(
                    &data[values.fields.target],
                    values.fields.target_word,
                    &values.ty,
                ),
                Operation::CustomReduce(values) => self.initialize_application_reduction(
                    &data[values.fields.target],
                    values.fields.target_word,
                    &values.ty,
                ),
                Operation::ScanExclusive(values) => {
                    if stage.prefix.is_none()
                        && same_length(&data[values.values.target], &data[values.values.source])
                    {
                        self.initialize_application_value(
                            &data[values.values.target],
                            values.values.target_word,
                            &values.ty,
                        );
                    }
                }
                Operation::CustomScan(values) => {
                    if stage.prefix.is_none()
                        && same_length(
                            &data[values.values.fields.target],
                            &data[values.values.fields.source],
                        )
                    {
                        self.initialize_application_value(
                            &data[values.values.fields.target],
                            values.values.fields.target_word,
                            &values.values.ty,
                        );
                    }
                }
                Operation::StableSort(values) => {
                    if stage.prefix.is_none()
                        && same_length(&data[values.target], &data[values.source])
                    {
                        self.initialize_application(&data[values.target]);
                    }
                }
                Operation::Compact(values) => self.initialize_application_reduction(
                    &data[values.count],
                    values.count_word,
                    &Ty::scalar(Scalar::U32),
                ),
                Operation::SegmentedReduce(values) => self.initialize_application_reduction(
                    &data[values.count.binding],
                    values.count.word,
                    &Ty::scalar(Scalar::U32),
                ),
                Operation::CustomSegmentedReduce(values) => self.initialize_application_reduction(
                    &data[values.count.binding],
                    values.count.word,
                    &Ty::scalar(Scalar::U32),
                ),
            }
        }
    }
    fn initialize_application_value(&mut self, data: &Data, first: u32, ty: &Ty) {
        if let Some(scope) = data.initialization {
            self.initialized.extend(
                ty.components
                    .iter()
                    .map(|(_, offset)| Position::Parameter(scope, (first + offset / 4) as usize)),
            );
        }
    }
    fn initialize_application_reduction(&mut self, data: &Data, first: u32, ty: &Ty) {
        if let Some(scope) = data.initialization {
            self.initialized
                .extend(ty.components.iter().map(|(_, offset)| {
                    Position::ApplicationElement(
                        scope,
                        ownership::ApplicationIndex::Constant(0),
                        first + offset / 4,
                    )
                }));
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
        let Some(scope) = data.initialization else {
            return true;
        };
        if self
            .initialized
            .contains(&Position::Parameter(scope, word as usize))
        {
            return true;
        }
        let length = data.length.minimum();
        data.length.maximum() == length && self.initialized.iter().filter(|position| matches!(position,
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
    fn application_frame(&mut self, span: Span) -> Result<Value> {
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
                ExternalArgument::Value {
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
        let data: Vec<_> = environment
            .iter()
            .filter_map(|(name, binding)| {
                if let (Binding::ApplicationData(data), Some(Binding::ApplicationData(after))) =
                    (binding, other.get(name))
                    && data.ownership == Ownership::Owned
                    && after.ownership == Ownership::Moved
                {
                    Some(data.clone())
                } else {
                    None
                }
            })
            .collect();
        data.iter()
            .flat_map(|data| self.application_drop(data))
            .collect()
    }
    fn application_drop(&mut self, data: &Data) -> Vec<Inst> {
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
            .filter_map(|binding| match binding {
                Binding::ApplicationData(data)
                    if data.handle.proof.known_identity(0) == Some(identity) =>
                {
                    Some(data)
                }
                _ => None,
            })
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
    fn application_movable(&self, data: &Data, span: Span) -> Result<()> {
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
            if let Some(Binding::ApplicationData(source)) = self.env.get_mut(name) {
                source.handle.proof = data.handle.proof.clone();
                source.length.proof = data.length.proof.clone();
            }
        }
        Ok(Some(data))
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
        let data: Vec<_> = self
            .env
            .iter()
            .filter_map(|(name, binding)| match binding {
                Binding::ApplicationData(data)
                    if data.ownership == Ownership::Owned
                        && !keep.is_some_and(|names| names.contains(name)) =>
                {
                    Some(data.clone())
                }
                _ => None,
            })
            .collect();
        for data in data {
            let code = self.application_drop(&data);
            self.code.extend(code);
        }
    }

    pub(super) fn application_data_expression(&self, expr: &Expr) -> bool {
        let name = match &expr.kind {
            ExprKind::Name(name) => {
                return matches!(self.env.get(name), Some(Binding::ApplicationData(_)));
            }
            ExprKind::Specialize(name, _, _) if name == "dataset" => return true,
            ExprKind::Call(name, _) | ExprKind::Specialize(name, _, _) => name,
            _ => return false,
        };
        self.world
            .resolve(
                name,
                &self.definition.module,
                &self.definition.imports,
                true,
            )
            .is_some_and(|key| self.world.functions[&key].function.result.dataset)
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
    pub(super) fn application_dataset(
        &mut self,
        annotation: &Option<TypeName>,
        name: &str,
        expr: &Expr,
        span: Span,
    ) -> Result<()> {
        let mut data = self.application_resource(expr, span)?;
        if let Some(annotation) = annotation {
            if !annotation.dataset {
                return self.fail(span, "B124", "dataset 资源不能初始化普通值");
            }
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
            self.application_data_type(&mut data, &ty, count, span)?;
        }
        let mutable = rebound(self.definition.function.body.as_ref().unwrap(), name);
        if mutable {
            if let Some(annotation) = annotation {
                data.count = self.world.shape(
                    &annotation.count,
                    &self.definition.source,
                    span,
                    &self.definition.module,
                    &self.definition.imports,
                )?;
            }
        }
        let data = self.application_bind(data, mutable);
        self.env.insert(name.into(), Binding::ApplicationData(data));
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
        let mut value = self.application_resource(rhs, span)?;
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
    pub(super) fn application_resource(&mut self, expr: &Expr, span: Span) -> Result<Data> {
        self.application_frame(span)?;
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
            return Ok(data);
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
        let arguments = self.function_arguments(&key, args, span)?;
        let instance = self.specialized_arguments(key, &arguments, explicit, span)?;
        let Some(returns::Returned::Dataset(data)) =
            self.inline_instance(instance, arguments, span)?
        else {
            return self.fail(span, "B224", "资源函数没有交付声明的 dataset 所有权");
        };
        Ok(data)
    }
    fn create_application_dataset(&mut self, expr: &Expr, span: Span) -> Result<Data> {
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
        if ty.contains_views() {
            return self.fail(
                span,
                "B266",
                "动态 dataset 的元素需要普通值；view 资源借用不能由零初始化存储创建",
            );
        }
        let mut length = self.expr(&args[0])?;
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
        Ok(Data {
            handle,
            slots: None,
            initialization: None,
            ty,
            length,
            count,
            ownership: Ownership::Owned,
            writable: true,
            residency: Some(Target::Cpu),
        })
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
    pub(super) fn application_index(&mut self, data: Data, index: &Expr) -> Result<Place> {
        self.application_cpu(&data, index.span)?;
        let value = self.expr(index)?;
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
        Ok(Self::application_element(
            &data,
            &value,
            (value.minimum(), value.maximum()),
        ))
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
    pub(super) fn application_run(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        self.application_frame(span)?;
        if args.len() != 1 {
            return self.fail(span, "B266", "run_cpu／run_gpu 需要一个 program 调用");
        }
        let (callee, explicit, arguments) = match &args[0].kind {
            ExprKind::Call(name, args) => (name, None, args),
            ExprKind::Specialize(name, explicit, args) => (name, Some(explicit.as_slice()), args),
            _ => {
                return self.fail(
                    span,
                    "B266",
                    "执行位置包围实际 program 调用，例如 run_cpu(step(data))",
                );
            }
        };
        let definition = self.definition.clone();
        let Some(key) = self
            .world
            .resolve(callee, &definition.module, &definition.imports, true)
        else {
            return self.fail(span, "B158", format!("未知或未导出的 program {callee}"));
        };
        let callee = self.world.functions[&key].clone();
        if callee.function.kind != FunctionKind::Program
            || arguments.len() != callee.function.params.len()
        {
            return self.fail(span, "B266", "run_cpu／run_gpu 需要参数完整的 program 调用");
        }
        let target = if name == "run_cpu" {
            Target::Cpu
        } else {
            Target::Gpu
        };
        if callee.function.target != Target::Compute && callee.function.target != target {
            return self.fail(span, "B266", "program 的执行目标与调用位置不一致");
        }
        let mut data = Vec::new();
        let mut uniforms = Vec::new();
        let mut actuals = Vec::new();
        for (parameter, arg) in callee.function.params.iter().zip(arguments) {
            if parameter.ty.dataset {
                let ExprKind::Name(name) = &arg.kind else {
                    return self.fail(arg.span, "B224", "program 借用实际 dataset 资源名称");
                };
                let Some(value) = self.application_value(name, arg.span)? else {
                    return self.fail(arg.span, "B224", "program 需要应用持有的动态 dataset");
                };
                if parameter.access.writes() && !value.writable {
                    return self.fail(arg.span, "B227", "只读 dataset 不能交给可写 program 参数");
                }
                if value.residency != Some(target) {
                    return self.fail(
                        arg.span,
                        "B266",
                        "program 的物理存储与执行位置不一致；显式 upload／download 后再执行",
                    );
                }
                if data
                    .iter()
                    .any(|existing: &Data| existing.handle.identity() == value.handle.identity())
                {
                    return self.fail(
                        arg.span,
                        "B227",
                        "同一独占 dataset 不能在一次 program 调用中重复绑定",
                    );
                }
                if parameter.access.reads() {
                    self.require_application_initialized(&value, arg.span)?;
                }
                actuals.push((value.ty.clone(), value.count));
                data.push(value);
            } else {
                let value = self.expr(arg)?;
                actuals.push((value.ty.clone(), None));
                uniforms.push(value);
            }
        }
        for value in &data {
            self.application_handle_available(value.handle.identity(), span)?;
        }
        let types = actuals
            .iter()
            .map(|(ty, count)| generics::Actual {
                ty: Some(ty),
                count: *count,
            })
            .collect::<Vec<_>>();
        let instance = generics::specialize(self.world, &key, &types, explicit, &definition, span)?;
        let program = super::compile_program(self.world, &instance)?;
        for (value, buffer) in data.iter().zip(&program.buffers) {
            self.require_same(&buffer.ty, &value.ty, span)?;
            if buffer.count.is_some() && buffer.count != value.count {
                return self.fail(span, "B167", "调用方没有声明 program 需要的数据形状");
            }
        }
        for (value, uniform) in uniforms.iter().zip(&program.uniforms) {
            self.require_same(&uniform.ty, &value.ty, span)?;
        }
        self.initialize_application_program(&program, &data);
        let app = self.application.as_mut().unwrap();
        let index = app
            .resources
            .programs
            .iter()
            .position(|(at, existing)| *at == target && existing.name == program.name)
            .unwrap_or_else(|| {
                let index = app.resources.programs.len();
                app.resources.programs.push((target, program));
                index
            });
        let mut params = data.into_iter().map(|data| data.handle).collect::<Vec<_>>();
        params.extend(uniforms.into_iter().flat_map(|value| {
            value
                .ty
                .components
                .into_iter()
                .zip(value.regs)
                .map(|((scalar, _), reg)| Value {
                    proof: bounds::Proof::default(),
                    literal: false,
                    ty: Ty::scalar(scalar),
                    regs: vec![reg],
                    uniform: false,
                    constant: None,
                })
        }));
        self.application_call(&format!("run_{index}"), params, None, span)
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
        let ExprKind::Name(binding) = &args[0].kind else {
            return self.fail(span, "B266", "upload／download 转移实际 dataset 存储");
        };
        let Some(data) = self.application_value(binding, span)? else {
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
        for value in self.env.values_mut() {
            if let Binding::ApplicationData(alias) = value
                && alias.ownership != Ownership::Moved
                && alias.handle.proof.known_identity(0) == Some(data.handle.identity())
            {
                alias.residency = Some(target);
            }
        }
        Ok(None)
    }
    pub(super) fn println(&mut self, args: &[Expr], span: Span) -> Result<Value> {
        if self.program.target != Target::Cpu {
            return self.fail(span, "B260", "println 是 CPU 标准输出操作，要求显式 on cpu");
        }
        if args.len() != 1 {
            return self.fail(span, "B261", "println 需要一个 u32、i32、f32 或 bool 标量");
        }
        let value = self.expr(&args[0])?;
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
        Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::Bool),
            regs: vec![dst],
            uniform: false,
            constant: None,
        })
    }
}

// Only whole binding assignment needs mutable handle/length storage; payload writes do not.
pub(super) fn rebound(body: &[Stmt], name: &str) -> bool {
    body.iter().any(|statement| match &statement.kind {
        StmtKind::Assign(
            Expr {
                kind: ExprKind::Name(target),
                ..
            },
            _,
            _,
        ) => target == name,
        StmtKind::If(_, yes, no) => rebound(yes, name) || rebound(no, name),
        StmtKind::For(_, _, _, body) | StmtKind::While(_, body) => rebound(body, name),
        _ => false,
    })
}
