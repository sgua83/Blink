//! B chooses the complete Clause composition. Rust owns the original typed
//! output and necessary intermediate slots; borrowed inputs remain pinned.
use super::{Clause, ClauseNode, Condition, Facts, condition_graph, contradictory};
use std::{ffi::c_void, mem::MaybeUninit, ptr, sync::Arc};
type Slot = *mut c_void;
type Borrowed = *const c_void;
fn borrowed<T>(value: &T) -> Borrowed {
    ptr::from_ref(value).cast()
}
unsafe extern "C" fn inspect(source: Borrowed) -> condition_graph::SourceNode {
    condition_graph::source_node(unsafe { &*source.cast::<Clause>() })
}
unsafe extern "C" fn same(left: Borrowed, right: Borrowed) -> u32 {
    u32::from(unsafe { Arc::ptr_eq(&(*left.cast::<Clause>()).0, &(*right.cast::<Clause>()).0) })
}
unsafe extern "C" fn same_condition(left: Borrowed, right: Borrowed) -> u32 {
    u32::from(unsafe {
        Arc::ptr_eq(
            &*left.cast::<Arc<Condition>>(),
            &*right.cast::<Arc<Condition>>(),
        )
    })
}
unsafe extern "C" fn subset(left: Borrowed, right: Borrowed) -> u32 {
    u32::from(unsafe { (&*left.cast::<Facts>()).is_subset(&*right.cast::<Facts>()) })
}
unsafe extern "C" fn copy(output: Slot, source: Borrowed) {
    unsafe {
        output
            .cast::<Clause>()
            .write((&*source.cast::<Clause>()).clone())
    };
}
unsafe extern "C" fn impossible(output: Slot) {
    unsafe { output.cast::<Clause>().write(Clause::impossible()) };
}
unsafe extern "C" fn make_both(output: Slot, left: Borrowed, right: Borrowed) {
    let value = Clause::node(ClauseNode::Both(
        unsafe { (&*left.cast::<Clause>()).clone() },
        unsafe { (&*right.cast::<Clause>()).clone() },
    ));
    unsafe { output.cast::<Clause>().write(value) };
}
unsafe extern "C" fn make_either(output: Slot, left: Borrowed, right: Borrowed) {
    let value = Clause::node(ClauseNode::Either(
        unsafe { (&*left.cast::<Clause>()).clone() },
        unsafe { (&*right.cast::<Clause>()).clone() },
    ));
    unsafe { output.cast::<Clause>().write(value) };
}
unsafe extern "C" fn conflict(source: Borrowed) -> u32 {
    u32::from(contradictory(unsafe { &*source.cast::<Facts>() }))
}
unsafe extern "C" fn make_facts(output: Slot, source: Slot) {
    let value = Clause::node(ClauseNode::Facts(std::mem::take(unsafe {
        &mut *source.cast::<Facts>()
    })));
    unsafe { output.cast::<Clause>().write(value) };
}
#[repr(C)]
struct LeafHost {
    conflict: unsafe extern "C" fn(Borrowed) -> u32,
    impossible: unsafe extern "C" fn(Slot),
    make: unsafe extern "C" fn(Slot, Slot),
}
fn leaf_host() -> LeafHost {
    LeafHost {
        conflict,
        impossible,
        make: make_facts,
    }
}
unsafe extern "C" fn unite(output: Slot, left: Borrowed, right: Borrowed) {
    let (left, right) = unsafe { (&*left.cast::<Facts>(), &*right.cast::<Facts>()) };
    let mut facts: Facts = left.union(right).copied().collect();
    unsafe { b_facts((&mut facts as *mut Facts).cast(), output, leaf_host()) };
}
#[repr(C)]
struct BothHost {
    inspect: unsafe extern "C" fn(Borrowed) -> condition_graph::SourceNode,
    same: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    unite: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
    impossible: unsafe extern "C" fn(Slot),
    copy: unsafe extern "C" fn(Slot, Borrowed),
    make: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
}
fn both_host() -> BothHost {
    BothHost {
        inspect,
        same,
        unite,
        impossible,
        copy,
        make: make_both,
    }
}
unsafe extern "C" fn factor(output: Slot, shared: Borrowed, left: Borrowed, right: Borrowed) {
    // This is the actual temporary Clause required by B's selected factoring.
    // Its two recursive operations both enter the B implementation; no logical
    // decision or second graph is kept in the native stack frame.
    let nested = either(unsafe { &*left.cast::<Clause>() }, unsafe {
        &*right.cast::<Clause>()
    });
    unsafe { b_both(shared, borrowed(&nested), output, both_host()) };
}
#[repr(C)]
struct EitherHost {
    inspect: unsafe extern "C" fn(Borrowed) -> condition_graph::SourceNode,
    same: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    same_condition: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    subset: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    copy: unsafe extern "C" fn(Slot, Borrowed),
    make: unsafe extern "C" fn(Slot, Borrowed, Borrowed),
    factor: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Borrowed),
}
fn either_host() -> EitherHost {
    EitherHost {
        inspect,
        same,
        same_condition,
        subset,
        copy,
        make: make_either,
        factor,
    }
}
unsafe extern "C" fn case(condition: Borrowed, truth: u32) -> Borrowed {
    borrowed(&unsafe { &*condition.cast::<Arc<Condition>>() }.cases[truth as usize])
}
unsafe extern "C" fn make_branch(output: Slot, parent: Borrowed, condition: Borrowed, truth: u32) {
    let value = Clause::node(ClauseNode::Branch(
        unsafe { (&*parent.cast::<Clause>()).clone() },
        unsafe { (&*condition.cast::<Arc<Condition>>()).clone() },
        truth != 0,
    ));
    unsafe { output.cast::<Clause>().write(value) };
}
#[repr(C)]
struct BranchHost {
    inspect: unsafe extern "C" fn(Borrowed) -> condition_graph::SourceNode,
    case: unsafe extern "C" fn(Borrowed, u32) -> Borrowed,
    impossible: unsafe extern "C" fn(Slot),
    make: unsafe extern "C" fn(Slot, Borrowed, Borrowed, u32),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::clause_facts"]
    fn b_facts(facts: Slot, output: Slot, host: LeafHost);
    #[link_name = "bootstrap.condition_graph::clause_both"]
    fn b_both(left: Borrowed, right: Borrowed, output: Slot, host: BothHost);
    #[link_name = "bootstrap.condition_graph::clause_either"]
    fn b_either(left: Borrowed, right: Borrowed, output: Slot, host: EitherHost);
    #[link_name = "bootstrap.condition_graph::clause_branch"]
    fn b_branch(parent: Borrowed, condition: Borrowed, truth: u32, output: Slot, host: BranchHost);
}
pub(super) fn facts(mut value: Facts) -> Clause {
    let mut output = MaybeUninit::<Clause>::uninit();
    unsafe {
        b_facts(
            (&mut value as *mut Facts).cast(),
            output.as_mut_ptr().cast(),
            leaf_host(),
        );
        output.assume_init()
    }
}
pub(super) fn both(left: &Clause, right: &Clause) -> Clause {
    let mut output = MaybeUninit::<Clause>::uninit();
    unsafe {
        b_both(
            borrowed(left),
            borrowed(right),
            output.as_mut_ptr().cast(),
            both_host(),
        );
        output.assume_init()
    }
}
pub(super) fn either(left: &Clause, right: &Clause) -> Clause {
    let mut output = MaybeUninit::<Clause>::uninit();
    unsafe {
        b_either(
            borrowed(left),
            borrowed(right),
            output.as_mut_ptr().cast(),
            either_host(),
        );
        output.assume_init()
    }
}
pub(super) fn branch(parent: &Clause, condition: &Arc<Condition>, truth: bool) -> Clause {
    let mut output = MaybeUninit::<Clause>::uninit();
    unsafe {
        b_branch(
            borrowed(parent),
            borrowed(condition),
            u32::from(truth),
            output.as_mut_ptr().cast(),
            BranchHost {
                inspect,
                case,
                impossible,
                make: make_branch,
            },
        );
        output.assume_init()
    }
}
