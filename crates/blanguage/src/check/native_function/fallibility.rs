//! Physical borrows for shared B effect operations and native failure effects.
use super::{Module, Unit};
use crate::ir::{CallTarget, Inst, InstKind};
use std::collections::{BTreeSet, btree_map};
use std::ffi::c_void;

#[derive(Clone, Copy)]
#[repr(C)]
pub(super) struct Span {
    pub(super) data: *const c_void,
    pub(super) count: usize,
    pub(super) stride: usize,
}

impl<T> From<&[T]> for Span {
    fn from(code: &[T]) -> Self {
        Self { data: code.as_ptr().cast(), count: code.len(), stride: std::mem::size_of::<T>() }
    }
}

#[repr(C)]
pub(super) struct Seed {
    pub(super) present: u32,
    pub(super) enabled: u32,
    pub(super) name: *const c_void,
}

#[repr(C)]
pub(super) struct Function {
    pub(super) present: u32,
    pub(super) flag: *mut bool,
    pub(super) body: Span,
}

#[repr(C)]
pub(super) struct Node {
    pub(super) first: Span,
    pub(super) second: Span,
    pub(super) target: *const c_void,
    pub(super) error: u32,
}

#[repr(C)]
pub(super) struct SeedHost {
    pub(super) iterator: *mut c_void,
    pub(super) known: *mut c_void,
    pub(super) next: unsafe extern "C-unwind" fn(*mut c_void) -> Seed,
    pub(super) remember: unsafe extern "C-unwind" fn(*mut c_void, *const c_void),
}

#[repr(C)]
pub(super) struct Host {
    pub(super) iterator: *mut c_void,
    pub(super) known: *const c_void,
    pub(super) next: unsafe extern "C-unwind" fn(*mut c_void) -> Function,
    pub(super) inspect: unsafe extern "C-unwind" fn(*const Inst) -> Node,
    pub(super) contains: unsafe extern "C-unwind" fn(*const c_void, *const c_void) -> u32,
}

unsafe extern "C-unwind" {
    #[link_name = "bootstrap.ir_dataflow::seed_effect_flags"]
    pub(super) fn seed_effect_flags(host: SeedHost);
    #[link_name = "bootstrap.ir_dataflow::mark_effects"]
    pub(super) fn mark_effects(host: Host, explicit_errors: u32) -> u32;
    #[link_name = "bootstrap.ir_dataflow::propagate_effects"]
    pub(super) fn propagate(context: *mut c_void, round: unsafe extern "C-unwind" fn(*mut c_void) -> u32);
}

unsafe extern "C-unwind" fn next_seed(context: *mut c_void) -> Seed {
    let iterator = unsafe { &mut *context.cast::<btree_map::Iter<'_, String, Option<Unit>>>() };
    match iterator.next() {
        Some((name, unit)) => Seed {
            present: 1,
            enabled: u32::from(unit.as_ref().unwrap().contract.fallible),
            name: (name as *const String).cast(),
        },
        None => Seed { present: 0, enabled: 0, name: std::ptr::null() },
    }
}

pub(super) unsafe extern "C-unwind" fn remember(context: *mut c_void, name: *const c_void) {
    let known = unsafe { &mut *context.cast::<BTreeSet<String>>() };
    known.insert(unsafe { &*name.cast::<String>() }.clone());
}

unsafe extern "C-unwind" fn next_function(context: *mut c_void) -> Function {
    let iterator = unsafe { &mut *context.cast::<btree_map::ValuesMut<'_, String, Option<Unit>>>() };
    match iterator.next() {
        Some(unit) => {
            let Unit { contract, function, .. } = unit.as_mut().unwrap();
            let code = function.as_ref().map_or(&[] as &[Inst], |function| function.body.code.as_slice());
            Function { present: 1, flag: &mut contract.fallible, body: code.into() }
        }
        None => Function { present: 0, flag: std::ptr::null_mut(), body: (&[] as &[Inst]).into() },
    }
}

pub(super) unsafe extern "C-unwind" fn inspect(instruction: *const Inst) -> Node {
    let empty = (&[] as &[Inst]).into();
    let mut node = Node { first: empty, second: empty, target: std::ptr::null(), error: 0 };
    match unsafe { &(*instruction).kind } {
        InstKind::Call(call) => {
            node.error = u32::from(call.error.is_some());
            if let CallTarget::Defined(name) = &call.target { node.target = (name as *const String).cast(); }
        }
        InstKind::Scope(body) | InstKind::Cleanup { body, .. } => node.first = body.as_slice().into(),
        InstKind::If { yes, no, .. } => {
            node.first = yes.as_slice().into();
            node.second = no.as_slice().into();
        }
        InstKind::Loop { condition, body, .. } => {
            node.first = condition.as_slice().into();
            node.second = body.as_slice().into();
        }
        _ => {}
    }
    node
}

pub(super) unsafe extern "C-unwind" fn contains(context: *const c_void, name: *const c_void) -> u32 {
    let known = unsafe { &*context.cast::<BTreeSet<String>>() };
    u32::from(known.contains(unsafe { &*name.cast::<String>() }))
}

unsafe extern "C-unwind" fn round(context: *mut c_void) -> u32 {
    let module = unsafe { &mut *context.cast::<Module>() };
    let mut known = BTreeSet::new();
    {
        let mut iterator = module.units.iter();
        unsafe {
            seed_effect_flags(SeedHost {
                iterator: (&mut iterator as *mut btree_map::Iter<'_, String, Option<Unit>>).cast(),
                known: (&mut known as *mut BTreeSet<String>).cast(),
                next: next_seed,
                remember,
            });
        }
    }
    // The source-name borrow has ended. This iterator owns exclusive, disjoint
    // Unit borrows; B consumes each body's final read before writing its flag.
    let mut iterator = module.units.values_mut();
    unsafe {
        mark_effects(Host {
            iterator: (&mut iterator as *mut btree_map::ValuesMut<'_, String, Option<Unit>>).cast(),
            known: (&known as *const BTreeSet<String>).cast(),
            next: next_function,
            inspect,
            contains,
        },1)
    }
}

pub(super) fn run(module: &mut Module) {
    unsafe { propagate((module as *mut Module).cast(), round); }
}
