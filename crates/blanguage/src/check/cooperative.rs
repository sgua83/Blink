//! 函数借用现有数据集、独占／原子位置或 tile；静态绑定实际来源与访问模式。
use super::*;
pub(super) enum Argument {
    BorrowedData(application::DataReference),
    Acceleration(usize),
    Texture(texture::TextureReference),
    Value(Value),
    Dataset(DatasetView),
    ApplicationData(application::Data),
    Scoped { value: returns::Returned, owner: String },
    OwnedValue(owned_values::OwnedValue),
    OwnedReference{value:owned_values::OwnedValue,place:Place,owner:Option<String>},
    Tile(TileView),
    Atomic(Place),
    Reference(OwnedBinding),
}
impl Argument {
    pub(super) fn resource(value:returns::Returned)->Self {
        match value {returns::Returned::Dataset(data)=>Self::ApplicationData(data),returns::Returned::OwnedValue(value)=>Self::OwnedValue(value),returns::Returned::Value(value)=>Self::Value(*value),returns::Returned::Unit=>unreachable!()}
    }
    fn overlaps(&self, other: &Self,env:&BTreeMap<String,Binding>) -> bool {
        let place=|argument:&Self|match argument {
            Self::Reference(owner)=>Some(owner.place.clone()),Self::Atomic(place)=>Some(place.clone()),
            Self::OwnedValue(value)=>value.place(),
            Self::OwnedReference{place,..}=>Some(place.clone()),
            Self::Scoped{owner,..}=>match env.get(owner){Some(Binding::OwnedValue(value))=>value.place(),Some(Binding::Owned(value))=>Some(value.place.clone()),_=>None},_=>None,
        };
        if matches!(self,Self::Reference(_)|Self::Atomic(_)|Self::OwnedReference{..})||matches!(other,Self::Reference(_)|Self::Atomic(_)|Self::OwnedReference{..}) {
            if let(Some(a),Some(b))=(place(self),place(other)) && a.overlaps(&b){return true;}
        }
        if let (Self::Reference(OwnedBinding{place:a,..}) | Self::Atomic(a), Self::Reference(OwnedBinding{place:b,..}) | Self::Atomic(b)) =
            (self, other)
        {
            return a.overlaps(b);
        }
        let handles = |arg: &Self| match arg {
            Self::BorrowedData(reference) => reference.sources.clone(),
            Self::ApplicationData(d) => vec![d.handle.identity()],
            Self::Scoped{value,..}=>match value {returns::Returned::Dataset(data)=>vec![data.handle.identity()],returns::Returned::OwnedValue(value)=>value.handles(),returns::Returned::Value(v)=>v.proof.borrowed_views().into_iter().filter_map(|s|if let view::Source::Data(id)=s{Some(id)}else{None}).collect(),_=>Vec::new()},
            Self::OwnedValue(r) | Self::OwnedReference{value:r,..} => r.handles(),
            Self::Value(v) => v.proof.borrowed_views().into_iter().filter_map(|s| if let view::Source::Data(id)=s {Some(id)}else{None}).collect(),
            Self::Reference(OwnedBinding{place:p,..}) | Self::Atomic(p) => match p.storage() {
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
            || !matches!(param.access, Access::Value | Access::Read | Access::Write | Access::InOut)
            || definition.function.kind != FunctionKind::Value
            || definition.function.target != Target::Cpu)
    {
        return definition.source.fail(
            param.span,
            "B224",
            "拥有聚合通过普通CPU函数按值交接或read／write／inout借用；其他访问与执行目标尚未实现",
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
    #[inline(never)]
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
            self.view_uses.before(arg.span);
            arguments.push(
                if param.ty.is_acceleration() {
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
                    let Some(argument)=self.mutable_argument(arg,param.access)?else{self.abandon_arguments(&held);return Ok(None);};
                    argument
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
                    let argument=if param.access==Access::Read {self.read_argument(arg,expected.as_ref())?}
                        else {self.evaluate_returned(arg,expected.as_ref())?.filter(|value|!matches!(value,returns::Returned::Unit)).map(Argument::resource)};
                    if !self.flow.continues() {
                        self.abandon_arguments(&held);
                        return Ok(None);
                    }
                    match argument {Some(argument)=>argument,None=>return self.fail(arg.span,"B228","函数实参需要值或拥有资源")}
                },
            );
            if !self.flow.continues() {
                self.abandon_arguments(&held);
                return Ok(None);
            }
            if let Some(name) = self.hold_argument(arguments.last_mut().unwrap(), param.access) {
                held.push((arguments.len() - 1, name));
            }
        }
        self.finish_arguments(&held,&mut arguments);
        // 拥有参数交给callee前，调用者仍将继续使用的外部view不能留指向这些资源。
        self.view_uses.after(span);
        let scoped=arguments.iter().filter_map(|argument|match argument{Argument::Scoped{owner,..}|Argument::OwnedReference{owner:Some(owner),..}=>Some(owner.as_str()),_=>None}).collect::<BTreeSet<_>>();
        let transferred=arguments.iter().filter_map(|argument|if let Argument::OwnedReference{place,..}=argument{Some(place.clone())}else{None}).collect::<Vec<_>>();
        let mut retained=view::Borrow::default();
        for(name,binding)in &self.env{if !scoped.contains(name.as_str()){retained.union(&self.view_uses.borrow(name,binding,&transferred));}}
        for argument in &arguments{if let Argument::Reference(owner)=argument{retained.union(&owner.proof.typed_view_borrow(owner.place.ty()));}}
        for(param,argument)in callee.function.params.iter().zip(&arguments){if param.access==Access::Value{
            let resources=match argument{Argument::ApplicationData(data)=>vec![data],Argument::OwnedValue(value)=>value.resources(),_=>Vec::new()};
            for data in resources{self.check_view_requirement(view::Requirement::Unborrowed{resource:self.data_value_borrow(data),borrow:retained.clone(),span})?;}
        }}
        self.view_uses.invocation(span);
        for (param, argument) in callee.function.params.iter().zip(&arguments) {
            if let Argument::OwnedReference{value,..}=argument {
                for data in value.resources(){self.check_view_requirement(view::Requirement::Unborrowed{resource:self.data_value_borrow(data),borrow:retained.clone(),span})?;}
                if param.access==Access::InOut{for data in value.resources(){self.application_handle_available(data.handle.identity(),span)?;}}
            }
            if let Argument::BorrowedData(reference) = argument {
                if param.access.writes() { self.check_view_requirement(view::Requirement::Unborrowed{resource:self.data_borrow(&reference.sources),borrow:retained.clone(),span})?; }
                for source in &reference.sources {
                    self.application_handle_available(*source, span)?;
                }
            }
            if let Argument::Reference(OwnedBinding{place,..}) | Argument::Atomic(place) = argument
                && let Place::ApplicationElement { sources, .. } = place.storage()
            {
                for source in sources {
                    self.application_handle_available(*source, span)?;
                }
            }
            let handle = match argument {
                Argument::ApplicationData(data) | Argument::Scoped { value:returns::Returned::Dataset(data), .. }
                    if param.access != Access::Value =>
                {
                    Some(data.handle.identity())
                }
                Argument::Reference(OwnedBinding{place,..}) | Argument::Atomic(place) => match place.storage() {
                    Place::ApplicationElement { identity, .. } => Some(*identity),
                    _ => None,
                },
                _ => None,
            };
            if let Some(handle) = handle {
                if param.access.writes() { self.check_view_requirement(view::Requirement::Unborrowed{resource:self.data_borrow(&[handle]),borrow:retained.clone(),span})?; }
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
                    || (param.access==Access::Value && matches!(argument, Argument::OwnedValue(_)))
                    || (other.access==Access::Value && matches!(before, Argument::OwnedValue(_))))
                    && argument.overlaps(before,&self.env)
                {
                    return self.fail(
                        args[index].span,
                        "B227",
                        "同一次调用的独占借用不能重叠，也不能与同一存储的只读或原子借用重叠",
                    );
                }
            }
        }
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
                .map(|v| v.map(returns::Returned::value));
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
                    return self.native_function(instance, args, span);
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
            FunctionKind::Value => {
                if self.application.is_some() && instance.definition.function.target != Target::Gpu {
                    self.native_function(instance, args, span)
                } else {
                    self.inline_instance(instance, args, span)
                }
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
        let mut data_access = Vec::new();
        let mut held = Vec::new();
        for (parameter,(param, arg)) in callee.function.params.iter().zip(args).enumerate() {
            let arg=if let Argument::Scoped{value,owner}=arg {held.push(owner);Argument::resource(value)}else{arg};
            let arg=match arg {Argument::Value(value) if param.access==Access::Read=>Argument::Reference(self.materialize_array(value)),other=>other};
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
                (true, false, Argument::Dataset(view)) => {
                    let binding = view.binding;
                    self.require_same(&ty, &self.program.buffers[binding].ty, span)?;
                    let access = self.view_access[&binding];
                    if param.access == Access::Read && access == Access::InOut {
                        data_access.push((binding, access));
                        self.view_access.insert(binding, Access::Read);
                    } else if !matches!(param.access, Access::Read | Access::Atomic)
                        || param.access != access
                    {
                        return self.fail(
                            span,
                            "B224",
                            "并行实例只能转借 read／atomic dataset；read 可以借自本阶段的 inout，不能取得整份独占修改权",
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
                (false, false, Argument::Reference(mut owner))
                    if matches!(param.access, Access::Read | Access::InOut | Access::Write) =>
                {
                    self.require_same(&ty, owner.place.ty(), span)?;
                    let place=owner.place.clone();
                    if param.access.writes() { borrowed.push((parameter,param.access, place.clone())); }
                    Binding::Owned(if param.access == Access::Read {
                        owner.place=owner.place.read_only();owner
                    } else if param.access == Access::Write {
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
                        owner
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
        let local_initialization = self.env.values().all(|binding| matches!(binding,
            Binding::Value(value) if !value.ty.contains_views() && !value.ty.owns_resources()
        )) && if callee.function.result.name == "void" || callee.function.result.is_never() {
            true
        } else {
            let ty = self.expected_storage(&callee.function.result, callee.function.span)?;
            !ty.contains_views() && !ty.owns_resources()
        };
        let caller_initialization = local_initialization.then(|| std::mem::take(&mut self.initialized));
        self.materialize_array_bindings();
        let output_inputs=outputs::inputs(self,callee,&self.env.clone())?;
        // With no fact or resource inputs/outputs, private range paths end at this call.
        let local_bounds = self.env.values().all(|binding| matches!(binding,
            Binding::Value(value) if !value.ty.contains_views() && !value.ty.owns_resources()
                && bounds::Proof::fact_components(&value.ty).next().is_none()
        )) && if callee.function.result.name == "void" || callee.function.result.is_never() {
            true
        } else {
            let ty = self.expected_storage(&callee.function.result, callee.function.span)?;
            !ty.contains_views() && !ty.owns_resources()
                && bounds::Proof::fact_components(&ty).next().is_none()
        };
        let caller_path = std::mem::take(&mut self.path);
        let (mut result,_) = self.function_body(callee.function.body.as_ref().unwrap())?;
        if let Some(initialized) = caller_initialization {
            self.initialized = initialized;
        }
        if let Some(returns::Returned::Value(value)) = &mut result {
            value.literal = false;
        }
        self.calls.pop();
        if self.flow.continues() {
            self.path = if local_bounds {
                caller_path
            } else {
                bounds::Clause::both(&caller_path, &self.path)
            };
            let returned=result.as_ref().map(|value|self.returned_storage(value));
            let mut updates=Vec::new();
            for(index,_,place)in &borrowed {
                updates.push((self.borrow_owned(place.clone()),outputs::guards(self,callee,*index,place.ty(),&output_inputs,returned.as_ref())?));
            }
            self.env = old_env;
            let fresh_scopes: BTreeSet<_> = borrowed
                .iter()
                .zip(&updates)
                .filter(|((_,access, _), _)| *access == Access::Write)
                .flat_map(|(_, (o,_))| o.initialization.iter().flatten())
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
            for ((index,_,place),(_,guards)) in borrowed.iter().zip(&updates) {
                if outputs::declared(callee,*index){self.forget_output_initialization(place,span)?;}
                self.initialize_output(place,guards);
            }
            for (owner,_) in updates {
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
        for (binding, access) in data_access { self.view_access.insert(binding, access); }
        for owner in held { self.finish_scoped_argument(&owner,span)?; }
        Ok(result)
    }
}
