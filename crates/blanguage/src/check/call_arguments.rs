//! Shared argument control consumes original typed products at their real scope.
use super::*;
use cooperative::{Argument,CallParameters};
use returns::Returned;
#[derive(Clone,Copy)]
#[repr(C)]
pub(super) struct Parameter{pub(super) access:Access,pub(super) form:DeclaredArgumentForm}
impl Parameter{
 fn read(parameters:CallParameters<'_>,index:usize)->Self{Self{access:parameters.access(index),form:parameters.form(index)}}
}
#[derive(Clone,Copy)]
#[repr(u32)]
enum Kind{BorrowedData,Acceleration,Texture,Value,Dataset,Data,Scoped,OwnedValue,OwnedReference,Tile,Atomic,Reference}
#[derive(Clone,Copy)]
#[repr(u32)]
enum BindingKind{Other,Owned,Data,OwnedValue,Value,DataReference,Atomic}
#[repr(C)]
struct Phase{phase:u32}
#[repr(C)]
struct Cursor{index:u64,phase:u32}
#[repr(C)]
struct BuildCursor{index:u64,phase:u32,retained:u32}
#[repr(C)]
struct CheckCursor{index:u64,part:u64,phase:u32}
#[derive(Clone,Copy)]
#[repr(C)]
struct ArgumentFacts{kind:Kind,owner:u32,scoped_dataset:u32,application_element:u32,sources:u64}
#[derive(Clone,Copy)]
#[repr(C)]
struct SourceFacts{present:u32,continuing:u32,dataset:u32}
#[derive(Clone,Copy)]
#[repr(C)]
struct BuildFacts{parameter:Parameter,kind:Kind,scoped_dataset:u32,present:u32,continuing:u32,declared:u32,held:u32}
#[derive(Clone,Copy)]
#[repr(C)]
struct HoldFacts{kind:Kind,owner:u32,application_element:u32,borrows:u32}
#[derive(Clone,Copy)]
#[repr(C)]
struct FinishFacts{kind:Kind,owner:u32,binding:BindingKind}
#[derive(Clone,Copy)]
#[repr(C)]
struct CollectFacts{kind:Kind,owner:u32,access:Access}
#[derive(Clone,Copy)]
#[repr(C)]
struct RetainFacts{present:u32,scoped:u32,kind:Kind}
#[derive(Clone,Copy)]
#[repr(C)]
struct CheckFacts{argument:ArgumentFacts,access:Access,resources:u64,paths:u64,tasks:u32}
#[repr(u32)]
enum SourceNeed{Acceleration,Texture,EvaluateDataset,ApplicationReference,DatasetReference,Scoped,Tile,Atomic,Mutable,Expected,Read,Value,ReturnAcceleration,InvalidAcceleration,ReturnDataset,MissingDataset,ReturnApplicationReference,ReturnDatasetReference,ReturnRead,ReturnValue,MissingValue,Empty}
#[repr(u32)]
enum BuildNeed{CountMismatch,Inspect,Before,Lower,Abandon,Append,OutputDeclared,PrepareOutput,Hold,Remember,Finish,Ready,Empty,ReadyRetained}
#[repr(u32)]
enum HoldNeed{ReadReference,ClearReference,DataReference,Reference,Atomic,Value,OwnedValue,ReadOwnedReference,BorrowOwnedReference,Data,Existing,None}
#[repr(u32)]
enum FinishNeed{Inspect,Remove,Discard,Reference,Data,OwnedValue,Value,Invalid,Complete}
#[repr(u32)]
enum CollectNeed{Inspect,Scoped,TransferOwned,TransferData,Complete}
#[repr(u32)]
enum RetainNeed{NextEnvironment,Environment,InspectArgument,Reference,Complete}
#[repr(u32)]
enum CheckNeed{Inspect,OwnedData,OwnedResources,OwnedResource,Invocation,MutableResources,MutableResource,Tasks,TaskPosition,DataSources,ResourcePaths,FieldResource,ReferenceSource,PlaceAccess,PlaceIdentity,PlaceSource,HandleExclusion,HandleAvailable,Aliases,Complete}
unsafe extern "C"{
 #[link_name="bootstrap.call_arguments::source_start"]
 fn source_start()->Phase;
 #[link_name="bootstrap.call_arguments::source_step"]
 fn source_step(cursor:*mut Phase,parameter:Parameter,facts:SourceFacts)->SourceNeed;
 #[link_name="bootstrap.call_arguments::build_start"]
 fn build_start(count:u64,parameters:u64)->BuildCursor;
 #[link_name="bootstrap.call_arguments::build_step"]
 fn build_step(cursor:*mut BuildCursor,count:u64,facts:BuildFacts)->BuildNeed;
 #[link_name="bootstrap.call_arguments::hold_start"]
 fn hold_start()->Phase;
 #[link_name="bootstrap.call_arguments::hold_step"]
 fn hold_step(cursor:*mut Phase,access:Access,facts:HoldFacts)->HoldNeed;
 #[link_name="bootstrap.call_arguments::finish_start"]
 fn finish_start()->Cursor;
 #[link_name="bootstrap.call_arguments::finish_step"]
 fn finish_step(cursor:*mut Cursor,count:u64,facts:FinishFacts)->FinishNeed;
 #[link_name="bootstrap.call_arguments::collect_start"]
 fn collect_start()->Cursor;
 #[link_name="bootstrap.call_arguments::collect_step"]
 fn collect_step(cursor:*mut Cursor,count:u64,facts:CollectFacts)->CollectNeed;
 #[link_name="bootstrap.call_arguments::retain_start"]
 fn retain_start()->Cursor;
 #[link_name="bootstrap.call_arguments::retain_step"]
 fn retain_step(cursor:*mut Cursor,count:u64,facts:RetainFacts)->RetainNeed;
 #[link_name="bootstrap.call_arguments::checks_start"]
 fn checks_start()->CheckCursor;
 #[link_name="bootstrap.call_arguments::checks_step"]
 fn checks_step(cursor:*mut CheckCursor,count:u64,facts:CheckFacts)->CheckNeed;
}

fn kind(argument:&Argument)->Kind{match argument{
 Argument::BorrowedData(_)=>Kind::BorrowedData,Argument::Acceleration(_)=>Kind::Acceleration,Argument::Texture(_)=>Kind::Texture,
 Argument::Value(_)=>Kind::Value,Argument::Dataset(_)=>Kind::Dataset,Argument::ApplicationData(_)=>Kind::Data,
 Argument::Scoped{..}=>Kind::Scoped,Argument::OwnedValue(_)=>Kind::OwnedValue,Argument::OwnedReference{..}=>Kind::OwnedReference,
 Argument::Tile(_)=>Kind::Tile,Argument::Atomic(_)=>Kind::Atomic,Argument::Reference(_)=>Kind::Reference,
}}
fn empty_facts()->ArgumentFacts{ArgumentFacts{kind:Kind::Value,owner:0,scoped_dataset:0,application_element:0,sources:0}}
fn argument_facts(argument:&Argument)->ArgumentFacts{
 let(application_element,sources)=match argument{
  Argument::BorrowedData(reference)=>(0,reference.sources.len()as u64),
  Argument::Reference(OwnedBinding{place,..})|Argument::Atomic(place)=>match place.storage(){
   Place::ApplicationElement{sources,..}=>(1,sources.len()as u64),_=>(0,0),
  },
  _=>(0,0),
 };
 ArgumentFacts{kind:kind(argument),owner:u32::from(matches!(argument,Argument::OwnedReference{owner:Some(_),..})),
  scoped_dataset:u32::from(matches!(argument,Argument::Scoped{value:Returned::Dataset(_),..})),application_element,sources}
}
fn binding_kind(binding:&Binding)->BindingKind{match binding{
 Binding::Owned(_)=>BindingKind::Owned,Binding::ApplicationData(_)=>BindingKind::Data,Binding::OwnedValue(_)=>BindingKind::OwnedValue,
 Binding::Value(_)=>BindingKind::Value,Binding::DataReference(_)=>BindingKind::DataReference,Binding::AtomicBorrow(_)=>BindingKind::Atomic,
 _=>BindingKind::Other,
}}
pub(super) async fn source(builder:&mut Builder<'_>,parameters:CallParameters<'_>,index:usize,parameter:Parameter,arg:&Expr)->Result<Option<Argument>>{
 let world=builder.world;let mut cursor=unsafe{source_start()};let mut facts=SourceFacts{present:0,continuing:0,dataset:0};
 let mut acceleration=None;let mut returned=None;let mut application=None;let mut dataset=None;let mut expected=None;let mut argument=None;
 loop{
  match unsafe{source_step(&mut cursor,parameter,facts)}{
   SourceNeed::Acceleration=>{
    acceleration=match arg.kind(){ExprKind::Name(name)=>match builder.env.get(name){Some(Binding::Acceleration(index))=>Some(*index),_=>None},_=>None};
    facts.present=u32::from(acceleration.is_some());
   }
   SourceNeed::ReturnAcceleration=>return Ok(Some(Argument::Acceleration(unsafe{acceleration.take().unwrap_unchecked()}))),
   SourceNeed::InvalidAcceleration=>return builder.fail(arg.span(),"B183","acceleration 借用需要已有资源名称"),
   SourceNeed::Texture=>return unsafe{world.call(builder.texture_argument(arg)).await}.map(|value|value.map(Argument::Texture)),
   SourceNeed::EvaluateDataset=>{
    returned=unsafe{world.call(builder.evaluate_returned(arg,None)).await}?;
    facts.continuing=u32::from(builder.flow.continues());facts.dataset=u32::from(matches!(returned,Some(Returned::Dataset(_))));
   }
   SourceNeed::ReturnDataset=>{
    let Some(Returned::Dataset(data))=returned.take()else{unsafe{std::hint::unreachable_unchecked()}};
    return Ok(Some(Argument::ApplicationData(data)));
   }
   SourceNeed::MissingDataset=>return builder.fail(arg.span(),"B224","dataset 值参数需要拥有的数据集"),
   SourceNeed::ApplicationReference=>{
    application=unsafe{world.call(builder.application_reference(arg)).await}?;facts.present=u32::from(application.is_some());
   }
   SourceNeed::ReturnApplicationReference=>return Ok(application.take().map(Argument::BorrowedData)),
   SourceNeed::DatasetReference=>{dataset=builder.dataset_reference(arg)?;facts.present=u32::from(dataset.is_some());}
   SourceNeed::ReturnDatasetReference=>return Ok(dataset.take().map(Argument::Dataset)),
   // scoped_argument owns the completed flow/dataset distinction; common lowering
   // consumes its actual Option without repeating that distinction and unwrap.
   SourceNeed::Scoped=>return unsafe{world.call(builder.scoped_argument(arg)).await},
   SourceNeed::Tile=>return builder.tile_view(arg).map(|value|Some(Argument::Tile(value))),
   SourceNeed::Atomic=>return unsafe{world.call(builder.atomic_place(arg)).await}.map(|value|value.map(Argument::Atomic)),
   SourceNeed::Mutable=>return unsafe{world.call(builder.mutable_argument(arg,parameter.access)).await},
   SourceNeed::Expected=>{expected=unsafe{world.call(parameters.expected(world,&builder.scope,index)).await}?;}
   SourceNeed::Read=>{
    argument=unsafe{world.call(builder.read_argument(arg,expected.as_ref())).await}?;
    facts.present=u32::from(argument.is_some());facts.continuing=u32::from(builder.flow.continues());
   }
   SourceNeed::Value=>{
    returned=unsafe{world.call(builder.evaluate_returned(arg,expected.as_ref())).await}?;
    facts.present=u32::from(returned.as_ref().is_some_and(|value|!matches!(value,Returned::Unit)));facts.continuing=u32::from(builder.flow.continues());
   }
   SourceNeed::ReturnRead=>return Ok(argument.take()),
   SourceNeed::ReturnValue=>return Ok(returned.take().map(Argument::resource)),
   SourceNeed::MissingValue=>return builder.fail(arg.span(),"B228","函数实参需要值或拥有资源"),
   SourceNeed::Empty=>return Ok(None),
  }
 }
}
pub(super) async fn build<'a>(
 builder:&mut Builder<'a>,parameters:CallParameters<'_>,count:usize,span:Span,
 argument_span:impl Fn(usize)->Span,
 mut lower:impl std::ops::AsyncFnMut(&mut Builder<'a>,usize,Parameter)->Result<Option<Argument>>
)->Result<Option<Vec<Argument>>>{
 let world=builder.world;let mut cursor=unsafe{build_start(count as u64,parameters.len()as u64)};
 let mut facts=BuildFacts{parameter:Parameter{access:Access::Value,form:DeclaredArgumentForm::Ordinary},kind:Kind::Value,scoped_dataset:0,present:0,continuing:0,declared:0,held:0};
 let mut arguments=Vec::new();let mut held=Vec::new();let mut lowered=None;let mut name=None;let mut current_span=span;
 let retain=loop{
  match unsafe{build_step(&mut cursor,count as u64,facts)}{
   BuildNeed::CountMismatch=>return builder.fail(span,"B162","函数参数数量不匹配"),
   BuildNeed::Inspect=>{facts.parameter=Parameter::read(parameters,cursor.index as usize);current_span=argument_span(cursor.index as usize);}
   BuildNeed::Before=>builder.view_uses.before(current_span),
   BuildNeed::Lower=>{
    lowered=lower(builder,cursor.index as usize,facts.parameter).await?;
    facts.present=u32::from(lowered.is_some());facts.continuing=u32::from(builder.flow.continues());
    facts.kind=lowered.as_ref().map_or(Kind::Value,kind);
    facts.scoped_dataset=u32::from(matches!(lowered.as_ref(),Some(Argument::Scoped{value:Returned::Dataset(_),..})));
   }
   BuildNeed::Abandon=>builder.abandon_arguments(&held),
   BuildNeed::Empty=>return Ok(None),
   BuildNeed::Append=>arguments.push(unsafe{lowered.take().unwrap_unchecked()}),
   BuildNeed::OutputDeclared=>{facts.declared=u32::from(parameters.declares_output(cursor.index as usize));}
   BuildNeed::PrepareOutput=>{
    let Some(Argument::BorrowedData(data))=arguments.last_mut()else{unsafe{std::hint::unreachable_unchecked()}};
    unsafe{world.call(builder.prepare_data_output(&mut data.data,current_span)).await}?;
   }
   BuildNeed::Hold=>{
    name=unsafe{world.call(hold(builder,arguments.last_mut().unwrap(),facts.parameter.access)).await}?;
    facts.held=u32::from(name.is_some());
   }
   BuildNeed::Remember=>held.push((arguments.len()-1,unsafe{name.take().unwrap_unchecked()})),
   BuildNeed::Finish=>finish(builder,&held,&mut arguments),
   BuildNeed::Ready=>break false,
   BuildNeed::ReadyRetained=>break true,
  }
 };
 builder.view_uses.after(span);
 let retained=if retain{Some(unsafe{world.call(retained(builder,parameters,&arguments)).await}?)}else{None};
 unsafe{world.call(checks(builder,parameters,&arguments,retained.as_ref(),span,&argument_span)).await}?;
 Ok(Some(arguments))
}


async fn hold(builder:&mut Builder<'_>,argument:&mut Argument,access:Access)->Result<Option<String>>{
 let world=builder.world;let mut cursor=unsafe{hold_start()};
 let facts=HoldFacts{kind:kind(argument),owner:u32::from(matches!(argument,Argument::OwnedReference{owner:Some(_),..})),application_element:u32::from(matches!(argument,Argument::Atomic(place)if matches!(place.storage(),Place::ApplicationElement{..}))),borrows:u32::from(matches!(argument,Argument::Value(value)if value.ty.contains_borrows()))};
 loop{
  let binding=match unsafe{hold_step(&mut cursor,access,facts)}{
   HoldNeed::ReadReference=>{
    let Argument::Reference(owner)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    let regs=owner.place.ty().components.iter().map(|(scalar,_)|builder.reg(*scalar)).collect::<Vec<_>>();
    let(proof,uniform,_)=unsafe{world.call(builder.read_owned_proof(&owner.place,&regs,&[])).await}?;
    owner.proof=proof;owner.uniform=uniform;continue;
   }
   HoldNeed::ClearReference=>{
    let Argument::Reference(owner)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    owner.proof=bounds::Proof::default();continue;
   }
   HoldNeed::DataReference=>{
    let Argument::BorrowedData(value)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    Binding::DataReference(value.clone())
   }
   HoldNeed::Reference=>{
    let Argument::Reference(value)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    Binding::Owned(value.clone())
   }
   HoldNeed::Atomic=>{
    let Argument::Atomic(value)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    Binding::AtomicBorrow(value.clone())
   }
   HoldNeed::Value=>{
    let Argument::Value(value)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    Binding::Value(value.clone())
   }
   HoldNeed::OwnedValue=>{
    let Argument::OwnedValue(value)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    Binding::OwnedValue(value.clone())
   }
   HoldNeed::ReadOwnedReference=>{
    let Argument::OwnedReference{place,..}=argument else{unsafe{std::hint::unreachable_unchecked()}};
    Binding::Owned(builder.borrow_owned(place.clone()))
   }
   HoldNeed::BorrowOwnedReference=>{
    let Argument::OwnedReference{value,..}=argument else{unsafe{std::hint::unreachable_unchecked()}};
    let mut value=value.clone();value.borrow();Binding::OwnedValue(value)
   }
   HoldNeed::Data=>{
    let Argument::ApplicationData(value)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    Binding::ApplicationData(value.clone())
   }
   HoldNeed::Existing=>{
    let name=match argument{
     Argument::Scoped{owner,..}=>owner,Argument::OwnedReference{owner:Some(owner),..}=>owner,
     _=>unsafe{std::hint::unreachable_unchecked()},
    };
    return Ok(Some(name.clone()));
   }
   HoldNeed::None=>return Ok(None),
  };
  return Ok(Some(builder.hold_binding(binding)));
 }
}
fn finish(builder:&mut Builder<'_>,held:&[(usize,String)],arguments:&mut[Argument]){
 let mut cursor=unsafe{finish_start()};let mut facts=FinishFacts{kind:Kind::Value,owner:0,binding:BindingKind::Other};
 let mut binding=None;
 loop{
  let need=unsafe{finish_step(&mut cursor,held.len()as u64,facts)};
  if matches!(need,FinishNeed::Complete){return;}
  let(index,name)=&held[cursor.index as usize];
  match need{
   FinishNeed::Inspect=>{facts.kind=kind(&arguments[*index]);facts.owner=u32::from(matches!(arguments[*index],Argument::OwnedReference{owner:Some(_),..}));}
   FinishNeed::Remove=>{
    binding=Some(builder.env.remove(name).unwrap());facts.binding=binding_kind(binding.as_ref().unwrap());
   }
   FinishNeed::Discard=>{binding.take();}
   FinishNeed::Reference=>{
    let Some(Binding::Owned(value))=binding.take()else{unsafe{std::hint::unreachable_unchecked()}};
    arguments[*index]=Argument::Reference(value);
   }
   FinishNeed::Data=>{
    let Some(Binding::ApplicationData(value))=binding.take()else{unsafe{std::hint::unreachable_unchecked()}};
    arguments[*index]=Argument::ApplicationData(value);
   }
   FinishNeed::OwnedValue=>{
    let Some(Binding::OwnedValue(value))=binding.take()else{unsafe{std::hint::unreachable_unchecked()}};
    arguments[*index]=Argument::OwnedValue(value);
   }
   FinishNeed::Value=>{
    let Some(Binding::Value(value))=binding.take()else{unsafe{std::hint::unreachable_unchecked()}};
    arguments[*index]=Argument::Value(value);
   }
   FinishNeed::Invalid=>unsafe{std::hint::unreachable_unchecked()},
   FinishNeed::Complete=>unsafe{std::hint::unreachable_unchecked()},
  }
 }
}


async fn retained(builder:&Builder<'_>,parameters:CallParameters<'_>,arguments:&[Argument])->Result<view::Borrow>{
 let world=builder.world;let mut scoped=BTreeSet::new();let mut transferred=Vec::new();
 let mut cursor=unsafe{collect_start()};let mut facts=CollectFacts{kind:Kind::Value,owner:0,access:Access::Value};
 loop{
  let need=unsafe{collect_step(&mut cursor,arguments.len()as u64,facts)};
  if matches!(need,CollectNeed::Complete){break;}
  let argument=&arguments[cursor.index as usize];
  match need{
   CollectNeed::Inspect=>{facts.kind=kind(argument);facts.owner=u32::from(matches!(argument,Argument::OwnedReference{owner:Some(_),..}));facts.access=parameters.access(cursor.index as usize);}
   CollectNeed::Scoped=>{
    let name=match argument{Argument::Scoped{owner,..}|Argument::OwnedReference{owner:Some(owner),..}=>owner.as_str(),_=>unsafe{std::hint::unreachable_unchecked()}};
    scoped.insert(name);
   }
   CollectNeed::TransferOwned=>{
    let Argument::OwnedReference{value,place,..}=argument else{unsafe{std::hint::unreachable_unchecked()}};
    transferred.push(view_liveness::Transfer::Owned{value,place:Some(place)});
   }
   CollectNeed::TransferData=>{
    let Argument::BorrowedData(reference)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    transferred.push(view_liveness::Transfer::BorrowedData(reference));
   }
   CollectNeed::Complete=>unsafe{std::hint::unreachable_unchecked()},
  }
 }
 let mut retained=view::Borrow::default();let mut cursor=unsafe{retain_start()};
 let mut facts=RetainFacts{present:0,scoped:0,kind:Kind::Value};let mut environment=builder.env.iter();let mut current=None;
 loop{
  match unsafe{retain_step(&mut cursor,arguments.len()as u64,facts)}{
   RetainNeed::NextEnvironment=>{
    current=environment.next();facts.present=u32::from(current.is_some());
    facts.scoped=u32::from(current.is_some_and(|(name,_)|scoped.contains(name.as_str())));
   }
   RetainNeed::Environment=>{
    let(name,binding)=unsafe{current.unwrap_unchecked()};
    retained.union(&unsafe{world.call(builder.view_uses.borrow(builder,name,binding,&transferred)).await}?);
   }
   RetainNeed::InspectArgument=>{facts.kind=kind(&arguments[cursor.index as usize]);}
   RetainNeed::Reference=>{
    let Argument::Reference(owner)=&arguments[cursor.index as usize]else{unsafe{std::hint::unreachable_unchecked()}};
    retained.union(&owner.proof.typed_view_borrow(owner.place.ty()));
   }
   RetainNeed::Complete=>return Ok(retained),
  }
 }
}
fn place(argument:&Argument)->&Place{match argument{
 Argument::Reference(owner)=>&owner.place,Argument::Atomic(place)=>place,_=>unsafe{std::hint::unreachable_unchecked()},
}}
fn data_handle(argument:&Argument)->ValueIdentity{match argument{
 Argument::ApplicationData(data)|Argument::Scoped{value:Returned::Dataset(data),..}=>data.root.identity(),
 _=>unsafe{std::hint::unreachable_unchecked()},
}}
async fn checks(builder:&mut Builder<'_>,parameters:CallParameters<'_>,arguments:&[Argument],retained:Option<&view::Borrow>,
 span:Span,argument_span:&impl Fn(usize)->Span)->Result<()>{
 let world=builder.world;let mut cursor=unsafe{checks_start()};
 let mut facts=CheckFacts{argument:empty_facts(),access:Access::Value,resources:0,paths:0,tasks:0};
 let mut resources=Vec::new();let mut tasks=view::Borrow::default();let mut paths=Vec::new();
 loop{
  let need=unsafe{checks_step(&mut cursor,arguments.len()as u64,facts)};
  match need{
   CheckNeed::Complete=>return Ok(()),
   CheckNeed::Invocation=>{builder.view_uses.invocation(span);continue;}
   CheckNeed::Aliases=>{
    builder.check_argument_aliases(|index|(parameters.access(index),parameters.form(index)==DeclaredArgumentForm::Dataset),arguments,argument_span)?;
    continue;
   }
   _=>{}
  }
  let argument=&arguments[cursor.index as usize];
  match need{
   CheckNeed::Inspect=>{facts.argument=argument_facts(argument);facts.access=parameters.access(cursor.index as usize);}
   CheckNeed::OwnedData=>{
    let Argument::ApplicationData(data)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    builder.check_view_requirement(view::Requirement::Unborrowed{resource:builder.owned_resource_borrow(data),
     borrow:unsafe{retained.unwrap_unchecked()}.clone(),kind:view::Exclusion::Ownership,span})?;
   }
   CheckNeed::OwnedResources=>{
    let Argument::OwnedValue(value)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    resources=value.resources();facts.resources=resources.len()as u64;
   }
   CheckNeed::OwnedResource=>{
    builder.check_view_requirement(view::Requirement::Unborrowed{resource:builder.owned_resource_borrow(resources[cursor.part as usize]),
     borrow:unsafe{retained.unwrap_unchecked()}.clone(),kind:view::Exclusion::Ownership,span})?;
   }
   CheckNeed::MutableResources=>{
    let Argument::OwnedReference{value,..}=argument else{unsafe{std::hint::unreachable_unchecked()}};
    resources=value.resources();facts.resources=resources.len()as u64;
   }
   CheckNeed::MutableResource=>{
    builder.check_view_requirement(view::Requirement::Unborrowed{resource:builder.data_value_borrow(resources[cursor.part as usize]),
     borrow:unsafe{retained.unwrap_unchecked()}.clone(),kind:view::Exclusion::Ownership,span})?;
   }
   CheckNeed::Tasks=>{tasks=builder.task_places();facts.tasks=u32::from(!tasks.empty());}
   CheckNeed::TaskPosition=>{
    let Argument::BorrowedData(reference)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    let(offset,ty)=reference.element_layout();let resource=application::dataset_origin(&reference.data,offset,ty,facts.access.reads()).normalize();
    builder.check_view_requirement(view::Requirement::Unborrowed{resource,borrow:std::mem::take(&mut tasks),kind:view::Exclusion::Position(facts.access),span})?;
   }
   CheckNeed::DataSources=>{
    let Argument::BorrowedData(reference)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    builder.check_view_requirement(view::Requirement::Unborrowed{resource:builder.data_borrow(&reference.sources),
     borrow:unsafe{retained.unwrap_unchecked()}.clone(),kind:view::Exclusion::View,span})?;
   }
   CheckNeed::ResourcePaths=>{
    let Argument::BorrowedData(reference)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    paths=view::resource_paths(reference.element_layout().1);facts.paths=paths.len()as u64;
   }
   CheckNeed::FieldResource=>{
    let Argument::BorrowedData(reference)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    // Extend the original owned path into its actual final consumer.
    let mut fields=std::mem::take(&mut paths[cursor.part as usize]);
    drop(fields.splice(0..0,reference.projection.iter().copied()));
    let resource=view::Borrow::element(builder.data_value_borrow(&reference.data),fields,None);
    builder.check_view_requirement(view::Requirement::Unborrowed{resource,borrow:unsafe{retained.unwrap_unchecked()}.clone(),kind:view::Exclusion::Ownership,span})?;
   }
   CheckNeed::ReferenceSource=>{
    let Argument::BorrowedData(reference)=argument else{unsafe{std::hint::unreachable_unchecked()}};
    builder.application_handle_available(reference.sources[cursor.part as usize],span,&[],&builder.path)?;
   }
   CheckNeed::PlaceAccess=>{unsafe{world.call(builder.require_place_access(place(argument),facts.access,span)).await}?;}
   CheckNeed::PlaceIdentity=>{
    let Place::ApplicationElement{identity,..}=place(argument).storage()else{unsafe{std::hint::unreachable_unchecked()}};
    builder.application_handle_available(*identity,span,&[],&builder.path)?;
   }
   CheckNeed::PlaceSource=>{
    let Place::ApplicationElement{sources,..}=place(argument).storage()else{unsafe{std::hint::unreachable_unchecked()}};
    builder.application_handle_available(sources[cursor.part as usize],span,&[],&builder.path)?;
   }
   CheckNeed::HandleExclusion=>{
    let handle=data_handle(argument);
    builder.check_view_requirement(view::Requirement::Unborrowed{resource:builder.data_borrow(&[handle]),
     borrow:unsafe{retained.unwrap_unchecked()}.clone(),kind:view::Exclusion::Resource,span})?;
   }
   CheckNeed::HandleAvailable=>builder.application_handle_available(data_handle(argument),span,&[],&builder.path)?,
   CheckNeed::Complete|CheckNeed::Invocation|CheckNeed::Aliases=>unsafe{std::hint::unreachable_unchecked()},
  }
 }
}
