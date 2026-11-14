//! The original owning IR, primitive containers and source construction.
use super::*;
use std::collections::btree_map::Entry;

#[derive(Clone, Default)]
pub(super) struct Values {
    aliases: BTreeMap<Reg, Reg>,
    expressions: BTreeMap<ExpressionKey, Reg>,
    memory: BTreeMap<(usize, Reg, u32), (Reg, Option<OperationOrigin>)>,
    locals: BTreeMap<usize, Reg>,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ExpressionKey { kind: u32, operation: u32, words: [u64; 3], arguments: Vec<Reg> }
#[repr(C)]
#[derive(Clone, Copy)]
struct Expression { kind: u32, operation: u32, words: [u64; 3], arguments: *const Reg, count: usize }
#[repr(C)]
#[derive(Clone, Copy)]
struct Optional { value: usize, present: u32 }
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Division { position: usize, quotient: Reg, remainder: Reg, mask: u32, present: u32 }
#[repr(C)]
struct Memory { value: Reg, origin: *const OperationOrigin, present: u32 }
#[repr(u32)]
enum ReuseKind { Expression, LocalRead, IndexedRead, IndexedStoreRead }
#[repr(u32)]
enum Role { Value, Quotient, Remainder }
#[repr(C)]
struct Reuse { origin: *const OperationOrigin, kind: ReuseKind, role: Role, paired: u32 }
#[repr(u32)]
#[derive(Clone, Copy)]
enum Map { Aliases, Expressions, Memory, Locals }
#[repr(u32)]
#[derive(Clone, Copy)]
enum RegMap { Aliases, Locals }
#[repr(u32)]
enum StateOp { New, Clone, Drop, Move, Copy, Clear, Extend }
#[repr(u32)]
enum DivisionOp { Clear, Take, Put }
#[repr(u32)]
enum DivisionChoice { Quotient = 1, Remainder, Both }
#[repr(u32)]
enum Action { Discard, Keep, Stop }
#[repr(u32)]
enum Kind {
    Ordinary, Constant, Index, Uniform, BufferStride, Unary, Binary, Cast, Math, Select,
    Division, Scope, Raise, Exit, Break, Continue, Atomic, Append, Barrier,
    LocalRead, LocalWrite, LocalIndexedWrite, IndexedWrite, Load, Store, If, Loop, Call, Publish, Collective,
}
#[repr(C)]
struct Node {
    first_code: *mut Vec<Inst>, second_code: *mut Vec<Inst>,
    arguments: *mut Reg, count: usize,
    division_result: *mut IntegerDivisionResult, divisor: *const IntegerDivisor, payload: *const std::ffi::c_void,
    dst: Reg, first: u64, second: u64, third: u64, extra: u64,
    kind: Kind, operation: u32, flags: u32,
}
#[repr(C)]
struct Flow { next: *mut LocalValues, exits: *mut LocalValues }
struct Context<'a> {
    registers: &'a [Scalar], reuses: &'a mut Reuses, output: Vec<Inst>,
    divisions: BTreeMap<(Reg, u64, u64), Division>,
}
type Get = unsafe extern "C" fn(*const Values, RegMap, usize) -> Optional;
#[repr(C)]
#[derive(Clone, Copy)]
struct Host {
    read: unsafe extern "C" fn(*mut Inst, *const Context<'_>) -> Node,
    remap: unsafe extern "C" fn(*mut Inst, *const Values),
    block: unsafe extern "C" fn(*mut Vec<Inst>, *mut Values, *mut Context<'_>) -> Flow,
    state: unsafe extern "C" fn(StateOp, *mut Values, *mut Values, Map) -> *mut Values,
    get: Get,
    put: unsafe extern "C" fn(*mut Values, RegMap, usize, usize),
    remove_range: unsafe extern "C" fn(*mut Values, usize, usize),
    snapshot: unsafe extern "C" fn(*const Values) -> *mut LocalValues,
    intersect: unsafe extern "C" fn(*mut LocalValues, *mut LocalValues),
    drop_snapshot: unsafe extern "C" fn(*mut LocalValues),
    apply: unsafe extern "C" fn(*mut Values, *mut LocalValues),
    expression: unsafe extern "C" fn(*mut Values, Expression, Reg) -> Optional,
    division: unsafe extern "C" fn(*mut Context<'_>, DivisionOp, Reg, u64, u64, Division) -> Division,
    rewrite_division: unsafe extern "C" fn(*mut IntegerDivisionResult, DivisionChoice, Reg, Reg),
    pair: unsafe extern "C" fn(*mut Context<'_>, *const Inst, Division, *const IntegerDivisor, Reg, Reg, Reg),
    position: unsafe extern "C" fn(*const Context<'_>) -> usize,
    note: unsafe extern "C" fn(*mut Context<'_>, Reg, *const Inst, Reuse),
    memory_get: unsafe extern "C" fn(*const Values, usize, Reg, u32) -> Memory,
    memory_put: unsafe extern "C" fn(*mut Values, usize, Reg, u32, Reg, *const Inst),
    memory_filter: unsafe extern "C" fn(*mut Values, u32, usize, Reg, u32),
    aliases: unsafe extern "C" fn(*mut Values, *const IndexedSource),
    written: unsafe extern "C" fn(*mut Values, *const Vec<Inst>, *const Vec<Inst>),
    call: unsafe extern "C" fn(*mut Values, *const Call),
}

unsafe extern "C" {
    #[link_name = "bootstrap.stage_fusion::value_instruction"]
    fn value_instruction(instruction: *mut Inst, values: *mut Values, flow: *mut Flow, context: *mut Context<'_>, host: Host) -> Action;
    #[link_name = "bootstrap.stage_fusion::value_finish"]
    fn value_finish(values: *const Values, flow: Flow, continuing: u32, host: Host) -> Flow;
    #[link_name = "bootstrap.stage_fusion::value_discard_flow"]
    fn value_discard_flow(flow: Flow, host: Host);
    #[link_name = "bootstrap.stage_fusion::value_root"]
    fn value_root(values: *const Values, reg: Reg, get: Get) -> Reg;
    #[link_name = "bootstrap.stage_fusion::value_memory_keeps"]
    fn value_memory_keeps(mode: u32, binding: usize, index: Reg, word: u32, target: usize, at: Reg, field: u32) -> u32;
    #[link_name = "bootstrap.stage_fusion::value_alias"]
    fn value_alias(values: *mut Values, kind: u32, first: usize, length: usize, host: Host);
    #[link_name = "bootstrap.stage_fusion::value_call_effect"]
    fn value_call_effect(values: *mut Values, kind: u32, first: usize, length: usize, writes: u32, host: Host);
}
pub(super) fn root(values: &Values, reg: Reg) -> Reg { unsafe { value_root(values, reg, get) } }
pub(super) fn run(code: &mut Vec<Inst>, values: &mut Values, registers: &[Scalar], reuses: &mut Reuses) {
    let flow = run_block(code, values, registers, reuses);
    unsafe { value_discard_flow(flow, HOST) }
}
pub(super) fn extend_aliases(target: &mut Values, source: Values) { target.aliases.extend(source.aliases); }
pub(super) fn aliases_only(values: &Values) -> Values { Values { aliases: values.aliases.clone(), ..Values::default() } }

fn run_block(code: &mut Vec<Inst>, values: &mut Values, registers: &[Scalar], reuses: &mut Reuses) -> Flow {
    let mut context = Context { registers, reuses, output: Vec::new(), divisions: BTreeMap::new() };
    let mut flow = Flow { next: std::ptr::null_mut(), exits: std::ptr::null_mut() };
    let mut continuing = true;
    for mut inst in std::mem::take(code) {
        match unsafe { value_instruction(&mut inst, values, &mut flow, &mut context, HOST) } {
            Action::Discard => {}
            Action::Keep => context.output.push(inst),
            Action::Stop => { context.output.push(inst); continuing = false; break; }
        }
    }
    *code = context.output;
    unsafe { value_finish(values, flow, u32::from(continuing), HOST) }
}
fn binary_code(op: BinaryOp) -> u32 {
    match op {
        BinaryOp::FAdd => 0, BinaryOp::FSub => 1, BinaryOp::FMul => 2, BinaryOp::FDiv => 3,
        BinaryOp::IAdd => 4, BinaryOp::ISub => 5, BinaryOp::IMul => 6,
        BinaryOp::Compare(cmp) => 7 + cmp as u32,
        BinaryOp::And => 13, BinaryOp::Or => 14, BinaryOp::Xor => 15,
        BinaryOp::ShiftLeft => 16, BinaryOp::ShiftRightLogical => 17, BinaryOp::ShiftRightArithmetic => 18,
    }
}
unsafe extern "C" fn read(instruction: *mut Inst, context: *const Context<'_>) -> Node {
    let mut node = Node { first_code: std::ptr::null_mut(), second_code: std::ptr::null_mut(),
        arguments: std::ptr::NonNull::<Reg>::dangling().as_ptr(), count: 0,
        division_result: std::ptr::null_mut(), divisor: std::ptr::null(), payload: std::ptr::null(),
        dst: 0, first: 0, second: 0, third: 0, extra: 0, kind: Kind::Ordinary, operation: 0, flags: 0 };
    let inst = unsafe { &mut *instruction }; let registers = unsafe { &*context }.registers;
    match &mut inst.kind {
        InstKind::Constant(dst, scalar, bits) => { node.kind = Kind::Constant; node.dst = *dst; node.first = *bits; node.operation = *scalar as u32; }
        InstKind::Index(dst) => { node.kind = Kind::Index; node.dst = *dst; }
        InstKind::Uniform(dst, offset) => { node.kind = Kind::Uniform; node.dst = *dst; node.first = u64::from(*offset); }
        InstKind::BufferStride(dst, binding) => { node.kind = Kind::BufferStride; node.dst = *dst; node.first = *binding as u64; }
        InstKind::Unary { dst, op, a } => { node.kind = Kind::Unary; node.dst = *dst; node.first = *a as u64; node.second = registers[*dst] as u64; node.operation = *op as u32; }
        InstKind::Binary { dst, op, a, b } => { node.kind = Kind::Binary; node.dst = *dst; node.first = *a as u64; node.second = *b as u64; node.operation = binary_code(*op); }
        InstKind::Cast { dst, src } => { node.kind = Kind::Cast; node.dst = *dst; node.first = *src as u64; node.operation = registers[*dst] as u32; }
        InstKind::Math { dst, op, args } => { node.kind = Kind::Math; node.dst = *dst; node.operation = *op as u32; node.arguments = args.as_mut_ptr(); node.count = args.len(); }
        InstKind::Select { dst, condition, a, b } => { node.kind = Kind::Select; node.dst = *dst; node.first = *condition as u64; node.second = *a as u64; node.third = *b as u64; }
        InstKind::IntegerDivide { result, src, divisor } => {
            node.kind = Kind::Division; node.first = *src as u64; node.division_result = result; node.divisor = divisor;
            match divisor { IntegerDivisor::Constant(value) => { node.second = 1; node.third = value.get(); }, IntegerDivisor::Value(reg) => { node.second = 2; node.third = *reg as u64; } }
            if let Some(reg) = result.quotient() { node.flags |= 1; node.dst = reg; }
            if let Some(reg) = result.remainder() { node.flags |= 2; node.extra = reg as u64; }
        }
        InstKind::Scope(code) => { node.kind = Kind::Scope; node.first_code = code; }
        InstKind::Raise(_) => node.kind = Kind::Raise,
        InstKind::Exit => node.kind = Kind::Exit,
        InstKind::Break => node.kind = Kind::Break,
        InstKind::Continue => node.kind = Kind::Continue,
        InstKind::Atomic(atomic) => { node.kind = Kind::Atomic; node.flags = u32::from(atomic.op.writes()); node.payload = (&atomic.target as *const IndexedSource).cast(); }
        InstKind::Append(_) => node.kind = Kind::Append,
        InstKind::TileBarrier => node.kind = Kind::Barrier,
        InstKind::TilePublish(_) => node.kind = Kind::Publish,
        InstKind::TileCollective(_) => node.kind = Kind::Collective,
        InstKind::LoadLocal(dst, slot) => { node.kind = Kind::LocalRead; node.dst = *dst; node.first = *slot as u64; }
        InstKind::StoreLocal(slot, value) => { node.kind = Kind::LocalWrite; node.first = *slot as u64; node.second = *value as u64; }
        InstKind::StoreLocalIndexed { base, len, .. } => { node.kind = Kind::LocalIndexedWrite; node.first = *base as u64; node.second = *len as u64; }
        InstKind::StoreIndexed { source, .. } => { node.kind = Kind::IndexedWrite; node.payload = (source as *const IndexedSource).cast(); }
        InstKind::Load { dst, binding, index, word } => { node.kind = Kind::Load; node.dst = *dst; node.first = *binding as u64; node.second = *index as u64; node.third = u64::from(*word); }
        InstKind::Store { binding, index, word, value } => { node.kind = Kind::Store; node.first = *binding as u64; node.second = *index as u64; node.third = u64::from(*word); node.extra = *value as u64; }
        InstKind::If { yes, no, .. } => { node.kind = Kind::If; node.first_code = yes; node.second_code = no; }
        InstKind::Loop { condition, test, body } => { node.kind = Kind::Loop; node.first_code = condition; node.second_code = body; node.arguments = test; node.count = 1; }
        InstKind::Call(call) => { node.kind = Kind::Call; node.payload = (call as *const Call).cast(); }
        _ => {}
    }
    node
}
unsafe extern "C" fn remap(instruction: *mut Inst, values: *const Values) {
    let values = unsafe { &*values }; super::super::map_operands(unsafe { &mut *instruction }, &|r| root(values, r));
}
unsafe extern "C" fn block(code: *mut Vec<Inst>, values: *mut Values, context: *mut Context<'_>) -> Flow {
    let context = unsafe { &mut *context }; run_block(unsafe { &mut *code }, unsafe { &mut *values }, context.registers, context.reuses)
}
unsafe extern "C" fn state(op: StateOp, target: *mut Values, source: *mut Values, map: Map) -> *mut Values {
    macro_rules! fields { ($action:ident) => { match map {
        Map::Aliases => fields!(@$action aliases), Map::Expressions => fields!(@$action expressions),
        Map::Memory => fields!(@$action memory), Map::Locals => fields!(@$action locals),
    } }; (@move $field:ident) => { unsafe { &mut *target }.$field = std::mem::take(&mut unsafe { &mut *source }.$field) };
        (@copy $field:ident) => { unsafe { &mut *target }.$field = unsafe { &*source }.$field.clone() };
        (@clear $field:ident) => { unsafe { &mut *target }.$field.clear() };
        (@extend $field:ident) => { unsafe { &mut *target }.$field.extend(std::mem::take(&mut unsafe { &mut *source }.$field)) };
    }
    match op {
        StateOp::New => return Box::into_raw(Box::new(Values::default())),
        StateOp::Clone => return Box::into_raw(Box::new(unsafe { &*source }.clone())),
        StateOp::Drop => drop(unsafe { Box::from_raw(target) }),
        StateOp::Move => fields!(move), StateOp::Copy => fields!(copy), StateOp::Clear => fields!(clear), StateOp::Extend => fields!(extend),
    }
    std::ptr::null_mut()
}
unsafe extern "C" fn get(values: *const Values, map: RegMap, key: usize) -> Optional {
    let values = unsafe { &*values }; let value = match map { RegMap::Aliases => values.aliases.get(&key), RegMap::Locals => values.locals.get(&key) };
    Optional { value: value.copied().unwrap_or(0), present: u32::from(value.is_some()) }
}
unsafe extern "C" fn put(values: *mut Values, map: RegMap, key: usize, value: usize) {
    let values = unsafe { &mut *values }; match map { RegMap::Aliases => { values.aliases.insert(key, value); }, RegMap::Locals => { values.locals.insert(key, value); } }
}
unsafe extern "C" fn remove_range(values: *mut Values, first: usize, end: usize) {
    unsafe { &mut *values }.locals.retain(|slot, _| !(first..end).contains(slot));
}
unsafe extern "C" fn snapshot(values: *const Values) -> *mut LocalValues {
    let values = unsafe { &*values }; Box::into_raw(Box::new(values.locals.iter()
        .map(|(&slot, &value)| LocalValue { slot, value: root(values, value) }).collect()))
}
unsafe extern "C" fn intersect(left: *mut LocalValues, right: *mut LocalValues) {
    let left = unsafe { &mut *left }; let right = unsafe { Box::from_raw(right) };
    let kept = unsafe { intersect_local_values(left.as_mut_ptr(), left.len(), right.as_ptr(), right.len()) }; left.truncate(kept);
}
unsafe extern "C" fn drop_snapshot(snapshot: *mut LocalValues) { drop(unsafe { Box::from_raw(snapshot) }); }
unsafe extern "C" fn apply(values: *mut Values, snapshot: *mut LocalValues) {
    unsafe { &mut *values }.locals = unsafe { *Box::from_raw(snapshot) }.into_iter().map(|entry| (entry.slot, entry.value)).collect();
}
unsafe extern "C" fn expression(values: *mut Values, key: Expression, dst: Reg) -> Optional {
    let key = ExpressionKey { kind: key.kind, operation: key.operation, words: key.words,
        arguments: unsafe { std::slice::from_raw_parts(key.arguments, key.count) }.to_vec() };
    match unsafe { &mut *values }.expressions.entry(key) {
        Entry::Occupied(old) => Optional { value: *old.get(), present: 1 },
        Entry::Vacant(slot) => { slot.insert(dst); Optional { value: 0, present: 0 } }
    }
}
unsafe extern "C" fn division(context: *mut Context<'_>, op: DivisionOp, src: Reg, tag: u64, divisor: u64, value: Division) -> Division {
    let context = unsafe { &mut *context }; match op {
        DivisionOp::Clear => context.divisions.clear(),
        DivisionOp::Take => return context.divisions.remove(&(src, tag, divisor)).unwrap_or_default(),
        DivisionOp::Put => { context.divisions.insert((src, tag, divisor), value); }
    }
    Division::default()
}
unsafe extern "C" fn rewrite_division(result: *mut IntegerDivisionResult, choice: DivisionChoice, quotient: Reg, remainder: Reg) {
    unsafe { *result = match choice { DivisionChoice::Quotient => IntegerDivisionResult::Quotient(quotient),
        DivisionChoice::Remainder => IntegerDivisionResult::Remainder(remainder), DivisionChoice::Both => IntegerDivisionResult::Both { quotient, remainder } }; }
}
unsafe extern "C" fn pair(context: *mut Context<'_>, instruction: *const Inst, previous: Division, divisor: *const IntegerDivisor, src: Reg, quotient: Reg, remainder: Reg) {
    let context = unsafe { &mut *context }; let old = OperationOrigin::from_instruction(&context.output[previous.position]); let current = OperationOrigin::from_instruction(unsafe { &*instruction });
    let (quotient_origin, remainder_origin) = if previous.mask == 1 { (old, current) } else { (current, old) };
    context.output[previous.position] = Inst::generated(InstKind::IntegerDivide { result: IntegerDivisionResult::Both { quotient, remainder }, src, divisor: unsafe { *divisor } },
        std::sync::Arc::new(GeneratedOrigin { reason: GenerationReason::IntegerDivisionPair,
            trigger: Some(GenerationAnchor::IntegerDivisionPair { quotient: quotient_origin, remainder: remainder_origin }) }));
}
unsafe extern "C" fn position(context: *const Context<'_>) -> usize { unsafe { &*context }.output.len() }
unsafe extern "C" fn note(context: *mut Context<'_>, retained: Reg, instruction: *const Inst, reuse: Reuse) {
    let origin = OperationOrigin::from_instruction(unsafe { &*instruction }); let fact = match reuse.kind {
        ReuseKind::Expression => RetainedUse::Expression(ExpressionReuse { result: match reuse.role { Role::Value => ReuseResult::Value, Role::Quotient => ReuseResult::Quotient, Role::Remainder => ReuseResult::Remainder }, paired_input: reuse.paired != 0, origin }),
        ReuseKind::LocalRead => RetainedUse::LocalRead(origin), ReuseKind::IndexedRead => RetainedUse::IndexedRead(origin),
        ReuseKind::IndexedStoreRead => RetainedUse::IndexedStoreRead { store: unsafe { &*reuse.origin }.clone(), read: origin },
    }; unsafe { &mut *context }.reuses.entry(retained).or_default().push(fact);
}
unsafe extern "C" fn memory_get(values: *const Values, binding: usize, index: Reg, word: u32) -> Memory {
    match unsafe { &*values }.memory.get(&(binding, index, word)) {
        Some((value, origin)) => Memory { value: *value, origin: origin.as_ref().map_or(std::ptr::null(), |origin| origin), present: 1 },
        None => Memory { value: 0, origin: std::ptr::null(), present: 0 },
    }
}
unsafe extern "C" fn memory_put(values: *mut Values, binding: usize, index: Reg, word: u32, value: Reg, instruction: *const Inst) {
    let origin = unsafe { instruction.as_ref() }.map(OperationOrigin::from_instruction); unsafe { &mut *values }.memory.insert((binding, index, word), (value, origin));
}
unsafe extern "C" fn memory_filter(values: *mut Values, mode: u32, binding: usize, index: Reg, word: u32) {
    unsafe { &mut *values }.memory.retain(|&(b, i, w), _| unsafe { value_memory_keeps(mode, b, i, w, binding, index, word) != 0 });
}
unsafe extern "C" fn aliases(values: *mut Values, source: *const IndexedSource) {
    for alias in unsafe { &*source }.aliases() {
        match alias { MemoryAlias::Local { base, len } => unsafe { value_alias(values, 0, base, len, HOST) }, MemoryAlias::Data(binding) => unsafe { value_alias(values, 1, binding, 0, HOST) } }
    }
}
unsafe extern "C" fn written(values: *mut Values, first: *const Vec<Inst>, second: *const Vec<Inst>) {
    let mut written = BTreeSet::new(); for block in [unsafe { &*first }, unsafe { &*second }] { super::super::visit(block, &mut |inst| written.extend(inst.local_writes())); }
    for slot in written { unsafe { &mut *values }.locals.remove(&slot); }
}
unsafe extern "C" fn call(values: *mut Values, call: *const Call) {
    let call = unsafe { &*call };
        for slot in call.locals(true) { unsafe { value_call_effect(values, 0, slot, 0, 1, HOST) }; }
        for effect in &call.memory { if let MemoryAlias::Data(binding) = effect.alias { unsafe { value_call_effect(values, 1, binding, 0, u32::from(effect.access.writes()), HOST) }; } }
        for argument in &call.arguments {
            if let CallArgument::Reference { access, address: CallReference { storage: CallStorage::Indexed(source), .. }, .. } = argument {
                match source {
                    IndexedSource::Pointer { .. } => for alias in source.aliases() {
                        match alias { MemoryAlias::Local { base, len } => unsafe { value_call_effect(values, 3, base, len, u32::from(access.writes()), HOST) }, MemoryAlias::Data(binding) => unsafe { value_call_effect(values, 4, binding, 0, u32::from(access.writes()), HOST) } }
                    },
                    _ => unsafe { value_call_effect(values, 2, 0, 0, u32::from(access.writes()), HOST) },
                }
            }
        }
}
const HOST: Host = Host { read, remap, block, state, get, put, remove_range, snapshot, intersect,
    drop_snapshot, apply, expression, division, rewrite_division, pair, position, note,
    memory_get, memory_put, memory_filter, aliases, written, call };
