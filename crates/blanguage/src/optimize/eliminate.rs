//! B selects deletion and propagation; Rust moves the original owning IR.
use crate::ir::*;
use std::collections::BTreeSet;
type Slots = BTreeSet<PrivateSlot>;
#[repr(C)]
#[derive(Clone, Copy)]
struct Block { instructions: *mut Inst, count: usize }
impl Block {
    unsafe fn slice<'a>(self) -> &'a [Inst] { unsafe { std::slice::from_raw_parts(self.instructions, self.count) } }
}
#[repr(u32)]
enum RemovalKind { Values, Division, DirectWrite, IndexedWrite }
#[repr(u32)]
enum DivisionChoice { Quotient = 1, Remainder, Both }
#[repr(u32)]
enum Space { Register, Local }
#[repr(u32)]
enum Action { Copy, Union, InsertRegister, RemoveRegister, InsertLocal, RemoveLocal, Swap }
#[repr(C)]
struct Span { data: *mut std::ffi::c_void, count: usize, stride: usize }
#[repr(C)]
struct Storage { first_code: *mut Vec<Inst>, second_code: *mut Vec<Inst>, payload: *mut std::ffi::c_void }
#[repr(C)]
struct Removal { kind: RemovalKind, value: Reg, index: Reg, length: usize, word: usize }
type Contains = unsafe extern "C" fn(*const Slots, Space, usize) -> u32;
#[repr(C)]
struct Host {
    ir: crate::ir::dataflow::Host,
    storage: unsafe extern "C" fn(*mut Inst) -> Storage,
    read_block: unsafe extern "C" fn(*mut Vec<Inst>) -> Span,
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
    #[link_name = "bootstrap.stage_fusion::dce_result_needed"]
    fn result_needed(needed: *const Slots, reg: Reg, contains: Contains) -> u32;
}

// Only exclusive addresses of original owning fields. B has already consumed
// the immutable scalar/tag projection before requesting these physical owners.
unsafe extern "C" fn storage(instruction: *mut Inst) -> Storage {
    let kind = unsafe { &mut (*instruction).kind };
    let mut fields = Storage { first_code: std::ptr::null_mut(), second_code: std::ptr::null_mut(), payload: std::ptr::null_mut() };
    match kind {
        InstKind::Scope(body) => fields.first_code = body,
        InstKind::If { yes, no, .. } => { fields.first_code = yes; fields.second_code = no; }
        InstKind::Loop { condition, body, .. } => { fields.first_code = condition; fields.second_code = body; }
        InstKind::IntegerDivide { result, .. } => fields.payload = (result as *mut IntegerDivisionResult).cast(),
        InstKind::TileCollective(collective) => fields.payload = (collective as *mut TileCollective).cast(),
        _ => {}
    }
    fields
}
unsafe extern "C" fn read_block(code: *mut Vec<Inst>) -> Span {
    let code = unsafe { &mut *code };
    Span { data: code.as_mut_ptr().cast(), count: code.len(), stride: std::mem::size_of::<Inst>() }
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
        RemovalKind::Values | RemovalKind::Division => {
            let mut result_types = Vec::new();
            let mut integer_division_results = None;
            crate::ir::dataflow::result_origins(inst, |reg, role| {
                result_types.push(context.registers[reg]);
                if let Some(role) = role.division() {
                    integer_division_results.get_or_insert_with(Vec::new).push(role);
                }
            });
            EliminatedOperation::ValueResults { result_types, integer_division_results, origin }
        }
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
const HOST: Host = Host { ir: crate::ir::dataflow::HOST, storage, read_block, contains, change, clone_set, drop_set,
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
