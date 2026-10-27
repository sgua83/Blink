//! 函数借用现有数据集、独占／原子位置或 tile；静态绑定实际来源与访问模式。
use super::*;
pub(super) enum Argument {
    BorrowedData(application::DataReference),
    Acceleration(usize),
    Texture(texture::TextureReference),
    Value(Value),
    Dataset(DatasetView),
    ApplicationData(application::Data),
    ScopedData {
        data: application::Data,
        owner: String,
    },
    OwnedValue(owned_values::OwnedValue),
    Tile(TileView),
    Atomic(Place),
    Exclusive(Place),
}
impl Argument {
    fn overlaps(&self, other: &Self) -> bool {
        if let (Self::Exclusive(a) | Self::Atomic(a), Self::Exclusive(b) | Self::Atomic(b)) =
            (self, other)
        {
            return a.overlaps(b);
        }
        let handles = |arg: &Self| match arg {
            Self::BorrowedData(reference) => reference.sources.clone(),
            Self::ApplicationData(d) | Self::ScopedData { data: d, .. } => {
                vec![d.handle.identity()]
            }
            Self::OwnedValue(r) => r.handles(),
            Self::Exclusive(p) | Self::Atomic(p) => match p.storage() {
                Place::ApplicationElement { sources, .. } => sources.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        let a = handles(self);
        let b = handles(other);
        a.iter().any(|h| b.contains(h))
    }
}
pub(super) fn parameter_type(world: &World, param: &Param, definition: &Definition) -> Result<Ty> {
    let mut name = param.ty.clone();
    name.tile = false;
    let ty = world.ty(
        &name,
        &definition.source,
        param.span,
        &definition.module,
        &definition.imports,
    )?;
    if ty.owns_resources()
        && (param.ty.dataset
            || param.ty.tile
            || param.access != Access::Value
            || definition.function.kind != FunctionKind::Value
            || definition.function.target != Target::Cpu)
    {
        return definition.source.fail(
            param.span,
            "B224",
            "拥有结果通过普通CPU函数的值参数交接；通用拥有聚合、借用与并行元素尚未实现",
        );
    }
    if matches!(param.access, Access::InOut | Access::Write)
        && !param.ty.dataset
        && !param.ty.tile
        && ty.components.is_empty()
    {
        return definition
            .source
            .fail(param.span, "B227", "write／inout 参数需要非空值类型");
    }
    if param.ty.tile && ty.contains_views() {
        return definition.source.fail(
            param.span,
            "B207",
            "tile 借用只能包含普通值，不能包含资源 view",
        );
    }
    if param.access == Access::Atomic
        && !param.ty.tile
        && (ty.contains_views() || ty.components.is_empty())
    {
        return definition.source.fail(
            param.span,
            "B223",
            "原子借用参数需要非空普通值类型，不能包含资源 view",
        );
    }
    Ok(ty)
}
impl<'a> Builder<'a> {
    pub(super) fn function_arguments(
        &mut self,
        key: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Vec<Argument>>> {
        let callee = &self.world.functions[key];
        if args.len() != callee.function.params.len() {
            return self.fail(span, "B162", "函数参数数量不匹配");
        }
        let mut arguments = Vec::new();
        let mut held = Vec::new();
        for (param, arg) in callee.function.params.iter().zip(args) {
            arguments.push(
                if callee.function.external.is_some() && param.access == Access::Read {
                    {
                        let Some(place) = self.external_read_place(arg)? else {
                            self.abandon_arguments(&held);
                            return Ok(None);
                        };
                        Argument::Exclusive(place)
                    }
                } else if param.ty.is_acceleration() {
                    let ExprKind::Name(name) = &arg.kind else {
                        return self.fail(arg.span, "B183", "acceleration 借用需要已有资源名称");
                    };
                    let Some(Binding::Acceleration(index)) = self.env.get(name) else {
                        return self.fail(arg.span, "B183", "acceleration 借用需要已有资源名称");
                    };
                    Argument::Acceleration(*index)
                } else if param.ty.is_texture_resource() {
                    {
                        let Some(texture) = self.texture_argument(arg)? else {
                            self.abandon_arguments(&held);
                            return Ok(None);
                        };
                        Argument::Texture(texture)
                    }
                } else if param.ty.dataset && param.access == Access::Value {
                    let returned = self.evaluate_returned(arg, None)?;
                    if !self.flow.continues() {
                        self.abandon_arguments(&held);
                        return Ok(None);
                    }
                    let Some(returns::Returned::Dataset(data)) = returned else {
                        return self.fail(arg.span, "B224", "dataset 值参数需要拥有的数据集");
                    };
                    Argument::ApplicationData(data)
                } else if param.ty.dataset {
                    if let Some(data) = self.application_reference(arg)? {
                        Argument::BorrowedData(data)
                    } else if let ExprKind::Name(name) = &arg.kind
                        && let Some(Binding::Data(view)) = self.env.get(name)
                    {
                        Argument::Dataset(view.clone())
                    } else {
                        let argument = self.scoped_argument(arg)?;
                        if !self.flow.continues() {
                            self.abandon_arguments(&held);
                            return Ok(None);
                        }
                        argument.unwrap()
                    }
                } else if param.ty.tile {
                    Argument::Tile(self.tile_view(arg)?)
                } else if param.access == Access::Atomic {
                    {
                        let Some(place) = self.atomic_place(arg)? else {
                            self.abandon_arguments(&held);
                            return Ok(None);
                        };
                        Argument::Atomic(place)
                    }
                } else if matches!(param.access, Access::InOut | Access::Write) {
                    let Some(place) = self.mutable_place(arg, param.access)? else {
                        self.abandon_arguments(&held);
                        return Ok(None);
                    };
                    Argument::Exclusive(place)
                } else {
                    let expected =
                        if callee.function.types.is_empty() && callee.function.values.is_empty() {
                            Some(self.world.ty(
                                &param.ty,
                                &callee.source,
                                param.span,
                                &callee.module,
                                &callee.imports,
                            )?)
                        } else {
                            None
                        };
                    let returned = self.evaluate_returned(arg, expected.as_ref())?;
                    if !self.flow.continues() {
                        self.abandon_arguments(&held);
                        return Ok(None);
                    }
                    match returned {
                        Some(returns::Returned::Value(v)) => Argument::Value(v),
                        Some(returns::Returned::OwnedValue(v)) => Argument::OwnedValue(v),
                        Some(returns::Returned::Dataset(v)) => Argument::ApplicationData(v),
                        _ => return self.fail(arg.span, "B228", "函数实参需要值或拥有资源"),
                    }
                },
            );
            if !self.flow.continues() {
                self.abandon_arguments(&held);
                return Ok(None);
            }
            if let Some(name) = self.hold_argument(arguments.last().unwrap(), param.access) {
                held.push((arguments.len() - 1, name));
            }
        }
        for (param, argument) in callee.function.params.iter().zip(&arguments) {
            if let Argument::BorrowedData(reference) = argument {
                for source in &reference.sources {
                    self.application_handle_available(*source, span)?;
                }
            }
            if let Argument::Exclusive(place) | Argument::Atomic(place) = argument
                && let Place::ApplicationElement { sources, .. } = place.storage()
            {
                for source in sources {
                    self.application_handle_available(*source, span)?;
                }
            }
            let handle = match argument {
                Argument::ApplicationData(data) | Argument::ScopedData { data, .. }
                    if param.access != Access::Value =>
                {
                    Some(data.handle.identity())
                }
                Argument::Exclusive(place) | Argument::Atomic(place) => match place.storage() {
                    Place::ApplicationElement { identity, .. } => Some(*identity),
                    _ => None,
                },
                _ => None,
            };
            if let Some(handle) = handle {
                self.application_handle_available(handle, span)?;
            }
        }
        for (index, (param, argument)) in callee.function.params.iter().zip(&arguments).enumerate()
        {
            for (other, before) in callee.function.params.iter().zip(&arguments).take(index) {
                if (matches!(param.access, Access::Write | Access::InOut)
                    || matches!(other.access, Access::Write | Access::InOut)
                    || (param.ty.dataset && param.access == Access::Value)
                    || (other.ty.dataset && other.access == Access::Value)
                    || (param.ty.dataset
                        && other.ty.dataset
                        && ((param.access == Access::Atomic && other.access != Access::Atomic)
                            || (other.access == Access::Atomic && param.access != Access::Atomic)))
                    || matches!(argument, Argument::OwnedValue(_))
                    || matches!(before, Argument::OwnedValue(_)))
                    && argument.overlaps(before)
                {
                    return self.fail(
                        args[index].span,
                        "B227",
                        "同一次调用的独占借用不能重叠，也不能与同一存储的只读或原子借用重叠",
                    );
                }
            }
        }
        self.finish_arguments(&held, &mut arguments);
        Ok(Some(arguments))
    }
    pub(super) fn invoke_instance(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<Value>> {
        let result = self.invoke_returned(instance, args, span)?;
        self.ordinary_return(result, span)
    }
    pub(super) fn invoke_returned(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<returns::Returned>> {
        if instance.definition.function.external.is_some() {
            return self
                .external_call(&instance.definition, args, span)
                .map(|v| v.map(returns::Returned::Value));
        }
        if instance.definition.compiled.is_some()
            && instance.definition.execution != crate::execution::Requirement::Domain
        {
            if self.application.is_none() {
                return self.fail(span, "B270", "该已编译普通原生模块入口需要CPU应用调用位置");
            }
            return self.native_function(instance, args, span);
        }
        if instance.definition.execution == crate::execution::Requirement::Domain {
            if self.application.is_some() {
                if instance.definition.function.target == Target::Cpu
                    && !super::kernel_interface(self.world, &instance.definition)?
                {
                    return self.inline_instance(instance, args, span);
                }
                return self.application_execute(instance, args, Target::Cpu, span);
            }
            return self.fail(
                span,
                "B159",
                "整域执行需要发起位置，不能由元素实例或协作成员发起",
            );
        }
        match instance.definition.function.kind {
            FunctionKind::Cooperative => {
                self.fail(span, "B208", "cooperative 函数需要显式 cooperate 根块调用")
            }
            FunctionKind::Value => self.inline_instance(instance, args, span),
            FunctionKind::Constant => {
                self.fail(span, "B234", "const 通过名称读取，不能作为函数调用")
            }
        }
    }
    pub(super) fn cooperate(
        &mut self,
        name: &str,
        explicit: Option<&[GenericArgument]>,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        if !self.is_cooperative_root() {
            return self.fail(span, "B208", "cooperate 需要 parallel 或 cooperative 函数的根块声明，不能位于分支／循环或普通值函数中");
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
                message: format!("未知协作函数 {name}"),
            })?;
        let callee = &self.world.functions[&key];
        if callee.function.kind != FunctionKind::Cooperative {
            return self.fail(span, "B208", "cooperate 只能调用声明为 cooperative 的函数");
        }
        if args.len() != callee.function.params.len() {
            return self.fail(span, "B162", "cooperate 参数数量不匹配");
        }
        let Some(arguments) = self.function_arguments(&key, args, span)? else {
            return Ok(None);
        };
        let instance = self.specialized_arguments(key, &arguments, explicit, span)?;
        let result = self.inline_instance(instance, arguments, span)?;
        self.ordinary_return(result, span)
    }
    pub(super) fn inline_function(
        &mut self,
        key: String,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<Value>> {
        if args.len() != self.world.functions[&key].function.params.len() {
            return self.fail(span, "B162", "函数参数数量不匹配");
        }
        let instance = self.specialized_function(key, &args, span)?;
        let result = self.inline_instance(instance, args, span)?;
        self.ordinary_return(result, span)
    }
    pub(super) fn inline_instance(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<returns::Returned>> {
        if self.application.is_some()
            && instance.definition.function.kind == FunctionKind::Value
            && instance.definition.function.target != Target::Gpu
        {
            return self.native_function(instance, args, span);
        }
        let mut held = Vec::new();
        let args = args
            .into_iter()
            .map(|arg| match arg {
                Argument::BorrowedData(reference) => Argument::ApplicationData(reference.data),
                Argument::ScopedData { data, owner } => {
                    held.push(owner);
                    Argument::ApplicationData(data)
                }
                other => other,
            })
            .collect();
        let result = self.inline_body(instance, args, span)?;
        if self.flow.continues() {
            for name in held {
                self.finish_scoped_argument(&name, span)?;
            }
        }
        Ok(result)
    }
    fn inline_body(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<returns::Returned>> {
        let key = instance.name;
        let callee = &instance.definition;
        if self.calls.contains(&key) {
            return self.fail(span, "B160", "当前设备函数不允许递归调用");
        }
        if callee.function.target != Target::Compute
            && callee.function.target != self.program.target
        {
            return self.fail(span, "B161", "函数设备能力与当前程序不相容");
        }
        let mut env = BTreeMap::new();
        let mut borrowed = Vec::new();
        let mut dataset_outputs = Vec::new();
        for (param, arg) in callee.function.params.iter().zip(args) {
            if param.ty.is_acceleration() {
                let Argument::Acceleration(index) = arg else {
                    return self.fail(span, "B183", "read acceleration 参数需要加速结构资源");
                };
                env.insert(param.name.clone(), Binding::Acceleration(index));
                continue;
            }
            if param.ty.is_texture_resource() {
                let Argument::Texture(index) = arg else {
                    return self.fail(span, "B239", "read texture2d 参数需要纹理资源");
                };
                if param.access != Access::Read {
                    return self.fail(span, "B239", "纹理只能只读借用");
                }
                if index.shape(self.program)
                    != texture::shape(self.world, &param.ty, callee, param.span)?
                {
                    return self.fail(span, "B239", "纹理资源数组形状与借用参数不一致");
                }
                env.insert(param.name.clone(), Binding::Texture(index));
                continue;
            }
            let ty = parameter_type(self.world, param, callee)?;
            if let Argument::OwnedValue(value) = arg {
                if param.access != Access::Value || !ty.owns_resources() {
                    return self.fail(
                        span,
                        "B224",
                        "拥有结果参数按值转移，不能冒充普通值或位置借用",
                    );
                }
                self.require_same(&ty, &value.value.ty, span)?;
                let mutable =
                    application::rebound(callee.function.body.as_ref().unwrap(), &param.name)
                        || !value.value.ty.has_variants();
                let value = self.owned_value_binding(value, mutable);
                env.insert(param.name.clone(), Binding::OwnedValue(value));
                continue;
            }
            let binding = match (param.ty.dataset, param.ty.tile, arg) {
                (true, false, Argument::ApplicationData(mut data)) => {
                    self.application_cpu(&data, span)?;
                    self.require_same(&ty, &data.ty, span)?;
                    if !matches!(
                        param.access,
                        Access::Read | Access::InOut | Access::Write | Access::Value
                    ) {
                        return self.fail(
                            span,
                            "B224",
                            "CPU 应用函数按值取得 dataset 所有权或按 read／write／inout 借用；原子借用属于实际并行域",
                        );
                    }
                    if matches!(param.access, Access::InOut | Access::Write) && !data.writable {
                        return self.fail(span, "B227", "只读 dataset 借用不能提升为 write／inout");
                    }
                    if param.access == Access::Write {
                        let scope = self.application_output_scope();
                        dataset_outputs.push((data.clone(), scope));
                        data.initialization = Some(Position::Parameter(scope,0));
                    } else {
                        self.require_application_initialized(&data, span)?;
                    }
                    if param.ty.count.is_some()
                        && self.world.shape(
                            &param.ty.count,
                            &callee.source,
                            param.span,
                            &callee.module,
                            &callee.imports,
                        )? != data.count
                    {
                        return self.fail(span, "B167", "调用方没有声明函数需要的数据形状");
                    }
                    data.ownership = if param.access == Access::Value {
                        application::Ownership::Owned
                    } else {
                        application::Ownership::Borrowed
                    };
                    data.writable =
                        matches!(param.access, Access::Value | Access::InOut | Access::Write);
                    if matches!(param.access, Access::InOut | Access::Write) {
                        self.invalidate_application_contents(data.handle.identity());
                    }
                    let mutable =
                        application::rebound(callee.function.body.as_ref().unwrap(), &param.name);
                    Binding::ApplicationData(self.application_bind(data, mutable))
                }
                (true, false, Argument::Dataset(view)) => {
                    let binding = view.binding;
                    self.require_same(&ty, &self.program.buffers[binding].ty, span)?;
                    if !matches!(param.access, Access::Read | Access::Atomic)
                        || param.access != self.view_access[&binding]
                    {
                        return self.fail(
                            span,
                            "B224",
                            "并行实例只能转借已有 read／atomic dataset；整份 inout 借用需要 CPU 应用的独占资源",
                        );
                    }
                    if param.ty.count.is_some()
                        && self.world.shape(
                            &param.ty.count,
                            &callee.source,
                            param.span,
                            &callee.module,
                            &callee.imports,
                        )? != self.program.buffers[binding].count
                    {
                        return self.fail(span, "B167", "调用方没有声明函数需要的数据形状");
                    }
                    Binding::Data(view)
                }
                (false, false, Argument::Atomic(place)) if param.access == Access::Atomic => {
                    self.require_same(&ty, place.ty(), span)?;
                    Binding::AtomicBorrow(place)
                }
                (false, false, Argument::Exclusive(place))
                    if matches!(param.access, Access::InOut | Access::Write) =>
                {
                    self.require_same(&ty, place.ty(), span)?;
                    borrowed.push((param.access, place.clone()));
                    Binding::Owned(if param.access == Access::Write {
                        self.output_scopes += 1;
                        let initialization = Some(
                            (0..place.ty().components.len())
                                .map(|i| Position::Parameter(self.output_scopes, i))
                                .collect(),
                        );
                        OwnedBinding {
                            initialization,
                            place,
                            proof: bounds::Proof::default(),
                            uniform: false,
                        }
                    } else {
                        self.borrow_owned(place)
                    })
                }
                (false, true, Argument::Tile(view)) => {
                    self.require_same(&ty, &view.ty, span)?;
                    if param.access != self.tiles[view.tile].access {
                        return self.fail(
                            span,
                            "B214",
                            "tile 实参与参数必须具有相同的只读／atomic 访问权限",
                        );
                    }
                    Binding::Tile(view)
                }
                (false, false, Argument::Value(mut value)) if param.access == Access::Value => {
                    self.require_same(&ty, &value.ty, span)?;
                    value.literal = false;
                    Binding::Value(value)
                }
                _ => {
                    return self.fail(
                        span,
                        "B207",
                        "函数参数需要匹配的值、数据集、独占／原子位置或 tile 借用",
                    );
                }
            };
            env.insert(param.name.clone(), binding);
        }
        let requirements = contracts::resolve(self, callee, &env)?;
        contracts::prove(self, &requirements, span)?;
        contracts::assume(&mut env, &requirements);
        let old_env = std::mem::replace(&mut self.env, env);
        self.materialize_array_bindings();
        let old_mutable_names = std::mem::take(&mut self.mutable_names);
        assigned_names(
            self.world,
            callee,
            callee.function.body.as_ref().unwrap(),
            &mut self.mutable_names,
        );
        let old_definition = self.definition.clone();
        let old_in_function = self.in_function;
        let old_cooperative = self.cooperative_root;
        let old_block = self.block_depth;
        let old_loop = self.loop_depth;
        let old_iterators = std::mem::take(&mut self.iterators);
        self.definition = callee.clone();
        self.in_function = true;
        self.block_depth = 0;
        self.loop_depth = 0;
        self.cooperative_root = if callee.function.kind == FunctionKind::Cooperative {
            Some(0)
        } else {
            None
        };
        self.calls.push(key);
        let mut result = self.function_body(callee.function.body.as_ref().unwrap())?;
        if let Some(returns::Returned::Value(value)) = &mut result {
            value.literal = false;
        }
        self.calls.pop();
        if self.flow.continues() {
            let updates: Vec<_> = borrowed
                .iter()
                .map(|(_, place)| self.borrow_owned(place.clone()))
                .collect();
            self.env = old_env;
            for (data, scope) in &dataset_outputs {
                self.initialized
                    .retain(|position| !position.uses_scope(*scope));
                self.initialize_application(data);
            }
            let fresh_scopes: BTreeSet<_> = borrowed
                .iter()
                .zip(&updates)
                .filter(|((access, _), _)| *access == Access::Write)
                .flat_map(|(_, o)| o.initialization.iter().flatten())
                .filter_map(|p| {
                    if let Position::Parameter(id, _) = p {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect();
            self.initialized
                .retain(|p| !fresh_scopes.iter().any(|scope| p.uses_scope(*scope)));
            for (access, place) in &borrowed {
                if *access == Access::Write {
                    self.initialize(place);
                }
            }
            for owner in updates {
                self.assign_owned_proof(&owner.place, owner.proof, owner.uniform);
            }
        } else {
            self.env = old_env;
        }
        self.mutable_names = old_mutable_names;
        self.definition = old_definition;
        self.in_function = old_in_function;
        self.cooperative_root = old_cooperative;
        self.block_depth = old_block;
        self.loop_depth = old_loop;
        self.iterators = old_iterators;
        Ok(result)
    }
}
