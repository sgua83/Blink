//! Source return control borrows the original Builder and return owners.
use super::*;
use crate::check::owned_presence::Presence;
#[derive(Clone,Copy)]
#[repr(C)] struct Cursor{phase:u32,flags:u32,index:u64,part:u64}
#[repr(u32)] enum StatementNeed{NoContext,EvaluateValue,EvaluateDataset,EvaluateNever,MissingValue,MissingDataset,CheckValue,CheckDataset,BadNever,EmptyValue,VoidValue,Finish,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct StatementFacts{context:u32,expected:u32,expression:u32,continuing:u32,present:u32,kind:u32}
#[repr(C)] struct StatementStep{cursor:Cursor,need:StatementNeed}
#[repr(u32)] enum CollectNeed{Parameter,Data,Snapshot,DataGuards,DataCheck,DataInitialized,DataDefer,BadData,InsertData,View,ViewGuards,ViewPath,GuardPath,GuardInitialized,GuardDefer,BadGuard,RestorePath,Place,ValueGuards,Conditional,Written,Normalize,InsertValue,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct CollectFacts{dataset:u32,tile:u32,writable:u32,write:u32,binding:u32,declared:u32,snapshot:u32,initialized:u32,deferred:u32,possible:u32,parameters:u64,guards:u64}
#[repr(C)] struct CollectStep{cursor:Cursor,need:CollectNeed}
#[repr(u32)] enum CheckNeed{Output,OwnData,OwnValue,Reset,Elements,View,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct CheckFacts{present:u32,kind:u32}
#[repr(C)] struct CheckStep{cursor:Cursor,need:CheckNeed}
#[repr(u32)] enum FinishNeed{Initialized,Value,Owned,Borrowed,Outputs,Environment,ViewUses,Hold,Release,Remove,Initialization,Borrow,Publish,Context,Store,Exit,Push,Flow,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct FinishFacts{present:u32,kind:u32,pending:u32,task:u32,view:u32,scoped:u32,slots:u64,registers:u64}
#[repr(C)] struct FinishStep{cursor:Cursor,need:FinishNeed}
#[repr(u32)] enum InitNeed{Parameter,Project,Effects,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct InitFacts{invocation:u32,dataset:u32,write:u32,data:u32,parameters:u64}
#[repr(C)] struct InitStep{cursor:Cursor,need:InitNeed}
unsafe extern "C"{
 #[link_name="bootstrap.return_exit::start"]fn b_start()->Cursor;
 #[link_name="bootstrap.return_exit::statement_step"]fn b_statement_step(state:Cursor,facts:StatementFacts)->StatementStep;
 #[link_name="bootstrap.return_exit::collect_step"]fn b_collect_step(state:Cursor,facts:CollectFacts)->CollectStep;
 #[link_name="bootstrap.return_exit::check_step"]fn b_check_step(state:Cursor,facts:CheckFacts)->CheckStep;
 #[link_name="bootstrap.return_exit::finish_step"]fn b_finish_step(state:Cursor,facts:FinishFacts)->FinishStep;
 #[link_name="bootstrap.return_exit::init_step"]fn b_init_step(state:Cursor,facts:InitFacts)->InitStep;
 #[link_name="bootstrap.return_exit::keep_temporary"]fn b_keep_temporary(temporary:u32,owner:u32,may_own:u32)->u32;
}
struct GuardInputs{inputs:BTreeMap<String,Value>,returned:Option<Value>}
impl Builder<'_>{
 pub(in crate::check) async fn return_statement(&mut self,expr:Option<&Expr>,span:Span)->Result<()>{
  let world=self.world;
  let expected=self.return_context.as_ref().map(|context|context.expected.clone());
  let mut facts=StatementFacts{context:u32::from(expected.is_some()),expected:expected.as_ref().map_or(0,|value|value.kind()),expression:u32::from(expr.is_some()),..StatementFacts::default()};
  let mut cursor=unsafe{b_start()};let mut value=None;
  loop{
   let step=unsafe{b_statement_step(cursor,facts)};cursor=step.cursor;
   match step.need{
    StatementNeed::NoContext=>return self.fail(span,"B130","return 只能结束当前普通函数，不能退出 parallel 实例"),
    StatementNeed::EvaluateValue=>{
     let Expected::Value(ty)=expected.as_deref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     value=unsafe{world.call(self.evaluate_returned(expr.unwrap(),Some(ty))).await}?;
     facts.continuing=u32::from(self.flow.continues());facts.present=u32::from(value.is_some());
    }
    StatementNeed::EvaluateDataset=>{
     let Expected::Dataset{ty,count}=expected.as_deref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     value=unsafe{world.call(self.evaluate_returned(expr.unwrap(),Some(&Ty::owned_dataset(ty.clone().syntax(),*count)))).await}?;
     facts.continuing=u32::from(self.flow.continues());facts.present=u32::from(value.is_some());facts.kind=value.as_ref().map_or(0,Returned::kind);
    }
    StatementNeed::EvaluateNever=>{
     unsafe{world.call(self.evaluate_returned(expr.unwrap(),None)).await}?;
     facts.continuing=u32::from(self.flow.continues());
    }
    StatementNeed::MissingValue=>return self.fail(span,"B228","返回需要值或拥有资源"),
    StatementNeed::MissingDataset=>return self.fail(span,"B224","返回需要实际拥有的dataset"),
    StatementNeed::CheckValue=>{
     let Expected::Value(ty)=expected.as_deref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     unsafe{world.call(self.check_returned_type(value.as_mut().unwrap(),ty,expr.unwrap().span())).await}?;
    }
    StatementNeed::CheckDataset=>{
     let Expected::Dataset{ty,count}=expected.as_deref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     let Returned::Dataset(data)=value.as_mut().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     self.application_data_type(data,ty,*count,span)?;data.count=*count;
    }
    StatementNeed::BadNever=>return self.fail(span,"B228","never 函数不能正常返回"),
    StatementNeed::EmptyValue=>return self.fail(span,"B228","有返回值函数不能使用 return;"),
    StatementNeed::VoidValue=>return self.fail(span,"B228","void 函数不能返回普通值"),
    StatementNeed::Finish=>return unsafe{world.call(self.finish_return(value,span)).await},
    StatementNeed::Complete=>return Ok(()),
    StatementNeed::Next=>{}
   }
  }
 }
 async fn return_borrowed(&mut self,value:&Option<Returned>,span:Span)->Result<BTreeMap<String,Returned>>{
  let world=self.world;let instance=self.return_context.as_ref().unwrap().instance.clone();let definition=&instance.definition;
  let mut borrowed=BTreeMap::new();let mut snapshot:Option<GuardInputs>=None;
  let mut parameters=definition.function.params().into_iter();let mut parameter=None;
  let mut data:Option<application::Data>=None;let mut place:Option<Place>=None;let mut model:Option<Returned>=None;
  let mut guards:Vec<Presence>=Vec::new();let mut positions=Vec::<Position>::new().into_iter().zip(Vec::<Presence>::new());let mut position=None;let mut path:Option<bounds::Clause>=None;
  let mut facts=CollectFacts{parameters:definition.function.params().len()as u64,..CollectFacts::default()};let mut cursor=unsafe{b_start()};
  loop{
   let step=unsafe{b_collect_step(cursor,facts)};cursor=step.cursor;
   match step.need{
    CollectNeed::Parameter=>{
     parameter=parameters.next();let param=parameter.as_ref().unwrap();
     facts.dataset=u32::from(param.ty().dataset());facts.tile=u32::from(param.ty().tile());
     facts.writable=u32::from(matches!(param.access(),Access::Write|Access::InOut));facts.write=u32::from(param.access()==Access::Write);
     facts.binding=match self.env.get(param.name()){Some(Binding::ApplicationData(_))=>1,Some(Binding::Data(_))=>2,_=>0};
     facts.declared=u32::from(outputs::declared(&instance,cursor.index as usize));
    }
    CollectNeed::Data=>{
     let Binding::ApplicationData(value)=&self.env[parameter.as_ref().unwrap().name()]else{unsafe{std::hint::unreachable_unchecked()}};
     data=Some(value.clone());
    }
    CollectNeed::Snapshot=>{
     snapshot=Some(GuardInputs{inputs:self.return_context.as_ref().unwrap().inputs.clone(),returned:value.as_ref().map(|value|self.returned_storage(value))});facts.snapshot=1;
    }
    CollectNeed::DataGuards=>{
     let input=snapshot.as_ref().unwrap();
     guards=unsafe{world.call(outputs::guards(self,&instance,cursor.index as usize,&data.as_ref().unwrap().ty,&input.inputs,input.returned.as_ref())).await}?;
    }
    CollectNeed::DataCheck=>unsafe{world.call(self.check_data_output(data.as_ref().unwrap(),&guards,span)).await}?,
    CollectNeed::DataInitialized=>facts.initialized=u32::from(self.application_initialized(data.as_ref().unwrap())),
    CollectNeed::DataDefer=>{
     let data=data.as_ref().unwrap();
     facts.deferred=u32::from(self.defer_dataset_output(data,data.ty.components.iter().map(|(_,word)|data.initialization.as_ref().unwrap().application_word(None,*word)).collect(),span)?);
    }
    CollectNeed::BadData=>return self.fail(span,"B229",format!("返回前没有完整初始化独占 dataset 参数 {}",parameter.as_ref().unwrap().name())),
    CollectNeed::InsertData=>{borrowed.insert(parameter.as_ref().unwrap().name().to_owned(),Returned::Dataset(data.take().unwrap()));}
    CollectNeed::View=>{
     let Binding::Data(view)=&self.env[parameter.as_ref().unwrap().name()]else{unsafe{std::hint::unreachable_unchecked()}};
     let view=view.clone();place=Some(self.dataset_element(&view,self.global_index,false));
    }
    CollectNeed::ViewGuards=>{
     let input=snapshot.as_ref().unwrap();
     guards=unsafe{world.call(outputs::guards(self,&instance,cursor.index as usize,place.as_ref().unwrap().ty(),&input.inputs,input.returned.as_ref())).await}?;
    }
    CollectNeed::ViewPath=>{
     path=Some(self.path.clone());positions=self.positions(place.as_ref().unwrap()).into_iter().zip(std::mem::take(&mut guards));facts.guards=positions.len()as u64;
    }
    CollectNeed::GuardPath=>{position=positions.next();self.path=position.as_ref().unwrap().1.condition().assume(path.as_ref().unwrap(),true);facts.possible=u32::from(self.path.possible());}
    CollectNeed::GuardInitialized=>facts.initialized=u32::from(self.positions_initialized(std::slice::from_ref(&position.as_ref().unwrap().0))),
    CollectNeed::GuardDefer=>facts.deferred=u32::from(self.defer_position_initialization(vec![position.take().unwrap().0],"B229",span)?),
    CollectNeed::BadGuard=>return self.fail(span,"B229","返回前没有初始化本次dataset借用的输出分量"),
    CollectNeed::RestorePath=>{self.path=path.take().unwrap();place=None;position=None;}
    CollectNeed::Place=>{place=self.env[parameter.as_ref().unwrap().name()].stored_place();}
    CollectNeed::ValueGuards=>{
     let input=snapshot.as_ref().unwrap();
     guards=unsafe{world.call(outputs::guards(self,&instance,cursor.index as usize,place.as_ref().unwrap().ty(),&input.inputs,input.returned.as_ref())).await}?;
    }
    CollectNeed::Conditional=>{model=Some(unsafe{world.call(self.conditional_written_model(&parameter.as_ref().unwrap().name(),&guards,span)).await}?);place=None;}
    CollectNeed::Written=>{model=Some(unsafe{world.call(self.written_model(&parameter.as_ref().unwrap().name(),span)).await}?);}
    CollectNeed::Normalize=>{if let Some(value)=model.as_mut().unwrap().view_value_mut(){value.uniform&=self.control_uniform;value.proof.under_path(&value.ty,&self.path);}}
    CollectNeed::InsertValue=>{borrowed.insert(parameter.as_ref().unwrap().name().to_owned(),model.take().unwrap());}
    CollectNeed::Complete=>return Ok(borrowed),
    CollectNeed::Next=>{}
   }
  }
 }

 async fn return_outputs(&mut self,value:&Option<Returned>,borrowed:&Result<BTreeMap<String,Returned>>,span:Span)->Result<(view::Borrow,view::Borrow)>{
  let world=self.world;let outputs=||value.iter().chain(borrowed.as_ref().ok().into_iter().flat_map(|written|written.values()));
  let mut iterator=outputs();let mut output=None;let mut owned=view::Borrow::default();let mut retained=view::Borrow::default();
  let mut cursor=unsafe{b_start()};let mut facts=CheckFacts::default();
  loop{
   let step=unsafe{b_check_step(cursor,facts)};cursor=step.cursor;
   match step.need{
    CheckNeed::Output=>{output=iterator.next();facts.present=u32::from(output.is_some());facts.kind=output.map_or(0,Returned::kind);}
    CheckNeed::OwnData=>{let Returned::Dataset(data)=output.unwrap()else{unsafe{std::hint::unreachable_unchecked()}};owned.union(&self.owned_resource_borrow(data));}
    CheckNeed::OwnValue=>{let Returned::OwnedValue(value)=output.unwrap()else{unsafe{std::hint::unreachable_unchecked()}};value.visit_resources(&mut |data|owned.union(&self.owned_resource_borrow(data)));}
    CheckNeed::Reset=>{iterator=outputs();}
    CheckNeed::Elements=>unsafe{world.call(self.require_element_return_borrows(output.unwrap(),&owned,span)).await}?,
    CheckNeed::View=>{
     let value=output.unwrap().view_value().unwrap();let mut proof=value.proof.borrow_state();proof.retain_view_borrows(&value.ty);
     self.require_view_return_proof(&proof,owned.clone(),span)?;retained.union(&proof.borrowed_view());
    }
    CheckNeed::Complete=>return Ok((owned,retained)),
    CheckNeed::Next=>{}
   }
  }
 }
 async fn return_initialization(&self)->Result<Vec<address::InitializationEffect>>{
  let world=self.world;let mut initialized=std::borrow::Cow::Borrowed(&self.initialized);
  let mut parameters=self.scope.parameters();let mut parameter=None;
  let mut facts=InitFacts{invocation:u32::from(self.scope.execution()==crate::execution::Requirement::Invocation),parameters:self.scope.parameters().count()as u64,..InitFacts::default()};
  let mut cursor=unsafe{b_start()};
  loop{
   let step=unsafe{b_init_step(cursor,facts)};cursor=step.cursor;
   match step.need{
    InitNeed::Parameter=>{
     let value=parameters.next().unwrap();facts.dataset=u32::from(unsafe{world.call(value.bound(world)).await}?.ty.dataset);
     facts.write=u32::from(value.access()==Access::Write);facts.data=u32::from(matches!(self.env.get(value.name()),Some(Binding::ApplicationData(_))));parameter=Some(value);
    }
    InitNeed::Project=>{
     let Binding::ApplicationData(data)=&self.env[parameter.as_ref().unwrap().name()]else{unsafe{std::hint::unreachable_unchecked()}};
     initialized=std::borrow::Cow::Owned(initialized.iter().map(|position|Self::dataset_output_position(position,data)).collect());
    }
    InitNeed::Effects=>return Ok(address::initialization_effects(&initialized,&self.path)),
    InitNeed::Next=>{}
   }
  }
 }
 pub(in crate::check) async fn finish_return(&mut self,mut value:Option<Returned>,span:Span)->Result<()>{
  let world=self.world;let mut cursor=unsafe{b_start()};
  let mut facts=FinishFacts{present:u32::from(value.is_some()),kind:value.as_ref().map_or(0,Returned::kind),..FinishFacts::default()};
  let mut borrowed=None;let mut owned=None;let mut retained=None;let mut pending=None;let mut initialized=None;
  loop{
   let step=unsafe{b_finish_step(cursor,facts)};cursor=step.cursor;
   match step.need{
    FinishNeed::Initialized=>unsafe{world.call(self.require_returned_initialized(value.as_mut().unwrap(),span)).await}?,
    FinishNeed::Value=>{
     let Returned::Value(value)=value.as_mut().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     value.uniform&=self.control_uniform;value.proof=bounds::Proof::returning(value,&self.env,&self.path);
    }
    FinishNeed::Owned=>{
     let Returned::OwnedValue(value)=value.as_mut().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     value.value.proof=bounds::Proof::returning(&value.value,&self.env,&self.path);
    }
    // The original delayed output error belongs to ReturnExit and its branch consumer.
    FinishNeed::Borrowed=>{borrowed=Some(unsafe{world.call(self.return_borrowed(&value,span)).await});}
    FinishNeed::Outputs=>{
     let result=unsafe{world.call(self.return_outputs(&value,borrowed.as_ref().unwrap(),span)).await}?;owned=Some(result.0);retained=Some(result.1);
    }
    FinishNeed::Environment=>self.env.retain(|name,binding|{
     let(owner,may_own)=match binding{Binding::ApplicationData(data)=>(1,u32::from(data.ownership.may_own())),Binding::OwnedValue(value)=>(1,u32::from(value.ownership.may_own())),_=>(0,0)};
     unsafe{b_keep_temporary(u32::from(name.starts_with('\0')),owner,may_own)!=0}
    }),
    FinishNeed::ViewUses=>self.view_uses.returning(),
    FinishNeed::Hold=>{
     let binding=match value.as_ref().unwrap(){Returned::OwnedValue(value)=>Binding::OwnedValue(value.clone()),Returned::Dataset(data)=>Binding::ApplicationData(data.clone()),_=>unsafe{std::hint::unreachable_unchecked()}};
     pending=Some(self.hold_binding(binding));facts.pending=1;
    }
    FinishNeed::Release=>{
     let keep=pending.iter().cloned().collect();unsafe{world.call(self.release_application_data(Some(&keep),retained.take().unwrap())).await}?;
    }
    FinishNeed::Remove=>{self.env.remove(&pending.take().unwrap());}
    FinishNeed::Initialization=>{initialized=Some(unsafe{world.call(self.return_initialization()).await}?);}
    FinishNeed::Borrow=>{
     let memory=self.view_model.uses.len();
     self.require_return_borrow(view::Borrow{reads:view::Edges::from([view_contracts::ContentRead::Returned{memory}]),..view::Borrow::default()},owned.take().unwrap(),span)?;
     facts.task=u32::from(self.task_return_source.is_some());facts.view=u32::from(value.as_ref().and_then(Returned::view_value).is_some());
    }
    FinishNeed::Publish=>self.publish_task_return_sources(value.as_mut().and_then(Returned::view_value_mut).unwrap(),self.task_return_source.unwrap()),
    FinishNeed::Context=>{
     let context=self.return_context.as_ref().unwrap();facts.scoped=u32::from(context.scoped);facts.slots=context.slots.len()as u64;
     facts.registers=value.as_ref().map_or(0,Returned::register_count)as u64;
    }
    FinishNeed::Store=>{
     let slot=self.return_context.as_ref().unwrap().slots[cursor.index as usize];let reg=value.as_ref().unwrap().register(cursor.index as usize);
     self.code.push(Inst{source_site:self.source_site.clone(),..Inst::new(InstKind::StoreLocal(slot,reg))});
    }
    FinishNeed::Exit=>self.code.push(Inst{source_site:self.source_site.clone(),..Inst::new(InstKind::Exit)}),
    FinishNeed::Push=>{
     self.return_context.as_mut().unwrap().exits.push(ReturnExit{path:self.path.clone(),value:value.take(),borrowed:borrowed.take().unwrap(),initialized:initialized.take().unwrap()});
    }
    FinishNeed::Flow=>self.flow=Flow::RETURN,
    FinishNeed::Complete=>return Ok(()),
    FinishNeed::Next=>{}
   }
  }
 }
}
