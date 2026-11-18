//! Physical owners and constructors for B's native failure IR lowering.
use super::{FailureContext, FailureContinuation, FailureScope, Module, World, Result, fallibility};
use crate::check::application;
use crate::ir::{BinaryOp, CallArgument, CallTarget, Comparison, GeneratedOrigin, Inst, InstKind, Reg, Scalar, SourceSite, Ty, ValueCode};
use fallibility::{Span, Node as EffectNode};
use std::{collections::{BTreeMap, BTreeSet}, ffi::c_void, ptr, sync::Arc};

#[derive(Clone, Copy)]
#[repr(C)]
struct Word { present: u32, value: usize }
#[repr(C)]
struct Text { data: *const u8, length: usize }
impl Text {
    fn of(value: &str) -> Self { Self { data: value.as_ptr(), length: value.len() } }
}
#[repr(C)]
struct EffectRead {
    known: *const c_void,
    inspect: unsafe extern "C-unwind" fn(*const Inst) -> EffectNode,
    contains: unsafe extern "C-unwind" fn(*const c_void, *const c_void) -> u32,
}
#[repr(C)]
struct LowerNode {
    present: u32, kind: u32, source: *mut Inst,
    first: *mut Vec<Inst>, second: *mut Vec<Inst>, body: Span, slot: usize,
    error: Word, error_slot: *mut Option<Reg>, target: *const c_void,
    continuation: u32, payload: *mut c_void,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct Branch { kind: u32, error: Reg, slot: usize, existing: u32, previous: Reg, joined: Reg }
#[repr(C)]
struct LowerHost {
    effects: EffectRead,
    next: unsafe extern "C-unwind" fn(*mut c_void) -> LowerNode,
    reg: unsafe extern "C-unwind" fn(*mut c_void, u32) -> Reg,
    capture: unsafe extern "C-unwind" fn(*mut c_void, *const Inst, *mut c_void, u32),
    set_error: unsafe extern "C-unwind" fn(*mut Option<Reg>, Reg),
    original: unsafe extern "C-unwind" fn(*mut c_void, *mut Vec<Inst>),
    cleanup_body: unsafe extern "C-unwind" fn(*mut c_void, *mut Vec<Inst>, *mut Vec<Inst>),
    cleanup_scope: unsafe extern "C-unwind" fn(*mut c_void, *mut Vec<Inst>, *mut Vec<Inst>),
    fold: unsafe extern "C-unwind" fn(*mut Vec<Inst>, usize),
    constant: unsafe extern "C-unwind" fn(*mut c_void, *mut Vec<Inst>, Reg, u64),
    load: unsafe extern "C-unwind" fn(*mut c_void, *mut Vec<Inst>, Reg, usize),
    store: unsafe extern "C-unwind" fn(*mut c_void, *mut Vec<Inst>, usize, Reg),
    compare: unsafe extern "C-unwind" fn(*mut c_void, *mut Vec<Inst>, Reg, Reg, Reg),
    raise: unsafe extern "C-unwind" fn(*mut c_void, *mut Vec<Inst>, Reg),
    join: unsafe extern "C-unwind" fn(*mut c_void, *mut Vec<Inst>, Reg, Reg, Reg),
    finish_if: unsafe extern "C-unwind" fn(*mut c_void, *mut Vec<Inst>, Reg, *mut Vec<Inst>, *mut Vec<Inst>),
}
#[repr(C)]
struct FoldNode {
    structure: EffectNode, kind: u32, reg: Reg, slot: usize,
    library: Text, symbol: Text, value_argument: u32,
    arguments: Span, outputs: Span,
}
#[repr(C)]
struct FoldHost {
    reads: *mut BTreeSet<Reg>, runtime: Text,
    inspect: unsafe extern "C-unwind" fn(*const Inst) -> FoldNode,
    insert: unsafe extern "C-unwind" fn(*mut BTreeSet<Reg>, Reg),
    contains: unsafe extern "C-unwind" fn(*const BTreeSet<Reg>, Reg) -> u32,
    replace: unsafe extern "C-unwind" fn(*mut Inst, Reg, u32),
}
unsafe extern "C-unwind" {
    #[link_name = "bootstrap.ir_dataflow::lower_failure_start"]
    fn b_start(state: *mut LowerCursor, block: *mut c_void, output: *mut Vec<Inst>, host: LowerHost) -> LowerNeed;
    #[link_name = "bootstrap.ir_dataflow::lower_failure_child"]
    fn b_child(state: *mut LowerCursor, block: *mut c_void, output: *mut Vec<Inst>, nested: u32, host: LowerHost) -> LowerNeed;
    #[link_name = "bootstrap.ir_dataflow::lower_failure_branch"]
    fn b_branch(state: *mut LowerCursor, block: *mut c_void, output: *mut Vec<Inst>, host: LowerHost) -> LowerNeed;
    #[link_name = "bootstrap.ir_dataflow::failure_branch_prepare"]
    fn b_prepare(block: *mut c_void, yes: *mut Vec<Inst>, branch: Branch, host: LowerHost) -> FailureBranchNeed;
    #[link_name = "bootstrap.ir_dataflow::failure_branch_finish"]
    fn b_finish(block: *mut c_void, output: *mut Vec<Inst>, yes: *mut Vec<Inst>, no: *mut Vec<Inst>, test: Word, branch: Branch, host: LowerHost);
    #[link_name = "bootstrap.ir_dataflow::fold_empty_status"]
    fn b_fold(code: Span, slot: usize, host: FoldHost);
}

// The original pass borrows the actual IR containers and Module context.
// Only current Inst storage is fixed for B descriptors across child calls.
struct State<'w,'b,'c,'k,'m> {
    world: &'w World,
    registers: &'b mut Vec<Scalar>, locals: &'b mut BTreeMap<usize,Ty>,
    context: Option<FailureContext<'c>>, known: &'k BTreeSet<String>,
    module: Option<&'m mut Module>,
}
struct Origin { site: Option<Arc<SourceSite>>, generated: Option<Arc<GeneratedOrigin>> }
struct Block<'s,'w,'b,'c,'k,'m> {
    state: &'s mut State<'w,'b,'c,'k,'m>, input: std::vec::IntoIter<Inst>,
    current: Option<Inst>, origin: Option<Origin>, scope: Option<Box<FailureScope>>,
}
#[repr(C)]
struct LowerCursor { node: LowerNode, flags: u32, phase: u32 }
#[repr(C)]
struct LowerNeed { kind: u32, flags: u32, child: *mut Vec<Inst>, test: Word, branch: Branch }
#[repr(C)]
struct FailureBranchNeed { kind: u32, test: Word, branch: Branch }
fn host(known: &BTreeSet<String>) -> LowerHost {
    LowerHost {
        effects: EffectRead { known: ptr::from_ref(known).cast(), inspect: fallibility::inspect, contains: fallibility::contains },
        next, reg, capture, set_error, original, cleanup_body, cleanup_scope,
        fold, constant, load, store, compare, raise, join, finish_if,
    }
}
async fn block(code: &mut Vec<Inst>, state: &mut State<'_,'_,'_,'_,'_>) -> Result<u32> {
    let world=state.world;
    let mut owner=Box::new(Block { state, input: std::mem::take(code).into_iter(), current: None, origin: None, scope: None });
    let context=ptr::from_mut(owner.as_mut()).cast();
    let mut output=Vec::new();
    let mut cursor=std::mem::MaybeUninit::<LowerCursor>::uninit();
    let mut need=unsafe { b_start(cursor.as_mut_ptr(),context,&mut output,host(owner.state.known)) };
    while need.kind!=0 {
        need=if need.kind==1 {
            let nested=unsafe { world.call(block(&mut *need.child,&mut *owner.state)).await }?;
            unsafe { b_child(cursor.as_mut_ptr(),context,&mut output,nested,host(owner.state.known)) }
        } else {
            unsafe { world.call(branches(owner.as_mut(),&mut output,need.test,need.branch)).await }?;
            unsafe { b_branch(cursor.as_mut_ptr(),context,&mut output,host(owner.state.known)) }
        };
    }
    *code=output;
    Ok(need.flags)
}
async fn branches(owner: &mut Block<'_,'_,'_,'_,'_,'_>, output: &mut Vec<Inst>, test: Word, branch: Branch) -> Result<()> {
    let world=owner.state.world;
    let context=ptr::from_mut(&mut *owner).cast();
    let mut yes=Vec::new();let mut no=Vec::new();
    let need=unsafe { b_prepare(context,&mut yes,branch,host(owner.state.known)) };
    if need.kind==1 {
        let scope=owner.scope.take().unwrap();
        let state=&mut *owner.state;
        yes=unsafe { world.call(super::failure_scope(world,*scope,need.branch.error,state.registers,state.locals,
            state.context.as_mut().unwrap(),state.module.as_deref_mut().unwrap(),
            (need.branch.existing!=0).then_some(need.branch.slot))).await }?;
    } else if need.kind==2 {
        unsafe { world.call(branches(owner,&mut yes,need.test,need.branch)).await }?;
    }
    unsafe { b_finish(context,output,&mut yes,&mut no,test,branch,host(owner.state.known)) };
    Ok(())
}
pub(super) async fn run(world: &World, body: &mut ValueCode, context: Option<FailureContext<'_>>, known: &BTreeSet<String>, module: Option<&mut Module>) -> Result<(bool,bool)> {
    let mut state=State { world, registers: &mut body.registers, locals: &mut body.locals, context, known, module };
    let result=unsafe { world.call(block(&mut body.code,&mut state)).await }?;
    Ok((result&1!=0,result&2!=0))
}
unsafe extern "C-unwind" fn next(context: *mut c_void) -> LowerNode {
    let owner=unsafe { &mut *context.cast::<Block<'_,'_,'_,'_,'_,'_>>() };
    owner.origin=None; owner.scope=None; owner.current=owner.input.next();
    let mut node=LowerNode {
        present: 0, kind: 0, source: ptr::null_mut(), first: ptr::null_mut(), second: ptr::null_mut(),
        body: (&[] as &[Inst]).into(), slot: 0, error: Word { present: 0, value: 0 },
        error_slot: ptr::null_mut(), target: ptr::null(), continuation: 0, payload: ptr::null_mut(),
    };
    if let Some(instruction)=owner.current.as_mut() {
        node.present=1; node.source=ptr::from_mut(instruction);
        match &mut instruction.kind {
            InstKind::Cleanup { slot, body, failure } => {
                node.kind=1; node.slot=*slot; node.body=body.as_slice().into(); node.first=ptr::from_mut(body);
                node.continuation=if failure.is_some() {2} else {0}; node.payload=ptr::from_mut(failure).cast();
            }
            InstKind::Call(call) => {
                node.kind=2; node.error=Word { present: u32::from(call.error.is_some()), value: call.error.unwrap_or(0) };
                node.error_slot=ptr::from_mut(&mut call.error); node.payload=ptr::from_mut(&mut call.failure).cast();
                if let CallTarget::Defined(name)=&call.target { node.target=ptr::from_ref(name).cast(); }
                node.continuation=match &call.failure {
                    None => 0, Some(FailureContinuation::Return) => 1,
                    Some(FailureContinuation::Scope(_)) => 2,
                    Some(FailureContinuation::Cleanup(slot)) => { node.slot=*slot; 3 }
                };
            }
            InstKind::Scope(body) => { node.kind=3; node.first=ptr::from_mut(body); }
            InstKind::If { yes, no, .. } => { node.kind=4; node.first=ptr::from_mut(yes); node.second=ptr::from_mut(no); }
            InstKind::Loop { condition, body, .. } => { node.kind=5; node.first=ptr::from_mut(condition); node.second=ptr::from_mut(body); }
            _ => {}
        }
    }
    node
}
unsafe extern "C-unwind" fn reg(context: *mut c_void, boolean: u32) -> Reg {
    let state=unsafe { &mut *(*context.cast::<Block<'_,'_,'_,'_,'_,'_>>()).state };
    let result=state.registers.len();state.registers.push(if boolean!=0 {Scalar::Bool} else {Scalar::U64});result
}
unsafe extern "C-unwind" fn capture(context: *mut c_void, source: *const Inst, payload: *mut c_void, cleanup: u32) {
    let source=unsafe { &*source };
    let origin=Origin { site: source.source_site.clone(), generated: source.generated_origin.clone() };
    let scope=if cleanup!=0 {
        unsafe { (&mut *payload.cast::<Option<Box<FailureScope>>>()).take() }
    } else {
        match unsafe { (&mut *payload.cast::<Option<FailureContinuation>>()).take() } {
            Some(FailureContinuation::Scope(scope)) => Some(scope), _ => None,
        }
    };
    unsafe { (*context.cast::<Block<'_,'_,'_,'_,'_,'_>>()).origin=Some(origin);(*context.cast::<Block<'_,'_,'_,'_,'_,'_>>()).scope=scope; }
}
unsafe extern "C-unwind" fn set_error(slot: *mut Option<Reg>, value: Reg) { unsafe { *slot=Some(value); } }
unsafe extern "C-unwind" fn original(context: *mut c_void, output: *mut Vec<Inst>) {
    let instruction=unsafe { (*context.cast::<Block<'_,'_,'_,'_,'_,'_>>()).current.take().unwrap() };
    unsafe { (&mut *output).push(instruction); }
}
fn generated(context: *mut c_void, kind: InstKind) -> Inst {
    let origin=unsafe { (&*context.cast::<Block<'_,'_,'_,'_,'_,'_>>()).origin.as_ref().unwrap() };
    Inst { source_site: origin.site.clone(), generated_origin: origin.generated.clone(), ..Inst::new(kind) }
}
unsafe extern "C-unwind" fn cleanup_body(context: *mut c_void, output: *mut Vec<Inst>, body: *mut Vec<Inst>) {
    let code=unsafe { std::mem::take(&mut *body) };
    let instruction=generated(context,InstKind::Scope(code));unsafe { (&mut *output).push(instruction); }
}
unsafe extern "C-unwind" fn cleanup_scope(context: *mut c_void, output: *mut Vec<Inst>, body: *mut Vec<Inst>) {
    let code=unsafe { std::mem::take(&mut *body) };
    let mut instruction=unsafe { (*context.cast::<Block<'_,'_,'_,'_,'_,'_>>()).current.take().unwrap() };
    instruction.kind=InstKind::Scope(code);unsafe { (&mut *output).push(instruction); }
}
unsafe extern "C-unwind" fn constant(context: *mut c_void, output: *mut Vec<Inst>, dst: Reg, value: u64) {
    let instruction=generated(context,InstKind::Constant(dst,Scalar::U64,value));unsafe { (&mut *output).push(instruction); }
}
unsafe extern "C-unwind" fn load(context: *mut c_void, output: *mut Vec<Inst>, dst: Reg, slot: usize) {
    let instruction=generated(context,InstKind::LoadLocal(dst,slot));unsafe { (&mut *output).push(instruction); }
}
unsafe extern "C-unwind" fn store(context: *mut c_void, output: *mut Vec<Inst>, slot: usize, value: Reg) {
    let instruction=generated(context,InstKind::StoreLocal(slot,value));unsafe { (&mut *output).push(instruction); }
}
unsafe extern "C-unwind" fn compare(context: *mut c_void, output: *mut Vec<Inst>, dst: Reg, a: Reg, b: Reg) {
    let instruction=generated(context,InstKind::Binary { dst, op: BinaryOp::Compare(Comparison::NotEqual), a, b });unsafe { (&mut *output).push(instruction); }
}
unsafe extern "C-unwind" fn raise(context: *mut c_void, output: *mut Vec<Inst>, error: Reg) {
    let instruction=generated(context,InstKind::Raise(error));unsafe { (&mut *output).push(instruction); }
}
unsafe extern "C-unwind" fn join(context: *mut c_void, output: *mut Vec<Inst>, dst: Reg, previous: Reg, error: Reg) {
    let ty=Ty::scalar(Scalar::U64);
    let argument=|value|CallArgument::Value { origin: None, dataset_borrow: None, ty: ty.clone(), values: vec![value] };
    let call=application::runtime_call("error_join",vec![argument(previous),argument(error)],Some(ty),vec![dst],None,None);
    let instruction=generated(context,call.kind);unsafe { (&mut *output).push(instruction); }
}
unsafe extern "C-unwind" fn finish_if(context: *mut c_void, output: *mut Vec<Inst>, condition: Reg, yes: *mut Vec<Inst>, no: *mut Vec<Inst>) {
    let yes=unsafe { std::mem::take(&mut *yes) };let no=unsafe { std::mem::take(&mut *no) };
    let instruction=generated(context,InstKind::If { condition, yes, no });unsafe { (&mut *output).push(instruction); }
}
unsafe extern "C-unwind" fn fold(code: *mut Vec<Inst>, slot: usize) {
    let span=unsafe { (&*code).as_slice() }.into();let mut reads=BTreeSet::new();
    unsafe { b_fold(span,slot,FoldHost { reads: &mut reads, runtime: Text::of(crate::application::RUNTIME_LIBRARY), inspect: fold_node, insert: insert_read, contains: contains_read, replace: replace_read }); }
}
unsafe extern "C-unwind" fn fold_node(source: *const Inst) -> FoldNode {
    let instruction=unsafe { &*source };
    let mut node=FoldNode { structure: unsafe { fallibility::inspect(source) }, kind: 0, reg: 0, slot: 0,
        library: Text::of(""), symbol: Text::of(""), value_argument: 0,
        arguments: (&[] as &[Reg]).into(), outputs: (&[] as &[Reg]).into() };
    match &instruction.kind {
        InstKind::LoadLocal(reg,slot) => { node.kind=1;node.reg=*reg;node.slot=*slot; }
        InstKind::Call(call) => {
            node.kind=2;node.outputs=call.values.as_slice().into();
            if let CallTarget::External(name)=&call.target {node.library=Text::of(&name.library);node.symbol=Text::of(&name.symbol);}
            if let Some(CallArgument::Value { values, .. })=call.arguments.first() {node.value_argument=1;node.arguments=values.as_slice().into();}
        }
        _ => {}
    }
    node
}
unsafe extern "C-unwind" fn insert_read(reads: *mut BTreeSet<Reg>, value: Reg) { unsafe { (&mut *reads).insert(value); } }
unsafe extern "C-unwind" fn contains_read(reads: *const BTreeSet<Reg>, value: Reg) -> u32 { u32::from(unsafe { (&*reads).contains(&value) }) }
unsafe extern "C-unwind" fn replace_read(source: *mut Inst, reg: Reg, boolean: u32) {
    unsafe { (*source).kind=InstKind::Constant(reg,if boolean!=0 {Scalar::Bool} else {Scalar::U64},0); }
}
