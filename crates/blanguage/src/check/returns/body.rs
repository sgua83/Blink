//! Function body control borrows original return, model and IR owners.
use super::*;
use std::ffi::c_void;
#[derive(Clone,Copy)]
#[repr(C)] struct Cursor{phase:u32,flags:u32,index:u64,part:u64}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct BeginFacts{void_name:u32,dataset:u32,tile:u32,view:u32,never:u32,owns:u32,value_function:u32,cpu:u32,cooperative:u32,early:u32,scalars:u64}
#[repr(u32)] enum BeginNeed{Name,Void,Never,Type,InvalidOwned,Shape,Dataset,Value,Early,InvalidCooperative,Slot,Initial,Inputs,Context,Complete,Next}
#[repr(C)] struct BeginStep{cursor:Cursor,need:BeginNeed}
#[repr(u32)] enum SequenceNeed{StartNext,StartStop,Statement,After,Complete,Next}
#[repr(C)] struct SequenceStep{cursor:Cursor,need:SequenceNeed}
#[repr(u32)] enum FinishNeed{InvalidNever,MissingEmpty,MissingReturn,ImplicitReturn,Context,Code,OtherExits,PopExit,Scope,Append,Initial,First,NoExit,FirstModel,Slot,Registers,EmptyRegisters,Written,BindFirst,NextExit,BindOther,MergePath,JoinResult,MergeBorrowed,RebindWritten,Snapshot,OutputStorage,NextModel,Dataset,Parameter,Place,Guards,DirectGuards,Ownership,Guard,Initialization,Value,Owned,InvalidModel,Insert,RebindResult,Publish,Complete,Next}
#[repr(C)] struct FinishStep{cursor:Cursor,need:FinishNeed}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct FinishFacts{expected:u32,continuing:u32,empty:u32,scoped:u32,last_exit:u32,other_exits:u32,present:u32,result:u32,other_result:u32,return_consumer:u32,model:u32,declared:u32,owns:u32,top:u32,cpu:u32,active:u32,slots:u64,scalars:u64,guards:u64}
#[derive(Clone,Copy)]
#[repr(C)] struct CodeSpan{data:*const Inst,count:u64}
impl CodeSpan{fn of(code:&[Inst])->Self{Self{data:code.as_ptr(),count:code.len()as u64}}fn empty()->Self{Self{data:std::ptr::null(),count:0}}}
#[repr(C)] struct ExitNode{kind:u32,first:CodeSpan,second:CodeSpan}
#[repr(C)] struct ExitHost{inspect:unsafe extern "C" fn(*const Inst,u64)->ExitNode}
#[repr(C)] struct WrittenItem{present:u32,kind:u32,fields:u64}
#[repr(C)] struct WrittenHost{next:unsafe extern "C" fn(*mut c_void)->WrittenItem,allocate:unsafe extern "C" fn(*mut c_void,u64),finish:unsafe extern "C" fn(*mut c_void)}
#[repr(C)] struct ModelsHost{next:unsafe extern "C" fn(*mut c_void)->u32,visit:unsafe extern "C" fn(*mut c_void)}
unsafe extern "C"{
 #[link_name="bootstrap.function_body::start"]fn b_start()->Cursor;
 #[link_name="bootstrap.function_body::begin_step"]fn b_begin_step(state:Cursor,facts:BeginFacts)->BeginStep;
 #[link_name="bootstrap.function_body::sequence_step"]fn b_sequence_step(state:Cursor,count:u64,possible:u32,continuing:u32)->SequenceStep;
 #[link_name="bootstrap.function_body::finish_step"]fn b_finish_step(state:Cursor,facts:FinishFacts)->FinishStep;
 #[link_name="bootstrap.function_body::owns_exit"]fn b_owns_exit(code:CodeSpan,host:ExitHost)->u32;
 #[link_name="bootstrap.function_body::allocate_written"]fn b_allocate_written(context:*mut c_void,host:WrittenHost)->u32;
 #[link_name="bootstrap.function_body::models"]fn b_models(context:*mut c_void,host:ModelsHost);
 #[link_name="bootstrap.function_body::snapshot"]fn b_snapshot(count:u64,context:*mut c_void,append:unsafe extern "C" fn(*mut c_void,u64));
 #[link_name="bootstrap.function_body::parameter"]fn b_parameter(count:u64,context:*mut c_void,equal:unsafe extern "C" fn(*mut c_void,u64)->u32)->u64;
}
impl Expected{
 fn scalar_count(&self)->usize{match self{Self::Void|Self::Never=>0,Self::Value(ty)=>ty.components.len(),Self::Dataset{..}=>2}}
 fn scalar(&self,index:usize)->Scalar{match self{Self::Value(ty)=>ty.components[index].0,Self::Dataset{..}=>Scalar::U32,_=>unsafe{std::hint::unreachable_unchecked()}}}
}
fn model_fields(value:&Returned)->usize{match value{Returned::Unit=>0,Returned::Value(value)=>value.ty.components.len(),Returned::OwnedValue(value)=>value.value.ty.components.len(),Returned::Dataset(_)=>2}}
fn model_scalar(value:&Returned,index:usize)->Scalar{match value{Returned::Value(value)=>value.ty.components[index].0,Returned::OwnedValue(value)=>value.value.ty.components[index].0,Returned::Dataset(data)=>if index==0{data.root.ty.components[0].0}else{Scalar::U32},Returned::Unit=>unsafe{std::hint::unreachable_unchecked()}}}
unsafe extern "C" fn inspect_exit(code:*const Inst,index:u64)->ExitNode{
 let value=unsafe{&*code.add(index as usize)};
 match &value.kind{
  InstKind::Exit=>ExitNode{kind:1,first:CodeSpan::empty(),second:CodeSpan::empty()},
  InstKind::If{yes,no,..}=>ExitNode{kind:2,first:CodeSpan::of(yes),second:CodeSpan::of(no)},
  InstKind::Loop{condition,body,..}=>ExitNode{kind:2,first:CodeSpan::of(condition),second:CodeSpan::of(body)},
  _=>ExitNode{kind:0,first:CodeSpan::empty(),second:CodeSpan::empty()},
 }
}
struct Written<'a,'b,'world>{builder:&'b mut Builder<'world>,entries:std::collections::btree_map::Iter<'a,String,Returned>,current:Option<(&'a String,&'a Returned)>,registers:Vec<Reg>,output:BTreeMap<String,Vec<Reg>>}
unsafe extern "C" fn next_written(context:*mut c_void)->WrittenItem{
 let c=unsafe{&mut *context.cast::<Written<'_,'_,'_>>()};c.current=c.entries.next();
 c.current.map_or(WrittenItem{present:0,kind:0,fields:0},|(_,value)|WrittenItem{present:1,kind:value.kind(),fields:model_fields(value)as u64})
}
unsafe extern "C" fn allocate_written(context:*mut c_void,index:u64){
 let c=unsafe{&mut *context.cast::<Written<'_,'_,'_>>()};let scalar=model_scalar(unsafe{c.current.unwrap_unchecked()}.1,index as usize);c.registers.push(c.builder.reg(scalar));
}
unsafe extern "C" fn finish_written(context:*mut c_void){
 let c=unsafe{&mut *context.cast::<Written<'_,'_,'_>>()};let(name,_)=unsafe{c.current.take().unwrap_unchecked()};c.output.insert(name.clone(),std::mem::take(&mut c.registers));
}
fn written(builder:&mut Builder<'_>,models:&BTreeMap<String,Returned>)->(BTreeMap<String,Vec<Reg>>,u32){
 let mut c=Written{builder,entries:models.iter(),current:None,registers:Vec::new(),output:BTreeMap::new()};
 let consumer=unsafe{b_allocate_written(std::ptr::from_mut(&mut c).cast(),WrittenHost{next:next_written,allocate:allocate_written,finish:finish_written})};(c.output,consumer)
}
struct Models<'a,F>{entries:std::collections::btree_map::IterMut<'a,String,Returned>,current:Option<(&'a String,&'a mut Returned)>,visit:F}
unsafe extern "C" fn next_model<F>(context:*mut c_void)->u32{
 let c=unsafe{&mut *context.cast::<Models<'_,F>>()};c.current=c.entries.next();u32::from(c.current.is_some())
}
unsafe extern "C" fn visit_model<F:FnMut(&str,&mut Returned)>(context:*mut c_void){
 let c=unsafe{&mut *context.cast::<Models<'_,F>>()};let(name,value)=unsafe{c.current.take().unwrap_unchecked()};(c.visit)(name,value);
}
fn models<F:FnMut(&str,&mut Returned)>(values:&mut BTreeMap<String,Returned>,visit:F){
 let mut c=Models{entries:values.iter_mut(),current:None,visit};unsafe{b_models(std::ptr::from_mut(&mut c).cast(),ModelsHost{next:next_model::<F>,visit:visit_model::<F>})}
}
struct Snapshot<'a>{definition:&'a Definition,models:&'a BTreeMap<String,Returned>,output:Vec<Option<Returned>>}
unsafe extern "C" fn append_snapshot(context:*mut c_void,index:u64){
 let c=unsafe{&mut *context.cast::<Snapshot<'_>>()};c.output.push(c.models.get(c.definition.function.params().at(index as usize).name()).cloned());
}
fn snapshot(definition:&Definition,models:&BTreeMap<String,Returned>)->Vec<Option<Returned>>{
 let mut c=Snapshot{definition,models,output:Vec::new()};unsafe{b_snapshot(definition.function.params().len()as u64,std::ptr::from_mut(&mut c).cast(),append_snapshot)};c.output
}
struct Parameter<'a>{definition:&'a Definition,name:&'a str}
unsafe extern "C" fn equal_parameter(context:*mut c_void,index:u64)->u32{
 let c=unsafe{&*context.cast::<Parameter<'_>>()};u32::from(c.definition.function.params().at(index as usize).name()==c.name)
}
fn parameter(definition:&Definition,name:&str)->usize{
 let mut c=Parameter{definition,name};unsafe{b_parameter(definition.function.params().len()as u64,std::ptr::from_mut(&mut c).cast(),equal_parameter)as usize}
}

impl Builder<'_>{
 pub(in crate::check) async fn function_body(&mut self,instance:&crate::generics::Instance,body:&Block)->Result<(Option<Returned>,Vec<Option<Returned>>)>{
  let world=self.world;let state=unsafe{world.call(self.begin_function_body(instance,body)).await}?;
  let possible=u32::from(self.residency_path().possible());let mut sequence=Flow::STOP;let mut cursor=unsafe{b_start()};let mut statements=body.into_iter();let mut statement=None;
  loop{
   let step=unsafe{b_sequence_step(cursor,body.len()as u64,possible,u32::from(sequence.continues()))};cursor=step.cursor;
   match step.need{
    SequenceNeed::StartNext=>{sequence=Flow::NEXT;}
    SequenceNeed::StartStop=>{sequence=Flow::STOP;}
    SequenceNeed::Statement=>{
     let value=unsafe{statements.next().unwrap_unchecked()};self.flow=Flow::NEXT;unsafe{world.call(self.statement(&value)).await}?;statement=Some(value);
    }
    SequenceNeed::After=>{let value=unsafe{statement.take().unwrap_unchecked()};self.view_uses.after(value.span());sequence=sequence.then(self.flow);}
    SequenceNeed::Complete=>{self.flow=sequence;return unsafe{world.call(self.finish_function_body(body,state)).await};}
    SequenceNeed::Next=>{},
   }
  }
 }
 #[inline(never)]
 async fn begin_function_body(&mut self,instance:&crate::generics::Instance,body:&Block)->Result<Box<BodyState>>{
  let world=self.world;let entry=self.registers.len();let definition=&instance.definition;let mut cursor=unsafe{b_start()};
  let mut facts=BeginFacts{value_function:u32::from(definition.function.kind()==FunctionKind::Value),cpu:u32::from(self.program.target==Target::Cpu),cooperative:u32::from(definition.function.kind()==FunctionKind::Cooperative),..BeginFacts::default()};
  let mut name=None;let mut ty=None;let mut expected=None;let mut count=None;let mut slots=Vec::new();let mut initial=None;let mut inputs=None;let mut state=None;
  loop{
   let step=unsafe{b_begin_step(cursor,facts)};cursor=step.cursor;
   match step.need{
    BeginNeed::Name=>{
     let value=unsafe{world.call(instance.bindings.type_name(world,&definition.function.result(),(&definition.source,&definition.module,&definition.imports),definition.function.span())).await}?;
     facts.void_name=u32::from(value.name=="void");facts.dataset=u32::from(value.dataset);facts.tile=u32::from(value.tile);facts.view=u32::from(value.view);facts.never=u32::from(value.is_never());name=Some(value);
    }
    BeginNeed::Void=>{expected=Some(Expected::Void);facts.scalars=0;}
    BeginNeed::Never=>{expected=Some(Expected::Never);facts.scalars=0;}
    BeginNeed::Type=>{
     let value=unsafe{world.call(world.ty(name.as_ref().unwrap_unchecked(),&definition.source,definition.function.span(),&definition.module,&definition.imports)).await}?;
     facts.owns=u32::from(value.owns_resources());ty=Some(value);
    }
    BeginNeed::InvalidOwned=>return self.fail(definition.function.span(),"B224","拥有结果由普通CPU函数返回；并行元素与通用拥有聚合尚未实现"),
    BeginNeed::Shape=>{
     count=unsafe{world.call(world.shape(&name.as_ref().unwrap_unchecked().count,&definition.source,definition.function.span(),&definition.module,&definition.imports)).await}?;
    }
    BeginNeed::Dataset=>{expected=Some(Expected::Dataset{ty:unsafe{ty.take().unwrap_unchecked()},count});facts.scalars=2;}
    BeginNeed::Value=>{let value=unsafe{ty.take().unwrap_unchecked()};facts.scalars=value.components.len()as u64;expected=Some(Expected::Value(value));}
    BeginNeed::Early=>{facts.early=u32::from(early_returns(body));}
    BeginNeed::InvalidCooperative=>return self.fail(definition.function.span(),"B208","cooperative 提前返回的整块收敛语义尚未实现；协作函数保持末尾返回"),
    BeginNeed::Slot=>{let scalar=unsafe{expected.as_ref().unwrap_unchecked()}.scalar(cursor.index as usize);slots.push(self.local_slots(&Ty::scalar(scalar)).0[0]);}
    BeginNeed::Initial=>{initial=Some(self.application_initialization_state());}
    BeginNeed::Inputs=>{inputs=Some(unsafe{world.call(outputs::entry_inputs(self,instance)).await}?);}
    BeginNeed::Context=>{
     let caller=self.return_context.replace(ReturnContext{instance:instance.clone(),expected:Arc::new(unsafe{expected.take().unwrap_unchecked()}),scoped:facts.early!=0,slots:std::mem::take(&mut slots),exits:Vec::new(),inputs:unsafe{inputs.take().unwrap_unchecked()}});
     let flow=std::mem::replace(&mut self.flow,Flow::NEXT);let uniform=self.control_uniform;let code_start=self.code.len();
     state=Some(Box::new(BodyState{entry,scoped:facts.early!=0,initial:unsafe{initial.take().unwrap_unchecked()},caller,flow,uniform,code_start}));
    }
    BeginNeed::Complete=>return Ok(unsafe{state.take().unwrap_unchecked()}),
    BeginNeed::Next=>{},
   }
  }
 }

 #[inline(never)]
 async fn finish_function_body(&mut self,body:&Block,state:Box<BodyState>)->Result<(Option<Returned>,Vec<Option<Returned>>)>{
  let world=self.world;let BodyState{entry,scoped,initial,mut caller,flow,uniform,code_start}=*state;
  let instance=self.return_context.as_ref().unwrap().instance.clone();let definition=&instance.definition;
  let mut facts=FinishFacts{expected:self.return_context.as_ref().unwrap().expected.kind(),continuing:u32::from(self.flow.continues()),empty:u32::from(body.is_empty()),scoped:u32::from(scoped),cpu:u32::from(self.program.target==Target::Cpu),..FinishFacts::default()};
  let mut cursor=unsafe{b_start()};let mut initial=Some(initial);let mut context=None;let mut code=None;let mut exits=None;let mut exit=None;
  let mut path=None;let mut result=None;let mut borrowed=BTreeMap::new();let mut initialized=Vec::new();let mut registers=Vec::new();let mut register_map=BTreeMap::new();
  let mut other_path=None;let mut other_value=None;let mut other_borrowed=BTreeMap::new();let mut other_initialized=Vec::new();let mut choice=None;
  let mut output_models=Vec::new();let mut returned=None;let mut published=None;let mut name=None;let mut model=None;let mut binding=None;let mut parameter_index=0;let mut place=None;let mut guards=Vec::new();let mut when=None;
  loop{
   let step=unsafe{b_finish_step(cursor,facts)};cursor=step.cursor;
   match step.need{
    FinishNeed::InvalidNever=>return self.fail(definition.function.span(),"B133","never 函数存在可以正常结束的路径"),
    need@(FinishNeed::MissingEmpty|FinishNeed::MissingReturn)=>return self.fail(definition.function.span(),if matches!(need,FinishNeed::MissingEmpty){"B132"}else{"B133"},"函数存在未返回普通值的继续路径"),
    FinishNeed::ImplicitReturn=>{unsafe{world.call(self.return_statement(None,definition.function.span())).await}?;}
    FinishNeed::Context=>{
     context=self.return_context.take();self.return_context=caller.take();self.flow=flow;self.control_uniform=uniform;facts.top=u32::from(self.return_context.is_none());
    }
    FinishNeed::Code=>{
     let value=self.code.split_off(code_start);facts.last_exit=u32::from(matches!(value.last().map(|inst|&inst.kind),Some(InstKind::Exit)));code=Some(value);
    }
    FinishNeed::OtherExits=>{
     let value=unsafe{code.as_ref().unwrap_unchecked()};facts.other_exits=unsafe{b_owns_exit(CodeSpan::of(&value[..value.len()-1]),ExitHost{inspect:inspect_exit})};
    }
    FinishNeed::PopExit=>{unsafe{code.as_mut().unwrap_unchecked()}.pop();}
    FinishNeed::Scope=>{self.emit(InstKind::Scope(unsafe{code.take().unwrap_unchecked()}));}
    FinishNeed::Append=>{self.code.extend(unsafe{code.take().unwrap_unchecked()});}
    FinishNeed::Initial=>{self.initialized=unsafe{initial.take().unwrap_unchecked()};}
    FinishNeed::First=>{
     let value=unsafe{context.as_mut().unwrap_unchecked()};facts.slots=value.slots.len()as u64;facts.scalars=value.expected.scalar_count()as u64;
     exits=Some(std::mem::take(&mut value.exits).into_iter());exit=unsafe{exits.as_mut().unwrap_unchecked()}.next();facts.present=u32::from(exit.is_some());
    }
    FinishNeed::NoExit=>{self.flow=Flow::STOP;self.path=bounds::Clause::impossible();return Ok((None,vec![None;definition.function.params().len()]));}
    FinishNeed::FirstModel=>{
     let ReturnExit{path:p,value,borrowed:b,initialized:i}=unsafe{exit.take().unwrap_unchecked()};path=Some(p);result=value;borrowed=b?;initialized=i;facts.result=u32::from(result.is_some());
    }
    FinishNeed::Slot=>{
     let value=unsafe{context.as_ref().unwrap_unchecked()};let scalar=value.expected.scalar(cursor.index as usize);let reg=self.reg(scalar);self.emit(InstKind::LoadLocal(reg,value.slots[cursor.index as usize]));registers.push(reg);
    }
    FinishNeed::Registers=>{registers=unsafe{result.as_ref().unwrap_unchecked()}.registers();}
    FinishNeed::EmptyRegisters=>{registers=Vec::new();}
    FinishNeed::Written=>{(register_map,facts.return_consumer)=written(self,&borrowed);}
    FinishNeed::BindFirst=>{
     self.bind_return_exit(&mut result,&mut borrowed,&mut initialized,unsafe{path.as_mut().unwrap_unchecked()},&registers,&register_map,entry);self.path=unsafe{path.take().unwrap_unchecked()};
    }
    FinishNeed::NextExit=>{exit=unsafe{exits.as_mut().unwrap_unchecked()}.next();facts.present=u32::from(exit.is_some());}
    FinishNeed::BindOther=>{
     let ReturnExit{path,value,borrowed,initialized}=unsafe{exit.take().unwrap_unchecked()};other_path=Some(path);other_value=value;other_borrowed=borrowed?;other_initialized=initialized;
     self.bind_return_exit(&mut other_value,&mut other_borrowed,&mut other_initialized,unsafe{other_path.as_mut().unwrap_unchecked()},&registers,&register_map,entry);facts.other_result=u32::from(other_value.is_some());
    }
    FinishNeed::MergePath=>{
     initialized.extend(std::mem::take(&mut other_initialized));choice=Some(bounds::Condition::paths(self.path.clone(),unsafe{other_path.as_ref().unwrap_unchecked()}.clone()));self.path=bounds::Clause::either(&self.path,unsafe{other_path.as_ref().unwrap_unchecked()});
    }
    FinishNeed::JoinResult=>{unsafe{result.as_mut().unwrap_unchecked()}.join(unsafe{other_value.take().unwrap_unchecked()},false,choice.as_ref());}
    FinishNeed::MergeBorrowed=>{
     models(&mut borrowed,|name,owner|{owner.join(other_borrowed.remove(name).unwrap(),false,choice.as_ref());});other_path.take();other_value.take();choice.take();
    }
    FinishNeed::RebindWritten=>{models(&mut borrowed,|name,model|{native_function::rebind_model(model,register_map.remove(name).unwrap());});}
    FinishNeed::Snapshot=>{output_models=snapshot(definition,&borrowed);published=Some(std::mem::take(&mut borrowed).into_iter());}
    FinishNeed::OutputStorage=>{returned=result.as_ref().map(|value|self.returned_storage(value));}
    FinishNeed::NextModel=>{
     let value=unsafe{published.as_mut().unwrap_unchecked()}.next();facts.present=u32::from(value.is_some());
     if let Some((key,value))=value{facts.model=value.kind();name=Some(key);model=Some(value);}
    }
    FinishNeed::Dataset=>{
     let Returned::Dataset(value)=(unsafe{model.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};binding=Some(Binding::ApplicationData(value));
    }
    FinishNeed::Parameter=>{parameter_index=parameter(definition,unsafe{name.as_deref().unwrap_unchecked()});}
    FinishNeed::Place=>{
     let value=self.env[unsafe{name.as_ref().unwrap_unchecked()}].stored_place().unwrap();facts.declared=u32::from(outputs::declared(&instance,parameter_index));facts.owns=u32::from(value.ty().owns_resources());place=Some(value);
    }
    FinishNeed::Guards=>{
     guards=unsafe{world.call(outputs::guards(self,&instance,parameter_index,place.as_ref().unwrap_unchecked().ty(),&context.as_ref().unwrap_unchecked().inputs,returned.as_ref())).await}?;facts.guards=guards.len()as u64;
    }
    FinishNeed::DirectGuards=>{
     let direct=returned.as_ref().map(|value:&Value|Value{ty:value.ty.clone(),regs:value.regs.clone(),proof:bounds::Proof::default(),uniform:value.uniform,literal:value.literal,constant:value.constant.clone()});
     guards=unsafe{world.call(outputs::guards(self,&instance,parameter_index,place.as_ref().unwrap_unchecked().ty(),&context.as_ref().unwrap_unchecked().inputs,direct.as_ref())).await}?;facts.guards=guards.len()as u64;
    }
    FinishNeed::Ownership=>{outputs::apply_ownership(unsafe{model.as_mut().unwrap_unchecked()},unsafe{place.as_ref().unwrap_unchecked()}.ty(),0,&guards);}
    FinishNeed::Guard=>{let value=guards[cursor.part as usize].condition();facts.active=u32::from(value.active());when=Some(value);}
    FinishNeed::Initialization=>{
     let place=unsafe{place.as_ref().unwrap_unchecked()};let part=place.clone().project(Ty::scalar(place.ty().components[cursor.part as usize].0),&[cursor.part as usize]);let source=address::InitializationSource::Position(unsafe{world.call(self.position_origin(&part,definition.function.span())).await}?);
     initialized.push(address::InitializationEffect::Position(address::Initialization{source,when:unsafe{when.take().unwrap_unchecked()}}));
    }
    FinishNeed::Value=>{
     let Returned::Value(value)=(unsafe{model.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};
     let Binding::Owned(owner)=&self.env[unsafe{name.as_ref().unwrap_unchecked()}]else{unsafe{std::hint::unreachable_unchecked()}};
     let owner=OwnedBinding{place:owner.place.clone(),initialization:owner.initialization.clone(),proof:value.proof,uniform:value.uniform};self.initialize_output(&owner.place,&guards);binding=Some(Binding::Owned(owner));
    }
    FinishNeed::Owned=>{
     let Returned::OwnedValue(mut value)=(unsafe{model.take().unwrap_unchecked()})else{unsafe{std::hint::unreachable_unchecked()}};let place=unsafe{place.take().unwrap_unchecked()};self.initialize_output(&place,&guards);value.place=Some(place);binding=Some(Binding::OwnedValue(value));
    }
    FinishNeed::InvalidModel=>unsafe{std::hint::unreachable_unchecked()},
    FinishNeed::Insert=>{self.env.insert(unsafe{name.take().unwrap_unchecked()},unsafe{binding.take().unwrap_unchecked()});place.take();guards.clear();when.take();}
    FinishNeed::RebindResult=>{self.rebind_returned(unsafe{result.as_mut().unwrap_unchecked()},std::mem::take(&mut registers));}
    FinishNeed::Publish=>{self.view_model.initialization=std::mem::take(&mut initialized);self.view_model.normal_returns=Some(bounds::Condition::execution(&self.path));}
    FinishNeed::Complete=>return Ok((result,output_models)),
    FinishNeed::Next=>{},
   }
  }
 }
}
