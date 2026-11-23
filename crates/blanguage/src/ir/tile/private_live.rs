//! B propagates liveness; the original IR and sets keep their native ownership.
use super::{BTreeSet, Inst, PrivateSlot, Reg, TilePhase};
use std::{ffi::c_void, mem, ptr};
use crate::ir::dataflow::{self, Span};
type Slots = BTreeSet<PrivateSlot>;

#[repr(u32)]
enum Action { Copy, Union, RemoveRegister, InsertLocal, RemoveLocal, Swap }
type Change = unsafe extern "C" fn(Action, *mut Slots, *const Slots, usize);
#[repr(C)]
pub(crate) struct Host {
    ir: dataflow::Host,
    insert_register: unsafe extern "C" fn(*mut Slots, Reg) -> u32,
    clone_set: unsafe extern "C" fn(*const Slots) -> *mut Slots,
    drop_set: unsafe extern "C" fn(*mut Slots),
    change: Change,
}
unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::private_before"]
    fn native_before(code: Span, live: *mut Slots, break_live: *const Slots,
        continue_live: *const Slots, exit_live: *const Slots, empty: *const Slots, host: Host);
    #[link_name = "bootstrap.stage_fusion::private_loop"]
    fn native_loop(condition: Span, test: Reg, body: Span, after: *const Slots,
        break_live: *const Slots, continue_live: *const Slots, exit_live: *const Slots,
        result: *mut Slots, empty: *const Slots, host: Host);
    #[link_name = "bootstrap.stage_fusion::private_tile_phases"]
    fn native_tile_phases(code: Span, live: *mut Slots, saved: *mut Slots, empty: *const Slots,
        host: Host, context: *mut c_void, emit: PhaseOutput);
}

unsafe extern "C" fn insert_register(live: *mut Slots, reg: Reg) -> u32 {
    unsafe { &mut *live }.insert(PrivateSlot::Register(reg));
    1
}

unsafe extern "C" fn clone_set(source: *const Slots) -> *mut Slots {
    Box::into_raw(Box::new(unsafe { &*source }.clone()))
}
unsafe extern "C" fn drop_set(source: *mut Slots) { drop(unsafe { Box::from_raw(source) }); }
unsafe extern "C" fn change(action: Action, output: *mut Slots, input: *const Slots, slot: usize) {
    match action {
        Action::Copy => unsafe { (*output).clone_from(&*input) },
        Action::Union => unsafe { (*output).extend((*input).iter().copied()) },
        Action::RemoveRegister => { unsafe { (*output).remove(&PrivateSlot::Register(slot)) }; }
        Action::InsertLocal => { unsafe { (*output).insert(PrivateSlot::Local(slot)) }; }
        Action::RemoveLocal => { unsafe { (*output).remove(&PrivateSlot::Local(slot)) }; }
        Action::Swap => unsafe { std::ptr::swap(output, input.cast_mut()) },
    }
}
pub(crate) const HOST: Host = Host { ir: dataflow::HOST, insert_register, clone_set, drop_set, change };

type PhaseOutput = unsafe extern "C" fn(*mut c_void, usize, usize, *const Slots, *mut Slots);

struct Phases<'a> {
    code: &'a [Inst],
    phases: Vec<TilePhase<'a>>,
}

unsafe extern "C" fn append_phase(
    context: *mut c_void, begin: usize, end: usize, live: *const Slots, saved: *mut Slots,
) {
    let output = unsafe { &mut *context.cast::<Phases<'_>>() };
    output.phases.push(TilePhase {
        code: &output.code[begin..end],
        step: output.code.get(end),
        live_in: unsafe { &*live }.clone(),
        saved: mem::take(unsafe { &mut *saved }),
    });
}

pub(super) fn tile_phases(code: &[Inst]) -> Vec<TilePhase<'_>> {
    let mut output = Phases { code, phases: Vec::new() };
    let empty = Slots::new();
    let mut live = Slots::new();
    let mut saved = Slots::new();
    unsafe {
        native_tile_phases(Span::slice(code), &mut live, &mut saved, &empty, HOST,
            ptr::from_mut(&mut output).cast(), append_phase);
    }
    output.phases.reverse();
    output.phases
}

pub(super) fn before(code: &[Inst], mut live: Slots, break_live: &Slots,
    continue_live: &Slots, exit_live: &Slots) -> Slots {
    let empty = Slots::new();
    unsafe { native_before(Span::slice(code), &mut live, break_live, continue_live, exit_live, &empty, HOST) };
    live
}
pub(super) fn loop_header(condition: &[Inst], test: Reg, body: &[Inst], after: &Slots,
    break_live: &Slots, continue_live: &Slots, exit_live: &Slots) -> Slots {
    let mut result = Slots::new(); let empty = Slots::new();
    unsafe { native_loop(Span::slice(condition), test, Span::slice(body), after,
        break_live, continue_live, exit_live, &mut result, &empty, HOST) };
    result
}
