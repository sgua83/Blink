//! B owns the shared cleanup declaration, actual body and invocation sequence.
use super::*;
use crate::check::dataset_drop as d;
use std::borrow::Cow;
struct Header{parameters:Arc<[Param]>,lexical:LexicalScope,contracts:[FunctionContract;1]}
fn header(builder:&Builder<'_>,name:&str,ty:&Ty,span:Span)->Header{
 let parameters:Arc<[Param]>=Arc::from([Param{name:"value".into(),ty:ty.syntax(),access:Access::Value,span}]);
 let contracts=[FunctionContract{kind:ContractKind::Requires,predicate:ContractPredicate::Cleanup{value:"value".into(),condition:OutputCondition::Bool(true)},span}];
 let lexical=LexicalScope::Values{source:builder.scope.source().clone(),module:builder.scope.module().to_owned(),imports:builder.scope.imports().to_vec(),bindings:builder.scope.bindings().clone(),span,parameters:parameters.clone(),source_definition:None,
  source_context:Arc::new(SourceContext{instance:Some(Arc::from(name)),inlined_at:Box::default(),domain_path:Arc::from([])})};
 Header{parameters,lexical,contracts}
}
impl Builder<'_>{
 async fn owned_cleanup(&mut self,ty:&Ty,span:Span)->Result<(String,Header)>{
  let mut cursor=unsafe{d::start()};let mut facts=d::DeclFacts{made:0};let mut name=String::new();let mut signature=None;
  loop{let next=unsafe{d::declaration_step(cursor,facts)};cursor=next.cursor;match next.need{
   d::DeclNeed::Name=>{name=crate::generics::key(&format!("{}::@dataset_element_drop",self.scope.module()),std::iter::once(ty.name.as_str()),&[]);}
   d::DeclNeed::Declare=>{facts.made=u32::from(self.functions.declare(&name));}
   d::DeclNeed::Header=>{signature=Some(header(self,&name,ty,span));}
   d::DeclNeed::Body=>{
    let signature=unsafe{signature.as_ref().unwrap_unchecked()};let contract=outputs::Contract::native(&signature.lexical,&signature.parameters,&signature.contracts,&name,span);
    self.functions=unsafe{self.world.call(element_drop_declaration(self.world,&name,ty,&signature.lexical,&signature.parameters,contract,std::mem::take(&mut self.functions),span)).await}?;
   }
   d::DeclNeed::Complete=>return Ok((name,unsafe{signature.take().unwrap_unchecked()})),
   d::DeclNeed::Next=>{},
  }}
 }
 pub(in crate::check) async fn native_element_drop(&mut self,value:Returned,retained:&view::Borrow,span:Span,origin:Arc<GeneratedOrigin>)->Result<()>{
  let kind=match &value{Returned::Unit=>d::Kind::Unit,Returned::Value(_)=>d::Kind::Value,Returned::OwnedValue(_)=>d::Kind::Owned,Returned::Dataset(_)=>d::Kind::Data};
  let mut cursor=unsafe{d::start()};let mut ty:Option<Cow<'_,Ty>>=None;let mut declaration=None;
  loop{let next=unsafe{d::native_step(cursor,kind)};cursor=next.cursor;match next.need{
   d::NativeNeed::UnitType=>{ty=Some(Cow::Owned(Ty::unit()));}
   d::NativeNeed::ValueType=>{let Returned::Value(value)=&value else{unsafe{std::hint::unreachable_unchecked()}};ty=Some(Cow::Borrowed(&value.ty));}
   d::NativeNeed::OwnedType=>{let Returned::OwnedValue(value)=&value else{unsafe{std::hint::unreachable_unchecked()}};ty=Some(Cow::Borrowed(&value.value.ty));}
   d::NativeNeed::DataType=>{let Returned::Dataset(value)=&value else{unsafe{std::hint::unreachable_unchecked()}};ty=Some(Cow::Owned(owned_values::storage::dataset_storage_type(value)));}
   d::NativeNeed::MovableData=>{let Returned::Dataset(data)=&value else{unsafe{std::hint::unreachable_unchecked()}};self.application_movable(data,retained,span)?;}
   d::NativeNeed::MovableTree=>{
    let Returned::OwnedValue(value)=&value else{unsafe{std::hint::unreachable_unchecked()}};let resources=value.resources();let mut at=unsafe{d::start()};
    loop{let next=unsafe{d::resource_step(at,resources.len()as u64)};at=next.cursor;if next.need==0{break;}self.application_movable(resources[(at.index-1)as usize],retained,span)?;}
   }
   d::NativeNeed::Declare=>{declaration=Some(unsafe{self.world.call(self.owned_cleanup(ty.as_ref().unwrap_unchecked().as_ref(),span)).await}?);}
   d::NativeNeed::Invoke=>{
    let(name,signature)=unsafe{declaration.take().unwrap_unchecked()};drop(ty);
    let contract=outputs::Contract::native(&signature.lexical,&signature.parameters,&signature.contracts,&name,span);let result=TypeName::named("void");
    unsafe{self.world.call(self.native_call(&name,CallSignature{parameters:&signature.parameters,result:&result,contract,lexical:&signature.lexical,kind:FunctionKind::Value,execution:crate::execution::Requirement::Invocation,c_abi:false},vec![Argument::resource(value)],span,Some(origin),None,None)).await}?;
    return Ok(());
   }
   d::NativeNeed::Next=>{},
  }}
 }
}
async fn element_drop_declaration(world:&World,name:&str,ty:&Ty,lexical:&LexicalScope,parameters:&[Param],contract:outputs::Contract<'_>,functions:Module,span:Span)->Result<Module>{
 let mut cursor=unsafe{d::start()};let mut facts=d::BodyFacts{dataset:u32::from(parameters[0].ty.dataset),owned:0};
 let program=Arc::new(unsafe{world.call(function_program(world,name,parameters.iter().map(|parameter| std::future::ready(Ok(parameter))),(lexical.source(),lexical.module(),lexical.imports(),FunctionKind::Value),Target::Cpu,None, |_| {})).await}?);
 let mut functions=Some(functions);let mut b:Option<Box<Builder<'_>>>=None;let mut data:Option<application::Data>=None;let mut value:Option<Value>=None;let mut owner=None;let mut origin=None;
 let mut inputs=Vec::new();let mut abi=Vec::new();let mut regs=Vec::new();let mut environment=BTreeMap::new();let mut paths=None;let mut resources=BTreeMap::new();let mut requirements=None;let mut checked=None;let mut result=None;
 loop{let next=unsafe{d::body_step(cursor,facts)};cursor=next.cursor;match next.need{
  d::BodyNeed::Builder=>{
   let(source_definition,source_context)=lexical.syntax_context(Some(name));
   let mut builder=Box::new(Builder::from_checked(world,lexical,&program,0,source_definition,source_context,BTreeSet::new()));
   builder.in_function=true;builder.application=Some(application::Context::default());builder.functions=unsafe{functions.take().unwrap_unchecked()};b=Some(builder);
  }
  d::BodyNeed::Frame=>{
   let builder=unsafe{b.as_mut().unwrap_unchecked()};let frame=builder.application_frame(span)?;inputs=frame.regs.clone();abi=vec![CallParameter::value(frame.ty).with_origin(Some(Arc::new(ParameterOrigin::Frame)))];
  }
  d::BodyNeed::Calls=>{unsafe{b.as_mut().unwrap_unchecked()}.calls.push(name.to_owned());}
  d::BodyNeed::Origin=>{origin=Some(Arc::new(ParameterOrigin::Parameter{index:0,name:"value".into(),instance:Arc::from(name),generated:true,site:None}));}
  d::BodyNeed::DataParameter=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};data=Some(unsafe{builder.world.call(builder.application_parameter(0,0,Access::Value,Scalar::U32)).await}?);}
  d::BodyNeed::DataAbi=>{
   let data=unsafe{data.as_ref().unwrap_unchecked()};abi.push(CallParameter::value(Ty::owned_dataset(data.ty.syntax(),data.count)).with_origin(origin.take()));inputs.extend(&data.root.regs);inputs.extend(&data.length.regs);
  }
  d::BodyNeed::DataBind=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};let data=builder.application_bind(unsafe{data.take().unwrap_unchecked()},false);builder.env.insert("value".into(),Binding::ApplicationData(data));}
  d::BodyNeed::Registers=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};regs=ty.components.iter().map(|(scalar,_)|builder.reg(*scalar)).collect();}
  d::BodyNeed::ValueAbi=>{inputs.extend(&regs);abi.push(CallParameter::value(ty.clone()).with_origin(origin.take()));}
  d::BodyNeed::Value=>{
   let proof=bounds::Proof::default().view_parameters(ty,&[],|index|ValueIdentity::Register(regs[index]));
   value=Some(Value{ty:ty.clone(),regs:std::mem::take(&mut regs),proof,literal:false,uniform:false,constant:None});
  }
  d::BodyNeed::Inspect=>{facts.owned=u32::from(ty.owns_resources());}
  d::BodyNeed::Incoming=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};owner=Some(unsafe{builder.world.call(builder.incoming_owned_value(value.take().unwrap_unchecked())).await}?);}
  d::BodyNeed::Residency=>{residency::parameter(unsafe{owner.as_mut().unwrap_unchecked()},0);}
  d::BodyNeed::OwnedBind=>{unsafe{b.as_mut().unwrap_unchecked()}.bind_owned_value("value",unsafe{owner.take().unwrap_unchecked()},span)?;}
  d::BodyNeed::ValueBind=>{unsafe{b.as_mut().unwrap_unchecked()}.env.insert("value".into(),Binding::Value(unsafe{value.take().unwrap_unchecked()}));}
  d::BodyNeed::TakeElementsEnv=>{environment=std::mem::take(&mut unsafe{b.as_mut().unwrap_unchecked()}.env);}
  d::BodyNeed::ElementPaths=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};paths=Some(unsafe{builder.world.call(contracts::element_input_paths(builder,contract,&mut environment,None)).await}?);}
  d::BodyNeed::PutElementsEnv=>{unsafe{b.as_mut().unwrap_unchecked()}.env=std::mem::take(&mut environment);}
  d::BodyNeed::BindElements=>{application::bind_parameter_elements(unsafe{b.as_mut().unwrap_unchecked()},contract,unsafe{paths.as_ref().unwrap_unchecked()});}
  d::BodyNeed::ResourceFacts=>{
   let Some(Binding::ApplicationData(data))=unsafe{b.as_ref().unwrap_unchecked()}.env.get("value")else{unsafe{std::hint::unreachable_unchecked()}};
   resources=BTreeMap::from([(ValueIdentity::ResourceParameter(0,0),{let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(0,view::resource_borrow(data));component_proof})]);
  }
  d::BodyNeed::TakeContractEnv=>{environment=std::mem::take(&mut unsafe{b.as_mut().unwrap_unchecked()}.env);}
  d::BodyNeed::Resolve=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};requirements=Some(unsafe{builder.world.call(contracts::resolve(builder,contract,lexical,&environment,&resources)).await}?);}
  d::BodyNeed::Assume=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};unsafe{builder.world.call(contracts::assume(builder,contract,&mut environment,requirements.as_ref().unwrap_unchecked())).await}?;}
  d::BodyNeed::PutContractEnv=>{unsafe{b.as_mut().unwrap_unchecked()}.env=std::mem::take(&mut environment);}
  d::BodyNeed::Arrays=>{unsafe{b.as_mut().unwrap_unchecked()}.materialize_array_bindings();}
  d::BodyNeed::Release=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};unsafe{builder.world.call(builder.release_application_data(None,view::Borrow::default())).await}?;}
  d::BodyNeed::Initialization=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};builder.view_model.initialization=address::initialization_effects(&builder.initialized,&builder.path);}
  d::BodyNeed::Returns=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};builder.view_model.normal_returns=Some(bounds::Condition::execution(&builder.path));}
  d::BodyNeed::Finish=>{
   let contracts=contract.contracts().map(|contract|(contract.kind,contract.predicate)).collect();
   checked=Some(unsafe{world.call(finish_checked_function(b.take().unwrap_unchecked(),program.clone(),name,span,None,None,std::mem::take(&mut abi),std::mem::take(&mut inputs),contracts,None,vec![None])).await}?);
  }
  d::BodyNeed::Publish=>{result=Some(finish_declaration(world,lexical,false,unsafe{checked.take().unwrap_unchecked()})?);}
  d::BodyNeed::Complete=>return Ok(unsafe{result.take().unwrap_unchecked()}),
  d::BodyNeed::Next=>{},
 }}
}
