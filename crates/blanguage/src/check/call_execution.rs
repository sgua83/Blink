//! B controls invocation and the original inline environment's full lifetime.
use super::*;
use cooperative::{Argument,parameter_type};
use returns::Returned;
use std::ffi::c_void;
#[repr(C)] struct Phase{phase:u32}
#[derive(Clone,Copy)]
#[repr(C)] struct InvokeFacts{external:u32,compiled:u32,domain:u32,application:u32,entry:u32,target:u32,callee_target:u32,kind:u32,kernel:u32}
#[repr(u32)] enum InvokeNeed{External,InvalidCompiledTarget,Native,Kernel,Domain,InvalidDomain,InvalidCooperative,InvalidConstant,Inline}
#[derive(Clone,Copy)]
#[repr(C)] struct CooperateFacts{root:u32,name:u32,kind:u32,arguments:u64,parameters:u64,present:u32,explicit:u32}
#[repr(u32)] enum CooperateNeed{InvalidRoot,Resolve,Unknown,InvalidKind,CountMismatch,Arguments,Empty,Explicit,Specialize,Inline,Return}
#[repr(C)] pub(super) struct ParameterFacts{access:Access,tile:u32,kind:u32,target:u32,owns:u32,views:u32}
#[repr(u32)] pub(super) enum ParameterError{None,Owned,Tile,Atomic}
#[derive(Clone,Copy)]
#[repr(C)] struct InlineCursor{index:u64,part:u64,phase:u32,initialization:u32,bounds:u32}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct Count{present:u32,value:u32}
impl From<Option<u32>> for Count{fn from(value:Option<u32>)->Self{match value{None=>Self{present:0,value:0},Some(value)=>Self{present:1,value}}}}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct InlineFacts{
 target:u32,callee_target:u32,kind:u32,recursive:u32,
 parameters:u64,arguments:u64,access:u32,dataset:u32,tile:u32,acceleration:u32,texture:u32,counted:u32,argument:u32,actual_access:u32,
 expected_count:Count,actual_count:Count,unit:u32,never:u32,result_ready:u32,result_borrows:u32,result_owns:u32,result_facts:u32,
 input_plain:u32,continuing:u32,items:u64,held:u64,positions:u64,parameter_position:u32,declared:u32,
}
#[repr(u32)] enum InlineNeed{
 InvalidTarget,Recursion,Native,InvalidRecursion,ResultName,Parameter,UnwrapScoped,MaterializeRead,Acceleration,InvalidAcceleration,Texture,InvalidTexture,ParameterType,
 DatasetType,InvalidDatasetAccess,DatasetWrite,DatasetAccess,DatasetShape,InvalidShape,Dataset,Atomic,ReferenceType,ReferenceRead,ReferenceWrite,ReferenceInOut,TileType,InvalidTileAccess,Tile,Value,InvalidArgument,
 Resolve,Prove,Assume,EnterEnvironment,AssignedNames,EnterContext,InitializationInputs,ResultProperties,IsolateInitialization,Materialize,OutputInputs,BoundsInputs,EnterPath,Body,RestoreInitialization,MarkResult,PopCall,RestorePath,JoinPath,ReturnedStorage,Update,RestoreEnvironment,InspectUpdate,InspectPosition,InsertScope,FilterInitialization,OutputDeclared,ForgetOutput,InitializeOutput,BeginProofs,AssignProof,RestoreContext,FinishScoped,Complete,ResultFacts,EnterCooperativeContext,Next,
}
#[repr(C)] struct InlineStep{cursor:InlineCursor,need:InlineNeed}
#[repr(C)] struct BindingItem{present:u32,value:*const Value}
#[repr(C)] struct EnvironmentHost{next:unsafe extern "C" fn(*mut c_void)->BindingItem,borrows:unsafe extern "C" fn(*const Value)->u32,owns:unsafe extern "C" fn(*const Value)->u32,facts:unsafe extern "C" fn(*const Value)->u32}
#[repr(C)] struct PositionHost{next:unsafe extern "C" fn(*mut c_void,*const Position)->u32}
unsafe extern "C"{
 #[link_name="bootstrap.call_execution::invoke_start"]fn b_invoke_start()->Phase;
 #[link_name="bootstrap.call_execution::invoke_step"]fn b_invoke_step(cursor:*mut Phase,facts:InvokeFacts)->InvokeNeed;
 #[link_name="bootstrap.call_execution::cooperate_start"]fn b_cooperate_start()->Phase;
 #[link_name="bootstrap.call_execution::cooperate_step"]fn b_cooperate_step(cursor:*mut Phase,facts:CooperateFacts)->CooperateNeed;
 #[link_name="bootstrap.call_execution::parameter_error"]fn b_parameter_error(facts:ParameterFacts)->ParameterError;
 #[link_name="bootstrap.call_execution::inline_start"]fn b_inline_start()->InlineCursor;
 #[link_name="bootstrap.call_execution::inline_step"]fn b_inline_step(state:InlineCursor,facts:InlineFacts)->InlineStep;
 #[link_name="bootstrap.call_execution::environment_plain"]fn b_environment_plain(iterator:*mut c_void,facts:u32,host:EnvironmentHost)->u32;
 #[link_name="bootstrap.call_execution::keep_position"]fn b_keep_position(iterator:*mut c_void,position:*const Position,host:PositionHost)->u32;
}
fn step_inline(cursor:&mut InlineCursor,facts:InlineFacts)->InlineNeed{
 let step=unsafe{b_inline_step(*cursor,facts)};*cursor=step.cursor;step.need
}
fn target(target:Target)->u32{match target{Target::Cpu=>0,Target::Gpu=>1,Target::Compute=>2}}
fn kind(kind:FunctionKind)->u32{match kind{FunctionKind::Value=>0,FunctionKind::Cooperative=>1,FunctionKind::Constant=>2}}
fn argument_kind(argument:&Argument)->u32{match argument{
 Argument::Acceleration(_)=>1,Argument::Texture(_)=>2,Argument::Value(_)=>3,Argument::Dataset(_)=>4,Argument::Scoped{..}=>5,Argument::Tile(_)=>6,Argument::Atomic(_)=>7,Argument::Reference(_)=>8,_=>0,
}}
pub(super) fn parameter_error(param:&Param,ty:&Ty,function:FunctionKind,execution:Target)->ParameterError{
 unsafe{b_parameter_error(ParameterFacts{access:param.access,tile:u32::from(param.ty.tile),kind:kind(function),target:target(execution),owns:u32::from(ty.owns_resources()),views:u32::from(ty.contains_views())})}
}
unsafe extern "C" fn next_binding(context:*mut c_void)->BindingItem{
 let iterator=unsafe{&mut *context.cast::<std::collections::btree_map::Values<'_,String,Binding>>()};
 match iterator.next(){None=>BindingItem{present:0,value:std::ptr::null()},Some(Binding::Value(value))=>BindingItem{present:1,value},Some(_)=>BindingItem{present:1,value:std::ptr::null()}}
}
unsafe extern "C" fn value_borrows(value:*const Value)->u32{u32::from(unsafe{&*value}.ty.contains_borrows())}
unsafe extern "C" fn value_owns(value:*const Value)->u32{u32::from(unsafe{&*value}.ty.owns_resources())}
unsafe extern "C" fn value_facts(value:*const Value)->u32{u32::from(bounds::Proof::fact_components(&unsafe{&*value}.ty).next().is_some())}
fn plain_environment(env:&BTreeMap<String,Binding>,facts:bool)->u32{
 let mut iterator=env.values();unsafe{b_environment_plain(std::ptr::from_mut(&mut iterator).cast(),u32::from(facts),EnvironmentHost{next:next_binding,borrows:value_borrows,owns:value_owns,facts:value_facts})}
}
unsafe extern "C" fn next_scope(context:*mut c_void,position:*const Position)->u32{
 let iterator=unsafe{&mut *context.cast::<std::collections::btree_set::Iter<'_,usize>>()};
 match iterator.next(){None=>0,Some(scope)=>if unsafe{&*position}.uses_scope(*scope){2}else{1}}
}
fn keep_position(position:&Position,scopes:&BTreeSet<usize>)->bool{
 let mut iterator=scopes.iter();unsafe{b_keep_position(std::ptr::from_mut(&mut iterator).cast(),position,PositionHost{next:next_scope})!=0}
}
impl<'a> Builder<'a>{
 pub(super) async fn invoke_returned(&mut self,instance:crate::generics::Instance,args:Vec<Argument>,span:Span,generated_origin:Option<Arc<GeneratedOrigin>>,syntax:Option<&Expressions>)->Result<Option<Returned>>{
  let mut cursor=unsafe{b_invoke_start()};
  let mut signature = None;
  let mut facts=InvokeFacts{external:u32::from(instance.definition.function.external().is_some()),compiled:u32::from(instance.definition.compiled.is_some()),domain:u32::from(instance.definition.execution==crate::execution::Requirement::Domain),
   application:u32::from(self.application.is_some()),entry:u32::from(self.application_entry()),target:target(self.program.target),callee_target:target(instance.definition.function.target()),kind:kind(instance.definition.function.kind()),kernel:0};
  loop{match unsafe{b_invoke_step(&mut cursor,facts)}{
   InvokeNeed::External=>return unsafe{self.world.call(self.external_call(&instance,args,span,generated_origin)).await}.map(|v|v.map(Returned::value)),
   InvokeNeed::InvalidCompiledTarget=>return self.fail(span,"B161","已编译普通CPU模块调用需要CPU执行目标"),
   InvokeNeed::Native=>return unsafe{self.world.call(self.native_function(instance,args,span,generated_origin,syntax,None)).await},
   InvokeNeed::Kernel => {
    signature = unsafe { self.world.call(super::kernel_signature(self.world, &instance)).await }?;
    facts.kernel = u32::from(signature.is_some());
   }
   InvokeNeed::Domain => {
    let input = DomainInput::Declaration { instance: &instance, signature: signature.as_ref() };
    return unsafe { self.world.call(self.application_execute(input, args, Target::Cpu, span)).await };
   }
   InvokeNeed::InvalidDomain=>return self.fail(span,"B159","整域执行需要发起位置，不能由元素实例或协作成员发起"),
   InvokeNeed::InvalidCooperative=>return self.fail(span,"B208","cooperative 函数需要显式 cooperate 根块调用"),
   InvokeNeed::InvalidConstant=>return self.fail(span,"B234","const 通过名称读取，不能作为函数调用"),
   InvokeNeed::Inline=>return unsafe{self.world.call(self.inline_instance(instance,args,span,syntax)).await},
  }}
 }
 pub(super) async fn cooperate(&mut self,name:&str,explicit:Option<&crate::syntax::List<crate::syntax::wire::ArgumentNode>>,args:&Expressions,span:Span)->Result<Option<Value>>{
  let mut cursor=unsafe{b_cooperate_start()};let mut key=None;let mut arguments=None;let mut bound=None;let mut instance=None;let mut result=None;
  let mut facts=CooperateFacts{root:u32::from(self.is_cooperative_root()),name:0,kind:0,arguments:args.len()as u64,parameters:0,present:0,explicit:u32::from(explicit.is_some())};
  loop{match unsafe{b_cooperate_step(&mut cursor,facts)}{
   CooperateNeed::InvalidRoot=>return self.fail(span,"B208","cooperate 需要 parallel 或 cooperative 函数的根块声明，不能位于分支／循环或普通值函数中"),
   CooperateNeed::Resolve=>{
    key=self.world.resolve(name,self.scope.module(),self.scope.imports(),true).unique();facts.name=u32::from(key.is_some());
    if let Some(key)=&key{let callee=&self.world.functions[key];facts.kind=kind(callee.function.kind());facts.parameters=callee.function.params().len()as u64;}
   }
   CooperateNeed::Unknown=>return Err(Diagnostic{source:self.scope.source().clone(),span,code:"B158",message:format!("未知协作函数 {name}")}),
   CooperateNeed::InvalidKind=>return self.fail(span,"B208","cooperate 只能调用声明为 cooperative 的函数"),
   CooperateNeed::CountMismatch=>return self.fail(span,"B162","cooperate 参数数量不匹配"),
   CooperateNeed::Arguments=>{arguments=unsafe{self.world.call(self.function_arguments(key.as_ref().unwrap_unchecked(),args,span)).await}?;facts.present=u32::from(arguments.is_some());}
   CooperateNeed::Empty=>return Ok(None),
   CooperateNeed::Explicit=>{bound=Some(unsafe{self.world.call(self.scope.bindings().arguments(self.world,explicit.unwrap_unchecked(),(self.scope.source(),self.scope.module(),self.scope.imports()),span)).await}?);}
   CooperateNeed::Specialize=>{instance=Some(unsafe{self.world.call(self.specialized_arguments(key.take().unwrap_unchecked(),arguments.as_ref().unwrap_unchecked(),bound.as_deref(),span)).await}?);bound.take();}
   CooperateNeed::Inline=>{result=unsafe{self.world.call(self.inline_instance(instance.take().unwrap_unchecked(),arguments.take().unwrap_unchecked(),span,Some(args))).await}?;}
   CooperateNeed::Return=>return self.ordinary_return(result,span),
  }}
 }

 pub(super) async fn inline_instance(&mut self,instance:crate::generics::Instance,args:Vec<Argument>,span:Span,syntax:Option<&Expressions>)->Result<Option<Returned>>{
  let world=self.world;let mut cursor=unsafe{b_inline_start()};
  let mut facts=InlineFacts{target:target(self.program.target),callee_target:target(instance.definition.function.target()),kind:kind(instance.definition.function.kind()),parameters:instance.definition.function.params().len()as u64,arguments:args.len()as u64,..InlineFacts::default()};
  loop{match step_inline(&mut cursor,facts){
   InlineNeed::InvalidTarget=>return self.fail(span,"B161","函数设备能力与当前程序不相容"),
   InlineNeed::Recursion=>{facts.recursive=u32::from(self.calls.contains(&instance.name));}
   InlineNeed::Native=>return unsafe{world.call(self.native_function(instance,args,span,None,syntax,None)).await},
   InlineNeed::InvalidRecursion=>return self.fail(span,"B160","当前设备函数不允许递归调用"),
   InlineNeed::ResultName=>break,
   _=>unsafe{std::hint::unreachable_unchecked()},
  }}
  let mut key=instance.name.clone();let callee=&instance.definition;
  let result_name=unsafe{world.call(instance.bindings.type_name(world,&callee.function.result(),(&callee.source,&callee.module,&callee.imports),callee.function.span())).await}?;
  facts.unit=u32::from(result_name.name=="void");facts.never=u32::from(result_name.is_never());
  let mut parameters=Some(instance.parameters(world).zip(args));let mut param:Option<Param>=None;let mut argument=None;let mut ty=None;
  let mut env=BTreeMap::new();let mut borrowed=Vec::new();let mut held=Vec::new();let mut requirements=None;
  let mut old_env=None;let mut old_binding_uses=None;let mut callee_scope=None;let mut old_context=None;
  let mut caller_initialization=None;let mut result_layout=None;let mut output_inputs=None;let mut caller_path=None;
  let mut result:Option<Returned>=None;let mut returned=None;let mut updates=Vec::new();let mut proofs=None;let mut fresh_scopes=BTreeSet::new();
  loop{
   let need=step_inline(&mut cursor,facts);let index=cursor.index as usize;
   match need{
    InlineNeed::Parameter=>{
     let(next,value)=unsafe{parameters.as_mut().unwrap_unchecked().next().unwrap_unchecked()};let bound=unsafe{world.call(next).await}?;
     facts.access=bound.access as u32;facts.dataset=u32::from(bound.ty.dataset);facts.tile=u32::from(bound.ty.tile);facts.acceleration=u32::from(bound.ty.is_acceleration());facts.texture=u32::from(bound.ty.is_texture_resource());facts.counted=u32::from(bound.ty.count.is_some());
     facts.argument=argument_kind(&value);param=Some(bound);argument=Some(value);
    }
    InlineNeed::UnwrapScoped=>{
     let Argument::Scoped{value,owner}=(unsafe{argument.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     held.push(owner);let value=Argument::resource(value);facts.argument=argument_kind(&value);argument=Some(value);
    }
    InlineNeed::MaterializeRead=>{
     let Argument::Value(value)=(unsafe{argument.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     argument=Some(Argument::Reference(self.materialize_array(value,None)));facts.argument=8;
    }
    InlineNeed::Acceleration=>{
     let Argument::Acceleration(value)=(unsafe{argument.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     env.insert(unsafe{param.as_ref().unwrap_unchecked()}.name.clone(),Binding::Acceleration(value));
    }
    InlineNeed::InvalidAcceleration=>return self.fail(span,"B183","read acceleration 参数需要加速结构资源"),
    InlineNeed::Texture=>{
     let Argument::Texture(value)=(unsafe{argument.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     let param=unsafe{param.as_ref().unwrap_unchecked()};unsafe{world.call(self.texture_parameter(param,&value,(&callee.source,&callee.module,&callee.imports),span)).await}?;
     env.insert(param.name.clone(),Binding::Texture(value));
    }
    InlineNeed::InvalidTexture=>return self.fail(span,"B239","只读纹理参数需要相同维度的纹理资源"),
    InlineNeed::ParameterType=>{
     ty=Some(unsafe{world.call(parameter_type(world,param.as_ref().unwrap_unchecked(),(&callee.source,&callee.module,&callee.imports,callee.function.kind()),self.program.target)).await}?);
    }
    InlineNeed::DatasetType=>{
     let Argument::Dataset(view)=(unsafe{argument.as_ref().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     self.require_same(unsafe{ty.as_ref().unwrap_unchecked()},self.dataset_layout(view).1,span)?;facts.actual_access=view.access as u32;
    }
    InlineNeed::InvalidDatasetAccess=>return self.fail(span,"B224","并行实例的dataset借用需要对应的实际来源权限"),
    InlineNeed::DatasetWrite=>{
     let Argument::Dataset(view)=(unsafe{argument.as_ref().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     let place=self.dataset_element(view,self.global_index,false);self.forget_output_initialization(&place,span)?;
     borrowed.push((index,unsafe{param.as_ref().unwrap_unchecked()}.access,place));
    }
    InlineNeed::DatasetAccess=>{
     let Argument::Dataset(view)=(unsafe{argument.as_mut().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     view.access=unsafe{param.as_ref().unwrap_unchecked()}.access;
    }
    InlineNeed::DatasetShape=>{
     let param=unsafe{param.as_ref().unwrap_unchecked()};let Argument::Dataset(view)=(unsafe{argument.as_ref().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     facts.expected_count=unsafe{world.call(world.shape(&param.ty.count,&callee.source,param.span,&callee.module,&callee.imports)).await}?.into();
     facts.actual_count=self.program.buffers[view.binding].count.into();
    }
    InlineNeed::InvalidShape=>return self.fail(span,"B167","调用方没有声明函数需要的数据形状"),
    InlineNeed::Dataset=>{
     let Argument::Dataset(view)=(unsafe{argument.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     env.insert(unsafe{param.as_ref().unwrap_unchecked()}.name.clone(),Binding::Data(view));
    }
    InlineNeed::Atomic=>{
     let Argument::Atomic(place)=(unsafe{argument.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     self.require_same(unsafe{ty.as_ref().unwrap_unchecked()},place.ty(),span)?;env.insert(unsafe{param.as_ref().unwrap_unchecked()}.name.clone(),Binding::AtomicBorrow(place));
    }
    InlineNeed::ReferenceType=>{
     let Argument::Reference(owner)=(unsafe{argument.as_ref().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     self.require_same(unsafe{ty.as_ref().unwrap_unchecked()},owner.place.ty(),span)?;
    }
    need@(InlineNeed::ReferenceRead|InlineNeed::ReferenceWrite|InlineNeed::ReferenceInOut)=>{
     let Argument::Reference(mut owner)=(unsafe{argument.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     let param=unsafe{param.as_ref().unwrap_unchecked()};
     match need{
      InlineNeed::ReferenceRead=>{owner.place=owner.place.read_only();}
      InlineNeed::ReferenceWrite=>{
       borrowed.push((index,param.access,owner.place.clone()));self.output_scopes+=1;
       owner=OwnedBinding{initialization:Some((0..owner.place.ty().components.len()).map(|i|Position::Parameter(self.output_scopes,i)).collect()),place:owner.place,proof:bounds::Proof::default(),uniform:false};
      }
      InlineNeed::ReferenceInOut=>{borrowed.push((index,param.access,owner.place.clone()));}
      _=>unsafe{std::hint::unreachable_unchecked()},
     }
     env.insert(param.name.clone(),Binding::Owned(owner));
    }
    InlineNeed::TileType=>{
     let Argument::Tile(view)=(unsafe{argument.as_ref().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     self.require_same(unsafe{ty.as_ref().unwrap_unchecked()},&view.ty,span)?;facts.actual_access=self.tiles[view.tile].access as u32;
    }
    InlineNeed::InvalidTileAccess=>return self.fail(span,"B214","tile 实参与参数必须具有相同的只读／atomic 访问权限"),
    InlineNeed::Tile=>{
     let Argument::Tile(view)=(unsafe{argument.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     env.insert(unsafe{param.as_ref().unwrap_unchecked()}.name.clone(),Binding::Tile(view));
    }
    InlineNeed::Value=>{
     let Argument::Value(mut value)=(unsafe{argument.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     self.require_same(unsafe{ty.as_ref().unwrap_unchecked()},&value.ty,span)?;value.literal=false;
     env.insert(unsafe{param.as_ref().unwrap_unchecked()}.name.clone(),Binding::Value(value));
    }
    InlineNeed::InvalidArgument=>return self.fail(span,"B207","函数参数需要匹配的值、数据集、独占／原子位置或 tile 借用"),

    InlineNeed::Resolve=>{
     parameters.take();param.take();argument.take();ty.take();
     requirements=Some(unsafe{world.call(contracts::resolve(self,(&instance).into(),&LexicalScope::Function(instance.clone()),&env,&BTreeMap::new())).await}?);
    }
    InlineNeed::Prove=>{unsafe{world.call(contracts::prove(self,requirements.as_ref().unwrap_unchecked(),span)).await}?;}
    InlineNeed::Assume=>{unsafe{world.call(contracts::assume(self,(&instance).into(),&mut env,requirements.as_ref().unwrap_unchecked())).await}?;requirements.take();}
    InlineNeed::EnterEnvironment=>{
     old_env=Some(std::mem::replace(&mut self.env,std::mem::take(&mut env)));old_binding_uses=Some(std::mem::take(&mut self.binding_uses));
     callee_scope=Some(LexicalScope::Function(instance.clone()));
    }
    InlineNeed::AssignedNames=>{
     self.binding_uses=unsafe{world.call(collect_binding_uses(world,callee_scope.as_ref().unwrap_unchecked(),callee.function.body().as_ref().unwrap_unchecked())).await}?;
    }
    need@(InlineNeed::EnterContext|InlineNeed::EnterCooperativeContext)=>{
     let old_scope=self.scope.clone();let old_in_function=self.in_function;let old_cooperative=self.cooperative_root;let old_block=self.block_depth;let old_loop=self.loop_depth;let old_iterators=std::mem::take(&mut self.iterators);
     let mut inlined_at=self.source_context.inlined_at.to_vec();inlined_at.push(InlineSite{origin:self.source_definition.clone(),instance:self.source_context.instance.clone(),span});
     let source_context=Arc::new(SourceContext{instance:Some(Arc::from(key.as_str())),inlined_at:inlined_at.into_boxed_slice(),domain_path:self.source_context.domain_path.clone()});
     let old_source_context=std::mem::replace(&mut self.source_context,source_context);
     let old_source_definition=std::mem::replace(&mut self.source_definition,callee.source_origin().map(Arc::new));let old_source_site=self.source_site.take();
     old_context=Some((old_scope,old_in_function,old_cooperative,old_block,old_loop,old_iterators,old_source_context,old_source_definition,old_source_site));
     self.scope=unsafe{callee_scope.take().unwrap_unchecked()};self.in_function=true;self.block_depth=0;self.loop_depth=0;
     self.cooperative_root=matches!(need,InlineNeed::EnterCooperativeContext).then_some(0);self.calls.push(std::mem::take(&mut key));
    }
    InlineNeed::InitializationInputs=>{facts.input_plain=plain_environment(&self.env,false);}
    InlineNeed::ResultProperties=>{
     let layout=unsafe{world.call(self.storage_layout(&result_name,callee.function.span())).await}?;
     let storage=layout.storage();
     facts.result_borrows=u32::from(storage.contains_borrows());
     facts.result_owns=u32::from(storage.owns_resources());
     facts.result_ready=1;
     result_layout=Some(layout);
    }
    InlineNeed::ResultFacts=>{
     let storage=unsafe{result_layout.as_ref().unwrap_unchecked()}.storage();
     facts.result_facts=u32::from(bounds::Proof::fact_components(&storage).next().is_some());
    }
    InlineNeed::IsolateInitialization=>{caller_initialization=Some(std::mem::take(&mut self.initialized));}
    InlineNeed::Materialize=>{self.materialize_input_bindings();}
    InlineNeed::OutputInputs=>{output_inputs=Some(unsafe{world.call(outputs::current_inputs(self,&instance)).await}?);}
    InlineNeed::BoundsInputs=>{facts.input_plain=plain_environment(&self.env,true);}
    InlineNeed::EnterPath=>{caller_path=Some(std::mem::take(&mut self.path));}
    InlineNeed::Body=>{
     (result,_)=unsafe{world.call(self.function_body(&instance,&result_name,result_layout.take(),callee.function.body().as_ref().unwrap_unchecked())).await}?;
     facts.continuing=u32::from(self.flow.continues());facts.items=borrowed.len()as u64;facts.held=held.len()as u64;
    }
    InlineNeed::RestoreInitialization=>{self.initialized=unsafe{caller_initialization.take().unwrap_unchecked()};}
    InlineNeed::MarkResult=>{if let Some(Returned::Value(value))=&mut result{value.literal=false;}}
    InlineNeed::PopCall=>{self.calls.pop();}
    InlineNeed::RestorePath=>{self.path=unsafe{caller_path.take().unwrap_unchecked()};}
    InlineNeed::JoinPath=>{self.path=bounds::Clause::both(unsafe{caller_path.as_ref().unwrap_unchecked()},&self.path);caller_path.take();}
    InlineNeed::ReturnedStorage=>{returned=result.as_ref().map(|value|self.returned_storage(value));}
    InlineNeed::Update=>{
     let(parameter,_,place)=&borrowed[index];let owner=self.borrow_owned(place.clone());
     let guards=unsafe{world.call(outputs::guards(self,&instance,*parameter,place.ty(),output_inputs.as_ref().unwrap_unchecked(),returned.as_ref())).await}?;
     updates.push((owner,guards));
    }
    InlineNeed::RestoreEnvironment=>{self.env=unsafe{old_env.take().unwrap_unchecked()};}
    InlineNeed::InspectUpdate=>{
     facts.access=borrowed[index].1 as u32;facts.positions=updates[index].0.initialization.as_ref().map_or(0,|positions|positions.len())as u64;
    }
    InlineNeed::InspectPosition=>{
     let position=&unsafe{updates[index].0.initialization.as_ref().unwrap_unchecked()}[cursor.part as usize];facts.parameter_position=u32::from(matches!(position,Position::Parameter(..)));
    }
    InlineNeed::InsertScope=>{
     let Position::Parameter(scope,_)=&unsafe{updates[index].0.initialization.as_ref().unwrap_unchecked()}[cursor.part as usize]else{unsafe{std::hint::unreachable_unchecked()}};
     fresh_scopes.insert(*scope);
    }
    InlineNeed::FilterInitialization=>{self.initialized.retain(|position|keep_position(position,&fresh_scopes));}
    InlineNeed::OutputDeclared=>{facts.declared=u32::from(outputs::declared(&instance,borrowed[index].0));}
    InlineNeed::ForgetOutput=>{self.forget_output_initialization(&borrowed[index].2,span)?;}
    InlineNeed::InitializeOutput=>{self.initialize_output(&borrowed[index].2,&updates[index].1);}
    InlineNeed::BeginProofs=>{proofs=Some(std::mem::take(&mut updates).into_iter());}
    InlineNeed::AssignProof=>{
     let(owner,_)=unsafe{proofs.as_mut().unwrap_unchecked().next().unwrap_unchecked()};
     unsafe{world.call(self.assign_owned_proof(&owner.place,owner.proof,owner.uniform)).await}?;
    }
    InlineNeed::RestoreContext=>{
     self.binding_uses=unsafe{old_binding_uses.take().unwrap_unchecked()};
     let(scope,in_function,cooperative,block,loop_depth,iterators,source_context,source_definition,source_site)=unsafe{old_context.take().unwrap_unchecked()};
     self.scope=scope;self.source_definition=source_definition;self.source_context=source_context;self.source_site=source_site;self.in_function=in_function;self.cooperative_root=cooperative;self.block_depth=block;self.loop_depth=loop_depth;self.iterators=iterators;
    }
    InlineNeed::FinishScoped=>{unsafe{world.call(self.finish_scoped_argument(&held[index],span)).await}?;}
    InlineNeed::Complete=>return Ok(result),
    InlineNeed::Next=>{},
    InlineNeed::InvalidTarget|InlineNeed::Recursion|InlineNeed::Native|InlineNeed::InvalidRecursion|InlineNeed::ResultName=>unsafe{std::hint::unreachable_unchecked()},
   }
  }
 }
}

#[derive(Clone,Copy)]
#[repr(C)]struct DomainParameterCursor{index:u64,phase:u32,alias_conflict:u32}
#[repr(C)]struct DomainParameterFacts{
 parameters:u64,argument:u32,access:Access,actual_access:Access,dataset:u32,acceleration:u32,counted:u32,same:u32,present:u32,
 expected_count:Count,actual_count:Count,previous:Access,
}
#[repr(u32)]enum DomainParameterNeed{Parameter,Texture,InvalidTexture,InvalidAcceleration,DatasetType,InvalidDataset,Shape,InvalidShape,InvalidAccess,ValueType,InvalidValue,BindData,BindRoot,InvalidAlias,Complete}
#[repr(C)]struct DomainParameterStep{cursor:DomainParameterCursor,need:DomainParameterNeed}
unsafe extern "C"{#[link_name="bootstrap.call_execution::domain_parameter_step"]fn domain_parameter_step(cursor:DomainParameterCursor,facts:DomainParameterFacts)->DomainParameterStep;}

pub(super) async fn domain_parameters(
 world:&World,program:&Program,instance:&crate::generics::Instance,args:Expressions,roots:Vec<Root>,caller:&Source,span:Span,
)->Result<BTreeMap<String,Root>>{
 let callee=&instance.definition;
 let mut parameters=instance.parameters(world).zip(args).zip(roots);
 let mut cursor=DomainParameterCursor{index:0,phase:0,alias_conflict:0};
 let mut facts=DomainParameterFacts{parameters:callee.function.params().len()as u64,argument:0,access:Access::Value,actual_access:Access::Value,dataset:0,acceleration:0,counted:0,same:0,present:0,expected_count:Count::default(),actual_count:Count::default(),previous:Access::Value};
 let mut parameter:Option<Param>=None;let mut argument:Option<Expr>=None;let mut root:Option<Root>=None;let mut binding=None;
 let mut child=BTreeMap::new();let mut used=BTreeMap::new();
 loop{let step=unsafe{domain_parameter_step(cursor,DomainParameterFacts{..facts})};cursor=step.cursor;match step.need{
  DomainParameterNeed::Parameter=>{
   let((pending,source),value)=parameters.next().unwrap();let p=unsafe{world.call(pending).await}?;
   facts.access=p.access;facts.dataset=u32::from(p.ty.dataset);facts.acceleration=u32::from(p.ty.name=="acceleration");facts.counted=u32::from(p.ty.count.is_some());
   facts.argument=match &value{
    Root::Data(index,access)=>{binding=Some(*index);facts.actual_access=*access;facts.actual_count=program.buffers[*index].count.into();4},
    Root::Acceleration(_)=>1,Root::Texture(index)=>{binding=Some(*index);2},Root::Uniform(_)|Root::Literal(..)=>3,
   };
   parameter=Some(p);argument=Some(source);root=Some(value);
  },
  DomainParameterNeed::Texture=>{
   let p=parameter.as_ref().unwrap();let resource=&program.textures[binding.unwrap()];
   facts.present=u32::from(unsafe{world.call(texture::compatible(world,p,resource,resource.count,(&callee.source,&callee.module,&callee.imports))).await}?);
  },
  DomainParameterNeed::InvalidTexture=>return caller.fail(argument.as_ref().unwrap().span(),"B239","纹理资源与只读纹理参数的维度／形状／格式不匹配"),
  DomainParameterNeed::InvalidAcceleration=>return caller.fail(argument.as_ref().unwrap().span(),"B182","加速资源的类型或访问权限不匹配"),
  need@(DomainParameterNeed::DatasetType|DomainParameterNeed::ValueType)=>{
   let p=parameter.as_ref().unwrap();let ty=unsafe{world.call(world.ty(&p.ty,&callee.source,p.span,&callee.module,&callee.imports)).await}?;
   facts.same=u32::from(match need{
    DomainParameterNeed::DatasetType=>ty==program.buffers[binding.unwrap()].ty,
    _=>match root.as_ref().unwrap(){Root::Uniform(index)=>ty==program.uniforms[*index].ty,Root::Literal(actual,..)=>ty==*actual,_=>false},
   });
  },
  DomainParameterNeed::InvalidDataset=>return caller.fail(argument.as_ref().unwrap().span(),"B115","数据集类型不匹配"),
  DomainParameterNeed::Shape=>{
   let p=parameter.as_ref().unwrap();facts.expected_count=unsafe{world.call(world.shape(&p.ty.count,&callee.source,p.span,&callee.module,&callee.imports)).await}?.into();
  },
  DomainParameterNeed::InvalidShape=>return caller.fail(argument.as_ref().unwrap().span(),"B167","调用方没有声明子程序需要的数据形状"),
  DomainParameterNeed::InvalidAccess=>return caller.fail(argument.as_ref().unwrap().span(),"B116","子程序请求了调用方没有的数据访问权限"),
  DomainParameterNeed::InvalidValue=>return caller.fail(argument.as_ref().unwrap().span(),"B117","标量参数类型不匹配"),
  DomainParameterNeed::BindData=>{
   let p=parameter.take().unwrap();
   let index=binding.unwrap();
   let previous=used.insert(index,p.access);
   facts.present=u32::from(previous.is_some());
   facts.previous=previous.unwrap_or(Access::Value);
   child.insert(p.name,Root::Data(index,p.access));
  },
  DomainParameterNeed::BindRoot=>{let p=parameter.take().unwrap();child.insert(p.name,root.take().unwrap());},
  DomainParameterNeed::InvalidAlias=>return caller.fail(span,"B118","同一数据集通过多个参数别名进入可写子程序"),
  DomainParameterNeed::Complete=>return Ok(child),
 }}
}

#[derive(Clone,Copy)]
#[repr(C)]struct DomainCallCursor{index:u64,phase:u32}
#[repr(C)]struct DomainCallFacts{arguments:u64,parameters:u64,domain:u32,target:u32,callee_target:u32}
#[repr(u32)]enum DomainCallNeed{InvalidDomain,InvalidCount,InvalidTarget,Argument,Specialize,Bind}
#[repr(C)]struct DomainCallStep{cursor:DomainCallCursor,need:DomainCallNeed}
#[repr(u32)]enum DomainInstanceNeed{Actuals,Explicit,Specialize,Complete}
unsafe extern "C"{
 #[link_name="bootstrap.call_execution::domain_call_step"]fn domain_call_step(state:DomainCallCursor,facts:DomainCallFacts)->DomainCallStep;
 #[link_name="bootstrap.call_execution::domain_instance_step"]fn domain_instance_step(cursor:*mut Phase,explicit:u32)->DomainInstanceNeed;
 #[link_name="bootstrap.call_execution::domain_actuals"]fn domain_actuals(count:u64,context:*mut c_void,append:unsafe extern "C" fn(*mut c_void,u64));
}
struct DomainActuals<'a>{program:&'a Program,roots:&'a[Root],values:Vec<generics::Actual<'a>>}
unsafe extern "C" fn append_domain_actual(context:*mut c_void,index:u64){
 let context=unsafe{&mut *context.cast::<DomainActuals<'_>>()};let program=context.program;let roots=context.roots;
 let actual=match &roots[index as usize]{
  Root::Data(index,_)=>generics::Actual{ty:Some(std::borrow::Cow::Borrowed(&program.buffers[*index].ty)),count:program.buffers[*index].count},
  Root::Uniform(index)=>generics::Actual::value(&program.uniforms[*index].ty),
  Root::Literal(ty,..)=>generics::Actual::value(ty),
  Root::Acceleration(_)=>generics::Actual{ty:None,count:None},
  Root::Texture(index)=>generics::Actual{ty:None,count:program.textures[*index].count},
 };
 context.values.push(actual);
}
async fn domain_instance(
 world:&World,caller:&crate::generics::Instance,program:&Program,key:&str,roots:&[Root],
 explicit:Option<&crate::syntax::List<crate::syntax::wire::ArgumentNode>>,span:Span,
)->Result<crate::generics::Instance>{
 let d=&caller.definition;let mut cursor=Phase{phase:0};let mut actuals=DomainActuals{program,roots,values:Vec::with_capacity(roots.len())};let mut arguments=None;let mut instance=None;
 loop{match unsafe{domain_instance_step(&mut cursor,u32::from(explicit.is_some()))}{
  DomainInstanceNeed::Actuals=>unsafe{domain_actuals(roots.len()as u64,std::ptr::from_mut(&mut actuals).cast(),append_domain_actual)},
  DomainInstanceNeed::Explicit=>arguments=Some(unsafe{world.call(caller.bindings.arguments(world,explicit.unwrap(),(&d.source,&d.module,&d.imports),span)).await}?),
  DomainInstanceNeed::Specialize=>instance=Some(unsafe{world.call(generics::specialize(world,key,&actuals.values,arguments.as_deref(),(&d.source,&d.module,&d.imports),span)).await}?),
  DomainInstanceNeed::Complete=>return Ok(instance.take().unwrap()),
 }}
}
pub(super) async fn domain_call(
 world:&World,caller:&crate::generics::Instance,env:&BTreeMap<String,Root>,program:&Program,key:&str,args:Expressions,
 explicit:Option<&crate::syntax::List<crate::syntax::wire::ArgumentNode>>,span:Span,path:&Arc<[DomainSite]>,
)->Result<(crate::generics::Instance,BTreeMap<String,Root>)>{
 let d=&caller.definition;let callee=&world.functions[key];let mut cursor=DomainCallCursor{index:0,phase:0};
 let facts=DomainCallFacts{arguments:args.len()as u64,parameters:callee.function.params().len()as u64,domain:u32::from(callee.execution==crate::execution::Requirement::Domain),target:target(program.target),callee_target:target(callee.function.target())};
 let mut args=Some(args);let mut roots=Vec::new();let mut instance=None;
 loop{let step=unsafe{domain_call_step(cursor,DomainCallFacts{..facts})};cursor=step.cursor;match step.need{
  DomainCallNeed::InvalidDomain=>return d.source.fail(span,"B110","program 顶层只能调用计算程序或声明 parallel 阶段"),
  DomainCallNeed::InvalidCount=>return d.source.fail(span,"B111","计算程序参数数量不匹配"),
  DomainCallNeed::InvalidTarget=>return d.source.fail(span,"B112","程序与子程序的设备目标不兼容"),
  DomainCallNeed::Argument=>{let arg=args.as_ref().unwrap().at(cursor.index as usize);roots.push(unsafe{world.call(stage_root(world,caller,env,program,&arg,path)).await}?);},
  DomainCallNeed::Specialize=>instance=Some(unsafe{world.call(domain_instance(world,caller,program,key,&roots,explicit,span)).await}?),
  DomainCallNeed::Bind=>{
   let instance=instance.take().unwrap();let child=unsafe{world.call(domain_parameters(world,program,&instance,args.take().unwrap(),roots,&d.source,span)).await}?;return Ok((instance,child));
  },
 }}
}

#[derive(Clone,Copy)]
#[repr(C)]struct DomainExecutionCursor{index:u64,phase:u32}
#[repr(C)]struct DomainExecutionFacts{items:u64,compiled:u32,present:u32,valid:u32,stage:u64,segment_start:u64,domain:u64,count:u64}
#[repr(u32)]enum DomainExecutionNeed{Contracts,Lengths,LengthPair,InvalidLength,Requirements,Elements,ProveElements,Prefix,Source,Compiled,Argument,Segment,SegmentFacts,Writes,Write,Outputs,Output,OutputDomain,AppendOutput,Publish,Guarantees,Guarantee,InvalidPrefix,Enter,Expand,Proved,Leave,Grants,Grant,Complete}
#[repr(C)]struct DomainExecutionStep{cursor:DomainExecutionCursor,need:DomainExecutionNeed}
#[repr(C)]struct DomainWriteFacts{regions:u64,output:u32}
#[repr(u32)]enum DomainWriteNeed{Proof,Region,Complete}
#[repr(C)]struct DomainWriteStep{cursor:DomainExecutionCursor,need:DomainWriteNeed}
unsafe extern "C"{
 #[link_name="bootstrap.call_execution::domain_execution_step"]fn domain_execution_step(state:DomainExecutionCursor,facts:DomainExecutionFacts)->DomainExecutionStep;
 #[link_name="bootstrap.call_execution::domain_write_step"]fn domain_write_step(state:DomainExecutionCursor,facts:DomainWriteFacts)->DomainWriteStep;
}
fn domain_write(
 program:&Program,facts:&mut prefix::Facts,bindings:&[usize],elements:&BTreeMap<usize,bounds::Proof>,logical:usize,written:&BTreeSet<(u32,u32)>,
)->usize{
 let binding=bindings[logical];let proof=elements.get(&logical);let mut output=None;let mut regions=written.iter();
 let mut cursor=DomainExecutionCursor{index:0,phase:0};let control=DomainWriteFacts{regions:written.len()as u64,output:u32::from(proof.is_some())};
 loop{let step=unsafe{domain_write_step(cursor,DomainWriteFacts{..control})};cursor=step.cursor;match step.need{
  DomainWriteNeed::Proof=>output=Some(proof.unwrap().element_facts(|target|bounds::Proof::default().with_domains(Domains::from([IndexDomain::Data(bindings[target])])))),
  DomainWriteNeed::Region=>{let &(start,end)=regions.next().unwrap();facts.written(program,binding,start,end,output.as_ref());},
  DomainWriteNeed::Complete=>return binding,
 }}
}
pub(super) async fn execute_domain_call(
 world:&World,caller:&crate::generics::Instance,instance:&crate::generics::Instance,mut child:BTreeMap<String,Root>,
 program:&mut Program,stack:&mut Vec<String>,boundary:&mut bool,facts:&mut prefix::Facts,calls:&mut Vec<DomainCall>,
 path:&Arc<[DomainSite]>,functions:&mut native_function::Module,scopes:&mut BTreeMap<usize,RangeContext>,span:Span,call_span:Span,
)->Result<()>{
 let d=&caller.definition;let callee=&instance.definition;
 let mut cursor=DomainExecutionCursor{index:0,phase:0};
 let mut control=DomainExecutionFacts{items:0,compiled:u32::from(callee.compiled.is_some()),present:0,valid:0,stage:0,segment_start:0,domain:0,count:0};
 let mut contracts=None;let mut lengths=None;let mut required=None;let mut elements=None;
 let mut context=None;let mut child_path=None;let mut contract:Option<crate::module::domain::Interface>=None;
 let mut parameters=None;let mut bindings=Vec::new();
 let mut stage=0;let mut segment_start=0;let mut prefix_guarantees=Vec::new();let mut element_guarantees=Vec::new();
 let mut writes=None;let mut write_regions=BTreeMap::new();let mut outputs=None;let mut output:Option<crate::module::domain::Output>=None;
 let mut arguments=Vec::new();let mut guarantees:Option<Vec<PrefixRelation>>=None;let mut grants=None;
 loop{let step=unsafe{domain_execution_step(cursor,DomainExecutionFacts{..control})};cursor=step.cursor;match step.need{
  DomainExecutionNeed::Contracts=>contracts=Some(instance.contracts().collect::<Vec<_>>()),
  DomainExecutionNeed::Lengths=>{let values=contracts::root_lengths(&child,callee,contracts.as_ref().unwrap())?;control.items=values.len()as u64;lengths=Some(values.into_iter());},
  DomainExecutionNeed::LengthPair=>{let(left,right)=lengths.as_mut().unwrap().next().unwrap();control.valid=u32::from(program.same_length(left,right));},
  DomainExecutionNeed::InvalidLength=>return d.source.fail(span,"B226","整域调用前未证明same_length数据集长度关系"),
  DomainExecutionNeed::Requirements=>{lengths.take();let values=prefix::requirements(program,&child,callee,contracts.as_ref().unwrap())?;control.items=values.len()as u64;required=Some(values);},
  DomainExecutionNeed::Elements=>elements=Some(unsafe{world.call(prefix::element_requirements(world,&program.buffers,&child,instance,contracts.as_ref().unwrap())).await}?),
  DomainExecutionNeed::ProveElements=>{facts.prove_elements(program,elements.as_ref().unwrap(),&d.source,span)?;elements.take();},
  DomainExecutionNeed::Prefix=>{let relation=&required.as_ref().unwrap()[cursor.index as usize];prefix::prove(facts,program,relation.domain,relation.count,&d.source,span)?;},
  DomainExecutionNeed::Source=>{
   let(origin,source_context)=d.syntax_context(Some(&caller.name));let mut next=path.to_vec();
   next.push(DomainSite{origin:origin.clone(),instance:source_context.instance.clone(),span:call_span,target:Arc::from(instance.name.as_str()),transition:if control.compiled!=0{DomainTransition::Compiled}else{DomainTransition::Expanded}});
   child_path=Some(Arc::<[DomainSite]>::from(next));context=Some((origin,source_context));
  },
  DomainExecutionNeed::Compiled=>{
   contracts.take();required.take();
   use crate::module::codec::Decode;
   let target=if callee.function.target()==Target::Gpu{Target::Gpu}else{Target::Cpu};let symbol=crate::module::domain::symbol(&instance.name,target);
   let bytes=callee.compiled.as_ref().unwrap().get(&symbol).ok_or_else(||Diagnostic{source:d.source.clone(),span,code:"B270",message:format!("缺少已编译整域入口合同 {symbol}")})?;
   contract=Some(native_function::Contract::from_bytes(bytes).map_err(|error|Diagnostic{source:d.source.clone(),span,code:"B270",message:error.to_string()})?.domain.ok_or_else(||Diagnostic{source:d.source.clone(),span,code:"B270",message:format!("入口 {symbol} 不是整域合同")})?);
   let declared=callee.function.params();
   control.items=declared.len()as u64;
   arguments=Vec::with_capacity(declared.len());
   parameters=Some(declared.into_iter());
  },
  DomainExecutionNeed::Argument=>{
   let parameter=parameters.as_mut().unwrap().next().unwrap();
   let argument=child.remove(parameter.name()).unwrap();
   if let Root::Data(index,_) = &argument {
    bindings.push(*index);
   }
   arguments.push(argument);
  },
  DomainExecutionNeed::Segment=>{parameters.take();stage=program.stages.len();segment_start=calls.last().map_or(0,|call|call.stage);control.stage=stage as u64;control.segment_start=segment_start as u64;},
  DomainExecutionNeed::SegmentFacts=>{
   write_regions=Program::stage_write_regions(&program.stages[segment_start..stage]);
   prefix_guarantees=facts.relations();
   element_guarantees=facts.element_facts(program,Some(&write_regions));
  },
  DomainExecutionNeed::Writes=>{let values=std::mem::take(&mut contract.as_mut().unwrap().write_regions);control.items=values.len()as u64;writes=Some(values.into_iter());},
  DomainExecutionNeed::Write=>{
   let(logical,written)=writes.as_mut().unwrap().next().unwrap();let binding=domain_write(program,facts,&bindings,&contract.as_ref().unwrap().elements,logical,&written);
   write_regions.entry(binding).or_insert_with(BTreeSet::new).extend(written);
  },
  DomainExecutionNeed::Outputs=>{writes.take();let values=std::mem::take(&mut contract.as_mut().unwrap().outputs);control.items=values.len()as u64;outputs=Some(values.into_iter());},
  DomainExecutionNeed::Output=>{let mut value=outputs.as_mut().unwrap().next().unwrap();value.binding=bindings[value.binding];control.present=u32::from(value.domain.is_some());output=Some(value);},
  DomainExecutionNeed::OutputDomain=>{let value=output.as_mut().unwrap();value.domain=Some(bindings[value.domain.unwrap()]);},
  DomainExecutionNeed::AppendOutput=>facts.outputs.push(output.take().unwrap()),
  DomainExecutionNeed::Publish=>{
   outputs.take();
   let(origin,source_context)=context.take().unwrap();let child_path=child_path.take().unwrap();
   calls.push(DomainCall{stage,instance:instance.clone(),arguments:std::mem::take(&mut arguments),write_regions:std::mem::take(&mut write_regions),element_guarantees:std::mem::take(&mut element_guarantees),element_requirements:Vec::new(),prefix_guarantees:std::mem::take(&mut prefix_guarantees),prefix_requirements:Vec::new(),source:origin.map(|origin|Arc::new(SourceSite{origin,context:Arc::new(SourceContext{domain_path:child_path,..(*source_context).clone()}),span:call_span,syntax:SourceSyntax::Expression}))});
   *boundary=true;
  },
  DomainExecutionNeed::Guarantees=>{let values=contract.take().unwrap().prefix_guarantees;control.items=values.len()as u64;guarantees=Some(values);},
  DomainExecutionNeed::Guarantee=>{let relation=&mut guarantees.as_mut().unwrap()[cursor.index as usize];relation.domain=bindings[relation.domain];relation.count.binding=bindings[relation.count.binding];control.domain=relation.domain as u64;control.count=relation.count.binding as u64;},
  DomainExecutionNeed::InvalidPrefix=>return d.source.fail(call_span,"B192","前缀域约束需要独立形状 1 资源中的 u32 计数字段"),
  DomainExecutionNeed::Enter=>{let requirements=required.take().unwrap();facts.enter(&requirements);context.take();},
  DomainExecutionNeed::Expand=>{unsafe{world.call(expand_program(world,instance,&mut child,program,stack,boundary,facts,calls,child_path.take().unwrap(),functions,scopes)).await}?;},
  DomainExecutionNeed::Proved=>{guarantees=Some(prefix::proved_guarantees(program,callee,facts,prefix::bindings(program,&child,callee,contracts.as_ref().unwrap(),ContractKind::Ensures))?);contracts.take();},
  DomainExecutionNeed::Leave=>facts.leave(),
  DomainExecutionNeed::Grants=>{let values=guarantees.take().unwrap();control.items=values.len()as u64;grants=Some(values.into_iter());},
  DomainExecutionNeed::Grant=>facts.grant(grants.as_mut().unwrap().next().unwrap()),
  DomainExecutionNeed::Complete=>return Ok(()),
 }}
}
