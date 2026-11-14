//! B selects deletion and propagation; Rust moves the original owning IR.
use crate::ir::*;
use std::collections::BTreeSet;
type Slots = BTreeSet<PrivateSlot>;
#[repr(C)]
#[derive(Clone, Copy)]
struct Block { instructions: *mut Inst, count: usize }
impl Block {
    fn from(code: &mut [Inst]) -> Self { Self { instructions: code.as_mut_ptr(), count: code.len() } }
    const EMPTY: Self = Self { instructions: std::ptr::null_mut(), count: 0 };
    unsafe fn slice<'a>(self) -> &'a [Inst] { unsafe { std::slice::from_raw_parts(self.instructions, self.count) } }
}
#[repr(u32)]
enum Kind { Ordinary, Scope, If, Loop, LocalWrite, IndexedWrite, Division, Collective }
#[repr(u32)]
enum RemovalKind { Values, Division, DirectWrite, IndexedWrite }
#[repr(u32)]
enum DivisionChoice { Quotient = 1, Remainder, Both }
#[repr(u32)]
enum Space { Register, Local }
#[repr(u32)]
enum Action { Copy, Union, InsertRegister, RemoveRegister, InsertLocal, RemoveLocal, Swap }
#[repr(C)]
struct Node {
    instruction: *mut Inst, first: Block, second: Block,
    first_code: *mut Vec<Inst>, second_code: *mut Vec<Inst>, payload: *mut std::ffi::c_void,
    first_slot: usize, second_slot: usize, index: Reg, word: usize, value: Reg,
    quotient: Reg, remainder: Reg, kind: Kind, flags: u32,
}
#[repr(C)]
struct Results { present: u32, needed: u32 }
#[repr(C)]
struct Removal { kind: RemovalKind, value: Reg, index: Reg, length: usize, word: usize, division: *const IntegerDivisionResult }
type Contains = unsafe extern "C" fn(*const Slots, Space, usize) -> u32;
#[repr(C)]
struct Host {
    read_node: unsafe extern "C" fn(Block, usize) -> Node,
    read_block: unsafe extern "C" fn(*mut Vec<Inst>) -> Block,
    results: unsafe extern "C" fn(*const Inst, *const Slots) -> Results,
    contains: Contains,
    change: unsafe extern "C" fn(Action, *mut Slots, *const Slots, usize),
    clone_set: unsafe extern "C" fn(*const Slots) -> *mut Slots,
    drop_set: unsafe extern "C" fn(*mut Slots),
    eliminate_block: unsafe extern "C" fn(*mut Vec<Inst>, *mut Slots, *const Slots, *const Slots, *const Slots, *mut Context<'_>),
    live_before: unsafe extern "C" fn(Block, *mut Slots, *const Slots, *const Slots, *const Slots),
    loop_live: unsafe extern "C" fn(Block, Reg, Block, *const Slots, *const Slots, *const Slots, *const Slots) -> *mut Slots,
    record: unsafe extern "C" fn(*mut Context<'_>, *const Inst, Removal),
    prune_division: unsafe extern "C" fn(*mut Inst, *mut IntegerDivisionResult, DivisionChoice, Reg, Reg),
    prune_collective: unsafe extern "C" fn(*mut TileCollective, *const Slots),
}
struct Context<'a> { registers: &'a [Scalar], removed: &'a mut Vec<EliminatedOperation>, empty: *const Slots }
unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::eliminate_instruction"]
    fn eliminate_instruction(instruction: *mut Inst, needed: *mut Slots, break_live: *const Slots,
        continue_live: *const Slots, exit_live: *const Slots, empty: *const Slots, context: *mut Context<'_>, host: Host) -> u32;
    #[link_name = "bootstrap.stage_fusion::dce_result"]
    fn visit_result(results: *mut Results, needed: *const Slots, reg: Reg, contains: Contains) -> u32;
    #[link_name = "bootstrap.stage_fusion::dce_result_needed"]
    fn result_needed(needed: *const Slots, reg: Reg, contains: Contains) -> u32;
}

unsafe extern "C" fn read_node(block: Block, index: usize) -> Node {
    let instruction = unsafe { block.instructions.add(index) };
    let inst = unsafe { &mut *instruction };
    let exempt = matches!(&inst.kind, InstKind::Atomic(_) | InstKind::Append(_) | InstKind::Print { .. } | InstKind::Call(_));
    let observable = matches!(&inst.kind, InstKind::Loop { .. } | InstKind::StoreIndexed { .. } | InstKind::Store { .. }
        | InstKind::Atomic(_) | InstKind::Append(_) | InstKind::Print { .. } | InstKind::Call(_)
        | InstKind::TilePublish(_) | InstKind::TileCollective(_) | InstKind::TileBarrier);
    let mut node = Node { instruction, first: Block::EMPTY, second: Block::EMPTY,
        first_code: std::ptr::null_mut(), second_code: std::ptr::null_mut(), payload: std::ptr::null_mut(),
        first_slot: 0, second_slot: 0, index: 0, word: 0, value: 0, quotient: 0, remainder: 0,
        kind: Kind::Ordinary, flags: u32::from(exempt) | (u32::from(observable) << 1) };
    match &mut inst.kind {
        InstKind::Scope(body) => {
            node.kind = Kind::Scope; node.first = Block::from(body); node.first_code = body;
        }
        InstKind::If { condition, yes, no } => {
            node.kind = Kind::If; node.first_slot = *condition;
            node.first = Block::from(yes); node.first_code = yes;
            node.second = Block::from(no); node.second_code = no;
        }
        InstKind::Loop { condition, test, body } => {
            node.kind = Kind::Loop; node.first_slot = *test;
            node.first = Block::from(condition); node.first_code = condition;
            node.second = Block::from(body); node.second_code = body;
        }
        InstKind::StoreLocal(slot, value) => {
            node.kind = Kind::LocalWrite; node.first_slot = *slot; node.value = *value;
        }
        InstKind::StoreLocalIndexed { base, len, index, word, value } => {
            node.kind = Kind::IndexedWrite; node.first_slot = *base; node.second_slot = *len;
            node.index = *index; node.word = *word as usize; node.value = *value;
        }
        InstKind::IntegerDivide { result, .. } => {
            node.kind = Kind::Division;
            if let Some(reg) = result.quotient() { node.quotient = reg; node.flags |= 4; }
            if let Some(reg) = result.remainder() { node.remainder = reg; node.flags |= 8; }
            node.payload = (result as *mut IntegerDivisionResult).cast();
        }
        InstKind::TileCollective(collective) => {
            node.kind = Kind::Collective; node.payload = (collective as *mut TileCollective).cast();
        }
        _ => {}
    }
    node
}
unsafe extern "C" fn read_block(code: *mut Vec<Inst>) -> Block { Block::from(unsafe { &mut *code }) }
unsafe extern "C" fn results(inst: *const Inst, needed: *const Slots) -> Results {
    let mut output = Results { present: 0, needed: 0 };
    for reg in unsafe { &*inst }.results() {
        if unsafe { visit_result(&mut output, needed, reg, contains) } == 0 { break; }
    }
    output
}
unsafe extern "C" fn contains(slots: *const Slots, space: Space, slot: usize) -> u32 {
    let slot = match space { Space::Register => PrivateSlot::Register(slot), Space::Local => PrivateSlot::Local(slot) };
    u32::from(unsafe { &*slots }.contains(&slot))
}
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
unsafe extern "C" fn clone_set(source: *const Slots) -> *mut Slots { Box::into_raw(Box::new(unsafe { &*source }.clone())) }
unsafe extern "C" fn drop_set(source: *mut Slots) { drop(unsafe { Box::from_raw(source) }); }
unsafe extern "C" fn before(code: Block, needed: *mut Slots, brk: *const Slots, cont: *const Slots, exit: *const Slots) {
    let live = unsafe { std::mem::take(&mut *needed) };
    let live = live_before(unsafe { code.slice() }, live, unsafe { &*brk }, unsafe { &*cont }, unsafe { &*exit });
    unsafe { *needed = live };
}
unsafe extern "C" fn header(condition: Block, test: Reg, body: Block, after: *const Slots,
    brk: *const Slots, cont: *const Slots, exit: *const Slots) -> *mut Slots {
    let live = loop_live(unsafe { condition.slice() }, test, unsafe { body.slice() }, unsafe { &*after },
        unsafe { &*brk }, unsafe { &*cont }, unsafe { &*exit });
    Box::into_raw(Box::new(live))
}
unsafe extern "C" fn record(context: *mut Context<'_>, instruction: *const Inst, removal: Removal) {
    let context = unsafe { &mut *context }; let inst = unsafe { &*instruction };
    let origin = OperationOrigin::from_instruction(inst);
    let operation = match removal.kind {
        RemovalKind::Values | RemovalKind::Division => EliminatedOperation::ValueResults {
            result_types: inst.results().map(|reg| context.registers[reg]).collect(),
            integer_division_results: match removal.kind {
                RemovalKind::Division => Some(unsafe { *removal.division }.outputs().map(|(role, _)| role).collect()),
                _ => None,
            },
            origin,
        },
        RemovalKind::DirectWrite => EliminatedOperation::DirectPrivateWrite { scalar: context.registers[removal.value], origin },
        RemovalKind::IndexedWrite => EliminatedOperation::IndexedPrivateWrite { component_count: removal.length,
            index_type: context.registers[removal.index], byte_offset: removal.word as u32,
            scalar: context.registers[removal.value], origin },
    };
    context.removed.push(operation);
}
unsafe extern "C" fn prune_division(instruction: *mut Inst, result: *mut IntegerDivisionResult,
    choice: DivisionChoice, quotient: Reg, remainder: Reg) {
    let original = unsafe { *result };
    let retained = match choice {
        DivisionChoice::Quotient => IntegerDivisionResult::Quotient(quotient),
        DivisionChoice::Remainder => IntegerDivisionResult::Remainder(remainder),
        DivisionChoice::Both => IntegerDivisionResult::Both { quotient, remainder },
    };
    unsafe { *result = retained };
    let inst = unsafe { &mut *instruction }; let origin = OperationOrigin::from_instruction(inst);
    inst.generated_origin = Some(std::sync::Arc::new(GeneratedOrigin {
        reason: GenerationReason::IntegerDivisionResultPrune,
        trigger: Some(GenerationAnchor::IntegerDivisionResultPrune { retained: origin,
            input_results: original.outputs().map(|(result, _)| result).collect(),
            output_results: retained.outputs().map(|(result, _)| result).collect() }),
    }));
}
unsafe extern "C" fn prune_collective(collective: *mut TileCollective, needed: *const Slots) {
    unsafe { &mut *collective }.results.retain(|(_, reg)| unsafe { result_needed(needed, *reg, contains) } != 0);
}
const HOST: Host = Host { read_node, read_block, results, contains, change, clone_set, drop_set,
    eliminate_block: block, live_before: before, loop_live: header, record, prune_division, prune_collective };
unsafe extern "C" fn block(code: *mut Vec<Inst>, needed: *mut Slots, brk: *const Slots,
    cont: *const Slots, exit: *const Slots, context: *mut Context<'_>) {
    let code = unsafe { &mut *code }; let mut output = Vec::new();
    for mut inst in std::mem::take(code).into_iter().rev() {
        let keep = unsafe { eliminate_instruction(&mut inst, needed, brk, cont, exit, (*context).empty, context, HOST) };
        if keep != 0 { output.push(inst); }
    }
    output.reverse(); *code = output;
}
pub(super) fn run(code: &mut Vec<Inst>, needed: &mut Slots, brk: &Slots, cont: &Slots, exit: &Slots,
    registers: &[Scalar]) -> Vec<EliminatedOperation> {
    let empty = Slots::new(); let mut removed = Vec::new();
    let mut context = Context { registers, removed: &mut removed, empty: &empty };
    unsafe { block(code, needed, brk, cont, exit, &mut context) };
    removed
}
