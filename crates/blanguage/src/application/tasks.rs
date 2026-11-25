//! B controls task binding, structured start-call rewrite and actual worker entries.
use super::{Resources,resource_prefix};
use crate::ast::Access;
use crate::ir::{self,BinaryOp,Call,CallArgument,CallParameter,CallParameterKind,CallReference,CallStorage,CallTarget,Comparison,FunctionBody,IndexedSource,Inst,InstKind,Scalar,Ty,ValueCode};
use std::{borrow::Cow,collections::BTreeMap,ffi::c_void,ptr};
mod wire{
 use std::ffi::c_void;
 #[derive(Clone,Copy)]#[repr(C)]pub(super)struct Cursor{pub(super)phase:u32,pub(super)index:u64,pub(super)part:u64}
 #[repr(C)]pub(super)struct Step<T>{pub(super)cursor:Cursor,pub(super)need:T}
 #[derive(Clone,Copy)]#[repr(u32)]pub(super)enum Kind{Other=0,Start=1,Scope=2,Pair=3}
 #[derive(Clone,Copy)]#[repr(u32)]pub(super)enum Role{Ordinary=0,Frame=1,ReturnSource=2,Completion=3}
 #[derive(Clone,Copy)]#[repr(C)]pub(super)struct BindFacts{pub(super)resources:u32,pub(super)root:u32,pub(super)functions:u64,pub(super)bindings:u64}
 #[repr(u32)]pub(super)enum BindNeed{Start=0,Root=1,Function=2,Entries=3,Entry=4,Empty=5,Present=6,Next=7}
 #[repr(C)]pub(super)struct Instruction {
  pub(super) kind: Kind,
  pub(super) first: *mut c_void,
  pub(super) second: *mut c_void,
 }
 #[repr(C)]pub(super)struct RewriteHost {
  pub(super) inspect: unsafe extern "C" fn(*mut c_void, usize) -> Instruction,
  pub(super) child: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32,
  pub(super) start: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> u32,
 }
 #[derive(Clone,Copy)]#[repr(C)]pub(super)struct LookupFacts{pub(super)count:u64,pub(super)equal:u32}
 #[repr(u32)]pub(super)enum LookupNeed{Worker=0,Input=1,Start=2,Selector=3,Found=4,Missing=5,Next=6}
 #[derive(Clone,Copy,Default)]#[repr(C)]pub(super)struct NewFacts{pub(super)fields:u64,pub(super)parameters:u64,pub(super)frame:u32,pub(super)owned:u32,pub(super)reference:u32,pub(super)inout:u32,pub(super)has_result:u32}
 #[repr(u32)]pub(super)enum NewNeed{CompletionResult=0,CompletionUnit=1,Output=2,Index=3,Layout=4,Parameter=5,Field=6,RenameWrite=7,RenameInOut=8,PositionType=9,Name=10,Entry=11,Types=12,Positions=13,Binding=14,Complete=15,Next=16}
 #[repr(u32)]pub(super)enum RewriteNeed{Registers=0,Address=1,Index=2,Worker=3,Trim=4,Argument=5,Symbol=6,Complete=7}
 #[derive(Clone,Copy)]#[repr(C)]pub(super)struct EntryFacts{pub(super)parameters:u64,pub(super)components:u64,pub(super)result_components:u64,pub(super)role:Role,pub(super)reference:u32,pub(super)address:u32,pub(super)same:u32,pub(super)size:u32,pub(super)fallible:u32,pub(super)result:u32,pub(super)selector:u32,pub(super)field:u32,pub(super)extra:u32}
 #[repr(u32)]pub(super)enum EntryNeed{Body=0,Zero=1,Parameter=2,MissingResult=3,ReturnSource=4,CompletionField=5,MissingCompletion=6,CompletionReference=7,Frame=8,Field=9,MissingArgument=10,ValidateAddress=11,BadAddress=12,AddressLow=13,AddressHigh=14,Reference=15,ValidateType=16,BadType=17,LoadValue=18,Value=19,Extra=20,Unused=21,ResultBase=22,ResultReg=23,ErrorReg=24,Call=25,ErrorZero=26,ErrorCompare=27,ErrorIf=28,Store=29,Selector=30,Finish=31,Next=32}
 unsafe extern "C"{
  #[link_name="bootstrap.task_lower::start"]pub(super)fn start()->Cursor;
  #[link_name="bootstrap.task_lower::bind_step"]pub(super)fn bind_step(state:Cursor,facts:BindFacts)->Step<BindNeed>;
  #[link_name="bootstrap.task_lower::rewrite_block"]
  pub(super) fn rewrite_block(block: *mut c_void, count: usize, context: *mut c_void, host: RewriteHost) -> u32;
  #[link_name="bootstrap.task_lower::lookup_step"]pub(super)fn lookup_step(state:Cursor,facts:LookupFacts)->Step<LookupNeed>;
  #[link_name="bootstrap.task_lower::new_step"]pub(super)fn new_step(state:Cursor,facts:NewFacts)->Step<NewNeed>;
  #[link_name="bootstrap.task_lower::rewrite_step"]pub(super)fn rewrite_step(state:Cursor)->Step<RewriteNeed>;
  #[link_name="bootstrap.task_lower::entry_step"]pub(super)fn entry_step(state:Cursor,facts:EntryFacts)->Step<EntryNeed>;
 }
}
use wire as w;
struct TaskInput<'a>{ty:&'a Ty,offset:u32,buffer:[usize;2]}
struct TaskBinding{worker:String,input:Ty,start:Ty,index:usize,entry:FunctionBody,selector:Option<String>}
fn is_start(call: &Call) -> bool {
 matches!(&call.target, CallTarget::External(name) if name.symbol == "blink_task_start")
}
fn parameter(ty:&CallParameter)->(&Ty,Option<Access>){match &ty.kind{CallParameterKind::Value(ty)=>(ty,None),CallParameterKind::Reference{ty,access}=>(ty,Some(*access))}}
pub(crate)fn bind_tasks(module:&str,mut body:Option<&mut ValueCode>,definitions:&[FunctionBody],functions:&mut Vec<FunctionBody>,imports:&BTreeMap<String,Vec<u8>>,mut resources:Option<&mut Resources>)->Result<bool,&'static str>{
 let mut cursor=unsafe{w::start()};let mut facts=w::BindFacts{resources:u32::from(resources.is_some()),root:u32::from(body.is_some()),functions:functions.len()as u64,bindings:0};
 let mut symbol=String::new();let mut tasks=Vec::new();let mut entries=Vec::new().into_iter();
 loop{let next=unsafe{w::bind_step(cursor,facts)};cursor=next.cursor;match next.need{
  w::BindNeed::Start=>{symbol=format!("{}_task_start",resource_prefix(module));}
  w::BindNeed::Root=>{bind_task_entries(unsafe{body.take().unwrap_unchecked()},definitions,imports,unsafe{resources.as_deref_mut().unwrap_unchecked()},&mut tasks,&symbol)?;}
  w::BindNeed::Function=>{bind_task_entries(&mut functions[cursor.index as usize].body,definitions,imports,unsafe{resources.as_deref_mut().unwrap_unchecked()},&mut tasks,&symbol)?;}
  w::BindNeed::Entries=>{facts.bindings=tasks.len()as u64;entries=std::mem::take(&mut tasks).into_iter();}
  w::BindNeed::Entry=>{functions.push(unsafe{entries.next().unwrap_unchecked()}.entry);}
  w::BindNeed::Empty=>return Ok(false),w::BindNeed::Present=>return Ok(true),w::BindNeed::Next=>{},
 }}
}
fn bind_task_entries(body:&mut ValueCode,functions:&[FunctionBody],imports:&BTreeMap<String,Vec<u8>>,resources:&mut Resources,tasks:&mut Vec<TaskBinding>,symbol:&str)->Result<(),&'static str>{
 let mut needed = false;
 crate::optimize::visit(&body.code, &mut |inst| {
  if let InstKind::Call(call) = &inst.kind { needed |= is_start(call); }
 });
 if !needed { return Ok(()); }

 let mut addresses = BTreeMap::new();
 crate::optimize::visit(&body.code, &mut |inst| {
  if let InstKind::FunctionAddress(reg,CallTarget::Defined(name)) = &inst.kind {
   addresses.insert(*reg, name.clone());
  }
 });
 let mut rewrite = TaskRewrite {
  registers: &mut body.registers, addresses: &addresses, functions, imports, resources, tasks, symbol,
  result: Ok(false),
 };
 rewrite.block(&mut body.code);
 if rewrite.result? { crate::optimize::value_code(body); }
 Ok(())
}

struct TaskRewrite<'a> {
 registers: &'a mut Vec<Scalar>,
 addresses: &'a BTreeMap<usize, String>,
 functions: &'a [FunctionBody],
 imports: &'a BTreeMap<String, Vec<u8>>,
 resources: &'a mut Resources,
 tasks: &'a mut Vec<TaskBinding>,
 symbol: &'a str,
 result: Result<bool, &'static str>,
}

struct RewriteBlock<'a> {
 code: &'a mut Vec<Inst>,
 starts: Vec<(usize, usize)>,
 prelude: Vec<Inst>,
}

impl TaskRewrite<'_> {
 fn block(&mut self, code: &mut Vec<Inst>) -> u32 {
  let count = code.len();
  let mut block = RewriteBlock { code, starts: Vec::new(), prelude: Vec::new() };
  let host = w::RewriteHost { inspect: inspect_instruction, child: rewrite_child, start: rewrite_task_start };
  let status = unsafe { w::rewrite_block(ptr::from_mut(&mut block).cast(), count, ptr::from_mut(self).cast(), host) };
  if status == 0 && !block.starts.is_empty() {
   // Only this block's direct insertions change its storage. Child blocks have already been visited.
   let mut original = std::mem::take(block.code).into_iter();
   let mut prelude = block.prelude.into_iter();
   let mut rewritten = Vec::with_capacity(count + prelude.len());
   let mut previous = 0;
   for (index, inserted) in block.starts {
    rewritten.extend(original.by_ref().take(index - previous));
    rewritten.extend(prelude.by_ref().take(inserted));
    previous = index;
   }
   rewritten.extend(original);
   *block.code = rewritten;
  }
  status
 }
}

unsafe extern "C" fn inspect_instruction(block: *mut c_void, index: usize) -> w::Instruction {
 let block = unsafe { &mut *block.cast::<RewriteBlock<'_>>() };
 let inst = &mut block.code[index];
 let empty = ptr::null_mut();
 let (kind, first, second) = match &mut inst.kind {
  InstKind::Scope(body) => (w::Kind::Scope, ptr::from_mut(body).cast(), empty),
  InstKind::If { yes, no, .. } => (w::Kind::Pair, ptr::from_mut(yes).cast(), ptr::from_mut(no).cast()),
  InstKind::Loop { condition, body, .. } => (w::Kind::Pair, ptr::from_mut(condition).cast(), ptr::from_mut(body).cast()),
  InstKind::Call(call) if is_start(call) => (w::Kind::Start, empty, empty),
  _ => (w::Kind::Other, empty, empty),
 };
 w::Instruction { kind, first, second }
}

unsafe extern "C" fn rewrite_child(context: *mut c_void, code: *mut c_void) -> u32 {
 let rewrite = unsafe { &mut *context.cast::<TaskRewrite<'_>>() };
 rewrite.block(unsafe { &mut *code.cast::<Vec<Inst>>() })
}

unsafe extern "C" fn rewrite_task_start(context: *mut c_void, block: *mut c_void, index: usize) -> u32 {
 let rewrite = unsafe { &mut *context.cast::<TaskRewrite<'_>>() };
 let block = unsafe { &mut *block.cast::<RewriteBlock<'_>>() };
 // B only requests a start at an instruction classified by inspect_instruction.
 let InstKind::Call(call) = &mut block.code[index].kind else { unsafe { std::hint::unreachable_unchecked() } };
 let previous = block.prelude.len();
 match lower_start(call, rewrite.registers, rewrite.addresses, rewrite.functions, rewrite.imports,
                   rewrite.resources, rewrite.tasks, rewrite.symbol, &mut block.prelude) {
  Ok(()) => {
   block.starts.push((index, block.prelude.len() - previous));
   rewrite.result = Ok(true);
   0
  }
  Err(error) => { rewrite.result = Err(error); 1 }
 }
}

fn lower_start(call:&mut Call,registers:&mut Vec<Scalar>,addresses:&BTreeMap<usize,String>,functions:&[FunctionBody],imports:&BTreeMap<String,Vec<u8>>,resources:&mut Resources,tasks:&mut Vec<TaskBinding>,symbol:&str,output:&mut Vec<Inst>)->Result<(),&'static str>{
 let CallArgument::Reference{ty:input,..}=&call.arguments[1]else{return Err("task start needs an actual input packet")};
 let CallArgument::Value{values,..}=&call.arguments[2]else{return Err("task start needs a bound worker")};
 let worker=addresses.get(&values[0]).ok_or("missing actual task function address")?;
 let selector=call.arguments.get(4).and_then(|arg|match arg{CallArgument::Value{values,..}=>addresses.get(&values[0]).map(String::as_str),_=>None});
 let start=call.result.as_ref().ok_or("missing task start result type")?;
 let binding=binding_for(worker,input,start,selector,functions,imports,resources,tasks)?;
 rewrite_start(call,registers,&tasks[binding],symbol,output);
 Ok(())
}
fn binding_for(worker:&str,input:&Ty,start:&Ty,selector:Option<&str>,functions:&[FunctionBody],imports:&BTreeMap<String,Vec<u8>>,resources:&mut Resources,tasks:&mut Vec<TaskBinding>)->Result<usize,&'static str>{
 let mut cursor=unsafe{w::start()};let mut facts=w::LookupFacts{count:tasks.len()as u64,equal:0};
 loop{let next=unsafe{w::lookup_step(cursor,facts)};cursor=next.cursor;match next.need{
  w::LookupNeed::Worker=>facts.equal=u32::from(tasks[cursor.index as usize].worker==worker),
  w::LookupNeed::Input=>facts.equal=u32::from(tasks[cursor.index as usize].input==*input),
  w::LookupNeed::Start=>facts.equal=u32::from(tasks[cursor.index as usize].start==*start),
  w::LookupNeed::Selector=>facts.equal=u32::from(tasks[cursor.index as usize].selector.as_deref()==selector),
  w::LookupNeed::Found=>return Ok(cursor.index as usize),
  w::LookupNeed::Missing=>return new_binding(worker,input,start,selector,functions,imports,resources,tasks),
  w::LookupNeed::Next=>{},
 }}
}
type Signature<'a>=(Cow<'a,[CallParameter]>,Option<Cow<'a,Ty>>,bool);
fn signature<'a>(worker:&str,functions:&'a[FunctionBody],imports:&BTreeMap<String,Vec<u8>>)->Result<Signature<'a>,&'static str>{
 if let Some(function)=functions.iter().find(|function|function.name==worker){
  Ok((Cow::Borrowed(&function.parameters),function.result.as_ref().map(Cow::Borrowed),function.fallible))
 }else{
  let(parameters,result,fallible)=crate::check::compiled_task_signature(imports.get(worker).ok_or("missing compiled ordinary worker contract")?)?;
  Ok((Cow::Owned(parameters),result.map(Cow::Owned),fallible))
 }
}
fn new_binding(worker:&str,input:&Ty,start:&Ty,selector:Option<&str>,functions:&[FunctionBody],imports:&BTreeMap<String,Vec<u8>>,resources:&mut Resources,tasks:&mut Vec<TaskBinding>)->Result<usize,&'static str>{
 let(parameters,result,fallible)=signature(worker,functions,imports)?;
 let mut cursor=unsafe{w::start()};let mut facts=w::NewFacts{fields:input.fields.len()as u64,parameters:parameters.len()as u64,has_result:u32::from(result.is_some()),..w::NewFacts::default()};
 let mut completion=None;let mut output:Option<&Ty>=None;let mut type_index=0;let mut layout:Option<Ty>=None;let mut position_types=Vec::new();let mut name=None;let mut entry=None;let mut binding_index=0;
 loop{let next=unsafe{w::new_step(cursor,facts)};cursor=next.cursor;match next.need{
  w::NewNeed::CompletionResult=>{completion=Some(unsafe{result.as_deref().unwrap_unchecked()}.task_wait_layout().ok_or("task result layout is too large")?);}
  w::NewNeed::CompletionUnit=>{completion=Some(Ty::unit().task_wait_layout().ok_or("task result layout is too large")?);}
  w::NewNeed::Output=>{output=Some(input.arguments.last().unwrap());}
  w::NewNeed::Index=>{type_index=resources.types.len();}
  w::NewNeed::Layout=>{layout=Some(input.clone());}
  w::NewNeed::Parameter=>{facts.frame=u32::from(matches!(&parameters[cursor.part as usize].kind,CallParameterKind::Value(ty)if ty.name=="__blink_frame"));}
  w::NewNeed::Field=>{
   let(_,access)=parameter(&parameters[cursor.part as usize]);
   facts.owned=u32::from(unsafe{layout.as_ref().unwrap_unchecked()}.fields[cursor.index as usize].0.starts_with(Ty::TASK_OWNED_REFERENCE_PREFIX));
   facts.reference=u32::from(access.is_some());facts.inout=u32::from(access==Some(Access::InOut));
  }
  w::NewNeed::RenameWrite|w::NewNeed::RenameInOut=>{
   let field=&mut unsafe{layout.as_mut().unwrap_unchecked()}.fields[cursor.index as usize].0;
   let ready=field[Ty::TASK_OWNED_REFERENCE_PREFIX.len()..].split_once(':').map(|(_,offset)|offset);
   let mode=if matches!(next.need,w::NewNeed::RenameInOut){"inout"}else{"write"};
   let mut next_name=format!("{}{mode}:{}",Ty::TASK_OWNED_REFERENCE_PREFIX,type_index+3+position_types.len());
   if let Some(ready)=ready{next_name.push(':');next_name.push_str(ready);}
   *field=next_name;
  }
  w::NewNeed::PositionType=>{position_types.push(parameter(&parameters[cursor.part as usize]).0.clone());}
  w::NewNeed::Name=>{name=Some(format!("{worker}@task{}",tasks.len()));}
  w::NewNeed::Entry=>{
   let work_output=unsafe{output.unwrap_unchecked()};
   let fields=input.fields.iter().map(|(_,offset,ty)|TaskInput{ty,offset:*offset,buffer:[2,3]})
    .chain(work_output.fields.iter().map(|(_,offset,ty)|TaskInput{ty,offset:*offset,buffer:[4,5]}));
   entry=Some(task_entry(unsafe{name.take().unwrap_unchecked()},worker,&parameters,result.as_deref(),fallible,fields,selector)?);
  }
  w::NewNeed::Types=>{resources.types.extend([unsafe{layout.take().unwrap_unchecked()},unsafe{completion.take().unwrap_unchecked()},start.clone()]);}
  w::NewNeed::Positions=>{resources.types.extend(std::mem::take(&mut position_types));}
  w::NewNeed::Binding=>{
   binding_index=tasks.len();tasks.push(TaskBinding{worker:worker.into(),input:input.clone(),start:start.clone(),index:type_index,entry:unsafe{entry.take().unwrap_unchecked()},selector:selector.map(str::to_owned)});
  }
  w::NewNeed::Complete=>return Ok(binding_index),w::NewNeed::Next=>{},
 }}
}
fn argument(scalar:Scalar,reg:usize)->CallArgument{CallArgument::Value{ty:Ty::scalar(scalar),values:vec![reg],dataset_borrow:None,origin:None}}
fn rewrite_start(call:&mut Call,registers:&mut Vec<Scalar>,binding:&TaskBinding,symbol:&str,output:&mut Vec<Inst>){
 let mut cursor=unsafe{w::start()};let mut entry=0;
 loop{let next=unsafe{w::rewrite_step(cursor)};cursor=next.cursor;match next.need{
  w::RewriteNeed::Registers=>{entry=registers.len();registers.extend([Scalar::U64,Scalar::U32]);}
  w::RewriteNeed::Address=>output.push(Inst::new(InstKind::FunctionAddress(entry,CallTarget::Defined(binding.entry.name.clone())))),
  w::RewriteNeed::Index=>output.push(Inst::new(InstKind::Constant(entry+1,Scalar::U32,binding.index as u64))),
  w::RewriteNeed::Worker=>{call.arguments[2]=argument(Scalar::U64,entry);}
  w::RewriteNeed::Trim=>{call.arguments.truncate(4);}
  w::RewriteNeed::Argument=>call.arguments.push(argument(Scalar::U32,entry+1)),
  w::RewriteNeed::Symbol=>{let CallTarget::External(target)=&mut call.target else{unsafe{std::hint::unreachable_unchecked()}};target.symbol=symbol.into();}
  w::RewriteNeed::Complete=>return,
 }}
}
fn register(body:&mut ValueCode,scalar:Scalar)->usize{let reg=body.registers.len();body.registers.push(scalar);reg}
fn buffer(address:[usize;2],zero:usize)->IndexedSource{IndexedSource::Pointer{address,index:zero,stride:1,aliases:Vec::new()}}
fn load(body:&mut ValueCode,zero:usize,scalar:Scalar,address:[usize;2],word:u32)->usize{
 let dst=register(body,scalar);body.code.push(Inst::new(InstKind::LoadIndexed{dst,source:buffer(address,zero),offset:zero,word}));dst
}

fn task_entry<'a>(name:String,worker:&str,parameters:&[CallParameter],result:Option<&Ty>,fallible:bool,mut fields:impl Iterator<Item=TaskInput<'a>>,selector:Option<&str>)->Result<FunctionBody,&'static str>{
 let pointer=Ty::vector(Scalar::U32,2);let mut body:Option<ValueCode>=None;
 let mut cursor=unsafe{w::start()};let mut facts=w::EntryFacts{parameters:parameters.len()as u64,components:0,result_components:result.map_or(0,|ty|ty.components.len())as u64,role:w::Role::Ordinary,reference:0,address:0,same:0,size:0,fallible:u32::from(fallible),result:u32::from(result.is_some()),selector:u32::from(selector.is_some()),field:0,extra:0};
 let mut zero=0;let mut arguments=Vec::new();let mut field:Option<TaskInput<'a>>=None;let mut parameter_values=Vec::new();let mut address=[0;2];let mut results=Vec::new();let mut result_base=0;let mut error=None;let mut none=0;let mut failed=0;
 loop{let next=unsafe{w::entry_step(cursor,facts)};cursor=next.cursor;match next.need{
  w::EntryNeed::Body=>{body=Some(ValueCode{entry_origin:None,generated_origin:None,registers:vec![Scalar::U32;6],locals:Default::default(),code:Vec::new(),inputs:(0..6).collect(),outputs:Vec::new()});}
  w::EntryNeed::Zero=>{let b=unsafe{body.as_mut().unwrap_unchecked()};zero=register(b,Scalar::U32);b.code.push(Inst::new(InstKind::Constant(zero,Scalar::U32,0)));}
  w::EntryNeed::Parameter=>{
   let(ty,access)=parameter(&parameters[cursor.index as usize]);field=None;parameter_values.clear();
   facts.role=if ty.name==Ty::TASK_RETURN_SOURCE{w::Role::ReturnSource}else if ty.name==Ty::TASK_COMPLETION{w::Role::Completion}else if ty.name=="__blink_frame"{w::Role::Frame}else{w::Role::Ordinary};
   facts.reference=u32::from(access.is_some());facts.components=ty.components.len()as u64;facts.size=ty.size;
  }
  w::EntryNeed::MissingResult=>return Err("missing result for task return source"),
  w::EntryNeed::ReturnSource=>{
   let ty=parameter(&parameters[cursor.index as usize]).0;let word=unsafe{result.unwrap_unchecked()}.size;
   arguments.push(CallArgument::Reference{origin:None,ty:ty.clone(),access:Access::Write,address:CallReference{storage:CallStorage::Indexed(buffer([4,5],zero)),offset:None,word}});
  }
  w::EntryNeed::CompletionField|w::EntryNeed::Field=>{field=fields.next();facts.field=u32::from(field.is_some());}
  w::EntryNeed::MissingCompletion=>return Err("missing task input completion field"),
  w::EntryNeed::CompletionReference=>{
   let field=unsafe{field.as_ref().unwrap_unchecked()};let ty=parameter(&parameters[cursor.index as usize]).0;
   arguments.push(CallArgument::Reference{origin:None,ty:ty.clone(),access:Access::InOut,address:CallReference{storage:CallStorage::Indexed(buffer(field.buffer,zero)),offset:None,word:field.offset}});
  }
  w::EntryNeed::Frame=>{parameter_values=vec![0,1];}
  w::EntryNeed::MissingArgument=>return Err("missing task argument"),
  w::EntryNeed::ValidateAddress=>{facts.address=u32::from(unsafe{field.as_ref().unwrap_unchecked()}.ty.is_address());}
  w::EntryNeed::BadAddress=>return Err("task reference needs the original position address"),
  w::EntryNeed::AddressLow=>{
   let field=unsafe{field.as_ref().unwrap_unchecked()};address[0]=load(unsafe{body.as_mut().unwrap_unchecked()},zero,Scalar::U32,field.buffer,field.offset);
  }
  w::EntryNeed::AddressHigh=>{
   let field=unsafe{field.as_ref().unwrap_unchecked()};address[1]=load(unsafe{body.as_mut().unwrap_unchecked()},zero,Scalar::U32,field.buffer,field.offset+4);
  }
  w::EntryNeed::Reference=>{
   let(ty,access)=parameter(&parameters[cursor.index as usize]);
   arguments.push(CallArgument::Reference{ty:ty.clone(),access:unsafe{access.unwrap_unchecked()},origin:None,address:CallReference{storage:CallStorage::Indexed(buffer(address,zero)),offset:None,word:0}});
  }
  w::EntryNeed::ValidateType=>{facts.same=u32::from(unsafe{field.as_ref().unwrap_unchecked()}.ty==parameter(&parameters[cursor.index as usize]).0);}
  w::EntryNeed::BadType=>return Err("task input layout differs from the ordinary parameter"),
  w::EntryNeed::LoadValue=>{
   let field=unsafe{field.as_ref().unwrap_unchecked()};let(scalar,offset)=parameter(&parameters[cursor.index as usize]).0.components[cursor.part as usize];
   parameter_values.push(load(unsafe{body.as_mut().unwrap_unchecked()},zero,scalar,field.buffer,field.offset+offset));
  }
  w::EntryNeed::Value=>{arguments.push(CallArgument::Value{ty:parameter(&parameters[cursor.index as usize]).0.clone(),values:std::mem::take(&mut parameter_values),dataset_borrow:None,origin:None});}
  w::EntryNeed::Extra=>{facts.extra=u32::from(fields.next().is_some());}
  w::EntryNeed::Unused=>return Err("unused task argument"),
  w::EntryNeed::ResultBase=>{result_base=unsafe{body.as_ref().unwrap_unchecked()}.registers.len();}
  w::EntryNeed::ResultReg=>{let scalar=unsafe{result.unwrap_unchecked()}.components[cursor.part as usize].0;results.push(register(unsafe{body.as_mut().unwrap_unchecked()},scalar));}
  w::EntryNeed::ErrorReg=>{error=Some(register(unsafe{body.as_mut().unwrap_unchecked()},Scalar::U64));}
  w::EntryNeed::Call=>{
   unsafe{body.as_mut().unwrap_unchecked()}.code.push(Inst::new(InstKind::Call(Call{failure:None,error,target:CallTarget::Defined(worker.into()),arguments:std::mem::take(&mut arguments),memory:Vec::new(),memory_use:None,result:result.cloned(),values:std::mem::take(&mut results)})));
  }
  w::EntryNeed::ErrorZero=>{let b=unsafe{body.as_mut().unwrap_unchecked()};none=register(b,Scalar::U64);b.code.push(Inst::new(InstKind::Constant(none,Scalar::U64,0)));}
  w::EntryNeed::ErrorCompare=>{
   let b=unsafe{body.as_mut().unwrap_unchecked()};failed=register(b,Scalar::Bool);b.code.push(Inst::new(InstKind::Binary{dst:failed,op:BinaryOp::Compare(Comparison::NotEqual),a:unsafe{error.unwrap_unchecked()},b:none}));
  }
  w::EntryNeed::ErrorIf=>{unsafe{body.as_mut().unwrap_unchecked()}.code.push(Inst::new(InstKind::If{condition:failed,yes:vec![Inst::new(InstKind::Raise(unsafe{error.unwrap_unchecked()}))],no:Vec::new()}));}
  w::EntryNeed::Store=>{
   let word=unsafe{result.unwrap_unchecked()}.components[cursor.part as usize].1;
   unsafe{body.as_mut().unwrap_unchecked()}.code.push(Inst::new(InstKind::StoreIndexed{source:buffer([4,5],zero),offset:zero,word,value:result_base+cursor.part as usize}));
  }
  w::EntryNeed::Selector=>{
   let arguments=[[2,3],[4,5]].into_iter().map(|values|CallArgument::Value{ty:pointer.clone(),values:values.to_vec(),dataset_borrow:None,origin:None}).collect();
   unsafe{body.as_mut().unwrap_unchecked()}.code.push(Inst::new(InstKind::Call(Call{failure:None,error:None,target:CallTarget::Defined(unsafe{selector.unwrap_unchecked()}.into()),arguments,memory:Vec::new(),memory_use:None,result:None,values:Vec::new()})));
  }
  w::EntryNeed::Finish=>return Ok(FunctionBody{name,fallible:true,source_origin:None,parameters:vec![CallParameter::value(pointer);3],result:None,body:unsafe{body.take().unwrap_unchecked()}}),
  w::EntryNeed::Next=>{},
 }}
}
