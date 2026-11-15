//! Physical borrows for B-owned native dependency discovery and reachability.
use super::Unit;
use crate::ir::{CallTarget, Inst, InstKind, Operation, Program, Stage};
use std::collections::{BTreeMap, BTreeSet, btree_set};
use std::ffi::c_void;

#[derive(Clone, Copy)]
#[repr(C)]
struct Span {
    data: *const c_void,
    count: usize,
    stride: usize,
}

fn span<T>(values: &[T]) -> Span {
    Span { data: values.as_ptr().cast(), count: values.len(), stride: std::mem::size_of::<T>() }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Text { data: *const u8, length: usize }

impl From<&str> for Text {
    fn from(value: &str) -> Self { Self { data: value.as_ptr(), length: value.len() } }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Owner { present: u32, name: Text }

#[repr(C)]
struct Node { first: Span, second: Span, target: *const c_void }

#[repr(C)]
struct StageView { body: Span, project: Span, identity: Span, combine: Span }

#[repr(C)]
struct UnitView { body: Span, name: Text }

#[repr(C)]
struct Work { present: u32, body: Span }

#[repr(C)]
struct DirectHost {
    output: *mut c_void,
    inspect: unsafe extern "C-unwind" fn(*const Inst) -> Node,
    add: unsafe extern "C-unwind" fn(*mut c_void, *const c_void),
}

#[repr(C)]
struct SeedHost {
    iterator: *mut c_void,
    units: *const c_void,
    pending: *mut c_void,
    owner: Owner,
    next: unsafe extern "C-unwind" fn(*mut c_void) -> *const c_void,
    lookup: unsafe extern "C-unwind" fn(*const c_void, *const c_void) -> UnitView,
    push: unsafe extern "C-unwind" fn(*mut c_void, Span),
}

#[repr(C)]
struct ClosureHost {
    units: *const c_void,
    required: *mut c_void,
    pending: *mut c_void,
    owner: Owner,
    inspect: unsafe extern "C-unwind" fn(*const Inst) -> Node,
    lookup: unsafe extern "C-unwind" fn(*const c_void, *const c_void) -> UnitView,
    insert: unsafe extern "C-unwind" fn(*mut c_void, *const c_void) -> u32,
    push: unsafe extern "C-unwind" fn(*mut c_void, Span),
    pop: unsafe extern "C-unwind" fn(*mut c_void) -> Work,
}

unsafe extern "C-unwind" {
    #[link_name = "bootstrap.ir_dataflow::collect_dependencies"]
    fn collect(code: Span, host: DirectHost);
    #[link_name = "bootstrap.ir_dataflow::collect_program_dependencies"]
    fn collect_program(stages: Span, inspect: unsafe extern "C-unwind" fn(*const Stage) -> StageView, host: DirectHost);
    #[link_name = "bootstrap.ir_dataflow::seed_dependencies"]
    fn seed(host: SeedHost);
    #[link_name = "bootstrap.ir_dataflow::expand_dependencies"]
    fn expand(host: ClosureHost);
}

unsafe extern "C-unwind" fn inspect(instruction: *const Inst) -> Node {
    let empty=span(&[] as &[Inst]);
    let mut node=Node { first: empty, second: empty, target: std::ptr::null() };
    match unsafe { &(*instruction).kind } {
        InstKind::Call(call) => {
            if let CallTarget::Defined(name)=&call.target { node.target=(name as *const String).cast(); }
        }
        InstKind::FunctionAddress(_, CallTarget::Defined(name)) => node.target=(name as *const String).cast(),
        InstKind::Scope(body) | InstKind::Cleanup { body, .. } => node.first=span(body),
        InstKind::If { yes, no, .. } => { node.first=span(yes); node.second=span(no); }
        InstKind::Loop { condition, body, .. } => { node.first=span(condition); node.second=span(body); }
        _ => {}
    }
    node
}

unsafe extern "C-unwind" fn inspect_stage(stage: *const Stage) -> StageView {
    let stage=unsafe { &*stage };
    let empty=span(&[] as &[Inst]);
    let mut view=StageView { body: span(&stage.code), project: empty, identity: empty, combine: empty };
    let reduction=match &stage.operation {
        Operation::CustomReduce(value) => Some(value),
        Operation::CustomScan(value) => Some(&value.values),
        Operation::CustomSegmentedReduce(value) => Some(&value.values),
        _ => None,
    };
    if let Some(reduction)=reduction {
        if let Some(project)=&reduction.project { view.project=span(&project.code); }
        view.identity=span(&reduction.identity.code);
        view.combine=span(&reduction.combine.code);
    }
    view
}

unsafe extern "C-unwind" fn insert(output: *mut c_void, name: *const c_void) -> u32 {
    let output=unsafe { &mut *output.cast::<BTreeSet<String>>() };
    let name=unsafe { &*name.cast::<String>() };
    if output.contains(name) { 0 } else { output.insert(name.clone()); 1 }
}

unsafe extern "C-unwind" fn add(output: *mut c_void, name: *const c_void) {
    unsafe { insert(output,name); }
}

unsafe extern "C-unwind" fn next(iterator: *mut c_void) -> *const c_void {
    let iterator=unsafe { &mut *iterator.cast::<btree_set::Iter<'_,String>>() };
    iterator.next().map_or(std::ptr::null(),|name|(name as *const String).cast())
}

unsafe extern "C-unwind" fn lookup(units: *const c_void, name: *const c_void) -> UnitView {
    let units=unsafe { &*units.cast::<BTreeMap<String,Unit>>() };
    let unit=&units[unsafe { &*name.cast::<String>() }];
    let body=unit.function.as_ref().map_or(&[] as &[Inst],|function|function.body.code.as_slice());
    UnitView { body: span(body), name: unit.module.as_str().into() }
}

unsafe extern "C-unwind" fn push(pending: *mut c_void, body: Span) {
    unsafe { &mut *pending.cast::<Vec<Span>>() }.push(body);
}

unsafe extern "C-unwind" fn pop(pending: *mut c_void) -> Work {
    match unsafe { &mut *pending.cast::<Vec<Span>>() }.pop() {
        Some(body) => Work { present: 1, body },
        None => Work { present: 0, body: span(&[] as &[Inst]) },
    }
}

fn direct_host(output: &mut BTreeSet<String>) -> DirectHost {
    DirectHost { output: (output as *mut BTreeSet<String>).cast(), inspect, add }
}

pub(super) fn code(code: &[Inst]) -> BTreeSet<String> {
    let mut output=BTreeSet::new();
    unsafe { collect(span(code),direct_host(&mut output)); }
    output
}

pub(super) fn program(program: &Program) -> BTreeSet<String> {
    let mut output=BTreeSet::new();
    unsafe { collect_program(span(&program.stages),inspect_stage,direct_host(&mut output)); }
    output
}

pub(super) fn complete(units: &BTreeMap<String,Unit>, required: &mut BTreeSet<String>, owner: Option<&str>) {
    let units=(units as *const BTreeMap<String,Unit>).cast();
    let owner=Owner { present: u32::from(owner.is_some()), name: owner.unwrap_or("").into() };
    let mut work=Vec::<Span>::new();
    let pending=(&mut work as *mut Vec<Span>).cast();
    {
        let mut iterator=required.iter();
        unsafe { seed(SeedHost { iterator: (&mut iterator as *mut btree_set::Iter<'_,String>).cast(), units, pending, owner, next, lookup, push }); }
    }
    // The required-name iterator has ended. The original units and their IR
    // remain read-only until B has consumed the final pending body.
    unsafe { expand(ClosureHost { units, required: (required as *mut BTreeSet<String>).cast(), pending, owner, inspect, lookup, insert, push, pop }); }
}
