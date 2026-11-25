//! Path queries borrow original positions, models and condition facts.
use super::*;
#[derive(Clone,Copy)]
#[repr(C)]struct Cursor{phase:u32,index:u64}
#[repr(u32)]enum ResidencyNeed{Inspect,Query,Yes,No,Left,Right,Conditional,Copy,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)]struct ResidencyFacts{selected:u32,truth:u32}
#[repr(C)]struct ResidencyStep{cursor:Cursor,need:ResidencyNeed}
#[repr(u32)]enum ModelNeed{Inspect,Ownership,Residency,Initialization,Child,Recurse,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)]struct ModelFacts{kind:u32,initialization:u32,present:u32,count:u64}
#[repr(C)]struct ModelStep{cursor:Cursor,need:ModelNeed}
#[repr(u32)]enum MetadataNeed{Inspect,Availability,ResolvedAvailability,Initialize,Forget,Child,Project,Recurse,Complete,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)]struct MetadataFacts{kind:u32,forget:u32,presence:u32,available:u32,place:u32,owns:u32,resolved:u32,count:u64}
#[repr(C)]struct MetadataStep{cursor:Cursor,need:MetadataNeed}
#[repr(u32)]enum IntersectNeed{Contains,Child,Yes,No,Next}
#[derive(Clone,Copy,Default)]
#[repr(C)]struct IntersectFacts{hit:u32,indexed:u32,child:u32,count:u64}
#[repr(C)]struct IntersectStep{cursor:Cursor,need:IntersectNeed}
unsafe extern "C"{
 #[link_name="bootstrap.ownership_path::start"]fn b_start()->Cursor;
 #[link_name="bootstrap.ownership_path::ownership"]fn b_ownership(kind:u32,truth:u32)->u32;
 #[link_name="bootstrap.ownership_path::possible"]fn b_possible(truth:u32)->u32;
 #[link_name="bootstrap.ownership_path::residency_step"]fn b_residency_step(state:Cursor,f:ResidencyFacts)->ResidencyStep;
 #[link_name="bootstrap.ownership_path::model_step"]fn b_model_step(state:Cursor,f:ModelFacts)->ModelStep;
 #[link_name="bootstrap.ownership_path::metadata_step"]fn b_metadata_step(state:Cursor,f:MetadataFacts)->MetadataStep;
 #[link_name="bootstrap.ownership_path::intersect_step"]fn b_intersect_step(state:Cursor,f:IntersectFacts)->IntersectStep;
}
fn truth(value:Option<bool>)->u32{match value{Some(false)=>0,Some(true)=>1,None=>2}}
struct Assumption<'a>{value:Option<&'a Presence>,condition:Option<bounds::Condition>}
impl<'a> Assumption<'a>{
 fn new(value:Option<&'a Presence>)->Self{Self{value,condition:value.map(Presence::condition)}}
}
pub(in crate::check) fn ownership(builder:&Builder<'_>,value:&Ownership,assuming:Option<&Presence>)->Ownership{ownership_in(builder,value,assuming.map(Presence::condition).as_ref())}
pub(in crate::check) fn ownership_in(builder:&Builder<'_>,value:&Ownership,assuming:Option<&bounds::Condition>)->Ownership{
 let(kind,known)=match value{Ownership::Moved=>(0,2),Ownership::Owned=>(1,2),Ownership::Borrowed=>(2,2),Ownership::Conditional(when)=>(3,truth(builder.condition_truth(&when.condition(),assuming)))};
 match unsafe{b_ownership(kind,known)}{0=>Ownership::Moved,1=>Ownership::Owned,2=>Ownership::Borrowed,_=>value.clone()}
}
pub(in crate::check) fn possible(builder:&Builder<'_>,value:&Presence)->bool{unsafe{b_possible(truth(builder.condition_truth(&value.condition(),None)))!=0}}
pub(in crate::check) fn initialization(builder:&Builder<'_>,value:&Position,assuming:Option<&Presence>)->Position{ownership::resolve_initialization(builder,value,assuming,assuming.map(Presence::condition).as_ref())}
fn residency(value:&residency::Residency,query:&impl Fn(&bounds::Condition)->Option<bool>)->residency::Residency{
 let mut cursor=unsafe{b_start()};let mut facts=ResidencyFacts::default();let mut result=None;let mut left=None;let mut right=None;
 loop{
  let step=unsafe{b_residency_step(cursor,facts)};cursor=step.cursor;
  match step.need{
   ResidencyNeed::Inspect=>facts.selected=u32::from(matches!(value,residency::Residency::Selected(..))),
   ResidencyNeed::Query=>{let residency::Residency::Selected(when,..)=value else{unsafe{std::hint::unreachable_unchecked()}};facts.truth=truth(query(when));}
   ResidencyNeed::Yes=>{let residency::Residency::Selected(_,a,_)=value else{unsafe{std::hint::unreachable_unchecked()}};result=Some(residency(a,query));}
   ResidencyNeed::No=>{let residency::Residency::Selected(_,_,b)=value else{unsafe{std::hint::unreachable_unchecked()}};result=Some(residency(b,query));}
   // Residency preserves its original unknown-branch assumptions.
   ResidencyNeed::Left=>{let residency::Residency::Selected(_,a,_)=value else{unsafe{std::hint::unreachable_unchecked()}};left=Some(residency(a,query));}
   ResidencyNeed::Right=>{let residency::Residency::Selected(_,_,b)=value else{unsafe{std::hint::unreachable_unchecked()}};right=Some(residency(b,query));}
   ResidencyNeed::Conditional=>{let residency::Residency::Selected(when,..)=value else{unsafe{std::hint::unreachable_unchecked()}};result=Some(residency::Residency::conditional(when.as_ref().clone(),left.take().unwrap(),right.take().unwrap()));}
   ResidencyNeed::Copy=>{result=Some(value.clone());}
   ResidencyNeed::Complete=>return result.take().unwrap(),
   ResidencyNeed::Next=>{}
  }
 }
}

pub(in crate::check) fn resolve_residency(builder:&Builder<'_>,value:&residency::Residency,assuming:Option<&Presence>)->residency::Residency{
 residency_in(builder,value,assuming.map(Presence::condition).as_ref())
}
pub(in crate::check) fn residency_in(builder:&Builder<'_>,value:&residency::Residency,assuming:Option<&bounds::Condition>)->residency::Residency{
 residency(value,&|when|builder.condition_truth(when,assuming))
}
pub(in crate::check) fn resolve_model(builder:&Builder<'_>,value:&mut Returned,assuming:Option<&Presence>){model(builder,value,&Assumption::new(assuming));}
fn model(builder:&Builder<'_>,value:&mut Returned,assuming:&Assumption<'_>){
 let mut cursor=unsafe{b_start()};let mut facts=ModelFacts::default();
 loop{
  let step=unsafe{b_model_step(cursor,facts)};cursor=step.cursor;
  match step.need{
   ModelNeed::Inspect=>{
    facts.kind=value.kind();
    match value{Returned::Dataset(data)=>facts.initialization=u32::from(data.initialization.is_some()),Returned::OwnedValue(value)=>facts.count=value.payloads.len()as u64,_=>{}}
   }
   ModelNeed::Ownership=>match value{Returned::Dataset(data)=>data.ownership=ownership_in(builder,&data.ownership,assuming.condition.as_ref()),Returned::OwnedValue(value)=>value.ownership=ownership_in(builder,&value.ownership,assuming.condition.as_ref()),_=>unsafe{std::hint::unreachable_unchecked()}},
   ModelNeed::Residency=>{let Returned::Dataset(data)=value else{unsafe{std::hint::unreachable_unchecked()}};data.residency=residency_in(builder,&data.residency,assuming.condition.as_ref());}
   ModelNeed::Initialization=>{let Returned::Dataset(data)=value else{unsafe{std::hint::unreachable_unchecked()}};data.initialization=Some(ownership::resolve_initialization(builder,data.initialization.as_ref().unwrap(),assuming.value,assuming.condition.as_ref()));}
   ModelNeed::Child=>{let Returned::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};facts.present=u32::from(value.payloads[cursor.index as usize].is_some());}
   ModelNeed::Recurse=>{let Returned::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};model(builder,value.payloads[cursor.index as usize].as_deref_mut().unwrap(),assuming);}
   ModelNeed::Complete=>return,
   ModelNeed::Next=>{}
  }
 }
}
// This view contains only a borrow of the actual node; no model or fields are copied.
#[derive(Clone,Copy)]
enum ReadModel<'a>{Data(&'a application::Data),Owned(&'a owned_values::OwnedValuePayload),Other}
impl<'a> ReadModel<'a>{
 fn of(value:&'a Returned)->Self{match value{Returned::Dataset(data)=>Self::Data(data),Returned::OwnedValue(value)=>Self::Owned(value),_=>Self::Other}}
 fn kind(self)->u32{match self{Self::Data(_)=>2,Self::Owned(_)=>3,Self::Other=>0}}
 fn ownership(self)->&'a Ownership{match self{Self::Data(data)=>&data.ownership,Self::Owned(value)=>&value.ownership,Self::Other=>unsafe{std::hint::unreachable_unchecked()}}}
 fn ty(self,declared:Option<&'a Ty>)->Option<&'a Ty>{match self{Self::Owned(value)=>Some(declared.unwrap_or(&value.value.ty)),_=>None}}
}
fn metadata_place<'a>(value:ReadModel<'a>,inherited:Option<&'a Place>,field:bool)->Option<&'a Place>{
 match value{
  ReadModel::Data(data)=>inherited.or(data.place.as_ref()),
  ReadModel::Owned(value)=>if field{inherited}else{value.place.as_ref()},
  ReadModel::Other=>None,
 }
}
fn metadata(builder:&mut Builder<'_>,value:ReadModel<'_>,declared:Option<&Ty>,inherited:Option<&Place>,field:bool,forget:bool,resolved:bool){
 let mut cursor=unsafe{b_start()};let mut facts=MetadataFacts{forget:u32::from(forget),resolved:u32::from(resolved),..MetadataFacts::default()};
 let ty=value.ty(declared);let place=metadata_place(value,inherited,field);let mut child_place=None;
 loop{
  let step=unsafe{b_metadata_step(cursor,facts)};cursor=step.cursor;
  match step.need{
   MetadataNeed::Inspect=>{
    facts.kind=value.kind();facts.place=u32::from(place.is_some());
    if let Some(ty)=ty{facts.presence=u32::from(ty.has_owned_presence());facts.count=ty.aggregate_len()as u64;}
   }
   MetadataNeed::Availability=>facts.available=u32::from(value.ownership().available()),
   MetadataNeed::ResolvedAvailability=>facts.available=u32::from(ownership(builder,value.ownership(),None).available()),
   MetadataNeed::Initialize=>builder.initialize(place.unwrap()),
   MetadataNeed::Forget=>{
    let changed=place.unwrap().owned_positions().unwrap().into_iter().collect();
    builder.initialized.retain(|position|!intersects(position,&changed));
   }
   MetadataNeed::Child=>{facts.owns=u32::from(ty.unwrap().aggregate_part(cursor.index as usize).1.owns_resources());}
   MetadataNeed::Project=>{
    let ty=ty.unwrap();let(offset,child)=ty.aggregate_part(cursor.index as usize);
    let components=child.components.iter().map(|(_,at)|ty.components.iter().position(|(_,word)|*word==offset+at).unwrap()).collect::<Vec<_>>();
    child_place=Some(place.unwrap().clone().project(child.clone(),&components));
   }
   MetadataNeed::Recurse=>{
    let ReadModel::Owned(owner)=value else{unsafe{std::hint::unreachable_unchecked()}};
    let(_,child_ty)=ty.unwrap().aggregate_part(cursor.index as usize);
    let child=owner.payloads[cursor.index as usize].as_deref().unwrap();
    metadata(builder,ReadModel::of(child),Some(child_ty),child_place.as_ref(),true,forget,resolved);child_place=None;
   }
   MetadataNeed::Complete=>return,
   MetadataNeed::Next=>{}
  }
 }
}
fn intersects(value:&Position,changed:&BTreeSet<Position>)->bool{
 let mut cursor=unsafe{b_start()};let mut facts=IntersectFacts::default();
 loop{
  let step=unsafe{b_intersect_step(cursor,facts)};cursor=step.cursor;
  match step.need{
   IntersectNeed::Contains=>{facts.hit=u32::from(changed.contains(value));if let Position::Indexed(choices,_)=value{facts.indexed=1;facts.count=choices.len()as u64;}}
   IntersectNeed::Child=>{let Position::Indexed(choices,_)=value else{unsafe{std::hint::unreachable_unchecked()}};facts.child=u32::from(intersects(&choices[cursor.index as usize],changed));}
   IntersectNeed::Yes=>return true,
   IntersectNeed::No=>return false,
   IntersectNeed::Next=>{}
  }
 }
}
pub(super) fn prepare(builder:&mut Builder<'_>,value:&Returned){metadata(builder,ReadModel::of(value),None,None,false,false,false);}
pub(super) fn prepare_value(builder:&mut Builder<'_>,value:&owned_values::OwnedValuePayload){metadata(builder,ReadModel::Owned(value),None,None,false,false,false);}
pub(super) fn prepare_data(builder:&mut Builder<'_>,value:&application::Data){metadata(builder,ReadModel::Data(value),None,None,false,false,false);}
pub(super) fn prepare_selected_value(builder:&mut Builder<'_>,value:&owned_values::OwnedValuePayload){metadata(builder,ReadModel::Owned(value),None,None,false,false,true);}
pub(super) fn forget(builder:&mut Builder<'_>,value:&Returned){metadata(builder,ReadModel::of(value),None,None,false,true,false);}
pub(super) fn forget_value(builder:&mut Builder<'_>,value:&owned_values::OwnedValuePayload){metadata(builder,ReadModel::Owned(value),None,None,false,true,false);}
