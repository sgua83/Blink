//! 普通 CPU 函数拥有独立调用帧；编译中的符号声明截断递归展开。
use super::*;
use cooperative::Argument;
use returns::Returned;
mod fallibility;
mod owned_cleanup;
mod task_completion;
mod task_callbacks;
mod dependencies;
mod frame_effects;
mod failure_lowering;
mod model;
mod canonical;
pub(super) use canonical::{canonical,element_inputs};
pub(super) use model::{rebind_model,translate_returned,translate_in_place};

type Units = BTreeMap<String, Option<Unit>>;
type InterruptedUnits = std::rc::Rc<std::cell::RefCell<Option<Units>>>;

#[derive(Default)]
pub(super) struct Module {
    pub(super) units: Units,
    pub(super) resources: crate::application::Resources,
    programs: Vec<Option<PendingProgram>>,
    interrupted_units: Option<InterruptedUnits>,
}

impl Drop for Module {
    fn drop(&mut self) {
        if let Some(owner) = self.interrupted_units.take() {
            owner.replace(Some(std::mem::take(&mut self.units)));
        }
    }
}
struct PendingProgram {
    target: Target,
    program: Program,
    scopes: BTreeMap<usize, RangeContext>,
    deferred_source: Option<std::path::PathBuf>,
}

impl Module {
    fn access(&self, name: &str) -> Option<&BTreeSet<view_contracts::Access>> {
        self.units.get(name)?.as_ref()?.contract.view_access.as_ref()
    }

    fn read_access(access: Option<&BTreeSet<view_contracts::Access>>) -> bool {
        access.is_some_and(|access| access.iter().all(|access| match access {
            view_contracts::Access::Resource { kind, .. } => *kind != view::Exclusion::Ownership,
            view_contracts::Access::Memory { access, .. } => !access.writes(),
            view_contracts::Access::Position { position, .. } => position.written.is_none(),
            view_contracts::Access::Dataset { access, .. } => !access.writes(),
            view_contracts::Access::Initialized { .. } | view_contracts::Access::Stored { .. } => true,
        }))
    }

    pub(super) fn read_only(&self, name: &str) -> bool {
        Self::read_access(self.access(name))
    }

    fn effect_access(&self, world: &World, name: &str) -> Result<Option<&BTreeSet<view_contracts::Access>>> {
        let Some(unit) = self.units.get(name).and_then(Option::as_ref) else { return Ok(None); };
        if let Some(pending) = &unit.module_view {
            let outcome = pending.get_or_init(|| view_work(self, Some(name)).unwrap().solve(&world.worker));
            return match outcome {
                Ok(outputs) => Ok(Some(&outputs[0].1.interface.access)),
                Err(error) => Err(error.clone()),
            };
        }
        Ok(unit.contract.view_access.as_ref())
    }

    pub(super) fn effect_read_only(&self, world: &World, name: &str) -> Result<bool> {
        Ok(Self::read_access(self.effect_access(world, name)?))
    }

    pub(super) fn effect_writes_only_references(&self, world: &World, name: &str) -> Result<bool> {
        Ok(Self::reference_access(self.effect_access(world, name)?))
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
    pub(super) fn writes_only_references(&self, name: &str) -> bool {
        Self::reference_access(self.access(name))
    }

    fn reference_access(access: Option<&BTreeSet<view_contracts::Access>>) -> bool {
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
        access.is_some_and(|access| access.iter().all(|access| match access {
                view_contracts::Access::Resource{kind,..}=>matches!(kind,view::Exclusion::Position(_)),
                view_contracts::Access::Memory{access,borrows,..}=>!access.writes()||references(borrows),
                view_contracts::Access::Dataset{access,..}=>!access.writes(),
                view_contracts::Access::Initialized{..}|view_contracts::Access::Stored{..}=>true,
                view_contracts::Access::Position{position,borrows,..}=>position.written.is_none()||references(borrows),
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


mod contract;


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
    module_view: Option<std::cell::OnceCell<Result<ViewOutputs>>>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewDemand {
    Caller,
    ModuleOutput,
}

async fn declaration(
    world: &World,
    instance: &crate::generics::Instance,
    mut functions: Module,
    demand: ViewDemand,
) -> Result<Module> {
    if !functions.declare(&instance.name) {
        if demand == ViewDemand::Caller && functions.units[&instance.name].as_ref()
            .is_some_and(|unit| unit.module_view.is_some())
        {
            resolve_views(world, &mut functions, Some(&instance.name))?;
        }
        return Ok(functions);
    }
    if let Some(definitions) = &instance.definition.compiled {
        return compiled_declaration(instance, definitions.get(&instance.name).map(Vec::as_slice), functions);
    }
    let target=match instance.definition.function.target(){Target::Compute=>Target::Cpu,target=>target};
    let checked=unsafe { world.call(checked_function(world,instance,target,Some(application::Context::default()),functions)).await }?;
    let lexical = LexicalScope::Function(instance.clone());
    let c_abi = instance.definition.function.c_abi();
    match demand {
        ViewDemand::Caller => finish_declaration(world, &lexical, c_abi, checked),
        ViewDemand::ModuleOutput => {
            let (name, mut module) = prepare_declaration(&lexical, c_abi, checked);
            let ready = module.units[&name].as_ref().unwrap().view_model.calls.iter().all(|call| {
                call.function == name || module.access(&call.function).is_some()
            });
            if ready {
                module.units.get_mut(&name).unwrap().as_mut().unwrap().module_view = Some(Default::default());
            } else {
                resolve_views(world, &mut module, Some(&name))?;
            }
            Ok(module)
        },
    }
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
            module_view: None,
        }),
    );
    Ok(functions)
}

#[inline(never)]
fn finish_declaration(
    world: &World, lexical: &LexicalScope, c_abi: bool, checked: Box<CheckedFunction>,
) -> Result<Module> {
    let (name, mut functions) = prepare_declaration(lexical, c_abi, checked);
    resolve_views(world, &mut functions, Some(&name))?;
    Ok(functions)
}

fn prepare_declaration(
    lexical: &LexicalScope, c_abi: bool, mut checked: Box<CheckedFunction>,
) -> (String, Module) {
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
            module_view: None,
        }),
    );
    (name, functions)
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
    pub(super) async fn complete_inputs(&self,builder:&mut Builder<'_>,inputs:&[view::TaskInputCompletion])->Result<()> {
        let (base,_)=self.layout.task_completion_payload();
        unsafe { builder.world.call(builder.complete_task_input_slots(inputs,&self.layout,&self.slots,base,self.span)).await }
    }
    pub(super) async fn complete_if_ready(&self,builder:&mut Builder<'_>)->Result<()> {
        let status=builder.reg(Scalar::U32);
        builder.emit(InstKind::LoadLocal(status,self.slots[0]));
        let test=|builder:&mut Builder<'_>,op,expected| {
            let value=builder.constant(Scalar::U32,Bits::from(expected)).regs[0];
            let reg=builder.reg(Scalar::Bool);
            builder.emit(InstKind::Binary{dst:reg,op:BinaryOp::Compare(op),a:status,b:value});
            Value{ty:Ty::scalar(Scalar::Bool),regs:vec![reg],proof:bounds::Proof::default(),literal:false,uniform:false,constant:None}
        };
        let ready=test(builder,Comparison::NotEqual,Ty::TASK_COMPLETION_PENDING);
        unsafe { builder.world.call(builder.conditional_blocks(ready,async |builder| {
            let task=self.selected_task(builder);
            let success=test(builder,Comparison::Equal,Ty::TASK_COMPLETION_SUCCESS);
            let result=self.value(builder);
            unsafe { builder.world.call(builder.complete_task_owned_places(&task,|_,_|Some(result.clone()),Some(&success),
                async |builder,inputs|unsafe { builder.world.call(self.complete_inputs(builder,inputs)).await },self.span)).await }
        },async |_|Ok(()),self.span)).await }
    }
}
mod task_parameters;
pub(in crate::check) use task_parameters::{task_argument_slots, task_completion_parameters};

pub(super) async fn check(world: &World, instance: &crate::generics::Instance) -> Result<()> {
    let mut functions = unsafe { world.call(declaration(world, instance, Module::default(), ViewDemand::Caller)).await }?;
    unsafe { world.call(lower_module_failures(world,&mut functions,&mut [])).await }?;
    resolve_residencies(world, &mut functions, None)
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
    pub(super) async fn native_declaration(&mut self, instance: &crate::generics::Instance) -> Result<()> {
        if let Some(definitions) = &instance.definition.compiled {
            if self.functions.declare(&instance.name) {
                self.functions = compiled_declaration(instance,
                    definitions.get(&instance.name).map(Vec::as_slice), std::mem::take(&mut self.functions))?;
            }
            return Ok(());
        }
        self.functions = unsafe { self.world.call(declaration(self.world, instance, std::mem::take(&mut self.functions), ViewDemand::Caller)).await }?;
        Ok(())
    }
    pub(super) async fn native_function(
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
        unsafe { self.world.call(self.native_declaration(&instance)).await }?;
        let d=&instance.definition;
        let mut parameters=Vec::new();
        for parameter in instance.parameters(self.world) {
            parameters.push(unsafe { self.world.call(parameter).await }?);
        }
        let result=unsafe { self.world.call(instance.bindings.type_name(self.world,&d.function.result(),(&d.source,&d.module,&d.imports),d.function.span())).await }?;
        let lexical=LexicalScope::Function(instance.clone());
        unsafe { self.world.call(self.native_call(&instance.name,CallSignature{parameters:&parameters,result:&result,contract:(&instance).into(),
            lexical:&lexical,kind:d.function.kind(),execution:d.execution,c_abi:d.function.c_abi()},args,span,generated_origin,syntax,launch)).await }
    }
    async fn drop_task_completion_value(&mut self, ty: &Ty, address: [Reg; 2], offset: u32, span: Span) -> Result<()> {
        let mut model = unsafe { self.world.call(self.template_returned(Value {
            ty: ty.clone(), regs: (0..ty.components.len()).collect(), proof: bounds::Proof::default(),
            uniform: false, literal: false, constant: None,
        }, true)).await }?;
        owned_borrows::each_data(&mut model, &mut Vec::new(), &mut |data, _| data.residency = residency::Residency::cpu());
        owned_borrows::set_storage(&mut model, Place::Reference {
            ty: ty.clone(), address: ReferenceAddress::Contiguous(address), scope: None,
            words: ty.components.iter().map(|&(_, word)| offset + word).collect(), access: Access::InOut,
        });
        unsafe { self.world.call(self.drop_owned_tree_with(model, &view::Borrow::default(), span, Self::drop_task_owned_dataset)).await }
    }

    pub(super) async fn drop_task_completion_element(&mut self, place: &Place, span: Span) -> Result<()> {
        let ty = place.ty();
        let name = crate::generics::key(&format!("{}::@task_element_cleanup", self.scope.module()), [ty.name.as_str()], &[]);
        let pointer = Ty::vector(Scalar::U32, 2);
        if self.functions.declare(&name) {
            let inputs = [("position", pointer.clone()), ("status", Ty::scalar(Scalar::U64))];
            unsafe { self.world.call(self.task_callback(name.clone(), &inputs, true, span, async |builder, registers| {
                let [low, high, status] = *registers else { unreachable!() };
                let slot = builder.local_slots(&Ty::scalar(Scalar::U64)).0[0];
                builder.emit(InstKind::StoreLocal(slot, status));
                builder.pending_error = Some(PendingError::Exit(slot));
                unsafe { builder.world.call(builder.drop_task_completion_value(ty, [low, high], 0, span)).await }?;
                let error = builder.reg(Scalar::U64);
                builder.emit(InstKind::LoadLocal(error, slot));
                builder.emit(InstKind::Raise(error));
                Ok(())
            })).await }?;
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

    async fn native_dataset_shape(&self,param:&Param,definition:outputs::Contract<'_>,count:Option<u32>,span:Span)->Result<()>{
        if param.ty.count.is_some()&&unsafe { self.world.call(self.world.shape(&param.ty.count,&definition.source,param.span,&definition.module,&definition.imports)).await }?!=count{
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
            let borrow=borrows.map_sources_at_call(arguments, |source, _| match &source {
                view::Source::Position(origin)if matches!(&origin.root,address::Root::Parameter(parameter)if project(*parameter))=>view::Borrow::default(),
                _=>source.at_call(arguments),
            });
            let borrow=self.current_address_borrow(&borrow);
            memory.push((borrow.restricted(&path),Some(access),written.map(|proof|proof.at_call(arguments))));
        }
        Some(memory)
    }
    async fn native_call(
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
        let mut dataset_inputs = BTreeMap::new();
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
                unsafe { self.world.call(self.texture_parameter(p,&resource,(d.source,d.module,d.imports),span)).await }?;
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
            let ty = unsafe { self.world.call(cooperative::parameter_type(self.world,&p,(d.source,d.module,d.imports,signature.kind),Target::Cpu)).await }?;
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
                    resource_inputs.insert(ValueIdentity::ResourceParameter(index,0),{let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(0,prefix::domain(&reference));component_proof});
                    let borrow=if matches!(p.access,Access::Read|Access::Atomic) && signature.execution==crate::execution::Requirement::Invocation {
                        let (_,ty)=reference.element_layout();
                        address::Origin{
                            root:address::Root::Derived(source),layout:address::Layout::of(ty),
                            base:address::Layout::of(&reference.data.ty),words:reference.words().into(),positions:Arc::from([]),
                            writable:p.access.writes(),initialized:vec![true;ty.components.len()].into(),
                        }.normalize()
                    }else{source};
                    reference.data.root.proof.set_view_borrow(0,borrow);
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
                    unsafe { self.world.call(self.require_place_access(&place,p.access,span)).await }?;
                }
                let borrow=unsafe { self.world.call(self.position_borrows(&place,span)).await }?;
                storage_inputs.insert(ValueIdentity::ParameterStorage(index),{let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(0,borrow);component_proof});
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
                    for(part,address)in unsafe { self.world.call(self.native_references(&place,p.access,reference_writes,span,signature.c_abi)).await }?{arguments.push(CallArgument::Reference{ #[cfg(feature = "compiler")] origin: Some(argument_origin.clone()),ty:part,access:p.access,address});actual.extend([None,None]);}
                    if p.access==Access::InOut {
                        for data in value.resources(){
                            unsafe { self.world.call(self.require_application_initialized(data,span)).await }?;
                            if launch.is_some() {
                                let resource=view::resource_borrow(data);
                                unsafe { self.world.call(self.require_view_unborrowed(&resource,span)).await }?;
                                if self.application_position_borrowed(data.root.identity()) {
                                    return self.fail(span,"B227","独占任务交接前须结束dataset元素的位置借用");
                                }
                                self.application_cpu(data,span)?;
                            }
                        }
                        unsafe { self.world.call(self.load_owned_value(&mut value,span)).await }?;
                        value.value.proof=self.current_address_proof(&value.value.ty,&value.value.proof);
                        memory_borrow.union(&self.address_contents(&value.value.ty,&value.value.proof));
                        let proof=bounds::Proof::capture(&value.value);
                        for component in 0..ty.components.len(){contents.insert(ValueIdentity::ParameterContent(index,component),proof.project(&[component],ty.components.len()));}
                        residency::owned_argument(&mut locations,index,&mut value);
                        for(resource,path)in view::resource_paths(&ty).into_iter().enumerate(){let borrow=match view::owned_data_at(&value,&path){Some(data)=>self.data_value_borrow(data),None=>view::Borrow::from_sources(BTreeSet::from([view::Source::Inactive]))};resource_inputs.insert(ValueIdentity::ResourceParameter(index,resource),{let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(0,borrow);component_proof});}
                    } else if launch.is_some() {
                        retired_outputs.push((place.clone(),Returned::OwnedValue(value.clone())));
                    } else {
                        unsafe { self.world.call(self.drop_returned(Returned::OwnedValue(value.clone()),&view::Borrow::default(),span)).await }?;
                    }
                    env.insert(p.name.to_owned(),Binding::OwnedValue(value));written.push((index,place));
                }
                Argument::Reference(mut owner) if matches!(p.access, Access::Read | Access::Write | Access::InOut) => {
                    let place=owner.place.clone();
                    self.require_same(&ty, place.ty(), span)?;
                    if p.access.reads(){owner.proof=self.current_address_proof(&ty,&owner.proof);memory_borrow.union(&self.address_contents(&ty,&owner.proof));}
                    for (part, address) in unsafe { self.world.call(self.native_references(&place, p.access, reference_writes, span, signature.c_abi)).await }? {
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
                    for (part, address) in unsafe { self.world.call(self.native_references(&place, Access::Atomic, reference_writes, span, signature.c_abi)).await }? {
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
                    unsafe { self.world.call(self.native_dataset_shape(p,d,self.program.buffers[binding].count,span)).await }?;
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
                    let borrow=unsafe { self.world.call(self.position_origin(&place,span)).await }?.normalize();
                    memory_borrow.union(&borrow);
                    let pointer=if signature.execution==crate::execution::Requirement::Invocation {
                        let address=self.reg(Scalar::U64);
                        self.code.push(Inst::generated(InstKind::PositionAddress{dst:address,address:CallReference{
                            storage:CallStorage::Indexed(IndexedSource::Data{binding,index:zero.regs[0]}),offset:None,word:0,
                        }},origin.clone()));
                        Value{ty:Ty::scalar(Scalar::U64),regs:vec![address],proof:{let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(0,borrow);component_proof},literal:false,uniform:true,constant:None}
                    }else{
                        let mut handle=self.constant_with_origin(Scalar::U32,binding as u64,Some(origin.clone()));
                        handle.proof.set_view_borrow(0,borrow);handle
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
                    unsafe { self.world.call(self.native_dataset_shape(p,d,reference.count,span)).await }?;
                    if p.access==Access::Write||p.access.writes()&&outputs::declared(d,index){
                        unsafe { self.world.call(self.prepare_data_output(&mut reference.data,span)).await }?;
                        if launch.is_none() {
                            if p.access == Access::Write && reference.element_layout().1.owns_resources() {
                                unsafe { self.world.call(self.retire_application_elements(&reference, span)).await }?;
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
                    value.proof.set_view_borrow(0,application::dataset_origin(&reference.data,field,ty,p.access.reads()).normalize());
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
                        value.proof.set_view_borrow(0,borrow);
                    }
                    if matches!(p.access,Access::Write|Access::InOut) {
                        dataset_inputs.insert(index,actual.len());
                    }
                    append_actual(&mut actual,&value);
                    arguments.push(CallArgument::Value{ #[cfg(feature = "compiler")] origin: Some(argument_origin.clone()),dataset_borrow,ty:value.ty,values:value.regs});
                    env.insert(p.name.to_owned(),Binding::DataReference(reference));
                }
                Argument::ApplicationData(mut data) if p.ty.dataset&&p.access==Access::Value=>{
                    residency::argument(&mut locations,index,&mut data,&[]);
                    self.require_same(&ty,&data.ty,span)?;
                    unsafe { self.world.call(self.require_application_initialized(&data,span)).await }?;
                    unsafe { self.world.call(self.native_dataset_shape(p,d,data.count,span)).await }?;
                    resource_inputs.insert(ValueIdentity::ResourceParameter(index,0),{let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(0,view::resource_borrow(&data));component_proof});
                    let mut value=self.returned_storage(&Returned::Dataset(data.clone()));
                    value.proof.set_view_borrow(0,application::dataset_origin(&data,0,&data.ty,true).normalize());
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
                        if launch.is_some() && ty.contains_dataset() {
                            for data in value.resources() {
                                self.application_cpu(data,span)?;
                            }
                            borrowed_arguments.insert(arguments.len(), None);
                        }
                    }else if !value.all_available(){return self.fail(span,"B227","按值参数需要实际拥有全部活动资源");}
                    for data in value.resources(){unsafe { self.world.call(self.require_application_initialized(data,span)).await }?;}
                    value.resources_mut(&mut |data|data.initialization=None);
                    residency::owned_argument(&mut locations, index, &mut value);
                    for(resource,path)in view::resource_paths(&value.value.ty).into_iter().enumerate(){let borrow=match view::owned_data_at(&value,&path){Some(data)=>self.data_value_borrow(data),None=>view::Borrow::from_sources(BTreeSet::from([view::Source::Inactive]))};resource_inputs.insert(ValueIdentity::ResourceParameter(index,resource),{let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(0,borrow);component_proof});}
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
        let element_paths=unsafe { self.world.call(contracts::element_input_paths(self,d,&mut env,declared.as_ref())).await }?;
        let (element_inputs,element_memory)=application::element_input_bindings(self,d,&env,&element_paths,declared.as_ref());
        memory_borrow.union(&element_memory);
        let requirements = unsafe { self.world.call(contracts::resolve(self,d,signature.lexical,&env,&resource_inputs)).await }?;
        unsafe { self.world.call(contracts::prove(self, &requirements, span)).await }?;
        let output_inputs=unsafe { self.world.call(outputs::inputs(self,d,&env)).await }?;
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
                let (decoded, outputs) = residency_contracts(self.world, &self.functions, None)?;
                let known = ResidencyContracts { units: &self.functions.units, decoded: &decoded };
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
            Some(unsafe { self.world.call(owned_values::storage_type(self.world,result_name,(d.source,d.module,d.imports),d.span())).await }?)
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
                        let element = unsafe { self.world.call(self.expected_storage(element, span)).await }?;
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
            let mut guards=Vec::new();
            for (parameter,place) in written.iter().filter(|(_,place)|place.ty().owns_resources()) {
                let model=contract.and_then(|contract|contract.written.get(*parameter)?.clone());
                let target=unsafe { self.world.call(self.position_origin(place,span)).await }?.normalize();
                let guard=unsafe { self.world.call(outputs::TaskGuard::new(self,d,*parameter,place.ty(),target,
                    &output_inputs,&result,span,model,&view_arguments,&returned)).await }?;
                guards.push((*parameter,guard));
            }
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
                let guard=unsafe { self.world.call(outputs::TaskGuard::new(self,d,parameter,ty,origin.normalize(),&output_inputs,
                    &result,span,model,&view_arguments,&returned)).await }?;
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
            let memory=unsafe { self.world.call(self.call_address_memory([(readable,Some(Access::Read))],span)).await }?;
            let call=self.view_model.call(name,memory,view_arguments,None,
                address::initialization_effects(&self.initialized,&bounds::Clause::default()),
                self.path.clone(),self.scope.source().clone(),span,true,live_task_places);
            let started=unsafe { self.world.call(self.emit_task_start(name,packet,arguments,captured,result,outputs,initialized,guards,conditional_positions,borrowed_arguments,dataset_writes,retired_outputs,memory,call,span)).await }?;
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
                    unsafe { self.world.call(self.write_address_values(&borrow,&memory)).await }?;None
                };
                (borrow,snapshots)
            },
            None=>(memory_borrow,Some(self.capture_address_values(memory_use,address::Capture::Write).into())),
        }};
        self.check_view_requirement(view::Requirement::Memory{target:address::MemoryTarget::Call(memory_use),borrow,access:Some(Access::InOut),written:None,snapshots:snapshots.clone(),span,address_fields:Arc::from([])})?;
        let source_site = if generated_origin.is_none() { self.source_site.clone() } else { None };
        let has_task_abi = self.application_entry() && !signature.c_abi;
        let task_results = if has_task_abi {
            result.as_ref().map(Ty::task_components).unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut completion_slots = BTreeMap::new();
        let mut completion_inputs = BTreeMap::new();
        if has_task_abi {
            let (_, task_parameters) = task_completion_parameters(
                arguments.iter().map(task_argument_slots), if implicit { 0 } else { 2 },
            );
            let first_completion=actual.len();
            let mut completion_sources=Vec::<(view::Borrow,Option<Reg>)>::new();
            for completion in task_parameters {
                let output=actual.len();
                let parent_position = completion.parent();
                let (source,forwarded)=match parent_position {
                    Some((parent,component))=>{
                        let (parent_source,identity)=&completion_sources[parent];
                        let source=parent_source.task_projection(view::TaskProjection::Returned(component,None));
                        let forwarded=identity.and_then(|parent|
                            self.task_completion(TaskCompletionSource::Returned(parent,component),&completion.result))
                            .map(|address|(address,Some(address[0])));
                        (source,forwarded)
                    },
                    None=>{
                        let source=actual[completion.input()].as_ref().map_or_else(view::Borrow::default,|proof|proof.view_borrow(0));
                        let register=completion.argument_register(&arguments);
                        let forwarded=self.task_completion_destination(register,&completion.result,&source);
                        (source,forwarded)
                    },
                };
                let result = completion.result;
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
                    let parent=parent_position.and_then(|(parent,component)|completion_slots.get(&(first_completion+2*parent))
                        .map(|output:&Arc<TaskCompletionOutput>|(output.clone(),component)));
                    completion_slots.insert(output,Arc::new(TaskCompletionOutput{result,layout:layout.clone(),slots,task:source,parent,span}));
                    CallStorage::Local{base:range.start,len:range.len()}
                };
                arguments.push(CallArgument::Reference{origin:None,ty:layout,access:Access::InOut,
                    address:CallReference{storage,offset:None,word:0}});
                actual.extend([None,None]);
            }
        }
        let task_return_slots=if !task_results.is_empty() {
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
        let mut ordinary_outputs=written.iter().peekable();
        for index in 0..bound_parameters.len() {
            if let Some(&input)=dataset_inputs.get(&index) {
                let ids=writes.get(index).and_then(Option::as_ref).map(Returned::registers)
                    .unwrap_or_else(||(semantic..semantic+2).collect());
                for(source,part)in ids.into_iter().zip(0..2) {
                    bindings.insert(ValueIdentity::Register(source),view_arguments[&ValueIdentity::Register(input+part)].clone());
                }
                semantic+=2;
                continue;
            }
            let Some((_,place))=ordinary_outputs.peek().copied().filter(|output|output.0==index) else{continue;};
            let regs=place.ty().components.iter().map(|(scalar,_)|self.reg(*scalar)).collect::<Vec<_>>();
            let contract=writes.get(index).and_then(Option::as_ref);
            let ids=contract.map(Returned::registers).unwrap_or_else(||(semantic..semantic+regs.len()).collect());
            for(source,target)in ids.into_iter().zip(&regs){bindings.insert(ValueIdentity::Register(source),bounds::Proof::default().with_identity(ValueIdentity::Register(*target)));}
            if let Some(model)=contract {let proof=bounds::Proof::capture(&self.returned_storage(model));for(component,reg)in regs.iter().enumerate(){if let Some(id)=proof.known_identity(component){bindings.entry(id).or_insert_with(||bounds::Proof::default().with_identity(ValueIdentity::Register(*reg)));}}}
            semantic+=regs.len();written_registers.insert(index,regs);
            ordinary_outputs.next();
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
            unsafe { self.world.call(self.complete_task_owned_places(&task,|builder,guard|guard.result_at_call(builder,&returned,&bindings),None,
                async |builder,inputs|match completion {
                    Some(output)=>unsafe { builder.world.call(output.complete_inputs(builder,inputs)).await },None=>Ok(()),
                },span)).await }?;
        }
        for (&task,output) in &completion_slots {
            if !needed.contains(&task) && output.has_owned_outputs() {
                unsafe { self.world.call(output.complete_if_ready(self)).await }?;
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
                    let guards = unsafe { self.world.call(outputs::guards(self, d, index, ty, &output_inputs, output_value.as_ref())).await }?;
                    if conditional && ty.owns_resources() {
                        let mut model = unsafe { self.world.call(self.element_model(ty, &updated.elements, (&updated.residency, &[]), span)).await }?;
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
                unsafe { self.world.call(self.apply_application_dataset_model(reference,updated.elements)).await }?;
                if !updated.prefixes.is_empty() {
                    let offset = reference.element_layout().0;
                    let facts = updated.prefixes.iter().map(|(&word, fact)| (offset + word, fact));
                    unsafe { self.world.call(self.apply_application_prefixes(reference, facts)).await }?;
                }
            }
        }
        for (index, place) in written {
            let guards=unsafe { self.world.call(outputs::guards(self,d,index,place.ty(),&output_inputs,output_value.as_ref())).await }?;
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
                    proof.set_view_borrow(component,self.view_model.result(call,view_contracts::Output::Written(index,component)));
                }
            }}
            let mut owned_output=None;
            if place.ty().owns_resources(){
                let regs=written_registers[&index].clone();
                let mut result=unsafe { self.world.call(self.template_returned(Value{ty:place.ty().clone(),regs,proof:proof.clone(),uniform:false,literal:false,constant:None},true)).await }?;
                if let Some(Some(model))=writes.get(index){translate_returned(&mut result,model,&bindings,false);residency::returned(&mut result,&locations);}
                residency::output(&mut result,name,residency::OutputKind::Written(index),&locations,&view_arguments);
                if !views_closed && let Some(call)=view_call{view_contracts::set_resource_outputs(&mut result,|resource|self.view_model.result(call,view_contracts::Output::WrittenResource(index,resource)));}
                if outputs::declared(d,index){outputs::apply_ownership(&mut result,place.ty(),0,&guards);}
                let Returned::OwnedValue(mut value)=result else{unreachable!()};proof=bounds::Proof::capture(&value.value);value.value.proof=proof.clone();owned_output=Some(Returned::OwnedValue(value));
            }
            unsafe { self.world.call(self.assign_owned_proof(&place, proof, false)).await }?;
            if let Some(value)=owned_output{unsafe { self.world.call(self.assign_owned_place(&place,value)).await }?;}
            self.initialize_output(&place,&guards);
        }
        for (index,place) in written_stage {
            let guards=unsafe { self.world.call(outputs::guards(self,d,index,place.ty(),&output_inputs,output_value.as_ref())).await }?;
            self.initialize_output(&place,&guards);
        }
        if syntax.is_some(){self.view_uses.after(span);}
        let Some(ty) = result else {
            for name in held { unsafe { self.world.call(self.finish_scoped_argument(&name, span)).await }?; }
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
        let mut returned = unsafe { self.world.call(self.template_returned(value, true)).await }?;
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
                        value.proof.set_view_borrow(component,selected);
                    }else if !projected.is_empty(){
                        let actual=borrow.map_sources_at_call(&bindings, |source, _| at_call(&source));
                        value.proof.set_view_borrow(component,actual);
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
                if actual.content_inputs()||actual.empty()||actual.pending()||actual.unresolved(){value.proof.set_view_borrow(component,resolved);}
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
            unsafe { self.world.call(self.finish_scoped_argument(&name, span)).await }?;
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
fn view_outputs(contract:&Contract)->view_contracts::Interface {
    let mut outputs=BTreeMap::new();
    if let Some(value)=contract.returned.as_ref().and_then(Returned::view_value) {
        for component in view_contracts::components(&value.ty){outputs.insert(view_contracts::Output::Returned(component),value.proof.view_borrow(component));}
    }
    // 位置参数的实际类型由原CallParameter携带；值输出合同保留原形参编号。
    for (parameter,model)in contract.written.iter().enumerate() {
        if let Some(value)=model.as_ref().and_then(Returned::view_value){for component in view_contracts::components(&value.ty){outputs.insert(view_contracts::Output::Written(parameter,component),value.proof.view_borrow(component));}}
    }
    if let Some(value) = &contract.returned {
        view_contracts::resource_outputs(value, |resource, borrow| {
            outputs.insert(view_contracts::Output::ReturnedResource(resource), borrow);
        });
    }
    for (parameter, value) in contract.written.iter().enumerate() {
        if let Some(value) = value {
            view_contracts::resource_outputs(value, |resource, borrow| {
                outputs.insert(view_contracts::Output::WrittenResource(parameter, resource), borrow);
            });
        }
    }
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
                    _=>!inst.local_writes().any(|written| written == *slot),
                });
                stored&&same
            },
            Some(InstKind::Cast{dst,src})=>body.registers[*dst]==body.registers[*src]&&same(module,name,*src,input,active),
            Some(InstKind::Select{a,b,..})=>same(module,name,*a,input,active)&&same(module,name,*b,input,active),
            Some(InstKind::Call(call))=>{
                if let CallTarget::Defined(callee)=&call.target {
                    let context=call.memory_use.and_then(|memory| unit.view_model.call_at_memory(memory, false)).map(|(_, entry)| entry);
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

type ViewOutputs = Vec<(String, view_contracts::SolvedFunction)>;

struct ViewSource<'a> {
    source: &'a Source,
    span: Span,
    input_access: &'a [Access],
}

#[derive(Default)]
struct ViewWork<'a> {
    functions: BTreeMap<String, &'a view_contracts::Function>,
    sources: BTreeMap<String, ViewSource<'a>>,
    closed: BTreeMap<String, view_contracts::Interface>,
}

impl ViewWork<'_> {
    fn solve(self, worker: &crate::compiler::Worker) -> Result<ViewOutputs> {
        let Self { functions, sources, closed } = self;
        let owns_input = |function: &str, id| match id {
            ValueIdentity::ResourceParameter(parameter, _) => sources[function].input_access[parameter] == Access::Value,
            _ => false,
        };
        let outputs = if let Some(observe) = &worker.compiler().borrow_solve_observer {
            use crate::compiler::{BorrowSolveEvent, BorrowSolveFunction, BorrowSolvePhase};
            let participating: Vec<_> = sources.iter().map(|(name, unit)| {
                BorrowSolveFunction { name, source: unit.source, span: unit.span }
            }).collect();
            observe(BorrowSolveEvent { phase: BorrowSolvePhase::Started, functions: &participating });
            let storage = |function: &str, storage| observe(BorrowSolveEvent {
                phase: BorrowSolvePhase::Storage { function, storage },
                functions: &participating,
            });
            let result = view_contracts::solve_views(worker, &functions, closed, owns_input, Some(&storage));
            observe(BorrowSolveEvent {
                phase: BorrowSolvePhase::Finished { succeeded: result.is_ok() },
                functions: &participating,
            });
            result
        } else {
            view_contracts::solve_views(worker, &functions, closed, owns_input, None)
        }?;
        for (name, solved) in &outputs {
            if solved.interface.outputs.values().any(view::Borrow::has_local) {
                let unit = &sources[name];
                return unit.source.fail(unit.span,"B227","地址不能返回或输出当前函数的局部存储");
            }
        }
        Ok(outputs)
    }
}

fn view_work<'a>(module: &'a Module, root: Option<&str>) -> Option<ViewWork<'a>> {
    let mut work = ViewWork::default();
    let mut pending = root.map(|name| vec![name.to_owned()]).unwrap_or_else(|| {
        module.units.iter().filter_map(|(name, unit)| {
            unit.as_ref().unwrap().contract.view_access.is_none().then(|| name.clone())
        }).collect()
    });
    while let Some(name) = pending.pop() {
        if work.functions.contains_key(&name) || work.closed.contains_key(&name) { continue; }
        let unit = module.units[&name].as_ref()?;
        if unit.contract.view_access.is_some() {
            work.closed.insert(name, view_outputs(&unit.contract));
        } else {
            pending.extend(unit.view_model.calls.iter().map(|call| call.function.clone()));
            work.sources.insert(name.clone(), ViewSource {
                source: &unit.source, span: unit.span, input_access: &unit.input_access,
            });
            work.functions.insert(name, &unit.view_model);
        }
    }
    Some(work)
}

fn resolve_views(world: &World, module: &mut Module, root: Option<&str>) -> Result<()> {
    if let Some(name) = root
        && let Some(outcome) = module.units.get_mut(name).and_then(Option::as_mut)
            .and_then(|unit| unit.module_view.as_mut()).and_then(std::cell::OnceCell::take)
    {
        publish_views(module, outcome?);
        return Ok(());
    }
    if let Some(work) = view_work(module, root) {
        let solved = work.solve(&world.worker)?;
        publish_views(module, solved);
    }
    Ok(())
}

fn publish_ready_views(world: &World, module: &mut Module, ready: &mut Vec<&str>) -> Result<()> {
    let jobs: Vec<_> = ready.drain(..).map(|name| view_work(module, Some(name)).unwrap()).collect();
    let results = crate::parallel::try_map(&world.worker, jobs, Clone::clone, |worker, work| work.solve(worker))
        .map_err(|error| match error {
            crate::parallel::Error::Work(diagnostic) => diagnostic,
            crate::parallel::Error::Resource(error) => crate::compiler::diagnostic(std::path::Path::new("<borrow-solver>"), error),
        })?;
    for outputs in results {
        publish_views(module, outputs);
    }
    Ok(())
}

fn resolve_module_views(world: &World, module: &mut Module, roots: &[String]) -> Result<()> {
    let mut ready = Vec::new();
    for name in roots {
        let Some(pending) = module.units.get(name).and_then(Option::as_ref)
            .and_then(|unit| unit.module_view.as_ref()) else { continue; };
        if pending.get().is_some() {
            publish_ready_views(world, module, &mut ready)?;
            resolve_views(world, module, Some(name))?;
        } else if !ready.contains(&name.as_str()) {
            ready.push(name.as_str());
        }
    }
    publish_ready_views(world, module, &mut ready)
}

fn publish_views(module: &mut Module, outputs: ViewOutputs) {
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

    for (name, view_contracts::SolvedFunction { interface, memory, values }) in outputs {
        let aliases=returned_input_aliases(module,&name);
        let unit=module.units.get_mut(&name).unwrap().as_mut().unwrap();
        unit.module_view = None;
        let contract=&mut unit.contract;
        contract.view_access=Some(interface.access);
        contract.initialization=Some(interface.initialization);
        if let Some(values)=values {
            for model in contract.returned.iter_mut().chain(contract.written.iter_mut().flatten()){returned_values(model,&values);}
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
        for (output, borrow) in interface.outputs {
            match output {
                view_contracts::Output::Returned(component) => {
                    if let Some(value) = contract.returned.as_mut().and_then(Returned::view_value_mut) {
                        value.proof.set_view_borrow(component, borrow);
                    }
                }
                view_contracts::Output::Written(parameter, component) => {
                    if let Some(value) = contract.written[parameter].as_mut().and_then(Returned::view_value_mut) {
                        value.proof.set_view_borrow(component, borrow);
                    }
                }
                view_contracts::Output::ReturnedResource(resource) => {
                    if let Some(value) = contract.returned.as_mut()
                        .and_then(|value| view_contracts::resource_output_mut(value, &resource))
                    {
                        value.root.proof.set_view_borrow(0, borrow);
                    }
                }
                view_contracts::Output::WrittenResource(parameter, resource) => {
                    if let Some(value) = contract.written[parameter].as_mut()
                        .and_then(|value| view_contracts::resource_output_mut(value, &resource))
                    {
                        value.root.proof.set_view_borrow(0, borrow);
                    }
                }
            }
        }
        if let Some(function)=&mut unit.function {
            address::apply_memory(&mut function.body.code,&memory,&function.body.locals);
        }
    }
}
// Borrow current contracts; own decoded dependencies from compiled definitions,
// including private callees without reconstructing their source declarations.
pub(super) struct ResidencyContracts<'a> {
    units: &'a Units,
    decoded: &'a BTreeMap<String, residency::Contract>,
}

impl<'a> ResidencyContracts<'a> {
    pub(super) fn get(&self, name: &str) -> &'a residency::Contract {
        self.units.get(name).and_then(Option::as_ref)
            .map_or_else(|| &self.decoded[name], |unit| &unit.contract.residency)
    }
}

fn residency_contracts(
    world: &World,
    module: &Module,
    mut imports: Option<&mut BTreeMap<String, Vec<u8>>>,
) -> Result<(BTreeMap<String, residency::Contract>, residency::Outputs)> {
    use crate::module::codec::Decode;
    use std::borrow::Cow;
    let mut decoded = BTreeMap::<String, Contract>::new();
    let mut pending = module.units.iter().filter_map(|(name, unit)| {
        unit.as_ref().map(|_| {
            let owner = world.declarations.compiled_function(name).map(|(_, declaration, _)| declaration);
            (Cow::Borrowed(name.as_str()), owner)
        })
    }).collect::<Vec<_>>();
    while let Some((name, owner)) = pending.pop() {
        let contract = module.units.get(name.as_ref()).and_then(Option::as_ref)
            .map_or_else(|| &decoded[name.as_ref()], |unit| &unit.contract);
        for callee in contract.residency_dependencies() {
            if module.units.get(&callee).is_some_and(Option::is_some) || decoded.contains_key(&callee) {
                continue;
            }
            // Private dependencies are read from the compiled call's own contract table.
            let supplied = match owner {
                Some(definition) => definition.compiled.as_ref()
                    .and_then(|contracts| contracts.get(&callee))
                    .map(|bytes| (definition, bytes.as_slice())),
                None => world.declarations.compiled_function(&callee)
                    .map(|(_, definition, bytes)| (definition, bytes)),
            };
            let (definition, bytes) = supplied
                .ok_or_else(||Diagnostic{source:module.units.values().find_map(|unit|unit.as_ref()).unwrap().source.clone(),span:Span::default(),code:"B270",message:format!("已编译模块没有位置合同定义 {callee}")})?;
            let contract=Contract::from_bytes(bytes).map_err(|error|Diagnostic{source:definition.source.clone(),span:definition.function.span(),code:"B270",message:format!("模块函数接口错误：{error}")})?;
            if let Some(inputs) = &world.build_declarations {
                inputs.borrow_mut().insert(crate::build::Declaration::CompiledContract {
                    name: callee.clone(),
                    provider: definition.module.clone(),
                });
            }
            if let Some(imports) = imports.as_deref_mut() {
                imports.insert(callee.clone(), bytes.to_vec());
            }
            decoded.insert(callee.clone(), contract);
            pending.push((Cow::Owned(callee), Some(definition)));
        }
    }
    let local = module.units.iter().filter_map(|(name, unit)| {
        unit.as_ref().map(|unit| (name, &unit.contract))
    });
    let outputs = residency::output_sources(local.chain(&decoded).map(|(name, contract)| {
        (name.as_str(), contract.returned.as_ref(), contract.written.as_slice())
    }));
    Ok((decoded.into_iter().map(|(name, contract)| (name, contract.residency)).collect(), outputs))
}
// 每个原函数持有自身Uses／Calls；调用者消费实际实参，不穷举递归后代。
fn resolve_residencies(
    world: &World,
    module: &mut Module,
    imports: Option<&mut BTreeMap<String, Vec<u8>>>,
) -> Result<()> {
    // Definition inputs retain their requirements in the existing call contract.
    fn unbound(value:&residency::Residency)->bool{match value{
        residency::Residency::Absent=>true,
        residency::Residency::Origins(origins)=>!origins.is_empty()&&origins.iter().all(|origin|matches!(origin,residency::Origin::Input(_))),
        residency::Residency::Selected(_,a,b)=>unbound(a)&&unbound(b),
    }}
    resolve_views(world,module,None)?;
    let (decoded, outputs) = residency_contracts(world, module, imports)?;
    for unit in module.units.values_mut() {
        let unit = unit.as_mut().unwrap();
        if unit.function.is_none() { continue; }
        let parameters=residency::input_parameters(&unit.view_model.parameters,unit.contract.inputs.len());
        unit.contract.residency=residency::infer(&unit.residency,&parameters,&outputs,&unit.source,unit.span)?;
    }
    let known = ResidencyContracts { units: &module.units, decoded: &decoded };
    for (name, unit) in &module.units {
        let unit = unit.as_ref().unwrap();
        if unit.function.is_none() { continue; }
        let parameters=residency::input_parameters(&unit.view_model.parameters,unit.contract.inputs.len());
        let frame_width=application::pointer_type().components.len();
        let pending=|id:&ValueIdentity|parameters.get(id).and_then(|proof|proof.known_identity(0)).is_some_and(|id|match id{
            ValueIdentity::Register(index)=>index>=frame_width,
            _=>true,
        });
        residency::check_calls(unit.residency.calls.iter().filter(|call|
            !call.arguments.values().all(unbound)
            && !call.bindings.values().any(|proof|proof.any_scalar_identity(&pending))
            && !bounds::Clause::any_scalar_identity([&call.path],&pending)
        ),&known,&outputs)?;
        residency::check_loops(name,&unit.residency,&unit.contract.residency,&parameters,&outputs,&known,unit.input_access.len())?;
    }
    for unit in module.units.values_mut() {
        let unit = unit.as_mut().unwrap();
        if unit.function.is_none() { continue; }
        for written in &mut unit.contract.written{if let Some(value)=written{if !residency::resolve_returned(value,&outputs){*written=None;}}}
        if let Some(returned)=&mut unit.contract.returned{if !residency::resolve_returned(returned,&outputs){unit.contract.returned=None;}}
    }
    Ok(())
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
pub(super) struct Finalized {
    pub functions: Vec<FunctionBody>,
    pub imports: BTreeMap<String, Vec<u8>>,
    pub exports: BTreeMap<String, Vec<u8>>,
    pub resources: crate::application::Resources,
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
fn failure_effects(module: &mut Module) {
    fallibility::run(module);
}
struct FailureContext<'a> {
    program: &'a Program,
    output_scopes: &'a mut usize,
    view_model: &'a mut view_contracts::Function,
    residency: &'a mut residency::Inputs,
    task_completions: Option<&'a BTreeMap<TaskCompletionSource, Vec<(Ty, [Reg; 2])>>>,
    range: bool,
}

async fn failure_scope(
    world:&World,scope:FailureScope,error:Reg,registers:&mut Vec<Scalar>,
    locals:&mut BTreeMap<usize,Ty>,context:&mut FailureContext<'_>,module:&mut Module,existing:Option<usize>,
)->Result<Vec<Inst>> {
    let mut b=Builder::from_checked(world,&scope.scope,context.program,scope.domain,
        scope.source_definition,scope.source_context,scope.binding_uses);
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
        unsafe { world.call(b.move_owned_place(&place)).await }?;
    }
    for output in scope.task_outputs {unsafe { world.call(output.complete_if_ready(&mut b)).await }?;}
    unsafe { world.call(b.release_application_data(None,view::Borrow::default())).await }?;
    // A failed call keeps the caller's original location facts for surviving loans.
    let loans = b.env.values().filter_map(|binding| {
        let Binding::ApplicationData(data) = binding else { return None; };
        let application::DataLayout::Parameter { parameter, .. } = data.layout else { return None; };
        data.returns_loan_on_error()
            .then(|| (parameter, Returned::Dataset(data.clone())))
    }).collect::<Vec<_>>();
    let path = b.residency_path();
    for (parameter, mut loan) in loans {
        unsafe { world.call(b.normalize_element_facts(&mut loan, b.scope.span())).await }?;
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
async fn lower_failure_calls(
    world:&World,body:&mut ValueCode,context:Option<FailureContext<'_>>,
    known:&BTreeSet<String>,module:Option<&mut Module>,
)->Result<(bool,bool)> {
    unsafe { world.call(failure_lowering::run(world,body,context,known,module)).await }
}
async fn lower_range_failures(
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
        let result = unsafe { world.call(lower_failure_calls(world, &mut body, Some(owner), known, Some(module))).await }?;
        changed |= result.0;
        scoped |= result.1;
        stage.registers = body.registers;
        stage.locals = body.locals;
        stage.code = body.code;
    }
    program.stages = stages;
    Ok((changed, scoped))
}

async fn lower_module_failures(
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
                let result=unsafe { world.call(lower_failure_calls(world,&mut function.body,owner,&known,Some(&mut *module))).await }?;
                changed|=result.0;scoped|=result.1;unit.function=Some(function);
            }
            *module.units.get_mut(&name).unwrap()=Some(unit);
        }
        for (program, scopes) in programs.iter_mut() {
            let result = unsafe { world.call(lower_range_failures(world, program, scopes, &known, module)).await }?;
            changed |= result.0;
            scoped |= result.1;
        }
        for index in 0..module.programs.len() {
            let mut pending = module.programs[index].take().unwrap();
            let result = unsafe { world.call(lower_range_failures(world, &mut pending.program, &mut pending.scopes, &known, module)).await }?;
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
        resolve_range_views(&world.worker, module, program, scopes)?;
    }
    for index in 0..module.programs.len() {
        let mut pending = module.programs[index].take().unwrap();
        resolve_range_views(&world.worker, module, &mut pending.program, &pending.scopes)?;
        module.programs[index] = Some(pending);
    }
    Ok(())
}

async fn bind_range_body(
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
        unsafe { world.call(lower_failure_calls(world, body, None, fallible, None)).await }?;
    }
    calls(&mut body.code,&mut |call|call.failure=None);
    Ok(())
}
async fn bind_range_program(
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
        unsafe { world.call(bind_range_body(world, &mut body, framed, fallible, program.native_frame, scopes.contains_key(&index))).await }?;
        stage.registers=body.registers;stage.locals=body.locals;stage.code=body.code;
        let values=match &mut stage.operation{
            Operation::CustomReduce(value)=>Some(value),
            Operation::CustomScan(value)=>Some(&mut value.values),
            Operation::CustomSegmentedReduce(value)=>Some(&mut value.values),
            _=>None,
        };
        if let Some(values)=values {
            for body in values.project.iter_mut().chain([&mut values.identity,&mut values.combine]){
                unsafe { world.call(bind_range_body(world, body, framed, fallible, program.native_frame, false)).await }?;
            }
        }
    }
    Ok(())
}
fn resolve_range_views(
    worker: &crate::compiler::Worker,
    module: &Module, program: &mut Program, scopes: &BTreeMap<usize, RangeContext>,
) -> Result<()> {
    if scopes.is_empty() { return Ok(()); }
    let mut closed = BTreeMap::new();
    for call in scopes.values().flat_map(|scope| &scope.view_model.calls) {
        let callee = &call.function;
        if !closed.contains_key(callee) {
            let contract = &module.units[callee].as_ref().unwrap().contract;
            closed.insert(callee.clone(), view_outputs(contract));
        }
    }
    let names = scopes.keys().map(|&index| (format!("{}@range{index}", program.name), index))
        .collect::<BTreeMap<_, _>>();
    let functions = names.iter().map(|(name, index)| (name.clone(), &scopes[index].view_model))
        .collect::<BTreeMap<_, _>>();
    for (name, memory) in view_contracts::solve_range_memory(worker, &functions, closed)? {
        let stage = &mut program.stages[names[&name]];
        address::apply_memory(&mut stage.code, &memory, &stage.locals);
    }
    Ok(())
}

async fn finalize(
    world:&World,
    mut module: Module,
    entries: BTreeSet<String>,
    mut root: Option<(&mut ValueCode,&view_contracts::Function)>,
    owner: Option<&str>,
    programs: &mut [(&mut Program, &mut BTreeMap<usize, RangeContext>)],
) -> Result<Finalized> {
    use crate::module::codec::Encode;
    unsafe { world.call(lower_module_failures(world,&mut module,programs)).await }?;
    let mut imports = BTreeMap::new();
    resolve_residencies(world, &mut module, Some(&mut imports))?;
    let mut resources = std::mem::take(&mut module.resources);
    let mut pending_programs = std::mem::take(&mut module.programs)
        .into_iter().map(Option::unwrap).collect::<Vec<_>>();
    let standalone = !programs.is_empty();
    let mut programs = programs.iter_mut().map(|(program, scopes)| (&mut **program, &**scopes, None))
        .chain(pending_programs.iter_mut().map(|pending|
            (&mut pending.program, &pending.scopes, pending.deferred_source.as_deref())))
        .map(|(program, scopes, source)| {
            let names=dependencies::program(program);
            (program, scopes, source, names)
        }).collect::<Vec<_>>();
    let mut reachable = entries.clone();
    for (_, _, _, names) in &programs { reachable.extend(names.iter().cloned()); }
    let mut units = std::mem::take(&mut module.units).into_iter()
        .map(|(name, unit)| (name, unit.unwrap()))
        .collect::<BTreeMap<_, _>>();
    dependencies::complete(&units, &mut reachable, owner);
    if standalone {
        // Carry the external contracts of the whole emitted code closure.
        imports.retain(|name, _| reachable.contains(name));
    }
    let fallible=units.iter().filter_map(|(name,unit)|unit.contract.fallible.then(||name.clone())).collect::<BTreeSet<_>>();
    if let Some((body,model))=&mut root {
        lower_dataset_borrows(body,model,&|callee|&units[callee].contract);
    }
    let frame=frame_effects::run(&mut units);
    for (name, unit) in &mut units {
        if unit.c_abi && unit.contract.frame {
            return unit.source.fail(unit.span, "B263", "C入口的实际调用闭包需要Frame；C ABI不能隐式取得执行上下文");
        }
        if (unit.function.is_none() || owner.is_some_and(|owner| unit.module != owner))
            && (!standalone || reachable.contains(name))
        {
            imports.insert(name.clone(), unit.contract.bytes());
        }
    }
    for (program, scopes, source, names) in &mut programs {
        for name in names.iter() {
            if frame.contains(name) && program.target()!=Target::Cpu {
                let unit = &units[name];
                return unit.source.fail(unit.span, "B263", "该范围产物尚未交接真实Frame及同源资源定义");
            }
        }
        let needs_frame=names.iter().any(|name|frame.contains(name));
        unsafe { world.call(bind_range_program(world, program, &frame, &fallible, needs_frame, scopes)).await }?;
        if let Some(source) = source {
            crate::optimize::optimize(program, &world.worker)
                .map_err(|error| crate::compiler::diagnostic(source, error))?;
        }
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
        unsafe { world.call(lower_failure_calls(world,body,None,&fallible,None)).await }?;
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
pub(super) async fn finish(world:&World, module: Module, root: &mut ValueCode, model:&view_contracts::Function) -> Result<Finalized> {
    let roots = dependencies::code(&root.code);
    unsafe { world.call(finalize(world, module, roots, Some((root,model)), None, &mut [])).await }
}
pub(super) async fn finish_program(
    world: &World, module: Module, mut program: Program,
    mut scopes: BTreeMap<usize, RangeContext>,
) -> Result<Program> {
    let mut finalized = unsafe { world.call(finalize(world, module, BTreeSet::new(), None, None, &mut [(&mut program, &mut scopes)])).await }?;
    application::lower_sdk_resources(&mut program, &mut finalized.functions, &mut finalized.resources);
    program.functions = finalized.functions.into();
    program.imports = finalized.imports;
    if program.native_frame.is_some() || !finalized.resources.types.is_empty() || !finalized.resources.programs.is_empty(){
        program.resources=Some(Box::new(finalized.resources));
    }
    Ok(program)
}

#[allow(dead_code)] // Returned by bootstrap.module_load.
#[derive(Clone, Copy)]
#[repr(u32)]
enum ModuleEntry {
    Native,
    DomainProgram,
    Check,
    Reject,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ModulePlan {
    entry: ModuleEntry,
    query_domain: u32,
    publish: u32,
    schedule: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.module_load::native_module_plan"]
    fn native_module_plan(
        source: crate::syntax::wire::SignatureNode,
        compiled: u32,
        local: u32,
        domain: u32,
        public: u32,
        explicit: u32,
    ) -> ModulePlan;
}

fn module_plan(definition: &crate::Definition, entry: &str, explicit: bool) -> ModulePlan {
    unsafe { native_module_plan(
        definition.function.signature_node(),
        u32::from(definition.compiled.is_some()),
        u32::from(definition.module == entry),
        u32::from(definition.execution == crate::execution::Requirement::Domain),
        u32::from(definition.public),
        u32::from(explicit),
    ) }
}

async fn module_instance(
    world: &World,
    instance: &crate::generics::Instance,
    functions: &mut Module,
    roots: &mut BTreeSet<String>,
    plan: ModulePlan,
) -> Result<()> {
    let definition = &instance.definition;

    if plan.query_domain != 0
        && let Some(signature) = unsafe { world.call(kernel_signature(world, instance)).await }?
    {
        let targets = unsafe { world.call(domain_declaration(world, instance, &signature, functions)).await }?;
        if plan.publish != 0 {
            roots.extend(targets.iter().map(|&target| crate::module::domain::symbol(&instance.name, target)));
        }
        return Ok(());
    }

    match plan.entry {
        ModuleEntry::Native => {
            if plan.publish != 0 {
                roots.insert(instance.name.clone());
            }
            *functions = unsafe { world.call(declaration(world, instance, std::mem::take(functions), ViewDemand::ModuleOutput)).await }?;
        }
        ModuleEntry::DomainProgram => {
            // The interface query already rejected this instance's program signature.
            return Err(invalid_kernel_interface(DomainInput::Declaration { instance, signature: None }));
        }
        ModuleEntry::Check => {
            unsafe { world.call(check_function(world, instance)).await }?;
        }
        ModuleEntry::Reject => {
            return definition.source.fail(
                definition.function.span(),
                "B270",
                "此普通原生模块入口需要CPU适用的普通函数",
            );
        }
    }
    Ok(())
}

pub(crate) async fn compile_module(
    world: &World,
    entry: &str,
    requests: crate::ExportRequests<'_>,
    imported: Option<&crate::module::Imports>,
) -> Result<crate::module::Module> {
    let interrupted = InterruptedUnits::default();
    let mut functions = Module::default();
    functions.interrupted_units = Some(interrupted.clone());
    let mut roots = BTreeSet::new();
    let mut prepared = Vec::new();
    let preparation = async {
        for (key, definition) in &world.functions {
            let plan = module_plan(definition, entry, false);
            if plan.schedule == 0 {
                continue;
            }
            let mut instances = Vec::new();
            unsafe { world.call(crate::generics::for_each_instance(world, key, definition, |instance| instances.push(instance))).await }?;
            for instance in instances {
                unsafe { world.call(module_instance(world, &instance, &mut functions, &mut roots, plan)).await }?;
                prepared.push(instance.name.clone());
            }
        }
        for request in requests {
            let request = request?;
            let crate::ExportRequest { source, name, arguments } = request.as_ref();
            let span = Span { start: 0, end: source.text.len() };
            let (key, plan) = world
                .resolve(name, entry, &[], true).unique()

                .and_then(|key| {
                    let plan = module_plan(&world.functions[&key], entry, true);
                    (plan.schedule != 0).then_some((key, plan))
                })
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
            let arguments=match arguments.as_ref() {
                Some(arguments)=>Some(unsafe { world.call(bindings.arguments(world,arguments,
                    (source,&d.module,&imports),span)).await }?),
                None=>None,
            };
            let instance = unsafe { world.call(super::generics::specialize(
                world,
                &key,
                &[],
                arguments.as_deref(),
                (source, &d.module, &imports),
                span,
            )).await }?;
            unsafe { world.call(module_instance(world, &instance, &mut functions, &mut roots, plan)).await }?;
            prepared.push(instance.name.clone());
        }

        Ok::<(), Diagnostic>(())
    }.await;
    functions.interrupted_units = None;
    if let Err(error) = preparation {
        // The moved builder may have ended while preparing a later declaration.
        // Earlier view models still determine the original diagnostic order.
        if let Some(units) = interrupted.borrow_mut().take() {
            functions.units = units;
        }
        for name in prepared {
            if functions.units.get(&name).and_then(Option::as_ref)
                .is_some_and(|unit| unit.contract.view_access.is_none())
            {
                resolve_views(world, &mut functions, Some(&name))?;
            }
        }
        return Err(error);
    }
    resolve_module_views(world, &mut functions, &prepared)?;
    let finalized = unsafe { world.call(finalize(world, functions, roots, None, Some(entry), &mut [])).await }?;
    let mut functions = finalized.functions;
    let resources = application::module_resources(&mut functions, finalized.resources);
    Ok(crate::module::Module {
        compiler: world.worker.compiler().clone(),
        name: entry.into(),
        header: world.modules[entry].interface.source.text.clone(),
        requirements: Arc::new(world
            .functions
            .iter()
            .filter(|(_, d)| d.module == entry && d.public)
            .map(|(name, d)| (name.to_owned(), d.execution))
            .collect()),
        functions,
        imports: finalized.imports,
        exports: finalized.exports,
        resources,
    })
}

async fn ensure_domain<'a>(
    world: &World,
    input: DomainInput<'_>,
    functions: &'a mut Module,
    target: Target,
    parameters: Option<&[Param]>,
) -> Result<&'a crate::module::domain::Interface> {
    let name = crate::module::domain::symbol(input.name(), target);
    if !functions.units.contains_key(&name) {
        if let DomainInput::Declaration { instance, .. } = input
            && instance.definition.compiled.is_some() {
            let mut reference = instance.clone();
            reference.name = name.clone();
            *functions = unsafe { world.call(declaration(world, &reference, std::mem::take(functions), ViewDemand::Caller)).await }?;
        } else {
            unsafe { world.call(build_domain(world, input, functions, target, parameters)).await }?;
        }
    }
    functions.units[&name]
        .as_ref()
        .unwrap()
        .contract
        .domain
        .as_ref()
        .ok_or_else(|| Diagnostic {
            source: input.owner().definition.source.clone(),
            span: input.span(),
            code: "B270",
            message: "模块入口缺少整域合同".into(),
        })
}
impl Builder<'_> {
    pub(super) async fn prepare_domain(
        &mut self,
        input: DomainInput<'_>,
        target: Target,
        parameters: &[Param],
    ) -> Result<crate::module::domain::Interface> {
        let interface = unsafe {
            self.world.call(ensure_domain(self.world, input, &mut self.functions, target, Some(parameters))).await
        }?;
        Ok(interface.clone())
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
pub(crate) async fn check_domain(
    world: &World,
    instance: &crate::generics::Instance,
    signature: Option<&KernelSignature>,
) -> Result<()> {
    let checked;
    let signature = match signature {
        Some(signature) => signature,
        None => {
            let input = DomainInput::Declaration { instance, signature: None };
            let parameters = unsafe { world.call(input.parameters(world)).await }?.into_owned();
            let bindings = unsafe { world.call(check_bound_kernel(world, instance, &parameters)).await }?;
            checked = KernelSignature { parameters, bindings };
            &checked
        }
    };
    unsafe { world.call(domain_declaration(world, instance, signature, &mut Module::default())).await }?;
    Ok(())
}

async fn check_bound_kernel(world: &World, instance: &crate::generics::Instance, parameters: &[Param]) -> Result<Vec<KernelBinding>> {
    let mut bindings = Vec::new();
    let eligible = unsafe {
        world.call(kernel_parameters(world, instance, parameters.iter().map(ScopeParameter::Bound), |_, binding| {
            bindings.push(binding);
        })).await
    }?;
    if !eligible {
        return Err(invalid_kernel_interface(DomainInput::Declaration { instance, signature: None }));
    }
    Ok(bindings)
}

async fn domain_declaration(
    world: &World,
    instance: &crate::generics::Instance,
    signature: &KernelSignature,
    functions: &mut Module,
) -> Result<&'static [Target]> {
    let targets: &'static [Target] = match instance.definition.function.target() {
        Target::Cpu => &[Target::Cpu],
        Target::Gpu => &[Target::Gpu],
        Target::Compute => &[Target::Cpu, Target::Gpu],
    };
    let input = DomainInput::Declaration { instance, signature: Some(signature) };
    for &target in targets {
        unsafe { world.call(ensure_domain(world, input, functions, target, Some(&signature.parameters))).await }?;
    }
    Ok(targets)
}

pub(super) struct DomainEntry {
    frame: CallArgument,
    registers: Vec<Scalar>,
    parameters: Vec<CallParameter>,
    data: Vec<[Reg; 2]>,
    textures: Vec<CallArgument>,
    uniforms: Vec<std::ops::Range<Reg>>,
}

impl DomainEntry {
    fn new() -> Self {
        let mut ty = Ty::vector(Scalar::U32, 2);
        ty.name = "__blink_frame".into();
        let parameters = vec![CallParameter::value(ty.clone())];
        let frame = CallArgument::Value {
            origin: None, dataset_borrow: None, ty, values: vec![0, 1],
        };
        Self { frame, parameters, registers: vec![Scalar::U32, Scalar::U32],
            data: Vec::new(), textures: Vec::new(), uniforms: Vec::new() }
    }

    pub(super) fn dataset(&mut self) {
        let first = self.registers.len();
        self.registers.extend([Scalar::U32; 2]);
        self.parameters.push(CallParameter::value(Builder::domain_dataset_parameter_type()));
        self.data.push([first, first + 1]);
    }

    pub(super) fn texture(&mut self) {
        let register = self.registers.len();
        self.registers.push(Scalar::U64);
        let ty = Ty::scalar(Scalar::U64);
        self.parameters.push(CallParameter::value(ty.clone()));
        self.textures.push(CallArgument::Value {
            origin: None, dataset_borrow: None, ty, values: vec![register],
        });
    }

    pub(super) fn uniform(&mut self, ty: &Ty) {
        let first = self.registers.len();
        self.registers.extend(ty.components.iter().map(|(scalar, _)| *scalar));
        self.parameters.push(CallParameter::value(ty.clone()));
        self.uniforms.push(first..self.registers.len());
    }
}

async fn build_domain(
    world: &World,
    input: DomainInput<'_>,
    functions: &mut Module,
    target: Target,
    parameters: Option<&[Param]>,
) -> Result<()> {
    let d = &input.owner().definition;
    let bound_parameters = match parameters {
        Some(parameters) => Cow::Borrowed(parameters),
        None => unsafe { world.call(input.parameters(world)).await }?,
    };
    let bindings = match input {
        DomainInput::Declaration { signature: Some(signature), .. } => {
            DomainBindings::Shared(signature.bindings.iter())
        }
        DomainInput::Declaration { instance, signature: None } => {
            let bindings = unsafe { world.call(check_bound_kernel(world, instance, &bound_parameters)).await }?;
            DomainBindings::Owned(bindings.into_iter())
        }
        DomainInput::Parallel { .. } => DomainBindings::Unqualified,
    };
    let mut entry = DomainEntry::new();
    let (expanded, bound_contracts) = unsafe {
        world.call(expand_domain(world, input, &bound_parameters, bindings, functions, Some(&mut entry))).await
    }?;
    let mut signature = expanded.program;
    let mut scopes = expanded.scopes;
    if !signature.accelerations.is_empty() || target == Target::Gpu && !signature.textures.is_empty() {
        return d.source.fail(
            input.span(),
            "B270",
            "B应用的独立整域入口尚未交接纹理或加速结构；原生宿主算法接口继续使用已有产物",
        );
    }
    let mut stages = std::mem::take(&mut signature.stages).into_iter();
    let expansion = signature.generated_origin.take();
    let mut required = std::mem::take(&mut signature.prefix_requirements);
    let guaranteed = std::mem::take(&mut signature.prefix_guarantees);
    let element_requirements = std::mem::take(&mut signature.element_requirements);
    let element_guarantees = std::mem::take(&mut signature.element_guarantees);
    let mut element_required = Cow::Borrowed(element_requirements.as_slice());
    let DomainEntry { frame, mut registers, parameters, data, textures: texture_values, uniforms: uniform_inputs } = entry;
    let inputs: Vec<_> = (0..registers.len()).collect();
    let mut code = Vec::new();
    let entry_context=match input {
        DomainInput::Declaration { instance, .. }=>match &d.syntax_origin {
            crate::frontend::DefinitionSyntax::Source=>Some(GenerationAnchor::Definition{
                origin:d.source_origin().map(Arc::new),instance:instance.name.clone().into()}),
            crate::frontend::DefinitionSyntax::CompiledInterface=>None,
        },
        DomainInput::Parallel{site,..}=>site.clone().map(GenerationAnchor::Syntax),
    };
    let mut interface = crate::module::domain::Interface {
        instance: signature.name.clone(), target,
        buffers: signature.buffers.clone(), textures: signature.textures.clone(), uniforms: signature.uniforms.clone(),
        write_regions: expanded.tail_writes,
        length_requirements: signature.length_requirements.clone(),
        element_requirements: element_requirements.clone(),
        prefix_guarantees: guaranteed.clone(),
        outputs: expanded.outputs,
        elements: expanded.elements,
    };
    let mut start = 0;
    let split = !expanded.calls.is_empty();
    let mut part = 0;
    for call in expanded.calls {
        for (binding, written) in call.write_regions {
            interface.write_regions.entry(binding).or_default().extend(written);
        }
        emit_domain_part(
            &world.worker,
            &d.source.path,
            entry_context.as_ref(),
            Cow::Borrowed(&signature),
            stages.by_ref().take(call.stage - start).collect(),
            &mut scopes,
            start,
            required,
            call.prefix_guarantees,
            element_required,
            call.element_guarantees,
            split,
            &mut part,
            functions,
            target,
            &frame,
            &data,
            &uniform_inputs,
            &texture_values,
            &mut registers,
            &mut code,
        )?;
        unsafe { world.call(ensure_domain(world, DomainInput::Declaration { instance: &call.instance, signature: None }, functions, target, None)).await }?;
        let mut arguments = vec![frame.clone()];
        for root in &call.arguments {
            arguments.push(match root {
                Root::Data(i, _) => CallArgument::Value { #[cfg(feature = "compiler")] origin: None,
                    dataset_borrow:None, ty: Builder::domain_dataset_parameter_type(),
                    values: data[*i].to_vec(),
                },
                Root::Texture(i) => texture_values[*i].clone(),
                Root::Uniform(i) => CallArgument::Value { #[cfg(feature = "compiler")] origin: None,
                    dataset_borrow:None, ty: signature.uniforms[*i].ty.clone(),
                    values: uniform_inputs[*i].clone().collect(),
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
        required = call.prefix_requirements;
        element_required = Cow::Owned(call.element_requirements);
    }
    emit_domain_part(
        &world.worker,
        &d.source.path,
        entry_context.as_ref(),
        Cow::Owned(signature),
        stages.collect(),
        &mut scopes,
        start,
        required,
        guaranteed,
        element_required,
        element_guarantees,
        split,
        &mut part,
        functions,
        target,
        &frame,
        &data,
        &uniform_inputs,
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
                context: entry_context.map(Box::new), expansion,
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
        requires: bound_contracts.into_iter().map(|contract| (contract.kind, contract.predicate)).collect(),
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
            module_view: None,
        }),
    );
    Ok(())
}
fn emit_domain_part(
    worker: &crate::compiler::Worker,
    source: &std::path::Path,
    generation: Option<&GenerationAnchor>,
    signature: Cow<'_, Program>,
    stages: Vec<Stage>,
    scopes: &mut BTreeMap<usize, RangeContext>,
    start: usize,
    required: Vec<PrefixRelation>,
    guaranteed: Vec<PrefixRelation>,
    element_required: Cow<'_, [ElementFact]>,
    element_guaranteed: Vec<ElementFact>,
    split: bool,
    part: &mut usize,
    functions: &mut Module,
    target: Target,
    frame: &CallArgument,
    data: &[[Reg; 2]],
    uniform_inputs: &[std::ops::Range<Reg>],
    textures: &[CallArgument],
    registers: &mut Vec<Scalar>,
    code: &mut Vec<Inst>,
) -> Result<()> {
    if stages.is_empty() {
        return Ok(());
    }
    let mut program = signature.into_owned();
    if split {
        program.name = format!("{}@part{}", program.name, *part);
    }
    *part += 1;
    let stage_count = stages.len();
    program.source_stages = stage_count;
    program.stages = stages;
    program.prefix_requirements = required;
    program.prefix_guarantees = guaranteed;
    program.element_requirements = element_required.into_owned();
    program.element_guarantees = element_guaranteed;
    let scopes = scopes
        .extract_if(start..start + stage_count, |_, _| true)
        .map(|(index, scope)| (index - start, scope))
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
    arguments.extend(uniform_inputs.iter().cloned().flatten().map(|reg| CallArgument::Value {
        #[cfg(feature = "compiler")]
        origin: None,
        dataset_borrow: None,
        ty: Ty::scalar(registers[reg]),
        values: vec![reg],
    }));
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

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(Contract);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        parameters: Slot,
        result: Slot,
        inputs: Slot,
        element_inputs: Slot,
        returned: Slot,
        written: Slot,
        requires: Slot,
        target: Slot,
        frame: Slot,
        fallible: Slot,
        domain: Slot,
        residency: Slot,
        view_access: Slot,
        initialization: Slot,
    }
    #[repr(C)]
    struct Writer {
        parameters: WriteCallback,
        result: WriteCallback,
        inputs: WriteCallback,
        element_inputs: WriteCallback,
        returned: WriteCallback,
        written: WriteCallback,
        requires: WriteCallback,
        target: WriteCallback,
        frame: WriteCallback,
        fallible: WriteCallback,
        domain: WriteCallback,
        residency: WriteCallback,
        view_access: WriteCallback,
        initialization: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        parameters: ReadCallback,
        result: ReadCallback,
        inputs: ReadCallback,
        element_inputs: ReadCallback,
        returned: ReadCallback,
        written: ReadCallback,
        requires: ReadCallback,
        target: ReadCallback,
        frame: ReadCallback,
        fallible: ReadCallback,
        domain: ReadCallback,
        residency: ReadCallback,
        view_access: ReadCallback,
        initialization: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_ordinary_contract"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_ordinary_contract"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Contract {
        pub(super) unsafe fn write_public(
            output: *mut crate::codec::Writer,
            parameters: &Vec<CallParameter>,
            result: &Option<Ty>,
            inputs: &Vec<Reg>,
            element_inputs: &BTreeMap<residency::Input, (usize, usize)>,
            returned: &Option<Returned>,
            written: &Vec<Option<Returned>>,
            requires: &BTreeSet<(ContractKind, ContractPredicate)>,
            target: &Target,
            frame: &bool,
            fallible: &bool,
            domain: &Option<crate::module::domain::Interface>,
            residency: &residency::Contract,
            view_access: &BTreeSet<view_contracts::Access>,
            initialization: &Vec<address::Initialization>,
        ) {
            unsafe {
                write(
                    output.cast(),
                    Fields {
                        parameters: contracts::address(parameters).cast_mut(),
                        result: contracts::address(result).cast_mut(),
                        inputs: contracts::address(inputs).cast_mut(),
                        element_inputs: contracts::address(element_inputs).cast_mut(),
                        returned: contracts::address(returned).cast_mut(),
                        written: contracts::address(written).cast_mut(),
                        requires: contracts::address(requires).cast_mut(),
                        target: contracts::address(target).cast_mut(),
                        frame: contracts::address(frame).cast_mut(),
                        fallible: contracts::address(fallible).cast_mut(),
                        domain: contracts::address(domain).cast_mut(),
                        residency: contracts::address(residency).cast_mut(),
                        view_access: contracts::address(view_access).cast_mut(),
                        initialization: contracts::address(initialization).cast_mut(),
                    },
                    Writer {
                        parameters: contracts::write_value::<Vec<CallParameter>>,
                        result: contracts::write_value::<Option<Ty>>,
                        inputs: contracts::write_value::<Vec<Reg>>,
                        element_inputs: contracts::write_value::<
                            BTreeMap<residency::Input, (usize, usize)>,
                        >,
                        returned: contracts::write_value::<Option<Returned>>,
                        written: contracts::write_value::<Vec<Option<Returned>>>,
                        requires: contracts::write_value::<
                            BTreeSet<(ContractKind, ContractPredicate)>,
                        >,
                        target: contracts::write_value::<Target>,
                        frame: contracts::write_value::<bool>,
                        fallible: contracts::write_value::<bool>,
                        domain: contracts::write_value::<Option<crate::module::domain::Interface>>,
                        residency: contracts::write_value::<residency::Contract>,
                        view_access: contracts::write_value::<BTreeSet<view_contracts::Access>>,
                        initialization: contracts::write_value::<Vec<address::Initialization>>,
                    },
                )
            }
        }
    }
    impl Native for Contract {
        fn empty() -> Self {
            Self {
                parameters: <Vec<CallParameter> as Native>::empty(),
                result: <Option<Ty> as Native>::empty(),
                inputs: <Vec<Reg> as Native>::empty(),
                element_inputs: <BTreeMap<residency::Input, (usize, usize)> as Native>::empty(),
                returned: <Option<Returned> as Native>::empty(),
                written: <Vec<Option<Returned>> as Native>::empty(),
                requires: <BTreeSet<(ContractKind, ContractPredicate)> as Native>::empty(),
                target: <Target as Native>::empty(),
                frame: <bool as Native>::empty(),
                fallible: <bool as Native>::empty(),
                domain: <Option<crate::module::domain::Interface> as Native>::empty(),
                residency: <residency::Contract as Native>::empty(),
                view_access: Some(<BTreeSet<view_contracts::Access> as Native>::empty()),
                initialization: Some(<Vec<address::Initialization> as Native>::empty()),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe { contract::encode(self, &mut *output.cast::<crate::codec::Writer>()) }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let mut parameters = <Vec<CallParameter> as Native>::empty();
            let mut result = <Option<Ty> as Native>::empty();
            let mut inputs = <Vec<Reg> as Native>::empty();
            let mut element_inputs =
                <BTreeMap<residency::Input, (usize, usize)> as Native>::empty();
            let mut returned = <Option<Returned> as Native>::empty();
            let mut written = <Vec<Option<Returned>> as Native>::empty();
            let mut requires = <BTreeSet<(ContractKind, ContractPredicate)> as Native>::empty();
            let mut target = <Target as Native>::empty();
            let mut frame = <bool as Native>::empty();
            let mut fallible = <bool as Native>::empty();
            let mut domain = <Option<crate::module::domain::Interface> as Native>::empty();
            let mut residency = <residency::Contract as Native>::empty();
            let mut view_access = <BTreeSet<view_contracts::Access> as Native>::empty();
            let mut initialization = <Vec<address::Initialization> as Native>::empty();
            let status = unsafe {
                read(
                    context,
                    Fields {
                        parameters: contracts::slot(&mut parameters),
                        result: contracts::slot(&mut result),
                        inputs: contracts::slot(&mut inputs),
                        element_inputs: contracts::slot(&mut element_inputs),
                        returned: contracts::slot(&mut returned),
                        written: contracts::slot(&mut written),
                        requires: contracts::slot(&mut requires),
                        target: contracts::slot(&mut target),
                        frame: contracts::slot(&mut frame),
                        fallible: contracts::slot(&mut fallible),
                        domain: contracts::slot(&mut domain),
                        residency: contracts::slot(&mut residency),
                        view_access: contracts::slot(&mut view_access),
                        initialization: contracts::slot(&mut initialization),
                    },
                    Reader {
                        parameters: contracts::read_value::<Vec<CallParameter>>,
                        result: contracts::read_value::<Option<Ty>>,
                        inputs: contracts::read_value::<Vec<Reg>>,
                        element_inputs: contracts::read_value::<
                            BTreeMap<residency::Input, (usize, usize)>,
                        >,
                        returned: contracts::read_value::<Option<Returned>>,
                        written: contracts::read_value::<Vec<Option<Returned>>>,
                        requires: contracts::read_value::<
                            BTreeSet<(ContractKind, ContractPredicate)>,
                        >,
                        target: contracts::read_value::<Target>,
                        frame: contracts::read_value::<bool>,
                        fallible: contracts::read_value::<bool>,
                        domain: contracts::read_value::<Option<crate::module::domain::Interface>>,
                        residency: contracts::read_value::<residency::Contract>,
                        view_access: contracts::read_value::<BTreeSet<view_contracts::Access>>,
                        initialization: contracts::read_value::<Vec<address::Initialization>>,
                    },
                )
            };
            if status == ReadStatus::Success {
                *output = Self {
                    parameters,
                    result,
                    inputs,
                    element_inputs,
                    returned,
                    written,
                    requires,
                    target,
                    frame,
                    fallible,
                    domain,
                    residency,
                    view_access: Some(view_access),
                    initialization: Some(initialization),
                };
            }
            status
        }
    }
};
