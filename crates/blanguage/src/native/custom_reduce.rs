//! 普通值 IR 直接成为范围累积与摘要合并机器码，不经宿主解释合并器。
use super::*;
use external::{AbiComponent, AbiValueComponent};
use structure::{ScalarBodyStatus, ScalarHost};
use crate::ir::dataflow::Span;
use std::{collections::BTreeSet, ffi::c_void};

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
    keep_slot: unsafe extern "C" fn(*mut c_void, usize, bool),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ir_dataflow::cpu_range_plan"]
    fn cpu_range_plan(pass: Pass, words: usize, has_head: u32, counted: u32) -> RangePlan;
    #[link_name = "bootstrap.ir_dataflow::cpu_range_body"]
    fn cpu_range_body(input: RangeInput, plan: RangePlan, context: *mut c_void,
        host: RangeHost) -> RangeBody;
    #[link_name = "bootstrap.stage_fusion::private_range_live"]
    fn private_range_live(code: Span, live: *mut BTreeSet<PrivateSlot>,
        empty: *const BTreeSet<PrivateSlot>, host: PrivateLiveHost);
}

struct RangeWriter<'a> {
    lowered: &'a mut ReductionBody,
    reduction: &'a CustomReduction,
    live: &'a mut BTreeSet<PrivateSlot>,
    project: Vec<Inst>,
    combine: Vec<Inst>,
    yes: Vec<Inst>,
    no: Vec<Inst>,
}
impl RangeWriter<'_> {
    fn block(&mut self, part: RangeBlock) -> &mut Vec<Inst> {
        match part {
            RangeBlock::Body => &mut self.lowered.code,
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
    let reg = writer.lowered.registers.len();
    writer.lowered.registers.push(scalar);
    reg
}

unsafe extern "C" fn range_local(context: *mut c_void, ty: RangeType) -> usize {
    let writer = unsafe { &mut *context.cast::<RangeWriter<'_>>() };
    let layout = match ty {
        RangeType::U32 => Ty::scalar(Scalar::U32),
        RangeType::Bool => Ty::scalar(Scalar::Bool),
        RangeType::Value => writer.reduction.ty.clone(),
    };
    let slot = writer.lowered.locals.last_key_value().map_or(0, |(&end, _)| end);
    writer.lowered.locals.insert(slot + layout.components.len(), layout);
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
        RangeOp::Constant => InstKind::Constant(a, writer.lowered.registers[a], b as Bits),
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
    writer.lowered.code.extend(code);
}

unsafe extern "C" fn range_branch(context: *mut c_void, condition: Reg, alternate: RangeBlock) {
    let writer = unsafe { &mut *context.cast::<RangeWriter<'_>>() };
    let yes = std::mem::take(&mut writer.yes);
    let no = std::mem::take(writer.block(alternate));
    writer.lowered.code.push(Inst::new(InstKind::If { condition, yes, no }));
}

unsafe extern "C" fn range_size(context: *mut c_void) -> usize {
    unsafe { &*context.cast::<RangeWriter<'_>>() }.lowered.code.len()
}

unsafe extern "C" fn keep_range_slot(context: *mut c_void, slot: usize, local: bool) {
    let writer = unsafe { &mut *context.cast::<RangeWriter<'_>>() };
    writer.live.insert(if local { PrivateSlot::Local(slot) } else { PrivateSlot::Register(slot) });
}

const RANGE_HOST: RangeHost = RangeHost {
    reg: range_reg, local: range_local, offset: range_offset, put: range_put,
    move_block: range_move_block, branch: range_branch, size: range_size, keep_slot: keep_range_slot,
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
    saved: usize,
}

#[repr(C)]
struct CustomRangeHost {
    frame: RangeFrameHost,
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

unsafe extern "C" fn range_frame(slot: *mut c_void, code_bytes: usize, operations: usize) -> FrameOutput {
    unsafe { &mut *slot.cast::<Emitter<'_>>() }.reserve_frame(code_bytes, operations)
}

unsafe extern "C" fn publish_range_frame(slot: *mut c_void, entry: FrameEntry) {
    unsafe { (&mut *slot.cast::<Emitter<'_>>()).publish_frame(entry) };
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
    let stage = &program.stages[index];
    let mut kernel = crate::optimize::reduction_parts(r, plan.project != 0, plan.identity != 0, None);
    let mut lowered = kernel.stage;
    let [identity, project, mut combine] = kernel.parts;
    lowered.code.extend(identity);
    let private = program.buffers.len();
    let buffer_count = private + usize::from(plan.apply != 0 || plan.finish != 0);

    crate::ir::rewrite::bind_reduction_arguments(
        &mut combine, &mut kernel.result, &kernel.right, &kernel.projected, lowered.registers.len(),
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
    let mut live = BTreeSet::new();
    let mut writer = RangeWriter {
        lowered: &mut lowered, reduction: r, live: &mut live, project, combine,
        yes: Vec::new(), no: Vec::new(),
    };
    let body = unsafe { cpu_range_body(input, plan, std::ptr::from_mut(&mut writer).cast(), RANGE_HOST) };
    let carry = body.carry;
    let mut phase_ends = [body.initialization_end, body.loop_end];
    reads::place_phases(&mut lowered.code, &mut lowered.registers, &stage.writes, &mut phase_ends);
    let [initialize_end, loop_end] = phase_ends;
    unsafe {
        private_range_live(Span::slice(&lowered.code[initialize_end..loop_end]), &mut live,
            &BTreeSet::new(), PRIVATE_LIVE_HOST);
    }
    let mut graph = allocation::Graph::with_live(&lowered.code, &lowered.registers, &lowered.locals, live);
    let allocation = Allocation::new(&lowered.locals, &mut graph, false);
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
        registers: &lowered.registers,
        tiles: &[],
        range_origin: OperationOrigin::default(),
        workgroup: Stage::DEFAULT_WORKGROUP_WIDTH,
        buffer_count,
        textures: &program.textures,
        allocation,
        frame: layout,
    };
    let saved = e.frame.saved.len();
    let emitter = std::ptr::from_mut(&mut e);
    let summary = RangeSummary {
        emitter, first_slot: carry, fields: &r.ty.components, payload_words: k,
    };
    let input = CustomRangeInput {
        initialize: Span::slice(&lowered.code[..initialize_end]),
        body: Span::slice(&lowered.code[initialize_end..loop_end]),
        finish: Span::slice(&lowered.code[loop_end..]),
        frame,
        break_on_head: body.break_on_head,
        publish: plan.publish,
        summary_count: fields + (plan.summary - k),
        summary: std::ptr::from_ref(&summary).cast(),
        saved,
    };
    let emitted = unsafe {
        emit_custom_range(emitter.cast(), input, ScalarHost::new(emitter.cast(), Stage::DEFAULT_WORKGROUP_WIDTH),
            CustomRangeHost {
                frame: RangeFrameHost { reserve: range_frame, publish: publish_range_frame },
                summary_component,
            })
    };
    emitted.status.result()?;
    Ok(e.finish(emitted.stack_bytes, true))
}
