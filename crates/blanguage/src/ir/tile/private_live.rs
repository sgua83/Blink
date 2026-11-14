//! B propagates liveness; the original IR and sets keep their native ownership.
use super::{BTreeSet, Inst, InstKind, PrivateSlot, Reg};
type Slots = BTreeSet<PrivateSlot>;

#[repr(C)]
#[derive(Clone, Copy)]
struct Block { instructions: *const Inst, count: usize }
impl Block {
    fn from(code: &[Inst]) -> Self { Self { instructions: code.as_ptr(), count: code.len() } }
    const EMPTY: Self = Self { instructions: std::ptr::null(), count: 0 };
}
#[repr(u32)]
enum Kind { Ordinary, If, Loop, Scope, Exit, Break, Continue, LocalRead, LocalWrite }
#[repr(C)]
struct Node {
    first: Block, second: Block, instruction: *const Inst,
    first_slot: usize, second_slot: usize, kind: Kind,
}
#[repr(u32)]
enum Action { Copy, Union, InsertRegister, RemoveRegister, InsertLocal, RemoveLocal, Swap }
#[repr(u32)]
enum ReferenceKind { Result, Operand, LocalRead }
type Change = unsafe extern "C" fn(Action, *mut Slots, *const Slots, usize);
#[repr(C)]
struct Host {
    read_node: unsafe extern "C" fn(Block, usize) -> Node,
    references: unsafe extern "C" fn(*const Inst, *mut Slots),
    clone_set: unsafe extern "C" fn(*const Slots) -> *mut Slots,
    drop_set: unsafe extern "C" fn(*mut Slots),
    change: Change,
}
unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::private_before"]
    fn native_before(code: Block, live: *mut Slots, break_live: *const Slots,
        continue_live: *const Slots, exit_live: *const Slots, empty: *const Slots, host: Host);
    #[link_name = "bootstrap.stage_fusion::private_loop"]
    fn native_loop(condition: Block, test: Reg, body: Block, after: *const Slots,
        break_live: *const Slots, continue_live: *const Slots, exit_live: *const Slots,
        result: *mut Slots, empty: *const Slots, host: Host);
    #[link_name = "bootstrap.stage_fusion::private_reference"]
    fn reference(kind: ReferenceKind, live: *mut Slots, slot: usize, change: Change);
}

unsafe extern "C" fn read_node(block: Block, index: usize) -> Node {
    let inst = unsafe { &*block.instructions.add(index) };
    let mut node = Node { first: Block::EMPTY, second: Block::EMPTY, instruction: inst,
        first_slot: 0, second_slot: 0, kind: Kind::Ordinary };
    match &inst.kind {
        InstKind::If { condition, yes, no } => {
            node.kind = Kind::If; node.first = Block::from(yes); node.second = Block::from(no); node.first_slot = *condition;
        }
        InstKind::Loop { condition, test, body } => {
            node.kind = Kind::Loop; node.first = Block::from(condition); node.second = Block::from(body); node.first_slot = *test;
        }
        InstKind::Scope(body) => { node.kind = Kind::Scope; node.first = Block::from(body); }
        InstKind::Exit => node.kind = Kind::Exit,
        InstKind::Break => node.kind = Kind::Break,
        InstKind::Continue => node.kind = Kind::Continue,
        InstKind::LoadLocal(dst, slot) => { node.kind = Kind::LocalRead; node.first_slot = *dst; node.second_slot = *slot; }
        InstKind::StoreLocal(slot, src) => { node.kind = Kind::LocalWrite; node.first_slot = *slot; node.second_slot = *src; }
        _ => {}
    }
    node
}
unsafe extern "C" fn references(inst: *const Inst, live: *mut Slots) {
    let inst = unsafe { &*inst };
    for reg in inst.results() { unsafe { reference(ReferenceKind::Result, live, reg, change) }; }
    inst.all_operands(&mut |reg| {
        unsafe { reference(ReferenceKind::Operand, live, reg, change) }; true
    });
    for slot in inst.local_reads() { unsafe { reference(ReferenceKind::LocalRead, live, slot, change) }; }
}
unsafe extern "C" fn clone_set(source: *const Slots) -> *mut Slots {
    Box::into_raw(Box::new(unsafe { &*source }.clone()))
}
unsafe extern "C" fn drop_set(source: *mut Slots) { drop(unsafe { Box::from_raw(source) }); }
unsafe extern "C" fn change(action: Action, output: *mut Slots, input: *const Slots, slot: usize) {
    match action {
        Action::Copy => unsafe { (*output).clone_from(&*input) },
        Action::Union => unsafe { (*output).extend((*input).iter().copied()) },
        Action::InsertRegister => { unsafe { (*output).insert(PrivateSlot::Register(slot)) }; }
        Action::RemoveRegister => { unsafe { (*output).remove(&PrivateSlot::Register(slot)) }; }
        Action::InsertLocal => { unsafe { (*output).insert(PrivateSlot::Local(slot)) }; }
        Action::RemoveLocal => { unsafe { (*output).remove(&PrivateSlot::Local(slot)) }; }
        Action::Swap => unsafe { std::ptr::swap(output, input.cast_mut()) },
    }
}
const HOST: Host = Host { read_node, references, clone_set, drop_set, change };

pub(super) fn before(code: &[Inst], mut live: Slots, break_live: &Slots,
    continue_live: &Slots, exit_live: &Slots) -> Slots {
    let empty = Slots::new();
    unsafe { native_before(Block::from(code), &mut live, break_live, continue_live, exit_live, &empty, HOST) };
    live
}
pub(super) fn loop_header(condition: &[Inst], test: Reg, body: &[Inst], after: &Slots,
    break_live: &Slots, continue_live: &Slots, exit_live: &Slots) -> Slots {
    let mut result = Slots::new(); let empty = Slots::new();
    unsafe { native_loop(Block::from(condition), test, Block::from(body), after,
        break_live, continue_live, exit_live, &mut result, &empty, HOST) };
    result
}
