//! B selects deletion and propagation; Rust moves the original owning IR.
use crate::ir::*;
use super::{BlockSpan, InstructionStorage, instruction_span, instruction_storage, swap_instructions};
use std::collections::BTreeSet;
type Slots = BTreeSet<PrivateSlot>;
#[repr(u32)]
enum RemovalKind { Values, Division, DirectWrite, IndexedWrite }
#[repr(u32)]
enum DivisionChoice { Quotient = 1, Remainder, Both }
#[repr(u32)]
enum Space { Register, Local }
#[repr(C)]
struct Removal { kind: RemovalKind, value: Reg, index: Reg, length: usize, word: usize }
type Contains = unsafe extern "C" fn(*const Slots, Space, usize) -> u32;
#[repr(C)]
struct Host {
    live: PrivateLiveHost,
    storage: unsafe extern "C" fn(*mut Inst) -> InstructionStorage,
    read_block: unsafe extern "C" fn(*mut Vec<Inst>) -> BlockSpan,
    contains: Contains,
    swap_instructions: unsafe extern "C" fn(*mut Inst, *mut Inst),
    discard_prefix: unsafe extern "C" fn(*mut Vec<Inst>, usize),
    record: unsafe extern "C" fn(*mut Context<'_>, *const Inst, Removal),
    prune_division: unsafe extern "C" fn(*mut Inst, *mut IntegerDivisionResult, DivisionChoice, Reg, Reg),
    prune_collective: unsafe extern "C" fn(*mut TileCollective, *const Slots),
}
struct Context<'a> { registers: &'a [Scalar], removed: &'a mut Vec<EliminatedOperation> }
unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::eliminate_code"]
    fn eliminate_code(code: *mut Vec<Inst>, needed: *mut Slots, break_live: *const Slots,
        continue_live: *const Slots, exit_live: *const Slots, empty: *const Slots, context: *mut Context<'_>, host: Host);
    #[link_name = "bootstrap.stage_fusion::dce_result_needed"]
    fn result_needed(needed: *const Slots, reg: Reg, contains: Contains) -> u32;
}

unsafe extern "C" fn contains(slots: *const Slots, space: Space, slot: usize) -> u32 {
    let slot = match space { Space::Register => PrivateSlot::Register(slot), Space::Local => PrivateSlot::Local(slot) };
    u32::from(unsafe { &*slots }.contains(&slot))
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
const HOST: Host = Host { live: PRIVATE_LIVE_HOST, storage: instruction_storage, read_block: instruction_span, contains,
    swap_instructions, discard_prefix, record, prune_division, prune_collective };
unsafe extern "C" fn discard_prefix(code: *mut Vec<Inst>, count: usize) {
    drop(unsafe { &mut *code }.drain(..count));
}
pub(super) fn run(code: &mut Vec<Inst>, needed: &mut Slots, brk: &Slots, cont: &Slots, exit: &Slots,
    registers: &[Scalar]) -> Vec<EliminatedOperation> {
    let empty = Slots::new(); let mut removed = Vec::new();
    let mut context = Context { registers, removed: &mut removed };
    unsafe { eliminate_code(code, needed, brk, cont, exit, &empty, &mut context, HOST) };
    removed
}
