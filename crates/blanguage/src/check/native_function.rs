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
    pub(super) units: BTreeMap<String, Option<Unit>>,
    pub(super) resources: crate::application::Resources,
    programs: Vec<Option<PendingProgram>>,
}
struct PendingProgram {
    target: Target,
    program: Program,
    scopes: BTreeMap<usize, RangeContext>,
    deferred_source: Option<std::path::PathBuf>,
}

impl Module {
    pub(super) fn read_only(&self,name:&str)->bool {
        self.units.get(name).and_then(Option::as_ref).and_then(|unit|unit.contract.view_access.as_ref()).is_some_and(|access|
            access.iter().all(|access|match access {
                view_contracts::Access::Resource{kind,..}=>*kind!=view::Exclusion::Ownership,
                view_contracts::Access::Memory{access,..}=>!access.writes(),
                view_contracts::Access::Position{position,..}=>position.written.is_none(),
                view_contracts::Access::Dataset{access,..}=>!access.writes(),
                view_contracts::Access::Initialized{..}|view_contracts::Access::Stored{..}=>true,
            }))
    }
    fn reference_read_only(&self,name:&str)->bool {
        fn read_only(borrow:&view::Borrow)->bool {
            !borrow.empty()&&borrow.inputs.is_empty()&&borrow.calls.is_empty()&&borrow.sources.iter().all(|source|match source {
                view::Source::Position(origin)=>!origin.writable,
                view::Source::Inactive=>true,
                _=>false,
            })
        }
        fn returned_read_only(model:&Returned)->bool {
            model.view_value().is_none_or(|value|value.proof.addresses.iter().flatten().all(|identity|match identity {
                address::Identity::Storage(borrow)=>read_only(borrow),
                _=>true,
            })&&value.ty.borrow_fields().into_iter().filter(|(_,ty)|ty.is_address()).all(|(word,_)| {
                let component=value.ty.components.iter().position(|(_,at)|*at==word).unwrap();
                read_only(&value.proof.view_borrow(component))
            }))&&match model {
                Returned::Dataset(data)=>data.elements.known().is_some_and(returned_read_only),
                Returned::OwnedValue(value)=>value.payloads.iter().flatten().all(|payload|returned_read_only(payload)),
                _=>true,
            }
        }
        self.read_only(name)&&self.units[name].as_ref().unwrap().contract.returned.as_ref().is_none_or(returned_read_only)
    }
    pub(super) fn writes_only_references(&self,name:&str)->bool {
        fn references(borrow:&view::Borrow)->bool {
            borrow.inputs.is_empty()&&borrow.calls.is_empty()&&borrow.sources.iter().all(|source|match source {
                view::Source::Parameter(ValueIdentity::ParameterStorage(_),_)=>true,
                view::Source::Position(origin)=>origin.words.len()==origin.layout.parts.len()
                    && origin.words.iter().zip(origin.layout.parts.iter()).all(|(word,(scalar,_))|u64::from(*word)+u64::from(scalar.size())<=u64::from(origin.base.size))
                    && match &origin.root {
                        address::Root::Parameter(_)=>true,
                        address::Root::Derived(borrow)=>references(borrow),
                        _=>false,
                    },
                _=>false,
            })
        }
        self.units.get(name).and_then(Option::as_ref).and_then(|unit|unit.contract.view_access.as_ref()).is_some_and(|access|
            access.iter().all(|access|match access {
                view_contracts::Access::Resource{kind,..}=>matches!(kind,view::Exclusion::Position(_)),
                view_contracts::Access::Memory{access,borrows,..}=>!access.writes()||references(borrows),
                view_contracts::Access::Dataset{access,..}=>!access.writes(),
                view_contracts::Access::Initialized{..}|view_contracts::Access::Stored{..}=>true,
                view_contracts::Access::Position{position,borrows,..}=>position.written.is_none()||references(borrows),
            }))
    }
    fn owns_input(&self, function: &str, id: ValueIdentity) -> bool {
        let unit = self.units[function].as_ref().unwrap();
        match id {
            ValueIdentity::ResourceParameter(parameter, _) => unit.input_access[parameter] == Access::Value,
            _ => false,
        }
    }
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
    element_inputs:BTreeMap<residency::Input,(usize,usize)>,
    returned: Option<Returned>,
    pub(super) written: Vec<Option<Returned>>,
    requires: BTreeSet<(ContractKind, ContractPredicate)>,
    target: Target,
    frame: bool,
    fallible: bool,
    pub(super) domain: Option<crate::module::domain::Interface>,
    residency: residency::Contract,
    view_access:Option<BTreeSet<view_contracts::Access>>,
    initialization:Option<Vec<address::Initialization>>,
}
impl Contract {
    fn residency_dependencies(&self) -> BTreeSet<String> {
        residency::dependencies(&self.residency, self.returned.iter().chain(self.written.iter().flatten()))
    }
}

impl crate::module::codec::Encode for Contract {
    fn encode(&self,w:&mut crate::module::codec::Writer){
        contract::encode(self,w);
    }
}
mod contract;
impl crate::module::codec::Decode for Contract {
    fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{
        use crate::module::codec::Decode;
        Ok(Self{
            parameters:Decode::decode(r)?,result:Decode::decode(r)?,inputs:Decode::decode(r)?,element_inputs:Decode::decode(r)?,
            returned:Decode::decode(r)?,written:Decode::decode(r)?,requires:Decode::decode(r)?,
            target:Decode::decode(r)?,frame:Decode::decode(r)?,fallible:Decode::decode(r)?,domain:Decode::decode(r)?,
            residency:Decode::decode(r)?,view_access:Some(Decode::decode(r)?),initialization:Some(Decode::decode(r)?),
        })
    }
}

pub(crate) fn compiled_task_signature(bytes:&[u8]) -> std::result::Result<(Vec<CallParameter>,Option<Ty>,bool),&'static str> {
    use crate::module::codec::Decode;
    let mut contract=Contract::from_bytes(bytes).map_err(|_|"invalid compiled ordinary worker contract")?;
    if !contract.frame
        && matches!(contract.parameters.first().map(|parameter|&parameter.kind),Some(CallParameterKind::Value(ty)) if ty.name=="__blink_frame")
    {
        contract.parameters.remove(0);
    }
    Ok((contract.parameters,contract.result,contract.fallible))
}

pub(super) struct Unit {
    context: Option<CheckedContext>,
    function: Option<FunctionBody>,
    pub(super) contract: Contract,
    source: Source,
    span: Span,
    module: String,
    input_access: Vec<Access>,
    c_abi: bool,
    residency: residency::Inputs,
    view_model: view_contracts::Function,
}
fn declaration(
    world: &World,
    instance: &crate::generics::Instance,
    mut functions: Module,
) -> Result<Module> {
    if !functions.declare(&instance.name) {
        return Ok(functions);
    }
    if let Some(definitions) = &instance.definition.compiled {
        return compiled_declaration(instance, definitions.get(&instance.name).map(Vec::as_slice), functions);
    }
    let target=match instance.definition.function.target(){Target::Compute=>Target::Cpu,target=>target};
    let checked=checked_function(world,instance,target,Some(application::Context::default()),functions)?;
    finish_declaration(world,&LexicalScope::Function(instance.clone()),instance.definition.function.c_abi(),checked)
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
        span: instance.definition.function.span(),
        code: "B270",
        message: format!("已编译模块没有原生实例 {}", instance.name),
    })?;
    let contract = Contract::from_bytes(bytes).map_err(|error| Diagnostic {
        source: instance.definition.source.clone(),
        span: instance.definition.function.span(),
        code: "B270",
        message: format!("模块函数接口错误：{error}"),
    })?;
    functions.units.insert(
        instance.name.clone(),
        Some(Unit {
            source: instance.definition.source.clone(),
            span: instance.definition.function.span(),
            module: instance.definition.module.clone(),
            input_access: instance.definition.function.params().iter().map(|parameter|parameter.access()).collect(),
            context: None,
            function: None,
            contract,
            c_abi: instance.definition.function.c_abi(),
            residency: residency::Inputs::default(),
            view_model: view_contracts::Function::default(),
        }),
    );
    Ok(functions)
}

#[inline(never)]
fn finish_declaration(
    world: &World,
    lexical:&LexicalScope,
    c_abi:bool,
    mut checked: Box<CheckedFunction>,
) -> Result<Module> {
    let residency=checked.residency;
    let mut functions = checked.functions;
    let function = checked.body;
    let name=function.name.clone();
    let contract = Contract {
        fallible: false,
        parameters: function.parameters.clone(),
        result: function.result.clone(),
        inputs: checked.inputs,
        element_inputs:std::mem::take(&mut checked.view_model.element_inputs),
        returned: checked.returned,
        written: checked.written,
        requires: checked.contracts,
        target: checked.context.program.target,
        frame: false,
        domain: None,
        residency: residency::Contract::default(),
        view_access:None,
        initialization:checked.initialization,
    };
    functions.units.insert(
        name.clone(),
        Some(Unit {
            source: lexical.source().clone(),
            span: lexical.span(),
            module: lexical.module().to_owned(),
            input_access: lexical.parameters().map(|parameter|parameter.access()).collect(),
            context: Some(checked.context),
            function: Some(function),
            contract,
            c_abi,
            residency,
            view_model: checked.view_model,
        }),
    );
    resolve_views(world,&mut functions,Some(&name))?;
    Ok(functions)
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
                            value.value.ty.variant_payload_offset(),
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
                                .position(|(scalar, word)| *word <= offset + at && offset + at < word + scalar.size())
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
#[derive(Clone)]
pub(super) struct TaskCompletionOutput {
    pub result:Ty,
    pub layout:Ty,
    pub slots:Vec<usize>,
    pub task:view::Borrow,
    pub parent:Option<(Arc<TaskCompletionOutput>,usize)>,
    pub span:Span,
}
impl TaskCompletionOutput {
    pub(super) fn payload_source(&self,builder:&mut Builder<'_>,word:u32)->Reg {
        let (payload,_)=self.layout.task_completion_payload();
        let component=self.layout.components.iter().position(|&(scalar,offset)|
            scalar==Scalar::U32&&offset==payload+word).unwrap();
        let reg=builder.reg(Scalar::U32);
        builder.emit(InstKind::LoadLocal(reg,self.slots[component]));
        reg
    }

    fn return_source(&self,builder:&mut Builder<'_>,component:usize)->Reg {
        let part=self.result.task_components().into_iter().position(|part|part==component).unwrap();
        let reg=builder.reg(Scalar::U32);
        builder.emit(InstKind::LoadLocal(reg,self.slots[3+self.result.components.len()+part]));
        reg
    }

    fn selected_task(&self,builder:&mut Builder<'_>)->view::Borrow {
        let Some((parent,component))=&self.parent else {return self.task.clone();};
        let task=parent.selected_task(builder).task_projection(view::TaskProjection::Returned(*component,None));
        let source=parent.return_source(builder,*component);
        builder.bind_work_task_return_sources(&task,*component,source,0,&|builder,word|
            parent.payload_source(builder,word))
    }
    fn value(&self,builder:&mut Builder<'_>)->Value {
        let regs=self.slots[3..].iter().zip(&self.result.components).map(|(&slot,&(scalar,_))| {
            let reg=builder.reg(scalar);
            builder.emit(InstKind::LoadLocal(reg,slot));
            reg
        }).collect();
        Value{ty:self.result.clone(),regs,proof:bounds::Proof::default(),literal:false,uniform:false,constant:None}
    }
    pub(super) fn has_owned_outputs(&self)->bool {
        self.task.sources.iter().any(|source|matches!(source,view::Source::Task{guards,completions,..} if !guards.is_empty()||!completions.is_empty()))
    }
    pub(super) fn complete_inputs(&self,builder:&mut Builder<'_>,inputs:&[view::TaskInputCompletion])->Result<()> {
        let (base,_)=self.layout.task_completion_payload();
        builder.complete_task_input_slots(inputs,&self.layout,&self.slots,base,self.span)
    }
    pub(super) fn complete_if_ready(&self,builder:&mut Builder<'_>)->Result<()> {
        let status=builder.reg(Scalar::U32);
        builder.emit(InstKind::LoadLocal(status,self.slots[0]));
        let test=|builder:&mut Builder<'_>,op,expected| {
            let value=builder.constant(Scalar::U32,Bits::from(expected)).regs[0];
            let reg=builder.reg(Scalar::Bool);
            builder.emit(InstKind::Binary{dst:reg,op:BinaryOp::Compare(op),a:status,b:value});
            Value{ty:Ty::scalar(Scalar::Bool),regs:vec![reg],proof:bounds::Proof::default(),literal:false,uniform:false,constant:None}
        };
        let ready=test(builder,Comparison::NotEqual,Ty::TASK_COMPLETION_PENDING);
        builder.conditional_blocks(ready,|builder| {
            let task=self.selected_task(builder);
            let success=test(builder,Comparison::Equal,Ty::TASK_COMPLETION_SUCCESS);
            let result=self.value(builder);
            builder.complete_task_owned_places(&task,|_,_|Some(result.clone()),Some(&success),
                |builder,inputs|self.complete_inputs(builder,inputs),self.span)
        },|_|Ok(()),self.span)
    }
}
pub(super) struct TaskCompletionParameter {
    pub input:usize,
    pub result:Ty,
    pub parent:Option<(usize,usize)>,
}

pub(super) fn task_completion_parameters<'a>(
    parameters: impl IntoIterator<Item=(usize,Option<&'a Ty>)>, start:usize,
)->Vec<TaskCompletionParameter> {
    fn collect(ty:&Ty,input:usize,parent:Option<usize>,completions:&mut Vec<TaskCompletionParameter>) {
        let first=completions.len();
        for (word,field) in ty.borrow_fields() {
            let Some(result)=field.task_result() else {continue;};
            let component=ty.components.iter().position(|(_,at)|*at==word).unwrap();
            let parent=parent.map(|parent|(parent,component));
            let input=if parent.is_some(){input}else{input+component};
            if completions[first..].iter().any(|existing|
                existing.input==input && existing.parent==parent && existing.result==*result) {
                continue;
            }
            let index=completions.len();
            completions.push(TaskCompletionParameter{input,result:result.clone(),parent});
            collect(result,input,Some(index),completions);
        }
    }

    let mut offset=start;
    let mut completions=Vec::new();
    for (width,ty) in parameters {
        if let Some(ty)=ty {collect(ty,offset,None,&mut completions);}
        offset+=width;
    }
    completions
}
pub(super) fn canonical(builder:&mut Builder<'_>,returned:Option<Returned>,inputs:&[Reg],parameters:&[CallParameter],written:Vec<Option<Returned>>)->Result<(Option<Returned>,Vec<Reg>,Vec<Option<Returned>>,Option<Vec<address::Initialization>>)> {
    builder.view_model.input_width=inputs.len();
    let mut bindings=inputs.iter().enumerate().map(|(index,reg)|(ValueIdentity::Register(*reg),bounds::Proof::default().with_identity(ValueIdentity::Register(index)))).collect::<BTreeMap<_,_>>();
    if builder.task_return_source.is_some()
        && let Some(value)=returned.as_ref().and_then(Returned::view_value) {
        for component in value.ty.task_components() {
            let id=ValueIdentity::TaskReturnSource(component);
            bindings.insert(id,bounds::Proof::default().with_identity(id));
        }
    }
    for (result,address) in builder.task_completions.values().flatten() {
        let input=inputs.iter().position(|reg|*reg==address[0]).unwrap();
        for component in 0..result.components.len()+result.task_components().len() {
            bindings.insert(ValueIdentity::TaskCompletion(address[0],component),
                bounds::Proof::default().with_identity(ValueIdentity::TaskCompletion(input,component)));
        }
    }
    for(parameter,p)in builder.scope.parameters().enumerate(){
        let p=p.bound(builder.world)?;
        if p.access!=Access::Value && !p.ty.dataset && !p.ty.tile {
            let id=ValueIdentity::ParameterStorage(parameter);
            bindings.insert(id,bounds::Proof::default().with_identity(id).with_view_borrow(0,view::Borrow::source(view::Source::Parameter(id,Vec::new()))));
        }
        if matches!(p.access,Access::Read|Access::InOut) && let Some(place)=builder.env.get(p.name.as_str()).and_then(Binding::stored_place){
            for component in 0..place.ty().components.len(){let id=ValueIdentity::ParameterContent(parameter,component);bindings.insert(id,bounds::Proof::default().with_identity(id));}
        }
        let resources=match builder.env.get(p.name.as_str()){
            Some(Binding::ApplicationData(_))if p.access==Access::Value=>1,
            Some(Binding::OwnedValue(value))=>view::resource_paths(&value.value.ty).len(),
            _=>0,
        };
        for resource in 0..resources{let id=ValueIdentity::ResourceParameter(parameter,resource);bindings.insert(id,bounds::Proof::default().with_identity(id).with_view_sources(0,BTreeSet::from([view::Source::Data(id)])));}
    }
    if let Some(value)=returned.as_ref().and_then(Returned::view_value){builder.view_model.capture(&value.proof,&value.ty,view_contracts::Output::Returned);}
    for(index,model)in written.iter().enumerate(){if let Some(value)=model.as_ref().and_then(Returned::view_value){builder.view_model.capture(&value.proof,&value.ty,|component|view_contracts::Output::Written(index,component));}}
    application::canonical_element_inputs(builder,&mut bindings);
    let sources=returned.iter().chain(written.iter().flatten()).flat_map(application::element_parameter_sources).collect::<BTreeSet<_>>();
    for id in sources{if let Some(proof)=bindings.get_mut(&id) && let Some(target)=proof.known_identity(0){*proof=proof.clone().with_view_sources(0,BTreeSet::from([view::Source::Parameter(target,Vec::new())]));}}
    if let Some(value)=&returned{builder.view_model.capture_resources(value,view_contracts::Output::ReturnedResource);}
    for(parameter,value)in written.iter().enumerate(){if let Some(value)=value{builder.view_model.capture_resources(value,|resource|view_contracts::Output::WrittenResource(parameter,resource));}}
    for(memory,usage)in builder.view_model.uses.iter().enumerate(){
        let count=match &usage.requirement{view::Requirement::Memory{snapshots:Some(snapshots),..}=>snapshots.len(),view::Requirement::Dataset{values:Some(_),..}=>1,_=>0};
        for component in 0..count{
            let id=ValueIdentity::MemoryValue(memory,component);bindings.insert(id,bounds::Proof::default().with_identity(id));
        }
    }
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
        let paths=view_contracts::resource_output_paths(&model);
        for resource in &paths{
            let(root,scope)=resource;
            let output=if model_index==0{view_contracts::Output::ReturnedResource(resource.clone())}else{view_contracts::Output::WrittenResource(model_index-1,resource.clone())};
            if let(Some(source),Some(target))=(view_contracts::resource_value_mut(&mut original,*root),view_contracts::resource_value_mut(&mut value,*root)){
                let name=if source.ownership==application::Ownership::Borrowed{
                    let identity=source.root.identity();
                    let bound=bindings[&identity].clone();
                    bindings.insert(ValueIdentity::Register(source.root.regs[0]),bound);
                    view::Borrow::source(view::Source::Data(identity)).at_call(&bindings)
                }else{view::Borrow::source(view::Source::Data(ValueIdentity::Register(target.root.regs[0])))};
                let name=scope.iter().enumerate().fold(name,|parent,(at,fields)|view::Borrow::element(parent,fields.clone(),Some(bounds::Operand::Value(ValueIdentity::ElementIndex(scope.len()-1-at)))));
                builder.view_model.resource_names.insert(output.clone(),name);
            }
            if !scope.is_empty(){if let Some(source)=view_contracts::resource_output_mut(&mut original,resource){
                let name=builder.view_model.resource_names[&output].clone();
                for origin in &source.root.proof.view_borrow(0).sources{if let view::Source::Data(id@ValueIdentity::Register(_))=origin{bindings.entry(*id).or_insert_with(||bounds::Proof::default().with_view_borrow(0,name.clone()));}}
            }}
            if scope.is_empty(){if let(Some(source),Some(target))=(view_contracts::resource_output_mut(&mut original,resource),view_contracts::resource_output_mut(&mut value,resource)){
                let bound=bounds::Proof::default().with_identity(ValueIdentity::Register(target.root.regs[0]));
                bindings.entry(source.root.identity()).or_insert_with(||bound.clone());bindings.entry(ValueIdentity::Register(source.root.regs[0])).or_insert(bound);
            }}
        }
        next+=value.registers().len();(value,model)
    })).collect::<Vec<_>>();
    let names=bindings.iter().filter_map(|(source,proof)|proof.known_identity(0).map(|target|(*source,target))).collect::<BTreeMap<_,_>>();
    let initialization=builder.view_model.initialization.iter().map(|effect|match effect {
        address::InitializationEffect::Position(fact)=>if fact.source.pending(){None}else{Some(fact.clone())},
        address::InitializationEffect::Call{..}|address::InitializationEffect::Iteration{..}=>None,
    }).collect::<Option<Vec<_>>>().map(|facts|{
        let facts=address::merge_initializations(facts).into_iter().filter(|fact|!fact.source.borrow().has_local());
        address::merge_initializations(facts.into_iter().map(|mut fact|{
            fact.when=builder.view_model.initialization_when(&fact);
            fact.output_bound(&names)
        }))
    });
    builder.view_model.parameters=names;
    // Count事实按原ABI输入槽命名，实际地址和存储效果继续沿原合同交接。
    let mut resources=BTreeMap::new();let mut start=0;
    for parameter in parameters {
        if let Some(origin)=&parameter.origin
            && let ParameterOrigin::Parameter{index,..}=origin.as_ref()
            && builder.scope.parameters().nth(*index).unwrap().bound(builder.world)?.ty.dataset {
            let id=ValueIdentity::Register(start);
            resources.insert(id,bounds::Proof::default().with_identity(id).with_view_borrow(0,view::Borrow::source(view::Source::Parameter(ValueIdentity::ResourceParameter(*index,0),Vec::new()))));
        }
        start+=match &parameter.kind{CallParameterKind::Value(ty)=>ty.components.len(),CallParameterKind::Reference{..}=>2};
    }
    fn prefix_inputs(value:&mut Returned,resources:&BTreeMap<ValueIdentity,bounds::Proof>) {
        match value {
            Returned::Dataset(data)=>{
                for fact in data.prefixes.values_mut(){*fact=fact.substitute_values(resources);}
                data.elements=std::mem::take(&mut data.elements).map(&mut|mut model|{prefix_inputs(&mut model,resources);model},&mut|condition|condition);
            },
            Returned::OwnedValue(value)=>for value in value.payloads.iter_mut().flatten(){prefix_inputs(value,resources);},
            _=>{},
        }
    }
    let mut models=models.into_iter().map(|pair|pair.map(|(mut value,model)|{
        translate_returned(&mut value,&model,&bindings,false);prefix_inputs(&mut value,&resources);value
    }));
    Ok((models.next().unwrap(),(0..inputs.len()).collect(),models.collect(),initialization))
}
pub(super) fn check(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let mut functions = declaration(world, instance, Module::default())?;
    lower_module_failures(world,&mut functions,&mut [])?;
    resolve_residencies(world,&mut functions).map(|_|())
}
fn element_drop_declaration(world:&World,name:&str,ty:&Ty,lexical:&LexicalScope,
    parameters:&[Param],contract:outputs::Contract<'_>,functions:Module,span:Span)->Result<Module> {
    let program=Arc::new(function_program(world,name,parameters.iter().cloned().map(Ok),
        (lexical.source(),lexical.module(),lexical.imports(),FunctionKind::Value),Target::Cpu,None)?);
    let(source_definition,source_context)=lexical.syntax_context(Some(name));
    let mut b=Box::new(Builder::from_checked(world,lexical,&program,0,source_definition,source_context,BTreeSet::new()));
    b.in_function=true;
    b.application=Some(application::Context::default());
    b.functions=functions;
    let frame=b.application_frame(span)?;
    let mut inputs=frame.regs.clone();
    let mut abi=vec![CallParameter::value(frame.ty).with_origin(Some(Arc::new(ParameterOrigin::Frame)))];
    b.calls.push(name.to_owned());
    let origin=Some(Arc::new(ParameterOrigin::Parameter{
        index:0,name:"value".into(),instance:Arc::from(name),generated:true,site:None}));
    if parameters[0].ty.dataset {
        let data=b.application_parameter(0,0,Access::Value)?;
        abi.push(CallParameter::value(Ty::owned_dataset(data.ty.syntax(),data.count)).with_origin(origin));
        inputs.extend(&data.root.regs);inputs.extend(&data.length.regs);
        let data=b.application_bind(data,false);
        b.env.insert("value".into(),Binding::ApplicationData(data));
    }else{
        let regs=ty.components.iter().map(|(scalar,_)|b.reg(*scalar)).collect::<Vec<_>>();
        inputs.extend(&regs);
        abi.push(CallParameter::value(ty.clone()).with_origin(origin));
        let proof=bounds::Proof::default().view_parameters(ty,&[],|index|ValueIdentity::Register(regs[index]));
        let value=Value{ty:ty.clone(),regs,proof,literal:false,uniform:false,constant:None};
        if ty.owns_resources(){
            let mut value=b.incoming_owned_value(value)?;
            residency::parameter(&mut value,0);
            b.bind_owned_value("value",value,span)?;
        }else{b.env.insert("value".into(),Binding::Value(value));}
    }
    let mut environment=std::mem::take(&mut b.env);
    let paths=contracts::element_input_paths(&mut b,contract,&mut environment,None)?;
    b.env=environment;
    application::bind_parameter_elements(&mut b,contract,&paths);
    let resources=match b.env.get("value"){
        Some(Binding::ApplicationData(data))=>BTreeMap::from([(ValueIdentity::ResourceParameter(0,0),
            bounds::Proof::default().with_view_borrow(0,view::resource_borrow(data)))]),
        _=>BTreeMap::new(),
    };
    let mut environment=std::mem::take(&mut b.env);
    let requirements=contracts::resolve(&mut b,contract,lexical,&environment,&resources)?;
    contracts::assume(&mut b,contract,&mut environment,&requirements)?;
    b.env=environment;
    b.materialize_array_bindings();
    b.release_application_data(None,view::Borrow::default())?;
    b.view_model.initialization=address::initialization_effects(&b.initialized,&b.path);
    b.view_model.normal_returns=Some(bounds::Condition::execution(&b.path));
    let contracts=contract.contracts().map(|contract|(contract.kind,contract.predicate)).collect();
    let checked=finish_checked_function(b,program.clone(),name,span,None,None,abi,inputs,contracts,None,vec![None])?;
    finish_declaration(world,lexical,false,checked)
}
struct CallSignature<'a> {
    parameters:&'a [Param],
    result:&'a TypeName,
    contract:outputs::Contract<'a>,
    lexical:&'a LexicalScope,
    kind:FunctionKind,
    execution:crate::execution::Requirement,
    c_abi:bool,
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
        self.functions = declaration(self.world, instance, std::mem::take(&mut self.functions))?;
        Ok(())
    }
    pub(super) fn native_function(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
        generated_origin: Option<Arc<GeneratedOrigin>>,
        syntax: Option<&Expressions>,
        launch: Option<Value>,
    ) -> Result<Option<Returned>> {
        if instance.definition.function.target() == Target::Gpu {
            return self.fail(span, "B161", "普通 CPU 调用不能改变函数声明的实际目标");
        }
        self.native_declaration(&instance)?;
        let d=&instance.definition;
        let parameters=instance.parameters(self.world).collect::<Result<Vec<_>>>()?;
        let result=instance.bindings.type_name(self.world,&d.function.result(),(&d.source,&d.module,&d.imports),d.function.span())?;
        let lexical=LexicalScope::Function(instance.clone());
        self.native_call(&instance.name,CallSignature{parameters:&parameters,result:&result,contract:(&instance).into(),
            lexical:&lexical,kind:d.function.kind(),execution:d.execution,c_abi:d.function.c_abi()},args,span,generated_origin,syntax,launch)
    }
    fn declare_owned_cleanup(&mut self,ty:&Ty,span:Span)
        -> Result<(String,Arc<[Param]>,LexicalScope,[FunctionContract;1])> {
        let name = crate::generics::key(&format!("{}::@dataset_element_drop", self.scope.module()), std::iter::once(ty.name.as_str()), &[]);
        let parameters:Arc<[Param]>=Arc::from([Param{name:"value".into(),ty:ty.syntax(),access:Access::Value,span}]);
        let contracts=[FunctionContract{kind:ContractKind::Requires,
            predicate:ContractPredicate::Cleanup{value:"value".into(),condition:OutputCondition::Bool(true)},span}];
        let lexical=LexicalScope::Values{
            source:self.scope.source().clone(),module:self.scope.module().to_owned(),imports:self.scope.imports().to_vec(),
            bindings:self.scope.bindings().clone(),span,parameters:parameters.clone(),source_definition:None,
            source_context:Arc::new(SourceContext{instance:Some(Arc::from(name.as_str())),inlined_at:Box::default(),domain_path:Arc::from([])}),
        };
        let contract=outputs::Contract::native(&lexical,&parameters,&contracts,&name,span);
        if self.functions.declare(&name){
            self.functions=element_drop_declaration(self.world,&name,ty,&lexical,&parameters,contract,
                std::mem::take(&mut self.functions),span)?;
        }
        Ok((name,parameters,lexical,contracts))
    }
    pub(super) fn task_cleanup(&mut self,ty:&Ty,span:Span) -> Result<String> {
        self.declare_owned_cleanup(ty,span).map(|(name,_,_,_)|name)
    }
    fn task_callback(
        &mut self, name: String, arguments: &[(&str, Ty)], frame: bool, span: Span,
        emit: impl FnOnce(&mut Builder<'_>, &[Reg]) -> Result<()>,
    ) -> Result<String> {
        let parameters: Arc<[Param]> = arguments.iter().map(|(name, ty)| Param {
            name: (*name).into(), ty: ty.syntax(), access: Access::Value, span,
        }).collect::<Vec<_>>().into();
        let lexical = LexicalScope::Values {
            source: self.scope.source().clone(), module: self.scope.module().into(),
            imports: self.scope.imports().to_vec(), bindings: self.scope.bindings().clone(),
            span, parameters: parameters.clone(), source_definition: None,
            source_context: Arc::new(SourceContext {
                instance: Some(Arc::from(name.as_str())), inlined_at: Box::default(), domain_path: Arc::from([]),
            }),
        };
        let program = Arc::new(function_program(self.world, &name, parameters.iter().cloned().map(Ok),
            (lexical.source(), lexical.module(), lexical.imports(), FunctionKind::Value), Target::Cpu, None)?);
        let (definition, context) = lexical.syntax_context(Some(&name));
        let mut b = Box::new(Builder::from_checked(self.world, &lexical, &program, 0, definition, context, BTreeSet::new()));
        b.in_function = true;
        let frame_value = if frame {
            b.application = Some(application::Context::default());
            Some(b.application_frame(span)?)
        } else {
            None
        };
        let registers = arguments.iter().flat_map(|(_, ty)| &ty.components)
            .map(|&(scalar, _)| b.reg(scalar)).collect::<Vec<_>>();
        let inputs = frame_value.iter().flat_map(|frame| frame.regs.iter().copied())
            .chain(registers.iter().copied()).collect();
        let abi = frame_value.iter().map(|frame| CallParameter::value(frame.ty.clone()).with_origin(Some(Arc::new(ParameterOrigin::Frame))))
            .chain(arguments.iter().map(|(_, ty)| CallParameter::value(ty.clone()))).collect();
        b.functions = std::mem::take(&mut self.functions);
        emit(&mut b, &registers)?;
        self.functions = finish_declaration(self.world, &lexical, false, finish_checked_function(
            b, program.clone(), &name, span, None, None, abi, inputs, BTreeSet::new(), None, vec![None; parameters.len()],
        )?)?;
        if frame {
            let contract = &mut self.functions.units.get_mut(&name).unwrap().as_mut().unwrap().contract;
            // This function is called directly by the task runtime.
            contract.frame = true;
            contract.fallible = true;
        }
        Ok(name)
    }

    pub(super) fn task_selection(
        &mut self, packet: &Value, result: &Ty,
        outputs: &[(u32, outputs::TaskGuard)], span: Span,
    ) -> Result<String> {
        let name = format!("{}::@task_selection{}", self.scope.module(), self.functions.units.len());
        let pointer = Ty::vector(Scalar::U32, 2);
        self.task_callback(name, &[("input", pointer.clone()), ("output", pointer)], false, span, |b, registers| {
            let [a, b0, c, d] = *registers else { unreachable!() };
            let (input, output) = ([a, b0], [c, d]);
            let zero = b.constant(Scalar::U32, 0).regs[0];
            let buffer = |address| IndexedSource::Pointer { address, index: zero, stride: 1, aliases: Vec::new() };
            let mut load = |address, scalar, word| {
                let dst = b.reg(scalar);
                b.emit(InstKind::LoadIndexed { dst, source: buffer(address), offset: zero, word });
                dst
            };
            let snapshot = packet.ty.components.iter().zip(&packet.regs).map(|(&(scalar, offset), &reg)|
                (reg, load(input, scalar, offset))).collect::<BTreeMap<_,_>>();
            let regs = result.components.iter().map(|&(scalar, offset)| load(output, scalar, offset)).collect();
            let returned = Value { ty: result.clone(), regs, proof: bounds::Proof::default(), uniform: false, literal: false, constant: None };
            for (base, guard) in outputs {
                let mut guard = guard.clone();
                guard.map_inputs(|operand| match operand {
                    bounds::Operand::Value(ValueIdentity::Register(reg)) => snapshot.get(&reg).map(|reg|
                        bounds::Operand::Value(ValueIdentity::Register(*reg))),
                    _ => Some(operand),
                });
                let ty = guard.frame_output_type().unwrap();
                let selected = guard.guards(b, Some(&returned), span)?;
                for (index, (at, _)) in ty.task_output_owners().into_iter().enumerate() {
                    let ready = outputs::resource_guard(ty, at, &selected);
                    let value = b.presence_register(&ready, span, None)?;
                    b.emit(InstKind::StoreIndexed {
                        source: buffer(output), offset: zero, word: *base + index as u32 * Scalar::Bool.size(), value,
                    });
                }
            }
            Ok(())
        })
    }

    pub(super) fn task_completion_cleanup(
        &mut self, positions: &[(u32, &Ty, Option<u32>)], datasets: &[(u32, &Ty, &Ty, Access, Option<u32>)],
        result: &Ty, span: Span,
    ) -> Result<String> {
        let name = format!("{}::@task_completion_cleanup{}", self.scope.module(), self.functions.units.len());
        let pointer = Ty::vector(Scalar::U32, 2);
        let inputs = [("output", pointer.clone()), ("input", pointer),
            ("status", Ty::scalar(Scalar::U64)), ("completed", Ty::scalar(Scalar::Bool))];
        self.task_callback(name, &inputs, true, span, |b, registers| {
            let [out_low, out_high, in_low, in_high, status, completed] = *registers else { unreachable!() };
            let (output, input) = ([out_low, out_high], [in_low, in_high]);
            let slot = b.local_slots(&Ty::scalar(Scalar::U64)).0[0];
            b.emit(InstKind::StoreLocal(slot, status));
            b.pending_error = Some(PendingError::Exit(slot));
            let zero = b.constant(Scalar::U32, 0).regs[0];
            let load = |b: &mut Builder<'_>, address, scalar, word| {
                let dst = b.reg(scalar);
                b.emit(InstKind::LoadIndexed {
                    dst, source: IndexedSource::Pointer { address, index: zero, stride: 1, aliases: Vec::new() },
                    offset: zero, word,
                });
                dst
            };
            let ready_at = |b: &mut Builder<'_>, base, index| {
                load(b, output, Scalar::Bool, base + index as u32 * Scalar::Bool.size())
            };
            let outer = std::mem::take(&mut b.code);
            if result.owns_resources() {
                b.drop_task_completion_value(result, output, 0, span)?;
            }
            for &(packet, ty, ready) in positions {
                let address = std::array::from_fn(|i| load(b, input, Scalar::U32, packet + i as u32 * 4));
                if let Some(base) = ready {
                    for (index, (offset, owner)) in ty.task_output_owners().into_iter().enumerate().rev() {
                        let condition = ready_at(b, base, index);
                        let outer = std::mem::take(&mut b.code);
                        b.drop_task_completion_value(owner, address, offset, span)?;
                        let yes = std::mem::replace(&mut b.code, outer);
                        b.emit(InstKind::If { condition, yes, no: Vec::new() });
                    }
                } else {
                    b.drop_task_completion_value(ty, address, 0, span)?;
                }
            }
            let yes = std::mem::replace(&mut b.code, outer);
            if !yes.is_empty() {
                b.emit(InstKind::If { condition: completed, yes, no: Vec::new() });
            }
            for &(packet, layout, element, access, ready) in datasets {
                let outer = std::mem::take(&mut b.code);
                let descriptor = layout.components.iter().map(|&(scalar, offset)| {
                    load(b, input, scalar, packet + offset)
                }).collect::<Vec<_>>();
                let mut guards = vec![owned_presence::Presence::All; element.components.len()];
                if let Some(base) = ready {
                    for (index, (offset, _)) in element.task_output_owners().into_iter().enumerate() {
                        let condition = ready_at(b, base, index);
                        let component = element.components.iter().position(|&(_, at)| at == offset).unwrap();
                        guards[component] = owned_presence::Presence::input(&Value {
                            ty: Ty::scalar(Scalar::Bool), regs: vec![condition],
                            proof: bounds::Proof::default().with_identity(ValueIdentity::Register(condition)),
                            uniform: false, literal: false, constant: None,
                        });
                    }
                }
                b.drop_task_completion_dataset(element, &descriptor, &guards, span)?;
                if access == Access::Write {
                    let yes = std::mem::replace(&mut b.code, outer);
                    b.emit(InstKind::If { condition: completed, yes, no: Vec::new() });
                } else {
                    b.code.splice(..0, outer);
                }
            }
            let error = b.reg(Scalar::U64);
            b.emit(InstKind::LoadLocal(error, slot));
            b.emit(InstKind::Raise(error));
            Ok(())
        })
    }

    fn drop_task_completion_value(&mut self, ty: &Ty, address: [Reg; 2], offset: u32, span: Span) -> Result<()> {
        let mut model = self.template_returned(Value {
            ty: ty.clone(), regs: (0..ty.components.len()).collect(), proof: bounds::Proof::default(),
            uniform: false, literal: false, constant: None,
        }, true)?;
        owned_borrows::each_data(&mut model, &mut Vec::new(), &mut |data, _| data.residency = residency::Residency::cpu());
        owned_borrows::set_storage(&mut model, Place::Reference {
            ty: ty.clone(), address: ReferenceAddress::Contiguous(address), scope: None,
            words: ty.components.iter().map(|&(_, word)| offset + word).collect(), access: Access::InOut,
        });
        self.drop_owned_tree_with(model, &view::Borrow::default(), span, Self::drop_task_owned_dataset)
    }

    pub(super) fn drop_task_completion_element(&mut self, place: &Place, span: Span) -> Result<()> {
        let ty = place.ty();
        let name = crate::generics::key(&format!("{}::@task_element_cleanup", self.scope.module()), [ty.name.as_str()], &[]);
        let pointer = Ty::vector(Scalar::U32, 2);
        if self.functions.declare(&name) {
            let inputs = [("position", pointer.clone()), ("status", Ty::scalar(Scalar::U64))];
            self.task_callback(name.clone(), &inputs, true, span, |builder, registers| {
                let [low, high, status] = *registers else { unreachable!() };
                let slot = builder.local_slots(&Ty::scalar(Scalar::U64)).0[0];
                builder.emit(InstKind::StoreLocal(slot, status));
                builder.pending_error = Some(PendingError::Exit(slot));
                builder.drop_task_completion_value(ty, [low, high], 0, span)?;
                let error = builder.reg(Scalar::U64);
                builder.emit(InstKind::LoadLocal(error, slot));
                builder.emit(InstKind::Raise(error));
                Ok(())
            })?;
        }
        let address = self.call_reference(place, Access::Read, false, span)?;
        let position = self.reg(Scalar::U64);
        self.emit(InstKind::PositionAddress { dst: position, address });
        let shift = self.constant(Scalar::U64, 32).regs[0];
        let upper = self.reg(Scalar::U64);
        self.emit(InstKind::Binary { dst: upper, op: BinaryOp::ShiftRightLogical, a: position, b: shift });
        let low = self.reg(Scalar::U32);
        let high = self.reg(Scalar::U32);
        self.emit(InstKind::Cast { dst: low, src: position });
        self.emit(InstKind::Cast { dst: high, src: upper });
        let frame = self.application_frame(span)?;
        let slot = self.pending_error.unwrap().slot();
        let status = self.reg(Scalar::U64);
        self.emit(InstKind::LoadLocal(status, slot));
        let error = self.reg(Scalar::U64);
        let value = |ty, values| CallArgument::Value { ty, values, origin: None, dataset_borrow: None };
        self.emit(InstKind::Call(ir::Call {
            target: CallTarget::Defined(name), failure: None, error: Some(error),
            arguments: vec![application::argument(frame), value(pointer, vec![low, high]),
                value(Ty::scalar(Scalar::U64), vec![status])],
            memory: Vec::new(), memory_use: None, result: None, values: Vec::new(),
        }));
        self.emit(InstKind::StoreLocal(slot, error));
        Ok(())
    }

    pub(super) fn native_element_drop(&mut self, value: Returned, retained: &view::Borrow, span: Span, origin: Arc<GeneratedOrigin>) -> Result<()> {
        let ty = self.returned_storage(&value).ty;
        let resources = match &value {
            Returned::Dataset(data) => vec![data],
            Returned::OwnedValue(value) => value.resources(),
            _ => Vec::new(),
        };
        for data in resources { self.application_movable(data, retained, span)?; }
        let (name,parameters,lexical,contracts)=self.declare_owned_cleanup(&ty,span)?;
        let contract=outputs::Contract::native(&lexical,&parameters,&contracts,&name,span);
        let result=TypeName::named("void");
        self.native_call(&name,CallSignature{parameters:&parameters,result:&result,contract,lexical:&lexical,
            kind:FunctionKind::Value,execution:crate::execution::Requirement::Invocation,c_abi:false},
            vec![Argument::resource(value)],span,Some(origin),None,None)?;
        Ok(())
    }
    fn native_dataset_shape(&self,param:&Param,definition:outputs::Contract<'_>,count:Option<u32>,span:Span)->Result<()>{
        if param.ty.count.is_some()&&self.world.shape(&param.ty.count,&definition.source,param.span,&definition.module,&definition.imports)?!=count{
            return self.fail(span,"B167","调用方没有声明函数需要的数据形状");
        }
        Ok(())
    }
    #[inline(never)]
    pub(super) fn native_call_memory(&self,name:&str,arguments:&BTreeMap<ValueIdentity,bounds::Proof>,project:impl Fn(usize)->bool)->Option<Vec<(view::Borrow,Option<Access>,Option<bounds::Proof>)>> {
        let access=self.functions.units.get(name)?.as_ref()?.contract.view_access.as_ref()?;
        let mut memory=Vec::new();
        for access in access {
            let(borrows,when,access,written)=match access {
                view_contracts::Access::Memory{borrows,when,access}|view_contracts::Access::Dataset{borrows,when,access,..}if access.writes()=>(borrows,when,*access,None),
                view_contracts::Access::Position{borrows,when,position}if position.written.is_some()=>(borrows,when,Access::Write,position.written.as_ref()),
                _=>continue,
            };
            let path=when.projected_case(true, arguments);
            if !path.possible(){continue;}
            if borrows.content_inputs(){return None;}
            let borrow=borrows.guards_at_call(arguments).map_sources(|source|match &source {
                view::Source::Position(origin)if matches!(&origin.root,address::Root::Parameter(parameter)if project(*parameter))=>view::Borrow::default(),
                _=>source.at_call(arguments),
            });
            let borrow=self.current_address_borrow(&borrow);
            memory.push((borrow.restricted(&path),Some(access),written.map(|proof|proof.at_call(arguments))));
        }
        Some(memory)
    }
    fn native_call(
        &mut self,
        name: &str,
        signature: CallSignature<'_>,
        args: Vec<Argument>,
        span: Span,
        generated_origin: Option<Arc<GeneratedOrigin>>,
        syntax: Option<&Expressions>,
        launch: Option<Value>,
    ) -> Result<Option<Returned>> {
        let live_task_places=if self.application_entry() {self.task_places()} else {view::Borrow::default()};
        let d=signature.contract;
        let bound_parameters=signature.parameters;
        let result_name=signature.result;
        let reference_writes = !self.functions.reference_read_only(name);
        let implicit = self.functions.units[name].as_ref()
            .is_none_or(|unit| unit.function.is_some() || unit.contract.frame);
        let mut arguments = Vec::new();
        // The interface retains its canonical Frame input slots even when its actual ABI omits Frame.
        // These absent compile-time facts do not create registers or machine arguments.
        let mut actual = vec![None; application::pointer_type().components.len()];
        if implicit && self.application_entry() {
            let frame = self.application_frame(span)?;
            actual = frame.regs.iter()
                .map(|reg| Some(bounds::Proof::default().with_identity(ValueIdentity::Register(*reg))))
                .collect();
            arguments.push(CallArgument::Value { #[cfg(feature = "compiler")] origin: Some(Arc::new(ArgumentOrigin::Frame)), dataset_borrow:None, ty: frame.ty, values: frame.regs });
        }
        let mut preparing=Vec::new();
        for (parameter,argument) in bound_parameters.iter().zip(&args) {
            if parameter.access!=Access::Value {continue;}
            let binding=match argument {
                Argument::OwnedValue(value) if value.ownership.may_own()=>Some(Binding::OwnedValue(value.clone())),
                Argument::ApplicationData(data) if data.ownership.may_own()=>Some(Binding::ApplicationData(data.clone())),
                _=>None,
            };
            if let Some(binding)=binding {preparing.push(self.hold_binding(binding));}
        }
        let mut env = BTreeMap::new();
        let mut locations = residency::Arguments::new();
        let mut written = Vec::new();
        let mut written_data = Vec::new();
        let mut written_stage = Vec::new();
        let mut held = Vec::new();
        let mut contents = BTreeMap::new();
        let mut resource_inputs=BTreeMap::new();
        let mut storage_inputs=BTreeMap::new();
        let mut memory_borrow=view::Borrow::default();
        let mut borrowed_arguments = BTreeMap::new();
        let mut retired_outputs=Vec::new();
        for (index, (p, arg)) in bound_parameters.iter().zip(args).enumerate() {
            let argument_origin = Arc::new(ArgumentOrigin::Parameter {
                index, name: p.name.to_owned(),
                site: syntax.and_then(|args| self.source_at(args.at(index).span(), SourceSyntax::Expression)),
            });
            if p.ty.is_texture_resource() {
                let Argument::Texture(resource) = arg else {
                    return self.fail(span, "B239", "只读纹理参数需要相同维度的纹理资源");
                };
                self.texture_parameter(p,&resource,(d.source,d.module,d.imports),span)?;
                let address = self.reg(Scalar::U64);
                self.emit(InstKind::TextureBindingAddress { dst: address, texture: resource.source });
                arguments.push(CallArgument::Value {
                    origin: Some(argument_origin.clone()), dataset_borrow: None,
                    ty: Ty::scalar(Scalar::U64), values: vec![address],
                });
                actual.push(None);
                env.insert(p.name.to_owned(), Binding::Texture(resource));
                continue;
            }
            let ty = cooperative::parameter_type(self.world,&p,(d.source,d.module,d.imports,signature.kind),Target::Cpu)?;
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
                    let source=self.data_value_borrow(&reference.data);
                    resource_inputs.insert(ValueIdentity::ResourceParameter(index,0),bounds::Proof::default().with_view_borrow(0,prefix::domain(&reference)));
                    let borrow=if matches!(p.access,Access::Read|Access::Atomic) && signature.execution==crate::execution::Requirement::Invocation {
                        let (_,ty)=reference.element_layout();
                        address::Origin{
                            root:address::Root::Derived(source),layout:address::Layout::of(ty),
                            base:address::Layout::of(&reference.data.ty),words:reference.words().into(),positions:Arc::from([]),
                            writable:p.access.writes(),initialized:vec![true;ty.components.len()].into(),
                        }.normalize()
                    }else{source};
                    reference.data.root.proof=reference.data.root.proof.with_view_borrow(0,borrow);
                    Argument::BorrowedData(reference)
                },
                other=>other,
            };
            let arg=match arg {Argument::Value(value) if p.access==Access::Read=>Argument::Reference(self.materialize_array(value,None)),other=>other};
            let position=match &arg {
                Argument::Reference(OwnedBinding{place,..})|Argument::OwnedReference{place,..}=>Some(place.clone()),
                Argument::OwnedValue(value) if launch.is_some() && p.access==Access::Read=>value.place(),
                _=>None,
            };
            if let Some(place)=position {
                if !matches!(&arg,Argument::Reference(_)) {
                    self.require_place_access(&place,p.access,span)?;
                }
                let borrow=self.position_borrows(&place,span)?;
                storage_inputs.insert(ValueIdentity::ParameterStorage(index),bounds::Proof::default().with_view_borrow(0,borrow));
            }
            match arg {
                Argument::Value(mut value) if p.access == Access::Value && !p.ty.dataset => {
                    self.require_same(&ty, &value.ty, span)?;
                    value.literal = false;
                    value.proof=self.current_address_proof(&value.ty,&value.proof);
                    memory_borrow.union(&self.address_contents(&value.ty,&value.proof));
                    append_actual(&mut actual, &value);
                    arguments.push(CallArgument::Value { #[cfg(feature = "compiler")] origin: Some(argument_origin.clone()),
                        dataset_borrow:None, ty: value.ty.clone(),
                        values: value.regs.clone(),
                    });
                    env.insert(p.name.to_owned(), Binding::Value(value));
                }
                Argument::OwnedReference{mut value,place,owner} if matches!(p.access,Access::Write|Access::InOut)=>{
                    self.require_same(&ty,&value.value.ty,span)?;
                    if let Some(owner)=owner{held.push(owner);}
                    for(part,address)in self.native_references(&place,p.access,reference_writes,span,signature.c_abi)?{arguments.push(CallArgument::Reference{ #[cfg(feature = "compiler")] origin: Some(argument_origin.clone()),ty:part,access:p.access,address});actual.extend([None,None]);}
                    if p.access==Access::InOut {
                        for data in value.resources(){
                            self.require_application_initialized(data,span)?;
                            if launch.is_some() {
                                let resource=view::resource_borrow(data);
                                self.require_view_unborrowed(&resource,span)?;
                                if self.application_position_borrowed(data.root.identity()) {
                                    return self.fail(span,"B227","独占任务交接前须结束dataset元素的位置借用");
                                }
                                self.application_cpu(data,span)?;
                            }
                        }
                        self.load_owned_value(&mut value,span)?;
                        value.value.proof=self.current_address_proof(&value.value.ty,&value.value.proof);
                        memory_borrow.union(&self.address_contents(&value.value.ty,&value.value.proof));
                        let proof=bounds::Proof::capture(&value.value);
                        for component in 0..ty.components.len(){contents.insert(ValueIdentity::ParameterContent(index,component),proof.project(&[component],ty.components.len()));}
                        residency::owned_argument(&mut locations,index,&mut value);
                        for(resource,path)in view::resource_paths(&ty).into_iter().enumerate(){let borrow=match view::owned_data_at(&value,&path){Some(data)=>self.data_value_borrow(data),None=>view::Borrow::from_sources(BTreeSet::from([view::Source::Inactive]))};resource_inputs.insert(ValueIdentity::ResourceParameter(index,resource),bounds::Proof::default().with_view_borrow(0,borrow));}
                    } else if launch.is_some() {
                        retired_outputs.push((place.clone(),Returned::OwnedValue(value.clone())));
                    } else {
                        self.drop_returned(Returned::OwnedValue(value.clone()),&view::Borrow::default(),span)?;
                    }
                    env.insert(p.name.to_owned(),Binding::OwnedValue(value));written.push((index,place));
                }
                Argument::Reference(mut owner) if matches!(p.access, Access::Read | Access::Write | Access::InOut) => {
                    let place=owner.place.clone();
                    self.require_same(&ty, place.ty(), span)?;
                    if p.access.reads(){owner.proof=self.current_address_proof(&ty,&owner.proof);memory_borrow.union(&self.address_contents(&ty,&owner.proof));}
                    for (part, address) in self.native_references(&place, p.access, reference_writes, span, signature.c_abi)? {
                        arguments.push(CallArgument::Reference { #[cfg(feature = "compiler")] origin: Some(argument_origin.clone()),
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
                    env.insert(p.name.to_owned(), Binding::Owned(owner));
                    if p.access.writes() { written.push((index, place)); }
                }
                Argument::Atomic(place) if p.access == Access::Atomic => {
                    self.require_same(&ty, place.ty(), span)?;
                    for (part, address) in self.native_references(&place, Access::Atomic, reference_writes, span, signature.c_abi)? {
                        arguments.push(CallArgument::Reference { #[cfg(feature = "compiler")] origin: Some(argument_origin.clone()), ty: part, access: Access::Atomic, address });
                        actual.extend([None, None]);
                    }
                    env.insert(p.name.to_owned(), Binding::AtomicBorrow(place));
                }
                Argument::Dataset(view) if p.ty.dataset && matches!(p.access,Access::Read|Access::Write|Access::Atomic|Access::InOut) => {
                    let binding=view.binding;
                    let access=view.access;
                    if !matches!((p.access,access),(Access::Read,Access::Read|Access::InOut)|(Access::Write,Access::Write|Access::InOut)|(Access::Atomic,Access::Atomic)|(Access::InOut,Access::InOut)) {
                        return self.fail(span,"B224","CPU范围的dataset借用需要对应的实际来源权限");
                    }
                    let (field,element)=self.dataset_layout(&view);let element=element.clone();
                    self.require_same(&ty,&element,span)?;
                    self.native_dataset_shape(p,d,self.program.buffers[binding].count,span)?;
                    let origin = Arc::new(GeneratedOrigin {
                        reason: GenerationReason::DatasetCallArgument,
                        trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
                    });
                    let zero=self.constant_with_origin(Scalar::U32,0,Some(origin.clone()));
                    let place=self.dataset_element(&view,zero.regs[0],false);
                    if p.access==Access::Write {
                        self.forget_output_initialization(&place,span)?;
                        written_stage.push((index,place.clone()));
                    }
                    let borrow=self.position_origin(&place,span)?.normalize();
                    memory_borrow.union(&borrow);
                    let pointer=if signature.execution==crate::execution::Requirement::Invocation {
                        let address=self.reg(Scalar::U64);
                        self.code.push(Inst::generated(InstKind::PositionAddress{dst:address,address:CallReference{
                            storage:CallStorage::Indexed(IndexedSource::Data{binding,index:zero.regs[0]}),offset:None,word:0,
                        }},origin.clone()));
                        Value{ty:Ty::scalar(Scalar::U64),regs:vec![address],proof:bounds::Proof::default().with_view_borrow(0,borrow),literal:false,uniform:true,constant:None}
                    }else{
                        let mut handle=self.constant_with_origin(Scalar::U32,binding as u64,Some(origin.clone()));
                        handle.proof=handle.proof.with_view_borrow(0,borrow);handle
                    };
                    let length=self.data_length(&view);
                    let step=self.reg(Scalar::U64);self.code.push(Inst::generated(InstKind::BufferStride(step,binding),origin.clone()));
                    let stride=Value{ty:Ty::scalar(Scalar::U64),regs:vec![step],proof:bounds::Proof::default(),literal:false,uniform:true,constant:None};
                    let offset=self.constant_with_origin(Scalar::U32,Bits::from(field),Some(origin.clone()));
                    let descriptor=Self::borrowed_dataset_parameter_type(p.access,signature.execution);
                    let stride=if signature.execution==crate::execution::Requirement::Invocation{stride}else{
                        let reg=self.reg(Scalar::U32);self.emit(InstKind::Cast{dst:reg,src:stride.regs[0]});
                        Value{ty:Ty::scalar(Scalar::U32),regs:vec![reg],proof:bounds::Proof::default(),literal:false,uniform:true,constant:None}
                    };
                    let mut proof=bounds::Proof::default();
                    for value in [&pointer,&length,&stride,&offset]{proof.append(value);}
                    let value=Value{ty:descriptor,regs:vec![pointer.regs[0],length.regs[0],stride.regs[0],offset.regs[0]],proof,literal:false,uniform:true,constant:None};
                    append_actual(&mut actual,&value);
                    arguments.push(CallArgument::Value{ #[cfg(feature = "compiler")] origin: Some(argument_origin.clone()),dataset_borrow:None, ty:value.ty,values:value.regs});
                    locations.insert(residency::Input::new(index),residency::Residency::cpu());
                    env.insert(p.name.to_owned(),Binding::Data(view));
                }
                Argument::BorrowedData(mut reference) if p.ty.dataset=>{
                    residency::argument(&mut locations,index,&mut reference.data,&reference.projection);
                    self.require_same(&ty,reference.element_layout().1,span)?;
                    if !matches!(p.access,Access::Read|Access::Write|Access::InOut|Access::Atomic){
                        return self.fail(span,"B224","CPU dataset借用需要read／write／inout／atomic");
                    }
                    if !reference.permits(p.access){
                        return self.fail(span,"B227","dataset借用的访问模式不能转换为被调参数权限");
                    }
                    if p.access!=Access::Write{self.require_application_reference_initialized(&reference,span)?;}
                    self.native_dataset_shape(p,d,reference.count,span)?;
                    if p.access==Access::Write||p.access.writes()&&outputs::declared(d,index){
                        self.prepare_data_output(&mut reference.data,span)?;
                        if launch.is_none() {
                            if p.access == Access::Write && reference.element_layout().1.owns_resources() {
                                self.retire_application_elements(&reference, span)?;
                            }
                            self.forget_data_output(&reference,span)?;
                        }
                    }
                    if p.access == Access::Write || p.access == Access::InOut
                        && (outputs::declared(d, index) || launch.is_some() && reference.element_layout().1.owns_resources())
                    {
                        written_data.push((index,reference.clone()));
                    }
                    let descriptor=Self::borrowed_dataset_parameter_type(p.access,signature.execution);
                    if launch.is_some() {
                        self.application_cpu(&reference.data, span)?;
                        borrowed_arguments.insert(arguments.len(), Some((reference.element_layout().1.clone(),p.access)));
                    }
                    let dataset_borrow=if descriptor.components[0].0==Scalar::U64 && reference.root.ty.components[0].0==Scalar::U32 {
                        let frame=self.application_frame(span)?;
                        Some(Box::new(ir::DatasetBorrow{frame:[frame.regs[0],frame.regs[1]],handle:reference.root.regs[0],input:actual.len(),call:self.view_model.calls.len()}))
                    }else{None};
                    let mut value=self.application_reference_value(&reference,descriptor,span)?;
                    let(field,ty)=reference.element_layout();
                    value.proof=value.proof.with_view_borrow(0,application::dataset_origin(&reference.data,field,ty,p.access.reads()).normalize());
                    if p.access==Access::Write&&signature.execution==crate::execution::Requirement::Invocation {
                        let borrow=if reference.root.ty.components[0].0==Scalar::U64&&reference.projection.is_empty() {
                            view::Borrow::source(view::Source::Parameter(reference.root.identity(),Vec::new()))
                        }else{
                            let layout=address::Layout::of(reference.element_layout().1);
                            let base=address::Layout::of(&reference.data.ty);
                            let words=reference.words();
                            let raw=reference.root.ty.components[0].0==Scalar::U64;
                            let root=if raw {address::Root::Derived(view::Borrow::source(view::Source::Parameter(reference.root.identity(),Vec::new())))}
                                else{address::Root::Resource{borrow:view::resource_borrow(&reference.data),index:None}};
                            let positions=if raw{Arc::from([])}else{
                                let scope=reference.initialization.as_ref().unwrap();
                                words.iter().map(|word|match(reference.length.minimum(),reference.length.maximum()){
                                    (0,0)=>Position::Initialized,
                                    (1,1)=>scope.application_word(Some(ownership::ApplicationIndex::Constant(0)),*word),
                                    _=>scope.application_word(None,*word),
                                }).collect()
                            };
                            address::Origin{root,base,words:words.into(),positions,writable:true,
                                initialized:vec![false;layout.parts.len()].into(),layout}.normalize()
                        };
                        value.proof=value.proof.with_view_borrow(0,borrow);
                    }
                    append_actual(&mut actual,&value);
                    arguments.push(CallArgument::Value{ #[cfg(feature = "compiler")] origin: Some(argument_origin.clone()),dataset_borrow,ty:value.ty,values:value.regs});
                    env.insert(p.name.to_owned(),Binding::DataReference(reference));
                }
                Argument::ApplicationData(mut data) if p.ty.dataset&&p.access==Access::Value=>{
                    residency::argument(&mut locations,index,&mut data,&[]);
                    self.require_same(&ty,&data.ty,span)?;
                    self.require_application_initialized(&data,span)?;
                    self.native_dataset_shape(p,d,data.count,span)?;
                    resource_inputs.insert(ValueIdentity::ResourceParameter(index,0),bounds::Proof::default().with_view_borrow(0,view::resource_borrow(&data)));
                    let mut value=self.returned_storage(&Returned::Dataset(data.clone()));
                    value.proof=value.proof.with_view_borrow(0,application::dataset_origin(&data,0,&data.ty,true).normalize());
                    append_actual(&mut actual,&value);
                    arguments.push(CallArgument::Value{ #[cfg(feature = "compiler")] origin: Some(argument_origin.clone()),dataset_borrow:None, ty:value.ty,values:value.regs});
                    env.insert(p.name.to_owned(),Binding::ApplicationData(data));
                }
                Argument::OwnedValue(mut value)
                    if matches!(p.access,Access::Value|Access::Read) && ty.owns_resources() =>
                {
                    self.require_same(&ty, &value.value.ty, span)?;
                    if p.access==Access::Read {
                        value.borrow();
                        if launch.is_some() && !view::resource_paths(&ty).is_empty() {
                            for data in value.resources() {
                                self.application_cpu(data,span)?;
                            }
                            borrowed_arguments.insert(arguments.len(), None);
                        }
                    }else if !value.all_available(){return self.fail(span,"B227","按值参数需要实际拥有全部活动资源");}
                    for data in value.resources(){self.require_application_initialized(data,span)?;}
                    value.resources_mut(&mut |data|data.initialization=None);
                    residency::owned_argument(&mut locations, index, &mut value);
                    for(resource,path)in view::resource_paths(&value.value.ty).into_iter().enumerate(){let borrow=match view::owned_data_at(&value,&path){Some(data)=>self.data_value_borrow(data),None=>view::Borrow::from_sources(BTreeSet::from([view::Source::Inactive]))};resource_inputs.insert(ValueIdentity::ResourceParameter(index,resource),bounds::Proof::default().with_view_borrow(0,borrow));}
                    value.value.proof=self.current_address_proof(&value.value.ty,&value.value.proof);
                    memory_borrow.union(&self.address_contents(&value.value.ty,&value.value.proof));
                    append_actual(&mut actual, &value.value);
                    arguments.push(CallArgument::Value { #[cfg(feature = "compiler")] origin: Some(argument_origin.clone()),
                        dataset_borrow:None, ty: value.value.ty.clone(),
                        values: value.value.regs.clone(),
                    });
                    env.insert(p.name.to_owned(), Binding::OwnedValue(value));
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
        let declared=self.functions.units[name].as_ref().map(|unit|unit.contract.element_inputs.clone());
        let element_paths=contracts::element_input_paths(self,d,&mut env,declared.as_ref())?;
        let (element_inputs,element_memory)=application::element_input_bindings(self,d,&env,&element_paths,declared.as_ref());
        memory_borrow.union(&element_memory);
        let requirements = contracts::resolve(self,d,signature.lexical,&env,&resource_inputs)?;
        contracts::prove(self, &requirements, span)?;
        let output_inputs=outputs::inputs(self,d,&env)?;
        let mut view_arguments=actual.iter().enumerate().filter_map(|(index,proof)|proof.clone().map(|proof|(ValueIdentity::Register(index),proof))).collect::<BTreeMap<_,_>>();
        view_arguments.extend(contents.clone());view_arguments.extend(resource_inputs.clone());view_arguments.extend(storage_inputs.clone());view_arguments.extend(element_inputs.clone());
        if !locations.is_empty() {
            let call = residency::Call {
                function: name.to_owned(),
                arguments: locations.clone(),
                bindings: view_arguments.iter().map(|(id,proof)|(*id,proof.numeric_facts())).collect(),
                path: self.residency_path(),
                source: Some(self.scope.source().clone()),
                span,
            };
            if !self.application_entry() {
                // This range has no application caller whose later closure could consume the call.
                // Consume the already closed imported contract with its actual CPU inputs now.
                let (known,outputs,_)=residency_contracts(self.world,&self.functions)?;
                if residency::check_calls(std::iter::once(&call),&known,&outputs)? {
                    return self.fail(span,"B270","CPU范围尚不能交接该已编译函数所需的条件存储位置");
                }
            } else {
                self.residency.calls.push(call);
            }
        }
        let result = if result_name.name == "void" || result_name.is_never() {
            None
        } else {
            Some(owned_values::storage_type(self.world,result_name,(d.source,d.module,d.imports),d.span())?)
        };
        if let Some(packet)=launch {
            let result=result.unwrap_or_else(Ty::unit);
            let mut captured=packet.proof.typed_view_borrow(&packet.ty);
            let mut readable=captured.clone();
            let mut initialized=view::Borrow::default();
            let mut conditional_positions=Vec::new();
            let mut dataset_writes=Vec::new();
            for (index,param) in bound_parameters.iter().enumerate() {
                if let Some(Binding::DataReference(data)) = env.get(&param.name) {
                    let (offset, ty) = data.element_layout();
                    let mut origin = application::dataset_origin(&data.data, offset, ty, param.access.reads());
                    origin.writable = param.access.writes();
                    let mut borrow = origin.normalize();
                    let parent = view::resource_borrow(&data.data);
                    for path in view::resource_paths(ty) {
                        let field = path.iter().fold(ty, |ty, &index| {
                            if ty.has_variants() { &ty.variants[index].ty } else { ty.aggregate_part(index).1 }
                        });
                        let Some(element) = field.dataset.as_deref() else { continue; };
                        let element = self.expected_storage(element, span)?;
                        let layout = address::Layout::of(&element);
                        let fields = data.projection.iter().copied().chain(path).collect();
                        borrow.union(&address::Origin {
                            root: address::Root::Dataset {
                                borrow: view::Borrow::element(parent.clone(), fields, None),
                                index: None,
                            },
                            words: element.components.iter().map(|(_, word)| *word).collect(),
                            initialized: vec![true; layout.parts.len()].into(),
                            layout: layout.clone(),
                            base: layout,
                            positions: Arc::from([]),
                            writable: false,
                        }.normalize());
                    }
                    if param.access.reads(){readable.union(&borrow);}
                    if param.access.writes(){dataset_writes.push((param.access, data.clone()));}
                    captured.union(&borrow);
                    continue;
                }
                let Some(proof)=storage_inputs.get(&ValueIdentity::ParameterStorage(index)) else {continue;};
                let borrow=proof.view_borrow(0).map_sources(|source| match source {
                    view::Source::Position(mut origin) if param.access==Access::Read => {
                        Arc::make_mut(&mut origin).writable=false;
                        view::Borrow::source(view::Source::Position(origin))
                    },
                    source=>view::Borrow::source(source),
                });
                if param.access.reads(){readable.union(&borrow);}
                if matches!(param.access,Access::Read|Access::InOut) && let Some(Binding::OwnedValue(value))=env.get(&param.name) {
                    let mut contents=value.value.proof.typed_view_borrow(&value.value.ty);
                    for data in value.resources() {
                        let mut origin=application::dataset_origin(data,0,&data.ty,true);
                        origin.root=address::Root::Dataset{borrow:view::resource_borrow(data),index:None};
                        origin.writable=param.access.writes();
                        contents.union(&origin.normalize());
                    }
                    readable.union(&contents);
                    captured.union(&contents);
                }
                if param.access.writes() && outputs::declared(d,index) {
                    conditional_positions.extend(self.positions(&written.iter().find(|(parameter,_)|*parameter==index).unwrap().1));
                } else if param.access==Access::Write
                    || param.access==Access::InOut&&written.iter().any(|(parameter,place)|
                        *parameter==index&&place.ty().owns_resources()) {
                    initialized.union(&borrow);
                }
                captured.union(&borrow);
            }
            if let Some(unit)=self.functions.units[name].as_ref() {
                let mut bindings=view_arguments.clone();
                if let Some(returned)=&unit.contract.returned {
                    for (component,reg) in returned.registers().into_iter().enumerate() {
                        bindings.insert(ValueIdentity::Register(reg),bounds::Proof::default().with_identity(ValueIdentity::TaskResult(component)));
                    }
                }
                for fact in unit.contract.initialization.iter().flatten() {
                    if let address::InitializationSource::Position(origin)=&fact.source
                        && let address::Root::Parameter(index)=origin.root
                        && outputs::declared(d,index)
                    {
                        let fact=fact.at_call(&bindings);
                        initialized.union(&fact.source.borrow().restricted(&fact.when.assume(&bounds::Clause::default(),true)));
                    }
                }
            }
            let contract=self.functions.units[name].as_ref().map(|unit|&unit.contract);
            let returned=contract.and_then(|contract|contract.returned.as_ref()).map_or_else(Vec::new,Returned::registers);
            let mut guards=written.iter().filter(|(_,place)|place.ty().owns_resources())
                .map(|(parameter,place)|{
                    let model=contract.and_then(|contract|contract.written.get(*parameter)?.clone());
                    let target=self.position_origin(place,span)?.normalize();
                    outputs::TaskGuard::new(self,d,*parameter,place.ty(),target,&output_inputs,&result,span,model,&view_arguments,&returned)
                        .map(|guard|(*parameter,guard))
                }).collect::<Result<Vec<_>>>()?;
            for (parameter,data) in written_data {
                let (offset,ty)=data.element_layout();
                let mut origin=application::dataset_origin(&data.data,offset,ty,false);
                if let Some(scope) = &data.initialization {
                    origin.positions=data.words().into_iter().map(|word|scope.application_word(None,word)).collect();
                    if bound_parameters[parameter].access == Access::Write || outputs::declared(d, parameter) {
                        conditional_positions.extend(origin.positions.iter().cloned());
                    }
                }
                let model = contract.and_then(|contract| match contract.written.get(parameter)?.as_ref()? {
                    Returned::Dataset(data) => data.elements.known().cloned(),
                    _ => None,
                });
                let guard=outputs::TaskGuard::new(self,d,parameter,ty,origin.normalize(),&output_inputs,
                    &result,span,model,&view_arguments,&returned)?;
                guards.push((parameter,guard));
            }
            let task_components=result.task_components();
            let mut outputs=vec![view::Borrow::default();result.components.len()];
            for &component in &task_components {
                let id=ValueIdentity::TaskReturnSource(component);
                view_arguments.insert(id,bounds::Proof::default().with_identity(id));
            }
            if let Some(value)=contract.and_then(|contract|contract.returned.as_ref()).and_then(Returned::view_value) {
                for component in task_components {
                    outputs[component]=value.proof.view_borrow(component);
                }
            }
            // These are the same actual output locations the task hands off or destroys.
            let mut transferred = Vec::new();
            if let Some(contract) = contract {
                if let Some(value) = &contract.returned {
                    transferred.push((residency::OutputKind::Returned, value.clone()));
                }
                transferred.extend(contract.written.iter().enumerate().filter_map(|(parameter, value)| {
                    value.clone().map(|value| (residency::OutputKind::Written(parameter), value))
                }));
            }
            let path = self.residency_path();
            for (kind, mut output) in transferred {
                residency::output(&mut output, name, kind, &locations, &view_arguments);
                residency::visit(&mut output, &mut residency::Input::new(0), &mut |data, _| {
                    if data.ownership.may_own() {
                        let active = data.ownership.presence().condition().assume(&path, true);
                        self.residency.uses.push(residency::Use::At {
                            value: data.residency.clone(), target: Target::Cpu, path: active,
                            source: self.scope.source().clone(), span,
                        });
                    }
                });
            }
            let memory=self.call_address_memory([(readable,Some(Access::Read))],span)?;
            let call=self.view_model.call(name,memory,view_arguments,None,
                address::initialization_effects(&self.initialized,&bounds::Clause::default()),
                self.path.clone(),self.scope.source().clone(),span,true,live_task_places);
            let started=self.emit_task_start(name,packet,arguments,captured,result,outputs,initialized,guards,conditional_positions,borrowed_arguments,dataset_writes,retired_outputs,memory,call,span)?;
            for owner in preparing { self.env.remove(&owner); }
            return Ok(started);
        }
        let values = result
            .iter()
            .flat_map(|ty| &ty.components)
            .map(|(scalar, _)| self.reg(*scalar))
            .collect::<Vec<_>>();
        let views_closed=self.functions.units[name].as_ref().is_some_and(|unit|unit.contract.view_access.is_some());
        let memory=if !self.application_entry(){
            let access=self.functions.units[name].as_ref().unwrap().contract.view_access.as_ref().unwrap().clone();
            self.native_dataset_access(&access,&view_arguments,span)?
        }else{Vec::new()};
        let content_context=self.functions.units[name].as_ref().is_some_and(|unit|{
            unit.contract.returned.iter().chain(unit.contract.written.iter().flatten())
                .filter_map(Returned::view_value).any(|value|value.proof.borrowed_view().content_inputs())
                ||unit.contract.view_access.as_ref().is_some_and(|access|access.iter().any(view_contracts::Access::content_context))
        });
        let memory_use=self.view_model.uses.len();
        let(borrow,snapshots)=if content_context {
            let kind=if self.functions.read_only(name){address::Capture::Read}else{address::Capture::Write};
            (memory_borrow,Some(self.capture_address_values(memory_use,kind).into()))
        }else{match self.native_call_memory(name,&view_arguments,|_|false) {
            Some(mut memory)=>{
                let borrow=memory.iter().fold(view::Borrow::default(),|mut result,(borrow,_,_)|{result.union(borrow);result}).restricted(&self.path);
                let snapshots=if borrow.pending()&&!self.resolved_address_write(&borrow){
                    Some(self.capture_address_values(memory_use,address::Capture::Write).into())
                }else{
                    for(source,_,_)in &mut memory{*source=source.restricted(&self.path);}
                    self.write_address_values(&borrow,&memory)?;None
                };
                (borrow,snapshots)
            },
            None=>(memory_borrow,Some(self.capture_address_values(memory_use,address::Capture::Write).into())),
        }};
        self.check_view_requirement(view::Requirement::Memory{target:address::MemoryTarget::Call(memory_use),borrow,access:Some(Access::InOut),written:None,snapshots:snapshots.clone(),span,address_fields:Arc::from([])})?;
        let source_site = if generated_origin.is_none() { self.source_site.clone() } else { None };
        let task_results=result.as_ref().map(Ty::task_components).unwrap_or_default();
        let mut machine_inputs=Vec::new();
        let mut completion_slots=BTreeMap::new();
        let mut completion_inputs=BTreeMap::new();
        if self.application_entry() && !signature.c_abi {
            let task_parameters=task_completion_parameters(arguments.iter().map(|argument| match argument {
                CallArgument::Value{ty,..}=>(ty.components.len(),Some(ty)),
                CallArgument::Reference{..}=>(2,None),
            }),if implicit{0}else{2});
            if !task_parameters.is_empty() {
                machine_inputs.resize(if implicit{0}else{2},None);
                for argument in &arguments {
                    match argument {
                        CallArgument::Value{values,..}=>machine_inputs.extend(values.iter().copied().map(Some)),
                        CallArgument::Reference{..}=>machine_inputs.extend([None,None]),
                    }
                }
            }
            let first_completion=actual.len();
            let mut completion_sources=Vec::<(view::Borrow,Option<Reg>)>::new();
            for completion in task_parameters {
                let output=actual.len();
                let result=completion.result;
                let (source,forwarded)=match completion.parent {
                    Some((parent,component))=>{
                        let (parent_source,identity)=&completion_sources[parent];
                        let source=parent_source.task_projection(view::TaskProjection::Returned(component,None));
                        let forwarded=identity.and_then(|parent|
                            self.task_completion(TaskCompletionSource::Returned(parent,component),&result))
                            .map(|address|(address,Some(address[0])));
                        (source,forwarded)
                    },
                    None=>{
                        let source=actual[completion.input].as_ref().map_or_else(view::Borrow::default,|proof|proof.view_borrow(0));
                        let register=machine_inputs[completion.input].unwrap();
                        let forwarded=self.task_completion_destination(register,&result,&source);
                        (source,forwarded)
                    },
                };
                completion_sources.push((source.clone(),forwarded.and_then(|(_,identity)|identity)));
                let layout=self.task_wait_output_layout(&result,&source,span)?.task_completion_storage().ok_or_else(||Diagnostic{
                    source:self.scope.source().clone(),span,code:"B224",message:"任务完成布局过大".into(),
                })?;
                let storage=if let Some((address,identity))=forwarded {
                    if let Some(identity)=identity {
                        for component in 0..result.components.len()+result.task_components().len() {
                            completion_inputs.insert(ValueIdentity::TaskCompletion(output,component),
                                bounds::Proof::default().with_identity(ValueIdentity::TaskCompletion(identity,component)));
                        }
                    }
                    CallStorage::Indexed(IndexedSource::View{address,index:self.constant(Scalar::U32,0).regs[0],stride:ElementStride::Constant(0)})
                } else {
                    let (slots,range)=self.local_slots(&layout);
                    self.initialize_task_completion(&layout,&slots);
                    let parent=completion.parent.and_then(|(parent,component)|completion_slots.get(&(first_completion+2*parent))
                        .map(|output:&Arc<TaskCompletionOutput>|(output.clone(),component)));
                    completion_slots.insert(output,Arc::new(TaskCompletionOutput{result,layout:layout.clone(),slots,task:source,parent,span}));
                    CallStorage::Local{base:range.start,len:range.len()}
                };
                arguments.push(CallArgument::Reference{origin:None,ty:layout,access:Access::InOut,
                    address:CallReference{storage,offset:None,word:0}});
                actual.extend([None,None]);
            }
        }
        let task_return_slots=if !task_results.is_empty()&&self.application_entry()&&!signature.c_abi {
            let layout=Ty::task_return_layout(&task_results);
            let (slots,range)=self.local_slots(&layout);
            arguments.push(CallArgument::Reference{origin:None,ty:layout,access:Access::Write,
                address:CallReference{storage:CallStorage::Local{base:range.start,len:range.len()},offset:None,word:0}});
            actual.extend([None,None]);
            slots
        }else{Vec::new()};
        for argument in &mut arguments {
            if let CallArgument::Value{dataset_borrow:Some(borrow),..}=argument {borrow.call=self.view_model.calls.len();}
        }
        for owner in preparing {self.env.remove(&owner);}
        let mut failure=self.failure_continuation(written.iter().filter(|(_,place)|place.ty().owns_resources()).map(|(_,place)|place.clone()).collect());
        if let Some(FailureContinuation::Scope(scope))=&mut failure {
            scope.task_outputs.extend(completion_slots.values().filter(|output|output.has_owned_outputs()).map(|output|(**output).clone()));
        }
        self.code.push(Inst { kind: InstKind::Call(Call {
            failure,
            error: None,
            target: CallTarget::Defined(name.to_owned()),
            arguments,
            memory,
            memory_use:Some(memory_use),
            result: result.clone(),
            values: values.clone(),
        }), float_preserve: true, source_site, generated_origin });
        // 已闭合合同按本次实参交接来源；未闭合函数才借原调用图等待结果。
        let prototype = self.functions.units[name]
            .as_ref()
            .map(|unit| {
                (
                    unit.contract.inputs.clone(),
                    unit.contract.returned.clone(),
                    unit.contract.written.clone(),
                )
            });
        let input_width=actual.len();
        let (mut bindings, mut model, writes) = if let Some((inputs, model, writes)) = prototype {
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
        bindings.extend(completion_inputs);
        let task_return_sources=task_results.into_iter().zip(task_return_slots).map(|(component,slot)| {
            let reg=self.reg(Scalar::U32);
            self.emit(InstKind::LoadLocal(reg,slot));
            bindings.insert(ValueIdentity::TaskReturnSource(component),bounds::Proof::default().with_identity(ValueIdentity::Register(reg)));
            (component,reg)
        }).collect::<Vec<_>>();
        let initialized=self.functions.units[name].as_ref().and_then(|unit|unit.contract.initialization.clone());
        let mut guarded=BTreeSet::new();
        for fact in initialized.iter().flatten() {
            let address::InitializationSource::Task(borrow)=&fact.source else{continue;};
            if fact.when.guaranteed_at_call(&bindings).truth_in(&self.path)!=Some(true){continue;}
            for (source,when) in borrow.sources.entries() {
                if let view::Source::TaskOutput{task,projection:view::TaskProjection::Initialized(returned)}=source
                    && bounds::Condition::guard(when).guaranteed_at_call(&bindings).truth_in(&self.path)==Some(true) {
                    guarded.insert((task.at_call(&bindings),returned.clone()));
                }
            }
        }
        let needed=guarded.iter().flat_map(|(_,returned)|returned.iter().flatten()).filter_map(|id|match id {
            ValueIdentity::TaskCompletion(task,_)=>Some(*task),_=>None,
        }).collect::<BTreeSet<_>>();
        for (&task,output) in &completion_slots {
            if !needed.contains(&task){continue;}
            for (component,reg) in output.value(self).regs.into_iter().enumerate() {
                bindings.insert(ValueIdentity::TaskCompletion(task,component),bounds::Proof::default().with_identity(ValueIdentity::Register(reg)));
            }
        }
        view_arguments.extend(bindings.clone());
        let view_call=Some(self.view_model.call(name,memory_use,view_arguments.clone(),snapshots,address::initialization_effects(&self.initialized,&bounds::Clause::default()),self.path.clone(),self.scope.source().clone(),span,false,live_task_places));
        if result_name.is_never() {
            self.flow = Flow::STOP;
            return Ok(None);
        }
        let effects=match initialized {
            Some(facts)=>facts.iter().map(|fact|address::InitializationEffect::Position(fact.at_call(&bindings))).collect::<Vec<_>>(),
            None=>vec![address::InitializationEffect::Call{call:view_call.unwrap(),when:bounds::Condition::constant(true)}],
        };
        self.initialized.extend(effects.into_iter().flat_map(|effect|effect.into_positions(&self.path)));
        for (task,returned) in guarded {
            let completion=returned.iter().flatten().find_map(|id|match id {
                ValueIdentity::TaskCompletion(input,_)=>completion_slots.get(input),_=>None,
            });
            let task=completion.map_or(task,|output|output.selected_task(self));
            self.complete_task_owned_places(&task,|builder,guard|guard.result_at_call(builder,&returned,&bindings),None,
                |builder,inputs|match completion {
                    Some(output)=>output.complete_inputs(builder,inputs),None=>Ok(()),
                },span)?;
        }
        for (&task,output) in &completion_slots {
            if !needed.contains(&task) && output.has_owned_outputs() {
                output.complete_if_ready(self)?;
            }
        }
        let output_value=result.as_ref().filter(|_|!written.is_empty()||!written_stage.is_empty()||!written_data.is_empty()).map(|ty|Value{ty:ty.clone(),regs:values.clone(),
            proof:model.as_ref().map(|model|bounds::Proof::capture(&self.returned_storage(model)).at_call(&bindings)).unwrap_or_default(),
            uniform:false,literal:false,constant:None});
        for(index,p)in bound_parameters.iter().enumerate(){
            if p.ty.dataset && matches!(p.access,Access::Write|Access::InOut) && let Some(Binding::DataReference(reference))=env.get(&p.name){
                let mut data=reference.data.clone();data.ty=reference.element_layout().1.clone();data.elements=application::ElementFacts::Unknown;data.prefixes.clear();
                let mut updated=Returned::Dataset(data);
                if let Some(Some(model))=writes.get(index){translate_returned(&mut updated,model,&bindings,false);}
                residency::output(&mut updated,name,residency::OutputKind::Written(index),&locations,&view_arguments);
                let Returned::Dataset(mut updated)=updated else{unreachable!()};
                let ty = reference.element_layout().1;
                let conditional = outputs::declared(d, index);
                if p.access == Access::Write || conditional {
                    let guards = outputs::guards(self, d, index, ty, &output_inputs, output_value.as_ref())?;
                    if conditional && ty.owns_resources() {
                        let mut model = self.element_model(ty, &updated.elements, (&updated.residency, &[]), span)?;
                        outputs::apply_ownership(&mut model, ty, 0, &guards);
                        updated.elements = application::ElementFacts::Known(model);
                    }
                    self.initialize_data_output(reference, &guards);
                }
                if !self.functions.read_only(name) {
                    let origin=Arc::new(GeneratedOrigin {
                        reason:GenerationReason::DatasetCallComplete,
                        trigger:self.source_site.clone().map(GenerationAnchor::Syntax),
                    });
                    let snapshots=env.values().filter_map(|binding|match binding { Binding::Value(value)=>Some(value),Binding::OwnedValue(value)=>Some(&value.value),_=>None }).collect::<Vec<_>>();
                    self.complete_application_elements(reference,updated.elements.known(),&snapshots,span,origin)?;
                }
                self.apply_application_dataset_model(reference,updated.elements)?;
                let offset=reference.element_layout().0;
                let facts=updated.prefixes.into_iter().map(|(word,fact)|(offset+word,fact)).collect();
                self.apply_application_prefixes(reference,&facts)?;
            }
        }
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
            let mut owned_output=None;
            if place.ty().owns_resources(){
                let regs=written_registers[&index].clone();
                let mut result=self.template_returned(Value{ty:place.ty().clone(),regs,proof:proof.clone(),uniform:false,literal:false,constant:None},true)?;
                if let Some(Some(model))=writes.get(index){translate_returned(&mut result,model,&bindings,false);residency::returned(&mut result,&locations);}
                residency::output(&mut result,name,residency::OutputKind::Written(index),&locations,&view_arguments);
                if !views_closed && let Some(call)=view_call{view_contracts::set_resource_outputs(&mut result,|resource|self.view_model.result(call,view_contracts::Output::WrittenResource(index,resource)));}
                if outputs::declared(d,index){outputs::apply_ownership(&mut result,place.ty(),0,&guards);}
                let Returned::OwnedValue(mut value)=result else{unreachable!()};proof=bounds::Proof::capture(&value.value);value.value.proof=proof.clone();owned_output=Some(Returned::OwnedValue(value));
            }
            self.assign_owned_proof(&place, proof, false)?;
            if let Some(value)=owned_output{self.assign_owned_place(&place,value)?;}
            self.initialize_output(&place,&guards);
        }
        for (index,place) in written_stage {
            let guards=outputs::guards(self,d,index,place.ty(),&output_inputs,output_value.as_ref())?;
            self.initialize_output(&place,&guards);
        }
        if syntax.is_some(){self.view_uses.after(span);}
        let Some(ty) = result else {
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
        let mut returned = self.template_returned(value, true)?;
        if let Some(model) = &model {
            translate_returned(&mut returned, model, &bindings,false);
            if !task_return_sources.is_empty() && let (Some(value),Some(model))=(returned.view_value_mut(),model.view_value()) {
                for &(component,source) in &task_return_sources {
                    let borrow=model.proof.view_borrow(component);
                    let mut projected=BTreeMap::new();
                    for original in &borrow.sources {
                        let view::Source::TaskOutput{task,projection:view::TaskProjection::Returned(part,
                            Some(ValueIdentity::TaskCompletion(input,slot)))}=original else{continue;};
                        let Some(output)=completion_slots.get(input) else{continue;};
                        let marker=self.reg(Scalar::U32);
                        self.emit(InstKind::LoadLocal(marker,output.slots[3+slot]));
                        let actual=task.at_call(&bindings).task_projection(view::TaskProjection::Returned(*part,None));
                        let selected=self.bind_work_task_return_sources(&actual,*part,marker,0,&|builder,word|
                            output.payload_source(builder,word));
                        projected.insert(original.clone(),selected);
                    }
                    let at_call=|source:&view::Source|projected.get(source).cloned()
                        .unwrap_or_else(||source.clone().at_call(&bindings));
                    let selected=self.select_task_return_sources(&borrow,component,source,|source,when| {
                        let when=bounds::Condition::guard(when.clone()).projected_case(true, &bindings);
                        at_call(source).restricted(&when)
                    });
                    if let Some(selected)=selected {
                        value.proof=value.proof.clone().with_view_borrow(component,selected);
                    }else if !projected.is_empty(){
                        let actual=borrow.guards_at_call(&bindings).map_sources(|source|at_call(&source));
                        value.proof=value.proof.clone().with_view_borrow(component,actual);
                    }
                }
            }
            residency::returned(&mut returned, &locations);
        }
        residency::output(&mut returned, name,residency::OutputKind::Returned, &locations,&view_arguments);
        if !views_closed && let Some(call)=view_call{view_contracts::set_resource_outputs(&mut returned,|resource|self.view_model.result(call,view_contracts::Output::ReturnedResource(resource)));}
        if let (Some(call),Some(value))=(view_call,returned.view_value_mut()) {
            for component in view_contracts::components(&value.ty){
                let actual=value.proof.view_borrow(component);
                let resolved=self.view_model.result(call,view_contracts::Output::Returned(component));
                if actual.content_inputs()||actual.empty()||actual.pending()||actual.unresolved(){value.proof=value.proof.clone().with_view_borrow(component,resolved);}
            }
        }
        if let Some(model)=&mut model {
            fn input_elements(location:&residency::Residency,definition:outputs::Contract<'_>,env:&BTreeMap<String,Binding>)->application::ElementFacts {
                match location {
                    residency::Residency::Selected(when,a,b)=>input_elements(a,definition,env).selected(input_elements(b,definition,env),when),
                    residency::Residency::Absent=>application::ElementFacts::Unknown,
                    residency::Residency::Origins(origins)=>{
                        let mut result:Option<application::ElementFacts>=None;
                        for origin in origins {
                            let facts=if let residency::Origin::Input(input)=origin {
                                let binding=&env[&definition.parameter_name(input.parameter)];
                                let mut paths=input.fields.split(|part|!matches!(part,residency::Part::Field(_)))
                                    .map(|fields|fields.iter().filter_map(|part|match part{residency::Part::Field(field)=>Some(*field),_=>None}).collect::<Vec<_>>()).collect::<Vec<_>>();
                                if let Binding::DataReference(reference)=binding && let Some(path)=paths.get_mut(1){path.splice(0..0,reference.projection.iter().copied());}
                                contracts::selected_data(binding,&paths).map(|data|data.elements.clone()).unwrap_or_default()
                            }else{application::ElementFacts::Unknown};
                            result=Some(match result{Some(old)=>old.common(facts),None=>facts});
                        }
                        result.unwrap_or_default()
                    },
                }
            }
            for resource in view_contracts::resource_output_paths(model){
                let source=view_contracts::resource_output_mut(model,&resource).unwrap();
                if matches!(source.elements,application::ElementFacts::Input(_)) && let Some(target)=view_contracts::resource_output_mut(&mut returned,&resource){
                    let location=source.residency.at_call(&bindings);
                    target.elements=input_elements(&location,d,&env);
                    if let Some(location)=location.bind(&locations){target.residency=location;}
                }
            }
        }
        let pending=match &returned {
            Returned::OwnedValue(value)=>Some(self.hold_binding(Binding::OwnedValue(value.clone()))),
            Returned::Dataset(data)=>Some(self.hold_binding(Binding::ApplicationData(data.clone()))),
            _=>None,
        };
        let borrow=returned.view_value().map(|value|value.proof.typed_view_borrow(&value.ty)).unwrap_or_default();
        for name in held {
            let resources=self.env.get(&name).into_iter().flat_map(Binding::application_resources).filter(|data|data.ownership==application::Ownership::Owned).map(|data|data.root.identity()).collect::<Vec<_>>();
            for identity in resources {
                let resource=self.data_borrow(&[identity]);
                self.check_view_requirement(view::Requirement::Unborrowed{resource,borrow:borrow.clone(),kind:view::Exclusion::Ownership,span})?;
            }
            self.finish_scoped_argument(&name, span)?;
        }
        if let Some(value)=returned.view_value() {
            let resources=match &returned{Returned::OwnedValue(value)=>value.resources(),_=>Vec::new()};
            self.require_view_available(value,&resources,span)?;
        }
        if let Some(pending)=pending {self.env.remove(&pending);}
        Ok(Some(returned))
    }
    pub(super) fn bind_work_task_return_sources(
        &mut self,borrow:&view::Borrow,component:usize,origin:Reg,base:u32,
        load:&impl Fn(&mut Self,u32)->Reg,
    )->view::Borrow {
        let mut projected=BTreeMap::new();
        for source in &borrow.sources {
            let view::Source::TaskOutput{task,projection:view::TaskProjection::Returned(part,
                Some(ValueIdentity::TaskWorkCompletion(payload,word)))}=source else {continue;};
            let payload=base+payload;
            let marker=load(self,payload+word);
            let actual=task.task_projection(view::TaskProjection::Returned(*part,None));
            let selected=self.bind_work_task_return_sources(&actual,*part,marker,payload,load);
            projected.insert(source.clone(),selected);
        }
        self.bind_task_return_sources(borrow,component,origin,|source|
            projected.get(&source).cloned().unwrap_or_else(||view::Borrow::source(source)))
    }

    fn bind_task_return_sources(
        &mut self,borrow:&view::Borrow,component:usize,origin:Reg,
        translate:impl Fn(view::Source)->view::Borrow,
    )->view::Borrow {
        let bindings=BTreeMap::from([(ValueIdentity::TaskReturnSource(component),ValueIdentity::Register(origin))]);
        self.select_task_return_sources(borrow,component,origin,|source,when|
            translate(source.clone()).restricted(&when.output_bound(&bindings)))
            .unwrap_or_else(||borrow.bind_guards(&bindings).map_sources(translate))
    }
    fn select_task_return_sources(
        &mut self, borrow:&view::Borrow, component:usize, origin:Reg,
        translate:impl Fn(&view::Source,&bounds::Clause)->view::Borrow,
    )->Option<view::Borrow> {
        if !borrow.inputs.is_empty()||!borrow.calls.is_empty()||!borrow.reads.is_empty(){return None;}
        let alternatives=borrow.sources.entries().map(|(source,when)| {
            let origins=when.canonical().task_return_sources(component)?;
            Some((translate(source,&when),origins))
        }).collect::<Option<Vec<_>>>()?;
        let groups=alternatives.iter().filter(|(task,origins)|task.active()&&!origins.is_empty())
            .map(|(_,origins)|origins.clone()).collect::<BTreeSet<_>>();
        if groups.len()<2{return None;}
        let comparisons=if groups.len()==2&&groups.first()?.is_disjoint(groups.last()?){1}else{groups.len()};
        let choices=groups.into_iter().take(comparisons).map(|origins| {
            let mut tests=origins.iter().map(|&source| {
                let constant=self.constant(Scalar::U32,Bits::from(source)).regs[0];
                let selected=self.reg(Scalar::Bool);
                self.emit(InstKind::Binary {
                    dst:selected,op:BinaryOp::Compare(Comparison::Equal),a:origin,b:constant,
                });
                selected
            }).collect::<Vec<_>>().into_iter();
            let selected=tests.next().unwrap();
            let selected=tests.fold(selected,|left,right| {
                let dst=self.reg(Scalar::Bool);
                self.emit(InstKind::Binary{dst,op:BinaryOp::Or,a:left,b:right});
                dst
            });
            (origins,selected)
        }).collect::<Vec<_>>();
        let mut selected=view::Borrow::default();
        for (task,origins) in alternatives {
            for source in origins {
                let mut task=task.clone();
                for (group,condition) in &choices {
                    task=task.restricted(&bounds::Clause::task_control(*condition,group.contains(&source)));
                }
                selected.union(&task);
            }
        }
        Some(selected)
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
            value.prefixes=model.prefixes.iter().map(|(word,fact)|(*word,fact.at_call(bindings))).collect();
            value.count = model.count;
            value.elements=model.elements.clone().map(&mut |model|{
                let element_bindings=application::element_scope_bindings(bindings,model.registers().len(),stored);
                let mut value=model.clone();translate_returned(&mut value,&model,&element_bindings,true);value
            },&mut |when|when.at_call(bindings));
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
    if let Some(value)=&contract.returned{for(resource,borrow)in view_contracts::resource_outputs(value){outputs.insert(view_contracts::Output::ReturnedResource(resource),borrow);}}
    for(parameter,value)in contract.written.iter().enumerate(){if let Some(value)=value{for(resource,borrow)in view_contracts::resource_outputs(value){outputs.insert(view_contracts::Output::WrittenResource(parameter,resource),borrow);}}}
    view_contracts::Interface{outputs,access:contract.view_access.as_ref().unwrap().clone(),initialization:contract.initialization.clone().unwrap()}
}
// Query the original code and call bindings; the returned Proof remains the owner.
fn returned_input_aliases(module:&Module,name:&str)->Vec<(usize,usize)> {
    fn walk<'a>(code:&'a [Inst],visit:&mut impl FnMut(&'a Inst)->bool)->bool {
        code.iter().all(|inst|visit(inst)&&match &inst.kind {
            InstKind::Scope(body)|InstKind::Cleanup{body,..}=>walk(body,visit),
            InstKind::If{yes,no,..}=>walk(yes,visit)&&walk(no,visit),
            InstKind::Loop{condition,body,..}=>walk(condition,visit)&&walk(body,visit),
            _=>true,
        })
    }
    fn part(model:&Returned,component:usize)->Option<(&Value,usize)> {
        match model {
            Returned::Value(value)=>Some((value,component)),
            Returned::OwnedValue(value)=>Some((&value.value,component)),
            Returned::Dataset(value)=>match component{0=>Some((&value.root,0)),1=>Some((&value.length,0)),_=>None},
            Returned::Unit=>None,
        }
    }
    fn output(module:&Module,name:&str,component:usize,input:usize,active:&mut BTreeSet<(String,Reg,Reg)>)->bool {
        let Some(unit)=module.units.get(name).and_then(Option::as_ref)else{return false;};
        if let Some(function)=&unit.function {
            let(Some(value),Some(input))=(function.body.outputs.get(component),function.body.inputs.get(input))else{return false;};
            same(module,name,*value,*input,active)
        }else{
            unit.contract.returned.as_ref().and_then(|model|part(model,component))
                .is_some_and(|(value,component)|value.component_origins(component).contains(&ValueIdentity::Register(input)))
        }
    }
    fn same(module:&Module,name:&str,value:Reg,input:Reg,active:&mut BTreeSet<(String,Reg,Reg)>)->bool {
        if value==input{return true;}
        let unit=module.units[name].as_ref().unwrap();
        let function=unit.function.as_ref().unwrap();
        let body=&function.body;
        if body.registers[value]!=body.registers[input]{return false;}
        let key=(name.to_owned(),value,input);
        if !active.insert(key.clone()){return true;}
        let mut producer=None;
        walk(&body.code,&mut|inst|{
            let found=match &inst.kind {
                InstKind::LoadLocal(dst,_)|InstKind::Cast{dst,..}|InstKind::Select{dst,..}=>*dst==value,
                InstKind::Call(call)=>call.values.contains(&value),
                _=>false,
            };
            if found{producer=Some(inst);}!found
        });
        let result=match producer.map(|inst|&inst.kind) {
            Some(InstKind::LoadLocal(_,slot))=>{
                let mut stored=false;
                let same=walk(&body.code,&mut|inst|match &inst.kind {
                    InstKind::StoreLocal(target,value)if target==slot=>{stored=true;same(module,name,*value,input,active)},
                    _=>!inst.local_writes().contains(slot),
                });
                stored&&same
            },
            Some(InstKind::Cast{dst,src})=>body.registers[*dst]==body.registers[*src]&&same(module,name,*src,input,active),
            Some(InstKind::Select{a,b,..})=>same(module,name,*a,input,active)&&same(module,name,*b,input,active),
            Some(InstKind::Call(call))=>{
                if let CallTarget::Defined(callee)=&call.target {
                    let context=call.memory_use.and_then(|memory|unit.view_model.calls.iter().find(|context|context.memory==memory));
                    let component=call.values.iter().position(|reg|*reg==value).unwrap();
                    let callee_unit=module.units.get(callee).and_then(Option::as_ref);
                    context.zip(callee_unit).is_some_and(|(context,callee_unit)|{
                        (0..callee_unit.contract.inputs.len()).any(|parameter|{
                            context.arguments.get(&ValueIdentity::Register(parameter)).is_some_and(|proof|{
                                proof.origins.first().is_some_and(|origins|origins.iter().any(|id|match id{
                                    ValueIdentity::Register(reg)=>same(module,name,*reg,input,active),
                                    _=>false,
                                }))&&output(module,callee,component,parameter,active)
                            })
                        })
                    })
                }else{false}
            },
            _=>false,
        };
        active.remove(&key);result
    }
    let unit=module.units[name].as_ref().unwrap();
    let Some(function)=&unit.function else{return Vec::new();};
    (0..function.body.outputs.len()).flat_map(|component|{
        (0..function.body.inputs.len()).filter_map(move|input|{
            output(module,name,component,input,&mut BTreeSet::new()).then_some((component,input))
        })
    }).collect()
}

fn resolve_views(world:&World,module:&mut Module,root:Option<&str>)->Result<()> {
    fn scalar_values(value:&mut Value,bindings:&BTreeMap<ValueIdentity,bounds::Proof>){
        value.proof=value.proof.substitute_values(bindings);
        value.constant=value.constant.take().or_else(||value.proof.known_value(&value.ty));
    }
    fn returned_values(value:&mut Returned,bindings:&BTreeMap<ValueIdentity,bounds::Proof>){
        match value {
            Returned::Unit=>{},
            Returned::Value(value)=>scalar_values(value,bindings),
            Returned::Dataset(value)=>{
                scalar_values(&mut value.root,bindings);scalar_values(&mut value.length,bindings);
                for fact in value.prefixes.values_mut(){*fact=fact.substitute_values(bindings);}
                if let Some(elements)=value.elements.known_mut(){returned_values(elements,bindings);}
            },
            Returned::OwnedValue(value)=>{
                scalar_values(&mut value.value,bindings);
                for payload in value.payloads.iter_mut().flatten(){returned_values(payload,bindings);}
            },
        }
    }

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
    let owns_input = |function: &str, id| module.owns_input(function, id);
    let outputs = if let Some(observe) = &world.worker.compiler().borrow_solve_observer {
        use crate::compiler::{BorrowSolveEvent, BorrowSolveFunction, BorrowSolvePhase};
        let participating: Vec<_> = functions.keys().map(|name| {
            let unit = module.units[name].as_ref().unwrap();
            BorrowSolveFunction { name, source: &unit.source, span: unit.span }
        }).collect();
        observe(BorrowSolveEvent { phase: BorrowSolvePhase::Started, functions: &participating });
        let storage = |function: &str, storage| observe(BorrowSolveEvent {
            phase: BorrowSolvePhase::Storage { function, storage },
            functions: &participating,
        });
        let result = view_contracts::solve(&functions, closed, owns_input, Some(&storage));
        observe(BorrowSolveEvent {
            phase: BorrowSolvePhase::Finished { succeeded: result.is_ok() },
            functions: &participating,
        });
        result
    } else {
        view_contracts::solve(&functions, closed, owns_input, None)
    }?;
    for (name,memory)in &outputs.memory {
        let aliases=returned_input_aliases(module,name);
        let unit=module.units.get_mut(name).unwrap().as_mut().unwrap();
        let contract=&mut unit.contract;
        let interface=&outputs.interfaces[name];
        if interface.outputs.values().any(view::Borrow::has_local) {
            return unit.source.fail(unit.span,"B227","地址不能返回或输出当前函数的局部存储");
        }
        contract.view_access=Some(interface.access.clone());
        contract.initialization=Some(interface.initialization.clone());
        if let Some(values)=outputs.values.get(name){
            for model in contract.returned.iter_mut().chain(contract.written.iter_mut().flatten()){returned_values(model,values);}
        }

        if let Some(model)=&mut contract.returned{for(component,input)in aliases{
            let(value,component)=match model{
                Returned::Value(value)=>(value.as_mut(),component),
                Returned::OwnedValue(value)=>(&mut value.value,component),
                Returned::Dataset(value)=>if component==0{(&mut value.root,0)}else{(&mut value.length,0)},
                Returned::Unit=>unreachable!(),
            };
            value.proof.origins.resize_with(value.regs.len(),Vec::new);
            let origins=&mut value.proof.origins[component];let id=ValueIdentity::Register(input);
            if !origins.contains(&id){origins.push(id);}
        }}
        for (output,borrow)in &interface.outputs {
            match output {
                view_contracts::Output::Returned(component)=>if let Some(value)=contract.returned.as_mut().and_then(Returned::view_value_mut) {value.proof=value.proof.clone().with_view_borrow(*component,borrow.clone());},
                view_contracts::Output::Written(parameter,component)=>if let Some(value)=contract.written[*parameter].as_mut().and_then(Returned::view_value_mut) {value.proof=value.proof.clone().with_view_borrow(*component,borrow.clone());},
                view_contracts::Output::ReturnedResource(resource)=>if let Some(value)=contract.returned.as_mut().and_then(|value|view_contracts::resource_output_mut(value,resource)){
                    value.root.proof=value.root.proof.clone().with_view_borrow(0,borrow.clone());
                },
                view_contracts::Output::WrittenResource(parameter,resource)=>if let Some(value)=contract.written[*parameter].as_mut().and_then(|value|view_contracts::resource_output_mut(value,resource)){value.root.proof=value.root.proof.clone().with_view_borrow(0,borrow.clone());},
            }
        }
        if let Some(function)=&mut unit.function {
            address::apply_memory(&mut function.body.code,memory,&function.body.locals);
        }
    }
    Ok(())
}
// Borrow current contracts; own decoded dependencies from compiled definitions,
// including private callees without reconstructing their source declarations.
fn residency_contracts(world:&World,module:&Module)->Result<(BTreeMap<String,residency::Contract>,residency::Outputs,BTreeMap<String,Vec<u8>>)> {
    use crate::module::codec::Decode;
    use std::borrow::Cow;
    let mut contracts=module.units.iter().filter_map(|(name,unit)|unit.as_ref().map(|unit|(name.clone(),Cow::Borrowed(&unit.contract)))).collect::<BTreeMap<_,_>>();
    let mut imports=BTreeMap::new();let mut pending=contracts.keys().cloned().collect::<Vec<_>>();
    while let Some(name)=pending.pop(){
        let contract=&contracts[&name];
        for callee in contract.residency_dependencies(){
            if contracts.contains_key(&callee){continue;}
            let (definition,bytes)=world.functions.values().find_map(|definition|definition.compiled.as_ref().and_then(|definitions|definitions.get(&callee)).map(|bytes|(definition,bytes)))
                .ok_or_else(||Diagnostic{source:module.units.values().find_map(|unit|unit.as_ref()).unwrap().source.clone(),span:Span::default(),code:"B270",message:format!("已编译模块没有位置合同定义 {callee}")})?;
            let contract=Contract::from_bytes(bytes).map_err(|error|Diagnostic{source:definition.source.clone(),span:definition.function.span(),code:"B270",message:format!("模块函数接口错误：{error}")})?;
            contracts.insert(callee.clone(),Cow::Owned(contract));imports.insert(callee.clone(),bytes.clone());pending.push(callee);
        }
    }
    let outputs=residency::output_sources(contracts.iter().map(|(name,contract)|(name.as_str(),contract.returned.as_ref(),contract.written.as_slice())));
    Ok((contracts.into_iter().map(|(name,contract)|(name,match contract{Cow::Borrowed(contract)=>contract.residency.clone(),Cow::Owned(contract)=>contract.residency})).collect(),outputs,imports))
}
// 每个原函数持有自身Uses／Calls；调用者消费实际实参，不穷举递归后代。
fn resolve_residencies(world:&World,module: &mut Module) -> Result<BTreeMap<String,Vec<u8>>> {
    // Definition inputs retain their requirements in the existing call contract.
    fn unbound(value:&residency::Residency)->bool{match value{
        residency::Residency::Absent=>true,
        residency::Residency::Origins(origins)=>!origins.is_empty()&&origins.iter().all(|origin|matches!(origin,residency::Origin::Input(_))),
        residency::Residency::Selected(_,a,b)=>unbound(a)&&unbound(b),
    }}
    resolve_views(world,module,None)?;
    let (mut known,outputs,imports)=residency_contracts(world,module)?;
    for(name,unit)in &mut module.units{let unit=unit.as_mut().unwrap();if unit.function.is_none(){continue;}
        let parameters=residency::input_parameters(&unit.view_model.parameters,unit.contract.inputs.len());
        unit.contract.residency=residency::infer(&unit.residency,&parameters,&outputs,&unit.source,unit.span)?;
        known.insert(name.clone(),unit.contract.residency.clone());
    }
    for(name,unit)in &mut module.units{let unit=unit.as_mut().unwrap();if unit.function.is_none(){continue;}
        let parameters=residency::input_parameters(&unit.view_model.parameters,unit.contract.inputs.len());
        let frame_width=application::pointer_type().components.len();
        let pending=|id:&ValueIdentity|parameters.get(id).and_then(|proof|proof.known_identity(0)).is_some_and(|id|match id{
            ValueIdentity::Register(index)=>index>=frame_width,
            _=>true,
        });
        residency::check_calls(unit.residency.calls.iter().filter(|call|
            !call.arguments.values().all(unbound)
            && !call.bindings.values().any(|proof|proof.scalar_identities().iter().any(&pending))
            && !bounds::Proof::condition(bounds::Condition::guard(call.path.clone())).scalar_identities().iter().any(&pending)
        ),&known,&outputs)?;
        residency::check_loops(name,&unit.residency,&unit.contract.residency,&parameters,&outputs,&known,unit.input_access.len())?;
        for written in &mut unit.contract.written{if let Some(value)=written{if !residency::resolve_returned(value,&outputs){*written=None;}}}
        if let Some(returned)=&mut unit.contract.returned{if !residency::resolve_returned(returned,&outputs){unit.contract.returned=None;}}
    }
    Ok(imports)
}
pub(super) fn calls(code: &mut [Inst], visit: &mut impl FnMut(&mut Call)) {
    for inst in code {
        match &mut (inst).kind {
            InstKind::Call(call) => visit(call),
            InstKind::Scope(body) | InstKind::Cleanup{body,..} => calls(body, visit),
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

fn program_dependencies(program: &Program) -> BTreeSet<String> {
    fn collect(code: &[Inst], names: &mut BTreeSet<String>) {
        crate::optimize::visit(code, &mut |inst| {
            if let InstKind::Call(Call { target: CallTarget::Defined(name), .. })
                | InstKind::FunctionAddress(_, CallTarget::Defined(name)) = &inst.kind
            { names.insert(name.clone()); }
        });
    }
    let mut names = BTreeSet::new();
    for stage in &program.stages {
        collect(&stage.code, &mut names);
        let reduction = match &stage.operation {
            Operation::CustomReduce(value) => Some(value),
            Operation::CustomScan(value) => Some(&value.values),
            Operation::CustomSegmentedReduce(value) => Some(&value.values),
            _ => None,
        };
        if let Some(reduction) = reduction {
            for body in reduction.project.iter().chain([&reduction.identity, &reduction.combine]) {
                collect(&body.code, &mut names);
            }
        }
    }
    names
}

pub(super) struct Finalized {
    pub functions: Vec<FunctionBody>,
    pub imports: BTreeMap<String, Vec<u8>>,
    pub exports: BTreeMap<String, Vec<u8>>,
    pub resources: crate::application::Resources,
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

// The original call keeps its actual Frame binding only until the same module's
// Dataset contract closes. No function body or resource graph is reconstructed.
fn dataset_write_regions(borrow:&DatasetBorrow,context:&view_contracts::Call,contract:&Contract)->BTreeSet<(u32,u32)> {
let mut regions=BTreeSet::new();
for effect in contract.view_access.as_ref().unwrap() {
    let view_contracts::Access::Dataset{borrows,when,access,..}=effect else{continue};
    if !access.writes(){continue;}
    let path=bounds::Clause::both(&context.path, &when.projected_case(true, &context.arguments));
    if !path.possible(){continue;}
    for(source,when)in borrows.sources.entries() {
        let path=bounds::Clause::both(&path, &bounds::Condition::guard(when).projected_case(true, &context.arguments));
        if !path.possible(){continue;}
        let view::Source::Position(origin)=source else{continue};
        for(source,when)in origin.resources().sources.entries() {
            if !matches!(source,view::Source::Data(ValueIdentity::Register(input))|view::Source::Parameter(ValueIdentity::Register(input),_)if *input==borrow.input){continue;}
            if !bounds::Clause::both(&path, &bounds::Condition::guard(when).projected_case(true, &context.arguments)).possible(){continue;}
            regions.extend(origin.layout.parts.iter().zip(origin.words.iter()).map(|((scalar,_),word)|(*word,scalar.size())));
        }
    }
}
    regions
}
fn lower_dataset_borrows<'a>(
    body:&mut ValueCode,
    model:&view_contracts::Function,
    contract:&impl Fn(&str)->&'a Contract,
) {
    fn register(registers:&mut Vec<Scalar>,scalar:Scalar)->Reg {
        let reg=registers.len();registers.push(scalar);reg
    }
    fn constant(code:&mut Vec<InstKind>,registers:&mut Vec<Scalar>,scalar:Scalar,bits:Bits)->Reg {
        let reg=register(registers,scalar);code.push(InstKind::Constant(reg,scalar,bits));reg
    }
    fn value(ty:Ty,values:Vec<Reg>)->CallArgument {CallArgument::Value{ #[cfg(feature = "compiler")] origin: None,ty,values,dataset_borrow:None}}
    fn block<'a>(code:&mut [Inst],registers:&mut Vec<Scalar>,model:&view_contracts::Function,contract:&impl Fn(&str)->&'a Contract) {
        for instruction in code {
            let mut before=Vec::new();
            match &mut instruction.kind {
                InstKind::Scope(code)=>block(code,registers,model,contract),
                InstKind::If{yes,no,..}=>{block(yes,registers,model,contract);block(no,registers,model,contract);}
                InstKind::Loop{condition,body,..}=>{block(condition,registers,model,contract);block(body,registers,model,contract);}
                InstKind::Call(call)=>{
                    for argument in &mut call.arguments {
                        let CallArgument::Value{values,dataset_borrow,..}=argument else{continue};
                        let Some(borrow)=dataset_borrow.take() else{continue};
                        let context=&model.calls[borrow.call];
                        let regions=dataset_write_regions(&borrow,context,contract(&context.function));
                        for(word,bytes)in regions {
                            let field=register(registers,Scalar::U64);
                            before.push(InstKind::Cast{dst:field,src:values[3]});
                            let word=constant(&mut before,registers,Scalar::U64,Bits::from(word));
                            let offset=register(registers,Scalar::U64);
                            before.push(InstKind::Binary{dst:offset,op:BinaryOp::IAdd,a:field,b:word});
                            let bytes=constant(&mut before,registers,Scalar::U32,Bits::from(bytes));
                            let result=application::pointer_type();
                            let outputs=result.components.iter().map(|(scalar,_)|register(registers,*scalar)).collect();
                            before.push(InstKind::Call(Call{
                                failure:None,error:None,
                                target:CallTarget::External(ExternalName{library:crate::application::RUNTIME_LIBRARY.into(),symbol:"blink_address".into()}),
                                arguments:vec![
                                    value(application::pointer_type(),borrow.frame.to_vec()),
                                    value(Ty::scalar(Scalar::U32),vec![borrow.handle]),
                                    value(Ty::scalar(Scalar::U64),vec![offset]),
                                    value(Ty::scalar(Scalar::U32),vec![bytes]),
                                    value(Ty::scalar(Scalar::U64),vec![values[2]]),
                                    value(Ty::scalar(Scalar::U32),vec![values[1]]),
                                ],
                                memory:Vec::new(),memory_use:None,result:Some(result),values:outputs,
                            }));
                        }
                    }
                }
                _=>{}
            }
            if !before.is_empty() {
                let original=std::mem::replace(&mut instruction.kind,InstKind::Scope(Vec::new()));
                before.push(original);
                instruction.kind=InstKind::Scope(before.into_iter().map(|kind|Inst{
                    source_site:instruction.source_site.clone(),generated_origin:instruction.generated_origin.clone(),..Inst::new(kind)
                }).collect());
            }
        }
    }
    block(&mut body.code,&mut body.registers,model,contract);
}

// The existing Module owns the closed effect; no second call graph is built.
fn failure_effects(module:&mut Module) {
    loop {
        let known=module.units.iter().filter_map(|(name,unit)|unit.as_ref().unwrap().contract.fallible.then(||name.clone())).collect::<BTreeSet<_>>();
        let mut required=BTreeSet::new();
        for(name,unit)in &module.units {
            let unit=unit.as_ref().unwrap();
            if let Some(function)=&unit.function {
                crate::optimize::visit(&function.body.code,&mut |inst|if let InstKind::Call(call)=&inst.kind {
                    if call.error.is_some() || matches!(&call.target,CallTarget::Defined(name)if known.contains(name)) {
                        required.insert(name.clone());
                    }
                });
            }
        }
        let mut changed=false;
        for name in required {
            let contract=&mut module.units.get_mut(&name).unwrap().as_mut().unwrap().contract;
            changed|=!contract.fallible;contract.fallible=true;
        }
        if !changed{break;}
    }
}
struct FailureContext<'a> {
    program: &'a Program,
    output_scopes: &'a mut usize,
    view_model: &'a mut view_contracts::Function,
    residency: &'a mut residency::Inputs,
    task_completions: Option<&'a BTreeMap<TaskCompletionSource, Vec<(Ty, [Reg; 2])>>>,
    range: bool,
}

fn failure_scope(
    world:&World,scope:FailureScope,error:Reg,registers:&mut Vec<Scalar>,
    locals:&mut BTreeMap<usize,Ty>,context:&mut FailureContext<'_>,module:&mut Module,existing:Option<usize>,
)->Result<Vec<Inst>> {
    let mut b=Builder::from_checked(world,&scope.scope,context.program,scope.domain,
        scope.source_definition,scope.source_context,scope.mutable_names);
    b.application=Some(application::Context { frame: scope.frame, range: context.range });
    if let Some(completions)=context.task_completions {b.task_completions=completions.clone();}
    b.env=scope.env;b.path=scope.path;b.initialized=scope.initialized;
    b.control_uniform=scope.control_uniform;b.calls=scope.calls;
    b.source_site=scope.source_site;b.in_function=true;
    b.registers=std::mem::take(registers);b.locals=std::mem::take(locals);
    b.output_scopes=*context.output_scopes;
    b.functions=std::mem::take(module);
    b.view_model=std::mem::take(context.view_model);
    b.residency=std::mem::take(context.residency);
    let initialization=b.view_model.initialization.len();
    let slot=match existing{Some(slot)=>slot,None=>{let slot=b.local_slots(&Ty::scalar(Scalar::U64)).0[0];b.emit(InstKind::StoreLocal(slot,error));slot}};
    b.pending_error=Some(PendingError::Exit(slot));
    // Failure cancels ordinary future uses; the original F consumers remain live.
    b.env.retain(|name,binding|!name.starts_with('\0')||match binding {
        Binding::ApplicationData(data)=>data.ownership.may_own(),
        Binding::OwnedValue(value)=>value.ownership.may_own(),
        _=>false,
    });
    for place in scope.consumed {
        if b.borrowed_element_storage(&place) {
            let arguments = b.error_position_arguments(&place, error, b.scope.span())?;
            b.emit(application::runtime_call("error_consume", arguments, None, Vec::new(), None, None).kind);
        }
        b.move_owned_place(&place)?;
    }
    for output in scope.task_outputs {output.complete_if_ready(&mut b)?;}
    b.release_application_data(None,view::Borrow::default())?;
    // A failed call keeps the caller's original location facts for surviving loans.
    let loans = b.env.values().filter_map(|binding| {
        let Binding::ApplicationData(data) = binding else { return None; };
        let application::DataLayout::Parameter { parameter, .. } = data.layout else { return None; };
        data.returns_loan_on_error()
            .then(|| (parameter, Returned::Dataset(data.clone())))
    }).collect::<Vec<_>>();
    let path = b.residency_path();
    for (parameter, mut loan) in loans {
        b.normalize_element_facts(&mut loan, b.scope.span())?;
        residency::visit(&mut loan, &mut residency::Input::new(parameter), &mut |data, input| {
            if data.ownership.may_own() {
                let entry = residency::Residency::input(input.model());
                if entry != data.residency {
                    let active = data.ownership.presence().condition().assume(&path, true);
                    b.residency.same(&entry, &data.residency, b.scope.source(), b.scope.span(), &active);
                }
            }
        });
    }
    let result=b.reg(Scalar::U64);
    b.emit(InstKind::LoadLocal(result,slot));
    b.emit(InstKind::Raise(result));
    // These writes happen on a failed execution, never promise normal outputs.
    b.view_model.initialization.truncate(initialization);
    for(memory,usage)in b.view_model.uses.iter().enumerate(){
        let count=match &usage.requirement{view::Requirement::Memory{snapshots:Some(snapshots),..}=>snapshots.len(),view::Requirement::Dataset{values:Some(_),..}=>1,_=>0};
        for component in 0..count{
            let id=ValueIdentity::MemoryValue(memory,component);b.view_model.parameters.insert(id,id);
        }
    }
    *context.output_scopes=b.output_scopes;
    *context.view_model=b.view_model;
    *context.residency=b.residency;
    *registers=b.registers;*locals=b.locals;*module=b.functions;
    Ok(b.code)
}
fn lower_failure_calls(
    world:&World,body:&mut ValueCode,mut context:Option<FailureContext<'_>>,
    known:&BTreeSet<String>,mut module:Option<&mut Module>,
)->Result<(bool,bool)> {
    fn fold_empty_status(code:&mut [Inst],slot:usize) {
        let mut reads=BTreeSet::new();
        crate::optimize::visit(code,&mut |inst| {
            if let InstKind::LoadLocal(reg,source)=&inst.kind && *source==slot {
                reads.insert(*reg);
            }
        });
        fn fold(code:&mut [Inst],reads:&BTreeSet<Reg>) {
            for inst in code {
                match &mut inst.kind {
                    InstKind::LoadLocal(reg,_) if reads.contains(reg) => {
                        inst.kind=InstKind::Constant(*reg,Scalar::U64,0);
                    }
                    InstKind::Call(call)
                        if matches!(&call.target,CallTarget::External(name)
                            if name.library==crate::application::RUNTIME_LIBRARY && name.symbol=="blink_error_consumed")
                            && matches!(call.arguments.first(),Some(CallArgument::Value{values,..}) if reads.contains(&values[0])) =>
                    {
                        inst.kind=InstKind::Constant(call.values[0],Scalar::Bool,0);
                    }
                    InstKind::Scope(body)|InstKind::Cleanup{body,..}=>fold(body,reads),
                    InstKind::If{yes,no,..}=>{fold(yes,reads);fold(no,reads);}
                    InstKind::Loop{condition,body,..}=>{fold(condition,reads);fold(body,reads);}
                    _=>{}
                }
            }
        }
        fold(code,&reads);
    }

    fn block(
        world:&World,code:&mut Vec<Inst>,registers:&mut Vec<Scalar>,locals:&mut BTreeMap<usize,Ty>,
        context:&mut Option<FailureContext<'_>>,known:&BTreeSet<String>,module:&mut Option<&mut Module>,
    )->Result<(bool,bool)> {
        let mut out=Vec::new();let mut changed=false;let mut scoped=false;
        for mut inst in std::mem::take(code) {
            if let InstKind::Cleanup{slot,body,failure}=&mut inst.kind {
                let slot=*slot;
                let mut can_fail=false;
                crate::optimize::visit(body,&mut |inst|if let InstKind::Call(call)=&inst.kind {
                    can_fail|=call.error.is_some()||matches!(&call.target,CallTarget::Defined(name)if known.contains(name));
                });
                let mut body=std::mem::take(body);let failure=failure.take();
                if !can_fail {
                    // No call can publish an error in this group, so its error consumers see zero.
                    fold_empty_status(&mut body,slot);
                    inst.kind=InstKind::Scope(body);out.push(inst);changed=true;continue;
                }
                let site=inst.source_site.clone();let origin=inst.generated_origin.clone();
                let generated=|kind|Inst{source_site:site.clone(),generated_origin:origin.clone(),..Inst::new(kind)};
                let zero=registers.len();registers.push(Scalar::U64);
                let error=registers.len();registers.push(Scalar::U64);
                let failed=registers.len();registers.push(Scalar::Bool);
                out.push(generated(InstKind::Constant(zero,Scalar::U64,0)));
                out.push(generated(InstKind::StoreLocal(slot,zero)));
                out.push(generated(InstKind::Scope(body)));
                out.push(generated(InstKind::LoadLocal(error,slot)));
                out.push(generated(InstKind::Binary{dst:failed,op:BinaryOp::Compare(Comparison::NotEqual),a:error,b:zero}));
                let yes=match failure {
                    Some(scope)=>{
                        scoped=true;
                        failure_scope(world,*scope,error,registers,locals,context.as_mut().unwrap(),module.as_deref_mut().unwrap(),Some(slot))?
                    }
                    None=>vec![generated(InstKind::Raise(error))],
                };
                out.push(generated(InstKind::If{condition:failed,yes,no:Vec::new()}));
                changed=true;continue;
            }
            let nested=match &mut inst.kind {
                InstKind::Scope(body)=>block(world,body,registers,locals,context,known,module)?,
                InstKind::If{yes,no,..}=>{
                    let a=block(world,yes,registers,locals,context,known,module)?;
                    let b=block(world,no,registers,locals,context,known,module)?;
                    (a.0||b.0,a.1||b.1)
                }
                InstKind::Loop{condition,body,..}=>{
                    let a=block(world,condition,registers,locals,context,known,module)?;
                    let b=block(world,body,registers,locals,context,known,module)?;
                    (a.0||b.0,a.1||b.1)
                }
                _=>(false,false),
            };
            changed|=nested.0;scoped|=nested.1;
            let failure=if let InstKind::Call(call)=&mut inst.kind {
                if call.failure.is_some()&&(call.error.is_some()||matches!(&call.target,CallTarget::Defined(name)if known.contains(name))) {
                    let error=*call.error.get_or_insert_with(||{let reg=registers.len();registers.push(Scalar::U64);reg});
                    Some((error,call.failure.take().unwrap()))
                }else{None}
            }else{None};
            let site=inst.source_site.clone();let origin=inst.generated_origin.clone();
            let generated=|kind|Inst{source_site:site.clone(),generated_origin:origin.clone(),..Inst::new(kind)};
            out.push(inst);
            if let Some((error,continuation))=failure {
                changed=true;
                let yes=match continuation {
                    FailureContinuation::Return=>vec![generated(InstKind::Raise(error))],
                    FailureContinuation::Scope(scope)=>{
                        scoped=true;
                        failure_scope(world,*scope,error,registers,locals,context.as_mut().unwrap(),module.as_deref_mut().unwrap(),None)?
                    }
                    FailureContinuation::Cleanup(slot)=>{
                        let previous=registers.len();registers.push(Scalar::U64);
                        let joined=registers.len();registers.push(Scalar::U64);
                        let ty=Ty::scalar(Scalar::U64);
                        let argument=|value|CallArgument::Value{origin:None,dataset_borrow:None,ty:ty.clone(),values:vec![value]};
                        let zero=registers.len();registers.push(Scalar::U64);
                        let present=registers.len();registers.push(Scalar::Bool);
                        let joined=vec![
                            generated(application::runtime_call("error_join",vec![argument(previous),argument(error)],Some(ty),vec![joined],None,None).kind),
                            generated(InstKind::StoreLocal(slot,joined)),
                        ];
                        vec![
                            generated(InstKind::LoadLocal(previous,slot)),
                            generated(InstKind::Constant(zero,Scalar::U64,0)),
                            generated(InstKind::Binary{dst:present,op:BinaryOp::Compare(Comparison::NotEqual),a:previous,b:zero}),
                            generated(InstKind::If{condition:present,yes:joined,no:vec![generated(InstKind::StoreLocal(slot,error))]}),
                        ]
                    }
                };
                let zero=registers.len();registers.push(Scalar::U64);
                let failed=registers.len();registers.push(Scalar::Bool);
                out.push(generated(InstKind::Constant(zero,Scalar::U64,0)));
                out.push(generated(InstKind::Binary{dst:failed,op:BinaryOp::Compare(Comparison::NotEqual),a:error,b:zero}));
                out.push(generated(InstKind::If{condition:failed,yes,no:Vec::new()}));
            }
        }
        *code=out;Ok((changed,scoped))
    }
    block(world,&mut body.code,&mut body.registers,&mut body.locals,&mut context,known,&mut module)
}
fn lower_range_failures(
    world: &World, program: &mut Program, scopes: &mut BTreeMap<usize, RangeContext>,
    known: &BTreeSet<String>, module: &mut Module,
) -> Result<(bool, bool)> {
    let mut changed = false;
    let mut scoped = false;
    let mut stages = std::mem::take(&mut program.stages);
    for (&index, scope) in scopes {
        let stage = &mut stages[index];
        let mut body = ValueCode {
            entry_origin: None, generated_origin: None,
            registers: std::mem::take(&mut stage.registers),
            locals: std::mem::take(&mut stage.locals),
            code: std::mem::take(&mut stage.code),
            inputs: Vec::new(), outputs: Vec::new(),
        };
        let owner = FailureContext {
            program,
            output_scopes: &mut scope.output_scopes,
            view_model: &mut scope.view_model,
            residency: &mut scope.residency,
            task_completions: None,
            range: true,
        };
        let result = lower_failure_calls(world, &mut body, Some(owner), known, Some(module))?;
        changed |= result.0;
        scoped |= result.1;
        stage.registers = body.registers;
        stage.locals = body.locals;
        stage.code = body.code;
    }
    program.stages = stages;
    Ok((changed, scoped))
}

fn lower_module_failures(
    world: &World, module: &mut Module,
    programs: &mut [(&mut Program, &mut BTreeMap<usize, RangeContext>)],
) -> Result<()> {
    loop {
        resolve_views(world,module,None)?;
        failure_effects(module);
        let known=module.units.iter().filter_map(|(name,unit)|unit.as_ref().unwrap().contract.fallible.then(||name.clone())).collect::<BTreeSet<_>>();
        let names=module.units.keys().cloned().collect::<Vec<_>>();
        let mut changed=false;let mut scoped=false;
        for name in names {
            let mut unit=module.units.get_mut(&name).unwrap().take().unwrap();
            if let Some(mut function)=unit.function.take() {
                let owner = unit.context.as_mut().map(|context| FailureContext {
                    program: &context.program,
                    output_scopes: &mut context.output_scopes,
                    view_model: &mut unit.view_model,
                    residency: &mut unit.residency,
                    task_completions: Some(&context.task_completions),
                    range: false,
                });
                let result=lower_failure_calls(world,&mut function.body,owner,&known,Some(&mut *module))?;
                changed|=result.0;scoped|=result.1;unit.function=Some(function);
            }
            *module.units.get_mut(&name).unwrap()=Some(unit);
        }
        for (program, scopes) in programs.iter_mut() {
            let result = lower_range_failures(world, program, scopes, &known, module)?;
            changed |= result.0;
            scoped |= result.1;
        }
        for index in 0..module.programs.len() {
            let mut pending = module.programs[index].take().unwrap();
            let result = lower_range_failures(world, &mut pending.program, &mut pending.scopes, &known, module)?;
            changed |= result.0;
            scoped |= result.1;
            module.programs[index] = Some(pending);
        }
        if scoped {
            for unit in module.units.values_mut() {
                let unit=unit.as_mut().unwrap();
                if unit.function.is_some(){unit.contract.view_access=None;}
            }
        }
        if !changed{break;}
    }
    // The Dataset contract is complete; address preparation cannot fail.
    let names=module.units.keys().cloned().collect::<Vec<_>>();
    for name in names {
        let mut unit=module.units.get_mut(&name).unwrap().take().unwrap();
        if let Some(function)=&mut unit.function {
            let own=&unit.contract;
            lower_dataset_borrows(&mut function.body,&unit.view_model,&|callee|
                if callee==name{own}else{&module.units[callee].as_ref().unwrap().contract});
            calls(&mut function.body.code,&mut |call|{call.failure=None;call.memory_use=None;});
        }
        unit.context=None;
        *module.units.get_mut(&name).unwrap()=Some(unit);
    }
    for (program, scopes) in programs.iter_mut() {
        resolve_range_views(module, program, scopes)?;
    }
    for index in 0..module.programs.len() {
        let mut pending = module.programs[index].take().unwrap();
        resolve_range_views(module, &mut pending.program, &pending.scopes)?;
        module.programs[index] = Some(pending);
    }
    Ok(())
}

fn bind_range_body(
    world: &World, body: &mut ValueCode, framed: &BTreeSet<String>, fallible: &BTreeSet<String>,
    offset: Option<u32>, scoped: bool,
) -> Result<()> {
    let mut uses=false;
    calls(&mut body.code,&mut |call|uses|=matches!(&call.target,CallTarget::Defined(name)if framed.contains(name)));
    if uses {
        let offset = offset.unwrap();
        let frame = match body.code.as_slice() {
            [Inst { kind: InstKind::Uniform(low, first), .. },
             Inst { kind: InstKind::Uniform(high, second), .. }, ..]
                if *first == offset && *second == offset + 4 => vec![*low, *high],
            _ => {
                let first = body.registers.len();
                body.registers.extend([Scalar::U32, Scalar::U32]);
                body.code.splice(0..0, [
                    Inst::new(InstKind::Uniform(first, offset)),
                    Inst::new(InstKind::Uniform(first + 1, offset + 4)),
                ]);
                vec![first, first + 1]
            }
        };
        calls(&mut body.code,&mut |call|{
            if matches!(&call.target,CallTarget::Defined(name)if framed.contains(name)) {
                call.arguments.insert(0,CallArgument::Value{origin:Some(Arc::new(ArgumentOrigin::Frame)),dataset_borrow:None,ty:application::pointer_type(),values:frame.clone()});
            }
        });
    }
    if !scoped {
        lower_failure_calls(world, body, None, fallible, None)?;
    }
    calls(&mut body.code,&mut |call|call.failure=None);
    Ok(())
}
fn bind_range_program(
    world: &World, program: &mut Program, framed: &BTreeSet<String>, fallible: &BTreeSet<String>,
    needs_frame: bool, scopes: &BTreeMap<usize, RangeContext>,
) -> Result<()> {
    if needs_frame && program.native_frame.is_none() {
        let offset=ir::align_up(program.uniform_bytes,8);
        program.native_frame=Some(offset);program.uniform_bytes=offset+8;
    }
    for (index, stage) in program.stages.iter_mut().enumerate() {
        let mut body=ValueCode{
            entry_origin:None,generated_origin:None,
            registers:std::mem::take(&mut stage.registers),locals:std::mem::take(&mut stage.locals),
            code:std::mem::take(&mut stage.code),inputs:Vec::new(),outputs:Vec::new(),
        };
        bind_range_body(world, &mut body, framed, fallible, program.native_frame, scopes.contains_key(&index))?;
        stage.registers=body.registers;stage.locals=body.locals;stage.code=body.code;
        let values=match &mut stage.operation{
            Operation::CustomReduce(value)=>Some(value),
            Operation::CustomScan(value)=>Some(&mut value.values),
            Operation::CustomSegmentedReduce(value)=>Some(&mut value.values),
            _=>None,
        };
        if let Some(values)=values {
            for body in values.project.iter_mut().chain([&mut values.identity,&mut values.combine]){
                bind_range_body(world, body, framed, fallible, program.native_frame, false)?;
            }
        }
    }
    Ok(())
}
fn resolve_range_views(
    module: &Module, program: &mut Program, scopes: &BTreeMap<usize, RangeContext>,
) -> Result<()> {
    if scopes.is_empty() { return Ok(()); }
    let closed = module.units.iter().map(|(name, unit)|
        (name.clone(), view_outputs(&unit.as_ref().unwrap().contract)))
        .collect::<BTreeMap<_, _>>();
    let names = scopes.keys().map(|&index| (format!("{}@range{index}", program.name), index))
        .collect::<BTreeMap<_, _>>();
    let functions = names.iter().map(|(name, index)| (name.clone(), &scopes[index].view_model))
        .collect::<BTreeMap<_, _>>();
    let solved = view_contracts::solve(&functions, closed, |_, _| false, None)?;
    for (name, memory) in &solved.memory {
        let stage = &mut program.stages[names[name]];
        address::apply_memory(&mut stage.code, memory, &stage.locals);
    }
    Ok(())
}

fn finalize(
    world:&World,
    mut module: Module,
    entries: BTreeSet<String>,
    mut root: Option<(&mut ValueCode,&view_contracts::Function)>,
    owner: Option<&str>,
    programs: &mut [(&mut Program, &mut BTreeMap<usize, RangeContext>)],
) -> Result<Finalized> {
    use crate::module::codec::Encode;
    lower_module_failures(world,&mut module,programs)?;
    let residency_imports=resolve_residencies(world,&mut module)?;
    let mut resources = std::mem::take(&mut module.resources);
    let mut pending_programs = std::mem::take(&mut module.programs)
        .into_iter().map(Option::unwrap).collect::<Vec<_>>();
    let mut programs = programs.iter_mut().map(|(program, scopes)| (&mut **program, &**scopes, None))
        .chain(pending_programs.iter_mut().map(|pending|
            (&mut pending.program, &pending.scopes, pending.deferred_source.as_deref())))
        .collect::<Vec<_>>();
    let mut reachable = entries.clone();
    for (program, _, _) in programs.iter() { reachable.extend(program_dependencies(program)); }
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
    let fallible=units.iter().filter_map(|(name,unit)|unit.contract.fallible.then(||name.clone())).collect::<BTreeSet<_>>();
    let names=units.keys().cloned().collect::<Vec<_>>();
    for name in names {
        let unit=units.remove(&name).unwrap();
        if unit.contract.frame {frame.insert(name.clone());}
        if let Some(function)=&unit.function {
            let registers=&function.body.inputs[..2];
            if uses_frame(&function.body.code,registers,&imported_without_frame)
                || function.body.outputs.iter().any(|reg|registers.contains(reg)) {
                frame.insert(name.clone());
            }
        }
        units.insert(name,unit);
    }
    if let Some((body,model))=&mut root {
        lower_dataset_borrows(body,model,&|callee|&units[callee].contract);
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
    let mut imports = residency_imports;
    for (name, unit) in &mut units {
        unit.contract.frame = frame.contains(name);
        if unit.c_abi && unit.contract.frame {
            return unit.source.fail(unit.span, "B263", "C入口的实际调用闭包需要Frame；C ABI不能隐式取得执行上下文");
        }
        if unit.function.is_none() || owner.is_some_and(|owner| unit.module != owner) {
            imports.insert(name.clone(), unit.contract.bytes());
        }
    }
    for (program, scopes, source) in &mut programs {
        let names = program_dependencies(program);
        for name in &names {
            if frame.contains(name) && program.target()!=Target::Cpu {
                let unit = &units[name];
                return unit.source.fail(unit.span, "B263", "该范围产物尚未交接真实Frame及同源资源定义");
            }
        }
        let needs_frame=names.iter().any(|name|frame.contains(name));
        bind_range_program(world, program, &frame, &fallible, needs_frame, scopes)?;
        if let Some(source) = source {
            crate::optimize::optimize(program, &world.worker)
                .map_err(|error| crate::compiler::diagnostic(source, error))?;
        }
        program.imports = names.into_iter().filter_map(|name|
            imports.get(&name).map(|contract| (name, contract.clone()))).collect();
    }
    drop(programs);
    resources.programs = pending_programs.into_iter().map(|pending| (pending.target, pending.program)).collect();
    units.retain(|name, _| reachable.contains(name));
    let mut exported_contracts = entries;
    if let Some(owner) = owner {
        // The object keeps every native symbol. Only imported contracts need these bytes.
        let mut pending = exported_contracts.iter().cloned().collect::<Vec<_>>();
        while let Some(name) = pending.pop() {
            for dependency in units[&name].contract.residency_dependencies() {
                let local = units.get(&dependency).is_some_and(|unit| unit.function.is_some() && unit.module == owner);
                if local && exported_contracts.insert(dependency.clone()) {
                    pending.push(dependency);
                }
            }
        }
    }
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
    if let Some((body,_)) = &mut root {
        prune(body, &unframed);
        lower_failure_calls(world,body,None,&fallible,None)?;
    }
    let mut result = Finalized {
        functions: Vec::new(),
        imports,
        exports: BTreeMap::new(),
        resources,
    };
    for (name, mut unit) in units {
        let local = unit.function.is_some() && owner.is_none_or(|owner| unit.module == owner);
        if local {
            let mut function = unit.function.take().unwrap();
            prune(&mut function.body, &unframed);
            function.fallible=fallible.contains(&name);
            if !frame.contains(&name) {
                function.parameters.remove(0);
                function.body.inputs.drain(..2);
                GeneratedOrigin::remove_input_prefix(&mut function.body.generated_origin, 2);
            }
            crate::optimize::value_code(&mut function.body);
            result.functions.push(function);
            if owner.is_some() {
                let contract = if exported_contracts.contains(&name) {
                    unit.contract.bytes()
                } else {
                    Vec::new()
                };
                result.exports.insert(name, contract);
            }
        }
    }
    Ok(result)
}
pub(super) fn finish(world:&World, module: Module, root: &mut ValueCode, model:&view_contracts::Function) -> Result<Finalized> {
    let roots = dependencies(root);
    finalize(world, module, roots, Some((root,model)), None, &mut [])
}
pub(super) fn finish_program(
    world: &World, module: Module, mut program: Program,
    mut scopes: BTreeMap<usize, RangeContext>,
) -> Result<Program> {
    let mut finalized = finalize(world, module, BTreeSet::new(), None, None, &mut [(&mut program, &mut scopes)])?;
    for function in &mut finalized.functions {
        application::lower_sdk_resources(&mut function.body,&finalized.resources);
    }
    program.functions = finalized.functions.into();
    if program.native_frame.is_some() || !finalized.resources.types.is_empty() || !finalized.resources.programs.is_empty(){
        program.resources=Some(Box::new(finalized.resources));
    }
    Ok(program)
}

pub(crate) fn compile_module(
    world: &World,
    entry: &str,
    requests: &[String],
    imported: Option<&crate::module::Imports>,
) -> Result<crate::module::Module> {
    let mut functions = Module::default();
    let mut roots = BTreeSet::new();
    for (key, d) in &world.functions {
        if d.compiled.is_some() || d.function.kind() == FunctionKind::Constant
            || d.function.external().is_some() { continue; }
        crate::generics::validate_declaration(d)?;
        if d.module != entry {
            continue;
        }
        for instance in crate::generics::instances(world, key, d)? {
            if d.execution == crate::execution::Requirement::Domain
                && super::kernel_interface(world, &instance)?
            {
                roots.extend(domain_declaration(world, &instance, &mut functions)?);
            } else if d.function.external().is_none()
                && d.function.kind() == FunctionKind::Value
                && d.function.target() != Target::Gpu
            {
                if d.public {
                    roots.insert(instance.name.clone());
                }
                functions = declaration(world, &instance, functions)?;
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
            .resolve(&name, entry, &[], true).unique()
            .filter(|key| world.functions[key].module == entry && world.functions[key].public)
            .ok_or_else(|| Diagnostic {
                source: source.clone(),
                span: Span::default(),
                code: "B270",
                message: format!("没有本模块公开导出 {name}"),
            })?;
        let d = &world.functions[&key];
        let imports = d.imports.iter().chain(imported.into_iter().flat_map(|input| input.modules.keys()))
            .cloned().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
        let bindings=Arc::new(crate::generics::Bindings::default());
        let arguments=arguments.as_ref().map(|arguments|bindings.arguments(world,arguments,
            (&source,&d.module,&imports),Span{start:0,end:request.len()})).transpose()?;
        let instance = super::generics::specialize(
            world,
            &key,
            &[],
            arguments.as_deref(),
            (&source, &d.module, &imports),
            Span { start: 0, end: request.len() },
        )?;
        if d.execution == crate::execution::Requirement::Domain
            && super::kernel_interface(world, &instance)?
        {
            roots.extend(domain_declaration(world, &instance, &mut functions)?);
            continue;
        }
        if d.function.external().is_some()
            || d.function.kind() != FunctionKind::Value
            || d.function.target() == Target::Gpu
        {
            return d.source.fail(
                d.function.span(),
                "B270",
                "此普通原生模块入口需要CPU适用的普通函数",
            );
        }
        roots.insert(instance.name.clone());
        functions = declaration(world, &instance, functions)?;
    }
    let finalized = finalize(world, functions, roots, None, Some(entry), &mut [])?;
    let mut functions = finalized.functions;
    let resources = application::module_resources(&mut functions, finalized.resources);
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
    input: DomainInput<'_>,
    functions: &mut Module,
    target: Target,
    element_inputs: &[ElementFact],
) -> Result<crate::module::domain::Interface> {
    let name = crate::module::domain::symbol(input.name(), target);
    if !functions.units.contains_key(&name) {
        if let DomainInput::Declaration(instance)=input
            && instance.definition.compiled.is_some() {
            let mut reference = instance.clone();
            reference.name = name.clone();
            *functions = declaration(world, &reference, std::mem::take(functions))?;
        } else {
            build_domain(world, input, functions, target, element_inputs)?;
        }
    }
    functions.units[&name]
        .as_ref()
        .unwrap()
        .contract
        .domain
        .clone()
        .ok_or_else(|| Diagnostic {
            source: input.owner().definition.source.clone(),
            span: input.span(),
            code: "B270",
            message: "模块入口缺少整域合同".into(),
        })
}
impl Builder<'_> {
    pub(super) fn prepare_domain(
        &mut self,
        input: DomainInput<'_>,
        target: Target,
        element_inputs: &[ElementFact],
    ) -> Result<crate::module::domain::Interface> {
        domain_at(self.world, input, &mut self.functions, target, element_inputs)
    }
    pub(super) fn call_domain(
        &mut self,
        name: &str,
        target: Target,
        arguments: Vec<CallArgument>,
    ) {
        let origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::ApplicationDomainCall,
            trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
        });
        let failure=self.failure_continuation(Vec::new());
        self.code.push(Inst::generated(InstKind::Call(Call {
            failure,
            error: None,
            target: CallTarget::Defined(crate::module::domain::symbol(name, target)),
            arguments,
            memory: Vec::new(),
            memory_use: None,
            result: None,
            values: Vec::new(),
        }), origin));
    }
}
pub(crate) fn check_domain(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    domain_declaration(world, instance, &mut Module::default())?;
    Ok(())
}
fn domain_declaration(
    world: &World,
    instance: &crate::generics::Instance,
    functions: &mut Module,
) -> Result<BTreeSet<String>> {
    let targets: &[Target] = match instance.definition.function.target() {
        Target::Cpu => &[Target::Cpu],
        Target::Gpu => &[Target::Gpu],
        Target::Compute => &[Target::Cpu, Target::Gpu],
    };
    let mut roots = BTreeSet::new();
    for &target in targets {
        domain_at(world, DomainInput::Declaration(instance), functions, target, &[])?;
        if instance.definition.public {
            roots.insert(crate::module::domain::symbol(&instance.name, target));
        }
    }
    Ok(roots)
}
fn build_domain(
    world: &World,
    input: DomainInput<'_>,
    functions: &mut Module,
    target: Target,
    element_inputs: &[ElementFact],
) -> Result<()> {
    let d = &input.owner().definition;
    let bound_parameters=input.parameters(world)?;
    let expanded = expand_domain(world, input, functions, element_inputs)?;
    let mut signature = expanded.program;
    let mut scopes = expanded.scopes;
    if !signature.accelerations.is_empty() || target == Target::Gpu && !signature.textures.is_empty() {
        return d.source.fail(
            input.span(),
            "B270",
            "B应用的独立整域入口尚未交接纹理或加速结构；原生宿主算法接口继续使用已有产物",
        );
    }
    let write_regions = signature.write_regions();
    let stages = std::mem::take(&mut signature.stages);
    let mut frame = Ty::vector(Scalar::U32, 2);
    frame.name = "__blink_frame".into();
    let mut registers = vec![Scalar::U32, Scalar::U32];
    let mut parameters = vec![CallParameter::value(frame.clone())];
    let frame = CallArgument::Value { #[cfg(feature = "compiler")] origin: None,
        dataset_borrow:None, ty: frame,
        values: vec![0, 1],
    };
    let mut data = Vec::new();
    let mut texture_values = Vec::new();
    let mut uniform_values = Vec::new();
    let mut uniform_regs = BTreeMap::new();
    let mut ui = 0;
    for param in &bound_parameters {
        let first = registers.len();
        if param.ty.dataset {
            registers.extend([Scalar::U32; 2]);
            parameters.push(CallParameter::value(Builder::domain_dataset_parameter_type()));
            data.push([first, first + 1]);
        } else if param.ty.is_texture_resource() {
            registers.push(Scalar::U64);
            parameters.push(CallParameter::value(Ty::scalar(Scalar::U64)));
            texture_values.push(CallArgument::Value {
                origin: None, dataset_borrow: None, ty: Ty::scalar(Scalar::U64), values: vec![first],
            });
        } else {
            let ty = cooperative::parameter_type(world,&param,(&d.source,&d.module,&d.imports,d.function.kind()),target)?;
            registers.extend(ty.components.iter().map(|(s, _)| *s));
            parameters.push(CallParameter::value(ty.clone()));
            for (i, (scalar, offset)) in ty.components.iter().enumerate() {
                uniform_regs.insert(signature.uniforms[ui].offset + offset, first + i);
                uniform_values.push(CallArgument::Value { #[cfg(feature = "compiler")] origin: None,
                    dataset_borrow:None, ty: Ty::scalar(*scalar),
                    values: vec![first + i],
                });
            }
            ui += 1;
        }
    }
    let inputs: Vec<_> = (0..registers.len()).collect();
    let mut code = Vec::new();
    let entry_context=match input {
        DomainInput::Declaration(instance)=>match &d.syntax_origin {
            crate::frontend::DefinitionSyntax::Source=>Some(GenerationAnchor::Definition{
                origin:d.source_origin().map(Arc::new),instance:instance.name.clone().into()}),
            crate::frontend::DefinitionSyntax::CompiledInterface=>None,
        },
        DomainInput::Parallel{site,..}=>site.clone().map(GenerationAnchor::Syntax),
    };
    let mut interface = crate::module::domain::Interface {
        instance: signature.name.clone(), target,
        buffers: signature.buffers.clone(), textures: signature.textures.clone(), uniforms: signature.uniforms.clone(),
        write_regions,
        length_requirements: signature.length_requirements.clone(),
        element_requirements: signature.element_requirements.clone(),
        prefix_guarantees: signature.prefix_guarantees.clone(),
        outputs: expanded.outputs,
        elements: expanded.elements,
    };
    let mut start = 0;
    let mut required = signature.prefix_requirements.clone();
    let mut element_required = signature.element_requirements.clone();
    let split = !expanded.calls.is_empty();
    let mut part = 0;
    for call in expanded.calls {
        for (&binding, written) in &call.write_regions {
            interface.write_regions.entry(binding).or_default().extend(written.iter().copied());
        }
        emit_domain_part(
            &world.worker,
            &d.source.path,
            entry_context.as_ref(),
            &signature,
            &stages[start..call.stage],
            &mut scopes,
            start,
            &required,
            &call.before,
            &element_required,
            &call.element_before,
            split,
            &mut part,
            functions,
            target,
            &frame,
            &data,
            &uniform_values,
            &texture_values,
            &mut registers,
            &mut code,
        )?;
        domain_at(world, DomainInput::Declaration(&call.instance), functions, target, &[])?;
        let mut arguments = vec![frame.clone()];
        for root in &call.arguments {
            arguments.push(match root {
                Root::Data(i, _) => CallArgument::Value { #[cfg(feature = "compiler")] origin: None,
                    dataset_borrow:None, ty: Builder::domain_dataset_parameter_type(),
                    values: data[*i].to_vec(),
                },
                Root::Texture(i) => texture_values[*i].clone(),
                Root::Uniform(ty, offset) => CallArgument::Value { #[cfg(feature = "compiler")] origin: None,
                    dataset_borrow:None, ty: ty.clone(),
                    values: ty
                        .components
                        .iter()
                        .map(|(_, at)| uniform_regs[&(offset + at)])
                        .collect(),
                },
                Root::Literal(ty, words, evaluation) => {
                    let origin = Arc::new(GeneratedOrigin {
                        reason: GenerationReason::DomainCallLiteral,
                        trigger: Some(GenerationAnchor::DomainCallLiteral {
                            call: call.source.clone(), evaluation: evaluation.clone(),
                        }),
                    });
                    let values = ty
                        .components
                        .iter()
                        .zip(words)
                        .map(|((scalar, _), word)| {
                            let reg = registers.len();
                            registers.push(*scalar);
                            code.push(Inst::generated(InstKind::Constant(reg, *scalar, *word), origin.clone()));
                            reg
                        })
                        .collect();
                    CallArgument::Value { #[cfg(feature = "compiler")] origin: None,
                        dataset_borrow:None, ty: ty.clone(),
                        values,
                    }
                }
                _ => {
                    return d
                        .source
                        .fail(input.span(), "B270", "该Frame入口尚未交接此资源种类");
                }
            });
        }
        code.push(Inst { source_site: call.source, ..Inst::new(InstKind::Call(Call {
            failure: Some(FailureContinuation::Return),
            error: None,
            target: CallTarget::Defined(crate::module::domain::symbol(&call.instance.name, target)),
            arguments,
            memory: Vec::new(),
            memory_use: None,
            result: None,
            values: Vec::new(),
        })) });
        start = call.stage;
        required = call.after;
        element_required = signature.element_requirements.iter().cloned()
            .chain(call.element_after).collect();
    }
    emit_domain_part(
        &world.worker,
        &d.source.path,
        entry_context.as_ref(),
        &signature,
        &stages[start..],
        &mut scopes,
        start,
        &required,
        &signature.prefix_guarantees,
        &element_required,
        &signature.element_guarantees,
        split,
        &mut part,
        functions,
        target,
        &frame,
        &data,
        &uniform_values,
        &texture_values,
        &mut registers,
        &mut code,
    )?;
    interface.outputs.sort();
    interface.outputs.dedup();
    let name = crate::module::domain::symbol(input.name(), target);
    let body = ValueCode {
        entry_origin: None,
        generated_origin: Some(Arc::new(GeneratedOrigin {
            reason: GenerationReason::DomainEntry,
            trigger: Some(GenerationAnchor::DomainEntry {
                context: entry_context.map(Box::new), expansion: signature.generated_origin.clone(),
            }),
        })),
        registers,
        locals: Default::default(),
        inputs: inputs.clone(),
        outputs: Vec::new(),
        code,
    };
    let function = FunctionBody {
        fallible: false,
        name: name.clone(),
        source_origin: input.source_origin(),
        parameters: parameters.clone(),
        result: None,
        body,
    };
    let contract = Contract {
        fallible: false,
        parameters,
        result: None,
        inputs,
        element_inputs:BTreeMap::new(),
        returned: None,
        written: vec![None; bound_parameters.len()],
        requires: input.contracts().into_iter().map(|contract| (contract.kind, contract.predicate)).collect(),
        target: Target::Cpu,
        frame: false,
        domain: Some(interface),
        residency: residency::Contract::default(),
        view_access:Some(BTreeSet::new()),
        initialization:Some(Vec::new()),
    };
    functions.units.insert(
        name,
        Some(Unit {
            source: d.source.clone(),
            span: input.span(),
            module: d.module.clone(),
            input_access: bound_parameters.iter().map(|parameter|parameter.access).collect(),
            context: None,
            function: Some(function),
            contract,
            c_abi: false,
            residency: residency::Inputs::default(),
            view_model: view_contracts::Function::default(),
        }),
    );
    Ok(())
}
fn emit_domain_part(
    worker: &crate::compiler::Worker,
    source: &std::path::Path,
    generation: Option<&GenerationAnchor>,
    signature: &Program,
    stages: &[Stage],
    scopes: &mut BTreeMap<usize, RangeContext>,
    start: usize,
    required: &[PrefixRelation],
    guaranteed: &[PrefixRelation],
    element_required: &[ElementFact],
    element_guaranteed: &[ElementFact],
    split: bool,
    part: &mut usize,
    functions: &mut Module,
    target: Target,
    frame: &CallArgument,
    data: &[[Reg; 2]],
    uniforms: &[CallArgument],
    textures: &[CallArgument],
    registers: &mut Vec<Scalar>,
    code: &mut Vec<Inst>,
) -> Result<()> {
    if stages.is_empty() {
        return Ok(());
    }
    let mut program = signature.clone();
    program.generated_origin = None;
    if split {
        program.name = format!("{}@part{}", signature.name, *part);
    }
    *part += 1;
    program.stages = stages.to_vec();
    program.source_stages = stages.len();
    program.prefix_requirements = required.to_vec();
    program.prefix_guarantees = guaranteed.to_vec();
    program.element_requirements = element_required.to_vec();
    program.element_guarantees = program.element_outputs(element_guaranteed.to_vec());
    let scopes = (start..start + stages.len())
        .filter_map(|index| scopes.remove(&index).map(|scope| (index - start, scope)))
        .collect::<BTreeMap<_, _>>();
    let deferred_source = if !scopes.is_empty() {
        Some(source.to_path_buf())
    } else {
        crate::optimize::optimize(&mut program, worker)
            .map_err(|error| crate::compiler::diagnostic(source, error))?;
        None
    };
    let index = functions.programs.len();
    functions.programs.push(Some(PendingProgram { target, program, scopes, deferred_source }));
    let mut arguments = vec![frame.clone()];
    arguments.extend(data.iter().flat_map(|regs| *regs).map(|reg| CallArgument::Value { #[cfg(feature = "compiler")] origin: None,
        dataset_borrow:None, ty: Ty::scalar(Scalar::U32),
        values: vec![reg],
    }));
    arguments.extend_from_slice(uniforms);
    arguments.extend_from_slice(textures);
    let error=registers.len();registers.push(Scalar::U64);
    code.push(Inst {
        generated_origin: Some(Arc::new(GeneratedOrigin {
            reason: GenerationReason::DomainPartCall,
            trigger: generation.cloned(),
        })),
        ..application::runtime_call(
            &format!("run_{index}"),
            arguments,
            None,
            Vec::new(),
            Some(error),
            Some(FailureContinuation::Return),
        )
    });
    Ok(())
}
