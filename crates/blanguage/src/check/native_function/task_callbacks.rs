//! B owns shared callback lifetime and actual output ready selection.
use super::*;
#[derive(Clone,Copy)]
#[repr(C)] struct Cursor{phase:u32,index:u64,part:u64}
#[repr(C)] struct Step<T>{cursor:Cursor,need:T}
#[repr(u32)] enum CallbackNeed{Builder=0,Frame=1,Registers=2,Inputs=3,Abi=4,Functions=5,Emit=6,Finish=7,Publish=8,RuntimeContract=9,Complete=10,Next=11}
#[repr(u32)] enum SelectionNeed{Zero=0,Packet=1,Result=2,Returned=3,Guard=4,Owners=5,Ready=6,Complete=7,Next=8}
#[derive(Clone,Copy)]
#[repr(C)] struct SelectionFacts{packet_components:u64,packet_registers:u64,result_components:u64,outputs:u64,owners:u64}
unsafe extern "C"{
 #[link_name="bootstrap.task_callbacks::start"]fn start()->Cursor;
 #[link_name="bootstrap.task_callbacks::callback_step"]fn callback_step(state:Cursor,frame:u32)->Step<CallbackNeed>;
 #[link_name="bootstrap.task_callbacks::selection_step"]fn selection_step(state:Cursor,facts:SelectionFacts)->Step<SelectionNeed>;
}
impl Builder<'_>{
 pub(in crate::check::native_function) async fn task_callback(
  &mut self,name:String,arguments:&[(&str,Ty)],frame:bool,span:Span,
  emit:impl std::ops::AsyncFnOnce(&mut Builder<'_>,&[Reg])->Result<()>,
 )->Result<String>{
  let parameters:Arc<[Param]>=arguments.iter().map(|(name,ty)|Param{name:(*name).into(),ty:ty.syntax(),access:Access::Value,span}).collect::<Vec<_>>().into();
  let lexical=LexicalScope::Values{
   source:self.scope.source().clone(),module:self.scope.module().into(),imports:self.scope.imports().to_vec(),bindings:self.scope.bindings().clone(),
   span,parameters:parameters.clone(),source_definition:None,
   source_context:Arc::new(SourceContext{instance:Some(Arc::from(name.as_str())),inlined_at:Box::default(),domain_path:Arc::from([])}),
  };
  let program=Arc::new(unsafe{self.world.call(function_program(self.world,&name,parameters.iter().cloned().map(|parameter|async{Ok(parameter)}),
   (lexical.source(),lexical.module(),lexical.imports(),FunctionKind::Value),Target::Cpu,None)).await}?);
  let mut cursor=unsafe{start()};let mut b:Option<Box<Builder<'_>>>=None;let mut frame_value:Option<Value>=None;
  let mut registers=Vec::new();let mut inputs=Vec::new();let mut abi=Vec::new();let mut emit=Some(emit);let mut checked=None;
  loop{let next=unsafe{callback_step(cursor,u32::from(frame))};cursor=next.cursor;match next.need{
   CallbackNeed::Builder=>{
    let(definition,context)=lexical.syntax_context(Some(&name));
    let mut builder=Box::new(Builder::from_checked(self.world,&lexical,&program,0,definition,context,BTreeSet::new()));
    builder.in_function=true;b=Some(builder);
   }
   CallbackNeed::Frame=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};builder.application=Some(application::Context::default());frame_value=Some(builder.application_frame(span)?);}
   CallbackNeed::Registers=>{let builder=unsafe{b.as_mut().unwrap_unchecked()};registers=arguments.iter().flat_map(|(_,ty)|&ty.components).map(|&(scalar,_)|builder.reg(scalar)).collect();}
   CallbackNeed::Inputs=>{inputs=frame_value.iter().flat_map(|frame|frame.regs.iter().copied()).chain(registers.iter().copied()).collect();}
   CallbackNeed::Abi=>{
    abi=frame_value.take().into_iter().map(|frame|CallParameter::value(frame.ty).with_origin(Some(Arc::new(ParameterOrigin::Frame))))
     .chain(arguments.iter().map(|(_,ty)|CallParameter::value(ty.clone()))).collect();
   }
   CallbackNeed::Functions=>{unsafe{b.as_mut().unwrap_unchecked()}.functions=std::mem::take(&mut self.functions);}
   CallbackNeed::Emit=>{(unsafe{emit.take().unwrap_unchecked()})(unsafe{b.as_mut().unwrap_unchecked()},&registers).await?;}
   CallbackNeed::Finish=>{
    checked=Some(unsafe{self.world.call(finish_checked_function(b.take().unwrap_unchecked(),program.clone(),&name,span,None,None,std::mem::take(&mut abi),std::mem::take(&mut inputs),BTreeSet::new(),None,vec![None;parameters.len()])).await}?);
   }
   CallbackNeed::Publish=>{self.functions=finish_declaration(self.world,&lexical,false,unsafe{checked.take().unwrap_unchecked()})?;}
   CallbackNeed::RuntimeContract=>{
    let contract=&mut self.functions.units.get_mut(&name).unwrap().as_mut().unwrap().contract;contract.frame=true;contract.fallible=true;
   }
   CallbackNeed::Complete=>return Ok(name),
   CallbackNeed::Next=>{},
  }}
 }
 pub(in crate::check) async fn task_selection(&mut self,packet:&Value,result:&Ty,outputs:&[(u32,&outputs::TaskGuard)],span:Span)->Result<String>{
  let name=format!("{}::@task_selection{}",self.scope.module(),self.functions.units.len());let pointer=Ty::vector(Scalar::U32,2);
  unsafe{self.world.call(self.task_callback(name,&[("input",pointer.clone()),("output",pointer)],false,span,async |b,registers|{
   let[a,b0,c,d]=*registers else{unreachable!()};let(input,output)=([a,b0],[c,d]);
   let mut cursor=unsafe{start()};let mut facts=SelectionFacts{packet_components:packet.ty.components.len()as u64,packet_registers:packet.regs.len()as u64,result_components:result.components.len()as u64,outputs:outputs.len()as u64,owners:0};
   let mut zero=None;let mut snapshot=BTreeMap::new();let mut regs=Vec::new();let mut returned=None;let mut owners=Vec::new();let mut selected=Vec::new();
   let load=|b:&mut Builder<'_>,zero,address,scalar,word|{
    let dst=b.reg(scalar);b.emit(InstKind::LoadIndexed{dst,source:IndexedSource::Pointer{address,index:zero,stride:1,aliases:Vec::new()},offset:zero,word});dst
   };
   loop{let next=unsafe{selection_step(cursor,facts)};cursor=next.cursor;match next.need{
    SelectionNeed::Zero=>{zero=Some(b.constant(Scalar::U32,0).regs[0]);}
    SelectionNeed::Packet=>{
     let index=cursor.index as usize;let(scalar,offset)=packet.ty.components[index];snapshot.insert(packet.regs[index],load(b,unsafe{zero.unwrap_unchecked()},input,scalar,offset));
    }
    SelectionNeed::Result=>{let(scalar,offset)=result.components[cursor.index as usize];regs.push(load(b,unsafe{zero.unwrap_unchecked()},output,scalar,offset));}
    SelectionNeed::Returned=>{returned=Some(Value{ty:result.clone(),regs:std::mem::take(&mut regs),proof:bounds::Proof::default(),uniform:false,literal:false,constant:None});}
    SelectionNeed::Guard=>{
     selected=unsafe{b.world.call(outputs[cursor.index as usize].1.guards(b,returned.as_ref(),span,|operand|match operand{
      bounds::Operand::Value(ValueIdentity::Register(reg))=>snapshot.get(&reg).map(|reg|bounds::Operand::Value(ValueIdentity::Register(*reg))),
      _=>Some(operand),
     })).await}?;
    }
    SelectionNeed::Owners=>{owners=outputs[cursor.index as usize].1.ty.task_output_owners();facts.owners=owners.len()as u64;}
    SelectionNeed::Ready=>{
     let(base,guard)=&outputs[cursor.index as usize];let ready=outputs::resource_guard(&guard.ty,owners[cursor.part as usize].0,&selected);let value=b.presence_register(&ready,span,None)?;
     b.emit(InstKind::StoreIndexed{source:IndexedSource::Pointer{address:output,index:unsafe{zero.unwrap_unchecked()},stride:1,aliases:Vec::new()},offset:unsafe{zero.unwrap_unchecked()},word:*base+cursor.part as u32*Scalar::Bool.size(),value});
    }
    SelectionNeed::Complete=>return Ok(()),
    SelectionNeed::Next=>{},
   }}
  })).await}
 }
}
