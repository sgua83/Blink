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

}
pub(super) async fn parameter_type(world: &World, param: &Param, context: (&Source,&str,&[String],FunctionKind), target: Target) -> Result<Ty> {
    let(source,module,imports,kind)=context;
    let declared=&param.ty;
    let mut name = declared.clone();
    name.tile = false;
    let ty = unsafe { world.call(world.ty(
        &name,
        source,
        param.span,
        module,
        imports,
    )).await }?;
    if ty.owns_resources()
        && (declared.tile
            || !matches!(param.access, Access::Value | Access::Read | Access::Write | Access::InOut)
            || kind != FunctionKind::Value
            || target != Target::Cpu)
    {
        return source.fail(
            param.span,
            "B224",
            "拥有聚合通过普通CPU函数按值交接或read／write／inout借用；其他访问与执行目标尚未实现",
        );
    }
    if declared.tile && ty.contains_views() {
        return source.fail(
            param.span,
            "B207",
            "tile 借用只能包含普通值，不能包含资源 view",
        );
    }
    if param.access == Access::Atomic
        && !declared.tile
        && ty.contains_views()
    {
        return source.fail(
            param.span,
            "B223",
            "原子借用参数不能包含资源 view",
        );
    }
    Ok(ty)
}
#[derive(Clone,Copy)]
pub(super) enum CallParameters<'a>{
    Declaration(&'a Definition),
    CFunction(&'a [Param]),
}
impl CallParameters<'_>{
    pub(super) fn len(self)->usize{match self{
        Self::Declaration(definition)=>definition.function.params().len(),
        Self::CFunction(parameters)=>parameters.len(),
    }}
    pub(super) fn access(self,index:usize)->Access{match self{
        Self::Declaration(definition)=>definition.function.params().at(index).access(),
        Self::CFunction(parameters)=>parameters[index].access,
    }}
    pub(super) fn form(self,index:usize)->DeclaredArgumentForm{match self{
        Self::Declaration(definition)=>declared_argument_form(&definition.function.params().at(index).ty()),
        // The cfn producer has already checked its ordinary C ABI layout.
        Self::CFunction(_)=>DeclaredArgumentForm::Ordinary,
    }}
    pub(super) async fn expected(self,world:&World,caller:&LexicalScope,index:usize)->Result<Option<Ty>>{
        match self{
            Self::Declaration(definition)=>{
                if !definition.function.types().is_empty()||!definition.function.values().is_empty(){return Ok(None);}
                let parameter=definition.function.params().at(index);
                // This declaration has no generic formals; its lexical names
                // resolve in its own module, never through the caller's bindings.
                let bindings=Arc::new(crate::generics::Bindings::default());
                let syntax=unsafe { world.call(bindings.type_name(world,&parameter.ty(),
                    (&definition.source,&definition.module,&definition.imports),parameter.span())).await }?;
                unsafe { world.call(world.ty(&syntax,&definition.source,parameter.span(),&definition.module,&definition.imports)).await }.map(Some)
            }
            Self::CFunction(parameters)=>{
                let parameter=&parameters[index];
                unsafe { world.call(world.ty(&parameter.ty,caller.source(),parameter.span,caller.module(),caller.imports())).await }.map(Some)
            }
        }
    }
    pub(super) fn declares_output(self,index:usize)->bool{
        match self{
            Self::Declaration(definition)=>{
                let parameter=definition.function.params().at(index);
                definition.function.contracts().iter().any(|contract|
                    matches!(contract.predicate().kind(),crate::syntax::PredicateKind::Initialized{value,..}
                        if value.split('.').next()==Some(parameter.name())))
            }
            Self::CFunction(_)=>false,
        }
    }
}

impl<'a> Builder<'a> {
    #[inline(never)]
    pub(super) async fn function_arguments(&mut self, key: &str, args: &Expressions, span: Span) -> Result<Option<Vec<Argument>>> {
        let callee = self.world.functions[key].clone();
        unsafe { self.world.call(self.call_arguments(CallParameters::Declaration(&callee), args, span)).await }
    }
    pub(super) async fn call_arguments(&mut self,parameters:CallParameters<'_>,args:&Expressions,span:Span)->Result<Option<Vec<Argument>>> {
        unsafe{self.world.call(call_arguments::build(self,parameters,args.len(),span,|index|args.at(index).span(),
            async |builder,index,parameter|unsafe{builder.world.call(call_arguments::source(builder,parameters,index,parameter,&args.at(index))).await})).await}
    }
    pub(super) fn check_argument_aliases(&self, parameter: impl Fn(usize)->(Access,bool), arguments: &[Argument], span: impl Fn(usize) -> Span) -> Result<()> {
        if let Some(index)=call_aliases::conflict(self,parameter,arguments){
            return self.fail(span(index),"B227","同一次调用的独占借用不能重叠，也不能与同一存储的只读或原子借用重叠");
        }
        Ok(())
    }
    pub(super) async fn invoke_instance(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
        syntax: Option<&Expressions>,
    ) -> Result<Option<Value>> {
        let result = unsafe { self.world.call(self.invoke_returned(instance, args, span, None, syntax)).await }?;
        self.ordinary_return(result, span)
    }
    pub(super) async fn invoke_returned(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
        generated_origin: Option<Arc<GeneratedOrigin>>,
        syntax: Option<&Expressions>,
    ) -> Result<Option<returns::Returned>> {
        if instance.definition.function.external().is_some() {
            return unsafe { self.world.call(self
                .external_call(&instance, args, span, generated_origin)).await }
                .map(|v| v.map(returns::Returned::value));
        }
        if instance.definition.compiled.is_some()
            && instance.definition.execution != crate::execution::Requirement::Domain
        {
            if self.application.is_none() && self.program.target != Target::Cpu {
                return self.fail(span, "B161", "已编译普通CPU模块调用需要CPU执行目标");
            }
            return unsafe { self.world.call(self.native_function(instance, args, span, generated_origin, syntax, None)).await };
        }
        if instance.definition.execution == crate::execution::Requirement::Domain {
            if self.application_entry() {
                if instance.definition.function.target() == Target::Cpu
                    && !unsafe { self.world.call(super::kernel_interface(self.world, &instance)).await }?
                {
                    return unsafe { self.world.call(self.native_function(instance, args, span, generated_origin, syntax, None)).await };
                }
                return unsafe { self.world.call(self.application_execute(DomainInput::Declaration(&instance), args, Target::Cpu, span, &[])).await };
            }
            return self.fail(
                span,
                "B159",
                "整域执行需要发起位置，不能由元素实例或协作成员发起",
            );
        }
        match instance.definition.function.kind() {
            FunctionKind::Cooperative => {
                self.fail(span, "B208", "cooperative 函数需要显式 cooperate 根块调用")
            }
            FunctionKind::Value => {
                if instance.definition.function.target() == Target::Cpu && self.program.target == Target::Cpu
                    || self.application_entry() && instance.definition.function.target() != Target::Gpu
                {
                    unsafe { self.world.call(self.native_function(instance, args, span, generated_origin, syntax, None)).await }
                } else {
                    unsafe { self.world.call(self.inline_instance(instance, args, span, syntax)).await }
                }
            }
            FunctionKind::Constant => {
                self.fail(span, "B234", "const 通过名称读取，不能作为函数调用")
            }
        }
    }
    pub(super) async fn cooperate(
        &mut self,
        name: &str,
        explicit: Option<&crate::syntax::List<crate::syntax::wire::ArgumentNode>>,
        args: &Expressions,
        span: Span,
    ) -> Result<Option<Value>> {
        if !self.is_cooperative_root() {
            return self.fail(span, "B208", "cooperate 需要 parallel 或 cooperative 函数的根块声明，不能位于分支／循环或普通值函数中");
        }
        let key = self
            .world
            .resolve(
                name,
                self.scope.module(),
                self.scope.imports(),
                true,
            ).unique()
            .ok_or_else(|| Diagnostic {
                source: self.scope.source().clone(),
                span,
                code: "B158",
                message: format!("未知协作函数 {name}"),
            })?;
        let callee = &self.world.functions[&key];
        if callee.function.kind() != FunctionKind::Cooperative {
            return self.fail(span, "B208", "cooperate 只能调用声明为 cooperative 的函数");
        }
        if args.len() != callee.function.params().len() {
            return self.fail(span, "B162", "cooperate 参数数量不匹配");
        }
        let Some(arguments) = unsafe { self.world.call(self.function_arguments(&key, args, span)).await }? else {
            return Ok(None);
        };
        let explicit=match explicit {
            Some(arguments)=>Some(unsafe { self.world.call(self.scope.bindings().arguments(self.world,arguments,
                (self.scope.source(),self.scope.module(),self.scope.imports()),span)).await }?),
            None=>None,
        };
        let instance = unsafe { self.world.call(self.specialized_arguments(key, &arguments, explicit.as_deref(), span)).await }?;
        let result = unsafe { self.world.call(self.inline_instance(instance, arguments, span, Some(args))).await }?;
        self.ordinary_return(result, span)
    }
    pub(super) async fn inline_instance(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
        syntax: Option<&Expressions>,
    ) -> Result<Option<returns::Returned>> {
        if instance.definition.function.target() != Target::Compute
            && instance.definition.function.target() != self.program.target
        {
            return self.fail(span, "B161", "函数设备能力与当前程序不相容");
        }
        if self.calls.contains(&instance.name) {
            if self.program.target != Target::Gpu
                && instance.definition.function.kind() == FunctionKind::Value
            {
                return unsafe { self.world.call(self.native_function(instance, args, span, None, syntax, None)).await };
            }
            return self.fail(span, "B160", "当前设备函数不允许递归调用");
        }
        let key = instance.name.clone();
        let callee = &instance.definition;
        let result_name=unsafe { self.world.call(instance.bindings.type_name(self.world,&callee.function.result(),
            (&callee.source,&callee.module,&callee.imports),callee.function.span())).await }?;
        let mut env = BTreeMap::new();
        let mut borrowed = Vec::new();
        let mut held = Vec::new();
        for (parameter,(param, arg)) in instance.parameters(self.world).zip(args).enumerate() {
            let param=unsafe { self.world.call(param).await }?;
            let arg=if let Argument::Scoped{value,owner}=arg {held.push(owner);Argument::resource(value)}else{arg};
            let arg=match arg {Argument::Value(value) if param.access==Access::Read=>Argument::Reference(self.materialize_array(value,None)),other=>other};
            if param.ty.is_acceleration() {
                let Argument::Acceleration(index) = arg else {
                    return self.fail(span, "B183", "read acceleration 参数需要加速结构资源");
                };
                env.insert(param.name.to_owned(), Binding::Acceleration(index));
                continue;
            }
            if param.ty.is_texture_resource() {
                let Argument::Texture(index) = arg else {
                    return self.fail(span, "B239", "只读纹理参数需要相同维度的纹理资源");
                };
                unsafe { self.world.call(self.texture_parameter(&param, &index,(&callee.source,&callee.module,&callee.imports),span)).await }?;
                env.insert(param.name.to_owned(), Binding::Texture(index));
                continue;
            }
            let ty = unsafe { self.world.call(parameter_type(self.world,&param,(&callee.source,&callee.module,&callee.imports,callee.function.kind()),self.program.target)).await }?;
            let binding = match (param.ty.dataset, param.ty.tile, arg) {
                (true, false, Argument::Dataset(view)) => {
                    let binding = view.binding;
                    self.require_same(&ty, self.dataset_layout(&view).1, span)?;
                    if !matches!((param.access,view.access),
                        (Access::Read,Access::Read|Access::InOut)|(Access::Write,Access::Write|Access::InOut)|(Access::Atomic,Access::Atomic)|(Access::InOut,Access::InOut)) {
                        return self.fail(span,"B224","并行实例的dataset借用需要对应的实际来源权限");
                    }
                    let mut view=view;
                    if param.access==Access::Write {
                        let place=self.dataset_element(&view,self.global_index,false);
                        self.forget_output_initialization(&place,span)?;
                        borrowed.push((parameter,param.access,place));
                    }
                    view.access=param.access;
                    if param.ty.count.is_some()
                        && unsafe { self.world.call(self.world.shape(
                            &param.ty.count,
                            &callee.source,
                            param.span,
                            &callee.module,
                            &callee.imports,
                        )).await }? != self.program.buffers[binding].count
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
            env.insert(param.name.to_owned(), binding);
        }
        let requirements = unsafe { self.world.call(contracts::resolve(self,(&instance).into(),&LexicalScope::Function(instance.clone()),&env,&BTreeMap::new())).await }?;
        unsafe { self.world.call(contracts::prove(self, &requirements, span)).await }?;
        unsafe { self.world.call(contracts::assume(self,(&instance).into(),&mut env,&requirements)).await }?;
        let old_env = std::mem::replace(&mut self.env, env);
        let old_mutable_names = std::mem::take(&mut self.mutable_names);
        let callee_scope = LexicalScope::Function(instance.clone());
        unsafe { self.world.call(assigned_names(
            self.world,
            &callee_scope,
            callee.function.body().as_ref().unwrap(),
            &mut self.mutable_names,
        )).await }?;
        let old_scope = self.scope.clone();
        let old_in_function = self.in_function;
        let old_cooperative = self.cooperative_root;
        let old_block = self.block_depth;
        let old_loop = self.loop_depth;
        let old_iterators = std::mem::take(&mut self.iterators);
        let mut inlined_at = self.source_context.inlined_at.to_vec();
        inlined_at.push(InlineSite { origin: self.source_definition.clone(), instance: self.source_context.instance.clone(), span });
        let source_context = Arc::new(SourceContext { instance: Some(Arc::from(key.as_str())), inlined_at: inlined_at.into_boxed_slice(), domain_path: self.source_context.domain_path.clone() });
        let old_source_context = std::mem::replace(&mut self.source_context, source_context);
        let old_source_definition = std::mem::replace(&mut self.source_definition, callee.source_origin().map(Arc::new));
        let old_source_site = self.source_site.take();
        self.scope = callee_scope;
        self.in_function = true;
        self.block_depth = 0;
        self.loop_depth = 0;
        self.cooperative_root = if callee.function.kind() == FunctionKind::Cooperative {
            Some(0)
        } else {
            None
        };
        self.calls.push(key);
        let local_initialization = self.env.values().all(|binding| matches!(binding,
            Binding::Value(value) if !value.ty.contains_borrows() && !value.ty.owns_resources()
        )) && if result_name.name == "void" || result_name.is_never() {
            true
        } else {
            let ty = unsafe { self.world.call(self.expected_storage(&result_name, callee.function.span())).await }?;
            !ty.contains_borrows() && !ty.owns_resources()
        };
        let caller_initialization = local_initialization.then(|| std::mem::take(&mut self.initialized));
        self.materialize_array_bindings();
        let output_inputs=unsafe { self.world.call(outputs::current_inputs(self,&instance)).await }?;
        // With no fact or resource inputs/outputs, private range paths end at this call.
        let local_bounds = self.env.values().all(|binding| matches!(binding,
            Binding::Value(value) if !value.ty.contains_borrows() && !value.ty.owns_resources()
                && bounds::Proof::fact_components(&value.ty).next().is_none()
        )) && if result_name.name == "void" || result_name.is_never() {
            true
        } else {
            let ty = unsafe { self.world.call(self.expected_storage(&result_name, callee.function.span())).await }?;
            !ty.contains_borrows() && !ty.owns_resources()
                && bounds::Proof::fact_components(&ty).next().is_none()
        };
        let caller_path = std::mem::take(&mut self.path);
        let (mut result,_) = unsafe { self.world.call(self.function_body(&instance, callee.function.body().as_ref().unwrap())).await }?;
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
                updates.push((self.borrow_owned(place.clone()),unsafe { self.world.call(outputs::guards(self,&instance,*index,place.ty(),&output_inputs,returned.as_ref())).await }?));
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
                if outputs::declared(&instance,*index){self.forget_output_initialization(place,span)?;}
                self.initialize_output(place,guards);
            }
            for (owner,_) in updates {
                unsafe { self.world.call(self.assign_owned_proof(&owner.place, owner.proof, owner.uniform)).await }?;
            }
        } else {
            self.env = old_env;
        }
        self.mutable_names = old_mutable_names;
        self.scope = old_scope;
        self.source_definition = old_source_definition;
        self.source_context = old_source_context;
        self.source_site = old_source_site;
        self.in_function = old_in_function;
        self.cooperative_root = old_cooperative;
        self.block_depth = old_block;
        self.loop_depth = old_loop;
        self.iterators = old_iterators;
        for owner in held { unsafe { self.world.call(self.finish_scoped_argument(&owner,span)).await }?; }
        Ok(result)
    }
}
