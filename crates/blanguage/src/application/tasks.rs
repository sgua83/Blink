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
 #[repr(C)]pub(super)struct BindHost {
  pub(super) root: unsafe extern "C" fn(*mut c_void) -> u32,
  pub(super) function: unsafe extern "C" fn(*mut c_void, usize) -> u32,
 }
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
 #[repr(C)]pub(super)struct EntrySpec {
  pub(super) parameters: usize,
  pub(super) fields: usize,
  pub(super) results: usize,
  pub(super) result_bytes: u32,
  pub(super) has_result: u32,
  pub(super) fallible: u32,
  pub(super) selector: u32,
 }
 #[repr(C)]pub(super)struct EntryParameter {
  pub(super) role: Role,
  pub(super) reference: u32,
  pub(super) size: u32,
  pub(super) components: usize,
 }
 #[derive(Clone, Copy)]
 #[repr(C)]pub(super)struct EntryField {
  pub(super) ty: *const super::Ty,
  pub(super) offset: u32,
  pub(super) buffer: usize,
 }
 #[repr(C)]pub(super)struct EntryAddress { pub(super) low: usize, pub(super) high: usize }
 #[repr(C)]pub(super)struct EntryCall { pub(super) result: usize, pub(super) error: usize }
 #[allow(dead_code)] // Produced by bootstrap.task_lower.
 #[repr(u32)]pub(super)enum EntryStatus {
  Complete, MissingResult, MissingCompletion, MissingArgument, BadAddress, BadType, Unused,
 }
 #[repr(C)]pub(super)struct EntryHost {
  pub(super) parameter: unsafe extern "C" fn(*mut c_void, usize) -> EntryParameter,
  pub(super) field: unsafe extern "C" fn(*mut c_void, usize) -> EntryField,
  pub(super) valid: unsafe extern "C" fn(*mut c_void, usize, *const super::Ty, u32) -> u32,
  pub(super) pointer: unsafe extern "C" fn(*mut c_void, EntryField) -> EntryAddress,
  pub(super) reference: unsafe extern "C" fn(*mut c_void, usize, Role, EntryAddress, u32),
  pub(super) load_value: unsafe extern "C" fn(*mut c_void, usize, usize, EntryField) -> usize,
  pub(super) value: unsafe extern "C" fn(*mut c_void, usize, usize, usize),
  pub(super) call: unsafe extern "C" fn(*mut c_void, u32) -> EntryCall,
  pub(super) raise_error: unsafe extern "C" fn(*mut c_void, usize),
  pub(super) store: unsafe extern "C" fn(*mut c_void, usize, usize),
  pub(super) selector: unsafe extern "C" fn(*mut c_void),
 }
 unsafe extern "C"{
  #[link_name="bootstrap.task_lower::start"]pub(super)fn start()->Cursor;
  #[link_name="bootstrap.task_lower::bind"]
  pub(super) fn bind(context: *mut c_void, root: u32, functions: usize, host: BindHost) -> u32;
  #[link_name="bootstrap.task_lower::rewrite_block"]
  pub(super) fn rewrite_block(block: *mut c_void, count: usize, context: *mut c_void, host: RewriteHost) -> u32;
  #[link_name="bootstrap.task_lower::lookup_step"]pub(super)fn lookup_step(state:Cursor,facts:LookupFacts)->Step<LookupNeed>;
  #[link_name="bootstrap.task_lower::new_step"]pub(super)fn new_step(state:Cursor,facts:NewFacts)->Step<NewNeed>;
  #[link_name="bootstrap.task_lower::rewrite_step"]pub(super)fn rewrite_step(state:Cursor)->Step<RewriteNeed>;
  #[link_name="bootstrap.task_lower::emit_entry"]
  pub(super) fn emit_entry(context: *mut c_void, spec: EntrySpec, host: EntryHost) -> EntryStatus;
 }
}
use wire as w;
struct TaskBinding{worker:String,input:Ty,start:Ty,index:usize,entry:FunctionBody,selector:Option<String>}
fn is_start(call: &Call) -> bool {
 matches!(&call.target, CallTarget::External(name) if name.symbol == "blink_task_start")
}
fn parameter(ty:&CallParameter)->(&Ty,Option<Access>){match &ty.kind{CallParameterKind::Value(ty)=>(ty,None),CallParameterKind::Reference{ty,access}=>(ty,Some(*access))}}
pub(crate) enum NativeFunction<'a> {
 Borrowed(&'a FunctionBody),
 Owned(Box<FunctionBody>),
}
impl NativeFunction<'_> {
 pub(crate) fn emit(self) -> Result<(String, crate::native::NativeCode), &'static str> {
  match self {
   Self::Borrowed(function) => crate::native::emit_function(function).map(|code| (function.name.clone(), code)),
   Self::Owned(function) => crate::native::emit_function(&function).map(|code| (function.name, code)),
  }
 }
}

struct TaskTypes {
 original_count: usize,
 added: Vec<Ty>,
}

struct TaskBindingInput<'a, 'src> {
 root: Option<&'a mut Cow<'src, ValueCode>>,
 definitions: &'src [FunctionBody],
 jobs: &'a mut [NativeFunction<'src>],
 imports: &'a BTreeMap<String, Vec<u8>>,
 types: TaskTypes,
 tasks: Vec<TaskBinding>,
 symbol: String,
 error: Option<&'static str>,
}

pub(crate) fn prepare_functions<'src>(
 module: &str,
 root: Option<&mut Cow<'src, ValueCode>>,
 definitions: &'src [FunctionBody],
 imports: &BTreeMap<String, Vec<u8>>,
 resources: Option<&Resources>,
) -> Result<(Vec<NativeFunction<'src>>, Vec<Ty>), &'static str> {
 let mut jobs: Vec<_> = definitions.iter().map(NativeFunction::Borrowed).collect();
 let Some(resources) = resources.filter(|resources| resources.task_starts) else {
  return Ok((jobs, Vec::new()));
 };

 let count = jobs.len();
 let has_root = u32::from(root.is_some());
 let mut input = TaskBindingInput {
  root, definitions, jobs: &mut jobs, imports,
  types: TaskTypes { original_count: resources.types.len(), added: Vec::new() },
  tasks: Vec::new(), symbol: format!("{}_task_start", resource_prefix(module)), error: None,
 };
 let host = w::BindHost { root: bind_root, function: bind_function };
 unsafe { w::bind(ptr::from_mut(&mut input).cast(), has_root, count, host) };
 if let Some(error) = input.error { return Err(error); }
 let TaskBindingInput { types, tasks, .. } = input;
 jobs.extend(tasks.into_iter().map(|task| NativeFunction::Owned(Box::new(task.entry))));
 Ok((jobs, types.added))
}

fn contains_start(code: &[Inst]) -> bool {
 code.iter().any(|inst| match &inst.kind {
  InstKind::Call(call) => is_start(call),
  InstKind::Scope(body) => contains_start(body),
  InstKind::If { yes, no, .. } => contains_start(yes) || contains_start(no),
  InstKind::Loop { condition, body, .. } => contains_start(condition) || contains_start(body),
  _ => false,
 })
}

unsafe extern "C" fn bind_root(context: *mut c_void) -> u32 {
 let input = unsafe { &mut *context.cast::<TaskBindingInput<'_, '_>>() };
 let body = input.root.as_deref_mut().unwrap();
 if !contains_start(&body.code) { return 0; }
 let result = bind_task_entries(body.to_mut(), input.definitions, input.imports,
                                &mut input.types, &mut input.tasks, &input.symbol);
 input.error = result.err();
 u32::from(input.error.is_some())
}

unsafe extern "C" fn bind_function(context: *mut c_void, index: usize) -> u32 {
 let input = unsafe { &mut *context.cast::<TaskBindingInput<'_, '_>>() };
 let definition = &input.definitions[index];
 if !contains_start(&definition.body.code) { return 0; }
 let mut function = Box::new(definition.clone());
 let result = bind_task_entries(&mut function.body, input.definitions, input.imports,
                                &mut input.types, &mut input.tasks, &input.symbol);
 input.error = result.err();
 if input.error.is_none() { input.jobs[index] = NativeFunction::Owned(function); }
 u32::from(input.error.is_some())
}

fn bind_task_entries(body:&mut ValueCode,functions:&[FunctionBody],imports:&BTreeMap<String,Vec<u8>>,types:&mut TaskTypes,tasks:&mut Vec<TaskBinding>,symbol:&str)->Result<(),&'static str>{
 let mut addresses = BTreeMap::new();
 crate::optimize::visit(&body.code, &mut |inst| {
  if let InstKind::FunctionAddress(reg,CallTarget::Defined(name)) = &inst.kind {
   addresses.insert(*reg, name.clone());
  }
 });
 let mut rewrite = TaskRewrite {
  registers: &mut body.registers, addresses: &addresses, functions, imports, types, tasks, symbol,
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
 types: &'a mut TaskTypes,
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
                   rewrite.types, rewrite.tasks, rewrite.symbol, &mut block.prelude) {
  Ok(()) => {
   block.starts.push((index, block.prelude.len() - previous));
   rewrite.result = Ok(true);
   0
  }
  Err(error) => { rewrite.result = Err(error); 1 }
 }
}

fn lower_start(call:&mut Call,registers:&mut Vec<Scalar>,addresses:&BTreeMap<usize,String>,functions:&[FunctionBody],imports:&BTreeMap<String,Vec<u8>>,types:&mut TaskTypes,tasks:&mut Vec<TaskBinding>,symbol:&str,output:&mut Vec<Inst>)->Result<(),&'static str>{
 let CallArgument::Reference{ty:input,..}=&call.arguments[1]else{return Err("task start needs an actual input packet")};
 let CallArgument::Value{values,..}=&call.arguments[2]else{return Err("task start needs a bound worker")};
 let worker=addresses.get(&values[0]).ok_or("missing actual task function address")?;
 let selector=call.arguments.get(4).and_then(|arg|match arg{CallArgument::Value{values,..}=>addresses.get(&values[0]).map(String::as_str),_=>None});
 let start=call.result.as_ref().ok_or("missing task start result type")?;
 let binding=binding_for(worker,input,start,selector,functions,imports,types,tasks)?;
 rewrite_start(call,registers,&tasks[binding],symbol,output);
 Ok(())
}
fn binding_for(worker:&str,input:&Ty,start:&Ty,selector:Option<&str>,functions:&[FunctionBody],imports:&BTreeMap<String,Vec<u8>>,types:&mut TaskTypes,tasks:&mut Vec<TaskBinding>)->Result<usize,&'static str>{
 let mut cursor=unsafe{w::start()};let mut facts=w::LookupFacts{count:tasks.len()as u64,equal:0};
 loop{let next=unsafe{w::lookup_step(cursor,facts)};cursor=next.cursor;match next.need{
  w::LookupNeed::Worker=>facts.equal=u32::from(tasks[cursor.index as usize].worker==worker),
  w::LookupNeed::Input=>facts.equal=u32::from(tasks[cursor.index as usize].input==*input),
  w::LookupNeed::Start=>facts.equal=u32::from(tasks[cursor.index as usize].start==*start),
  w::LookupNeed::Selector=>facts.equal=u32::from(tasks[cursor.index as usize].selector.as_deref()==selector),
  w::LookupNeed::Found=>return Ok(cursor.index as usize),
  w::LookupNeed::Missing=>return new_binding(worker,input,start,selector,functions,imports,types,tasks),
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
fn new_binding(worker:&str,input:&Ty,start:&Ty,selector:Option<&str>,functions:&[FunctionBody],imports:&BTreeMap<String,Vec<u8>>,types:&mut TaskTypes,tasks:&mut Vec<TaskBinding>)->Result<usize,&'static str>{
 let(parameters,result,fallible)=signature(worker,functions,imports)?;
 let mut cursor=unsafe{w::start()};let mut facts=w::NewFacts{fields:input.fields.len()as u64,parameters:parameters.len()as u64,has_result:u32::from(result.is_some()),..w::NewFacts::default()};
 let mut completion=None;let mut output:Option<&Ty>=None;let mut type_index=0;let mut layout:Option<Ty>=None;let mut position_types=Vec::new();let mut name=None;let mut entry=None;let mut binding_index=0;
 loop{let next=unsafe{w::new_step(cursor,facts)};cursor=next.cursor;match next.need{
  w::NewNeed::CompletionResult=>{completion=Some(unsafe{result.as_deref().unwrap_unchecked()}.task_wait_layout().ok_or("task result layout is too large")?);}
  w::NewNeed::CompletionUnit=>{completion=Some(Ty::unit().task_wait_layout().ok_or("task result layout is too large")?);}
  w::NewNeed::Output=>{output=Some(input.arguments.last().unwrap());}
  w::NewNeed::Index=>{type_index=types.original_count+types.added.len();}
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
   entry=Some(task_entry(unsafe{name.take().unwrap_unchecked()},worker,&parameters,result.as_deref(),fallible,input,work_output,selector)?);
  }
  w::NewNeed::Types=>{types.added.extend([unsafe{layout.take().unwrap_unchecked()},unsafe{completion.take().unwrap_unchecked()},start.clone()]);}
  w::NewNeed::Positions=>{types.added.extend(std::mem::take(&mut position_types));}
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

struct TaskEntry<'a> {
 body: ValueCode,
 arguments: Vec<CallArgument>,
 worker: &'a str,
 parameters: &'a [CallParameter],
 result: Option<&'a Ty>,
 input: &'a Ty,
 output: &'a Ty,
 selector: Option<&'a str>,
 zero: usize,
}

unsafe extern "C" fn entry_parameter(context: *mut c_void, index: usize) -> w::EntryParameter {
 let state = unsafe { &*context.cast::<TaskEntry<'_>>() };
 let (ty, access) = parameter(&state.parameters[index]);
 let role = match ty.name.as_str() {
  Ty::TASK_RETURN_SOURCE => w::Role::ReturnSource,
  Ty::TASK_COMPLETION => w::Role::Completion,
  "__blink_frame" => w::Role::Frame,
  _ => w::Role::Ordinary,
 };
 w::EntryParameter {
  role, reference: u32::from(access.is_some()), size: ty.size, components: ty.components.len(),
 }
}

unsafe extern "C" fn entry_field(context: *mut c_void, index: usize) -> w::EntryField {
 let state = unsafe { &*context.cast::<TaskEntry<'_>>() };
 let inputs = state.input.fields.len();
 let ((_, offset, ty), buffer) = if index < inputs {
  (&state.input.fields[index], 2)
 } else {
  (&state.output.fields[index - inputs], 4)
 };
 w::EntryField { ty: ty.as_ref(), offset: *offset, buffer }
}

unsafe extern "C" fn entry_valid(context: *mut c_void, index: usize, ty: *const Ty, reference: u32) -> u32 {
 let state = unsafe { &*context.cast::<TaskEntry<'_>>() };
 let actual = unsafe { &*ty };
 u32::from(if reference != 0 {
  actual.is_address()
 } else {
  actual == parameter(&state.parameters[index]).0
 })
}

unsafe extern "C" fn entry_pointer(context: *mut c_void, field: w::EntryField) -> w::EntryAddress {
 let state = unsafe { &mut *context.cast::<TaskEntry<'_>>() };
 let buffer = [field.buffer, field.buffer + 1];
 let low = load(&mut state.body, state.zero, Scalar::U32, buffer, field.offset);
 let high = load(&mut state.body, state.zero, Scalar::U32, buffer, field.offset + 4);
 w::EntryAddress { low, high }
}

unsafe extern "C" fn entry_reference(context: *mut c_void, index: usize, role: w::Role, address: w::EntryAddress, word: u32) {
 let state = unsafe { &mut *context.cast::<TaskEntry<'_>>() };
 let (ty, access) = parameter(&state.parameters[index]);
 let access = match role {
  w::Role::ReturnSource => Access::Write,
  w::Role::Completion => Access::InOut,
  _ => unsafe { access.unwrap_unchecked() },
 };
 state.arguments.push(CallArgument::Reference {
  ty: ty.clone(), access, origin: None,
  address: CallReference {
   storage: CallStorage::Indexed(buffer([address.low, address.high], state.zero)),
   offset: None, word,
  },
 });
}

unsafe extern "C" fn entry_load_value(context: *mut c_void, index: usize, part: usize, field: w::EntryField) -> usize {
 let state = unsafe { &mut *context.cast::<TaskEntry<'_>>() };
 let (scalar, offset) = parameter(&state.parameters[index]).0.components[part];
 load(&mut state.body, state.zero, scalar, [field.buffer, field.buffer + 1], field.offset + offset)
}

unsafe extern "C" fn entry_value(context: *mut c_void, index: usize, first: usize, count: usize) {
 let state = unsafe { &mut *context.cast::<TaskEntry<'_>>() };
 state.arguments.push(CallArgument::Value {
  ty: parameter(&state.parameters[index]).0.clone(), values: (first..first + count).collect(),
  dataset_borrow: None, origin: None,
 });
}

unsafe extern "C" fn entry_call(context: *mut c_void, fallible: u32) -> w::EntryCall {
 let state = unsafe { &mut *context.cast::<TaskEntry<'_>>() };
 let result = state.body.registers.len();
 if let Some(ty) = state.result {
  state.body.registers.extend(ty.components.iter().map(|field| field.0));
 }
 let values = (result..state.body.registers.len()).collect();
 let error = (fallible != 0).then(|| register(&mut state.body, Scalar::U64));
 state.body.code.push(Inst::new(InstKind::Call(Call {
  target: CallTarget::Defined(state.worker.into()), arguments: std::mem::take(&mut state.arguments),
  failure: None, error, memory: Vec::new(), memory_use: None, result: state.result.cloned(), values,
 })));
 w::EntryCall { result, error: error.unwrap_or(0) }
}

unsafe extern "C" fn entry_raise_error(context: *mut c_void, error: usize) {
 let state = unsafe { &mut *context.cast::<TaskEntry<'_>>() };
 let code = &mut state.body;
 let none = register(code, Scalar::U64);
 code.code.push(Inst::new(InstKind::Constant(none, Scalar::U64, 0)));
 let failed = register(code, Scalar::Bool);
 code.code.push(Inst::new(InstKind::Binary {
  dst: failed, op: BinaryOp::Compare(Comparison::NotEqual), a: error, b: none,
 }));
 code.code.push(Inst::new(InstKind::If {
  condition: failed, yes: vec![Inst::new(InstKind::Raise(error))], no: Vec::new(),
 }));
}

unsafe extern "C" fn entry_store(context: *mut c_void, part: usize, reg: usize) {
 let state = unsafe { &mut *context.cast::<TaskEntry<'_>>() };
 let word = unsafe { state.result.unwrap_unchecked() }.components[part].1;
 state.body.code.push(Inst::new(InstKind::StoreIndexed {
  source: buffer([4, 5], state.zero), offset: state.zero, word, value: reg,
 }));
}

unsafe extern "C" fn entry_selector(context: *mut c_void) {
 let state = unsafe { &mut *context.cast::<TaskEntry<'_>>() };
 let pointer = Ty::vector(Scalar::U32, 2);
 let arguments = [[2, 3], [4, 5]].into_iter().map(|values| CallArgument::Value {
  ty: pointer.clone(), values: values.to_vec(), dataset_borrow: None, origin: None,
 }).collect();
 let target = CallTarget::Defined(unsafe { state.selector.unwrap_unchecked() }.into());
 state.body.code.push(Inst::new(InstKind::Call(Call {
  target, arguments, failure: None, error: None, memory: Vec::new(), memory_use: None,
  result: None, values: Vec::new(),
 })));
}

fn task_entry(
 name: String, worker: &str, parameters: &[CallParameter], result: Option<&Ty>,
 fallible: bool, input: &Ty, output: &Ty, selector: Option<&str>,
) -> Result<FunctionBody, &'static str> {
 let mut body = ValueCode {
  entry_origin: None, generated_origin: None, registers: vec![Scalar::U32; 6],
  locals: Default::default(), code: Vec::new(), inputs: (0..6).collect(), outputs: Vec::new(),
 };
 let zero = register(&mut body, Scalar::U32);
 body.code.push(Inst::new(InstKind::Constant(zero, Scalar::U32, 0)));
 let mut state = TaskEntry { body, arguments: Vec::new(), worker, parameters, result, input, output, selector, zero };
 let spec = w::EntrySpec {
  parameters: parameters.len(), fields: input.fields.len() + output.fields.len(),
  results: result.map_or(0, |ty| ty.components.len()), result_bytes: result.map_or(0, |ty| ty.size),
  has_result: u32::from(result.is_some()), fallible: u32::from(fallible), selector: u32::from(selector.is_some()),
 };
 let host = w::EntryHost {
  parameter: entry_parameter, field: entry_field, valid: entry_valid, pointer: entry_pointer,
  reference: entry_reference, load_value: entry_load_value, value: entry_value,
  call: entry_call, raise_error: entry_raise_error, store: entry_store, selector: entry_selector,
 };
 match unsafe { w::emit_entry(ptr::from_mut(&mut state).cast(), spec, host) } {
  w::EntryStatus::Complete => Ok(FunctionBody {
   name, fallible: true, source_origin: None,
   parameters: vec![CallParameter::value(Ty::vector(Scalar::U32, 2)); 3],
   result: None, body: state.body,
  }),
  w::EntryStatus::MissingResult => Err("missing result for task return source"),
  w::EntryStatus::MissingCompletion => Err("missing task input completion field"),
  w::EntryStatus::MissingArgument => Err("missing task argument"),
  w::EntryStatus::BadAddress => Err("task reference needs the original position address"),
  w::EntryStatus::BadType => Err("task input layout differs from the ordinary parameter"),
  w::EntryStatus::Unused => Err("unused task argument"),
 }
}
