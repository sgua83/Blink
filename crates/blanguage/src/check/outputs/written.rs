//! Writable output control borrows the actual model, initialization and resource owners.
use super::*;
#[derive(Clone,Copy)]
#[repr(C)] struct Cursor{phase:u32,flags:u32,index:u64,part:u64}
#[repr(u32)] enum WrittenNeed{Binding,Resolve,Prepare,Available,Place,Initialized,Defer,BadInit,BadAvailable,Load,ReturnedInitialized,Register,Plain,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct WrittenFacts{kind:u32,initialized:u32,deferred:u32,available:u32,variants:u32,fields:u64}
#[repr(C)] struct WrittenStep{cursor:Cursor,need:WrittenNeed}
#[repr(u32)] enum ConditionalNeed{Binding,Place,Path,Guard,Snapshot,Prepare,Selected,Initialized,Defer,BadInit,RestoreInitialization,RestorePath,Model,Resources,Register,Plain,Rebind,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct ConditionalFacts{kind:u32,possible:u32,initialized:u32,deferred:u32,snapshot:u32,guards:u64,fields:u64}
#[repr(C)] struct ConditionalStep{cursor:Cursor,need:ConditionalNeed}
#[repr(u32)] enum DataNeed{Path,FieldPath,Initialized,Defer,BadInit,Elements,Resources,Restore,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct DataFacts{resources:u32,nonzero:u32,possible:u32,initialized:u32,deferred:u32,fields:u64,guards:u64}
#[repr(C)] struct DataStep{cursor:Cursor,need:DataNeed}
#[repr(u32)] enum ResourceNeed{Inspect,Guard,TruePath,ResolveData,AvailableData,RequireData,FalsePath,MayOwnData,BadData,BadDataAbsent,AvailableValue,MayOwnValue,BadValue,BadValueAbsent,Child,Recurse,MarkAbsent,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)] struct ResourceFacts{kind:u32,cleanup:u32,possible:u32,available:u32,may_own:u32,present:u32,moved:u32,children:u64}
#[repr(C)] struct ResourceStep{cursor:Cursor,need:ResourceNeed}
unsafe extern "C"{
 #[link_name="bootstrap.written_output::start"]fn b_start()->Cursor;
 #[link_name="bootstrap.written_output::written_step"]fn b_written_step(state:Cursor,f:WrittenFacts)->WrittenStep;
 #[link_name="bootstrap.written_output::conditional_step"]fn b_conditional_step(state:Cursor,f:ConditionalFacts)->ConditionalStep;
 #[link_name="bootstrap.written_output::data_step"]fn b_data_step(state:Cursor,f:DataFacts)->DataStep;
 #[link_name="bootstrap.written_output::resource_step"]fn b_resource_step(state:Cursor,f:ResourceFacts)->ResourceStep;
}
fn written_place<'a>(binding:&'a Option<Binding>,place:&'a Option<Place>)->&'a Place{
 match binding{Some(Binding::Owned(owner))=>&owner.place,_=>place.as_ref().unwrap()}
}
fn binding_kind(binding:&Binding)->u32{match binding{Binding::Owned(_)=>1,Binding::OwnedValue(_)=>2,_=>unsafe{std::hint::unreachable_unchecked()}}}
impl Builder<'_>{
 pub(in crate::check) async fn written_model(&mut self,name:&str,span:Span)->Result<Returned>{
  let world=self.world;let mut cursor=unsafe{b_start()};let mut facts=WrittenFacts::default();
  let mut binding=None;let mut model=None;let mut place=None;let mut registers=Vec::new();
  loop{
   let step=unsafe{b_written_step(cursor,facts)};cursor=step.cursor;
   match step.need{
    WrittenNeed::Binding=>{let value=self.env.get(name).cloned().unwrap();facts.kind=binding_kind(&value);binding=Some(value);}
    WrittenNeed::Resolve=>{
     let Binding::OwnedValue(value)=binding.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     model=Some(Returned::OwnedValue(value));self.resolve_owned_model(model.as_mut().unwrap(),None);
    }
    WrittenNeed::Prepare=>self.prepare_owned_read(model.as_ref().unwrap()),
    WrittenNeed::Available=>{let Returned::OwnedValue(value)=model.as_ref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};facts.available=u32::from(value.all_available());}
    WrittenNeed::Place=>{
     if let Some(Returned::OwnedValue(value))=&model{place=Some(value.place().unwrap());facts.variants=u32::from(value.value.ty.has_variants());}
     facts.fields=written_place(&binding,&place).ty().components.len()as u64;
    }
    WrittenNeed::Initialized=>facts.initialized=u32::from(self.is_initialized(written_place(&binding,&place))),
    WrittenNeed::Defer=>facts.deferred=u32::from(self.defer_initialization(written_place(&binding,&place),"B229",span)?),
    WrittenNeed::BadInit=>return self.fail(span,"B229",format!("返回前没有完整初始化独占参数 {name}")),
    WrittenNeed::BadAvailable=>return self.fail(span,"B229",format!("返回前没有归还独占参数 {name} 的全部活动拥有资源")),
    WrittenNeed::Load=>{let Returned::OwnedValue(value)=model.as_mut().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};unsafe{world.call(self.load_owned_value(value,span)).await}?;}
    WrittenNeed::ReturnedInitialized=>unsafe{world.call(self.require_returned_initialized(model.as_mut().unwrap(),span)).await}?,
    WrittenNeed::Register=>registers.push(self.reg(written_place(&binding,&place).ty().components[cursor.index as usize].0)),
    WrittenNeed::Plain=>{
     let Binding::Owned(owner)=binding.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     model=Some(Returned::value(Value{ty:owner.place.ty().clone(),regs:std::mem::take(&mut registers),proof:owner.proof,uniform:owner.uniform,literal:false,constant:None}));
    }
    WrittenNeed::Complete=>return Ok(model.take().unwrap()),
    WrittenNeed::Next=>{}
   }
  }
 }
 pub(in crate::check) async fn conditional_written_model(&mut self,name:&str,guards:&[Presence],span:Span)->Result<Returned>{
  let world=self.world;let mut cursor=unsafe{b_start()};let mut facts=ConditionalFacts{guards:guards.len()as u64,..ConditionalFacts::default()};
  let mut binding=None;let mut place:Option<Place>=None;let mut path=None;let mut initialized=None;let mut selected=None;let mut model=None;let mut registers=Vec::new();
  loop{
   let step=unsafe{b_conditional_step(cursor,facts)};cursor=step.cursor;
   match step.need{
    ConditionalNeed::Binding=>{let value=self.env[name].clone();facts.kind=binding_kind(&value);binding=Some(value);}
    ConditionalNeed::Place=>{place=binding.as_ref().unwrap().stored_place();facts.fields=place.as_ref().unwrap().ty().components.len()as u64;}
    ConditionalNeed::Path=>{path=Some(self.path.clone());}
    ConditionalNeed::Guard=>{self.path=guards[cursor.index as usize].condition().assume(path.as_ref().unwrap(),true);facts.possible=u32::from(self.path.possible());}
    ConditionalNeed::Snapshot=>{initialized=Some(self.initialized.clone());facts.snapshot=1;}
    ConditionalNeed::Prepare=>{
     let Binding::OwnedValue(value)=binding.as_ref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     self.prepare_selected_owned_read(value);
    }
    ConditionalNeed::Selected=>{let place=place.as_ref().unwrap();selected=Some(place.clone().project(Ty::scalar(place.ty().components[cursor.index as usize].0),&[cursor.index as usize]));}
    ConditionalNeed::Initialized=>facts.initialized=u32::from(self.is_initialized(selected.as_ref().unwrap())),
    ConditionalNeed::Defer=>facts.deferred=u32::from(self.defer_initialization(selected.as_ref().unwrap(),"B229",span)?),
    ConditionalNeed::BadInit=>return self.fail(span,"B229",format!("返回分支没有满足 {name} 的条件输出初始化合同")),
    ConditionalNeed::RestoreInitialization=>{self.initialized=initialized.as_ref().unwrap().clone();}
    ConditionalNeed::RestorePath=>{self.path=path.as_ref().unwrap().clone();}
    ConditionalNeed::Model=>{
     let Binding::OwnedValue(value)=binding.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     model=Some(Returned::OwnedValue(value));
    }
    ConditionalNeed::Resources=>unsafe{world.call(Self::check_output_resources(self,model.as_mut().unwrap(),place.as_ref().unwrap().ty(),0,guards,path.as_ref().unwrap(),span)).await}?,
    ConditionalNeed::Register=>registers.push(self.reg(place.as_ref().unwrap().ty().components[cursor.part as usize].0)),
    ConditionalNeed::Plain=>{
     let Binding::Owned(owner)=binding.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     model=Some(Returned::value(Value{ty:place.as_ref().unwrap().ty().clone(),regs:std::mem::take(&mut registers),proof:owner.proof,uniform:owner.uniform,literal:false,constant:None}));
    }
    ConditionalNeed::Rebind=>native_function::rebind_model(model.as_mut().unwrap(),std::mem::take(&mut registers)),
    ConditionalNeed::Complete=>return Ok(model.take().unwrap()),
    ConditionalNeed::Next=>{}
   }
  }
 }

 pub(in crate::check) async fn check_data_output(&mut self,data:&application::Data,guards:&[Presence],span:Span)->Result<()>{
  let world=self.world;let mut cursor=unsafe{b_start()};
  let mut facts=DataFacts{resources:u32::from(data.ty.owns_resources()),nonzero:u32::from(data.length.maximum()!=0),fields:data.ty.components.len()as u64,guards:guards.len()as u64,..DataFacts::default()};
  let mut path=None;let mut model=None;
  loop{
   let step=unsafe{b_data_step(cursor,facts)};cursor=step.cursor;
   match step.need{
    DataNeed::Path=>{path=Some(self.path.clone());}
    DataNeed::FieldPath=>{self.path=guards[cursor.index as usize].condition().assume(path.as_ref().unwrap(),true);facts.possible=u32::from(self.path.possible());}
    DataNeed::Initialized=>facts.initialized=u32::from(self.application_field_initialized(data,data.ty.components[cursor.index as usize].1)),
    DataNeed::Defer=>facts.deferred=u32::from(self.defer_dataset_output(data,vec![data.initialization.as_ref().unwrap().application_word(None,data.ty.components[cursor.index as usize].1)],span)?),
    DataNeed::BadInit=>return self.fail(span,"B229","返回分支没有满足dataset字段的条件初始化合同"),
    DataNeed::Elements=>{
     self.path=path.as_ref().unwrap().clone();model=Some(unsafe{world.call(self.element_model(&data.ty,&data.elements,(&data.residency,&[]),span)).await}?);
    }
    DataNeed::Resources=>unsafe{world.call(Self::check_output_resources(self,model.as_deref_mut().unwrap(),&data.ty,0,guards,path.as_ref().unwrap(),span)).await}?,
    DataNeed::Restore=>{self.path=path.take().unwrap();}
    DataNeed::Complete=>return Ok(()),
    DataNeed::Next=>{}
   }
  }
 }
 async fn check_output_resources(builder:&mut Builder<'_>,model:&mut Returned,root:&Ty,offset:u32,guards:&[Presence],path:&bounds::Clause,span:Span)->Result<()>{
  let world=builder.world;let mut cursor=unsafe{b_start()};let mut facts=ResourceFacts::default();
  let mut when:Option<Presence>=None;let mut selected=None;
  loop{
   let step=unsafe{b_resource_step(cursor,facts)};cursor=step.cursor;
   match step.need{
    ResourceNeed::Inspect=>{
     facts.kind=model.kind();
     if let Returned::Dataset(data)=model{facts.moved=u32::from(data.ownership==Ownership::Moved);}
     if let Returned::OwnedValue(value)=model{facts.cleanup=u32::from(value.value.ty.cleanup.is_some());facts.children=value.payloads.len()as u64;}
    }
    ResourceNeed::Guard=>{when=Some(resource_guard(root,offset,guards));}
    ResourceNeed::TruePath=>{builder.path=when.as_ref().unwrap().condition().assume(path,true);facts.possible=u32::from(builder.path.possible());}
    ResourceNeed::ResolveData=>{selected=Some(model.clone());builder.resolve_owned_model(selected.as_mut().unwrap(),None);}
    ResourceNeed::AvailableData=>{let Returned::Dataset(data)=selected.as_ref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};facts.available=u32::from(data.ownership.available());}
    ResourceNeed::RequireData=>{
     let Returned::Dataset(data)=selected.as_ref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};
     unsafe{world.call(builder.require_application_initialized(data,span)).await}?;selected=None;
    }
    ResourceNeed::FalsePath=>{builder.path=when.as_ref().unwrap().condition().assume(path,false);facts.possible=u32::from(builder.path.possible());}
    ResourceNeed::MayOwnData=>{let Returned::Dataset(data)=model else{unsafe{std::hint::unreachable_unchecked()}};facts.may_own=u32::from(builder.resolve_ownership(&data.ownership,None).may_own());}
    ResourceNeed::BadData=>return builder.fail(span,"B229","条件输出分支没有实际拥有所声明的资源"),
    ResourceNeed::BadDataAbsent=>return builder.fail(span,"B229","未交付分支仍拥有输出资源；按实际返回关系交付，或由原所有者释放"),
    ResourceNeed::AvailableValue=>{let Returned::OwnedValue(value)=model else{unsafe{std::hint::unreachable_unchecked()}};facts.available=u32::from(builder.resolve_ownership(&value.ownership,None).available());}
    ResourceNeed::MayOwnValue=>{let Returned::OwnedValue(value)=model else{unsafe{std::hint::unreachable_unchecked()}};facts.may_own=u32::from(builder.resolve_ownership(&value.ownership,None).may_own());}
    ResourceNeed::BadValue=>return builder.fail(span,"B229","条件输出分支没有实际构造用户拥有值"),
    ResourceNeed::BadValueAbsent=>return builder.fail(span,"B229","未交付分支仍拥有用户资源；按实际返回关系交付或由原所有者释放"),
    ResourceNeed::Child=>{
     let Returned::OwnedValue(value)=model else{unsafe{std::hint::unreachable_unchecked()}};
     facts.present=u32::from(value.payloads[cursor.index as usize].is_some());
    }
    ResourceNeed::Recurse=>{
     let Returned::OwnedValue(value)=model else{unsafe{std::hint::unreachable_unchecked()}};
     let at=if value.value.ty.has_variants(){value.value.ty.variant_payload_offset()}else{value.value.ty.aggregate_part(cursor.index as usize).0};
     unsafe{world.call(Self::check_output_resources(builder,value.payloads[cursor.index as usize].as_deref_mut().unwrap(),root,offset+at,guards,path,span)).await}?;
    }
    ResourceNeed::MarkAbsent=>{let Returned::Dataset(data)=model else{unsafe{std::hint::unreachable_unchecked()}};data.residency=residency::Residency::Absent;}
    ResourceNeed::Complete=>return Ok(()),
    ResourceNeed::Next=>{}
   }
  }
 }
}
