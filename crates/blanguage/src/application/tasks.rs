//! B controls task binding, structured start-call rewrite and actual worker entries.
use super::{Resources,resource_prefix};
use crate::ast::Access;
use crate::ir::{self,BinaryOp,Call,CallArgument,CallParameter,CallParameterKind,CallReference,CallStorage,CallTarget,Comparison,FunctionBody,IndexedSource,Inst,InstKind,Scalar,Ty,ValueCode};
use std::{borrow::Cow, collections::{BTreeMap, BTreeSet}, ffi::c_void, ptr, vec::IntoIter};
mod wire{
 use std::ffi::c_void;
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
 #[repr(C)]pub(super)struct ScanHost {
  pub(super) ir: super::ir::dataflow::Host,
  pub(super) address: unsafe extern "C" fn(*mut c_void, *const super::Inst),
  pub(super) start: unsafe extern "C" fn(*mut c_void, *const super::Inst) -> u32,
 }
 #[repr(C)]pub(super)struct BoundStart {
  pub(super) binding: usize,
  pub(super) worker_argument: usize,
  pub(super) retained_arguments: usize,
 }
 #[repr(C)]pub(super)struct StartHost {
  pub(super) matches: unsafe extern "C" fn(*mut c_void, usize) -> u32,
  pub(super) create: unsafe extern "C" fn(*mut c_void),
 }
 #[repr(u32)]pub(super)enum BindingParameter { Frame, Value, Reference, InOut }
 #[repr(C)]pub(super)struct PositionHost {
  pub(super) parameter: unsafe extern "C" fn(*mut c_void, usize) -> BindingParameter,
  pub(super) owned: unsafe extern "C" fn(*mut c_void, usize) -> u32,
  pub(super) publish: unsafe extern "C" fn(*mut c_void, usize, usize, usize, BindingParameter),
 }
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
  #[link_name="bootstrap.task_lower::bind"]
  pub(super) fn bind(context: *mut c_void, root: u32, functions: usize, host: BindHost) -> u32;
  #[link_name="bootstrap.task_lower::rewrite_block"]
  pub(super) fn rewrite_block(block: *mut c_void, count: usize, context: *mut c_void, host: RewriteHost) -> u32;
  #[link_name="bootstrap.task_lower::scan_starts"]
  pub(super) fn scan_starts(code: super::ir::dataflow::Span, context: *mut c_void, rewrite: u32, host: ScanHost) -> u32;
  #[link_name="bootstrap.task_lower::bind_start"]
  pub(super) fn bind_start(context: *mut c_void, count: usize, host: StartHost) -> BoundStart;
  #[link_name="bootstrap.task_lower::bind_positions"]
  pub(super) fn bind_positions(context: *mut c_void, fields: usize, parameters: usize, first: usize, host: PositionHost);
  #[link_name="bootstrap.task_lower::emit_entry"]
  pub(super) fn emit_entry(context: *mut c_void, spec: EntrySpec, host: EntryHost) -> EntryStatus;
 }
}
use wire as w;
struct TaskBinding{worker:String,input:Ty,index:usize,entry:FunctionBody,selector:Option<String>}
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

#[derive(Default)]
struct TaskScan<'a> {
 definitions: Vec<(usize, &'a str)>,
 requested: BTreeSet<usize>,
}

unsafe extern "C" fn scan_address(context: *mut c_void, instruction: *const Inst) {
 let scan = unsafe { &mut *context.cast::<TaskScan<'_>>() };
 let InstKind::FunctionAddress(reg, CallTarget::Defined(name)) = &unsafe { &*instruction }.kind else {
  unsafe { std::hint::unreachable_unchecked() }
 };
 scan.definitions.push((*reg, name));
}

unsafe extern "C" fn scan_start(context: *mut c_void, instruction: *const Inst) -> u32 {
 let InstKind::Call(call) = &unsafe { &*instruction }.kind else {
  unsafe { std::hint::unreachable_unchecked() }
 };
 if !is_start(call) { return 0; }
 let scan = unsafe { &mut *context.cast::<TaskScan<'_>>() };
 scan.requested.extend([task_register(call, 2), task_register(call, 4)].into_iter().flatten());
 1
}

fn task_register(call: &Call, index: usize) -> Option<usize> {
 match call.arguments.get(index)? {
  CallArgument::Value { values, .. } => values.first().copied(),
  _ => None,
 }
}

fn task_addresses(code: &[Inst]) -> Option<BTreeMap<usize, String>> {
 let mut scan = TaskScan::default();
 let host = w::ScanHost { ir: ir::dataflow::HOST, address: scan_address, start: scan_start };
 // The scan only borrows from this original code. Selected names become owned
 // before the mutable rewrite starts, including when the original code is moved.
 let found = unsafe { w::scan_starts(ir::dataflow::Span::slice(code), ptr::from_mut(&mut scan).cast(), 1, host) };
 if found == 0 { return None; }
 Some(scan.definitions.into_iter().rev()
  .filter(|(reg, _)| scan.requested.remove(reg))
  .map(|(reg, name)| (reg, name.to_owned())).collect())
}

unsafe extern "C" fn bind_root(context: *mut c_void) -> u32 {
 let input = unsafe { &mut *context.cast::<TaskBindingInput<'_, '_>>() };
 let body = input.root.as_deref_mut().unwrap();
 let Some(addresses) = task_addresses(&body.code) else { return 0; };
 let result = bind_task_entries(body.to_mut(), &addresses, input.definitions, input.imports,
                                &mut input.types, &mut input.tasks, &input.symbol);
 input.error = result.err();
 u32::from(input.error.is_some())
}

unsafe extern "C" fn bind_function(context: *mut c_void, index: usize) -> u32 {
 let input = unsafe { &mut *context.cast::<TaskBindingInput<'_, '_>>() };
 let definition = &input.definitions[index];
 let Some(addresses) = task_addresses(&definition.body.code) else { return 0; };
 let mut function = Box::new(definition.clone());
 let result = bind_task_entries(&mut function.body, &addresses, input.definitions, input.imports,
                                &mut input.types, &mut input.tasks, &input.symbol);
 input.error = result.err();
 if input.error.is_none() { input.jobs[index] = NativeFunction::Owned(function); }
 u32::from(input.error.is_some())
}

fn bind_task_entries(
 body: &mut ValueCode, addresses: &BTreeMap<usize, String>, functions: &[FunctionBody],
 imports: &BTreeMap<String, Vec<u8>>, types: &mut TaskTypes, tasks: &mut Vec<TaskBinding>, symbol: &str,
) -> Result<(), &'static str> {
 let mut rewrite = TaskRewrite {
  registers: &mut body.registers, addresses, functions, imports, types, tasks, symbol,
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
 original: Option<IntoIter<Inst>>,
 count: usize,
}

impl RewriteBlock<'_> {
 fn instruction(&mut self, index: usize) -> &mut Inst {
  match &mut self.original {
   Some(original) => {
    let consumed = self.count - original.len();
    &mut original.as_mut_slice()[index - consumed]
   }
   None => &mut self.code[index],
  }
 }

 fn take_start(&mut self, index: usize) -> Inst {
  let original = self.original.get_or_insert_with(|| {
   std::mem::replace(self.code, Vec::with_capacity(self.count + 2)).into_iter()
  });
  let consumed = self.count - original.len();
  self.code.extend(original.by_ref().take(index - consumed));
  original.next().unwrap()
 }
}

impl TaskRewrite<'_> {
 fn block(&mut self, code: &mut Vec<Inst>) -> u32 {
  let count = code.len();
  let mut block = RewriteBlock { code, original: None, count };
  let host = w::RewriteHost { inspect: inspect_instruction, child: rewrite_child, start: rewrite_task_start };
  let status = unsafe { w::rewrite_block(ptr::from_mut(&mut block).cast(), count, ptr::from_mut(self).cast(), host) };
  if let Some(original) = block.original {
   block.code.extend(original);
  }
  status
 }
}

unsafe extern "C" fn inspect_instruction(block: *mut c_void, index: usize) -> w::Instruction {
 let block = unsafe { &mut *block.cast::<RewriteBlock<'_>>() };
 let inst = block.instruction(index);
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
 // The B traversal keeps original absolute indices while the final block grows.
 let mut instruction = block.take_start(index);
 let InstKind::Call(call) = &mut instruction.kind else { unsafe { std::hint::unreachable_unchecked() } };
 let result = lower_start(call, rewrite.registers, rewrite.addresses, rewrite.functions, rewrite.imports,
                          rewrite.types, rewrite.tasks, rewrite.symbol, block.code);
 block.code.push(instruction);
 match result {
  Ok(()) => { rewrite.result = Ok(true); 0 }
  Err(error) => { rewrite.result = Err(error); 1 }
 }
}

fn lower_start(
 call: &mut Call, registers: &mut Vec<Scalar>, addresses: &BTreeMap<usize, String>,
 functions: &[FunctionBody], imports: &BTreeMap<String, Vec<u8>>, types: &mut TaskTypes,
 tasks: &mut Vec<TaskBinding>, symbol: &str, output: &mut Vec<Inst>,
) -> Result<(), &'static str> {
 let CallArgument::Reference { ty: input, .. } = &call.arguments[1] else {
  return Err("task start needs an actual input packet");
 };
 let reg = task_register(call, 2).ok_or("task start needs a bound worker")?;
 let worker = addresses.get(&reg).ok_or("missing actual task function address")?;
 let selector = task_register(call, 4).and_then(|reg| addresses.get(&reg)).map(String::as_str);
 let start = call.result.as_ref().ok_or("missing task start result type")?;
 let request = TaskKey { worker, input, start, selector };
 let count = tasks.len();
 let mut binding = BindingRequest { request, functions, imports, types, tasks, error: None };
 let host = w::StartHost { matches: binding_matches, create: create_binding };
 let bound = unsafe { w::bind_start(ptr::from_mut(&mut binding).cast(), count, host) };
 if let Some(error) = binding.error { return Err(error); }
 rewrite_start(call, registers, &tasks[bound.binding], &bound, symbol, output);
 Ok(())
}

#[derive(Clone, Copy)]
struct TaskKey<'a> {
 worker: &'a str,
 input: &'a Ty,
 start: &'a Ty,
 selector: Option<&'a str>,
}

struct BindingRequest<'a> {
 request: TaskKey<'a>,
 functions: &'a [FunctionBody],
 imports: &'a BTreeMap<String, Vec<u8>>,
 types: &'a mut TaskTypes,
 tasks: &'a mut Vec<TaskBinding>,
 error: Option<&'static str>,
}

unsafe extern "C" fn binding_matches(context: *mut c_void, index: usize) -> u32 {
 let state = unsafe { &*context.cast::<BindingRequest<'_>>() };
 let task = &state.tasks[index];
 let key = state.request;
 let start = task.index - state.types.original_count + 2;
 u32::from(task.worker == key.worker && task.input == *key.input
  && state.types.added[start] == *key.start && task.selector.as_deref() == key.selector)
}

unsafe extern "C" fn create_binding(context: *mut c_void) {
 let state = unsafe { &mut *context.cast::<BindingRequest<'_>>() };
 state.error = new_binding(state.request, state.functions, state.imports, state.types, state.tasks).err();
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
struct PositionBinding<'a> {
 types: &'a mut Vec<Ty>,
 layout: usize,
 parameters: &'a [CallParameter],
}

unsafe extern "C" fn position_parameter(context: *mut c_void, index: usize) -> w::BindingParameter {
 let state = unsafe { &*context.cast::<PositionBinding<'_>>() };
 match &state.parameters[index].kind {
  CallParameterKind::Value(ty) if ty.name == "__blink_frame" => w::BindingParameter::Frame,
  CallParameterKind::Value(_) => w::BindingParameter::Value,
  CallParameterKind::Reference { access: Access::InOut, .. } => w::BindingParameter::InOut,
  CallParameterKind::Reference { .. } => w::BindingParameter::Reference,
 }
}

unsafe extern "C" fn position_owned(context: *mut c_void, field: usize) -> u32 {
 let state = unsafe { &*context.cast::<PositionBinding<'_>>() };
 u32::from(state.types[state.layout].fields[field].0.starts_with(Ty::TASK_OWNED_REFERENCE_PREFIX))
}

unsafe extern "C" fn publish_position(
 context: *mut c_void, field: usize, parameter_index: usize, type_index: usize, kind: w::BindingParameter,
) {
 let state = unsafe { &mut *context.cast::<PositionBinding<'_>>() };
 let field = &mut state.types[state.layout].fields[field].0;
 let ready = field[Ty::TASK_OWNED_REFERENCE_PREFIX.len()..].split_once(':').map(|(_, offset)| offset);
 let mode = if matches!(kind, w::BindingParameter::InOut) { "inout" } else { "write" };
 let mut name = format!("{}{mode}:{type_index}", Ty::TASK_OWNED_REFERENCE_PREFIX);
 if let Some(ready) = ready {
  name.push(':');
  name.push_str(ready);
 }
 *field = name;
 state.types.push(parameter(&state.parameters[parameter_index]).0.clone());
}

fn new_binding(
 key: TaskKey<'_>, functions: &[FunctionBody], imports: &BTreeMap<String, Vec<u8>>,
 types: &mut TaskTypes, tasks: &mut Vec<TaskBinding>,
) -> Result<(), &'static str> {
 let (parameters, result, fallible) = signature(key.worker, functions, imports)?;
 let completion = match result.as_deref() {
  Some(result) => result.task_wait_layout(),
  None => Ty::unit().task_wait_layout(),
 }.ok_or("task result layout is too large")?;
 let output = key.input.arguments.last().unwrap();
 let name = format!("{}@task{}", key.worker, tasks.len());
 let entry = task_entry(name, key.worker, &parameters, result.as_deref(), fallible, key.input, output, key.selector)?;

 let layout = types.added.len();
 let index = types.original_count + layout;
 types.added.extend([key.input.clone(), completion, key.start.clone()]);
 let first = types.original_count + types.added.len();
 let mut positions = PositionBinding { types: &mut types.added, layout, parameters: &parameters };
 let host = w::PositionHost { parameter: position_parameter, owned: position_owned, publish: publish_position };
 unsafe {
  w::bind_positions(ptr::from_mut(&mut positions).cast(), key.input.fields.len(), parameters.len(), first, host);
 }
 tasks.push(TaskBinding {
  worker: key.worker.into(), input: key.input.clone(), index, entry, selector: key.selector.map(str::to_owned),
 });
 Ok(())
}

fn argument(scalar: Scalar, reg: usize) -> CallArgument {
 CallArgument::Value { ty: Ty::scalar(scalar), values: vec![reg], dataset_borrow: None, origin: None }
}

fn rewrite_start(
 call: &mut Call, registers: &mut Vec<Scalar>, binding: &TaskBinding, bound: &w::BoundStart,
 symbol: &str, output: &mut Vec<Inst>,
) {
 let entry = registers.len();
 registers.extend([Scalar::U64, Scalar::U32]);
 output.push(Inst::new(InstKind::FunctionAddress(entry, CallTarget::Defined(binding.entry.name.clone()))));
 output.push(Inst::new(InstKind::Constant(entry + 1, Scalar::U32, binding.index as u64)));
 call.arguments[bound.worker_argument] = argument(Scalar::U64, entry);
 call.arguments.truncate(bound.retained_arguments);
 call.arguments.push(argument(Scalar::U32, entry + 1));
 let CallTarget::External(target) = &mut call.target else { unsafe { std::hint::unreachable_unchecked() } };
 target.symbol = symbol.into();
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
