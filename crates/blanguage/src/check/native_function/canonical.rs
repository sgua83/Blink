//! B owns native interface canonicalization; original typed products remain here.
use super::*;
use std::{ffi::c_void,ptr};
type Bindings=BTreeMap<ValueIdentity,bounds::Proof>;
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum CanonicalNeed{Seed,Parameters,Values,Elements,Sources,BindSources,Resources,Memory,Models,Names,Initialization,PublishNames,PrefixBindings,Translate,Complete}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum OutputMode{Values,Sources,Resources,Models,Translate}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum OutputNeed{Inspect,Value,Sources,Resources,Model,Empty,Translate,Prefix,Continue,Complete}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum SourceAccess{Value,Read,InOut,Other}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum ResourceKind{None,Application,Owned}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum SourceNeed{Next,Bound,Storage,InspectContent,Content,InspectResources,Resource,Continue,Complete}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum MemoryKind{None,Snapshot,Dataset}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum MemoryNeed{Next,Bind,Continue,Complete}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum ModelNeed{Capture,Rebind,Register,Identity,Paths,Root,BorrowedName,OwnedName,Element,InspectNested,Nested,RootIdentities,ReturnedName,WrittenName,Continue,Complete}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum InitialNeed{Next,Keep,Incomplete,Complete}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum BoundNeed{Next,When,Bind,Yield,Continue,Complete}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum PrefixParameterNeed{Next,Bound,Bind,Continue,Complete}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum Kind{Unit,Value,Owned,Data}
#[derive(Clone,Copy,PartialEq,Eq)]#[repr(u32)]enum PrefixNeed{Facts,Elements,Inspect,Payload,Continue,Complete}

#[derive(Clone,Copy)]#[repr(C)]struct Cursor{phase:u32,index:u64,part:u64}
#[repr(C)]struct Step<T>{cursor:Cursor,need:T}
#[derive(Clone,Copy)]#[repr(C)]struct OutputFacts{present:u32,view:u32,count:u64}
#[derive(Clone,Copy)]#[repr(C)]struct SourceFacts{present:u32,access:SourceAccess,dataset:u32,tile:u32,resource:ResourceKind,content:u64,resources:u64}
#[derive(Clone,Copy)]#[repr(C)]struct MemoryFacts{present:u32,kind:MemoryKind,width:u64}
#[derive(Clone,Copy)]#[repr(C)]struct ModelFacts{present:u32,known:u32,root:u32,borrowed:u32,nested:u32,width:u64,paths:u64,depth:u64,base:u64,model_index:u64}
#[derive(Clone,Copy)]#[repr(C)]struct InitialFacts{present:u32,position:u32,pending:u32}
#[derive(Clone,Copy)]#[repr(C)]struct BoundFacts{present:u32,local:u32}
#[derive(Clone,Copy)]#[repr(C)]struct PrefixParameterFacts{present:u32,origin:u32,reference:u32,dataset:u32,width:u64}
#[derive(Clone,Copy)]#[repr(C)]struct PrefixFacts{kind:Kind,present:u32,payloads:u64}
#[repr(C)]struct Index{present:u32,value:u64}
#[repr(C)]struct Completion{present:u32,address:u64,components:u64,tasks:u64}
#[repr(C)]struct Known{present:u32,known:u32}
#[repr(C)]struct SeedHost{
 count:unsafe extern "C" fn(*const c_void)->u64,input:unsafe extern "C" fn(*const c_void,u64)->u64,
 prepare_returns:unsafe extern "C" fn(*mut c_void),next_return:unsafe extern "C" fn(*mut c_void)->Index,next_completion:unsafe extern "C" fn(*mut c_void)->Completion,
 input_binding:unsafe extern "C" fn(*mut c_void,u64,u64),return_binding:unsafe extern "C" fn(*mut c_void,u64),completion_binding:unsafe extern "C" fn(*mut c_void,u64,u64,u64),
}
#[repr(C)]struct BindingHost{next:unsafe extern "C" fn(*mut c_void)->Known,apply:unsafe extern "C" fn(*mut c_void)}
unsafe extern "C"{
 #[link_name="bootstrap.native_canonical::start"]fn start()->Cursor;
 #[link_name="bootstrap.native_canonical::canonical_step"]fn canonical_step(cursor:Cursor)->Step<CanonicalNeed>;
 #[link_name="bootstrap.native_canonical::output_step"]fn output_step(cursor:Cursor,mode:OutputMode,facts:OutputFacts)->Step<OutputNeed>;
 #[link_name="bootstrap.native_canonical::source_step"]fn source_step(cursor:Cursor,facts:SourceFacts)->Step<SourceNeed>;
 #[link_name="bootstrap.native_canonical::memory_step"]fn memory_step(cursor:Cursor,facts:MemoryFacts)->Step<MemoryNeed>;
 #[link_name="bootstrap.native_canonical::model_step"]fn model_step(cursor:Cursor,facts:ModelFacts)->Step<ModelNeed>;
 #[link_name="bootstrap.native_canonical::initial_step"]fn initial_step(cursor:Cursor,facts:InitialFacts)->Step<InitialNeed>;
 #[link_name="bootstrap.native_canonical::bound_step"]fn bound_step(cursor:Cursor,facts:BoundFacts)->Step<BoundNeed>;
 #[link_name="bootstrap.native_canonical::prefix_parameter_step"]fn prefix_parameter_step(cursor:Cursor,facts:PrefixParameterFacts)->Step<PrefixParameterNeed>;
 #[link_name="bootstrap.native_canonical::prefix_step"]fn prefix_step(cursor:Cursor,facts:PrefixFacts)->Step<PrefixNeed>;
 #[link_name="bootstrap.native_canonical::seed"]fn seed(context:*mut c_void,task_source:u32,returned_view:u32,host:SeedHost);
 #[link_name="bootstrap.native_canonical::apply_known"]fn apply_known(context:*mut c_void,host:BindingHost);
}
fn identity(id:ValueIdentity)->bounds::Proof{bounds::Proof::default().with_identity(id)}
fn with_borrow(mut proof:bounds::Proof,borrow:view::Borrow)->bounds::Proof{proof.set_view_borrow(0,borrow);proof}
struct Seed<'a>{
 inputs:&'a [Reg],returned:Option<&'a Returned>,returns:Option<std::vec::IntoIter<usize>>,
 completions:std::iter::Flatten<std::collections::btree_map::Values<'a,TaskCompletionSource,Vec<(Ty,[Reg;2])>>>,bindings:&'a mut Bindings,
}
unsafe extern "C" fn input_count(context:*const c_void)->u64{unsafe{&*context.cast::<Seed<'_>>()}.inputs.len()as u64}
unsafe extern "C" fn input(context:*const c_void,index:u64)->u64{unsafe{&*context.cast::<Seed<'_>>()}.inputs[index as usize]as u64}
unsafe extern "C" fn prepare_returns(context:*mut c_void){let c=unsafe{&mut *context.cast::<Seed<'_>>()};c.returns=Some(unsafe{c.returned.and_then(Returned::view_value).unwrap_unchecked()}.ty.task_components().into_iter());}
unsafe extern "C" fn next_return(context:*mut c_void)->Index{let c=unsafe{&mut *context.cast::<Seed<'_>>()};match unsafe{c.returns.as_mut().unwrap_unchecked()}.next(){Some(index)=>Index{present:1,value:index as u64},None=>Index{present:0,value:0}}}
unsafe extern "C" fn next_completion(context:*mut c_void)->Completion{let c=unsafe{&mut *context.cast::<Seed<'_>>()};match c.completions.next(){Some((result,address))=>Completion{present:1,address:address[0]as u64,components:result.components.len()as u64,tasks:result.task_components().len()as u64},None=>Completion{present:0,address:0,components:0,tasks:0}}}
unsafe extern "C" fn input_binding(context:*mut c_void,source:u64,target:u64){unsafe{&mut *context.cast::<Seed<'_>>()}.bindings.insert(ValueIdentity::Register(source as usize),identity(ValueIdentity::Register(target as usize)));}
unsafe extern "C" fn return_binding(context:*mut c_void,component:u64){let id=ValueIdentity::TaskReturnSource(component as usize);unsafe{&mut *context.cast::<Seed<'_>>()}.bindings.insert(id,identity(id));}
unsafe extern "C" fn completion_binding(context:*mut c_void,address:u64,component:u64,input:u64){unsafe{&mut *context.cast::<Seed<'_>>()}.bindings.insert(ValueIdentity::TaskCompletion(address as usize,component as usize),identity(ValueIdentity::TaskCompletion(input as usize,component as usize)));}
fn seed_bindings(builder:&Builder<'_>,returned:Option<&Returned>,inputs:&[Reg],bindings:&mut Bindings){
 let view=u32::from(returned.and_then(Returned::view_value).is_some());let mut context=Seed{inputs,returned,returns:None,completions:builder.task_completions.values().flatten(),bindings};
 unsafe{seed(ptr::from_mut(&mut context).cast(),u32::from(builder.task_return_source.is_some()),view,SeedHost{count:input_count,input,prepare_returns,next_return,next_completion,input_binding,return_binding,completion_binding})};
}
async fn source_bindings(builder:&Builder<'_>,bindings:&mut Bindings)->Result<()>{
 let mut cursor=unsafe{start()};let mut facts=SourceFacts{present:0,access:SourceAccess::Other,dataset:0,tile:0,resource:ResourceKind::None,content:0,resources:0};
 let mut parameters=builder.scope.parameters();let mut query=None;let mut bound:Option<Param>=None;
 loop{let step=unsafe{source_step(cursor,facts)};cursor=step.cursor;let index=cursor.index as usize;match step.need{
  SourceNeed::Next=>{bound=None;query=parameters.next();facts.present=u32::from(query.is_some());}
  SourceNeed::Bound=>{let parameter=unsafe{query.take().unwrap_unchecked()};let p=unsafe{builder.world.call(parameter.bound(builder.world)).await}?;facts.access=match p.access{Access::Value=>SourceAccess::Value,Access::Read=>SourceAccess::Read,Access::InOut=>SourceAccess::InOut,_=>SourceAccess::Other};facts.dataset=u32::from(p.ty.dataset);facts.tile=u32::from(p.ty.tile);bound=Some(p);}
  SourceNeed::Storage=>{let id=ValueIdentity::ParameterStorage(index);bindings.insert(id,with_borrow(identity(id),view::Borrow::source(view::Source::Parameter(id,Vec::new()))));}
  SourceNeed::InspectContent=>{let p=unsafe{bound.as_ref().unwrap_unchecked()};facts.content=builder.env.get(p.name.as_str()).and_then(Binding::stored_place).map(|place|place.ty().components.len()).unwrap_or(0)as u64;}
  SourceNeed::Content=>{let id=ValueIdentity::ParameterContent(index,cursor.part as usize);bindings.insert(id,identity(id));}
  SourceNeed::InspectResources=>{let p=unsafe{bound.as_ref().unwrap_unchecked()};let(kind,count)=match builder.env.get(p.name.as_str()){Some(Binding::ApplicationData(_))=>(ResourceKind::Application,0),Some(Binding::OwnedValue(value))=>(ResourceKind::Owned,view::resource_paths(&value.value.ty).len()),_=>(ResourceKind::None,0)};facts.resource=kind;facts.resources=count as u64;}
  SourceNeed::Resource=>{let id=ValueIdentity::ResourceParameter(index,cursor.part as usize);bindings.insert(id,with_borrow(identity(id),view::Borrow::source(view::Source::Data(id))));}
  SourceNeed::Continue=>{},
  SourceNeed::Complete=>return Ok(()),
 }}
}
fn memory_bindings(builder:&Builder<'_>,bindings:&mut Bindings){
 let mut cursor=unsafe{start()};let mut facts=MemoryFacts{present:0,kind:MemoryKind::None,width:0};let mut uses=builder.view_model.uses.iter();
 loop{let step=unsafe{memory_step(cursor,facts)};cursor=step.cursor;match step.need{
  MemoryNeed::Next=>{let usage=uses.next();facts.present=u32::from(usage.is_some());let(kind,width)=match usage.map(|usage|&usage.requirement){Some(view::Requirement::Memory{snapshots:Some(snapshots),..})=>(MemoryKind::Snapshot,snapshots.len()),Some(view::Requirement::Dataset{values:Some(_),..})=>(MemoryKind::Dataset,0),_=>(MemoryKind::None,0)};facts.kind=kind;facts.width=width as u64;}
  MemoryNeed::Bind=>{let id=ValueIdentity::MemoryValue(cursor.index as usize,cursor.part as usize);bindings.insert(id,identity(id));}
  MemoryNeed::Continue=>{},
  MemoryNeed::Complete=>return,
 }}
}

struct ElementSources<'a>{iter:std::collections::btree_set::IntoIter<ValueIdentity>,current:Option<(ValueIdentity,ValueIdentity)>,bindings:&'a mut Bindings}
unsafe extern "C" fn element_source_next(context:*mut c_void)->Known{
 let c=unsafe{&mut *context.cast::<ElementSources<'_>>()};let source=c.iter.next();c.current=source.and_then(|id|c.bindings.get(&id).and_then(|proof|proof.known_identity(0)).map(|target|(id,target)));Known{present:u32::from(source.is_some()),known:u32::from(c.current.is_some())}
}
unsafe extern "C" fn element_source_apply(context:*mut c_void){
 let c=unsafe{&mut *context.cast::<ElementSources<'_>>()};let(source,target)=unsafe{c.current.unwrap_unchecked()};let proof=unsafe{c.bindings.get_mut(&source).unwrap_unchecked()};proof.set_view_borrow(0,view::Borrow::source(view::Source::Parameter(target,Vec::new())));
}
fn bind_element_sources(sources:BTreeSet<ValueIdentity>,bindings:&mut Bindings){
 let mut context=ElementSources{iter:sources.into_iter(),current:None,bindings};unsafe{apply_known(ptr::from_mut(&mut context).cast(),BindingHost{next:element_source_next,apply:element_source_apply})};
}
struct Names<'a>{iter:std::collections::btree_map::Iter<'a,ValueIdentity,bounds::Proof>,current:Option<(ValueIdentity,ValueIdentity)>,output:BTreeMap<ValueIdentity,ValueIdentity>}
unsafe extern "C" fn name_next(context:*mut c_void)->Known{
 let c=unsafe{&mut *context.cast::<Names<'_>>()};let source=c.iter.next();c.current=source.and_then(|(id,proof)|proof.known_identity(0).map(|target|(*id,target)));Known{present:u32::from(source.is_some()),known:u32::from(c.current.is_some())}
}
unsafe extern "C" fn name_apply(context:*mut c_void){let c=unsafe{&mut *context.cast::<Names<'_>>()};let(source,target)=unsafe{c.current.unwrap_unchecked()};c.output.insert(source,target);}
fn names(bindings:&Bindings)->BTreeMap<ValueIdentity,ValueIdentity>{
 let mut context=Names{iter:bindings.iter(),current:None,output:BTreeMap::new()};unsafe{apply_known(ptr::from_mut(&mut context).cast(),BindingHost{next:name_next,apply:name_apply})};context.output
}
struct Nested<'a,I>{iter:I,current:Option<Reg>,name:&'a view::Borrow,bindings:&'a mut Bindings}
unsafe extern "C" fn nested_next<'s,I:Iterator<Item=&'s view::Source>>(context:*mut c_void)->Known{
 let c=unsafe{&mut *context.cast::<Nested<'_,I>>()};let source=c.iter.next();c.current=match source{Some(view::Source::Data(ValueIdentity::Register(reg)))=>Some(*reg),_=>None};Known{present:u32::from(source.is_some()),known:u32::from(c.current.is_some())}
}
unsafe extern "C" fn nested_apply<I>(context:*mut c_void){
 let c=unsafe{&mut *context.cast::<Nested<'_,I>>()};let id=ValueIdentity::Register(unsafe{c.current.unwrap_unchecked()});c.bindings.entry(id).or_insert_with(||with_borrow(bounds::Proof::default(),c.name.clone()));
}
fn nested_bindings<'s,I:Iterator<Item=&'s view::Source>>(iter:I,name:&view::Borrow,bindings:&mut Bindings){
 let mut context=Nested{iter,current:None,name,bindings};unsafe{apply_known(ptr::from_mut(&mut context).cast(),BindingHost{next:nested_next::<I>,apply:nested_apply::<I>})};
}
#[derive(Clone,Copy)]struct Roots{identity:ValueIdentity,source:Reg,target:Reg}
fn canonical_model(builder:&mut Builder<'_>,source:Option<Returned>,model_index:usize,base:usize,bindings:&mut Bindings)->(Option<Returned>,usize){
 let mut cursor=unsafe{start()};let mut facts=ModelFacts{present:u32::from(source.is_some()),known:0,root:0,borrowed:0,nested:0,width:0,paths:0,depth:0,base:base as u64,model_index:model_index as u64};
 let mut proof:Option<bounds::Proof>=None;let mut registers=Vec::new();let mut value=None;let mut known=None;
 let mut paths=Vec::new();let mut roots:Option<Roots>=None;let mut name:Option<view::Borrow>=None;
 loop{let step=unsafe{model_step(cursor,facts)};cursor=step.cursor;match step.need{
  ModelNeed::Capture=>{let model=unsafe{source.as_ref().unwrap_unchecked()};proof=Some(bounds::Proof::capture(&builder.returned_storage(model)));registers=model.registers();facts.width=registers.len()as u64;}
  ModelNeed::Rebind=>{let mut model=unsafe{source.as_ref().unwrap_unchecked()}.clone();rebind_model(&mut model,(base..base+facts.width as usize).collect());value=Some(model);}
  ModelNeed::Register=>{
   let component=cursor.part as usize-base;let source=registers[component];let target=ValueIdentity::Register(cursor.part as usize);
   bindings.entry(ValueIdentity::Register(source)).or_insert_with(||identity(target));known=unsafe{proof.as_ref().unwrap_unchecked()}.known_identity(component);facts.known=u32::from(known.is_some());
  }
  ModelNeed::Identity=>{let target=ValueIdentity::Register(cursor.part as usize);bindings.entry(unsafe{known.unwrap_unchecked()}).or_insert_with(||identity(target));}
  ModelNeed::Paths=>{proof=None;registers=Vec::new();paths=view_contracts::resource_output_paths(unsafe{source.as_ref().unwrap_unchecked()});facts.paths=paths.len()as u64;}
  ModelNeed::Root=>{
   let(root,scope)=&paths[cursor.index as usize];facts.depth=scope.len()as u64;facts.nested=0;name=None;
   roots=match(view_contracts::root_resource(unsafe{source.as_ref().unwrap_unchecked()},*root),view_contracts::root_resource(unsafe{value.as_ref().unwrap_unchecked()},*root)){
    (Some(source),Some(target))=>{facts.borrowed=u32::from(source.ownership==application::Ownership::Borrowed);Some(Roots{identity:source.root.identity(),source:source.root.regs[0],target:target.root.regs[0]})}
    _=>None,
   };facts.root=u32::from(roots.is_some());
  }
  ModelNeed::BorrowedName=>{
   let root=unsafe{roots.unwrap_unchecked()};let bound=bindings[&root.identity].clone();bindings.insert(ValueIdentity::Register(root.source),bound);
   name=Some(view::Borrow::source(view::Source::Data(root.identity)).at_call(bindings));
  }
  ModelNeed::OwnedName=>name=Some(view::Borrow::source(view::Source::Data(ValueIdentity::Register(unsafe{roots.unwrap_unchecked()}.target)))),
  ModelNeed::Element=>{
   let scope=&paths[cursor.index as usize].1;let fields=&scope[scope.len()-1-cursor.part as usize];
   name=Some(view::Borrow::element(unsafe{name.take().unwrap_unchecked()},fields.clone(),Some(bounds::Operand::Value(ValueIdentity::ElementIndex(cursor.part as usize)))));
  }
  ModelNeed::InspectNested=>facts.nested=u32::from(view_contracts::resource_output(unsafe{source.as_ref().unwrap_unchecked()},&paths[cursor.index as usize]).is_some()),
  ModelNeed::Nested=>{
   let data=unsafe{view_contracts::resource_output(source.as_ref().unwrap_unchecked(),&paths[cursor.index as usize]).unwrap_unchecked()};
   let borrow=data.root.proof.view_borrow(0);nested_bindings((&borrow.sources).into_iter(),unsafe{name.as_ref().unwrap_unchecked()},bindings);
  }
  ModelNeed::RootIdentities=>{
   let root=unsafe{roots.unwrap_unchecked()};let target=ValueIdentity::Register(root.target);
   bindings.entry(root.identity).or_insert_with(||identity(target));bindings.entry(ValueIdentity::Register(root.source)).or_insert_with(||identity(target));
  }
  ModelNeed::ReturnedName=>{builder.view_model.resource_names.insert(view_contracts::Output::ReturnedResource(paths[cursor.index as usize].clone()),unsafe{name.take().unwrap_unchecked()});}
  ModelNeed::WrittenName=>{builder.view_model.resource_names.insert(view_contracts::Output::WrittenResource(cursor.part as usize,paths[cursor.index as usize].clone()),unsafe{name.take().unwrap_unchecked()});}
  ModelNeed::Continue=>{},
  ModelNeed::Complete=>return(value,cursor.part as usize),
 }}
}

fn initialization(builder:&Builder<'_>,names:&BTreeMap<ValueIdentity,ValueIdentity>)->Option<Vec<address::Initialization>>{
 let mut cursor=unsafe{start()};let mut facts=InitialFacts{present:0,position:0,pending:0};
 let mut effects=builder.view_model.initialization.iter();let mut current=None;let mut original=Vec::new();
 loop{let step=unsafe{initial_step(cursor,facts)};cursor=step.cursor;match step.need{
  InitialNeed::Next=>{let effect=effects.next();facts.present=u32::from(effect.is_some());current=match effect{Some(address::InitializationEffect::Position(fact))=>Some(fact),_=>None};facts.position=u32::from(current.is_some());facts.pending=current.map(|fact|u32::from(fact.source.pending())).unwrap_or(0);}
  InitialNeed::Keep=>original.push(unsafe{current.unwrap_unchecked()}.clone()),
  InitialNeed::Incomplete=>return None,
  InitialNeed::Complete=>break,
 }}
 let mut input=address::merge_initializations(original).into_iter();let mut current:Option<address::Initialization>=None;let mut cursor=unsafe{start()};let mut facts=BoundFacts{present:0,local:0};
 let values=std::iter::from_fn(||loop{
  let step=unsafe{bound_step(cursor,facts)};cursor=step.cursor;match step.need{
   BoundNeed::Next=>{current=input.next();facts.present=u32::from(current.is_some());facts.local=current.as_ref().map(|fact|u32::from(fact.source.borrow().has_local())).unwrap_or(0);}
   BoundNeed::When=>{let fact=unsafe{current.as_mut().unwrap_unchecked()};fact.when=builder.view_model.initialization_when(fact);}
   BoundNeed::Bind=>{current=Some(unsafe{current.take().unwrap_unchecked()}.output_bound(names));}
   BoundNeed::Yield=>return current.take(),
   BoundNeed::Continue=>{},
   BoundNeed::Complete=>return None,
  }
 });
 Some(address::merge_initializations(values))
}
async fn prefix_bindings(builder:&Builder<'_>,parameters:&[CallParameter])->Result<Bindings>{
 let mut cursor=unsafe{start()};let mut facts=PrefixParameterFacts{present:0,origin:0,reference:0,dataset:0,width:0};let mut parameters=parameters.iter();let mut source=None;let mut result=BTreeMap::new();
 loop{let step=unsafe{prefix_parameter_step(cursor,facts)};cursor=step.cursor;match step.need{
  PrefixParameterNeed::Next=>{
   let parameter=parameters.next();facts.present=u32::from(parameter.is_some());
   source=parameter.and_then(|parameter|match parameter.origin.as_deref(){Some(ParameterOrigin::Parameter{index,..})=>Some(*index),_=>None});facts.origin=u32::from(source.is_some());
   let(reference,width)=match parameter.map(|parameter|&parameter.kind){Some(CallParameterKind::Value(ty))=>(0,ty.components.len()),Some(CallParameterKind::Reference{..})=>(1,0),None=>(0,0)};facts.reference=reference;facts.width=width as u64;
  }
  PrefixParameterNeed::Bound=>{facts.dataset=u32::from(unsafe{builder.world.call(builder.scope.parameters().nth(source.unwrap_unchecked()).unwrap_unchecked().bound(builder.world)).await}?.ty.dataset);}
  PrefixParameterNeed::Bind=>{let id=ValueIdentity::Register(cursor.part as usize);let parameter=unsafe{source.unwrap_unchecked()};result.insert(id,with_borrow(identity(id),view::Borrow::source(view::Source::Parameter(ValueIdentity::ResourceParameter(parameter,0),Vec::new()))));}
  PrefixParameterNeed::Continue=>{},
  PrefixParameterNeed::Complete=>return Ok(result),
 }}
}
fn prefix_inputs(value:&mut Returned,resources:&Bindings){
 let kind=match value{Returned::Unit=>Kind::Unit,Returned::Value(_)=>Kind::Value,Returned::OwnedValue(_)=>Kind::Owned,Returned::Dataset(_)=>Kind::Data};
 let mut cursor=unsafe{start()};let mut facts=PrefixFacts{kind,present:0,payloads:match value{Returned::OwnedValue(value)=>value.payloads.len()as u64,_=>0}};
 loop{let step=unsafe{prefix_step(cursor,facts)};cursor=step.cursor;match step.need{
  PrefixNeed::Facts=>{let Returned::Dataset(data)=value else{unsafe{std::hint::unreachable_unchecked()}};data.prefixes.values_mut().for_each(|fact|*fact=fact.substitute_values(resources));}
  PrefixNeed::Elements=>{let Returned::Dataset(data)=value else{unsafe{std::hint::unreachable_unchecked()}};data.elements=std::mem::take(&mut data.elements).map(&mut|mut model|{prefix_inputs(&mut model,resources);model},&mut|condition|condition);}
  PrefixNeed::Inspect=>{let Returned::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};facts.present=u32::from(value.payloads[cursor.index as usize].is_some());}
  PrefixNeed::Payload=>{let Returned::OwnedValue(value)=value else{unsafe{std::hint::unreachable_unchecked()}};prefix_inputs(unsafe{value.payloads[cursor.index as usize].as_mut().unwrap_unchecked()},resources);}
  PrefixNeed::Continue=>{},
  PrefixNeed::Complete=>return,
 }}
}
fn source_model<'a>(returned:&'a Option<Returned>,written:&'a [Option<Returned>],index:usize)->Option<&'a Returned>{if index==0{returned.as_ref()}else{written[index-1].as_ref()}}
fn source_slot<'a>(returned:&'a mut Option<Returned>,written:&'a mut [Option<Returned>],index:usize)->&'a mut Option<Returned>{if index==0{returned}else{&mut written[index-1]}}
struct Outputs{
 returned:Option<Returned>,written:Vec<Option<Returned>>,models:Vec<Option<Returned>>,next:usize,
}
impl Outputs{
 fn run(&mut self,mode:OutputMode,builder:&mut Builder<'_>,bindings:&mut Bindings,sources:&mut BTreeSet<ValueIdentity>,resources:&Bindings){
  let mut cursor=unsafe{start()};let mut facts=OutputFacts{present:0,view:0,count:if mode==OutputMode::Translate{self.models.len()}else{1+self.written.len()}as u64};
  loop{let step=unsafe{output_step(cursor,mode,facts)};cursor=step.cursor;let index=cursor.index as usize;match step.need{
   OutputNeed::Inspect=>{let model=if mode==OutputMode::Translate{self.models[index].as_ref()}else{source_model(&self.returned,&self.written,index)};facts.present=u32::from(model.is_some());facts.view=if mode==OutputMode::Values{u32::from(model.and_then(Returned::view_value).is_some())}else{0};}
   OutputNeed::Value=>{
    let value=unsafe{source_model(&self.returned,&self.written,index).and_then(Returned::view_value).unwrap_unchecked()};
    if index==0{builder.view_model.capture(&value.proof,&value.ty,view_contracts::Output::Returned);}else{builder.view_model.capture(&value.proof,&value.ty,|component|view_contracts::Output::Written(index-1,component));}
   }
   OutputNeed::Sources=>element_sources(unsafe{source_model(&self.returned,&self.written,index).unwrap_unchecked()},sources),
   OutputNeed::Resources=>{
    let value=unsafe{source_model(&self.returned,&self.written,index).unwrap_unchecked()};
    if index==0{builder.view_model.capture_resources(value,view_contracts::Output::ReturnedResource);}else{builder.view_model.capture_resources(value,|resource|view_contracts::Output::WrittenResource(index-1,resource));}
   }
   OutputNeed::Model=>{let source=source_slot(&mut self.returned,&mut self.written,index).take();let(value,next)=canonical_model(builder,source,index,self.next,bindings);self.models.push(value);self.next=next;}
   OutputNeed::Empty=>self.models.push(None),
   OutputNeed::Translate=>translate_in_place(unsafe{self.models[index].as_mut().unwrap_unchecked()},bindings,false),
   OutputNeed::Prefix=>prefix_inputs(unsafe{self.models[index].as_mut().unwrap_unchecked()},resources),
   OutputNeed::Continue=>{},
   OutputNeed::Complete=>return,
  }}
 }
}
pub(in crate::check) async fn canonical(
 builder:&mut Builder<'_>,returned:Option<Returned>,inputs:&[Reg],parameters:&[CallParameter],written:Vec<Option<Returned>>,
)->Result<(Option<Returned>,Vec<Reg>,Vec<Option<Returned>>,Option<Vec<address::Initialization>>)>{
 let mut cursor=unsafe{start()};let mut bindings=BTreeMap::new();let mut sources=BTreeSet::new();let mut output=Outputs{returned,written,models:Vec::new(),next:inputs.len()};
 let mut names=None;let mut initialization_result=None;let mut resources=BTreeMap::new();
 loop{let step=unsafe{canonical_step(cursor)};cursor=step.cursor;match step.need{
  CanonicalNeed::Seed=>{builder.view_model.input_width=inputs.len();seed_bindings(builder,output.returned.as_ref(),inputs,&mut bindings);}
  CanonicalNeed::Parameters=>unsafe{builder.world.call(source_bindings(builder,&mut bindings)).await}?,
  CanonicalNeed::Values=>output.run(OutputMode::Values,builder,&mut bindings,&mut sources,&resources),
  CanonicalNeed::Elements=>element_inputs(builder,&mut bindings),
  CanonicalNeed::Sources=>output.run(OutputMode::Sources,builder,&mut bindings,&mut sources,&resources),
  CanonicalNeed::BindSources=>bind_element_sources(std::mem::take(&mut sources),&mut bindings),
  CanonicalNeed::Resources=>output.run(OutputMode::Resources,builder,&mut bindings,&mut sources,&resources),
  CanonicalNeed::Memory=>memory_bindings(builder,&mut bindings),
  CanonicalNeed::Models=>output.run(OutputMode::Models,builder,&mut bindings,&mut sources,&resources),
  CanonicalNeed::Names=>names=Some(self::names(&bindings)),
  CanonicalNeed::Initialization=>initialization_result=initialization(builder,unsafe{names.as_ref().unwrap_unchecked()}),
  CanonicalNeed::PublishNames=>builder.view_model.parameters=unsafe{names.take().unwrap_unchecked()},
  CanonicalNeed::PrefixBindings=>resources=unsafe{builder.world.call(prefix_bindings(builder,parameters)).await}?,
  CanonicalNeed::Translate=>output.run(OutputMode::Translate,builder,&mut bindings,&mut sources,&resources),
  CanonicalNeed::Complete=>{let mut models=output.models.into_iter();return Ok((unsafe{models.next().unwrap_unchecked()},(0..inputs.len()).collect(),models.collect(),initialization_result));}
 }}
}

struct ElementInputs<'a>{iter:std::collections::btree_map::Iter<'a,ValueIdentity,ValueIdentity>,current:Option<(ValueIdentity,ValueIdentity)>,bindings:&'a mut Bindings}
unsafe extern "C" fn element_input_next(context:*mut c_void)->Known{
 let c=unsafe{&mut *context.cast::<ElementInputs<'_>>()};c.current=c.iter.next().map(|(source,target)|(*source,*target));Known{present:u32::from(c.current.is_some()),known:u32::from(matches!(c.current,Some((ValueIdentity::DatasetContent(..),_))))}
}
unsafe extern "C" fn element_input_apply(context:*mut c_void){
 let c=unsafe{&mut *context.cast::<ElementInputs<'_>>()};let(source,target)=unsafe{c.current.unwrap_unchecked()};c.bindings.insert(source,with_borrow(identity(target),view::Borrow::source(view::Source::Parameter(target,Vec::new()))));
}
pub(in crate::check) fn element_inputs(builder:&Builder<'_>,bindings:&mut Bindings){
 let mut context=ElementInputs{iter:builder.view_model.parameters.iter(),current:None,bindings};unsafe{apply_known(ptr::from_mut(&mut context).cast(),BindingHost{next:element_input_next,apply:element_input_apply})};
}
struct ParameterSources<'a,I>{iter:I,current:Option<ValueIdentity>,output:&'a mut BTreeSet<ValueIdentity>}
unsafe extern "C" fn parameter_source_next<I:Iterator<Item=view::Source>>(context:*mut c_void)->Known{
 let c=unsafe{&mut *context.cast::<ParameterSources<'_,I>>()};let source=c.iter.next();c.current=match &source{Some(view::Source::Parameter(id,_))=>Some(*id),_=>None};Known{present:u32::from(source.is_some()),known:u32::from(c.current.is_some())}
}
unsafe extern "C" fn parameter_source_apply<I>(context:*mut c_void){let c=unsafe{&mut *context.cast::<ParameterSources<'_,I>>()};c.output.insert(unsafe{c.current.unwrap_unchecked()});}
fn collect_parameter_sources<I:Iterator<Item=view::Source>>(iter:I,output:&mut BTreeSet<ValueIdentity>){
 let mut context=ParameterSources{iter,current:None,output};unsafe{apply_known(ptr::from_mut(&mut context).cast(),BindingHost{next:parameter_source_next::<I>,apply:parameter_source_apply::<I>})};
}
#[derive(Clone,Copy)]#[repr(u32)]enum SourceModelNeed{View,InspectElement,Element,InspectPayload,Payload,Continue,Complete}
#[derive(Clone,Copy)]#[repr(C)]struct SourceModelFacts{kind:Kind,view:u32,present:u32,payloads:u64}
unsafe extern "C"{
 #[link_name="bootstrap.native_canonical::element_sources_step"]fn element_sources_step(cursor:Cursor,facts:SourceModelFacts)->Step<SourceModelNeed>;
}
fn element_sources(model:&Returned,output:&mut BTreeSet<ValueIdentity>){
 let kind=match model{Returned::Unit=>Kind::Unit,Returned::Value(_)=>Kind::Value,Returned::OwnedValue(_)=>Kind::Owned,Returned::Dataset(_)=>Kind::Data};
 let mut cursor=unsafe{start()};let mut facts=SourceModelFacts{kind,view:u32::from(model.view_value().is_some()),present:0,payloads:match model{Returned::OwnedValue(value)=>value.payloads.len()as u64,_=>0}};
 loop{let step=unsafe{element_sources_step(cursor,facts)};cursor=step.cursor;match step.need{
  SourceModelNeed::View=>{let value=unsafe{model.view_value().unwrap_unchecked()};let sources=value.proof.typed_view_borrow(&value.ty).owners().sources;collect_parameter_sources(sources.into_iter(),output);}
  SourceModelNeed::InspectElement=>{let Returned::Dataset(data)=model else{unsafe{std::hint::unreachable_unchecked()}};facts.present=u32::from(data.elements.known().is_some());}
  SourceModelNeed::Element=>{let Returned::Dataset(data)=model else{unsafe{std::hint::unreachable_unchecked()}};element_sources(unsafe{data.elements.known().unwrap_unchecked()},output);}
  SourceModelNeed::InspectPayload=>{let Returned::OwnedValue(value)=model else{unsafe{std::hint::unreachable_unchecked()}};facts.present=u32::from(value.payloads[cursor.index as usize].is_some());}
  SourceModelNeed::Payload=>{let Returned::OwnedValue(value)=model else{unsafe{std::hint::unreachable_unchecked()}};element_sources(unsafe{value.payloads[cursor.index as usize].as_ref().unwrap_unchecked()},output);}
  SourceModelNeed::Continue=>{},
  SourceModelNeed::Complete=>return,
 }}
}
