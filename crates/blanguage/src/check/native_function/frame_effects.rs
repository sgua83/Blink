//! Physical borrows for B-owned native Frame need inference.
use super::{Unit, fallibility};
use fallibility::{Span, Seed, Function, Node, SeedHost, Host};
use crate::ir::{CallArgument, Inst, InstKind, Reg, dataflow};
use std::collections::{BTreeMap, BTreeSet, btree_map};
use std::ffi::c_void;

#[repr(C)]
struct Meta { present: u32, enabled: u32, has_body: u32, name: *const c_void }

#[repr(C)]
struct ImportHost {
    iterator: *mut c_void,
    output: *mut c_void,
    next: unsafe extern "C-unwind" fn(*mut c_void) -> Meta,
    remember: unsafe extern "C-unwind" fn(*mut c_void, *const c_void),
}

#[repr(C)]
struct SourceFunction { present: u32, flag: *mut bool, code: Span, roots: Span, outputs: Span }

#[repr(C)]
struct SourceHost {
    iterator: *mut c_void,
    data: dataflow::Host,
    imported: *const c_void,
    next: unsafe extern "C-unwind" fn(*mut c_void) -> SourceFunction,
    inspect: unsafe extern "C-unwind" fn(*const Inst) -> Node,
    arguments: unsafe extern "C-unwind" fn(*const Inst) -> Span,
    contains: unsafe extern "C-unwind" fn(*const c_void, *const c_void) -> u32,
}

unsafe extern "C-unwind" {
    #[link_name = "bootstrap.ir_dataflow::seed_frame_imports"]
    fn seed_imports(host: ImportHost);
    #[link_name = "bootstrap.ir_dataflow::mark_frame_sources"]
    fn mark_sources(host: SourceHost);
}

unsafe extern "C-unwind" fn next_meta(context: *mut c_void) -> Meta {
    let iterator=unsafe { &mut *context.cast::<btree_map::Iter<'_,String,Unit>>() };
    match iterator.next() {
        Some((name,unit)) => Meta { present: 1, enabled: u32::from(unit.contract.frame), has_body: u32::from(unit.function.is_some()), name: (name as *const String).cast() },
        None => Meta { present: 0, enabled: 0, has_body: 0, name: std::ptr::null() },
    }
}

unsafe extern "C-unwind" fn next_source(context: *mut c_void) -> SourceFunction {
    let iterator=unsafe { &mut *context.cast::<btree_map::ValuesMut<'_,String,Unit>>() };
    match iterator.next() {
        Some(Unit { contract, function, .. }) => {
            let (code,roots,outputs)=function.as_ref().map_or(
                (&[] as &[Inst],&[] as &[Reg],&[] as &[Reg]),
                |function|(function.body.code.as_slice(),&function.body.inputs[..2],function.body.outputs.as_slice()));
            SourceFunction { present: 1, flag: &mut contract.frame, code: code.into(), roots: roots.into(), outputs: outputs.into() }
        }
        None => SourceFunction { present: 0, flag: std::ptr::null_mut(), code: (&[] as &[Inst]).into(), roots: (&[] as &[Reg]).into(), outputs: (&[] as &[Reg]).into() },
    }
}

unsafe extern "C-unwind" fn arguments(instruction: *const Inst) -> Span {
    match unsafe { &(*instruction).kind } {
        InstKind::Call(call) => call.arguments.as_slice().into(),
        _ => (&[] as &[CallArgument]).into(),
    }
}

unsafe extern "C-unwind" fn next_seed(context: *mut c_void) -> Seed {
    let iterator=unsafe { &mut *context.cast::<btree_map::Iter<'_,String,Unit>>() };
    match iterator.next() {
        Some((name,unit)) => Seed { present: 1, enabled: u32::from(unit.contract.frame), name: (name as *const String).cast() },
        None => Seed { present: 0, enabled: 0, name: std::ptr::null() },
    }
}

unsafe extern "C-unwind" fn next_function(context: *mut c_void) -> Function {
    let iterator=unsafe { &mut *context.cast::<btree_map::ValuesMut<'_,String,Unit>>() };
    match iterator.next() {
        Some(Unit { contract, function, .. }) => {
            let code=function.as_ref().map_or(&[] as &[Inst],|function|function.body.code.as_slice());
            Function { present: 1, flag: &mut contract.frame, body: code.into() }
        }
        None => Function { present: 0, flag: std::ptr::null_mut(), body: (&[] as &[Inst]).into() },
    }
}

struct Round<'a> { units: &'a mut BTreeMap<String,Unit>, known: BTreeSet<String> }

unsafe extern "C-unwind" fn round(context: *mut c_void) -> u32 {
    let context=unsafe { &mut *context.cast::<Round<'_>>() };
    context.known.clear();
    {
        let mut iterator=context.units.iter();
        unsafe { fallibility::seed_effect_flags(SeedHost {
            iterator: (&mut iterator as *mut btree_map::Iter<'_,String,Unit>).cast(),
            known: (&mut context.known as *mut BTreeSet<String>).cast(),
            next: next_seed, remember: fallibility::remember,
        }); }
    }
    let mut iterator=context.units.values_mut();
    unsafe { fallibility::mark_effects(Host {
        iterator: (&mut iterator as *mut btree_map::ValuesMut<'_,String,Unit>).cast(),
        known: (&context.known as *const BTreeSet<String>).cast(),
        next: next_function, inspect: fallibility::inspect, contains: fallibility::contains,
    },0) }
}

pub(super) fn run(units: &mut BTreeMap<String,Unit>) -> BTreeSet<String> {
    {
        let mut imported=BTreeSet::<String>::new();
        {
            let mut iterator=units.iter();
            unsafe { seed_imports(ImportHost {
                iterator: (&mut iterator as *mut btree_map::Iter<'_,String,Unit>).cast(),
                output: (&mut imported as *mut BTreeSet<String>).cast(),
                next: next_meta, remember: fallibility::remember,
            }); }
        }
        let mut iterator=units.values_mut();
        unsafe { mark_sources(SourceHost {
            iterator: (&mut iterator as *mut btree_map::ValuesMut<'_,String,Unit>).cast(),
            data: dataflow::HOST, imported: (&imported as *const BTreeSet<String>).cast(),
            next: next_source, inspect: fallibility::inspect, arguments, contains: fallibility::contains,
        }); }
    }
    // Each round ends its source-name iteration before borrowing Unit fields
    // exclusively. The unchanged round's read-only snapshot is the existing
    // final ABI material; no additional collection or authority is needed.
    let mut context=Round { units, known: BTreeSet::new() };
    unsafe { fallibility::propagate((&mut context as *mut Round<'_>).cast(),round); }
    context.known
}
