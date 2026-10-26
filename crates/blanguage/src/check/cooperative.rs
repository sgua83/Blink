//! 函数借用现有数据集、独占／原子位置或 tile；静态绑定实际来源与访问模式。
use super::*;
pub(super) enum Argument {
    Acceleration(usize),
    Texture(texture::TextureReference),
    Value(Value),
    Dataset(DatasetView),
    ApplicationData(application::Data),
    Tile(TileView),
    Atomic(Place),
    Exclusive(Place),
}
impl Argument {
    fn overlaps(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ApplicationData(a), Self::ApplicationData(b)) => a.handle == b.handle,
            (Self::ApplicationData(data), Self::Exclusive(place) | Self::Atomic(place))
            | (Self::Exclusive(place) | Self::Atomic(place), Self::ApplicationData(data)) => {
                matches!(place.storage(), Place::ApplicationElement { handle, .. } if *handle == data.handle)
            }
            (Self::Exclusive(a) | Self::Atomic(a), Self::Exclusive(b) | Self::Atomic(b)) => {
                a.overlaps(b)
            }
            _ => false,
        }
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
    ) -> Result<Vec<Argument>> {
        let callee = &self.world.functions[key];
        if args.len() != callee.function.params.len() {
            return self.fail(span, "B162", "函数参数数量不匹配");
        }
        let mut arguments = Vec::new();
        for (param, arg) in callee.function.params.iter().zip(args) {
            arguments.push(
                if callee.function.external.is_some() && param.access == Access::Read {
                    Argument::Exclusive(self.external_read_place(arg)?)
                } else if param.ty.is_acceleration() {
                    let ExprKind::Name(name) = &arg.kind else {
                        return self.fail(arg.span, "B183", "acceleration 借用需要已有资源名称");
                    };
                    let Some(Binding::Acceleration(index)) = self.env.get(name) else {
                        return self.fail(arg.span, "B183", "acceleration 借用需要已有资源名称");
                    };
                    Argument::Acceleration(*index)
                } else if param.ty.is_texture_resource() {
                    Argument::Texture(self.texture_argument(arg)?)
                } else if param.ty.dataset {
                    let ExprKind::Name(name) = &arg.kind else {
                        return self.fail(arg.span, "B224", "dataset 借用需要已有数据集名称");
                    };
                    match self.env.get(name) {
                        Some(Binding::Data(view)) => Argument::Dataset(view.clone()),
                        Some(Binding::ApplicationData(data)) => {
                            Argument::ApplicationData(data.clone())
                        }
                        _ => return self.fail(arg.span, "B224", "dataset 借用需要已有数据集名称"),
                    }
                } else if param.ty.tile {
                    Argument::Tile(self.tile_view(arg)?)
                } else if param.access == Access::Atomic {
                    Argument::Atomic(self.atomic_place(arg)?)
                } else if matches!(param.access, Access::InOut | Access::Write) {
                    let place = self.mutable_place(arg, param.access)?;
                    Argument::Exclusive(place)
                } else {
                    Argument::Value(self.expr(arg)?)
                },
            );
        }
        for (index, (param, argument)) in callee.function.params.iter().zip(&arguments).enumerate()
        {
            for (other, before) in callee.function.params.iter().zip(&arguments).take(index) {
                if (matches!(param.access, Access::Write | Access::InOut)
                    || matches!(other.access, Access::Write | Access::InOut))
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
        Ok(arguments)
    }
    pub(super) fn invoke_function(
        &mut self,
        key: String,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<Value>> {
        let instance = self.specialized_function(key, &args, span)?;
        self.invoke_instance(instance, args, span)
    }
    pub(super) fn invoke_instance(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<Value>> {
        if instance.definition.function.external.is_some() {
            return self.external_call(&instance.definition, args, span);
        }
        match instance.definition.function.kind {
            FunctionKind::Program => self.fail(span, "B159", "program 调用只能位于程序阶段层"),
            FunctionKind::Cooperative => {
                self.fail(span, "B208", "cooperative 函数需要显式 cooperate 根块调用")
            }
            FunctionKind::Value => {
                let result = self.inline_instance(instance, args, span)?;
                self.ordinary_return(result, span)
            }
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
        let arguments = self.function_arguments(&key, args, span)?;
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
            let binding = match (param.ty.dataset, param.ty.tile, arg) {
                (true, false, Argument::ApplicationData(mut data)) => {
                    self.application_cpu(&data, span)?;
                    self.require_same(&ty, &data.ty, span)?;
                    if !matches!(param.access, Access::Read | Access::InOut) {
                        return self.fail(
                            span,
                            "B224",
                            "CPU 应用函数按 read／inout 借用 dataset；原子与 write 全域借用尚未实现",
                        );
                    }
                    if param.access == Access::InOut && !data.writable {
                        return self.fail(span, "B227", "只读 dataset 借用不能提升为 inout");
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
                    data.owned = false;
                    data.writable = param.access == Access::InOut;
                    Binding::ApplicationData(data)
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
        let updates: Vec<_> = borrowed
            .iter()
            .map(|(_, place)| self.borrow_owned(place.clone()))
            .collect();
        self.env = old_env;
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
            .retain(|p| !matches!(p, Position::Parameter(id, _) if fresh_scopes.contains(id)));
        for (access, place) in &borrowed {
            if *access == Access::Write {
                self.initialize(place);
            }
        }
        for owner in updates {
            self.assign_owned_proof(&owner.place, owner.proof, owner.uniform);
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
