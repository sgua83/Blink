//! 普通 CPU 函数拥有独立调用帧；编译中的符号声明截断递归展开。
use super::*;
use cooperative::Argument;
use returns::Returned;

// Only source vectors admit a different component placement with the same type.
// Aggregate fields and arrays retain their declared layout.
pub(super) fn reference_parameters(ty: &Ty) -> Vec<Ty> {
    if ty.array.is_none()
        && ty.fields.is_empty()
        && ty.view.is_none()
        && ty.dataset.is_none()
        && !ty.has_variants()
        && !ty.enumeration
        && ty.components.len() > 1
    {
        ty.components
            .iter()
            .map(|(scalar, _)| Ty::scalar(*scalar))
            .collect()
    } else {
        vec![ty.clone()]
    }
}
#[derive(Default)]
pub(super) struct Module {
    units: BTreeMap<String, Option<Unit>>,
}
impl Module {
    #[inline(never)]
    fn declare(&mut self, name: &str) -> bool {
        if self.units.contains_key(name) {
            return false;
        }
        self.units.insert(name.to_owned(), None);
        true
    }
}
#[derive(Clone)]
pub(crate) struct Contract {
    parameters: Vec<CallParameter>,
    result: Option<Ty>,
    inputs: Vec<Reg>,
    returned: Option<Returned>,
    written: Vec<Option<Returned>>,
    requires: BTreeSet<(ContractKind, ContractPredicate)>,
    target: Target,
    frame: bool,
    pub(super) domain: Option<crate::module::domain::Interface>,
    residency: residency::Requirements,
    view_access:BTreeSet<view_contracts::Access>,
}
crate::module::codec::record!(Contract {
    parameters,
    result,
    inputs,
    returned,
    written,
    requires,
    target,
    frame,
    domain,
    residency,
    view_access
});
struct Unit {
    function: Option<FunctionBody>,
    contract: Contract,
    module: String,
    public: bool,
    residency: residency::Inputs,
    source: Source,
    span: Span,
    view_model: view_contracts::Function,
}
fn declaration(
    world: &World,
    instance: &crate::generics::Instance,
    mut context: application::Context,
    mut functions: Module,
) -> Result<(application::Context, Module)> {
    if !functions.declare(&instance.name) {
        return Ok((context, functions));
    }
    if let Some(definitions) = &instance.definition.compiled {
        return compiled_declaration(instance, definitions.get(&instance.name).map(Vec::as_slice), context, functions);
    }
    let frame = context.frame.take();
    let mut instance = instance.clone();
    if instance.definition.function.target == Target::Compute {
        Arc::make_mut(&mut instance.definition).function.target = Target::Cpu;
    }
    let checked = checked_function(world, &instance, Some(context), functions)?;
    Ok(finish_declaration(&instance, frame, checked))
}

#[inline(never)]
fn compiled_declaration(
    instance: &crate::generics::Instance,
    bytes: Option<&[u8]>,
    context: application::Context,
    mut functions: Module,
) -> Result<(application::Context, Module)> {
    use crate::module::codec::Decode;
    let bytes = bytes.ok_or_else(|| Diagnostic {
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
    functions.units.insert(
        instance.name.clone(),
        Some(Unit {
            function: None,
            contract,
            module: instance.definition.module.clone(),
            public: instance.definition.public,
            residency: residency::Inputs::default(),
            source: instance.definition.source.clone(),
            span: instance.definition.function.span,
            view_model: view_contracts::Function::default(),
        }),
    );
    Ok((context, functions))
}

#[inline(never)]
fn finish_declaration(
    instance: &crate::generics::Instance,
    frame: Option<Vec<Reg>>,
    checked: Box<CheckedFunction>,
) -> (application::Context, Module) {
    let residency=checked.residency;
    let mut functions = checked.functions;
    let mut context = checked.application.unwrap();
    context.frame = frame;
    let mut function = checked.body;
    let contract = Contract {
        parameters: function.parameters.clone(),
        result: function.result.clone(),
        inputs: checked.inputs,
        returned: checked.returned,
        written: checked.written,
        requires: checked.contracts,
        target: instance.definition.function.target,
        frame: false,
        domain: None,
        residency: BTreeMap::new(),
        view_access:BTreeSet::new(),
    };
    crate::optimize::value_code(&mut function.body);
    functions.units.insert(
        instance.name.clone(),
        Some(Unit {
            function: Some(function),
            contract,
            module: instance.definition.module.clone(),
            public: instance.definition.public,
            residency,
            source: instance.definition.source.clone(),
            span: instance.definition.function.span,
            view_model: checked.view_model,
        }),
    );
    (context, functions)
}
// 合同中的值编号属于接口，不进入实际IR寄存器表或产生转换指令。
pub(super) fn rebind_model(value: &mut Returned, registers: Vec<Reg>) {
    match value {
        Returned::Unit => {}
        Returned::Value(value) => value.regs = registers,
        Returned::Dataset(data) => {
            data.handle.regs = vec![registers[0]];
            data.length.regs = vec![registers[1]];
            data.place = None;
            data.initialization = None;
        }
        Returned::OwnedValue(value) => {
            value.value.regs = registers;
            value.place = None;
            for (index, payload) in value.payloads.iter_mut().enumerate() {
                if let Some(payload) = payload {
                    let (offset, ty) = if value.value.ty.has_variants() {
                        (
                            ir::align_up(4, value.value.ty.align),
                            value.value.ty.variants[index].ty.as_ref(),
                        )
                    } else {
                        value.value.ty.aggregate_part(index)
                    };
                    let registers = ty
                        .components
                        .iter()
                        .map(|(_, at)| {
                            let component = value
                                .value
                                .ty
                                .components
                                .iter()
                                .position(|(_, word)| *word == offset + at)
                                .unwrap();
                            value.value.regs[component]
                        })
                        .collect();
                    rebind_model(payload, registers);
                }
            }
        }
    }
}
pub(super) fn canonical(builder:&mut Builder<'_>,returned:Option<Returned>,inputs:&[Reg],written:Vec<Option<Returned>>)->(Option<Returned>,Vec<Reg>,Vec<Option<Returned>>) {
    builder.view_model.input_width=inputs.len();
    let mut bindings=inputs.iter().enumerate().map(|(index,reg)|(ValueIdentity::Register(*reg),bounds::Proof::default().with_identity(ValueIdentity::Register(index)))).collect::<BTreeMap<_,_>>();
    for(parameter,p)in builder.definition.function.params.iter().enumerate(){
        if matches!(p.access,Access::Read|Access::InOut) && let Some(place)=builder.env.get(&p.name).and_then(Binding::stored_place){
            for component in 0..place.ty().components.len(){let id=ValueIdentity::ParameterContent(parameter,component);bindings.insert(id,bounds::Proof::default().with_identity(id));}
        }
        if let Some(Binding::OwnedValue(value))=builder.env.get(&p.name){for resource in 0..view::resource_paths(&value.value.ty).len(){let id=ValueIdentity::ResourceParameter(parameter,resource);bindings.insert(id,bounds::Proof::default().with_identity(id).with_view_sources(0,BTreeSet::from([view::Source::Data(id)])));}}
    }
    let output_aliases=std::mem::take(&mut builder.view_model.parameters);
    if let Some(value)=returned.as_ref().and_then(Returned::view_value){builder.view_model.capture(&value.proof,&value.ty,view_contracts::Output::Returned);}
    for(index,model)in written.iter().enumerate(){if let Some(value)=model.as_ref().and_then(Returned::view_value){builder.view_model.capture(&value.proof,&value.ty,|component|view_contracts::Output::Written(index,component));}}
    let sources=returned.iter().chain(written.iter().flatten()).filter_map(Returned::view_value).flat_map(|value|value.proof.typed_view_borrow(&value.ty).sources).collect::<BTreeSet<_>>();
    for source in sources{if let view::Source::Parameter(id)=source && let Some(proof)=bindings.get_mut(&id) && let Some(target)=proof.known_identity(0){*proof=proof.clone().with_view_sources(0,BTreeSet::from([view::Source::Parameter(target)]));}}
    if let Some(value)=&returned{builder.view_model.capture_resources(value,view_contracts::Output::ReturnedResource);}
    for(parameter,value)in written.iter().enumerate(){if let Some(value)=value{builder.view_model.capture_resources(value,|resource|view_contracts::Output::WrittenResource(parameter,resource));}}
    let mut next=inputs.len();
    let models=std::iter::once(returned).chain(written).enumerate().map(|(model_index,model)|model.map(|model|{
        let proof=bounds::Proof::capture(&builder.returned_storage(&model));let regs=model.registers();
        let mut value=model.clone();rebind_model(&mut value,(next..next+regs.len()).collect());
        for(component,(source,target))in regs.into_iter().zip(value.registers()).enumerate(){
            let bound=bounds::Proof::default().with_identity(ValueIdentity::Register(target));
            bindings.entry(ValueIdentity::Register(source)).or_insert_with(||bound.clone());
            if let Some(id)=proof.known_identity(component){bindings.entry(id).or_insert(bound);}
        }
        // 拥有叶的存储身份与其输出分量共同命名；不把私有位置身份丢成未知。
        let mut original=model.clone();
        for resource in 0..view_contracts::resource_borrows(&model).len(){
            if let(Some(source),Some(target))=(view_contracts::resource_value_mut(&mut original,resource),view_contracts::resource_value_mut(&mut value,resource)){
                let output=if model_index==0{view_contracts::Output::ReturnedResource(resource)}else{view_contracts::Output::WrittenResource(model_index-1,resource)};
                builder.view_model.resource_names.insert(output,ValueIdentity::Register(target.handle.regs[0]));
                let bound=bounds::Proof::default().with_identity(ValueIdentity::Register(target.handle.regs[0]));
                bindings.entry(source.handle.identity()).or_insert_with(||bound.clone());
                bindings.entry(ValueIdentity::Register(source.handle.regs[0])).or_insert(bound);
            }
        }
        next+=value.registers().len();(value,model)
    })).collect::<Vec<_>>();
    let mut names=bindings.iter().filter_map(|(source,proof)|proof.known_identity(0).map(|target|(*source,target))).collect::<BTreeMap<_,_>>();
    for(source,target)in output_aliases{if let Some(target)=names.get(&target).copied(){names.insert(source,target);}}
    builder.view_model.parameters=names;
    let mut models=models.into_iter().map(|pair|pair.map(|(mut value,model)|{translate_returned(&mut value,&model,&bindings);value}));
    (models.next().unwrap(),(0..inputs.len()).collect(),models.collect())
}
pub(super) fn check(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let (_, mut functions) = declaration(world, instance, application::Context::default(), Module::default())?;
    resolve_residencies(&mut functions)
}
impl Builder<'_> {
    fn native_declaration(&mut self, instance: &crate::generics::Instance) -> Result<()> {
        let context = self.application.take().unwrap();
        let (context, functions) = declaration(self.world, instance, context, std::mem::take(&mut self.functions))?;
        self.application = Some(context);
        self.functions = functions;
        Ok(())
    }
    pub(super) fn native_function(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<Returned>> {
        if instance.definition.function.target == Target::Gpu {
            return self.fail(span, "B161", "普通 CPU 调用不能改变函数声明的实际目标");
        }
        self.native_declaration(&instance)?;
        self.native_call(instance, args, span)
    }
    fn native_dataset_shape(&self,param:&Param,definition:&Definition,count:Option<u32>,span:Span)->Result<()>{
        if param.ty.count.is_some()&&self.world.shape(&param.ty.count,&definition.source,param.span,&definition.module,&definition.imports)?!=count{
            return self.fail(span,"B167","调用方没有声明函数需要的数据形状");
        }
        Ok(())
    }
    #[inline(never)]
    fn native_call(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
    ) -> Result<Option<Returned>> {
        let d = &instance.definition;
        let frame = self.application_frame(span)?;
        let mut arguments = vec![CallArgument::Value {
            ty: frame.ty.clone(),
            values: frame.regs.clone(),
        }];
        let mut actual = frame
            .regs
            .iter()
            .map(|reg| Some(bounds::Proof::default().with_identity(ValueIdentity::Register(*reg))))
            .collect::<Vec<_>>();
        let mut env = BTreeMap::new();
        let mut locations = residency::Arguments::new();
        let mut layouts = residency::Layouts::new();
        let mut written = Vec::new();
        let mut written_data = Vec::new();
        let mut held = Vec::new();
        let mut contents = BTreeMap::new();
        let mut resource_inputs=BTreeMap::new();
        for (index, (p, arg)) in d.function.params.iter().zip(args).enumerate() {
            let ty = cooperative::parameter_type(self.world, p, d)?;
            let arg=match arg {Argument::Scoped{value,owner}=>{held.push(owner);Argument::resource(value)},other=>other};
            let arg=match arg {
                Argument::ApplicationData(data) if p.ty.dataset&&p.access==Access::Read=>Argument::BorrowedData(application::DataReference::direct(data)),
                other=>other,
            };
            let arg=match arg {
                Argument::BorrowedData(mut reference)=>{
                    residency::layout_argument(&mut layouts,index,reference.layout_source());
                    reference.data.handle.proof=reference.data.handle.proof.with_view_borrow(0,self.data_borrow(&reference.sources));
                    if p.access==Access::Read{Argument::BorrowedData(reference)}else{
                        self.application_layout_requirement(&reference,residency::LayoutNeed::Whole,span)?;
                        if p.access.writes(){for source in &reference.sources{self.invalidate_application_contents(*source);}}
                        Argument::ApplicationData(reference.data)
                    }
                },
                other=>other,
            };
            let arg=match arg {Argument::Value(value) if p.access==Access::Read=>Argument::Reference(self.materialize_array(value)),other=>other};
            match arg {
                Argument::Value(mut value) if p.access == Access::Value && !p.ty.dataset => {
                    self.require_same(&ty, &value.ty, span)?;
                    value.literal = false;
                    append_actual(&mut actual, &value);
                    arguments.push(CallArgument::Value {
                        ty: value.ty.clone(),
                        values: value.regs.clone(),
                    });
                    env.insert(p.name.clone(), Binding::Value(value));
                }
                Argument::OwnedReference{mut value,place,owner} if matches!(p.access,Access::Write|Access::InOut)=>{
                    self.require_same(&ty,&value.value.ty,span)?;
                    if let Some(owner)=owner{held.push(owner);}
                    for(part,address)in self.native_references(&place,p.access,span)?{arguments.push(CallArgument::Reference{ty:part,access:p.access,address});actual.extend([None,None]);}
                    if p.access==Access::InOut {
                        for data in value.resources(){self.require_application_initialized(data,span)?;}
                        self.load_owned_value(&mut value,span)?;
                        let proof=bounds::Proof::capture(&value.value);
                        for component in 0..ty.components.len(){contents.insert(ValueIdentity::ParameterContent(index,component),proof.project(&[component],ty.components.len()));}
                        residency::owned_argument(&mut locations,index,&mut value);
                        for(resource,path)in view::resource_paths(&ty).into_iter().enumerate(){let borrow=match view::owned_data_at(&value,&path){Some(data)=>self.data_value_borrow(data),None=>view::Borrow::from_sources(BTreeSet::from([view::Source::Inactive]))};resource_inputs.insert(ValueIdentity::ResourceParameter(index,resource),bounds::Proof::default().with_view_borrow(0,borrow));}
                    }else{self.drop_returned(Returned::OwnedValue(value.clone()),&view::Borrow::default(),span)?;}
                    env.insert(p.name.clone(),Binding::OwnedValue(value));written.push((index,place));
                }
                Argument::Reference(owner) if matches!(p.access, Access::Read | Access::Write | Access::InOut) => {
                    let place=owner.place.clone();
                    self.require_same(&ty, place.ty(), span)?;
                    for (part, address) in self.native_references(&place, p.access, span)? {
                        arguments.push(CallArgument::Reference {
                            ty: part,
                            access: p.access,
                            address,
                        });
                        // Actual pointer inputs are private to the callee, never caller scalar facts.
                        actual.extend([None, None]);
                    }
                    if p.access.reads() {
                        let width = ty.components.len();
                        for component in 0..width {
                            contents.insert(
                                ValueIdentity::ParameterContent(index, component),
                                owner.proof.project(&[component], width),
                            );
                        }
                    }
                    env.insert(p.name.clone(), Binding::Owned(owner));
                    if p.access.writes() { written.push((index, place)); }
                }
                Argument::BorrowedData(reference) if p.ty.dataset&&p.access==Access::Read=>{
                    residency::argument(&mut locations,index,&reference.data);
                    self.require_same(&ty,reference.element_layout().1,span)?;
                    self.require_application_reference_initialized(&reference,span)?;
                    self.native_dataset_shape(p,d,reference.count,span)?;
                    let value=self.application_reference_value(&reference);
                    append_actual(&mut actual,&value);
                    arguments.push(CallArgument::Value{ty:value.ty,values:value.regs});
                    env.insert(p.name.clone(),Binding::DataReference(reference));
                }
                Argument::ApplicationData(mut data) if p.ty.dataset => {
                    residency::layout_argument(&mut layouts,index,residency::LayoutSource::Declared{stride:ir::align_up(data.ty.size,data.ty.align),projected:false});
                    residency::argument(&mut locations, index, &data);
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
                    self.native_dataset_shape(p,d,data.count,span)?;
                    if matches!(p.access,Access::Write|Access::InOut) && outputs::declared(d,index){self.reset_data_output(&mut data,span)?;}
                    if p.access == Access::Write || p.access==Access::InOut && outputs::declared(d,index) {
                        written_data.push((index,data.clone()));
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
                Argument::OwnedValue(mut value)
                    if matches!(p.access,Access::Value|Access::Read) && ty.owns_resources() =>
                {
                    self.require_same(&ty, &value.value.ty, span)?;
                    if p.access==Access::Read {
                        value.borrow();
                    }else if !value.all_available(){return self.fail(span,"B227","按值参数需要实际拥有全部活动资源");}
                    for data in value.resources(){self.require_application_initialized(data,span)?;}
                    value.resources_mut(&mut |data|data.initialization=None);
                    residency::owned_argument(&mut locations, index, &mut value);
                    for(resource,path)in view::resource_paths(&value.value.ty).into_iter().enumerate(){let borrow=match view::owned_data_at(&value,&path){Some(data)=>self.data_value_borrow(data),None=>view::Borrow::from_sources(BTreeSet::from([view::Source::Inactive]))};resource_inputs.insert(ValueIdentity::ResourceParameter(index,resource),bounds::Proof::default().with_view_borrow(0,borrow));}
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
        let output_inputs=outputs::inputs(self,d,&env)?;
        let mut view_arguments=actual.iter().enumerate().filter_map(|(index,proof)|proof.clone().map(|proof|(ValueIdentity::Register(index),proof))).collect::<BTreeMap<_,_>>();
        view_arguments.extend(contents.clone());view_arguments.extend(resource_inputs.clone());
        if !locations.is_empty() {
            let call = residency::Call {
                function: instance.name.clone(),
                arguments: locations.clone(),
                layouts,
                bindings: view_arguments.clone(),
                path: self.residency_path(),
                source: self.definition.source.clone(),
                span,
            };
            self.residency.calls.push(call);
        }
        let result = if d.function.result.name == "void" || d.function.result.is_never() {
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
        if d.function.result.is_never() {
            self.flow = Flow::STOP;
            return Ok(None);
        }
        let prototype = self.functions.units[&instance.name]
            .as_ref()
            .map(|unit| {
                (
                    unit.contract.inputs.clone(),
                    unit.contract.returned.clone(),
                    unit.contract.written.clone(),
                )
            });
        let input_width=actual.len();
        let (mut bindings, model, writes) = if let Some((inputs, model, writes)) = prototype {
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
        bindings.extend(contents);bindings.extend(resource_inputs);
        if let Some(model) = &model {
            for (source, target) in model.registers().into_iter().zip(&values) {
                bindings.insert(
                    ValueIdentity::Register(source),
                    bounds::Proof::default().with_identity(ValueIdentity::Register(*target)),
                );
            }
        }
        let mut written_registers=BTreeMap::new();let mut semantic=input_width+values.len();
        // 所有输出位置沿原合同编号映射；这些编号不是机器参数或元数据副本。
        if model.is_none(){for(component,reg)in values.iter().enumerate(){bindings.insert(ValueIdentity::Register(semantic-values.len()+component),bounds::Proof::default().with_identity(ValueIdentity::Register(*reg)));}}
        for(index,place)in &written {
            let regs=place.ty().components.iter().map(|(scalar,_)|self.reg(*scalar)).collect::<Vec<_>>();
            let contract=writes.get(*index).and_then(Option::as_ref);
            let ids=contract.map(Returned::registers).unwrap_or_else(||(semantic..semantic+regs.len()).collect());
            for(source,target)in ids.into_iter().zip(&regs){bindings.insert(ValueIdentity::Register(source),bounds::Proof::default().with_identity(ValueIdentity::Register(*target)));}
            if let Some(model)=contract {let proof=bounds::Proof::capture(&self.returned_storage(model));for(component,reg)in regs.iter().enumerate(){if let Some(id)=proof.known_identity(component){bindings.entry(id).or_insert_with(||bounds::Proof::default().with_identity(ValueIdentity::Register(*reg)));}}}
            semantic+=regs.len();written_registers.insert(*index,regs);
        }
        view_arguments.extend(bindings.clone());
        let view_call=Some(self.view_model.call(&instance.name,view_arguments.clone(),self.path.clone(),self.definition.source.clone(),span));
        let output_value=result.as_ref().map(|ty|Value{ty:ty.clone(),regs:values.clone(),
            proof:model.as_ref().map(|model|bounds::Proof::capture(&self.returned_storage(model)).at_call(&bindings)).unwrap_or_default(),
            uniform:false,literal:false,constant:None});
        for (index, place) in written {
            let guards=outputs::guards(self,d,index,place.ty(),&output_inputs,output_value.as_ref())?;
            if outputs::declared(d,index) {self.forget_output_initialization(&place,span)?;}
            let mut proof = writes
                .get(index)
                .and_then(Option::as_ref)
                .map(|model| bounds::Proof::capture(&self.returned_storage(model)).at_call(&bindings))
                .unwrap_or_default();
            if let Some(call)=view_call {for component in view_contracts::components(place.ty()) {
                proof=proof.with_view_borrow(component,self.view_model.result(call,view_contracts::Output::Written(index,component)));
            }}
            if place.ty().owns_resources(){
                let regs=written_registers[&index].clone();
                let mut result=self.template_returned(Value{ty:place.ty().clone(),regs,proof:proof.clone(),uniform:false,literal:false,constant:None},true);
                if let Some(Some(model))=writes.get(index){translate_returned(&mut result,model,&bindings);residency::returned(&mut result,&locations);}else{residency::output(&mut result,&instance.name,residency::OutputKind::Written(index),&locations,&view_arguments);}
                if let Some(call)=view_call{view_contracts::set_resource_borrows(&mut result,|resource|self.view_model.result(call,view_contracts::Output::WrittenResource(index,resource)));}
                if outputs::declared(d,index){outputs::apply_ownership(&mut result,place.ty(),0,&guards);}
                let Returned::OwnedValue(mut value)=result else{unreachable!()};proof=bounds::Proof::capture(&value.value);value.value.proof=proof.clone();self.assign_owned_place(&place,Returned::OwnedValue(value));
            }
            self.assign_owned_proof(&place, proof, false);
            self.initialize_output(&place,&guards);
        }
        for (index,data) in written_data {
            let guards=outputs::guards(self,d,index,&data.ty,&output_inputs,output_value.as_ref())?;
            self.initialize_data_output(&data,&guards);
        }
        let Some(ty) = result else {
            self.view_uses.after(span);
            for name in held { self.finish_scoped_argument(&name, span)?; }
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
            residency::returned(&mut returned, &locations);
        } else {
            residency::output(&mut returned, &instance.name,residency::OutputKind::Returned, &locations,&view_arguments);
        }
        if let Some(call)=view_call{view_contracts::set_resource_borrows(&mut returned,|resource|self.view_model.result(call,view_contracts::Output::ReturnedResource(resource)));}
        if let (Some(call),Some(value))=(view_call,returned.view_value_mut()) {
            for component in view_contracts::components(&value.ty){value.proof=value.proof.clone().with_view_borrow(component,self.view_model.result(call,view_contracts::Output::Returned(component)));}
        }
        self.view_uses.after(span);
        let borrow=returned.view_value().map(|value|value.proof.typed_view_borrow(&value.ty)).unwrap_or_default();
        for name in held {
            let resources=self.env.get(&name).into_iter().flat_map(Binding::application_resources).filter(|data|data.ownership==application::Ownership::Owned).map(|data|data.handle.identity()).collect::<Vec<_>>();
            for identity in resources {
                let resource=self.data_borrow(&[identity]);
                self.check_view_requirement(view::Requirement::Unborrowed{resource,borrow:borrow.clone(),span})?;
            }
            self.finish_scoped_argument(&name, span)?;
        }
        if let Some(value)=returned.view_value() { self.require_view_available(value,span)?; }
        Ok(Some(returned))
    }
}
fn append_actual(actual: &mut Vec<Option<bounds::Proof>>, value: &Value) {
    let proof = bounds::Proof::capture(value);
    for i in 0..value.regs.len() {
        actual.push(Some(proof.project(&[i], value.regs.len())));
    }
}
fn translate_value(
    value: &mut Value,
    model: &Value,
    bindings: &BTreeMap<ValueIdentity, bounds::Proof>,
) {
    // 合同的未知来源保持未知；模型保留的寄存器不是本次真实返回值。
    value.proof = model.proof.clone().at_call(bindings);
    value.constant = model
        .constant
        .clone()
        .or_else(|| value.proof.known_value(&value.ty));
    value.uniform = false;
}
fn translate_returned(
    value: &mut Returned,
    model: &Returned,
    bindings: &BTreeMap<ValueIdentity, bounds::Proof>,
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
            value.residency = model.residency.at_call(bindings);
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
fn view_outputs(contract:&Contract)->view_contracts::Interface {
    let mut outputs=BTreeMap::new();
    if let Some(value)=contract.returned.as_ref().and_then(Returned::view_value) {
        for component in view_contracts::components(&value.ty){outputs.insert(view_contracts::Output::Returned(component),value.proof.view_borrow(component));}
    }
    // 位置参数的实际类型由原CallParameter携带；值输出合同保留原形参编号。
    for (parameter,model)in contract.written.iter().enumerate() {
        if let Some(value)=model.as_ref().and_then(Returned::view_value){for component in view_contracts::components(&value.ty){outputs.insert(view_contracts::Output::Written(parameter,component),value.proof.view_borrow(component));}}
    }
    if let Some(value)=&contract.returned{for(resource,borrow)in view_contracts::resource_borrows(value).into_iter().enumerate(){outputs.insert(view_contracts::Output::ReturnedResource(resource),borrow);}}
    for(parameter,value)in contract.written.iter().enumerate(){if let Some(value)=value{for(resource,borrow)in view_contracts::resource_borrows(value).into_iter().enumerate(){outputs.insert(view_contracts::Output::WrittenResource(parameter,resource),borrow);}}}
    view_contracts::Interface{outputs,access:contract.view_access.clone()}
}
fn resolve_views(module:&mut Module)->Result<()> {
    let mut functions=BTreeMap::new();let mut closed=BTreeMap::new();
    for (name,unit)in &module.units {
        let unit=unit.as_ref().unwrap();
        if unit.function.is_some(){functions.insert(name.clone(),&unit.view_model);}else{closed.insert(name.clone(),view_outputs(&unit.contract));}
    }
    let outputs=view_contracts::solve(&functions,closed)?;
    for (name,unit)in &mut module.units {
        let contract=&mut unit.as_mut().unwrap().contract;
        let interface=&outputs[name];contract.view_access=interface.access.clone();
        for (output,borrow)in &interface.outputs {
            match *output {
                view_contracts::Output::Returned(component)=>if let Some(value)=contract.returned.as_mut().and_then(Returned::view_value_mut) {value.proof=value.proof.clone().with_view_borrow(component,borrow.clone());},
                view_contracts::Output::Written(parameter,component)=>if let Some(value)=contract.written[parameter].as_mut().and_then(Returned::view_value_mut) {value.proof=value.proof.clone().with_view_borrow(component,borrow.clone());},
                view_contracts::Output::ReturnedResource(resource)=>if let Some(value)=contract.returned.as_mut().and_then(|value|view_contracts::resource_value_mut(value,resource)){value.handle.proof=value.handle.proof.clone().with_view_borrow(0,borrow.clone());},
                view_contracts::Output::WrittenResource(parameter,resource)=>if let Some(value)=contract.written[parameter].as_mut().and_then(|value|view_contracts::resource_value_mut(value,resource)){value.handle.proof=value.handle.proof.clone().with_view_borrow(0,borrow.clone());},
            }
        }
    }
    Ok(())
}
// 函数合同沿真实资源调用关系收敛，递归参数置换也消费同一输入条件。
fn resolve_residencies(module: &mut Module) -> Result<()> {
    resolve_views(module)?;
    let outputs = residency::solve_outputs(module.units.iter().map(|(name, unit)| {
        (
            name.as_str(),
            unit.as_ref().unwrap().contract.returned.as_ref(),
            unit.as_ref().unwrap().contract.written.as_slice(),
        )
    }));
    loop{
        let known=module.units.iter().map(|(name,unit)|(name.clone(),unit.as_ref().unwrap().contract.residency.clone())).collect::<BTreeMap<_,_>>();let mut changed=false;
        for unit in module.units.values_mut(){let unit=unit.as_mut().unwrap();if unit.function.is_none(){continue;}
            let required=residency::infer(&unit.residency,&known,&outputs)?;
            let parameters=residency::input_parameters(&unit.view_model.parameters,unit.contract.inputs.len());
            let required=residency::canonical(&required,&parameters,&unit.source,unit.span)?;
            changed|=required!=unit.contract.residency;unit.contract.residency=required;
        }
        if !changed{break;}
    }
    for unit in module.units.values_mut(){let unit=unit.as_mut().unwrap();
        let parameters=residency::input_parameters(&unit.view_model.parameters,unit.contract.inputs.len());
        residency::check_loops(&unit.residency,&unit.contract.residency,&parameters,&outputs)?;
        for written in &mut unit.contract.written{if let Some(value)=written{if !residency::resolve_returned(value,&outputs){*written=None;}}}
        if let Some(returned)=&mut unit.contract.returned{if !residency::resolve_returned(returned,&outputs){unit.contract.returned=None;}}
    }
    Ok(())
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
    mut module: Module,
    mut reachable: BTreeSet<String>,
    mut root: Option<&mut ValueCode>,
    owner: Option<&str>,
) -> Result<Finalized> {
    use crate::module::codec::Encode;
    resolve_residencies(&mut module)?;
    let mut units = module
        .units
        .into_iter()
        .map(|(name, unit)| (name, unit.unwrap()))
        .collect::<BTreeMap<_, _>>();
    loop {
        let mut next = reachable.clone();
        for name in &reachable {
            let unit=units.get_mut(name).unwrap();
            if owner.is_none_or(|owner|unit.module==owner) {
                if let Some(function)=&mut unit.function {next.extend(dependencies(&mut function.body));}
            }
        }
        if next == reachable {
            break;
        }
        reachable = next;
    }
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
    units.retain(|name, _| reachable.contains(name));
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
    Ok(result)
}
pub(super) fn finish(module: Module, root: &mut ValueCode) -> Result<Finalized> {
    let roots = dependencies(root);
    finalize(module, roots, Some(root), None)
}

pub(crate) fn compile_module(
    world: &World,
    entry: &str,
    requests: &[String],
) -> Result<crate::module::Module> {
    let mut context = application::Context::default();
    let mut functions = Module::default();
    let mut roots = BTreeSet::new();
    for (key, d) in &world.functions {
        crate::generics::validate_declaration(d)?;
        if d.module != entry || d.compiled.is_some() {
            continue;
        }
        for instance in crate::generics::instances(world, key, d)? {
            if d.execution == crate::execution::Requirement::Domain
                && super::kernel_interface(world, &instance.definition)?
            {
                roots.extend(domain_declaration(world, &instance, &mut context, &mut functions)?);
            } else if d.function.external.is_none()
                && d.function.kind == FunctionKind::Value
                && d.function.target != Target::Gpu
            {
                if d.public {
                    roots.insert(instance.name.clone());
                }
                (context, functions) = declaration(world, &instance, context, functions)?;
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
        let instance = super::generics::specialize(
            world,
            &key,
            &[],
            arguments.as_deref(),
            d,
            d.function.span,
        )?;
        if d.execution == crate::execution::Requirement::Domain
            && super::kernel_interface(world, &instance.definition)?
        {
            roots.extend(domain_declaration(world, &instance, &mut context, &mut functions)?);
            continue;
        }
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
        roots.insert(instance.name.clone());
        (context, functions) = declaration(world, &instance, context, functions)?;
    }
    let finalized = finalize(functions, roots, None, Some(entry))?;
    let mut functions = finalized.functions;
    let resources = application::module_resources(&mut functions, context.resources);
    Ok(crate::module::Module {
        compiler: world.worker.compiler().clone(),
        name: entry.into(),
        header: world.interfaces[entry].text.to_string(),
        requirements: world
            .functions
            .iter()
            .filter(|(_, d)| d.module == entry && d.public)
            .map(|(name, d)| (name.clone(), d.execution))
            .collect(),
        functions,
        imports: finalized.imports,
        exports: finalized.exports,
        resources,
    })
}

fn domain_at(
    world: &World,
    instance: &crate::generics::Instance,
    context: &mut application::Context,
    functions: &mut Module,
    target: Target,
) -> Result<crate::module::domain::Interface> {
    let name = crate::module::domain::symbol(&instance.name, target);
    if !functions.units.contains_key(&name) {
        if instance.definition.compiled.is_some() {
            let mut reference = instance.clone();
            reference.name = name.clone();
            (*context, *functions) = declaration(world, &reference, std::mem::take(context), std::mem::take(functions))?;
        } else {
            build_domain(world, instance, context, functions, target)?;
        }
    }
    functions.units[&name]
        .as_ref()
        .unwrap()
        .contract
        .domain
        .clone()
        .ok_or_else(|| Diagnostic {
            source: instance.definition.source.clone(),
            span: instance.definition.function.span,
            code: "B270",
            message: "模块入口缺少整域合同".into(),
        })
}
impl Builder<'_> {
    pub(super) fn prepare_domain(
        &mut self,
        instance: &crate::generics::Instance,
        target: Target,
    ) -> Result<crate::module::domain::Interface> {
        let mut context = self.application.take().unwrap();
        let interface = domain_at(self.world, instance, &mut context, &mut self.functions, target)?;
        self.application = Some(context);
        Ok(interface)
    }
    pub(super) fn call_domain(
        &mut self,
        instance: crate::generics::Instance,
        target: Target,
        arguments: Vec<CallArgument>,
    ) {
        self.code.push(Inst::Call(Call {
            target: CallTarget::Defined(crate::module::domain::symbol(&instance.name, target)),
            arguments,
            result: None,
            values: Vec::new(),
        }));
    }
}
pub(crate) fn check_domain(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    domain_declaration(world, instance, &mut application::Context::default(), &mut Module::default())?;
    Ok(())
}
fn domain_declaration(
    world: &World,
    instance: &crate::generics::Instance,
    context: &mut application::Context,
    functions: &mut Module,
) -> Result<BTreeSet<String>> {
    let targets: &[Target] = match instance.definition.function.target {
        Target::Cpu => &[Target::Cpu],
        Target::Gpu => &[Target::Gpu],
        Target::Compute => &[Target::Cpu, Target::Gpu],
    };
    let mut roots = BTreeSet::new();
    for &target in targets {
        domain_at(world, instance, context, functions, target)?;
        if instance.definition.public {
            roots.insert(crate::module::domain::symbol(&instance.name, target));
        }
    }
    Ok(roots)
}
fn build_domain(
    world: &World,
    instance: &crate::generics::Instance,
    context: &mut application::Context,
    functions: &mut Module,
    target: Target,
) -> Result<()> {
    let d = &instance.definition;
    let expanded = expand_domain(world, instance)?;
    let mut signature = expanded.program;
    if !signature.accelerations.is_empty() || !signature.textures.is_empty() {
        return d.source.fail(
            d.function.span,
            "B270",
            "B应用的独立整域入口尚未交接纹理或加速结构；原生宿主算法接口继续使用已有产物",
        );
    }
    let stages = std::mem::take(&mut signature.stages);
    let mut frame = Ty::vector(Scalar::U32, 2);
    frame.name = "__blink_frame".into();
    let mut registers = vec![Scalar::U32, Scalar::U32];
    let mut parameters = vec![CallParameter::Value(frame.clone())];
    let frame = CallArgument::Value {
        ty: frame,
        values: vec![0, 1],
    };
    let mut data = Vec::new();
    let mut uniform_values = Vec::new();
    let mut uniform_regs = BTreeMap::new();
    let mut ui = 0;
    for param in &d.function.params {
        let ty = cooperative::parameter_type(world, param, d)?;
        let first = registers.len();
        if param.ty.dataset {
            registers.extend([Scalar::U32, Scalar::U32]);
            let count = world.shape(
                &param.ty.count,
                &d.source,
                param.span,
                &d.module,
                &d.imports,
            )?;
            parameters.push(CallParameter::Value(Ty::owned_dataset(ty, count)));
            data.push([first, first + 1]);
        } else {
            registers.extend(ty.components.iter().map(|(s, _)| *s));
            parameters.push(CallParameter::Value(ty.clone()));
            for (i, (scalar, offset)) in ty.components.iter().enumerate() {
                uniform_regs.insert(signature.uniforms[ui].offset + offset, first + i);
                uniform_values.push(CallArgument::Value {
                    ty: Ty::scalar(*scalar),
                    values: vec![first + i],
                });
            }
            ui += 1;
        }
    }
    let inputs: Vec<_> = (0..registers.len()).collect();
    let mut code = Vec::new();
    let mut interface = crate::module::domain::Interface {
        instance: signature.name.clone(), target,
        buffers: signature.buffers.clone(), uniforms: signature.uniforms.clone(),
        outputs: expanded.outputs,
    };
    let mut start = 0;
    let mut required = signature.prefix_requirements.clone();
    let split = !expanded.calls.is_empty();
    let mut part = 0;
    for call in expanded.calls {
        emit_domain_part(
            &world.worker,
            &d.source.path,
            &signature,
            &stages[start..call.stage],
            &required,
            &call.before,
            split,
            &mut part,
            context,
            target,
            &frame,
            &data,
            &uniform_values,
            &mut code,
        )?;
        domain_at(world, &call.instance, context, functions, target)?;
        let mut arguments = vec![frame.clone()];
        for root in &call.arguments {
            arguments.push(match root {
                Root::Data(i, _) => CallArgument::Value {
                    ty: Ty::owned_dataset(
                        signature.buffers[*i].ty.clone(),
                        signature.buffers[*i].count,
                    ),
                    values: data[*i].to_vec(),
                },
                Root::Uniform(ty, offset) => CallArgument::Value {
                    ty: ty.clone(),
                    values: ty
                        .components
                        .iter()
                        .map(|(_, at)| uniform_regs[&(offset + at)])
                        .collect(),
                },
                Root::Literal(ty, words) => {
                    let values = ty
                        .components
                        .iter()
                        .zip(words)
                        .map(|((scalar, _), word)| {
                            let reg = registers.len();
                            registers.push(*scalar);
                            code.push(Inst::Constant(reg, *scalar, *word));
                            reg
                        })
                        .collect();
                    CallArgument::Value {
                        ty: ty.clone(),
                        values,
                    }
                }
                _ => {
                    return d
                        .source
                        .fail(d.function.span, "B270", "该Frame入口尚未交接此资源种类");
                }
            });
        }
        code.push(Inst::Call(Call {
            target: CallTarget::Defined(crate::module::domain::symbol(&call.instance.name, target)),
            arguments,
            result: None,
            values: Vec::new(),
        }));
        start = call.stage;
        required = call.after;
    }
    emit_domain_part(
        &world.worker,
        &d.source.path,
        &signature,
        &stages[start..],
        &required,
        &signature.prefix_guarantees,
        split,
        &mut part,
        context,
        target,
        &frame,
        &data,
        &uniform_values,
        &mut code,
    )?;
    interface.outputs.sort();
    interface.outputs.dedup();
    let name = crate::module::domain::symbol(&instance.name, target);
    let body = ValueCode {
        registers,
        locals: Default::default(),
        inputs: inputs.clone(),
        outputs: Vec::new(),
        code,
    };
    let function = FunctionBody {
        name: name.clone(),
        parameters: parameters.clone(),
        result: None,
        body,
    };
    let contract = Contract {
        parameters,
        result: None,
        inputs,
        returned: None,
        written: vec![None; d.function.params.len()],
        requires: crate::contract_signature(&d.function)
            .into_iter()
            .map(|(kind, predicate)| (kind, predicate.clone()))
            .collect(),
        target: Target::Cpu,
        frame: false,
        domain: Some(interface),
        residency: BTreeMap::new(),
        view_access:BTreeSet::new(),
    };
    functions.units.insert(
        name,
        Some(Unit {
            function: Some(function),
            contract,
            module: d.module.clone(),
            public: d.public,
            residency: residency::Inputs::default(),
            source: d.source.clone(),
            span: d.function.span,
                view_model: view_contracts::Function::default(),
        }),
    );
    Ok(())
}
fn emit_domain_part(
    worker: &crate::compiler::Worker,
    source: &std::path::Path,
    signature: &Program,
    stages: &[Stage],
    required: &[PrefixRelation],
    guaranteed: &[PrefixRelation],
    split: bool,
    part: &mut usize,
    context: &mut application::Context,
    target: Target,
    frame: &CallArgument,
    data: &[[Reg; 2]],
    uniforms: &[CallArgument],
    code: &mut Vec<Inst>,
) -> Result<()> {
    if stages.is_empty() {
        return Ok(());
    }
    let mut program = signature.clone();
    if split {
        program.name = format!("{}@part{}", signature.name, *part);
    }
    *part += 1;
    program.stages = stages.to_vec();
    program.source_stages = stages.len();
    program.prefix_requirements = required.to_vec();
    program.prefix_guarantees = guaranteed.to_vec();
    crate::optimize::optimize(&mut program, worker).map_err(|error| crate::compiler::diagnostic(source, error))?;
    let index = context.resources.programs.len();
    context.resources.programs.push((target, program));
    let mut arguments = vec![frame.clone()];
    arguments.extend(data.iter().map(|regs| CallArgument::Value {
        ty: Ty::scalar(Scalar::U32),
        values: vec![regs[0]],
    }));
    arguments.extend_from_slice(uniforms);
    code.push(application::runtime_call(
        &format!("run_{index}"),
        arguments,
        None,
        Vec::new(),
    ));
    Ok(())
}
