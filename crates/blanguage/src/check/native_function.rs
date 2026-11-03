//! 普通 CPU 函数拥有独立调用帧；编译中的符号声明截断递归展开。
use super::*;
use cooperative::Argument;
use returns::Returned;

// Only source vectors admit a different component placement with the same type.
// Aggregate fields and arrays retain their declared layout.
pub(super) fn reference_parameters(ty: &Ty) -> Vec<Ty> {
    if ty.components.is_empty() { return Vec::new(); }
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
    pub(super) fn writes_only_references(&self,name:&str)->bool {
        fn references(borrow:&view::Borrow)->bool {
            borrow.inputs.is_empty()&&borrow.calls.is_empty()&&borrow.sources.iter().all(|source|match source {
                view::Source::Parameter(ValueIdentity::ParameterStorage(_))=>true,
                view::Source::Position(origin)=>match &origin.root {
                    address::Root::Parameter(_)=>true,
                    address::Root::Derived(borrow)=>references(borrow),
                    _=>false,
                },
                _=>false,
            })
        }
        self.units.get(name).and_then(Option::as_ref).and_then(|unit|unit.contract.view_access.as_ref()).is_some_and(|access|
            access.iter().all(|access|match access {
                view_contracts::Access::Resource{..}|view_contracts::Access::Memory{..}=>false,
                view_contracts::Access::Dataset{..}=>true,
                view_contracts::Access::Position{position,borrows,..}=>!position.writes||references(borrows),
            }))
    }
    #[inline(never)]
    fn declare(&mut self, name: &str) -> bool {
        if self.units.contains_key(name) {
            return false;
        }
        self.units.insert(name.to_owned(), None);
        true
    }
    pub(super) fn into_imports(self) -> BTreeMap<String, Vec<u8>> {
        use crate::module::codec::Encode;
        self.units.into_iter().map(|(name, unit)| (name, unit.unwrap().contract.bytes())).collect()
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
    view_access:Option<BTreeSet<view_contracts::Access>>,
    initialization:Option<Vec<address::Initialization>>,
}
// The wire still stores a vector of guards. Its meaning is one requirement
// whenever any use is active, independent of the number of source use sites.
impl crate::module::codec::Encode for Contract {
    fn encode(&self,w:&mut crate::module::codec::Writer){
        self.parameters.encode(w);self.result.encode(w);self.inputs.encode(w);
        self.returned.encode(w);self.written.encode(w);self.requires.encode(w);
        self.target.encode(w);self.frame.encode(w);self.domain.encode(w);
        self.residency.len().encode(w);
        for(key,guards)in &self.residency{
            key.encode(w);1usize.encode(w);
            guards.iter().cloned().fold(bounds::Condition::constant(false),|a,b|a.logical(b,true)).encode(w);
        }
        self.view_access.as_ref().unwrap().encode(w);self.initialization.as_ref().unwrap().encode(w);
    }
}
impl crate::module::codec::Decode for Contract {
    fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{
        use crate::module::codec::Decode;
        Ok(Self{
            parameters:Decode::decode(r)?,result:Decode::decode(r)?,inputs:Decode::decode(r)?,
            returned:Decode::decode(r)?,written:Decode::decode(r)?,requires:Decode::decode(r)?,
            target:Decode::decode(r)?,frame:Decode::decode(r)?,domain:Decode::decode(r)?,
            residency:Decode::decode(r)?,view_access:Some(Decode::decode(r)?),initialization:Some(Decode::decode(r)?),
        })
    }
}

struct Unit {
    function: Option<FunctionBody>,
    contract: Contract,
    module: String,
    public: bool,
    c_abi: bool,
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
        return Ok((context, compiled_declaration(instance, definitions.get(&instance.name).map(Vec::as_slice), functions)?));
    }
    let frame = context.frame.take();
    let mut instance = instance.clone();
    if instance.definition.function.target == Target::Compute {
        Arc::make_mut(&mut instance.definition).function.target = Target::Cpu;
    }
    let checked = checked_function(world, &instance, Some(context), functions)?;
    finish_declaration(&instance, frame, checked)
}

#[inline(never)]
fn compiled_declaration(
    instance: &crate::generics::Instance,
    bytes: Option<&[u8]>,
    mut functions: Module,
) -> Result<Module> {
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
            c_abi: instance.definition.function.c_abi,
            residency: residency::Inputs::default(),
            source: instance.definition.source.clone(),
            span: instance.definition.function.span,
            view_model: view_contracts::Function::default(),
        }),
    );
    Ok(functions)
}

#[inline(never)]
fn finish_declaration(
    instance: &crate::generics::Instance,
    frame: Option<Vec<Reg>>,
    checked: Box<CheckedFunction>,
) -> Result<(application::Context, Module)> {
    let residency=checked.residency;
    let mut functions = checked.functions;
    let mut context = checked.application.unwrap();
    context.frame = frame;
    let function = checked.body;
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
        view_access:None,
        initialization:checked.initialization,
    };
    functions.units.insert(
        instance.name.clone(),
        Some(Unit {
            function: Some(function),
            contract,
            module: instance.definition.module.clone(),
            public: instance.definition.public,
            c_abi: instance.definition.function.c_abi,
            residency,
            source: instance.definition.source.clone(),
            span: instance.definition.function.span,
            view_model: checked.view_model,
        }),
    );
    resolve_views(&mut functions,Some(&instance.name))?;
    Ok((context, functions))
}
// 合同中的值编号属于接口，不进入实际IR寄存器表或产生转换指令。
pub(super) fn rebind_model(value: &mut Returned, registers: Vec<Reg>) {
    match value {
        Returned::Unit => {}
        Returned::Value(value) => value.regs = registers,
        Returned::Dataset(data) => {
            data.root.regs = vec![registers[0]];
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
pub(super) fn canonical(builder:&mut Builder<'_>,returned:Option<Returned>,inputs:&[Reg],written:Vec<Option<Returned>>)->(Option<Returned>,Vec<Reg>,Vec<Option<Returned>>,Option<Vec<address::Initialization>>) {
    builder.view_model.input_width=inputs.len();
    let mut bindings=inputs.iter().enumerate().map(|(index,reg)|(ValueIdentity::Register(*reg),bounds::Proof::default().with_identity(ValueIdentity::Register(index)))).collect::<BTreeMap<_,_>>();
    for(parameter,p)in builder.definition.function.params.iter().enumerate(){
        if p.access!=Access::Value && !p.ty.dataset && !p.ty.tile {
            let id=ValueIdentity::ParameterStorage(parameter);
            bindings.insert(id,bounds::Proof::default().with_identity(id).with_view_borrow(0,view::Borrow::source(view::Source::Parameter(id))));
        }
        if matches!(p.access,Access::Read|Access::InOut) && let Some(place)=builder.env.get(&p.name).and_then(Binding::stored_place){
            for component in 0..place.ty().components.len(){let id=ValueIdentity::ParameterContent(parameter,component);bindings.insert(id,bounds::Proof::default().with_identity(id));}
        }
        if let Some(Binding::OwnedValue(value))=builder.env.get(&p.name){for resource in 0..view::resource_paths(&value.value.ty).len(){let id=ValueIdentity::ResourceParameter(parameter,resource);bindings.insert(id,bounds::Proof::default().with_identity(id).with_view_sources(0,BTreeSet::from([view::Source::Data(id)])));}}
    }
    if let Some(value)=returned.as_ref().and_then(Returned::view_value){builder.view_model.capture(&value.proof,&value.ty,view_contracts::Output::Returned);}
    for(index,model)in written.iter().enumerate(){if let Some(value)=model.as_ref().and_then(Returned::view_value){builder.view_model.capture(&value.proof,&value.ty,|component|view_contracts::Output::Written(index,component));}}
    application::canonical_element_inputs(builder,&mut bindings);
    let sources=returned.iter().chain(written.iter().flatten()).flat_map(application::element_parameter_sources).collect::<BTreeSet<_>>();
    for id in sources{if let Some(proof)=bindings.get_mut(&id) && let Some(target)=proof.known_identity(0){*proof=proof.clone().with_view_sources(0,BTreeSet::from([view::Source::Parameter(target)]));}}
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
                builder.view_model.resource_names.insert(output,ValueIdentity::Register(target.root.regs[0]));
                let bound=bounds::Proof::default().with_identity(ValueIdentity::Register(target.root.regs[0]));
                bindings.entry(source.root.identity()).or_insert_with(||bound.clone());
                bindings.entry(ValueIdentity::Register(source.root.regs[0])).or_insert(bound);
            }
        }
        next+=value.registers().len();(value,model)
    })).collect::<Vec<_>>();
    let names=bindings.iter().filter_map(|(source,proof)|proof.known_identity(0).map(|target|(*source,target))).collect::<BTreeMap<_,_>>();
    let initialization=builder.view_model.initialization.iter().map(|effect|match effect {
        address::InitializationEffect::Position(fact)=>if fact.origin.pending(){None}else{Some(fact.clone())},
        address::InitializationEffect::Call{..}|address::InitializationEffect::Iteration{..}=>None,
    }).collect::<Option<Vec<_>>>().map(|facts|{
        let facts=address::merge_initializations(facts.into_iter().filter(|fact|!fact.origin.has_local()));
        address::merge_initializations(facts.into_iter().map(|mut fact|{
            fact.when=builder.view_model.initialization_when(fact.when);
            fact.output_bound(&names)
        }))
    });
    builder.view_model.parameters=names;
    let mut models=models.into_iter().map(|pair|pair.map(|(mut value,model)|{translate_returned(&mut value,&model,&bindings,false);value}));
    (models.next().unwrap(),(0..inputs.len()).collect(),models.collect(),initialization)
}
pub(super) fn check(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let (_, mut functions) = declaration(world, instance, application::Context::default(), Module::default())?;
    resolve_residencies(&mut functions)
}
impl Builder<'_> {
    pub(super) fn native_declaration(&mut self, instance: &crate::generics::Instance) -> Result<()> {
        if let Some(definitions) = &instance.definition.compiled {
            if self.functions.declare(&instance.name) {
                self.functions = compiled_declaration(instance,
                    definitions.get(&instance.name).map(Vec::as_slice), std::mem::take(&mut self.functions))?;
            }
            return Ok(());
        }
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
        generated_origin: Option<Arc<GeneratedOrigin>>,
    ) -> Result<Option<Returned>> {
        if instance.definition.function.target == Target::Gpu {
            return self.fail(span, "B161", "普通 CPU 调用不能改变函数声明的实际目标");
        }
        self.native_declaration(&instance)?;
        self.native_call(instance, args, span, generated_origin)
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
        generated_origin: Option<Arc<GeneratedOrigin>>,
    ) -> Result<Option<Returned>> {
        let d = &instance.definition;
        let implicit = self.functions.units[&instance.name].as_ref()
            .is_none_or(|unit| unit.function.is_some() || unit.contract.frame);
        let mut arguments = Vec::new();
        // The interface retains its canonical Frame input slots even when its actual ABI omits Frame.
        // These absent compile-time facts do not create registers or machine arguments.
        let mut actual = vec![None; application::pointer_type().components.len()];
        if implicit {
            let frame = self.application_frame(span)?;
            actual = frame.regs.iter()
                .map(|reg| Some(bounds::Proof::default().with_identity(ValueIdentity::Register(*reg))))
                .collect();
            arguments.push(CallArgument::Value { ty: frame.ty, values: frame.regs });
        }
        let mut env = BTreeMap::new();
        let mut locations = residency::Arguments::new();
        let mut layouts = residency::Layouts::new();
        let mut written = Vec::new();
        let mut written_data = Vec::new();
        let mut held = Vec::new();
        let mut contents = BTreeMap::new();
        let mut resource_inputs=BTreeMap::new();
        let mut storage_inputs=BTreeMap::new();
        let mut memory_borrow=view::Borrow::default();
        for (index, (p, arg)) in d.function.params.iter().zip(args).enumerate() {
            let ty = cooperative::parameter_type(self.world, p, d)?;
            let arg=match arg {Argument::Scoped{value,owner}=>{held.push(owner);Argument::resource(value)},other=>other};
            let arg=match arg {
                Argument::OwnedReference{value,owner,..} if p.access==Access::Read=>{
                    if let Some(owner)=owner{held.push(owner);}Argument::OwnedValue(value)
                },other=>other,
            };
            let arg=match arg {
                Argument::ApplicationData(data) if p.ty.dataset&&p.access!=Access::Value=>Argument::BorrowedData(application::DataReference::direct(data)),
                other=>other,
            };
            let arg=match arg {
                Argument::BorrowedData(mut reference)=>{
                    residency::layout_argument(&mut layouts,index,reference.layout_source());
                    let source=self.data_value_borrow(&reference.data);
                    let borrow=if p.access==Access::Read && d.execution==crate::execution::Requirement::Invocation {
                        let (_,ty)=reference.element_layout();
                        address::Origin{
                            root:address::Root::Derived(source),layout:address::Layout::of(ty),
                            base:address::Layout::of(&reference.data.ty),words:reference.words().into(),positions:Arc::from([]),
                            writable:false,initialized:vec![true;ty.components.len()].into(),
                        }.normalize()
                    }else{source};
                    reference.data.root.proof=reference.data.root.proof.with_view_borrow(0,borrow);
                    Argument::BorrowedData(reference)
                },
                other=>other,
            };
            let arg=match arg {Argument::Value(value) if p.access==Access::Read=>Argument::Reference(self.materialize_array(value)),other=>other};
            match arg {
                Argument::Value(mut value) if p.access == Access::Value && !p.ty.dataset => {
                    self.require_same(&ty, &value.ty, span)?;
                    value.literal = false;
                    value.proof=self.current_address_proof(&value.ty,&value.proof);
                    memory_borrow.union(&self.address_contents(&value.ty,&value.proof));
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
                    for(part,address)in self.native_references(&place,p.access,span,d.function.c_abi)?{arguments.push(CallArgument::Reference{ty:part,access:p.access,address});actual.extend([None,None]);}
                    if p.access==Access::InOut {
                        for data in value.resources(){self.require_application_initialized(data,span)?;}
                        self.load_owned_value(&mut value,span)?;
                        value.value.proof=self.current_address_proof(&value.value.ty,&value.value.proof);
                        memory_borrow.union(&self.address_contents(&value.value.ty,&value.value.proof));
                        let proof=bounds::Proof::capture(&value.value);
                        for component in 0..ty.components.len(){contents.insert(ValueIdentity::ParameterContent(index,component),proof.project(&[component],ty.components.len()));}
                        residency::owned_argument(&mut locations,index,&mut value);
                        for(resource,path)in view::resource_paths(&ty).into_iter().enumerate(){let borrow=match view::owned_data_at(&value,&path){Some(data)=>self.data_value_borrow(data),None=>view::Borrow::from_sources(BTreeSet::from([view::Source::Inactive]))};resource_inputs.insert(ValueIdentity::ResourceParameter(index,resource),bounds::Proof::default().with_view_borrow(0,borrow));}
                    }else{self.drop_returned(Returned::OwnedValue(value.clone()),&view::Borrow::default(),span)?;}
                    env.insert(p.name.clone(),Binding::OwnedValue(value));written.push((index,place));
                }
                Argument::Reference(mut owner) if matches!(p.access, Access::Read | Access::Write | Access::InOut) => {
                    let place=owner.place.clone();
                    self.require_same(&ty, place.ty(), span)?;
                    if p.access.reads(){owner.proof=self.current_address_proof(&ty,&owner.proof);memory_borrow.union(&self.address_contents(&ty,&owner.proof));}
                    let origin=self.position_origin(&place,span)?;
                    storage_inputs.insert(ValueIdentity::ParameterStorage(index),bounds::Proof::default().with_view_borrow(0,view::Borrow::source(view::Source::Position(Arc::new(origin)))));
                    for (part, address) in self.native_references(&place, p.access, span, d.function.c_abi)? {
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
                Argument::Atomic(place) if p.access == Access::Atomic => {
                    self.require_same(&ty, place.ty(), span)?;
                    for (part, address) in self.native_references(&place, Access::Atomic, span, d.function.c_abi)? {
                        arguments.push(CallArgument::Reference { ty: part, access: Access::Atomic, address });
                        actual.extend([None, None]);
                    }
                    env.insert(p.name.clone(), Binding::AtomicBorrow(place));
                }
                Argument::Dataset(view) if p.ty.dataset && p.access==Access::Read => {
                    let binding=view.binding;
                    let access=self.view_access[&binding];
                    if !matches!(access,Access::Read|Access::InOut) {
                        return self.fail(span,"B224","CPU范围只能把可读的实际dataset转借给read参数");
                    }
                    let (field,element)=self.dataset_layout(&view);let element=element.clone();
                    self.require_same(&ty,&element,span)?;
                    self.native_dataset_shape(p,d,self.program.buffers[binding].count,span)?;
                    let zero=self.constant(Scalar::U32,0);
                    let place=self.dataset_element(&view,zero.regs[0],false);
                    let borrow=self.position_origin(&place,span)?.normalize();
                    memory_borrow.union(&borrow);
                    let address=self.reg(Scalar::U64);
                    self.emit(InstKind::PositionAddress{dst:address,address:CallReference{
                        storage:CallStorage::Indexed(IndexedSource::Data{binding,index:zero.regs[0]}),offset:None,word:0,
                    }});
                    let pointer=Value{ty:Ty::scalar(Scalar::U64),regs:vec![address],proof:bounds::Proof::default().with_view_borrow(0,borrow),literal:false,uniform:true,constant:None};
                    let length=self.data_length(&view);
                    let step=self.reg(Scalar::U64);self.emit(InstKind::BufferStride(step,binding));
                    let stride=Value{ty:Ty::scalar(Scalar::U64),regs:vec![step],proof:bounds::Proof::default(),literal:false,uniform:true,constant:None};
                    let offset=self.constant(Scalar::U32,Bits::from(field));
                    let descriptor=Self::borrowed_dataset_parameter_type(p.access,d.execution);
                    let mut proof=bounds::Proof::default();
                    for value in [&pointer,&length,&stride,&offset]{proof.append(value);}
                    let value=Value{ty:descriptor,regs:vec![address,length.regs[0],step,offset.regs[0]],proof,literal:false,uniform:true,constant:None};
                    append_actual(&mut actual,&value);
                    arguments.push(CallArgument::Value{ty:value.ty,values:value.regs});
                    residency::argument(&mut locations,index,&residency::Residency::cpu());
                    residency::layout_argument(&mut layouts,index,residency::LayoutSource::parameter(binding,!view.projection.is_empty()));
                    env.insert(p.name.clone(),Binding::Data(view));
                }
                Argument::BorrowedData(mut reference) if p.ty.dataset=>{
                    residency::argument(&mut locations,index,&reference.data.residency);
                    self.require_same(&ty,reference.element_layout().1,span)?;
                    if !matches!(p.access,Access::Read|Access::Write|Access::InOut|Access::Atomic){
                        return self.fail(span,"B224","CPU dataset借用需要read／write／inout／atomic");
                    }
                    if !reference.permits(p.access){
                        return self.fail(span,"B227","dataset借用的访问模式不能转换为被调参数权限");
                    }
                    if p.access!=Access::Write{self.require_application_reference_initialized(&reference,span)?;}
                    self.native_dataset_shape(p,d,reference.count,span)?;
                    if p.access.writes()&&outputs::declared(d,index){
                        self.prepare_data_output(&mut reference.data,span)?;
                        self.forget_data_output(&reference,span)?;
                    }
                    if p.access==Access::Write||p.access==Access::InOut&&outputs::declared(d,index){
                        written_data.push((index,reference.clone()));
                    }
                    if p.access.writes(){self.invalidate_application_contents(&reference);}
                    let descriptor=Self::borrowed_dataset_parameter_type(p.access,d.execution);
                    let value=self.application_reference_value(&reference,descriptor,span)?;
                    append_actual(&mut actual,&value);
                    arguments.push(CallArgument::Value{ty:value.ty,values:value.regs});
                    env.insert(p.name.clone(),Binding::DataReference(reference));
                }
                Argument::ApplicationData(data) if p.ty.dataset&&p.access==Access::Value=>{
                    residency::layout_argument(&mut layouts,index,residency::LayoutSource::Declared{stride:ir::align_up(data.ty.size,data.ty.align),projected:false});
                    residency::argument(&mut locations,index,&data.residency);
                    self.require_same(&ty,&data.ty,span)?;
                    self.require_application_initialized(&data,span)?;
                    self.native_dataset_shape(p,d,data.count,span)?;
                    let value=self.returned_storage(&Returned::Dataset(data.clone()));
                    append_actual(&mut actual,&value);
                    arguments.push(CallArgument::Value{ty:value.ty,values:value.regs});
                    env.insert(p.name.clone(),Binding::ApplicationData(data));
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
                    value.value.proof=self.current_address_proof(&value.value.ty,&value.value.proof);
                    memory_borrow.union(&self.address_contents(&value.value.ty,&value.value.proof));
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
        let (element_inputs,element_memory)=application::element_input_bindings(self,d,&env);
        memory_borrow.union(&element_memory);
        let requirements = contracts::resolve(self, d, &env)?;
        contracts::prove(self, &requirements, span)?;
        let output_inputs=outputs::inputs(self,d,&env)?;
        let mut view_arguments=actual.iter().enumerate().filter_map(|(index,proof)|proof.clone().map(|proof|(ValueIdentity::Register(index),proof))).collect::<BTreeMap<_,_>>();
        view_arguments.extend(contents.clone());view_arguments.extend(resource_inputs.clone());view_arguments.extend(storage_inputs.clone());view_arguments.extend(element_inputs.clone());
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
            if self.application.is_none() {
                // This range has no application caller whose later closure could consume the call.
                // Consume the already closed imported contract with its actual CPU inputs now.
                let contract=&self.functions.units[&instance.name].as_ref().unwrap().contract;
                let known=BTreeMap::from([(instance.name.clone(),contract.residency.clone())]);
                let inputs=residency::Inputs{calls:vec![call],..Default::default()};
                let remaining=residency::infer(&inputs,&known,&Default::default())?;
                if !remaining.is_empty() {
                    return self.fail(span,"B270","CPU范围尚不能交接该已编译函数所需的布局前提");
                }
            } else {
                self.residency.calls.push(call);
            }
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
        if let Some(access)=self.functions.units[&instance.name].as_ref().and_then(|unit|unit.contract.view_access.as_ref()) {
            memory_borrow=view::Borrow::default();
            for access in access {
                let (view_contracts::Access::Memory{borrows,when}|view_contracts::Access::Position{borrows,when,position:view_contracts::PositionAccess{writes:true,..}})=access else{continue;};
                let path=when.guard_at_call(&view_arguments).assume(&self.path,true);
                let borrow=self.current_address_borrow(&borrows.at_call(&view_arguments));
                memory_borrow.union(&borrow.restricted(&path));
            }
        }
        let memory=if self.application.is_none(){
            let access=self.functions.units[&instance.name].as_ref().unwrap().contract.view_access.as_ref().unwrap().clone();
            self.native_dataset_reads(&access,&view_arguments)
        }else{Vec::new()};
        let memory_use=self.call_address_memory(memory_borrow,span)?;
        let source_site = if generated_origin.is_none() { self.source_site.clone() } else { None };
        self.code.push(Inst { kind: InstKind::Call(Call {
            target: CallTarget::Defined(instance.name.clone()),
            arguments,
            memory,
            memory_use:Some(memory_use),
            result: result.clone(),
            values: values.clone(),
        }), source_site, generated_origin });
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
        bindings.extend(contents);bindings.extend(resource_inputs);bindings.extend(storage_inputs);bindings.extend(element_inputs);
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
        let view_call=Some(self.view_model.call(&instance.name,memory_use,view_arguments.clone(),address::initialization_effects(&self.initialized,&bounds::Clause::default()),self.path.clone(),self.definition.source.clone(),span));
        if d.function.result.is_never() {
            self.flow = Flow::STOP;
            return Ok(None);
        }
        let initialized=self.functions.units[&instance.name].as_ref().and_then(|unit|unit.contract.initialization.as_ref());
        let effects=match initialized {
            Some(facts)=>facts.iter().map(|fact|address::InitializationEffect::Position(fact.at_call(&bindings))).collect::<Vec<_>>(),
            None=>vec![address::InitializationEffect::Call{call:view_call.unwrap(),when:bounds::Condition::constant(true)}],
        };
        let positions=effects.into_iter().flat_map(|effect|match &effect {
            address::InitializationEffect::Position(fact)if fact.when.truth_in(&self.path)==Some(true)=>fact.positions(&self.path),
            _=>vec![Position::Initialization(Arc::new(effect))],
        }).collect::<Vec<_>>();
        self.initialized.extend(positions);
        let output_value=result.as_ref().map(|ty|Value{ty:ty.clone(),regs:values.clone(),
            proof:model.as_ref().map(|model|bounds::Proof::capture(&self.returned_storage(model)).at_call(&bindings)).unwrap_or_default(),
            uniform:false,literal:false,constant:None});
        for (index, place) in written {
            let guards=outputs::guards(self,d,index,place.ty(),&output_inputs,output_value.as_ref())?;
            if outputs::declared(d,index) {self.forget_output_initialization(&place,span)?;}
            let model = writes
                .get(index)
                .and_then(Option::as_ref)
                .map(|model| bounds::Proof::capture(&self.returned_storage(model)))
                .unwrap_or_default();
            let mut proof=model.at_call(&bindings);
            if let Some(call)=view_call {for component in view_contracts::components(place.ty()) {
                let relation=model.view_borrow(component);
                if relation.empty()||relation.pending()||relation.unresolved(){
                    proof=proof.with_view_borrow(component,self.view_model.result(call,view_contracts::Output::Written(index,component)));
                }
            }}
            if place.ty().owns_resources(){
                let regs=written_registers[&index].clone();
                let mut result=self.template_returned(Value{ty:place.ty().clone(),regs,proof:proof.clone(),uniform:false,literal:false,constant:None},true);
                if let Some(Some(model))=writes.get(index){translate_returned(&mut result,model,&bindings,false);residency::returned(&mut result,&locations);}else{residency::output(&mut result,&instance.name,residency::OutputKind::Written(index),&locations,&view_arguments);}
                if let Some(call)=view_call{view_contracts::set_resource_borrows(&mut result,|resource|self.view_model.result(call,view_contracts::Output::WrittenResource(index,resource)));}
                if outputs::declared(d,index){outputs::apply_ownership(&mut result,place.ty(),0,&guards);}
                let Returned::OwnedValue(mut value)=result else{unreachable!()};proof=bounds::Proof::capture(&value.value);value.value.proof=proof.clone();self.assign_owned_place(&place,Returned::OwnedValue(value));
            }
            self.assign_owned_proof(&place, proof, false);
            self.initialize_output(&place,&guards);
        }
        for (index,data) in written_data {
            let guards=outputs::guards(self,d,index,data.element_layout().1,&output_inputs,output_value.as_ref())?;
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
            translate_returned(&mut returned, &model, &bindings,false);
            residency::returned(&mut returned, &locations);
        } else {
            residency::output(&mut returned, &instance.name,residency::OutputKind::Returned, &locations,&view_arguments);
        }
        if let Some(call)=view_call{view_contracts::set_resource_borrows(&mut returned,|resource|self.view_model.result(call,view_contracts::Output::ReturnedResource(resource)));}
        if let (Some(call),Some(value))=(view_call,returned.view_value_mut()) {
            for component in view_contracts::components(&value.ty){
                let actual=value.proof.view_borrow(component);
                let resolved=self.view_model.result(call,view_contracts::Output::Returned(component));
                if actual.empty()||actual.pending()||actual.unresolved(){value.proof=value.proof.clone().with_view_borrow(component,resolved);}
            }
        }
        self.view_uses.after(span);
        let borrow=returned.view_value().map(|value|value.proof.typed_view_borrow(&value.ty)).unwrap_or_default();
        for name in held {
            let resources=self.env.get(&name).into_iter().flat_map(Binding::application_resources).filter(|data|data.ownership==application::Ownership::Owned).map(|data|data.root.identity()).collect::<Vec<_>>();
            for identity in resources {
                let resource=self.data_borrow(&[identity]);
                self.check_view_requirement(view::Requirement::Unborrowed{resource,borrow:borrow.clone(),ownership:true,span})?;
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
pub(super) fn translate_returned(
    value: &mut Returned,
    model: &Returned,
    bindings: &BTreeMap<ValueIdentity, bounds::Proof>,
    stored: bool,
) {
    match (value, model) {
        (Returned::Value(value), Returned::Value(model)) => translate_value(value, model, bindings),
        (Returned::Dataset(value), Returned::Dataset(model)) => {
            translate_value(&mut value.root, &model.root, bindings);
            translate_value(&mut value.length, &model.length, bindings);
            value.root.proof = bounds::Proof::capture(&value.root);
            value.length.proof =
                bounds::Proof::capture(&value.length).with_domains(Domains::from([
                    IndexDomain::ApplicationLength(value.root.identity()),
                ]));
            value.residency = model.residency.at_call(bindings);
            value.count = model.count;
            let element_bindings=application::element_scope_bindings(bindings,model.elements.registers().len(),stored);
            translate_returned(&mut value.elements,&model.elements,&element_bindings,true);
        }
        (Returned::OwnedValue(value), Returned::OwnedValue(model)) => {
            translate_value(&mut value.value, &model.value, bindings);
            for (value, model) in value.payloads.iter_mut().zip(&model.payloads) {
                match (value.as_mut(), model.as_ref()) {
                    (Some(value), Some(model)) => translate_returned(value, model, bindings,stored),
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
    view_contracts::Interface{outputs,access:contract.view_access.as_ref().unwrap().clone(),initialization:contract.initialization.clone().unwrap()}
}
fn resolve_views(module:&mut Module,root:Option<&str>)->Result<()> {
    let mut functions=BTreeMap::new();let mut closed=BTreeMap::new();
    let mut pending=root.map(|name|vec![name.to_owned()]).unwrap_or_else(||module.units.iter().filter_map(|(name,unit)|unit.as_ref().unwrap().contract.view_access.is_none().then(||name.clone())).collect());
    while let Some(name)=pending.pop() {
        if functions.contains_key(&name)||closed.contains_key(&name){continue;}
        let Some(unit)=module.units[&name].as_ref() else{return Ok(());};
        if unit.contract.view_access.is_some(){closed.insert(name,view_outputs(&unit.contract));}
        else{
            pending.extend(unit.view_model.calls.iter().map(|call|call.function.clone()));
            functions.insert(name,&unit.view_model);
        }
    }
    let outputs=view_contracts::solve(&functions,closed)?;
    for (name,memory)in &outputs.memory {
        let unit=module.units.get_mut(name).unwrap().as_mut().unwrap();
        let contract=&mut unit.contract;
        let interface=&outputs.interfaces[name];
        if interface.outputs.values().any(view::Borrow::has_local) {
            return unit.source.fail(unit.span,"B227","地址不能返回或输出当前函数的局部存储");
        }
        contract.view_access=Some(interface.access.clone());
        contract.initialization=Some(interface.initialization.clone());
        for (output,borrow)in &interface.outputs {
            match *output {
                view_contracts::Output::Returned(component)=>if let Some(value)=contract.returned.as_mut().and_then(Returned::view_value_mut) {value.proof=value.proof.clone().with_view_borrow(component,borrow.clone());},
                view_contracts::Output::Written(parameter,component)=>if let Some(value)=contract.written[parameter].as_mut().and_then(Returned::view_value_mut) {value.proof=value.proof.clone().with_view_borrow(component,borrow.clone());},
                view_contracts::Output::ReturnedResource(resource)=>if let Some(value)=contract.returned.as_mut().and_then(|value|view_contracts::resource_value_mut(value,resource)){value.root.proof=value.root.proof.clone().with_view_borrow(0,borrow.clone());},
                view_contracts::Output::WrittenResource(parameter,resource)=>if let Some(value)=contract.written[parameter].as_mut().and_then(|value|view_contracts::resource_value_mut(value,resource)){value.root.proof=value.root.proof.clone().with_view_borrow(0,borrow.clone());},
            }
        }
        if let Some(function)=&mut unit.function {
            address::apply_memory(&mut function.body.code,memory,&function.body.locals);
        }
    }
    Ok(())
}
// 函数合同沿真实资源调用关系收敛，递归参数置换也消费同一输入条件。
fn resolve_residencies(module: &mut Module) -> Result<()> {
    resolve_views(module,None)?;
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
        match &mut (inst).kind {
            InstKind::Call(call) => visit(call),
            InstKind::Scope(body) => calls(body, visit),
            InstKind::If { yes, no, .. } => {
                calls(yes, visit);
                calls(no, visit);
            }
            InstKind::Loop {
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
    crate::optimize::visit(&body.code, &mut |inst| {
        if let InstKind::Call(Call { target: CallTarget::Defined(name), .. }) | InstKind::FunctionAddress(_, CallTarget::Defined(name)) = &(inst).kind {
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
fn uses_frame(code: &[Inst], registers: &[Reg], imported: &BTreeSet<String>) -> bool {
    code.iter().any(|inst| {
        let mut other = |reg| !registers.contains(&reg);
        let operands = match &(inst).kind {
            // Imported no-Frame ABIs already contain only their real arguments.
            InstKind::Call(Call { target: CallTarget::Defined(name), arguments, .. }) =>
                arguments.iter().skip(usize::from(!imported.contains(name))).all(|argument| match argument {
                    CallArgument::Value { values, .. } => values.iter().all(|reg| other(*reg)),
                    CallArgument::Reference { address, .. } => address.all_operands(&mut other),
                }),
            _ => inst.all_operands(&mut other),
        };
        !operands || match &(inst).kind {
            InstKind::Scope(code) => uses_frame(code, registers, imported),
            InstKind::If { yes, no, .. } => uses_frame(yes, registers, imported) || uses_frame(no, registers, imported),
            InstKind::Loop { condition, body, .. } => uses_frame(condition, registers, imported) || uses_frame(body, registers, imported),
            _ => false,
        }
    })
}
fn finalize(
    mut module: Module,
    mut reachable: BTreeSet<String>,
    mut root: Option<&mut ValueCode>,
    owner: Option<&str>,
) -> Result<Finalized> {
    use crate::module::codec::Encode;
    resolve_residencies(&mut module)?;
    for unit in module.units.values_mut().flatten() {
        if let Some(function)=&mut unit.function {crate::optimize::value_code(&mut function.body);}
    }
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
    let imported_without_frame = units.iter()
        .filter(|(_, unit)| unit.function.is_none() && !unit.contract.frame)
        .map(|(name, _)| name.clone()).collect::<BTreeSet<_>>();
    let mut frame = BTreeSet::new();
    for (name, unit) in &mut units {
        if unit.contract.frame {
            frame.insert(name.clone());
        }
        if let Some(function) = &unit.function {
            let registers = &function.body.inputs[..2];
            if uses_frame(&function.body.code, registers, &imported_without_frame)
                || function.body.outputs.iter().any(|reg| registers.contains(reg)) {
                frame.insert(name.clone());
            }
        }
    }
    loop {
        let mut next = frame.clone();
        for (name, unit) in &mut units {
            if let Some(function) = &mut unit.function {
                calls(&mut function.body.code, &mut |call| {
                    if matches!(&call.target, CallTarget::Defined(callee) if frame.contains(callee)) {
                        next.insert(name.clone());
                    }
                });
            }
        }
        if next == frame {
            break;
        }
        frame = next;
    }
    let mut imports = BTreeMap::new();
    for (name, unit) in &mut units {
        unit.contract.frame = frame.contains(name);
        if unit.c_abi && unit.contract.frame {
            return unit.source.fail(unit.span, "B263", "C入口的实际调用闭包需要Frame；C ABI不能隐式取得执行上下文");
        }
        if unit.function.is_none() || owner.is_some_and(|owner| unit.module != owner) {
            imports.insert(name.clone(), unit.contract.bytes());
        }
    }
    units.retain(|name, _| reachable.contains(name));
    let unframed = units.iter()
        .filter(|(name, unit)| unit.function.is_some() && !frame.contains(*name))
        .map(|(name, _)| name.clone()).collect::<BTreeSet<_>>();
    fn prune(body: &mut ValueCode, unframed: &BTreeSet<String>) {
        calls(&mut body.code, &mut |call| {
            if matches!(&call.target,CallTarget::Defined(name) if unframed.contains(name)) {
                call.arguments.remove(0);
            }
        });
    }
    if let Some(body) = &mut root {
        prune(body, &unframed);
    }
    let mut result = Finalized {
        functions: Vec::new(),
        imports,
        exports: BTreeMap::new(),
    };
    for (name, mut unit) in units {
        let local = unit.function.is_some() && owner.is_none_or(|owner| unit.module == owner);
        if local {
            let mut function = unit.function.take().unwrap();
            prune(&mut function.body, &unframed);
            if !frame.contains(&name) {
                function.parameters.remove(0);
                function.body.inputs.drain(..2);
            }
            crate::optimize::value_code(&mut function.body);
            result.functions.push(function);
            if owner.is_some() && unit.public {
                result.exports.insert(name, unit.contract.bytes());
            }
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
        if d.compiled.is_some() || d.function.kind == FunctionKind::Constant
            || d.function.external.is_some() { continue; }
        crate::generics::validate_declaration(d)?;
        if d.module != entry {
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
        header: world.interfaces[entry].source.text.to_string(),
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
        self.emit(InstKind::Call(Call {
            target: CallTarget::Defined(crate::module::domain::symbol(&instance.name, target)),
            arguments,
            memory: Vec::new(),
            memory_use: None,
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
        let first = registers.len();
        if param.ty.dataset {
            registers.extend([Scalar::U32; 2]);
            parameters.push(CallParameter::Value(Builder::domain_dataset_parameter_type()));
            data.push([first, first + 1]);
        } else {
            let ty = cooperative::parameter_type(world, param, d)?;
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
        length_requirements: signature.length_requirements.clone(),
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
                    ty: Builder::domain_dataset_parameter_type(),
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
                            code.push(Inst::new(InstKind::Constant(reg, *scalar, *word)));
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
        code.push(Inst { source_site: call.source, ..Inst::new(InstKind::Call(Call {
            target: CallTarget::Defined(crate::module::domain::symbol(&call.instance.name, target)),
            arguments,
            memory: Vec::new(),
            memory_use: None,
            result: None,
            values: Vec::new(),
        })) });
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
        entry_origin: None,
        generated_origin: None,
        registers,
        locals: Default::default(),
        inputs: inputs.clone(),
        outputs: Vec::new(),
        code,
    };
    let function = FunctionBody {
        name: name.clone(),
        source_origin: d.source_origin(),
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
        view_access:Some(BTreeSet::new()),
        initialization:Some(Vec::new()),
    };
    functions.units.insert(
        name,
        Some(Unit {
            function: Some(function),
            contract,
            module: d.module.clone(),
            public: d.public,
            c_abi: false,
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
    arguments.extend(data.iter().flat_map(|regs| *regs).map(|reg| CallArgument::Value {
        ty: Ty::scalar(Scalar::U32),
        values: vec![reg],
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
