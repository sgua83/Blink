//! 按 SPIR-V 1.4 标准直接编码；不调用 HLSL/GLSL 或第三方编译器。
use crate::ir::*;
use crate::builtin_types::{ScalarBinaryOp, ScalarCode};
use super::{PAGE_BYTES, PREFIX_LEVELS, SCAN_WIDTH, ProgramImage, ShaderCode, StageCode};
#[path = "append.rs"]
mod append;
#[path = "atomic.rs"]
mod atomic;
#[path = "compact.rs"]
mod compact;
#[path = "custom_reduce.rs"]
mod custom_reduce;
#[path = "prefix.rs"]
mod prefix;
#[path = "program.rs"]
pub(crate) mod program;
pub use program::compile_program;
#[path = "reduce.rs"]
mod reduce;
#[path = "returns.rs"]
mod returns;
#[path = "scan.rs"]
mod scan;
#[path = "segmented.rs"]
mod segmented;
#[path = "selection.rs"]
mod selection;
#[path = "sort.rs"]
mod sort;
#[path = "subgroup.rs"]
mod subgroup;
#[path = "texture.rs"]
mod texture;
#[path = "tile.rs"]
mod tile;
#[path = "tile_collective.rs"]
mod tile_collective;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    Main,
    ReductionTail,
    ScanTail,
    ScanAdd,
    ScanRootAdd,
    SegmentedSingleGroup,
    CompactScatter,
    SelectionArguments,
    SelectionScan,
    SelectionScatter,
    SortHistogram(u32),
    SortScatter(u32),
    SortKey(usize),
    SortScan,
    SortGather,
    DispatchArguments(PrefixCount),
}
use std::collections::{BTreeMap, BTreeSet};

#[repr(u32)]
enum EntryOperation {
    Map,
    Reduce,
    Scan,
    Segmented,
    Compact,
    Sort,
    CustomScan,
    CustomSegmented,
    CustomReduce,
}

impl<R> From<&Operation<R>> for EntryOperation {
    fn from(operation: &Operation<R>) -> Self {
        match operation {
            Operation::Map => Self::Map,
            Operation::Reduce(_) => Self::Reduce,
            Operation::ScanExclusive(_) => Self::Scan,
            Operation::SegmentedReduce(_) => Self::Segmented,
            Operation::Compact(_) => Self::Compact,
            Operation::StableSort(_) => Self::Sort,
            Operation::CustomScan(_) => Self::CustomScan,
            Operation::CustomSegmentedReduce(_) => Self::CustomSegmented,
            Operation::CustomReduce(_) => Self::CustomReduce,
        }
    }
}

#[repr(u32)]
enum WorkgroupPass {
    Main, ReductionTail, ScanTail, ScanRoot, SelectionScan, SortScan, SortHistogram, SortScatter, Other,
}

impl From<Pass> for WorkgroupPass {
    fn from(pass: Pass) -> Self {
        match pass {
            Pass::Main => Self::Main,
            Pass::ReductionTail => Self::ReductionTail,
            Pass::ScanTail => Self::ScanTail,
            Pass::ScanRootAdd | Pass::SegmentedSingleGroup => Self::ScanRoot,
            Pass::SelectionScan => Self::SelectionScan,
            Pass::SortScan => Self::SortScan,
            Pass::SortHistogram(_) => Self::SortHistogram,
            Pass::SortScatter(_) => Self::SortScatter,
            _ => Self::Other,
        }
    }
}

#[repr(C)]
struct WorkgroupLayout {
    scan: u32,
    local_index: u32,
    ranks: u32,
    words: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::workgroup_layout"]
    fn workgroup_layout(
        operation: EntryOperation, pass: WorkgroupPass, selected: u32, summary_words: u32, scan_width: u32,
    ) -> WorkgroupLayout;
}

#[repr(C)]
struct ShaderFeatures {
    atomic64: u32,
    float64: u32,
    integer8: u32,
    integer16: u32,
    preserve32: u32,
    preserve64: u32,
    subgroup_basic: u32,
    subgroup_vote: u32,
    subgroup_arithmetic: u32,
    subgroup_ballot: u32,
    subgroup_shuffle: u32,
    subgroup_shuffle_relative: u32,
    acceleration: u32,
    position_fetch: u32,
    descriptor_indexing: u32,
    maximal_reconvergence: u32,
}

#[repr(C)]
struct ShaderEnvelope {
    bound: u32,
    ext: u32,
    entry: u32,
    workgroup_width: u32,
    prepare: u32,
    features: ShaderFeatures,
}

#[repr(C)]
struct ShaderWords {
    data: *const u32,
    count: u64,
}

#[repr(u32)]
enum TypeShape {
    Void,
    Vector,
    Matrix,
    Array = 4,
    Function,
    Pointer = 9,
}

#[repr(C)]
struct ShaderResourceInput {
    buffers: usize,
    slots: usize,
    indirect: usize,
    textures: usize,
    accelerations: usize,
    next: u32,
    float_type: u32,
    word_type: u32,
    wide_type: u32,
    byte_type: u32,
    short_type: u32,
    uniform_bytes: u32,
    publishes_summary: u32,
    single_group: u32,
    argument_pass: u32,
    grid: u32,
}

#[repr(C)]
struct ResourceStreams {
    definitions: *mut u32,
    decorations: *mut u32,
    pages: *mut u32,
    textures: *mut u32,
    accelerations: *mut u32,
}

#[repr(C)]
struct TextureDeclaration {
    dimensions: u32,
    arrayed: u32,
    length: u32,
}

#[repr(C)]
struct ShaderResourceHost {
    textures: *const std::ffi::c_void,
    texture: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> TextureDeclaration,
    constants: *mut std::ffi::c_void,
    intern_word: unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32) -> u32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct SampledType {
    sampled: u32,
    pointer: u32,
}

#[repr(C)]
struct ShaderResourceIds {
    definitions: u64,
    decorations: u64,
    next: u32,
    vec3: u32,
    vec2: u32,
    vec4: u32,
    acceleration: u32,
    ray_pointer: u32,
    index_type: u32,
    global: u32,
    num_groups: u32,
    words32: u32,
    words64: u32,
    bytes: u32,
    shorts: u32,
    table_pointer: u32,
    push: u32,
    push_pointer: u32,
    sampled2: SampledType,
    sampled3: SampledType,
}

unsafe extern "C" fn texture_declaration(
    textures: *const std::ffi::c_void, index: usize,
) -> TextureDeclaration {
    let texture = unsafe { &*textures.cast::<TextureBinding>().add(index) };
    TextureDeclaration {
        dimensions: texture.dimension.components() as u32,
        arrayed: u32::from(texture.count.is_some()),
        length: texture.count.unwrap_or(0),
    }
}

unsafe extern "C" fn intern_resource_word(
    constants: *mut std::ffi::c_void, bits: u32, next: u32,
) -> u32 {
    *unsafe { &mut *constants.cast::<BTreeMap<(Scalar, Bits), u32>>() }.entry((Scalar::U32, Bits::from(bits))).or_insert(next)
}


#[repr(u32)]
enum StorageClass {
    Input = 1,
    Workgroup = 4,
    Function = 7,
}

#[repr(u32)]
enum VariableAnnotation {
    BuiltIn = 2,
}

#[repr(C)]
struct BinaryEmission {
    operation: ScalarBinaryOp,
    scalar: ScalarCode,
    result_type: u32,
    result: u32,
    left: u32,
    right: u32,
    mask: u32,
    masked: u32,
}

#[repr(C)]
struct BinaryWords {
    body: u64,
    decorations: u64,
}

#[repr(C)]
struct CastEmission {
    from: ScalarCode,
    to: ScalarCode,
    result_type: u32,
    result: u32,
    value: u32,
    zero: u32,
    one: u32,
}

#[repr(C)]
struct DivisionEmission {
    integer_type: u32,
    magnitude_type: u32,
    numerator: u32,
    denominator: u32,
    quotient: u32,
    remainder: u32,
    sign_shift: u32,
    next: u32,
}

#[repr(C)]
struct DivisionWords {
    body: u32,
    next: u32,
}

#[repr(C)]
#[derive(Default)]
struct FloatSignOperands {
    raw_type: u32,
    mask: u32,
    temporary: u32,
}

#[repr(C)]
struct UnaryEmission {
    operation: u32,
    result_type: u32,
    result: u32,
    value: u32,
    zero: u32,
    sign: FloatSignOperands,
}

#[repr(C)]
struct MathEmission {
    operation: u32,
    result_type: u32,
    result: u32,
    left: u32,
    right: u32,
    extended_set: u32,
    sign: FloatSignOperands,
}

enum PageSource {
    Dataset(u32),
    View { low: u32, high: u32, stride: u32 },
}

#[repr(C)]
struct PageAddress {
    u32_type: u32,
    u64_type: u32,
    descriptor_pointer: u32,
    page_pointer: u32,
    result_pointer: u32,
    descriptor: u32,
    low: u32,
    high: u32,
    stride: u32,
    index: u32,
    word: u32,
    extra: u32,
    zero: u32,
    one: u32,
    high_shift: u32,
    page_shift: u32,
    page_bytes: u32,
    next: u32,
}

#[repr(C)]
struct RecordCopy {
    source: PageAddress,
    target_descriptor: u32,
    unit_type: u32,
    unit_bytes: u32,
    count: u32,
    step: u32,
}

impl RecordCopy {
    fn word_capacity(&self) -> usize {
        // Two record origins and up to 29 words per scalar copy.
        182 + 29 * self.count as usize
    }
}

#[repr(C)]
struct PageAddressResult {
    words: u64,
    next: u32,
    pointer: u32,
    second: u32,
}

#[repr(u32)]
#[derive(Clone, Copy)]
enum PayloadAccess {
    Words = 0,
    Local = 1,
    Physical = 2,
}

#[repr(C)]
struct PayloadEmission {
    access: PayloadAccess,
    raw_type: u32,
    unit_type: u32,
    unit_bytes: u32,
    count: u32,
    next: u32,
}

#[repr(C)]
struct PackedField {
    value: u32,
    offset: u32,
    raw_type: u32,
    bytes: u32,
}

#[repr(C)]
struct PackingConstant {
    id: u32,
    next: u32,
}

type PackingContext = *mut std::ffi::c_void;

#[repr(C)]
struct PackingHost {
    context: PackingContext,
    field: unsafe extern "C" fn(PackingContext, *mut PackedField) -> u32,
    constant: unsafe extern "C" fn(PackingContext, u32, u32) -> PackingConstant,
}

struct PackingSource<'a, 'program, I> {
    emitter: &'a mut Emitter<'program>,
    components: I,
}

unsafe extern "C" fn packing_field<I: Iterator<Item = (Scalar, u32, u32)>>(
    context: PackingContext, field: *mut PackedField,
) -> u32 {
    let source = unsafe { &mut *context.cast::<PackingSource<'_, '_, I>>() };
    let Some((scalar, offset, value)) = source.components.next() else { return 0 };
    let component = PackedField { value, offset, raw_type: source.emitter.raw_type(scalar), bytes: scalar.size() };
    unsafe { field.write(component); }
    1
}

unsafe extern "C" fn packing_constant<I>(
    context: PackingContext, bits: u32, next: u32,
) -> PackingConstant {
    let emitter = &mut unsafe { &mut *context.cast::<PackingSource<'_, '_, I>>() }.emitter;
    emitter.next = next;
    let id = emitter.constant(Scalar::U32, bits);
    PackingConstant { id, next: emitter.next }
}

#[repr(C)]
struct PayloadRead {
    words: u64,
    next: u32,
    value: u32,
}

#[repr(C)]
struct PayloadWrite {
    words: u64,
    next: u32,
}

#[repr(C)]
struct PayloadResult {
    type_id: u32,
    id: u32,
    compare_zero: u32,
}

#[repr(C)]
struct UniformRead {
    next: u32,
    push_pointer: u32,
    push_block: u32,
    uint_type: u32,
    raw_type: u32,
    result: PayloadResult,
    zero: u32,
    low_word: u32,
    high_word: u32,
    byte_shift: u32,
    high_shift: u32,
}

#[repr(C)]
struct ControlWords {
    words: u64,
    label: u32,
    reaches_next: u32,
}

#[repr(u32)]
enum StructuredKind { Selection, Loop }

#[repr(u32)]
enum BlockFlow { Error, Stop, Next }

#[repr(C)]
struct InstructionBlock { data: *const std::ffi::c_void, count: usize }
impl From<&[Inst]> for InstructionBlock {
    fn from(code: &[Inst]) -> Self {
        Self { data: code.as_ptr().cast(), count: code.len() }
    }
}
impl InstructionBlock {
    unsafe fn original(&self) -> &[Inst] {
        unsafe { std::slice::from_raw_parts(self.data.cast(), self.count) }
    }
}

#[repr(C)]
struct ControlTargets { end: u32, next: u32 }
impl From<Option<(u32, u32)>> for ControlTargets {
    fn from(targets: Option<(u32, u32)>) -> Self {
        let (end, next) = targets.unwrap_or_default();
        Self { end, next }
    }
}

#[repr(C)]
struct StructuredControl {
    first: InstructionBlock,
    second: InstructionBlock,
    test: Reg,
    targets: ControlTargets,
    kind: StructuredKind,
}

#[allow(dead_code)] // B requests these two constants.
#[repr(u32)]
enum ControlConstant { Selector, Flag }

#[repr(C)]
struct ReturnTargets {
    function_merge: u32,
    loop_merge: u32,
    loop_edges: *mut std::ffi::c_void,
    label: u32,
}

#[repr(C)]
struct StructuredHost {
    context: *mut std::ffi::c_void,
    labels: unsafe extern "C" fn(*mut std::ffi::c_void, u32) -> u32,
    reserve_words: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> *mut u32,
    finish_words: unsafe extern "C" fn(*mut std::ffi::c_void, ControlWords),
    block: unsafe extern "C" fn(*mut std::ffi::c_void, InstructionBlock, ControlTargets) -> BlockFlow,
    value: unsafe extern "C" fn(*mut std::ffi::c_void, Reg) -> u32,
    begin_return: unsafe extern "C" fn(*mut std::ffi::c_void, InstructionBlock, u32) -> u32,
    normal_exit: unsafe extern "C" fn(*mut std::ffi::c_void, u32),
    finish_return: unsafe extern "C" fn(*mut std::ffi::c_void),
    constant: unsafe extern "C" fn(StructuredContext, ControlConstant, u32) -> u32,
    open_function: unsafe extern "C" fn(StructuredContext, u32),
    close_function: unsafe extern "C" fn(StructuredContext),
    exit_targets: unsafe extern "C" fn(StructuredContext) -> ReturnTargets,
    record_return: unsafe extern "C" fn(*mut std::ffi::c_void, u32),
}

struct StructuredEmission<'a, 'program> {
    emitter: &'a mut Emitter<'program>,
    error: Option<&'static str>,
}

impl StructuredEmission<'_, '_> {
    fn host(&mut self) -> StructuredHost {
        StructuredHost {
            context: std::ptr::from_mut(self).cast(),
            labels: structured_labels,
            reserve_words: structured_reserve,
            finish_words: structured_finish,
            block: structured_block,
            value: structured_value,
            begin_return: structured_begin_return,
            normal_exit: structured_normal_exit,
            finish_return: structured_finish_return,
            constant: returns::structured_constant,
            open_function: returns::structured_open_function,
            close_function: returns::structured_close_function,
            exit_targets: returns::structured_exit_targets,
            record_return: returns::structured_record_return,
        }
    }
}

type StructuredContext = *mut std::ffi::c_void;
unsafe extern "C" fn structured_labels(context: StructuredContext, count: u32) -> u32 {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    let first = emitter.next;
    emitter.next += count;
    first
}
unsafe extern "C" fn structured_reserve(context: StructuredContext, capacity: usize) -> *mut u32 {
    unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter.reserve_control(capacity)
}
unsafe extern "C" fn structured_finish(context: StructuredContext, emitted: ControlWords) {
    unsafe { (&mut *context.cast::<StructuredEmission<'_, '_>>()).emitter.finish_control(emitted); }
}
unsafe extern "C" fn structured_block(
    context: StructuredContext, code: InstructionBlock, targets: ControlTargets,
) -> BlockFlow {
    let emission = unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() };
    let targets = (targets.end != 0).then_some((targets.end, targets.next));
    match emission.emitter.block(unsafe { code.original() }, targets) {
        Ok(true) => BlockFlow::Next,
        Ok(false) => BlockFlow::Stop,
        Err(error) => { emission.error = Some(error); BlockFlow::Error }
    }
}
unsafe extern "C" fn structured_value(context: StructuredContext, register: Reg) -> u32 {
    unsafe { &*context.cast::<StructuredEmission<'_, '_>>() }.emitter.value(register)
}
unsafe extern "C" fn structured_begin_return(context: StructuredContext, body: InstructionBlock, merge: u32) -> u32 {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    u32::from(emitter.begin_return_loop(unsafe { body.original() }, merge))
}
unsafe extern "C" fn structured_normal_exit(context: StructuredContext, end: u32) {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    emitter.loop_exit_edge(end, emitter.current_label);
}
unsafe extern "C" fn structured_finish_return(context: StructuredContext) {
    unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter.finish_return_loop();
}

#[repr(C)]
struct SubgroupEmission {
    operation: SubgroupOp,
    scalar: ScalarCode,
    result_type: u32,
    result: u32,
    scope_or_pointer: u32,
    value: u32,
    argument: u32,
    ballot_type: u32,
    next: u32,
}

#[repr(C)]
struct SubgroupWords {
    words: u64,
    next: u32,
}

#[repr(u32)]
enum FlagDecoration {
    NoContraction = 42,
}

#[repr(C)]
struct TextureSampleEmission {
    variable: u32,
    element: u32,
    index_type: u32,
    sampled_type: u32,
    sampled_pointer: u32,
    coordinate_type: u32,
    vector_type: u32,
    component_type: u32,
    x: u32,
    y: u32,
    z: u32,
    lod: u32,
    next: u32,
}

#[repr(C)]
struct TextureWords {
    body: u64,
    decorations: u64,
    next: u32,
}

#[repr(u32)]
#[derive(Clone, Copy)]
enum AtomicOperation {
    Load, Store, Add, Sub, Exchange, CompareExchange, Min, Max, And, Or, Xor,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AtomicEmission {
    operation: AtomicOperation,
    scalar: ScalarCode,
    result_type: u32,
    result: u32,
    pointer: u32,
    scope: u32,
    semantics: u32,
    value: u32,
    compare: u32,
    failure: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct DispatchScalars {
    uint_type: u32,
    bool_type: u32,
    zero: u32,
    one: u32,
}

#[repr(C)]
struct SortRankEmission {
    source: PageAddress,
    totals: PageAddress,
    target: PageAddress,
    scalars: DispatchScalars,
    histogram: u32,
    count: u32,
    index: u32,
    label: u32,
    wave_input: u32,
    lane_input: u32,
    ballot_type: u32,
    shared: u32,
    shared_pointer: u32,
    shift: u32,
    width: u32,
    wave_width: u32,
    bins: u32,
    mask: u32,
    bit_masks: [u32; 4],
    subgroup_scope: u32,
    workgroup_scope: u32,
    shared_semantics: u32,
    pair_step: u32,
    next: u32,
}

#[repr(C)]
struct StageWords { words: u64, next: u32, label: u32 }

#[repr(C)]
struct DispatchValue {
    words: u64,
    next: u32,
    value: u32,
}

#[repr(C)]
struct DispatchGrid {
    scalars: DispatchScalars,
    wide_type: u32,
    table_pointer: u32,
    page_pointer: u32,
    word_pointer: u32,
    descriptor: u32,
    count: u32,
    width: u32,
    max_x: u32,
    max_y: u32,
    capacity: u32,
    collective: u32,
    bins: u32,
    first_byte: u32,
    histogram_byte: u32,
    width_shift: u32,
    record_bytes: u32,
    field_bytes: [u32; 3],
    next: u32,
}

impl DispatchGrid {
    fn word_capacity(&self) -> usize {
        // First record, optional histogram setup and one loop body for the hierarchy.
        25 + 167 + (56 + 167) * usize::from(self.bins != 0)
            + (96 + 167) * usize::from(self.capacity != 0)
    }
}

#[repr(C)]
struct DispatchPush {
    uint_type: u32,
    pointer_type: u32,
    block: u32,
    zero: u32,
}

#[repr(C)]
struct PrefixIndexEmission {
    uint_type: u32,
    wide_type: u32,
    bool_type: u32,
    vector_type: u32,
    global: u32,
    global_x: u32,
    num_groups: u32,
    count_pointer: u32,
    width: u32,
    wide_width: u32,
    zero: u32,
    one: u32,
    collective: u32,
    next: u32,
}

#[repr(C)]
struct PrefixIndexWords {
    words: u64,
    next: u32,
    index: u32,
    count: u32,
    active: u32,
    label: u32,
    groups_x: u32,
    groups_y: u32,
}

#[repr(C)]
struct AppendEmission {
    uint_type: u32,
    bool_type: u32,
    ballot_type: u32,
    device_scope: u32,
    relaxed: u32,
    subgroup_scope: u32,
    active: u32,
    counter: u32,
    push_pointer_type: u32,
    push: u32,
    capacity_word: u32,
    outer: u32,
    accepted: u32,
    next: u32,
}

#[repr(C)]
struct AppendWords {
    words: u64,
    next: u32,
    ticket: u32,
    write_label: u32,
    finish: u32,
}

#[repr(C)]
struct PrivateAddress {
    index_type: u32,
    pointer_type: u32,
    variable: u32,
    index: u32,
    offset: u32,
    width: u32,
    count: u32,
    next: u32,
}

#[repr(C)]
struct PrivateAddressWords {
    words: u64,
    next: u32,
}

#[repr(C)]
struct RayEmission {
    acceleration_type: u32,
    acceleration: u32,
    query: u32,
    u32_type: u32,
    f32_type: u32,
    bool_type: u32,
    vec2_type: u32,
    vec3_type: u32,
    origin: [u32; 3],
    direction: [u32; 3],
    minimum: u32,
    maximum: u32,
    flags: u32,
    mask: u32,
    committed: u32,
    zero: u32,
    float_zero: u32,
    position_type: u32,
    object_to_world: u32,
    world_to_object: u32,
    closest: u32,
    next: u32,
}

#[repr(C)]
struct RayWords {
    words: u64,
    next: u32,
    label: u32,
}

#[repr(C)]
struct TileAddress {
    index_type: u32,
    pointer_type: u32,
    shared: u32,
    index: u32,
    offset: u32,
    bias: u32,
    cell_bytes: u32,
    width: u32,
    base: u32,
    next: u32,
}

#[repr(C)]
struct TileAddressWords {
    words: u64,
    next: u32,
    pointer: u32,
    byte: u32,
}

#[repr(C)]
struct TileExtract {
    index_type: u32,
    cell_type: u32,
    pointer: u32,
    byte: u32,
    cell_bytes: u32,
    bits_per_byte: u32,
    shift: u32,
    next: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::tile_address"]
    fn emit_tile_address(input: TileAddress, body: *mut u32) -> TileAddressWords;
    #[link_name = "bootstrap.spirv_envelope::tile_extract"]
    fn emit_tile_extract(input: TileExtract, body: *mut u32) -> PayloadRead;
    #[link_name = "bootstrap.spirv_envelope::encode_ray"]
    fn encode_ray(input: RayEmission, results: *const u32, body: *mut u32) -> RayWords;
    #[link_name = "bootstrap.spirv_envelope::private_address"]
    fn emit_private_address(
        input: PrivateAddress, parts: *const u32, pointers: *mut u32, body: *mut u32,
    ) -> PrivateAddressWords;
    #[link_name = "bootstrap.spirv_envelope::workgroup_barrier"]
    fn encode_workgroup_barrier(body: *mut u32, at: u64, scope: u32, semantics: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::encode_sort_rank"]
    fn encode_sort_rank(input: SortRankEmission, body: *mut u32) -> StageWords;
    #[link_name = "bootstrap.spirv_envelope::encode_dispatch_grid"]
    fn encode_dispatch_grid(input: DispatchGrid, body: *mut u32) -> DispatchValue;
    #[link_name = "bootstrap.spirv_envelope::read_dispatch_word"]
    fn read_dispatch_word(push: DispatchPush, word: u32, first: u32, body: *mut u32) -> PayloadRead;
    #[link_name = "bootstrap.spirv_envelope::encode_linear_index"]
    fn encode_linear_index(
        push: DispatchPush, wide_type: u32, bool_type: u32, local: u32, one: u32, first: u32,
        body: *mut u32,
    ) -> PrefixIndexWords;
    #[link_name = "bootstrap.spirv_envelope::encode_prefix_index"]
    fn encode_prefix_index(input: PrefixIndexEmission, body: *mut u32) -> PrefixIndexWords;
    #[link_name = "bootstrap.spirv_envelope::encode_append_reservation"]
    fn encode_append_reservation(input: AppendEmission, body: *mut u32) -> AppendWords;
    #[link_name = "bootstrap.spirv_envelope::encode_atomic"]
    fn encode_atomic(input: AtomicEmission, body: *mut u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::flag_decoration"]
    fn flag_decoration(decorations: *mut u32, at: u64, decoration: FlagDecoration, id: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::encode_texture_sample"]
    fn encode_texture_sample(
        input: TextureSampleEmission, results: *const u32, body: *mut u32, decorations: *mut u32,
    ) -> TextureWords;
    #[link_name = "bootstrap.spirv_envelope::envelope_capacity"]
    fn envelope_capacity() -> u64;
    #[link_name = "bootstrap.spirv_envelope::encode"]
    fn encode_envelope(
        input: ShaderEnvelope, interfaces: *const ShaderWords, count: u64, words: *mut u32,
    ) -> u64;
    #[link_name = "bootstrap.spirv_envelope::type_declaration"]
    fn type_declaration(shape: TypeShape, id: u32, first: u32, second: u32, words: *mut u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::shader_resources"]
    fn shader_resources(input: ShaderResourceInput, host: ShaderResourceHost, streams: ResourceStreams) -> ShaderResourceIds;
    #[link_name = "bootstrap.spirv_envelope::variable_declaration"]
    fn variable_declaration(storage: StorageClass, pointer: u32, id: u32, words: *mut u32);
    #[link_name = "bootstrap.spirv_envelope::annotated_variable"]
    fn annotated_variable(
        annotation: VariableAnnotation, pointer: u32, id: u32, operand: u32,
        definitions: *mut u32, decorations: *mut u32,
    ) -> u64;
    #[link_name = "bootstrap.spirv_envelope::scalar_binary"]
    fn scalar_binary(input: BinaryEmission, body: *mut u32, decorations: *mut u32) -> BinaryWords;
    #[link_name = "bootstrap.spirv_envelope::scalar_cast"]
    fn scalar_cast(input: CastEmission, body: *mut u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::select_words"]
    fn select_words(body: *mut u32, at: u64, result_type: u32, result: u32,
                    condition: u32, yes: u32, no: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::integer_division"]
    fn integer_division(input: DivisionEmission, body: *mut u32) -> DivisionWords;
    #[link_name = "bootstrap.spirv_envelope::scalar_unary"]
    fn scalar_unary(input: UnaryEmission, body: *mut u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::scalar_math"]
    fn scalar_math(input: MathEmission, body: *mut u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::page_address"]
    fn page_address(input: PageAddress, step: u32, body: *mut u32) -> PageAddressResult;
    #[link_name = "bootstrap.spirv_envelope::summary_addresses"]
    fn emit_summary_addresses(input: PageAddress, step: u32, pointers: *mut u32, count: u32,
        body: *mut u32) -> PayloadWrite;
    #[link_name = "bootstrap.spirv_envelope::read_summary_words"]
    fn read_summary_words(body: *mut u32, type_id: u32, first: u32, pointers: *const u32, count: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::write_summary_words"]
    fn write_summary_words(body: *mut u32, pointers: *const u32, values: *const u32, count: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::read_uniform"]
    fn read_uniform(input: UniformRead, body: *mut u32) -> PayloadWrite;
    #[link_name = "bootstrap.spirv_envelope::payload_read"]
    fn payload_read(input: PayloadEmission, operands: *const u32, shifts: *const u32, body: *mut u32) -> PayloadRead;
    #[link_name = "bootstrap.spirv_envelope::typed_payload_read"]
    fn typed_payload_read(
        input: PayloadEmission, result: PayloadResult, operands: *const u32, shifts: *const u32,
        body: *mut u32,
    ) -> PayloadWrite;
    #[link_name = "bootstrap.spirv_envelope::payload_write_bits"]
    fn payload_write_bits(type_id: u32, result: u32, value: u32, one: u32, zero: u32, body: *mut u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::payload_write"]
    fn payload_write(
        input: PayloadEmission, value: u32, pointers: *const u32, shifts: *const u32,
        output: *mut u32, body: *mut u32,
    ) -> PayloadWrite;
    #[link_name = "bootstrap.spirv_envelope::pack_record"]
    fn pack_record(host: PackingHost, word_type: u32, zero: u32, next: u32,
        packed: *mut u32, body: *mut u32) -> PayloadWrite;
    #[link_name = "bootstrap.spirv_envelope::begin_shader_body"]
    fn begin_shader_body(
        result_type: u32, function: u32, signature: u32, entry: u32, body: *mut u32,
    ) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::end_shader_body"]
    fn end_shader_body(body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::phi_two"]
    fn phi_two(
        body: *mut u32, at: u64, ty: u32, result: u32, first_value: u32, first_block: u32,
        second_value: u32, second_block: u32,
    ) -> u64;
    #[link_name = "bootstrap.spirv_envelope::open_selection"]
    fn open_selection(merge: u32, condition: u32, yes: u32, no: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::open_index_selection"]
    fn open_index_selection(bool_type: u32, test: u32, index: u32, bound: u32,
        less: bool, run: u32, end: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::open_tile_padding"]
    fn open_tile_padding(uint_type: u32, bool_type: u32, lane: u32, width: u32,
        capacity: u32, index: u32, test: u32, run: u32, end: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::copy_shader_value"]
    fn copy_shader_value(body: *mut u32, at: u64, type_id: u32, result: u32, value: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::plain_load"]
    fn plain_load(body: *mut u32, at: u64, type_id: u32, result: u32, pointer: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::plain_store"]
    fn plain_store(body: *mut u32, at: u64, pointer: u32, value: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::aligned_load"]
    fn aligned_load(body: *mut u32, at: u64, type_id: u32, result: u32, pointer: u32, alignment: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::read_shader_index"]
    fn read_shader_index(vector_type: u32, uint_type: u32, input: u32, global: u32, x: u32, body: *mut u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::unpack_summary_word"]
    fn unpack_summary_word(type_id: u32, result: u32, word: u32, shift: u32, body: *mut u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::declare_dispatch_continuation"]
    fn declare_dispatch_continuation(type_id: u32, result: u32, spec_id: u32, bytes: u32,
        definitions: *mut u32, decorations: *mut u32);
    #[link_name = "bootstrap.spirv_envelope::dispatch_count_index"]
    fn dispatch_count_index(push: DispatchPush, word: u32, field: u32, first: u32, body: *mut u32) -> PayloadRead;
    #[link_name = "bootstrap.spirv_envelope::next_control_block"]
    fn next_control_block(target: u32, label: u32, reaches: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::lower_structured"]
    fn lower_structured(input: StructuredControl, host: StructuredHost) -> BlockFlow;
    #[link_name = "bootstrap.spirv_envelope::lower_function_scope"]
    fn lower_function_scope(body: InstructionBlock, host: StructuredHost);
    #[link_name = "bootstrap.spirv_envelope::lower_function_exit"]
    fn lower_function_exit(host: StructuredHost) -> u32;
    #[link_name = "bootstrap.spirv_envelope::lower_return_loop"]
    fn lower_return_loop(edges: *const returns::ReturnEdge, count: u32, bool_type: u32, host: StructuredHost);
    #[link_name = "bootstrap.spirv_envelope::leave_control"]
    fn leave_control(target: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::encode_subgroup"]
    fn encode_subgroup(input: SubgroupEmission, body: *mut u32) -> SubgroupWords;
    #[link_name = "bootstrap.spirv_envelope::scalar_declaration"]
    fn scalar_declaration(scalar: ScalarCode, type_id: u32, pointer_id: u32, words: *mut u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::constant_declaration"]
    fn constant_declaration(scalar: ScalarCode, type_id: u32, id: u32, bits: u64, words: *mut u32) -> u64;
}

struct PrivateArray {
    start: usize,
    unit: Scalar,
    variable: u32,
}

struct Emitter<'a> {
    next: u32,
    types: BTreeMap<Scalar, u32>,
    pointers: BTreeMap<Scalar, u32>,
    constants: BTreeMap<(Scalar, Bits), u32>,
    defs: Vec<u32>,
    decorations: Vec<u32>,
    body: Vec<u32>,
    buffers: Vec<u32>,
    accelerations: Vec<u32>,
    textures: Vec<u32>,
    texture_types: [SampledType; 2],
    vec4_type: u32,
    acceleration_type: u32,
    ray_variables: BTreeMap<Reg, (u32, crate::ir::dataflow::RayFields)>,
    ray_position_type: u32,
    ray_transform_type: u32,
    vec3_type: u32,
    vec2_type: u32,
    push: u32,
    ptr_push_u32: u32,
    physical_pointers: BTreeMap<Scalar, u32>,
    ptr_table_u64: u32,
    u64_type: u32,
    index: u32,
    regs: Vec<u32>,
    locals: Vec<u32>,
    private_arrays: BTreeMap<usize, PrivateArray>,
    stage: &'a Stage,
    program: &'a Program,
    execution_buffers: ExecutionBuffers,
    ext: u32,
    subgroup_inputs: BTreeMap<SubgroupOp, u32>,
    ballot_type: u32,
    workgroup_width: u32,
    tile_layout: &'a crate::ir::dataflow::TileLayout,
    tile_shared64: u32,
    tile_pointer64: u32,
    tile_shared: u32,
    tile_pointer: u32,
    tile_length: u32,
    tile_lane: u32,
    current_label: u32,
    return_scopes: Vec<returns::ReturnScope>,
}
impl Emitter<'_> {
    fn private_address(
        &mut self, base: usize, index: Option<Reg>, word: u32, scalar: Scalar,
    ) -> (Scalar, [u32; 8], usize) {
        use std::ops::Bound::{Excluded, Unbounded};
        let (&end, array) = self.private_arrays.range((Excluded(base), Unbounded)).next().unwrap();
        let offset = self.stage.locals[&end].components[base - array.start].1 + word;
        let unit = array.unit;
        let variable = array.variable;
        let bytes = unit.size();
        let count = scalar.size().div_ceil(bytes) as usize;
        let mut parts = [0; 8];
        let (index, offset, width) = if let Some(index) = index {
            let offset = if offset == 0 { 0 } else { self.constant(Scalar::U32, offset) };
            let width = if bytes == 1 { 0 } else { self.constant(Scalar::U32, bytes) };
            for (part, slot) in parts[..count].iter_mut().enumerate().skip(1) {
                *slot = self.constant(Scalar::U32, part as u32);
            }
            (self.value(index), offset, width)
        } else {
            let first = offset / bytes;
            for (part, slot) in parts[..count].iter_mut().enumerate() {
                *slot = self.constant(Scalar::U32, first + part as u32);
            }
            (0, 0, 0)
        };
        let input = PrivateAddress {
            index_type: self.types[&Scalar::U32], pointer_type: self.pointers[&unit], variable,
            index, offset, width, count: count as u32, next: self.next,
        };
        let mut pointers = [0; 8];
        let at = self.body.len();
        self.body.reserve(10 * count + 5);
        unsafe {
            let emitted = emit_private_address(input, parts.as_ptr(), pointers.as_mut_ptr(), self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            self.next = emitted.next;
        }
        (unit, pointers, count)
    }

    fn load_private(&mut self, dst: Reg, base: usize, index: Option<Reg>, word: u32) {
        let scalar = self.stage.registers[dst];
        let (unit, pointers, count) = self.private_address(base, index, word, scalar);
        let raw = self.load_raw(scalar, unit, &pointers[..count], false);
        self.from_words(dst, &[raw], Self::raw_scalar(scalar));
    }

    fn store_private(&mut self, src: Reg, base: usize, index: Option<Reg>, word: u32) {
        let scalar = self.stage.registers[src];
        let (unit, pointers, count) = self.private_address(base, index, word, scalar);
        let raw = self.raw_bits(src);
        self.store_raw(scalar, unit, &pointers[..count], raw, false);
    }

    fn id(&mut self) -> u32 {
        let v = self.next;
        self.next += 1;
        v
    }
    fn constant(&mut self, s: Scalar, bits: u32) -> u32 {
        self.constant_bits(s, Bits::from(bits))
    }
    fn constant_bits(&mut self, s: Scalar, bits: Bits) -> u32 {
        let bits = s.truncate(bits);
        if let Some(id) = self.constants.get(&(s, bits)) {
            return *id;
        }
        let id = self.id();
        let start = self.defs.len();
        self.defs.reserve(5);
        unsafe {
            let used = constant_declaration(ScalarCode::from(s), self.types[&s], id, bits, self.defs.as_mut_ptr().add(start));
            self.defs.set_len(start + used as usize);
        }
        self.constants.insert((s, bits), id);
        id
    }
    fn declare_type(&mut self, shape: TypeShape, first: u32, second: u32) -> u32 {
        let id = self.id();
        let at = self.defs.len();
        self.defs.reserve(9);
        unsafe {
            let used = type_declaration(shape, id, first, second, self.defs.as_mut_ptr().add(at));
            self.defs.set_len(at + used as usize);
        }
        id
    }
    fn declare_pointer(&mut self, storage: StorageClass, pointee: u32) -> u32 {
        self.declare_type(TypeShape::Pointer, storage as u32, pointee)
    }
    fn append_variable(words: &mut Vec<u32>, storage: StorageClass, pointer: u32, id: u32) {
        let start = words.len();
        words.reserve(4);
        unsafe {
            variable_declaration(storage, pointer, id, words.as_mut_ptr().add(start));
            words.set_len(start + 4);
        }
    }
    fn declare_variable(&mut self, storage: StorageClass, pointer: u32) -> u32 {
        let id = self.id();
        Self::append_variable(&mut self.defs, storage, pointer, id);
        id
    }
    fn declare_function_variable(&mut self, pointer: u32) -> u32 {
        let id = self.id();
        Self::append_variable(&mut self.body, StorageClass::Function, pointer, id);
        id
    }
    fn declare_annotated(&mut self, annotation: VariableAnnotation, pointer: u32, operand: u32) -> u32 {
        let id = self.id();
        let defs = self.defs.len();
        let decorations = self.decorations.len();
        self.defs.reserve(4);
        self.decorations.reserve(11);
        unsafe {
            let used = annotated_variable(annotation, pointer, id, operand,
                self.defs.as_mut_ptr().add(defs), self.decorations.as_mut_ptr().add(decorations));
            self.defs.set_len(defs + 4);
            self.decorations.set_len(decorations + used as usize);
        }
        id
    }
    fn decorate_flag(&mut self, decoration: FlagDecoration, id: u32) {
        let start = self.decorations.len();
        self.decorations.reserve(3);
        unsafe {
            let words = flag_decoration(self.decorations.as_mut_ptr().add(start), 0, decoration, id);
            self.decorations.set_len(start + words as usize);
        }
    }
    fn declare_shader_resources(
        &mut self, publishes_summary: bool, single_group: bool, argument_pass: bool,
    ) -> ShaderResourceIds {
        let program = self.program;
        let buffers = program.buffers.len();
        let slots = buffers + self.execution_buffers.auxiliary;
        let textures = program.textures.len();
        let accelerations = program.accelerations.len();
        let input = ShaderResourceInput {
            buffers, slots, indirect: self.execution_buffers.indirect, textures, accelerations,
            next: self.next,
            float_type: self.types[&Scalar::F32],
            word_type: self.types[&Scalar::U32],
            wide_type: self.u64_type,
            byte_type: self.types.get(&Scalar::U8).copied().unwrap_or(0),
            short_type: self.types.get(&Scalar::U16).copied().unwrap_or(0),
            uniform_bytes: program.uniform_bytes,
            publishes_summary: u32::from(publishes_summary),
            single_group: u32::from(single_group),
            argument_pass: u32::from(argument_pass),
            grid: u32::from(self.stage.prefix.is_some() || self.stage.selection.is_some()),
        };
        let definitions = self.defs.len();
        let decorations = self.decorations.len();
        // The fixed types and built-ins plus each resource's declarations and annotations.
        self.defs.reserve(96 + 32 * textures + 4 * (accelerations + slots));
        self.decorations.reserve(32 + 8 * (textures + accelerations) + 11 * slots);
        self.buffers.reserve(slots);
        self.accelerations.reserve(accelerations);
        self.textures.reserve(textures);
        let streams = ResourceStreams {
            definitions: unsafe { self.defs.as_mut_ptr().add(definitions) },
            decorations: unsafe { self.decorations.as_mut_ptr().add(decorations) },
            pages: self.buffers.as_mut_ptr(),
            textures: self.textures.as_mut_ptr(),
            accelerations: self.accelerations.as_mut_ptr(),
        };
        let host = ShaderResourceHost {
            textures: program.textures.as_ptr().cast(),
            texture: texture_declaration,
            constants: std::ptr::from_mut(&mut self.constants).cast(),
            intern_word: intern_resource_word,
        };
        let ids = unsafe { shader_resources(input, host, streams) };
        unsafe {
            self.defs.set_len(definitions + ids.definitions as usize);
            self.decorations.set_len(decorations + ids.decorations as usize);
            self.buffers.set_len(slots);
            self.textures.set_len(textures);
            self.accelerations.set_len(accelerations);
        }
        self.next = ids.next;
        self.vec3_type = ids.vec3;
        self.vec2_type = ids.vec2;
        self.vec4_type = ids.vec4;
        self.texture_types = [ids.sampled2, ids.sampled3];
        self.acceleration_type = ids.acceleration;
        self.ptr_table_u64 = ids.table_pointer;
        self.push = ids.push;
        self.ptr_push_u32 = ids.push_pointer;
        for (scalar, pointer) in [(Scalar::U32, ids.words32), (Scalar::U64, ids.words64),
                                  (Scalar::U8, ids.bytes), (Scalar::U16, ids.shorts)] {
            if pointer != 0 {
                self.physical_pointers.insert(scalar, pointer);
            }
        }
        ids
    }
    fn emit_body(&mut self, capacity: usize, encode: impl FnOnce(*mut u32) -> u64) {
        let start = self.body.len();
        self.body.reserve(capacity);
        unsafe {
            let words = encode(self.body.as_mut_ptr().add(start));
            self.body.set_len(start + words as usize);
        }
    }
    fn copy_register(&mut self, dst: Reg, source: u32) {
        let (ty, dst) = (self.ty(dst), self.value(dst));
        self.emit_body(4, |body| unsafe { copy_shader_value(body, 0, ty, dst, source) });
    }
    fn read_pointer(&mut self, ty: u32, result: u32, pointer: u32) {
        self.emit_body(4, |body| unsafe { plain_load(body, 0, ty, result, pointer) });
    }
    fn load_pointer(&mut self, ty: u32, pointer: u32) -> u32 {
        let result = self.id();
        self.read_pointer(ty, result, pointer);
        result
    }
    fn load_aligned_word(&mut self, pointer: u32) -> u32 {
        let ty = self.types[&Scalar::U32];
        let result = self.id();
        self.emit_body(6, |body| unsafe { aligned_load(body, 0, ty, result, pointer, 4) });
        result
    }
    fn store_pointer(&mut self, pointer: u32, value: u32) {
        self.emit_body(3, |body| unsafe { plain_store(body, 0, pointer, value) });
    }
    fn read_summary_words(&mut self, pointers: &[u32]) -> Vec<u32> {
        let at = self.body.len();
        self.body.reserve(6 * pointers.len());
        let first = self.next;
        self.next += pointers.len() as u32;
        unsafe {
            let words = read_summary_words(self.body.as_mut_ptr().add(at),
                self.types[&Scalar::U32], first, pointers.as_ptr(), pointers.len() as u32);
            self.body.set_len(at + words as usize);
        }
        (first..self.next).collect()
    }

    fn write_summary_words(&mut self, pointers: &[u32], values: &[u32]) {
        let count = pointers.len().min(values.len());
        let at = self.body.len();
        self.body.reserve(5 * count);
        unsafe {
            let words = write_summary_words(self.body.as_mut_ptr().add(at),
                pointers.as_ptr(), values.as_ptr(), count as u32);
            self.body.set_len(at + words as usize);
        }
    }

    fn workgroup_barrier(&mut self, memory_semantics: u32) {
        let scope = self.constant(Scalar::U32, 2);
        let semantics = self.constant(Scalar::U32, memory_semantics);
        let at = self.body.len();
        self.body.reserve(4);
        unsafe {
            let end = encode_workgroup_barrier(self.body.as_mut_ptr(), at as u64, scope, semantics);
            self.body.set_len(end as usize);
        }
    }
    fn select(&mut self, result_type: u32, result: u32, condition: u32, yes: u32, no: u32) {
        let at = self.body.len();
        self.body.reserve(6);
        unsafe {
            let words = select_words(self.body.as_mut_ptr().add(at), 0, result_type, result, condition, yes, no);
            self.body.set_len(at + words as usize);
        }
    }
    fn value(&self, r: Reg) -> u32 {
        self.regs[r]
    }
    fn ty(&self, r: Reg) -> u32 {
        self.types[&self.stage.registers[r]]
    }
    fn integer_divide(&mut self, result: IntegerDivisionResult, src: Reg, divisor: IntegerDivisor) {
        let scalar = self.stage.registers[src];
        let denominator = match divisor {
            IntegerDivisor::Constant(divisor) => self.constant_bits(scalar, divisor.get()),
            IntegerDivisor::Value(divisor) => self.value(divisor),
        };
        let sign_shift = if scalar.is_signed() { self.constant(scalar, scalar.bit_width() - 1) } else { 0 };
        let input = DivisionEmission {
            integer_type: self.types[&scalar],
            magnitude_type: self.types[&Self::raw_scalar(scalar)],
            numerator: self.value(src), denominator,
            quotient: result.quotient().map_or(0, |r| self.value(r)),
            remainder: result.remainder().map_or(0, |r| self.value(r)),
            sign_shift, next: self.next,
        };
        let at = self.body.len();
        self.body.reserve(if sign_shift == 0 { 10 } else { 81 });
        unsafe {
            let emitted = integer_division(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.body as usize);
            self.next = emitted.next;
        }
    }
    fn address_scalar(&mut self, unit: Scalar, binding: usize, index: u32, word: u32) -> u32 {
        self.page_address(unit, PageSource::Dataset(self.buffers[binding]), index, word, 0)
    }

    fn summary_addresses(&mut self, count: usize, binding: usize, index: u32) -> Vec<u32> {
        if count == 0 {
            return Vec::new();
        }
        let step = if count > 1 { self.constant(Scalar::U64, 4) } else { 0 };
        let input = self.page_input(Scalar::U32, PageSource::Dataset(self.buffers[binding]), index, Some(0), 0);
        let at = self.body.len();
        self.body.reserve(109 + 9 * (count - 1));
        let mut pointers = Vec::with_capacity(count);
        unsafe {
            let emitted = emit_summary_addresses(input, step, pointers.as_mut_ptr(),
                count as u32, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            self.next = emitted.next;
            pointers.set_len(count);
        }
        pointers
    }

    fn page_address(&mut self, unit: Scalar, source: PageSource, index: u32, word: u32, extra: u32) -> u32 {
        self.paged_pointers(unit, source, index, word, extra, false)[0]
    }

    fn paged_pointers(
        &mut self, unit: Scalar, source: PageSource, index: u32, word: u32, extra: u32, pair: bool,
    ) -> [u32; 2] {
        let step = if pair { self.constant(Scalar::U64, unit.size()) } else { 0 };
        let input = self.page_input(unit, source, index, Some(word), extra);
        let at = self.body.len();
        self.body.reserve(if pair { 118 } else { 109 });
        unsafe {
            let address = page_address(input, step, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + address.words as usize);
            self.next = address.next;
            [address.pointer, address.second]
        }
    }

    fn page_input(
        &mut self, unit: Scalar, source: PageSource, index: u32, word: Option<u32>, extra: u32,
    ) -> PageAddress {
        let (descriptor, low, high, stride, zero, one) = match source {
            PageSource::Dataset(descriptor) => (
                descriptor, 0, 0, 0, self.constant(Scalar::U32, 0), self.constant(Scalar::U32, 1),
            ),
            PageSource::View { low, high, stride } => (0, low, high, stride, 0, 0),
        };
        PageAddress {
            u32_type: self.types[&Scalar::U32],
            u64_type: self.u64_type,
            descriptor_pointer: self.ptr_table_u64,
            page_pointer: self.physical_pointers[&Scalar::U64],
            result_pointer: self.physical_pointers[&unit],
            descriptor, low, high, stride, index, extra, zero, one,
            high_shift: self.constant(Scalar::U32, 32),
            page_shift: self.constant(Scalar::U32, 3),
            page_bytes: self.constant(Scalar::U32, PAGE_BYTES),
            word: word.map_or(0, |word| self.constant(Scalar::U32, word)),
            next: self.next,
        }
    }
    // 记录页不拆分记录。两端沿原物理地址与对齐复制全部字节。
    fn record_copy_input(
        &mut self,
        source_binding: usize,
        target_binding: usize,
    ) -> Option<RecordCopy> {
        let stride = self.program.buffers[source_binding].stride;
        if stride == 0 {
            return None;
        }
        let align = self.program.buffers[source_binding].ty.align;
        let scalar = match align { 1 => Scalar::U8, 2 => Scalar::U16, _ => Scalar::U32 };
        let unit_bytes = scalar.size();
        let count = stride / unit_bytes;
        let step = if count > 1 { self.constant(Scalar::U64, unit_bytes) } else { 0 };
        let source = self.page_input(
            scalar, PageSource::Dataset(self.buffers[source_binding]), self.index, None, 0,
        );
        Some(RecordCopy {
            source,
            target_descriptor: self.buffers[target_binding],
            unit_type: self.types[&scalar],
            unit_bytes,
            count,
            step,
        })
    }

    fn emit_stage(&mut self, capacity: usize, encode: impl FnOnce(*mut u32) -> StageWords) {
        let at = self.body.len();
        self.body.reserve(capacity);
        unsafe {
            let emitted = encode(self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            self.next = emitted.next;
            self.current_label = emitted.label;
        }
    }

    fn view_source(&mut self, table: &[Reg; 2], stride: ElementStride) -> PageSource {
        let stride = match stride {
            ElementStride::Constant(bytes) => self.constant(Scalar::U32, bytes as u32),
            ElementStride::Value(parts) => self.value(parts[0]),
        };
        PageSource::View { low: self.value(table[0]), high: self.value(table[1]), stride }
    }

    fn indexed_page(&mut self, source: &IndexedSource) -> (PageSource, u32) {
        match source {
            IndexedSource::Data { binding, index } =>
                (PageSource::Dataset(self.buffers[*binding]), self.value(*index)),
            IndexedSource::View { address, index, stride } =>
                (self.view_source(address, *stride), self.value(*index)),
            _ => unreachable!(),
        }
    }

    fn reserve_control(&mut self, capacity: usize) -> *mut u32 {
        let at = self.body.len();
        self.body.reserve(capacity);
        unsafe { self.body.as_mut_ptr().add(at) }
    }

    unsafe fn finish_control(&mut self, emitted: ControlWords) -> bool {
        unsafe { self.body.set_len(self.body.len() + emitted.words as usize); }
        if emitted.label != 0 {
            self.current_label = emitted.label;
        }
        emitted.reaches_next != 0
    }

    fn emit_control(&mut self, capacity: usize, encode: impl FnOnce(*mut u32) -> ControlWords) -> bool {
        let emitted = encode(self.reserve_control(capacity));
        unsafe { self.finish_control(emitted) }
    }

    fn structured_control(&mut self, input: StructuredControl) -> Result<bool, &'static str> {
        let mut emission = StructuredEmission { emitter: self, error: None };
        let host = emission.host();
        let flow = unsafe { lower_structured(input, host) };
        emission.error.map_or(Ok(matches!(flow, BlockFlow::Next)), Err)
    }

    fn open_active(&mut self, condition: u32, run: u32, merge: u32) {
        self.emit_control(9, |body| unsafe { open_selection(merge, condition, run, merge, body) });
    }

    fn open_index_branch(&mut self, test: u32, index: u32, bound: u32, less: bool, run: u32, end: u32) {
        let ty = self.types[&Scalar::Bool];
        self.emit_control(14, |body| unsafe { open_index_selection(ty, test, index, bound, less, run, end, body) });
    }
    fn merge_active(&mut self, merge: u32, reaches: bool) {
        self.emit_control(4, |body| unsafe { next_control_block(merge, merge, u32::from(reaches), body) });
    }

    fn join_two(&mut self, ty: u32, result: u32, first: [u32; 2], second: [u32; 2]) {
        let at = self.body.len();
        self.body.reserve(7);
        unsafe {
            let end = phi_two(
                self.body.as_mut_ptr().add(at), 0, ty, result, first[0], first[1], second[0], second[1],
            );
            self.body.set_len(at + end as usize);
        }
    }

    fn leave_control(&mut self, target: u32) -> bool {
        self.emit_control(2, |body| unsafe { leave_control(target, body) })
    }

    fn block(
        &mut self,
        code: &[Inst],
        loop_targets: Option<(u32, u32)>,
    ) -> Result<bool, &'static str> {
        for inst in code {
            match &(inst).kind {
                InstKind::Print { .. } => return Err("GPU 后端不能执行 CPU 标准输出"),
                InstKind::Call(_) | InstKind::Raise(_) | InstKind::Cleanup { .. } => return Err("GPU 后端不能执行普通 CPU ABI 调用或其函数退出"),
                InstKind::TextureSample {
                    result,
                    texture,
                    coordinate,
                    lod,
                } => self.texture_sample(*result, *texture, coordinate, *lod),
                InstKind::TextureBindingAddress { .. }
                | InstKind::TextureAddress { .. }
                | InstKind::TextureFormat { .. }
                | InstKind::TextureRead { .. }
                | InstKind::TextureLevels { .. }
                | InstKind::TextureSize { .. }
                | InstKind::TextureFetch { .. } => return Err("CPU纹理降低指令不能进入SPIR-V"),
                InstKind::Scope(body) => self.function_scope(body)?,
                InstKind::Exit => return Ok(self.function_exit()),
                InstKind::Append(append) => self.append(append),
                InstKind::Atomic(atomic) => self.atomic(atomic)?,
                InstKind::TileBarrier => self.tile_barrier(),
                InstKind::TilePublish(publication) => self.publish_values(publication),
                InstKind::TileCollective(collective) => self.tile_collective(collective),
                InstKind::TileLength(dst) => self.copy_register(*dst, self.tile_length),
                InstKind::TileLane(dst) => self.copy_register(*dst, self.tile_lane),
                InstKind::TileLoad {
                    dst,
                    tile,
                    index,
                    word,
                } => {
                    let scalar = self.stage.registers[*dst];
                    let raw = self.tile_read(scalar, *tile, self.value(*index), *word, None);
                    self.from_words(*dst, &[raw], Self::raw_scalar(scalar));
                }
                InstKind::Subgroup {
                    dst,
                    op,
                    value,
                    argument,
                } => self.subgroup(*dst, *op, *value, *argument),
                InstKind::ViewLoad { dst, address, index, stride, word } => {
                    let scalar = self.stage.registers[*dst];
                    let source = self.view_source(address, *stride);
                    let raw = self.load_physical(scalar, source, self.value(*index), *word, 0);
                    self.from_words(*dst, &[raw], Self::raw_scalar(scalar));
                }
                InstKind::RayQuery { scene, origin, direction, minimum, maximum, cull_back_faces, result } => {
                    self.ray(*scene, origin, direction, *minimum, *maximum, *cull_back_faces, result);
                }
                InstKind::PositionAddress { .. }
                | InstKind::BufferStride(..)
                | InstKind::LoadIndexed { source: IndexedSource::Pointer { .. }, .. }
                | InstKind::StoreIndexed { source: IndexedSource::Pointer { .. }, .. } =>
                    return Err("CPU原生位置地址不能在GPU内核中使用"),
                InstKind::LoadIndexed { dst, source, offset, word } => {
                    let scalar = self.stage.registers[*dst];
                    let raw = if let IndexedSource::Tile { tile, index } = source {
                        self.tile_read(scalar, *tile, self.value(*index), *word, Some(self.value(*offset)))
                    } else {
                        let (page, index) = self.indexed_page(source);
                        self.load_physical(scalar, page, index, *word, self.value(*offset))
                    };
                    self.from_words(*dst, &[raw], Self::raw_scalar(scalar));
                }
                InstKind::StoreIndexed { source, offset, word, value } => {
                    let scalar = self.stage.registers[*value];
                    let unit = Self::physical_unit(scalar);
                    let count = scalar.size().div_ceil(4) as usize;
                    let offset = self.value(*offset);
                    let pointers = if let IndexedSource::Tile { tile, index } = source {
                        let mut pointers = [0; 2];
                        for (part, pointer) in pointers[..count].iter_mut().enumerate() {
                            let slot = self.stage.shared_word(*tile, *word + part as u32 * 4);
                            *pointer = self.tile_word_address(slot, self.value(*index), Some(offset)).0;
                        }
                        pointers
                    } else {
                        let (page, index) = self.indexed_page(source);
                        self.paged_pointers(unit, page, index, *word, offset, count == 2)
                    };
                    let raw = self.raw_bits(*value);
                    self.store_raw(scalar, unit, &pointers[..count], raw, true);
                }
                InstKind::FunctionAddress(..) => return Err("CPU函数符号地址不能在GPU内核中生成"),
                InstKind::Constant(dst, s, bits) if s.is_float() => {
                    let mut words = vec![self.constant(Scalar::U32, *bits as u32)];
                    if s.size() == 8 { words.push(self.constant(Scalar::U32, (*bits >> 32) as u32)); }
                    self.from_words(*dst, &words, Scalar::U32);
                }
                InstKind::Constant(dst, s, bits) => {
                    self.regs[*dst] = self.constant_bits(*s, *bits);
                }
                InstKind::Index(dst) => self.copy_register(*dst, self.index),
                InstKind::Uniform(dst, offset) => self.uniform(*dst, *offset),
                InstKind::LoadLocal(dst, slot) => {
                    let pointer = self.locals[*slot];
                    if pointer == 0 {
                        self.load_private(*dst, *slot, None, 0);
                    } else {
                        self.read_pointer(self.ty(*dst), self.value(*dst), pointer);
                    }
                }
                InstKind::StoreLocal(slot, src) => {
                    let pointer = self.locals[*slot];
                    if pointer == 0 {
                        self.store_private(*src, *slot, None, 0);
                    } else {
                        self.store_pointer(pointer, self.value(*src));
                    }
                }
                InstKind::LoadLocalIndexed { dst, base, index, word, .. } =>
                    self.load_private(*dst, *base, Some(*index), *word),
                InstKind::StoreLocalIndexed { base, index, word, value, .. } =>
                    self.store_private(*value, *base, Some(*index), *word),
                InstKind::Load { dst, binding, index, word } => {
                    let scalar = self.stage.registers[*dst];
                    let raw = self.load_raw_buffer(scalar, *binding, self.value(*index), *word);
                    self.from_words(*dst, &[raw], Self::raw_scalar(scalar));
                }
                InstKind::Store { binding, index, word, value } => {
                    let scalar = self.stage.registers[*value];
                    let raw = self.raw_bits(*value);
                    self.store_raw_buffer(scalar, *binding, self.value(*index), *word, raw);
                }
                InstKind::Unary { dst, op, a } => {
                    let scalar = self.stage.registers[*a];
                    let input = UnaryEmission {
                        operation: *op as u32,
                        result_type: self.ty(*dst),
                        result: self.value(*dst),
                        value: self.value(*a),
                        zero: if *op == UnaryOp::UnsignedNegate { self.constant(scalar, 0) } else { 0 },
                        sign: if *op == UnaryOp::FloatNegate {
                            self.float_sign_operands(scalar, true)
                        } else {
                            FloatSignOperands::default()
                        },
                    };
                    let at = self.body.len();
                    self.body.reserve(13);
                    unsafe {
                        let words = scalar_unary(input, self.body.as_mut_ptr().add(at));
                        self.body.set_len(at + words as usize);
                    }
                }
                InstKind::IntegerDivide { result, src, divisor } => {
                    self.integer_divide(*result, *src, *divisor);
                }
                InstKind::Binary {
                    dst,
                    op: operation,
                    a,
                    b,
                } => {
                    let scalar = self.stage.registers[*a];
                    let (mask, masked) = if matches!(
                        operation, BinaryOp::ShiftLeft | BinaryOp::ShiftRightLogical | BinaryOp::ShiftRightArithmetic
                    ) {
                        (self.constant(scalar, scalar.bit_width() - 1), self.id())
                    } else {
                        (0, 0)
                    };
                    let input = BinaryEmission {
                        operation: (*operation).into(), scalar: scalar.into(),
                        result_type: self.ty(*dst), result: self.value(*dst),
                        left: self.value(*a), right: self.value(*b), mask, masked,
                    };
                    let body = self.body.len();
                    let decorations = self.decorations.len();
                    self.body.reserve(10);
                    self.decorations.reserve(3);
                    unsafe {
                        let words = scalar_binary(input, self.body.as_mut_ptr().add(body),
                            self.decorations.as_mut_ptr().add(decorations));
                        self.body.set_len(body + words.body as usize);
                        self.decorations.set_len(decorations + words.decorations as usize);
                    }
                }
                InstKind::Cast { dst, src } => {
                    let from = self.stage.registers[*src];
                    let to = self.stage.registers[*dst];
                    let (zero, one) = if from == Scalar::Bool {
                        let one = self.constant_bits(to, match to {
                            Scalar::F32 => 1.0f32.to_bits() as Bits,
                            Scalar::F64 => 1.0f64.to_bits(),
                            _ => 1,
                        });
                        (self.constant(to, 0), one)
                    } else if to == Scalar::Bool {
                        (self.constant(from, 0), 0)
                    } else {
                        (0, 0)
                    };
                    let input = CastEmission {
                        from: from.into(), to: to.into(), result_type: self.ty(*dst),
                        result: self.value(*dst), value: self.value(*src), zero, one,
                    };
                    let body = self.body.len();
                    self.body.reserve(6);
                    unsafe {
                        let words = scalar_cast(input, self.body.as_mut_ptr().add(body));
                        self.body.set_len(body + words as usize);
                    }
                }
                InstKind::Math { dst, op, args } => {
                    let scalar = self.stage.registers[*dst];
                    if scalar == Scalar::F64 && matches!(op,
                        MathOp::Sin | MathOp::Cos | MathOp::Pow | MathOp::Exp | MathOp::Exp2 | MathOp::Log | MathOp::Log2) {
                        return Err("GPU f64 超越函数尚未实现；GLSL.std.450 对应指令不接受64位浮点，不能隐式降低精度");
                    }
                    let input = MathEmission {
                        operation: *op as u32,
                        result_type: self.ty(*dst),
                        result: self.value(*dst),
                        left: self.value(args[0]),
                        right: args.get(1).map_or(0, |&r| self.value(r)),
                        extended_set: self.ext,
                        sign: if *op == MathOp::Abs {
                            self.float_sign_operands(scalar, false)
                        } else {
                            FloatSignOperands::default()
                        },
                    };
                    let at = self.body.len();
                    self.body.reserve(13);
                    unsafe {
                        let words = scalar_math(input, self.body.as_mut_ptr().add(at));
                        self.body.set_len(at + words as usize);
                    }
                }
                InstKind::Select { dst, condition, a, b } => self.select(
                    self.ty(*dst), self.value(*dst), self.value(*condition), self.value(*a), self.value(*b),
                ),
                InstKind::If { condition, yes, no } => {
                    if !self.structured_control(StructuredControl {
                        first: yes.as_slice().into(), second: no.as_slice().into(),
                        test: *condition, targets: loop_targets.into(), kind: StructuredKind::Selection,
                    })? {
                        return Ok(false);
                    }
                }
                InstKind::Loop { condition, test, body } => {
                    self.structured_control(StructuredControl {
                        first: condition.as_slice().into(), second: body.as_slice().into(),
                        test: *test, targets: loop_targets.into(), kind: StructuredKind::Loop,
                    })?;
                }
                InstKind::Break => {
                    let (end, _) = loop_targets.unwrap();
                    self.loop_exit_edge(end, self.current_label);
                    return Ok(self.leave_control(end));
                }
                InstKind::Continue => return Ok(self.leave_control(loop_targets.unwrap().1)),
            }
        }
        Ok(true)
    }
    fn float_sign_operands(&mut self, scalar: Scalar, negate: bool) -> FloatSignOperands {
        let sign = 1u64 << (scalar.bit_width() - 1);
        let mask = self.raw_constant(scalar, if negate { sign } else { sign - 1 });
        let temporary = self.id();
        self.id();
        FloatSignOperands { raw_type: self.raw_type(scalar), mask, temporary }
    }
    fn uniform(&mut self, dst: Reg, offset: u32) {
        let scalar = self.stage.registers[dst];
        let zero = self.constant(Scalar::U32, 0);
        let word = offset / 4;
        let low_word = self.constant(Scalar::U32, word);
        let shift = offset % 4 * 8;
        let byte_shift = if shift == 0 { 0 } else { self.constant(Scalar::U32, shift) };
        let (high_word, high_shift) = if scalar.size() == 8 {
            (self.constant(Scalar::U32, word + 1), self.constant(Scalar::U32, 32))
        } else {
            (0, 0)
        };
        let input = UniformRead {
            next: self.next,
            push_pointer: self.ptr_push_u32,
            push_block: self.push,
            uint_type: self.types[&Scalar::U32],
            raw_type: self.raw_type(scalar),
            result: PayloadResult {
                type_id: self.ty(dst),
                id: self.value(dst),
                compare_zero: if scalar == Scalar::Bool { zero } else { 0 },
            },
            zero,
            low_word,
            high_word,
            byte_shift,
            high_shift,
        };
        let at = self.body.len();
        // Up to two access/load/shift groups, a 64-bit join and one result.
        self.body.reserve(57);
        unsafe {
            let emitted = read_uniform(input, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            self.next = emitted.next;
        }
    }
    fn from_words(&mut self, dst: Reg, words: &[u32], unit: Scalar) {
        let scalar = self.stage.registers[dst];
        let (mut input, shifts) = self.payload_input(scalar, unit, words.len(), PayloadAccess::Words);
        let result = PayloadResult {
            type_id: self.ty(dst),
            id: self.value(dst),
            compare_zero: if scalar == Scalar::Bool { self.constant(Scalar::U32, 0) } else { 0 },
        };
        input.next = self.next;
        let at = self.body.len();
        self.body.reserve(20 * words.len() + 5);
        unsafe {
            let read = typed_payload_read(
                input, result, words.as_ptr(), shifts.as_ptr(), self.body.as_mut_ptr().add(at),
            );
            self.body.set_len(at + read.words as usize);
            self.next = read.next;
        }
    }
    fn pack_words<I>(&mut self, size: u32, components: I) -> Vec<u32>
    where
        I: IntoIterator<Item = (Scalar, u32, u32)>,
        I::IntoIter: ExactSizeIterator,
    {
        let zero = self.constant(Scalar::U32, 0);
        let mut values = vec![zero; size.div_ceil(4) as usize];
        let components = components.into_iter();
        let at = self.body.len();
        // A field can emit one split shift, two conversions and two shift/merge pairs.
        self.body.reserve(33 * components.len());
        let body = unsafe { self.body.as_mut_ptr().add(at) };
        let word_type = self.types[&Scalar::U32];
        let next = self.next;
        let mut source = PackingSource { emitter: self, components };
        let host = PackingHost {
            context: std::ptr::from_mut(&mut source).cast(),
            field: packing_field::<I::IntoIter>,
            constant: packing_constant::<I::IntoIter>,
        };
        let result = unsafe { pack_record(host, word_type, zero, next, values.as_mut_ptr(), body) };
        unsafe { self.body.set_len(at + result.words as usize); }
        self.next = result.next;
        values
    }
    fn raw_scalar(scalar: Scalar) -> Scalar {
        match scalar.size() { 1 => Scalar::U8, 2 => Scalar::U16, 4 => Scalar::U32, _ => Scalar::U64 }
    }
    fn raw_type(&self, scalar: Scalar) -> u32 {
        self.types[&Self::raw_scalar(scalar)]
    }
    fn raw_constant(&mut self, scalar: Scalar, bits: Bits) -> u32 {
        self.constant_bits(Self::raw_scalar(scalar), bits)
    }
    fn payload_input(&mut self, scalar: Scalar, unit: Scalar, count: usize, access: PayloadAccess)
        -> (PayloadEmission, [u32; 8])
    {
        let mut shifts = [0; 8];
        for (part, shift) in shifts.iter_mut().enumerate().take(count).skip(1) {
            *shift = self.constant(Scalar::U32, part as u32 * unit.bit_width());
        }
        let input = PayloadEmission {
            access,
            raw_type: self.raw_type(scalar),
            unit_type: self.types[&unit],
            unit_bytes: unit.size(),
            count: count as u32,
            next: self.next,
        };
        (input, shifts)
    }

    fn read_payload(&mut self, scalar: Scalar, unit: Scalar, operands: &[u32], access: PayloadAccess) -> u32 {
        let (input, shifts) = self.payload_input(scalar, unit, operands.len(), access);
        let at = self.body.len();
        self.body.reserve(20 * operands.len());
        unsafe {
            let read = payload_read(input, operands.as_ptr(), shifts.as_ptr(), self.body.as_mut_ptr().add(at));
            self.body.set_len(at + read.words as usize);
            self.next = read.next;
            read.value
        }
    }

    fn join_words(&mut self, scalar: Scalar, words: &[u32], unit: Scalar) -> u32 {
        self.read_payload(scalar, unit, words, PayloadAccess::Words)
    }

    fn raw_bits(&mut self, value: Reg) -> u32 {
        let scalar = self.stage.registers[value];
        let value = self.value(value);
        let raw = Self::raw_scalar(scalar);
        if scalar == raw {
            return value;
        }
        let bits = self.id();
        let (one, zero) = if scalar == Scalar::Bool {
            (self.constant(Scalar::U32, 1), self.constant(Scalar::U32, 0))
        } else {
            (0, 0)
        };
        let at = self.body.len();
        self.body.reserve(6);
        unsafe {
            let words = payload_write_bits(
                self.types[&raw], bits, value, one, zero, self.body.as_mut_ptr().add(at),
            );
            self.body.set_len(at + words as usize);
        }
        bits
    }

    fn write_payload(&mut self, input: PayloadEmission, value: u32, pointers: &[u32], shifts: &[u32; 8], output: *mut u32) {
        let at = self.body.len();
        self.body.reserve(14 * input.count as usize);
        unsafe {
            let write = payload_write(
                input, value, pointers.as_ptr(), shifts.as_ptr(), output, self.body.as_mut_ptr().add(at),
            );
            self.body.set_len(at + write.words as usize);
            self.next = write.next;
        }
    }

    fn split_words(&mut self, scalar: Scalar, bits: u32, unit: Scalar) -> Vec<u32> {
        let count = scalar.size().div_ceil(unit.size()) as usize;
        let (input, shifts) = self.payload_input(scalar, unit, count, PayloadAccess::Words);
        let mut words = Vec::with_capacity(count);
        self.write_payload(input, bits, &[], &shifts, words.as_mut_ptr());
        unsafe { words.set_len(count); }
        words
    }

    fn load_raw(&mut self, scalar: Scalar, unit: Scalar, pointers: &[u32], physical: bool) -> u32 {
        let access = if physical { PayloadAccess::Physical } else { PayloadAccess::Local };
        self.read_payload(scalar, unit, pointers, access)
    }

    fn store_raw(&mut self, scalar: Scalar, unit: Scalar, pointers: &[u32], value: u32, physical: bool) {
        let access = if physical { PayloadAccess::Physical } else { PayloadAccess::Local };
        let (input, shifts) = self.payload_input(scalar, unit, pointers.len(), access);
        self.write_payload(input, value, pointers, &shifts, std::ptr::null_mut());
    }
    fn physical_unit(scalar: Scalar) -> Scalar {
        Self::raw_scalar(if scalar.size() < 4 { scalar } else { Scalar::U32 })
    }

    fn load_physical(&mut self, scalar: Scalar, source: PageSource, index: u32, word: u32, extra: u32) -> u32 {
        let unit = Self::physical_unit(scalar);
        let count = scalar.size().div_ceil(4) as usize;
        let pointers = self.paged_pointers(unit, source, index, word, extra, count == 2);
        self.load_raw(scalar, unit, &pointers[..count], true)
    }

    fn load_raw_buffer(&mut self, scalar: Scalar, binding: usize, index: u32, word: u32) -> u32 {
        self.load_physical(scalar, PageSource::Dataset(self.buffers[binding]), index, word, 0)
    }

    fn store_raw_buffer(&mut self, scalar: Scalar, binding: usize, index: u32, word: u32, value: u32) {
        let unit = Self::physical_unit(scalar);
        let count = scalar.size().div_ceil(4) as usize;
        let pointers = self.paged_pointers(unit, PageSource::Dataset(self.buffers[binding]), index, word, 0, count == 2);
        self.store_raw(scalar, unit, &pointers[..count], value, true);
    }
    fn ray(
        &mut self,
        scene: usize,
        origin: &[Reg; 3],
        direction: &[Reg; 3],
        minimum: Reg,
        maximum: Reg,
        cull_back_faces: Option<Reg>,
        result: &RayResult,
    ) {
        let mut results = [0; 39];
        for (id, reg) in results.iter_mut().zip(result.registers()) {
            *id = self.value(*reg);
        }
        let hit = result.registers()[0];
        let closest = matches!(result, RayResult::Closest(_));
        let (query, fields) = self.ray_variables[&hit];
        let crate::ir::dataflow::RayFields { positions, object_to_world, world_to_object } = fields;
        if positions && self.ray_position_type == 0 {
            let length = self.constant(Scalar::U32, 3);
            self.ray_position_type = self.declare_type(TypeShape::Array, self.vec3_type, length);
        }
        if (object_to_world || world_to_object) && self.ray_transform_type == 0 {
            self.ray_transform_type = self.declare_type(TypeShape::Matrix, self.vec3_type, 4);
        }
        let base_flags = if closest { 1 } else { 5 };
        let mut flags = self.constant(Scalar::U32, base_flags);
        if let Some(cull) = cull_back_faces {
            const CULL_BACK_FACING_TRIANGLES: u32 = 0x10;
            let cull_flags = self.constant(Scalar::U32, base_flags | CULL_BACK_FACING_TRIANGLES);
            let selected = self.id();
            self.select(self.types[&Scalar::U32], selected, self.value(cull), cull_flags, flags);
            flags = selected;
        }
        let input = RayEmission {
            acceleration_type: self.acceleration_type,
            acceleration: self.accelerations[scene],
            query,
            u32_type: self.types[&Scalar::U32],
            f32_type: self.types[&Scalar::F32],
            bool_type: self.types[&Scalar::Bool],
            vec2_type: self.vec2_type,
            vec3_type: self.vec3_type,
            origin: origin.map(|reg| self.value(reg)),
            direction: direction.map(|reg| self.value(reg)),
            minimum: self.value(minimum),
            maximum: self.value(maximum),
            flags,
            mask: self.constant(Scalar::U32, 255),
            committed: self.constant(Scalar::U32, 1),
            zero: self.constant(Scalar::U32, 0),
            float_zero: if closest { self.constant(Scalar::F32, 0) } else { 0 },
            position_type: if positions { self.ray_position_type } else { 0 },
            object_to_world: if object_to_world { self.ray_transform_type } else { 0 },
            world_to_object: if world_to_object { self.ray_transform_type } else { 0 },
            closest: u32::from(closest),
            next: self.next,
        };
        let at = self.body.len();
        self.body.reserve(if closest { 587 } else { 61 });
        unsafe {
            let emitted = encode_ray(input, results.as_ptr(), self.body.as_mut_ptr().add(at));
            self.body.set_len(at + emitted.words as usize);
            self.next = emitted.next;
            self.current_label = emitted.label;
        }
    }
}
pub fn emit(program: &Program, stage_index: usize) -> Result<Vec<u32>, &'static str> {
    emit_pass(program, stage_index, Pass::Main)
}
#[derive(Clone, Copy)]
struct ProgramShaderFeatures {
    float64: bool,
    integer8: bool,
    integer16: bool,
    acceleration: bool,
    descriptor_indexing: bool,
    buffers: ExecutionBuffers,
}

impl ProgramShaderFeatures {
    fn new(requirements: DeviceRequirements, buffers: ExecutionBuffers) -> Self {
        Self {
            float64: requirements.float64,
            integer8: requirements.integer8,
            integer16: requirements.integer16,
            acceleration: requirements.ray_query,
            descriptor_indexing: requirements.texture_indexing,
            buffers,
        }
    }
}

pub fn emit_pass(
    program: &Program,
    stage_index: usize,
    pass: Pass,
) -> Result<Vec<u32>, &'static str> {
    let stage = &program.stages[stage_index];
    program::check_shader_pages(program)?;
    let (layout, mut inputs) = shader_stage_inputs(stage);
    let (float64, integer8, integer16) = crate::ir::dataflow::numeric_requirements(program);
    let features = ProgramShaderFeatures {
        float64, integer8, integer16,
        acceleration: !program.accelerations.is_empty(),
        descriptor_indexing: program.requires_texture_indexing(),
        buffers: program.execution_buffers(),
    };
    let reduction = prepare_reduction(stage, pass);
    emit_shader(program, stage, pass, &mut inputs, &layout, features, reduction.as_ref(), None, true)
        .map(|(words, _)| words)
}
fn shader_stage_inputs(
    stage: &Stage,
) -> (crate::ir::dataflow::TileLayout, crate::ir::dataflow::InputFragments) {
    let mut used = BTreeSet::new();
    let layout = stage.tile_layout(true, Some(&mut used));
    let inputs = crate::ir::dataflow::input_fragments(stage, &used);
    (layout, inputs)
}
#[repr(u32)]
enum ReductionPass {
    Main,
    Add,
    Root,
    Arguments,
    Tail,
}

#[allow(dead_code)]
#[repr(u32)]
enum KernelPreparation {
    Original,
    ProjectIdentity,
    Identity,
    Combine,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::custom_kernel_preparation"]
    fn custom_kernel_preparation(pass: ReductionPass, segmented: u32) -> KernelPreparation;
}

struct ShaderStageFacts {
    ray_queries: Vec<(Reg, crate::ir::dataflow::RayFields)>,
    code: crate::ir::dataflow::ShaderCodeRequirements,
}

fn uses_float_modes(pass: Pass) -> bool {
    matches!(
        pass, Pass::Main | Pass::ReductionTail | Pass::ScanTail | Pass::ScanAdd
            | Pass::ScanRootAdd | Pass::SegmentedSingleGroup
    )
}

impl ShaderStageFacts {
    fn new(stage: &Stage, pass: Pass) -> Self {
        Self {
            ray_queries: crate::ir::dataflow::shader_ray_queries(stage),
            code: crate::ir::dataflow::shader_code_requirements(
                stage, uses_float_modes(pass), pass == Pass::Main,
            ),
        }
    }
}

struct PreparedReduction {
    kernel: ReductionKernel,
    layout: crate::ir::dataflow::TileLayout,
    facts: ShaderStageFacts,
}

fn prepare_reduction(stage: &Stage, pass: Pass) -> Option<PreparedReduction> {
    let (reduction, segmented) = match &stage.operation {
        Operation::CustomReduce(value)
        | Operation::CustomScan(CustomScan { values: value, .. }) => (value, false),
        Operation::CustomSegmentedReduce(value) => (&value.values, true),
        _ => return None,
    };
    let selected = match pass {
        Pass::Main => ReductionPass::Main,
        Pass::ScanAdd => ReductionPass::Add,
        Pass::ScanRootAdd | Pass::SegmentedSingleGroup => ReductionPass::Root,
        Pass::DispatchArguments(_) | Pass::SelectionArguments => ReductionPass::Arguments,
        _ => ReductionPass::Tail,
    };
    let (project, identity) = match unsafe { custom_kernel_preparation(selected, u32::from(segmented)) } {
        KernelPreparation::Original => return None,
        KernelPreparation::ProjectIdentity => (true, true),
        KernelPreparation::Identity => (false, true),
        KernelPreparation::Combine => (false, false),
    };
    let kernel = reduction.kernel(stage, project, identity);
    let layout = kernel.stage.tile_layout(true, None);
    let facts = ShaderStageFacts::new(&kernel.stage, pass);
    Some(PreparedReduction { kernel, layout, facts })
}

fn emit_shader(
    program: &Program, original_stage: &Stage, pass: Pass,
    inputs: &mut crate::ir::dataflow::InputFragments, layout: &crate::ir::dataflow::TileLayout,
    program_features: ProgramShaderFeatures, reduction: Option<&PreparedReduction>,
    shared_original_facts: Option<&ShaderStageFacts>, publishes_summary: bool,
) -> Result<(Vec<u32>, u64), &'static str> {
    let single_group = pass == Pass::SegmentedSingleGroup;
    let pass = if single_group { Pass::ScanRootAdd } else { pass };
    let argument_pass = matches!(pass, Pass::DispatchArguments(_) | Pass::SelectionArguments);
    let numeric_pass = uses_float_modes(pass);
    let local_facts;
    let (stage, tile_layout, facts, custom_kernel) = match reduction {
        Some(prepared) => (&prepared.kernel.stage, &prepared.layout, &prepared.facts, Some(&prepared.kernel)),
        None => {
            let facts = match shared_original_facts {
                Some(shared) => shared,
                None => {
                    local_facts = ShaderStageFacts::new(original_stage, pass);
                    &local_facts
                }
            };
            (original_stage, layout, facts, None)
        }
    };
    let mut e = Emitter {
        next: 1,
        types: BTreeMap::new(),
        pointers: BTreeMap::new(),
        constants: BTreeMap::new(),
        defs: Vec::new(),
        decorations: Vec::new(),
        body: Vec::new(),
        buffers: Vec::new(),
        accelerations: Vec::new(),
        textures: Vec::new(),
        texture_types: [SampledType::default(); 2],
        vec4_type: 0,
        acceleration_type: 0,
        ray_variables: BTreeMap::new(),
        ray_position_type: 0,
        ray_transform_type: 0,
        vec3_type: 0,
        vec2_type: 0,
        push: 0,
        ptr_push_u32: 0,
        physical_pointers: BTreeMap::new(),
        ptr_table_u64: 0,
        u64_type: 0,
        index: 0,
        regs: Vec::new(),
        locals: Vec::new(),
        private_arrays: BTreeMap::new(),
        stage,
        program,
        execution_buffers: program_features.buffers,
        ext: 0,
        subgroup_inputs: BTreeMap::new(),
        ballot_type: 0,
        workgroup_width: stage.workgroup_width(),
        tile_layout,
        tile_shared64: 0,
        tile_pointer64: 0,
        tile_shared: 0,
        tile_pointer: 0,
        tile_length: 0,
        tile_lane: 0,
        current_label: 0,
        return_scopes: Vec::new(),
    };
    let ProgramShaderFeatures { float64, integer8, integer16, acceleration, descriptor_indexing, .. } = program_features;
    let (float32_preserve, float64_preserve) = if numeric_pass {
        facts.code.float_modes()
    } else {
        (false, false)
    };
    let summary_words = match &stage.operation {
        Operation::Reduce(r) => r.ty.size.div_ceil(4),
        Operation::CustomReduce(r) => r.ty.size.div_ceil(4),
        Operation::CustomScan(r) => r.summary_words() as u32,
        Operation::CustomSegmentedReduce(r) => r.summary_words() as u32,
        Operation::ScanExclusive(r) => r.summary_words() as u32,
        Operation::SegmentedReduce(r) => r.summary_words() as u32,
        _ => 1,
    };
    let workgroup = unsafe {
        workgroup_layout(
            EntryOperation::from(&stage.operation), WorkgroupPass::from(pass),
            u32::from(stage.selection.is_some()), summary_words, SCAN_WIDTH,
        )
    };
    let scan_local = workgroup.scan != 0;
    let ranks = workgroup.ranks != 0;
    let empty_subgroups = Default::default();
    let subgroups = if pass == Pass::Main { &facts.code.subgroups } else { &empty_subgroups };
    let subgroup_caps = subgroups.capabilities();
    let features = ShaderFeatures {
        atomic64: u32::from(facts.code.has_atomic64()),
        float64: u32::from(float64),
        integer8: u32::from(integer8),
        integer16: u32::from(integer16),
        preserve32: u32::from(float32_preserve),
        preserve64: u32::from(float64_preserve),
        subgroup_basic: u32::from(subgroup_caps.basic || ranks),
        subgroup_vote: u32::from(subgroup_caps.vote),
        subgroup_arithmetic: u32::from(subgroup_caps.arithmetic),
        subgroup_ballot: u32::from(subgroup_caps.ballot || ranks),
        subgroup_shuffle: u32::from(subgroup_caps.shuffle),
        subgroup_shuffle_relative: u32::from(subgroup_caps.shuffle_relative),
        acceleration: u32::from(acceleration),
        position_fetch: u32::from(facts.ray_queries.iter().any(|(_, fields)| fields.positions)),
        descriptor_indexing: u32::from(descriptor_indexing),
        maximal_reconvergence: subgroups.maximal_reconvergence,
    };
    e.ext = e.id();
    let void = e.declare_type(TypeShape::Void, 0, 0);
    for s in [Scalar::F32, Scalar::U32, Scalar::I32, Scalar::Bool, Scalar::U64, Scalar::I64].into_iter()
        .chain(float64.then_some(Scalar::F64))
        .chain(integer8.then_some([Scalar::U8, Scalar::I8]).into_iter().flatten())
        .chain(integer16.then_some([Scalar::U16, Scalar::I16]).into_iter().flatten()) {
        let id = e.id();
        let ptr = e.id();
        let start = e.defs.len();
        e.defs.reserve(8);
        unsafe {
            let used = scalar_declaration(ScalarCode::from(s), id, ptr, e.defs.as_mut_ptr().add(start));
            e.defs.set_len(start + used as usize);
        }
        e.types.insert(s, id);
        e.pointers.insert(s, ptr);
    }
    let u = e.types[&Scalar::U32];
    e.u64_type = e.types[&Scalar::U64];
    let resources = e.declare_shader_resources(publishes_summary, single_group, argument_pass);
    let ShaderResourceIds { ray_pointer, index_type: v3, global, num_groups, .. } = resources;
    let shared_scalar = Scalar::U32;
    let mut local_input = 0;
    let mut shared = 0;
    let mut shared_pointer = 0;
    if workgroup.local_index != 0 {
        let input_ptr = e.declare_pointer(StorageClass::Input, u);
        local_input = e.declare_annotated(VariableAnnotation::BuiltIn, input_ptr, 29);
    }
    let mut subgroup_inputs = [0; 2];
    let mut ballot_type = 0;
    if ranks {
        let input_ptr = e.declare_pointer(StorageClass::Input, u);
        for (input, builtin) in subgroup_inputs.iter_mut().zip([40, 41]) {
            *input = e.declare_annotated(VariableAnnotation::BuiltIn, input_ptr, builtin);
        }
    }
    if subgroup_caps.ballot || ranks {
        ballot_type = e.declare_type(TypeShape::Vector, u, 4);
        e.ballot_type = ballot_type;
    }
    for (operation, builtin) in [
        (SubgroupOp::Lane, 41),
        (SubgroupOp::Size, 36),
        (SubgroupOp::Id, 40),
    ] {
        if subgroups.uses_builtin(operation) {
            let input_ptr = e.declare_pointer(StorageClass::Input, u);
            let variable = e.declare_annotated(VariableAnnotation::BuiltIn, input_ptr, builtin);
            e.subgroup_inputs.insert(operation, variable);
        }
    }
    let mut shared_bytes = 0u64;
    if workgroup.local_index != 0 || ranks {
        let words = workgroup.words;
        shared_bytes += u64::from(words) * 4;
        let count = e.constant(Scalar::U32, words);
        let array = e.declare_type(TypeShape::Array, e.types[&shared_scalar], count);
        let array_ptr = e.declare_pointer(StorageClass::Workgroup, array);
        shared = e.declare_variable(StorageClass::Workgroup, array_ptr);
        shared_pointer = e.declare_pointer(StorageClass::Workgroup, e.types[&shared_scalar]);
    }
    if pass == Pass::Main {
        for unit in [Scalar::U32, Scalar::U64] {
            let cells = e.tile_layout.values().filter(|(kind, _, _)| *kind == unit)
                .map(|(_, slot, _)| *slot as u64 + 1).max().unwrap_or(0);
            let words = cells * u64::from(e.workgroup_width)
                + if unit == Scalar::U32 { stage.tile_scratch_bytes() / 4 } else { 0 };
            if words == 0 { continue; }
            let words = u32::try_from(words).map_err(|_| "tile 共享数组超过 SPIR-V 长度范围")?;
            shared_bytes += u64::from(words) * u64::from(unit.size());
            let length = e.constant(Scalar::U32, words);
            let array = e.declare_type(TypeShape::Array, e.types[&unit], length);
            let array_ptr = e.declare_pointer(StorageClass::Workgroup, array);
            let shared = e.declare_variable(StorageClass::Workgroup, array_ptr);
            let pointer = e.declare_pointer(StorageClass::Workgroup, e.types[&unit]);
            if unit == Scalar::U32 {
                e.tile_shared = shared; e.tile_pointer = pointer;
            } else {
                e.tile_shared64 = shared; e.tile_pointer64 = pointer;
            }
        }
    }
    let fn_ty = e.declare_type(TypeShape::Function, void, 0);
    let main = e.id();
    let entry = e.id();
    let execute = e.id();
    let end = e.id();
    let finish = |mut e: Emitter<'_>| {
        e.emit_control(2, |body| unsafe { end_shader_body(body) });
        let global_inputs = [global, num_groups];
        let workgroup_inputs = [e.push, shared, local_input];
        let mut builtin_inputs = [0; 3];
        for (input, &id) in builtin_inputs.iter_mut().zip(e.subgroup_inputs.values()) {
            *input = id;
        }
        let tile_inputs = [e.tile_shared, e.tile_shared64];
        let parts: [&[u32]; 8] = [
            &global_inputs, &e.buffers, &e.accelerations, &e.textures,
            &workgroup_inputs, &subgroup_inputs, &builtin_inputs, &tile_inputs,
        ];
        let interface_words: usize = parts.iter().map(|part| part.len()).sum();
        let interfaces = parts.map(|part| ShaderWords {
            data: part.as_ptr(),
            count: part.len() as u64,
        });
        let prepare = argument_pass && (e.stage.selection.is_some() || !matches!(e.stage.operation, Operation::Map));
        let input = ShaderEnvelope {
            bound: e.next, ext: e.ext, entry: main, workgroup_width: e.workgroup_width,
            prepare: u32::from(prepare), features,
        };
        let entry_words = interface_words + usize::from(prepare) * (5 + interface_words);
        let mut out = Vec::with_capacity(
            unsafe { envelope_capacity() } as usize + entry_words
                + e.decorations.len() + e.defs.len() + e.body.len(),
        );
        // The envelope initializes the prefix in the same allocation the module returns.
        unsafe {
            let used = encode_envelope(input, interfaces.as_ptr(), interfaces.len() as u64, out.as_mut_ptr());
            out.set_len(used as usize);
        }
        out.extend(e.decorations);
        out.extend(e.defs);
        out.extend(e.body);
        Ok((out, shared_bytes))
    };
    e.emit_control(7, |body| unsafe { begin_shader_body(void, main, fn_ty, entry, body) });
    if !argument_pass {
        e.regs = (0..stage.registers.len()).map(|_| e.id()).collect();
        for &(result, fields) in &facts.ray_queries {
            let id = e.declare_function_variable(ray_pointer);
            e.ray_variables.insert(result, (id, fields));
        }
        e.locals.resize(stage.locals.last_key_value().map_or(0, |(end, _)| *end), 0);
        for (begin, end) in private_spans(&stage.code) {
            let ty = &stage.locals[&end];
            let size = ty.components.iter().map(|(s, _)| s.size()).min().unwrap().min(4);
            let unit = match size { 1 => Scalar::U8, 2 => Scalar::U16, _ => Scalar::U32 };
            let length = e.constant(Scalar::U32, ty.size / size);
            let array = e.declare_type(TypeShape::Array, e.types[&unit], length);
            let pointer = e.declare_pointer(StorageClass::Function, array);
            let variable = e.declare_function_variable(pointer);
            e.private_arrays.insert(end, PrivateArray { start: begin, unit, variable });
        }
        for (&end, ty) in &stage.locals {
            if e.private_arrays.contains_key(&end) {
                continue;
            }
            let start = end - ty.components.len();
            for (slot, (scalar, _)) in (start..end).zip(&ty.components) {
                e.locals[slot] = e.declare_function_variable(e.pointers[scalar]);
            }
        }
    }
    let gid = e.id();
    let local_index = e.id();
    e.emit_body(9, |body| unsafe { read_shader_index(v3, u, global, gid, local_index, body) });
    match pass {
        Pass::DispatchArguments(prefix) => {
            e.prefix_arguments(Some(&prefix), local_index);
            return finish(e);
        }
        Pass::SelectionArguments => {
            e.prefix_arguments(stage.prefix.as_ref(), local_index);
            return finish(e);
        }
        _ => {}
    }
    let (count, active, groups) = if num_groups != 0 {
        e.prefix_index(
            gid,
            local_index,
            num_groups,
            v3,
            !matches!(stage.operation, Operation::Map)
                || (stage.selection.is_some() && pass != Pass::Main),
        )
    } else {
        e.linear_index(local_index)
    };
    if let Some(kernel) = custom_kernel {
        e.block(&kernel.stage.code[..kernel.invariant_end], None)?;
    }
    if let Some(selection) = stage.selection
        && pass != Pass::Main
    {
        if pass == Pass::SelectionScatter {
            e.selection_scatter(selection, count, active, groups);
        } else if scan_local {
            let local = e.load_pointer(u, local_input);
            e.scan(
                &Scan::counts(selection.scan(e.execution_buffers.selection())),
                pass == Pass::ScanTail,
                local,
                shared,
                shared_pointer,
                active,
                count,
            );
        } else {
            e.open_active(active, execute, end);
            e.scan_add(
                &Scan::counts(selection.scan(e.execution_buffers.selection())),
                false,
            );
            e.merge_active(end, true);
        }
    } else if let Operation::StableSort(s) = &stage.operation {
        match pass {
            Pass::Main => e.sort_initialize(s, s.keys.len() - 1, true, active),
            Pass::SortKey(key) => e.sort_initialize(s, key, false, active),
            Pass::SortGather => e.sort_gather(s, active),
            Pass::SortHistogram(shift) | Pass::SortScatter(shift) => e.sort_rank(
                s,
                shift,
                pass == Pass::SortHistogram(shift),
                count,
                &subgroup_inputs,
                ballot_type,
                shared,
                shared_pointer,
            ),
            Pass::SortScan | Pass::ScanTail => {
                let local = e.load_pointer(u, local_input);
                let r = Reduction {
                    source: s.source,
                    source_word: 0,
                    target: s.target,
                    target_word: 0,
                };
                e.scan(
                    &Scan::counts(r),
                    true,
                    local,
                    shared,
                    shared_pointer,
                    active,
                    count,
                );
            }
            Pass::ScanAdd => {
                e.open_active(active, execute, end);
                e.scan_add(
                    &Scan::counts(Reduction {
                        source: s.source,
                        source_word: 0,
                        target: s.target,
                        target_word: 0,
                    }),
                    false,
                );
                e.merge_active(end, true);
            }
            _ => return Err("排序操作不支持该代码生成阶段"),
        }
    } else if let Operation::CustomSegmentedReduce(r) = &stage.operation {
        let local = if scan_local {
            let local = e.load_pointer(u, local_input);
            local
        } else {
            0
        };
        e.custom_segmented_reduce(
            r,
            custom_kernel.unwrap(),
            pass,
            single_group,
            local,
            shared,
            shared_pointer,
            active,
            count,
        )?;
    } else if let Operation::CustomScan(r) = &stage.operation {
        let kernel = custom_kernel.unwrap();
        if scan_local {
            let local = e.load_pointer(u, local_input);
            e.custom_scan(
                r,
                kernel,
                pass == Pass::ScanTail,
                local,
                shared,
                shared_pointer,
                active,
                count,
            )?;
        } else {
            e.custom_scan_add(r, kernel, pass == Pass::ScanRootAdd, active)?;
        }
    } else if let Operation::CustomReduce(r) = &stage.operation {
        let local = e.load_pointer(u, local_input);
        e.custom_reduce(
            r,
            custom_kernel.unwrap(),
            pass == Pass::ReductionTail,
            local,
            shared,
            shared_pointer,
            active,
            count,
        )?;
    } else if let Operation::Reduce(r) = &stage.operation {
        let local = e.load_pointer(u, local_input);
        e.reduce(
            r,
            pass == Pass::ReductionTail,
            local,
            shared,
            shared_pointer,
            active,
            count,
        );
    } else if let Operation::SegmentedReduce(r) = &stage.operation {
        if pass == Pass::ScanAdd {
            e.segmented_reduce_add(r, active, [execute, end]);
        } else {
            let local = if scan_local {
                let local = e.load_pointer(u, local_input);
                local
            } else {
                0
            };
            e.segmented_reduce(r, pass, single_group, local, shared, shared_pointer, active, count);
        }
    } else if let Operation::Compact(c) = &stage.operation {
        if pass == Pass::CompactScatter {
            e.compact_scatter(c, count, active);
        } else if scan_local {
            let local = e.load_pointer(u, local_input);
            e.scan(
                &Scan::counts(c.scan()),
                pass == Pass::ScanTail,
                local,
                shared,
                shared_pointer,
                active,
                count,
            );
        } else {
            e.open_active(active, execute, end);
            e.scan_add(&Scan::counts(c.scan()), false);
            e.merge_active(end, true);
        }
    } else if let Operation::ScanExclusive(r) = &stage.operation {
        if let Some(head) = r.head_word {
            if scan_local {
                let local = e.load_pointer(u, local_input);
                e.segmented_scan(
                    r,
                    head,
                    pass == Pass::ScanTail,
                    local,
                    shared,
                    shared_pointer,
                    active,
                    count,
                );
            } else {
                match pass {
                    Pass::ScanAdd => e.segmented_add(r, active, [execute, end]),
                    Pass::ScanRootAdd => {
                        e.open_active(active, execute, end);
                        e.segmented_root_add(r);
                        e.merge_active(end, true);
                    }
                    _ => return Err("分段扫描不支持该代码生成阶段"),
                }
            }
        } else {
            if scan_local {
                let local = e.load_pointer(u, local_input);
                e.scan(
                    r,
                    pass == Pass::ScanTail,
                    local,
                    shared,
                    shared_pointer,
                    active,
                    count,
                );
            } else {
                e.open_active(active, execute, end);
                e.scan_add(r, pass == Pass::ScanRootAdd);
                e.merge_active(end, true);
            }
        }
    } else {
        if stage.has_tile_scope() {
            e.tile_input(count, active, inputs);
        }
        let phases = stage.tile_phases();
        if !phases.is_empty() {
            e.tile_phases(active, phases)?;
        } else {
            e.open_active(active, execute, end);
            if stage.selection.is_some() {
                let at = e.address_scalar(Scalar::U32, program.buffers.len(), e.index, 0);
                e.index = e.load_aligned_word(at);
            }
            let reaches = e.block(&stage.code, None)?;
            e.merge_active(end, reaches);
        }
    }
    finish(e)
}
