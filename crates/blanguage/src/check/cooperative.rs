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
    fn overlaps(&self, other: &Self,builder:&Builder<'_>) -> bool {
        let env=&builder.env;
        let stage_region=|argument:&Self|match argument {
            Self::Dataset(view)=>{
                let (field,ty)=builder.dataset_layout(view);
                Some((view.binding,ty.components.iter().map(|(scalar,word)|(field+word,field+word+scalar.size())).collect::<Vec<_>>()))
            },
            Self::Reference(OwnedBinding{place,..})|Self::Atomic(place)|Self::OwnedReference{place,..}=>match place.storage(){
                Place::Element{binding,ty,words,..}=>Some((*binding,ty.components.iter().zip(words).map(|((scalar,_),word)|(*word,*word+scalar.size())).collect())),
                _=>None,
            },
            _=>None,
        };
        if (matches!(self,Self::Dataset(_))||matches!(other,Self::Dataset(_)))
            && let(Some((a,words)),Some((b,other)))=(stage_region(self),stage_region(other)){
            return a==b&&words.iter().any(|(start,end)|other.iter().any(|(other_start,other_end)|start<other_end&&other_start<end));
        }
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
        let region=|argument:&Self|match argument {
            Self::BorrowedData(data)=>Some((data.sources.clone(),data.words())),
            Self::Reference(OwnedBinding{place,..})|Self::Atomic(place)|Self::OwnedReference{place,..}=>match place.storage(){
                Place::ApplicationElement{sources,words,..}=>Some((sources.clone(),words.clone())),
                _=>None,
            },
            _=>None,
        };
        if let(Some((a,words)),Some((b,other)))=(region(self),region(other)){
            return a.iter().any(|source|b.contains(source))&&words.iter().any(|word|other.contains(word));
        }
        let owners=|borrow:view::Borrow|borrow.owners().sources.into_iter().filter_map(|source|if let view::Source::Data(id)=source{Some(id)}else{None}).collect::<Vec<_>>();
        let handles = |arg: &Self| match arg {
            Self::BorrowedData(reference) => {
                let mut handles=reference.sources.clone();
                handles.extend(owners(view::resource_borrow(&reference.data)));handles
            },
            Self::ApplicationData(d) => vec![d.root.identity()],
            Self::Scoped{value,..}=>match value {returns::Returned::Dataset(data)=>vec![data.root.identity()],returns::Returned::OwnedValue(value)=>value.handles(),returns::Returned::Value(v)=>owners(v.proof.typed_view_borrow(&v.ty)),_=>Vec::new()},
            Self::OwnedValue(r) => r.handles(),
            Self::OwnedReference{value,place,..} => {
                let mut handles=value.handles();
                if let Place::ApplicationElement{sources,resource,..}=place.storage(){
                    handles.extend(sources);handles.extend(owners(resource.clone()));
                }
                handles
            },
            Self::Value(v) => owners(v.proof.typed_view_borrow(&v.ty)),
            Self::Reference(owner) => {
                let mut handles=owners(owner.proof.typed_view_borrow(owner.place.ty()));
                if let Place::ApplicationElement{sources,resource,..}=owner.place.storage(){handles.extend(sources);handles.extend(owners(resource.clone()));}
                handles
            },
            Self::Atomic(p) => match p.storage() {
                Place::ApplicationElement { sources,resource,.. } => {
                    let mut handles=sources.clone();handles.extend(owners(resource.clone()));handles
                },
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        let a = handles(self);
        let b = handles(other);
        a.iter().any(|h| b.contains(h))
    }
}
pub(super) fn parameter_type(world: &World, param: &Param, context: (&Source,&str,&[String],FunctionKind), target: Target) -> Result<Ty> {
    let(source,module,imports,kind)=context;
    let declared=&param.ty;
    let mut name = declared.clone();
    name.tile = false;
    let ty = world.ty(
        &name,
        source,
        param.span,
        module,
        imports,
    )?;
    if ty.owns_resources()
        && (declared.dataset && !matches!(param.access, Access::Value | Access::Read | Access::InOut)
            || declared.tile
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
    pub(super) fn expected(self,world:&World,caller:&LexicalScope,index:usize)->Result<Option<Ty>>{
        match self{
            Self::Declaration(definition)=>{
                if !definition.function.types().is_empty()||!definition.function.values().is_empty(){return Ok(None);}
                let parameter=definition.function.params().at(index);
                // This declaration has no generic formals; its lexical names
                // resolve in its own module, never through the caller's bindings.
                let bindings=Arc::new(crate::generics::Bindings::default());
                let syntax=bindings.type_name(world,&parameter.ty(),
                    (&definition.source,&definition.module,&definition.imports),parameter.span())?;
                world.ty(&syntax,&definition.source,parameter.span(),&definition.module,&definition.imports).map(Some)
            }
            Self::CFunction(parameters)=>{
                let parameter=&parameters[index];
                world.ty(&parameter.ty,caller.source(),parameter.span,caller.module(),caller.imports()).map(Some)
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
    pub(super) fn function_arguments(&mut self, key: &str, args: &Expressions, span: Span) -> Result<Option<Vec<Argument>>> {
        let callee = self.world.functions[key].clone();
        self.call_arguments(CallParameters::Declaration(&callee), args, span)
    }
    #[inline(never)]
    fn source_call_argument(&mut self,parameters:CallParameters<'_>,index:usize,arg:&Expr)->Result<Option<Argument>> {
        let access=parameters.access(index);
        let form=parameters.form(index);
        Ok(Some(
if form==DeclaredArgumentForm::Acceleration {
                    let ExprKind::Name(name) = arg.kind() else {
                        return self.fail(arg.span(), "B183", "acceleration 借用需要已有资源名称");
                    };
                    let Some(Binding::Acceleration(index)) = self.env.get(name) else {
                        return self.fail(arg.span(), "B183", "acceleration 借用需要已有资源名称");
                    };
                    Argument::Acceleration(*index)
                } else if form==DeclaredArgumentForm::Texture {
                    {
                        let Some(texture) = self.texture_argument(&arg)? else {
                                return Ok(None);
                        };
                        Argument::Texture(texture)
                    }
                } else if form==DeclaredArgumentForm::Dataset && access == Access::Value {
                    let returned = self.evaluate_returned(&arg, None)?;
                    if !self.flow.continues() {
                        return Ok(None);
                    }
                    let Some(returns::Returned::Dataset(data)) = returned else {
                        return self.fail(arg.span(), "B224", "dataset 值参数需要拥有的数据集");
                    };
                    Argument::ApplicationData(data)
                } else if form==DeclaredArgumentForm::Dataset {
                    if let Some(data) = self.application_reference(&arg)? {
                        Argument::BorrowedData(data)
                    } else if let Some(view) = self.dataset_reference(&arg)? {
                        Argument::Dataset(view)
                    } else {
                        let argument = self.scoped_argument(&arg)?;
                        if !self.flow.continues() {
                                return Ok(None);
                        }
                        argument.unwrap()
                    }
                } else if form==DeclaredArgumentForm::Tile {
                    Argument::Tile(self.tile_view(&arg)?)
                } else if access == Access::Atomic {
                    {
                        let Some(place) = self.atomic_place(&arg)? else {
                                return Ok(None);
                        };
                        Argument::Atomic(place)
                    }
                } else if matches!(access, Access::InOut | Access::Write) {
                    let Some(argument)=self.mutable_argument(&arg,access)?else{return Ok(None);};
                    argument
                } else {
                    let expected=parameters.expected(self.world,&self.scope,index)?;
                    let argument=if access==Access::Read {self.read_argument(&arg,expected.as_ref())?}
                        else {self.evaluate_returned(&arg,expected.as_ref())?.filter(|value|!matches!(value,returns::Returned::Unit)).map(Argument::resource)};
                    if !self.flow.continues() {
                        return Ok(None);
                    }
                    match argument {Some(argument)=>argument,None=>return self.fail(arg.span(),"B228","函数实参需要值或拥有资源")}
                }
        ))
    }
    pub(super) fn call_arguments(&mut self,parameters:CallParameters<'_>,args:&Expressions,span:Span)->Result<Option<Vec<Argument>>> {
        self.call_arguments_from(parameters,args.len(),span,|index|args.at(index).span(),
            |builder,index|builder.source_call_argument(parameters,index,&args.at(index)))
    }
    pub(super) fn call_arguments_from(&mut self,parameters:CallParameters<'_>,count:usize,span:Span,
        argument_span:impl Fn(usize)->Span,
        mut lower:impl FnMut(&mut Self,usize)->Result<Option<Argument>>)->Result<Option<Vec<Argument>>> {
        if count != parameters.len() {return self.fail(span,"B162","函数参数数量不匹配");}
        let mut arguments=Vec::new();
        let mut held=Vec::new();
        for parameter_index in 0..count {
            let access=parameters.access(parameter_index);
            let form=parameters.form(parameter_index);
            let arg_span=argument_span(parameter_index);
            self.view_uses.before(arg_span);
            let argument=lower(self,parameter_index)?;
            if !self.flow.continues() {self.abandon_arguments(&held);return Ok(None);}
            let Some(argument)=argument else {self.abandon_arguments(&held);return Ok(None);};
            arguments.push(argument);
            if form==DeclaredArgumentForm::Dataset && (access==Access::Write || access==Access::InOut && parameters.declares_output(parameter_index))
                && let Some(Argument::BorrowedData(data))=arguments.last_mut()
            {
                self.prepare_data_output(&mut data.data,arg_span)?;
            }
            if let Some(name) = self.hold_argument(arguments.last_mut().unwrap(), access)? {
                held.push((arguments.len() - 1, name));
            }
        }
        self.finish_arguments(&held,&mut arguments);
        // 拥有参数交给callee前，调用者仍将继续使用的外部view不能留指向这些资源。
        self.view_uses.after(span);
        let scoped=arguments.iter().filter_map(|argument|match argument{Argument::Scoped{owner,..}|Argument::OwnedReference{owner:Some(owner),..}=>Some(owner.as_str()),_=>None}).collect::<BTreeSet<_>>();
        let transferred=(0..parameters.len()).zip(&arguments).filter_map(|(parameter,argument)|{
            if !parameters.access(parameter).writes(){return None;}
            match argument{
                Argument::OwnedReference{value,place,..}=>Some(view_liveness::Transfer::Owned{value,place:Some(place)}),
                Argument::BorrowedData(reference)=>Some(view_liveness::Transfer::BorrowedData(reference)),
                _=>None,
            }
        }).collect::<Vec<_>>();
        let mut retained=view::Borrow::default();
        for(name,binding)in &self.env{if !scoped.contains(name.as_str()){retained.union(&self.view_uses.borrow(self,name,binding,&transferred)?);}}
        for argument in &arguments{if let Argument::Reference(owner)=argument{retained.union(&owner.proof.typed_view_borrow(owner.place.ty()));}}
        for(param,argument)in (0..parameters.len()).zip(&arguments){if parameters.access(param)==Access::Value{
            let resources=match argument{Argument::ApplicationData(data)=>vec![data],Argument::OwnedValue(value)=>value.resources(),_=>Vec::new()};
            for data in resources{self.check_view_requirement(view::Requirement::Unborrowed{resource:self.owned_resource_borrow(data),borrow:retained.clone(),ownership:true,span})?;}
        }}
        self.view_uses.invocation(span);
        for (param, argument) in (0..parameters.len()).zip(&arguments) {
            if let Argument::OwnedReference{value,..}=argument && parameters.access(param).writes() {
                for data in value.resources(){self.check_view_requirement(view::Requirement::Unborrowed{resource:self.data_value_borrow(data),borrow:retained.clone(),ownership:true,span})?;}
                if parameters.access(param)==Access::InOut{for data in value.resources(){self.application_handle_available(data.root.identity(),span,&[],&self.path)?;}}
            }
            if let Argument::BorrowedData(reference) = argument {
                if parameters.access(param).writes() {
                    self.check_view_requirement(view::Requirement::Unborrowed{resource:self.data_borrow(&reference.sources),borrow:retained.clone(),ownership:false,span})?;
                    for path in view::resource_paths(reference.element_layout().1){
                        let mut fields=reference.projection.clone();fields.extend(path);
                        let resource=view::Borrow::element(self.data_value_borrow(&reference.data),fields,None);
                        self.check_view_requirement(view::Requirement::Unborrowed{resource,borrow:retained.clone(),ownership:true,span})?;
                    }
                }
                for source in &reference.sources {
                    self.application_handle_available(*source, span, &[],&self.path)?;
                }
            }
            if let Argument::Reference(OwnedBinding{place,..}) | Argument::Atomic(place) = argument
                && let Place::ApplicationElement { sources, .. } = place.storage()
            {
                for source in sources {
                    self.application_handle_available(*source, span, &[],&self.path)?;
                }
            }
            let handle = match argument {
                Argument::ApplicationData(data) | Argument::Scoped { value:returns::Returned::Dataset(data), .. }
                    if parameters.access(param) != Access::Value =>
                {
                    Some(data.root.identity())
                }
                Argument::Reference(OwnedBinding{place,..}) | Argument::Atomic(place) => match place.storage() {
                    Place::ApplicationElement { identity, .. } => Some(*identity),
                    _ => None,
                },
                _ => None,
            };
            if let Some(handle) = handle {
                if parameters.access(param).writes() { self.check_view_requirement(view::Requirement::Unborrowed{resource:self.data_borrow(&[handle]),borrow:retained.clone(),ownership:false,span})?; }
                self.application_handle_available(handle, span, &[],&self.path)?;
            }
        }
        self.check_argument_aliases(|index|(parameters.access(index),parameters.form(index)==DeclaredArgumentForm::Dataset), &arguments, argument_span)?;
        Ok(Some(arguments))
    }
    pub(super) fn check_argument_aliases(&self, parameter: impl Fn(usize)->(Access,bool), arguments: &[Argument], span: impl Fn(usize) -> Span) -> Result<()> {
        for (index, argument) in arguments.iter().enumerate()
        {
            let (access,dataset)=parameter(index);
            for (other_index,before) in arguments.iter().take(index).enumerate() {
                let (other_access,other_dataset)=parameter(other_index);
                if (matches!(access, Access::Write | Access::InOut)
                    || matches!(other_access, Access::Write | Access::InOut)
                    || (dataset && access == Access::Value)
                    || (other_dataset && other_access == Access::Value)
                    || (dataset
                        && other_dataset
                        && ((access == Access::Atomic && other_access != Access::Atomic)
                            || (other_access == Access::Atomic && access != Access::Atomic)))
                    || (access==Access::Value && matches!(argument, Argument::OwnedValue(_)))
                    || (other_access==Access::Value && matches!(before, Argument::OwnedValue(_))))
                    && argument.overlaps(before,self)
                {
                    return self.fail(
                        span(index),
                        "B227",
                        "同一次调用的独占借用不能重叠，也不能与同一存储的只读或原子借用重叠",
                    );
                }
            }
        }
        Ok(())
    }
    pub(super) fn invoke_instance(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
        syntax: Option<&Expressions>,
    ) -> Result<Option<Value>> {
        let result = self.invoke_returned(instance, args, span, None, syntax)?;
        self.ordinary_return(result, span)
    }
    pub(super) fn invoke_returned(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
        generated_origin: Option<Arc<GeneratedOrigin>>,
        syntax: Option<&Expressions>,
    ) -> Result<Option<returns::Returned>> {
        if instance.definition.function.external().is_some() {
            return self
                .external_call(&instance, args, span, generated_origin)
                .map(|v| v.map(returns::Returned::value));
        }
        if instance.definition.compiled.is_some()
            && instance.definition.execution != crate::execution::Requirement::Domain
        {
            if self.application.is_none() && self.program.target != Target::Cpu {
                return self.fail(span, "B161", "已编译普通CPU模块调用需要CPU执行目标");
            }
            return self.native_function(instance, args, span, generated_origin, syntax, None);
        }
        if instance.definition.execution == crate::execution::Requirement::Domain {
            if self.application_entry() {
                if instance.definition.function.target() == Target::Cpu
                    && !super::kernel_interface(self.world, &instance)?
                {
                    return self.native_function(instance, args, span, generated_origin, syntax, None);
                }
                return self.application_execute(DomainInput::Declaration(&instance), args, Target::Cpu, span, &[]);
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
                    self.native_function(instance, args, span, generated_origin, syntax, None)
                } else {
                    self.inline_instance(instance, args, span, syntax)
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
        let Some(arguments) = self.function_arguments(&key, args, span)? else {
            return Ok(None);
        };
        let explicit=explicit.map(|arguments|self.scope.bindings().arguments(self.world,arguments,
            (self.scope.source(),self.scope.module(),self.scope.imports()),span)).transpose()?;
        let instance = self.specialized_arguments(key, &arguments, explicit.as_deref(), span)?;
        let result = self.inline_instance(instance, arguments, span, Some(args))?;
        self.ordinary_return(result, span)
    }
    pub(super) fn inline_function(
        &mut self,
        key: String,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<Value>> {
        if args.len() != self.world.functions[&key].function.params().len() {
            return self.fail(span, "B162", "函数参数数量不匹配");
        }
        let instance = self.specialized_function(key, &args, span)?;
        let result = self.inline_instance(instance, args, span, None)?;
        self.ordinary_return(result, span)
    }
    pub(super) fn inline_instance(
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
                return self.native_function(instance, args, span, None, syntax, None);
            }
            return self.fail(span, "B160", "当前设备函数不允许递归调用");
        }
        let key = instance.name.clone();
        let callee = &instance.definition;
        let result_name=instance.bindings.type_name(self.world,&callee.function.result(),
            (&callee.source,&callee.module,&callee.imports),callee.function.span())?;
        let mut env = BTreeMap::new();
        let mut borrowed = Vec::new();
        let mut held = Vec::new();
        for (parameter,(param, arg)) in instance.parameters(self.world).zip(args).enumerate() {
            let param=param?;
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
                self.texture_parameter(&param, &index,(&callee.source,&callee.module,&callee.imports),span)?;
                env.insert(param.name.to_owned(), Binding::Texture(index));
                continue;
            }
            let ty = parameter_type(self.world,&param,(&callee.source,&callee.module,&callee.imports,callee.function.kind()),self.program.target)?;
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
            env.insert(param.name.to_owned(), binding);
        }
        let requirements = contracts::resolve(self,(&instance).into(),&LexicalScope::Function(instance.clone()),&env,&BTreeMap::new())?;
        contracts::prove(self, &requirements, span)?;
        contracts::assume(self,(&instance).into(),&mut env,&requirements)?;
        let old_env = std::mem::replace(&mut self.env, env);
        let old_mutable_names = std::mem::take(&mut self.mutable_names);
        let callee_scope = LexicalScope::Function(instance.clone());
        assigned_names(
            self.world,
            &callee_scope,
            callee.function.body().as_ref().unwrap(),
            &mut self.mutable_names,
        )?;
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
            let ty = self.expected_storage(&result_name, callee.function.span())?;
            !ty.contains_borrows() && !ty.owns_resources()
        };
        let caller_initialization = local_initialization.then(|| std::mem::take(&mut self.initialized));
        self.materialize_array_bindings();
        let output_inputs=outputs::inputs(self,&instance,&self.env.clone())?;
        // With no fact or resource inputs/outputs, private range paths end at this call.
        let local_bounds = self.env.values().all(|binding| matches!(binding,
            Binding::Value(value) if !value.ty.contains_borrows() && !value.ty.owns_resources()
                && bounds::Proof::fact_components(&value.ty).next().is_none()
        )) && if result_name.name == "void" || result_name.is_never() {
            true
        } else {
            let ty = self.expected_storage(&result_name, callee.function.span())?;
            !ty.contains_borrows() && !ty.owns_resources()
                && bounds::Proof::fact_components(&ty).next().is_none()
        };
        let caller_path = std::mem::take(&mut self.path);
        let (mut result,_) = self.function_body(&instance, callee.function.body().as_ref().unwrap())?;
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
                updates.push((self.borrow_owned(place.clone()),outputs::guards(self,&instance,*index,place.ty(),&output_inputs,returned.as_ref())?));
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
                self.assign_owned_proof(&owner.place, owner.proof, owner.uniform)?;
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
        for owner in held { self.finish_scoped_argument(&owner,span)?; }
        Ok(result)
    }
}
