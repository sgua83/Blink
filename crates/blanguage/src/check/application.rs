//! 应用入口是一次普通 CPU 值函数调用，不伪造 dataset 或 parallel 域。
use super::*;
#[derive(Default)]
pub(super) struct Context {
    pub frame: Option<Value>,
    pub resources: crate::application::Resources,
}
#[derive(Clone)]
pub(super) struct Data {
    pub handle: Reg,
    pub ty: Ty,
    pub length: Value,
    pub owned: bool,
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
    let result = builder
        .function_body(d.function.body.as_ref().unwrap())?
        .unwrap();
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
    pub(super) fn release_application_data(&mut self, keep: Option<&BTreeSet<String>>) {
        let Some(frame) = self.application.as_ref().and_then(|app| app.frame.clone()) else {
            return;
        };
        let handles: Vec<_> = self
            .env
            .iter()
            .filter_map(|(name, binding)| match binding {
                Binding::ApplicationData(data)
                    if data.owned && !keep.is_some_and(|names| names.contains(name)) =>
                {
                    Some(data.handle)
                }
                _ => None,
            })
            .collect();
        for handle in handles {
            self.code.push(runtime_call(
                "release",
                vec![
                    argument(frame.clone()),
                    ExternalArgument::Value {
                        ty: Ty::scalar(Scalar::U32),
                        values: vec![handle],
                    },
                ],
                None,
                Vec::new(),
            ));
        }
    }
    pub(super) fn application_dataset(
        &mut self,
        annotation: &Option<TypeName>,
        name: &str,
        expr: &Expr,
        span: Span,
    ) -> Result<()> {
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
        let mut count = if let Some(argument) = explicit.get(1) {
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
        if let Some(annotation) = annotation {
            if !annotation.dataset {
                return self.fail(
                    span,
                    "B124",
                    "dataset 不能初始化普通值；使用 dataset<T> 声明或 let 推导资源类型",
                );
            }
            let expected = self.world.ty(
                annotation,
                &definition.source,
                span,
                &definition.module,
                &definition.imports,
            )?;
            self.require_same(&expected, &ty, span)?;
            let annotated_count = self.world.shape(
                &annotation.count,
                &definition.source,
                span,
                &definition.module,
                &definition.imports,
            )?;
            if annotated_count.is_some() && count.is_some() && annotated_count != count {
                return self.fail(span, "B167", "dataset 声明与构造的静态形状不一致");
            }
            count = annotated_count.or(count);
        }
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
            .unwrap()
            .regs[0];
        length.proof = bounds::Proof::capture(&length)
            .with_domains(Domains::from([IndexDomain::ApplicationLength(handle)]));
        self.env.insert(
            name.into(),
            Binding::ApplicationData(Data {
                handle,
                ty,
                length,
                count,
                owned: true,
                writable: true,
                residency: Some(Target::Cpu),
            }),
        );
        Ok(())
    }
    pub(super) fn application_element(data: &Data, index: Reg, index_range: (u32, u32)) -> Place {
        Place::ApplicationElement {
            ty: data.ty.clone(),
            handle: data.handle,
            index,
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
            IndexDomain::ApplicationData(data.handle),
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
            value.regs[0],
            (value.minimum(), value.maximum()),
        ))
    }
    pub(super) fn application_address(
        &mut self,
        handle: Reg,
        write: bool,
        span: Span,
    ) -> Result<[Reg; 2]> {
        if let Some(data) = self.env.values().find_map(|binding| match binding {
            Binding::ApplicationData(data) if data.handle == handle => Some(data),
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
        index: Reg,
        stride: u32,
        words: &[u32],
        span: Span,
    ) -> Result<Value> {
        let address = self.application_address(handle, false, span)?;
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
        index: Reg,
        stride: u32,
        words: &[u32],
        value: Value,
        span: Span,
    ) -> Result<()> {
        if words.iter().copied().collect::<BTreeSet<_>>().len() != words.len() {
            return self.fail(span, "B143", "赋值位置包含重复分量");
        }
        let address = self.application_address(handle, true, span)?;
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
                let Some(Binding::ApplicationData(value)) = self.env.get(name).cloned() else {
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
                    .any(|existing: &Data| existing.handle == value.handle)
                {
                    return self.fail(
                        arg.span,
                        "B227",
                        "同一独占 dataset 不能在一次 program 调用中重复绑定",
                    );
                }
                actuals.push((value.ty.clone(), value.count));
                data.push(value);
            } else {
                let value = self.expr(arg)?;
                actuals.push((value.ty.clone(), None));
                uniforms.push(value);
            }
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
        let mut params = data
            .into_iter()
            .map(|data| Value {
                proof: bounds::Proof::default(),
                literal: false,
                ty: Ty::scalar(Scalar::U32),
                regs: vec![data.handle],
                uniform: false,
                constant: None,
            })
            .collect::<Vec<_>>();
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
        let Some(Binding::ApplicationData(data)) = self.env.get(binding).cloned() else {
            return self.fail(span, "B266", "upload／download 需要应用拥有的 dataset");
        };
        if !data.owned {
            return self.fail(
                span,
                "B227",
                "只有资源所有者可以转移 CPU／GPU 存储，read 借用不能转移",
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
        let value = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![data.handle],
            uniform: false,
            constant: None,
        };
        self.application_call(name, vec![value], None, span)?;
        for value in self.env.values_mut() {
            if let Binding::ApplicationData(alias) = value
                && alias.handle == data.handle
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
