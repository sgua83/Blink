//! 普通值 IR 直接成为范围累积与摘要合并机器码，不经宿主解释合并器。
use super::*;
use external::{AbiComponent, AbiValueComponent};
use structure::{ScalarBodyStatus, ScalarHost};
use crate::ir::dataflow::Span;
use std::ffi::c_void;

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pass {
    Reduce,
    ReduceTail,
    Scan,
    ScanPrefix,
    ScanAdd,
    SegmentReduce,
    SegmentPrefix,
    SegmentFinish,
}
pub fn emit_custom_reduce_tail(
    program: &Program,
    index: usize,
) -> Result<NativeCode, &'static str> {
    let Operation::CustomReduce(r) = &program.stages[index].operation else {
        return Err("摘要合并需要自定义归约阶段");
    };
    emit(program, index, r, Pass::ReduceTail, None, None)
}
pub fn emit_custom_scan_prefix(
    program: &Program,
    index: usize,
) -> Result<NativeCode, &'static str> {
    scan(program, index, Pass::ScanPrefix)
}
pub fn emit_custom_scan_add(program: &Program, index: usize) -> Result<NativeCode, &'static str> {
    scan(program, index, Pass::ScanAdd)
}
fn scan(program: &Program, index: usize, pass: Pass) -> Result<NativeCode, &'static str> {
    let Operation::CustomScan(r) = &program.stages[index].operation else {
        return Err("自定义扫描代码生成需要扫描阶段");
    };
    emit(program, index, &r.values, pass, r.head_word, None)
}
pub fn emit_custom_segmented_prefix(
    program: &Program,
    index: usize,
) -> Result<NativeCode, &'static str> {
    segmented(program, index, Pass::SegmentPrefix)
}
pub fn emit_custom_segmented_finish(
    program: &Program,
    index: usize,
) -> Result<NativeCode, &'static str> {
    segmented(program, index, Pass::SegmentFinish)
}
fn segmented(program: &Program, index: usize, pass: Pass) -> Result<NativeCode, &'static str> {
    let Operation::CustomSegmentedReduce(r) = &program.stages[index].operation else {
        return Err("分段摘要与段尾代码需要自定义分段归约阶段");
    };
    emit(program, index, &r.values, pass, Some(r.head_word), Some(r.count))
}
#[derive(Clone, Copy)]
#[repr(C)]
struct RangePlan {
    project: u32,
    identity: u32,
    apply: u32,
    finish: u32,
    packed: u32,
    prefix: u32,
    publish: u32,
    summary: usize,
}

#[repr(C)]
struct RangeInput {
    pass: Pass,
    has_head: u32,
    head_word: u32,
    counted: u32,
    source: usize,
    target: usize,
    temporary: usize,
    count_binding: usize,
    count_word: u32,
    source_word: u32,
    target_word: u32,
    payload_bytes: u32,
    fields: usize,
    initial: Span,
    left: Span,
    source_registers: Span,
    projected: Span,
    result: Span,
}

#[repr(C)]
struct RangeBody {
    initialization_end: usize,
    loop_end: usize,
    carry: usize,
    count_slot: usize,
    length_slot: usize,
    history: u32,
    reset_on_head: u32,
    break_on_head: u32,
}

#[allow(dead_code)]
#[repr(u32)]
enum RangeType { U32, Bool, Value }

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum RangeBlock { Body, Yes, No, Project, Combine }

#[allow(dead_code)]
#[repr(u32)]
enum RangeOp { Index, Constant, Load, Store, LocalLoad, LocalStore,
    Add, Subtract, Equal, Or, Uniform, Break }

#[repr(C)]
struct RangeHost {
    reg: unsafe extern "C" fn(*mut c_void, RangeType, usize) -> Reg,
    local: unsafe extern "C" fn(*mut c_void, RangeType) -> usize,
    offset: unsafe extern "C" fn(*mut c_void, usize, u32) -> u32,
    put: unsafe extern "C" fn(*mut c_void, RangeBlock, RangeOp, usize, usize, usize, u32),
    move_block: unsafe extern "C" fn(*mut c_void, RangeBlock),
    branch: unsafe extern "C" fn(*mut c_void, Reg, RangeBlock),
    size: unsafe extern "C" fn(*mut c_void) -> usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::cpu_range_plan"]
    fn cpu_range_plan(pass: Pass, words: usize, has_head: u32, counted: u32) -> RangePlan;
    #[link_name = "bootstrap.ir_dataflow::cpu_range_body"]
    fn cpu_range_body(input: RangeInput, plan: RangePlan, context: *mut c_void,
        host: RangeHost) -> RangeBody;
}

struct RangeWriter<'a> {
    stage: &'a mut Stage,
    reduction: &'a CustomReduction,
    project: Vec<Inst>,
    combine: Vec<Inst>,
    yes: Vec<Inst>,
    no: Vec<Inst>,
}
impl RangeWriter<'_> {
    fn block(&mut self, part: RangeBlock) -> &mut Vec<Inst> {
        match part {
            RangeBlock::Body => &mut self.stage.code,
            RangeBlock::Yes => &mut self.yes,
            RangeBlock::No => &mut self.no,
            RangeBlock::Project => &mut self.project,
            RangeBlock::Combine => &mut self.combine,
        }
    }
}

unsafe extern "C" fn range_reg(context: *mut c_void, ty: RangeType, component: usize) -> Reg {
    let writer = unsafe { &mut *context.cast::<RangeWriter<'_>>() };
    let scalar = match ty {
        RangeType::U32 => Scalar::U32,
        RangeType::Bool => Scalar::Bool,
        RangeType::Value => writer.reduction.ty.components[component].0,
    };
    let reg = writer.stage.registers.len();
    writer.stage.registers.push(scalar);
    reg
}

unsafe extern "C" fn range_local(context: *mut c_void, ty: RangeType) -> usize {
    let writer = unsafe { &mut *context.cast::<RangeWriter<'_>>() };
    let layout = match ty {
        RangeType::U32 => Ty::scalar(Scalar::U32),
        RangeType::Bool => Ty::scalar(Scalar::Bool),
        RangeType::Value => writer.reduction.ty.clone(),
    };
    let slot = writer.stage.locals.last_key_value().map_or(0, |(&end, _)| end);
    writer.stage.locals.insert(slot + layout.components.len(), layout);
    slot
}

unsafe extern "C" fn range_offset(context: *mut c_void, component: usize, source: u32) -> u32 {
    let writer = unsafe { &*context.cast::<RangeWriter<'_>>() };
    let layout = if source != 0 { &writer.reduction.source_ty } else { &writer.reduction.ty };
    layout.components[component].1
}

unsafe extern "C" fn range_put(context: *mut c_void, part: RangeBlock, op: RangeOp,
    a: usize, b: usize, c: usize, word: u32,
) {
    let writer = unsafe { &mut *context.cast::<RangeWriter<'_>>() };
    let kind = match op {
        RangeOp::Index => InstKind::Index(a),
        RangeOp::Constant => InstKind::Constant(a, writer.stage.registers[a], b as Bits),
        RangeOp::Load => InstKind::Load { dst: a, binding: b, index: c, word },
        RangeOp::Store => InstKind::Store { value: a, binding: b, index: c, word },
        RangeOp::LocalLoad => InstKind::LoadLocal(a, b),
        RangeOp::LocalStore => InstKind::StoreLocal(a, b),
        RangeOp::Add => InstKind::Binary { dst: a, op: BinaryOp::IAdd, a: b, b: c },
        RangeOp::Subtract => InstKind::Binary { dst: a, op: BinaryOp::ISub, a: b, b: c },
        RangeOp::Equal => InstKind::Binary { dst: a, op: BinaryOp::Compare(Comparison::Equal), a: b, b: c },
        RangeOp::Or => InstKind::Binary { dst: a, op: BinaryOp::Or, a: b, b: c },
        RangeOp::Uniform => InstKind::Uniform(a, word),
        RangeOp::Break => InstKind::Break,
    };
    writer.block(part).push(Inst::new(kind));
}

unsafe extern "C" fn range_move_block(context: *mut c_void, part: RangeBlock) {
    let writer = unsafe { &mut *context.cast::<RangeWriter<'_>>() };
    let code = std::mem::take(writer.block(part));
    writer.stage.code.extend(code);
}

unsafe extern "C" fn range_branch(context: *mut c_void, condition: Reg, alternate: RangeBlock) {
    let writer = unsafe { &mut *context.cast::<RangeWriter<'_>>() };
    let yes = std::mem::take(&mut writer.yes);
    let no = std::mem::take(writer.block(alternate));
    writer.stage.code.push(Inst::new(InstKind::If { condition, yes, no }));
}

unsafe extern "C" fn range_size(context: *mut c_void) -> usize {
    unsafe { &*context.cast::<RangeWriter<'_>>() }.stage.code.len()
}

const RANGE_HOST: RangeHost = RangeHost {
    reg: range_reg, local: range_local, offset: range_offset, put: range_put,
    move_block: range_move_block, branch: range_branch, size: range_size,
};

#[repr(C)]
struct CustomRangeInput {
    initialize: Span,
    body: Span,
    finish: Span,
    frame: u32,
    break_on_head: u32,
    publish: u32,
    summary_count: usize,
    summary: *const c_void,
}

#[repr(C)]
struct CustomRangeHost {
    enter: unsafe extern "C" fn(*mut c_void, u32) -> u32,
    leave: unsafe extern "C" fn(*mut c_void, u32),
    breaks: unsafe extern "C" fn(*mut c_void, u32),
    summary_component: AbiComponent,
}

#[repr(C)]
struct CustomRangeProduct {
    stack_bytes: u32,
    status: ScalarBodyStatus,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::emit_custom_range"]
    fn emit_custom_range(
        emitter: *mut c_void, input: CustomRangeInput, scalar: ScalarHost, host: CustomRangeHost,
    ) -> CustomRangeProduct;
}

struct RangeSummary<'a> {
    emitter: *mut Emitter<'a>,
    first_slot: usize,
    fields: &'a [(Scalar, u32)],
    payload_words: usize,
}

unsafe extern "C" fn summary_component(context: *const c_void, index: u64) -> AbiValueComponent {
    let input = unsafe { &*context.cast::<RangeSummary<'_>>() };
    let index = index as usize;
    let (scalar, offset) = match input.fields.get(index) {
        Some(&field) => field,
        None => (Scalar::U32, (input.payload_words + index - input.fields.len()) as u32 * 4),
    };
    // The image has been lent to the B writer. Borrow only the disjoint allocation.
    let allocation = unsafe { &(*input.emitter).allocation };
    let location = allocation.location(PrivateSlot::Local(input.first_slot + index));
    AbiValueComponent {
        location: location.into(), scalar: scalar.into(), field_scalar: scalar.into(), offset,
    }
}

unsafe extern "C" fn enter_range(slot: *mut c_void, frame: u32) -> u32 {
    unsafe { &mut *slot.cast::<Emitter<'_>>() }.enter(frame)
}

unsafe extern "C" fn leave_range(slot: *mut c_void, frame: u32) {
    unsafe { &mut *slot.cast::<Emitter<'_>>() }.leave(frame);
}

unsafe extern "C" fn break_scope(slot: *mut c_void, open: u32) {
    let emitter = unsafe { &mut *slot.cast::<Emitter<'_>>() };
    if open != 0 {
        emitter.breaks.push(Vec::new());
    } else {
        emitter.close_breaks();
    }
}

pub(super) fn emit(
    program: &Program,
    index: usize,
    r: &CustomReduction,
    pass: Pass,
    head_word: Option<u32>,
    count: Option<PrefixCount>,
) -> Result<NativeCode, &'static str> {
    let k = r.ty.size.div_ceil(4) as usize;
    let fields = r.ty.components.len();
    let counted = count.is_some();
    let has_head = u32::from(head_word.is_some());
    let plan = unsafe { cpu_range_plan(pass, k, has_head, u32::from(counted)) };
    let mut kernel = r.kernel(&program.stages[index], plan.project != 0, plan.identity != 0);
    let mut stage = kernel.stage;
    let private = program.buffers.len();
    let buffer_count = private + usize::from(plan.apply != 0 || plan.finish != 0);

    let mut combine = stage.code.split_off(kernel.project_end);
    let project = stage.code.split_off(kernel.initialize_end);
    if plan.apply != 0 {
        stage.code.truncate(kernel.invariant_end);
    }
    crate::ir::rewrite::bind_reduction_arguments(
        &mut combine, &mut kernel.result, &kernel.right, &kernel.projected, stage.registers.len(),
    );

    let input = RangeInput {
        pass, has_head, head_word: head_word.unwrap_or_default(), counted: u32::from(counted),
        source: r.fields.source, target: r.fields.target, temporary: private,
        count_binding: count.as_ref().map_or(0, |count| count.binding),
        count_word: count.as_ref().map_or(0, |count| count.word),
        source_word: r.fields.source_word, target_word: r.fields.target_word,
        payload_bytes: k as u32 * 4, fields,
        initial: Span::slice(&kernel.initial), left: Span::slice(&kernel.left),
        source_registers: Span::slice(&kernel.source), projected: Span::slice(&kernel.projected),
        result: Span::slice(&kernel.result),
    };
    let mut writer = RangeWriter {
        stage: &mut stage, reduction: r, project, combine, yes: Vec::new(), no: Vec::new(),
    };
    let body = unsafe { cpu_range_body(input, plan, std::ptr::from_mut(&mut writer).cast(), RANGE_HOST) };
    let carry = body.carry;
    // Each actual range phase owns its own producers; no value crosses these cuts.
    let mut after_code = stage.code.split_off(body.loop_end);
    let mut loop_code = stage.code.split_off(body.initialization_end);
    reads::place(&mut stage.code, &mut stage.registers, &stage.writes);
    reads::place(&mut loop_code, &mut stage.registers, &stage.writes);
    reads::place(&mut after_code, &mut stage.registers, &stage.writes);
    let initialize_end = stage.code.len();
    stage.code.extend(loop_code);
    let loop_end = stage.code.len();
    stage.code.extend(after_code);
    let mut live: std::collections::BTreeSet<_> =
        (0..fields).map(|field| PrivateSlot::Local(carry + field)).collect();
    if body.history != 0 {
        live.insert(PrivateSlot::Local(carry + fields));
    }
    if counted {
        live.insert(PrivateSlot::Local(body.count_slot));
    }
    if plan.finish != 0 {
        live.insert(PrivateSlot::Local(body.length_slot));
    }
    if body.reset_on_head != 0 {
        live.extend(kernel.initial.iter().map(|r| PrivateSlot::Register(*r)));
    }
    live.extend(live_before(
        &stage.code[initialize_end..loop_end],
        live.clone(),
        &live,
        &live,
        &std::collections::BTreeSet::new(),
    ));
    let mut graph = allocation::Graph::with_live(&stage.code, &stage.registers, &stage.locals, live);
    let allocation = Allocation::new(&stage.locals, &mut graph, false);
    drop(graph);
    let layout = Frame::new(&allocation, None);
    let frame = i32::try_from(layout.end.div_ceil(16) * 16)
        .map_err(|_| "原生集合栈帧超过 x64 范围函数 ABI")? as u32;
    let mut e = Emitter {
        used_tile_words: &super::EMPTY_TILE_WORDS,
        unwind: Vec::new(),
        prolog: 0,
        probe: None,
        code: Vec::new(),
        relocations: Vec::new(),
        calls: Vec::new(),
        operation_ranges: Vec::new(),
        function_abi: None,
        literals: std::collections::BTreeMap::new(),
        registers: &stage.registers,
        tiles: &[],
        range_origin: OperationOrigin::default(),
        workgroup: Stage::DEFAULT_WORKGROUP_WIDTH,
        buffer_count,
        textures: &program.textures,
        breaks: Vec::new(),
        exits: vec![Vec::new()],
        continues: Vec::new(),
        allocation,
        frame: layout,
    };
    let emitter = std::ptr::from_mut(&mut e);
    let summary = RangeSummary {
        emitter, first_slot: carry, fields: &r.ty.components, payload_words: k,
    };
    let input = CustomRangeInput {
        initialize: Span::slice(&stage.code[..initialize_end]),
        body: Span::slice(&stage.code[initialize_end..loop_end]),
        finish: Span::slice(&stage.code[loop_end..]),
        frame,
        break_on_head: body.break_on_head,
        publish: plan.publish,
        summary_count: fields + (plan.summary - k),
        summary: std::ptr::from_ref(&summary).cast(),
    };
    let emitted = unsafe {
        emit_custom_range(emitter.cast(), input, ScalarHost::new(emitter.cast(), Stage::DEFAULT_WORKGROUP_WIDTH),
            CustomRangeHost { enter: enter_range, leave: leave_range, breaks: break_scope, summary_component })
    };
    emitted.status.result()?;
    Ok(e.finish(emitted.stack_bytes, true))
}
