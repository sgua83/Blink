//! B propagates liveness and snapshots ordered slots into the final phase plan.
use super::{BTreeSet, Inst, PrivateSlot, Reg, TilePhase};
use std::{ffi::c_void, mem, ptr};
use crate::ir::dataflow::{self, Span};
type Slots = BTreeSet<PrivateSlot>;

#[repr(u32)]
enum Action { Copy, Union, RemoveRegister, InsertLocal, RemoveLocal, Swap, DrainUnion, Clear }
type Change = unsafe extern "C" fn(Action, *mut Slots, *const Slots, usize);
#[repr(C)]
pub(crate) struct Host {
    ir: dataflow::Host,
    pub(crate) insert_register: unsafe extern "C" fn(*mut Slots, Reg) -> u32,
    clone_set: unsafe extern "C" fn(*const Slots) -> *mut Slots,
    drop_set: unsafe extern "C" fn(*mut Slots),
    change: Change,
}
unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::private_tile_phases"]
    fn native_tile_phases(code: Span, live: *mut Slots, empty: *const Slots, host: Host,
        context: *mut c_void, emit: PhaseOutput, save: PhaseSave);
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
        Action::Swap => unsafe { ptr::swap(output, input.cast_mut()) },
        Action::DrainUnion => unsafe { (*output).append(&mut *input.cast_mut()) },
        Action::Clear => unsafe { (*output).clear() },
    }
}
pub(crate) const HOST: Host = Host { ir: dataflow::HOST, insert_register, clone_set, drop_set, change };

type PhaseOutput = unsafe extern "C" fn(*mut c_void, usize, usize, *const Slots);
type PhaseSave = unsafe extern "C" fn(*mut c_void, *const Slots);

struct Phases<'a> {
    code: &'a [Inst],
    phases: Vec<TilePhase<'a>>,
    saved: Vec<PrivateSlot>,
}

unsafe extern "C" fn append_phase(
    context: *mut c_void, begin: usize, end: usize, live: *const Slots,
) {
    let output = unsafe { &mut *context.cast::<Phases<'_>>() };
    output.phases.push(TilePhase {
        code: &output.code[begin..end],
        step: output.code.get(end),
        live_in: unsafe { &*live }.iter().copied().collect(),
        saved: mem::take(&mut output.saved),
    });
}

unsafe extern "C" fn save_phase_slots(context: *mut c_void, live: *const Slots) {
    let output = unsafe { &mut *context.cast::<Phases<'_>>() };
    output.saved.extend(unsafe { &*live }.iter().copied());
}

pub(super) fn tile_phases(code: &[Inst]) -> Vec<TilePhase<'_>> {
    let mut output = Phases { code, phases: Vec::new(), saved: Vec::new() };
    let empty = Slots::new();
    let mut live = Slots::new();
    unsafe {
        native_tile_phases(Span::slice(code), &mut live, &empty, HOST,
            ptr::from_mut(&mut output).cast(), append_phase, save_phase_slots);
    }
    output.phases.reverse();
    output.phases
}
