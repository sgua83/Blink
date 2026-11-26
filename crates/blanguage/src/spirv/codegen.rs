//! 按 SPIR-V 1.4 标准直接编码；不调用 HLSL/GLSL 或第三方编译器。
use crate::ir::*;
use crate::ir::dataflow::{self, Span};
use crate::builtin_types::{ScalarBinaryOp, ScalarCode};
use super::{PAGE_BYTES, PREFIX_LEVELS, SCAN_WIDTH, ProgramImage, ShaderCode, StageCode};
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

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum EntryPass {
    Main, ReductionTail, ScanTail, ScanAdd, ScanRootAdd, SegmentedSingleGroup,
    CompactScatter, SelectionArguments, SelectionScan, SelectionScatter,
    SortHistogram, SortScatter, SortKey, SortScan, SortGather, DispatchArguments,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ShaderPass {
    kind: EntryPass,
    numeric: u32,
    parameter: usize,
    word: u32,
    arguments: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::shader_pass"]
    fn shader_pass(kind: EntryPass, parameter: usize, word: u32) -> ShaderPass;
}

#[derive(Clone, Copy)]
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

fn entry_operation<R>(operation: &Operation<R>) -> (EntryOperation, *const std::ffi::c_void) {
    use EntryOperation::*;
    match operation {
        Operation::Map => (Map, std::ptr::null()),
        Operation::Reduce(r) => (Reduce, std::ptr::from_ref(r).cast()),
        Operation::ScanExclusive(r) => (Scan, std::ptr::from_ref(r).cast()),
        Operation::SegmentedReduce(r) => (Segmented, std::ptr::from_ref(r).cast()),
        Operation::Compact(r) => (Compact, std::ptr::from_ref(r).cast()),
        Operation::StableSort(r) => (Sort, std::ptr::from_ref(r).cast()),
        Operation::CustomScan(r) => (CustomScan, std::ptr::from_ref(r).cast()),
        Operation::CustomSegmentedReduce(r) => (CustomSegmented, std::ptr::from_ref(r).cast()),
        Operation::CustomReduce(r) => (CustomReduce, std::ptr::from_ref(r).cast()),
    }
}

impl<R> From<&Operation<R>> for EntryOperation {
    fn from(operation: &Operation<R>) -> Self {
        entry_operation(operation).0
    }
}

#[repr(C)]
struct WorkgroupLayout {
    scan: u32,
    local_index: u32,
    ranks: u32,
    words: u32,
}

#[repr(C)]
struct WorkgroupDeclaration {
    layout: WorkgroupLayout,
    next: u32,
    word_type: u32,
    wide_type: u32,
    subgroup_mask: u32,
    ballot: u32,
    width: u32,
    narrow_cells: u64,
    wide_cells: u64,
    scratch_words: u64,
}

#[repr(C)]
struct SharedArray {
    variable: u32,
    pointer: u32,
}

#[repr(C)]
struct WorkgroupVariables {
    definitions: u64,
    decorations: u64,
    shared_bytes: u64,
    next: u32,
    local: u32,
    rank_id: u32,
    rank_lane: u32,
    ballot: u32,
    lane: u32,
    size: u32,
    id: u32,
    tile_words: u32,
    reduction: SharedArray,
    tile: SharedArray,
    tile_wide: SharedArray,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::workgroup_declarations"]
    fn workgroup_declarations(
        input: WorkgroupDeclaration, constants: ShaderConstants, defs: *mut u32, decorations: *mut u32,
        output: *mut WorkgroupVariables,
    ) -> u32;

    #[link_name = "bootstrap.spirv_envelope::workgroup_layout"]
    fn workgroup_layout(
        operation: EntryOperation, pass: EntryPass, selected: u32, summary_words: u32, scan_width: u32,
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
    Matrix = 2,
    Array = 4,
    Function,
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
    words: ShaderConstants,
}

type ConstantIds = BTreeMap<(u32, Bits), u32>;

#[repr(C)]
struct ShaderConstants {
    context: *mut std::ffi::c_void,
    intern: unsafe extern "C" fn(*mut std::ffi::c_void, ScalarCode, Bits, u32) -> u32,
}

impl ShaderConstants {
    fn new(pool: &mut ConstantIds) -> Self {
        Self { context: std::ptr::from_mut(pool).cast(), intern: intern_shader_constant }
    }
}

#[derive(Default)]
struct ScalarIds([u32; ScalarCode::I16 as usize + 1]);

impl std::ops::Index<&Scalar> for ScalarIds {
    type Output = u32;
    fn index(&self, scalar: &Scalar) -> &Self::Output {
        &self.0[ScalarCode::from(*scalar) as usize]
    }
}

#[repr(C)]
struct ScalarDeclarations {
    words: u64,
    void: u32,
    next: u32,
}

#[repr(C)]
struct ShaderConstant {
    scalar: ScalarCode,
    type_id: u32,
    next: u32,
    bits: Bits,
}

#[repr(C)]
struct ConstantWords {
    words: u64,
    id: u32,
    next: u32,
}

type ReserveConstant = unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut u32;

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

unsafe extern "C" fn intern_shader_constant(
    constants: *mut std::ffi::c_void, scalar: ScalarCode, bits: Bits, next: u32,
) -> u32 {
    let pool = unsafe { &mut *constants.cast::<ConstantIds>() };
    *pool.entry((scalar as u32, bits)).or_insert(next)
}

unsafe extern "C" fn reserve_constant_words(definitions: *mut std::ffi::c_void) -> *mut u32 {
    let words = unsafe { &mut *definitions.cast::<Vec<u32>>() };
    words.reserve(5);
    unsafe { words.as_mut_ptr().add(words.len()) }
}

#[repr(C)]
struct ScalarWords {
    body: u64,
    decorations: u64,
}

#[repr(C)]
struct ScalarStreams {
    body: *mut u32,
    decorations: *mut u32,
}

#[repr(C)]
struct ScalarHost {
    context: *mut std::ffi::c_void,
    constant: unsafe extern "C" fn(*mut std::ffi::c_void, ScalarCode, Bits) -> u32,
    type_id: unsafe extern "C" fn(*mut std::ffi::c_void, ScalarCode) -> u32,
    ids: unsafe extern "C" fn(*mut std::ffi::c_void, u32) -> u32,
    reserve: unsafe extern "C" fn(*mut std::ffi::c_void, usize, usize) -> ScalarStreams,
}

unsafe extern "C" fn scalar_constant(context: *mut std::ffi::c_void, scalar: ScalarCode, bits: Bits) -> u32 {
    unsafe { &mut *context.cast::<Emitter<'_>>() }.constant_code(scalar, bits)
}

unsafe extern "C" fn scalar_type_id(context: *mut std::ffi::c_void, scalar: ScalarCode) -> u32 {
    unsafe { &*context.cast::<Emitter<'_>>() }.types.0[scalar as usize]
}

unsafe extern "C" fn scalar_ids(context: *mut std::ffi::c_void, count: u32) -> u32 {
    let emitter = unsafe { &mut *context.cast::<Emitter<'_>>() };
    let first = emitter.next;
    emitter.next += count;
    first
}

unsafe extern "C" fn scalar_reserve(
    context: *mut std::ffi::c_void, body: usize, decorations: usize,
) -> ScalarStreams {
    let emitter = unsafe { &mut *context.cast::<Emitter<'_>>() };
    emitter.body.reserve(body);
    emitter.decorations.reserve(decorations);
    unsafe {
        ScalarStreams {
            body: emitter.body.as_mut_ptr().add(emitter.body.len()),
            decorations: emitter.decorations.as_mut_ptr().add(emitter.decorations.len()),
        }
    }
}

#[repr(C)]
struct PageInputs {
    descriptor_pointer: u32,
    page_pointer: u32,
    descriptor: u32,
    low: u32,
    high: u32,
    stride: u32,
    index: u32,
    word: u32,
    has_word: u32,
    extra: u32,
    page_bytes: u32,
}

#[repr(C)]
struct PhysicalAccess {
    page: PageInputs,
    scalar: ScalarCode,
    value: u32,
    value_type: u32,
}

#[repr(C)]
struct MemoryHost {
    scalars: ScalarHost,
    pointer_type: unsafe extern "C" fn(*mut std::ffi::c_void, ScalarCode) -> u32,
    finish_body: unsafe extern "C" fn(*mut std::ffi::c_void, u64),
}

unsafe extern "C" fn memory_pointer_type(context: *mut std::ffi::c_void, scalar: ScalarCode) -> u32 {
    unsafe { &*context.cast::<Emitter<'_>>() }.physical_pointers.0[scalar as usize]
}

unsafe extern "C" fn memory_finish_body(context: *mut std::ffi::c_void, words: u64) {
    let emitter = unsafe { &mut *context.cast::<Emitter<'_>>() };
    unsafe { emitter.body.set_len(emitter.body.len() + words as usize); }
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
struct PayloadShape {
    access: PayloadAccess,
    scalar: ScalarCode,
    unit: ScalarCode,
    count: u32,
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
struct ControlWords {
    words: u64,
    label: u32,
    reaches_next: u32,
}

#[repr(u32)]
enum BlockFlow { Error, #[allow(dead_code)] Stop, Next }

#[repr(C)]
struct ControlTargets { end: u32, next: u32 }
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
    block: unsafe extern "C" fn(*mut std::ffi::c_void, Span, ControlTargets) -> BlockFlow,
    value: unsafe extern "C" fn(*mut std::ffi::c_void, Reg) -> u32,
    begin_return: unsafe extern "C" fn(*mut std::ffi::c_void, Span, u32) -> u32,
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
    context: StructuredContext, code: Span, targets: ControlTargets,
) -> BlockFlow {
    let emission = unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() };
    let host = emission.host();
    let block = block_host(emission.emitter);
    unsafe { emit_instruction_block(code, targets, host, block) }
}

unsafe extern "C" fn structured_value(context: StructuredContext, register: Reg) -> u32 {
    unsafe { &*context.cast::<StructuredEmission<'_, '_>>() }.emitter.value(register)
}
unsafe extern "C" fn structured_begin_return(context: StructuredContext, body: Span, merge: u32) -> u32 {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    u32::from(emitter.begin_return_loop(unsafe { body.as_slice() }, merge))
}
unsafe extern "C" fn structured_normal_exit(context: StructuredContext, end: u32) {
    let emitter = &mut unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter;
    emitter.loop_exit_edge(end, emitter.current_label);
}
unsafe extern "C" fn structured_finish_return(context: StructuredContext) {
    unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.emitter.finish_return_loop();
}

// The B block emitter selects each operation. These callbacks borrow the corresponding
// payload or resolve its operands in the original emitter and invoke a single encoder.
#[repr(u32)]
#[allow(dead_code)] // Selected by the B block emitter.
enum BlockRejection { Print, CpuCall, CpuTexture, CpuAddress, CpuFunction, FloatMath }

#[repr(C)]
struct BlockValues {
    registers: *mut u32,
    scalars: ScalarHost,
    scalar: unsafe extern "C" fn(*mut std::ffi::c_void, Reg) -> ScalarCode,
    binary: unsafe extern "C" fn(*const BinaryOp) -> ScalarBinaryOp,
    finish: unsafe extern "C" fn(*mut std::ffi::c_void, ScalarWords),
    index: u32,
    push_pointer: u32,
    push_block: u32,
    extended_set: u32,
}

unsafe extern "C" fn value_scalar(context: *mut std::ffi::c_void, register: Reg) -> ScalarCode {
    unsafe { &*context.cast::<Emitter<'_>>() }.stage.registers[register].into()
}

unsafe extern "C" fn value_binary(op: *const BinaryOp) -> ScalarBinaryOp {
    unsafe { (*op).into() }
}

unsafe extern "C" fn finish_value(context: *mut std::ffi::c_void, words: ScalarWords) {
    let emitter = unsafe { &mut *context.cast::<Emitter<'_>>() };
    unsafe {
        emitter.body.set_len(emitter.body.len() + words.body as usize);
        emitter.decorations.set_len(emitter.decorations.len() + words.decorations as usize);
    }
}

#[repr(C)]
struct BlockPaging {
    base: PageInputs,
    descriptors: *const u32,
    pointer_type: unsafe extern "C" fn(*mut std::ffi::c_void, ScalarCode) -> u32,
    finish_body: unsafe extern "C" fn(*mut std::ffi::c_void, u64),
    current_label: unsafe extern "C" fn(*mut std::ffi::c_void) -> u32,
    ballot_type: u32,
}

#[repr(C)]
struct PrivateField {
    variable: u32,
    pointer_type: u32,
    unit: ScalarCode,
    offset: u32,
}

#[repr(C)]
struct BlockLocals {
    slots: *const u32,
    array: unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> PrivateField,
}

unsafe extern "C" fn private_array_field(context: *mut std::ffi::c_void, base: usize) -> PrivateField {
    use std::ops::Bound::{Excluded, Unbounded};

    let emitter = unsafe { &*context.cast::<Emitter<'_>>() };
    let (_, array) = emitter.private_arrays.range((Excluded(base), Unbounded)).next().unwrap();
    PrivateField {
        variable: array.variable,
        pointer_type: emitter.pointers[&array.unit],
        unit: array.unit.into(),
        offset: array.shape.components[base - array.start].1,
    }
}

#[repr(C)]
struct BlockHost {
    ir: dataflow::Host,
    values: BlockValues,
    paging: BlockPaging,
    locals: BlockLocals,
    tile: tile::TileStorage,
    reject: unsafe extern "C" fn(StructuredContext, BlockRejection) -> BlockFlow,
    texture_sample: unsafe extern "C" fn(StructuredContext, *const [Reg; 4], *const TextureSource, Span, Reg),
    ray: unsafe extern "C" fn(StructuredContext, *const Inst),
    atomic: unsafe extern "C" fn(StructuredContext, *const Inst) -> BlockFlow,
    subgroup: unsafe extern "C" fn(StructuredContext, Reg, SubgroupOp, Span, Span),
    barrier: unsafe extern "C" fn(StructuredContext),
    publish: unsafe extern "C" fn(StructuredContext, *const Publication),
    collective: unsafe extern "C" fn(StructuredContext, *const TileCollective),
}

fn block_host(emitter: &mut Emitter<'_>) -> BlockHost {
    BlockHost {
        ir: dataflow::HOST,
        values: BlockValues {
            registers: emitter.regs.as_mut_ptr(),
            scalars: emitter.scalar_host(),
            scalar: value_scalar,
            binary: value_binary,
            finish: finish_value,
            index: emitter.index,
            push_pointer: emitter.ptr_push_u32,
            push_block: emitter.push,
            extended_set: emitter.ext,
        },
        paging: BlockPaging {
            base: emitter.page_inputs(0, 0, None, 0),
            descriptors: emitter.buffers.as_ptr(),
            pointer_type: memory_pointer_type,
            finish_body: memory_finish_body,
            current_label: block_current_label,
            ballot_type: emitter.ballot_type,
        },
        locals: BlockLocals {
            slots: emitter.locals.as_ptr(),
            array: private_array_field,
        },
        tile: emitter.tile_storage(),
        reject: block_reject,
        texture_sample: block_texture_sample,
        ray: block_ray,
        atomic: block_atomic,
        subgroup: block_subgroup,
        barrier: block_barrier,
        publish: block_publish,
        collective: block_collective,
    }
}

unsafe extern "C" fn block_current_label(context: *mut std::ffi::c_void) -> u32 {
    unsafe { &*context.cast::<Emitter<'_>>() }.current_label
}

unsafe fn block_emitter<'a, 'program>(context: StructuredContext) -> &'a mut Emitter<'program> {
    unsafe { &mut *context.cast::<StructuredEmission<'_, 'program>>() }.emitter
}

unsafe extern "C" fn block_reject(context: StructuredContext, rejection: BlockRejection) -> BlockFlow {
    unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() }.error = Some(match rejection {
        BlockRejection::Print => "GPU 后端不能执行 CPU 标准输出",
        BlockRejection::CpuCall => "GPU 后端不能执行普通 CPU ABI 调用或其函数退出",
        BlockRejection::CpuTexture => "CPU纹理降低指令不能进入SPIR-V",
        BlockRejection::CpuAddress => "CPU原生位置地址不能在GPU内核中使用",
        BlockRejection::CpuFunction => "CPU函数符号地址不能在GPU内核中生成",
        BlockRejection::FloatMath => "GPU f64 超越函数尚未实现；GLSL.std.450 对应指令不接受64位浮点，不能隐式降低精度",
    });
    BlockFlow::Error
}

unsafe extern "C" fn block_texture_sample(context: StructuredContext, result: *const [Reg; 4],
    texture: *const TextureSource, coordinate: Span, lod: Reg,
) {
    unsafe { block_emitter(context).texture_sample(*result, *texture, coordinate.as_slice(), lod) };
}

unsafe extern "C" fn block_ray(context: StructuredContext, instruction: *const Inst) {
    let InstKind::RayQuery { scene, origin, direction, minimum, maximum, cull_back_faces, result } =
        &unsafe { &*instruction }.kind else { unreachable!() };
    unsafe { block_emitter(context) }.ray(*scene, origin, direction, *minimum, *maximum, *cull_back_faces, result);
}

unsafe extern "C" fn block_atomic(context: StructuredContext, instruction: *const Inst) -> BlockFlow {
    let InstKind::Atomic(atomic) = &unsafe { &*instruction }.kind else { unreachable!() };
    let emission = unsafe { &mut *context.cast::<StructuredEmission<'_, '_>>() };
    match emission.emitter.atomic(atomic) {
        Ok(()) => BlockFlow::Next,
        Err(error) => {
            emission.error = Some(error);
            BlockFlow::Error
        }
    }
}

unsafe extern "C" fn block_subgroup(context: StructuredContext, dst: Reg, op: SubgroupOp, value: Span, argument: Span) {
    let value = unsafe { value.as_slice::<Reg>() }.first().copied();
    let argument = unsafe { argument.as_slice::<Reg>() }.first().copied();
    unsafe { block_emitter(context) }.subgroup(dst, op, value, argument);
}

unsafe extern "C" fn block_barrier(context: StructuredContext) {
    unsafe { block_emitter(context) }.tile_barrier();
}

unsafe extern "C" fn block_publish(context: StructuredContext, publication: *const Publication) {
    unsafe { block_emitter(context).publish_values(&*publication) };
}

unsafe extern "C" fn block_collective(context: StructuredContext, collective: *const TileCollective) {
    unsafe { block_emitter(context).tile_collective(&*collective) };
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
struct DispatchLayout {
    table_pointer: u32,
    page_pointer: u32,
    word_pointer: u32,
    descriptor: u32,
    width: u32,
    width_shift: u32,
    record_bytes: u32,
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

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::encode_ray"]
    fn encode_ray(input: RayEmission, results: *const u32, body: *mut u32) -> RayWords;
    #[link_name = "bootstrap.spirv_envelope::workgroup_barrier"]
    fn encode_workgroup_barrier(body: *mut u32, at: u64, scope: u32, semantics: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::encode_sort_rank"]
    fn encode_sort_rank(input: SortRankEmission, body: *mut u32) -> StageWords;
    #[link_name = "bootstrap.spirv_envelope::encode_linear_index"]
    fn encode_linear_index(
        push: DispatchPush, wide_type: u32, bool_type: u32, local: u32, one: u32, first: u32,
        body: *mut u32,
    ) -> PrefixIndexWords;
    #[link_name = "bootstrap.spirv_envelope::encode_prefix_index"]
    fn encode_prefix_index(input: PrefixIndexEmission, body: *mut u32) -> PrefixIndexWords;
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
    #[link_name = "bootstrap.spirv_envelope::function_storage"]
    fn function_storage(input: FunctionStorageInput, host: FunctionStorageHost, constants: ShaderConstants,
        locals: *mut u32, definitions: *mut u32, body: *mut u32) -> FunctionStorageWords;
    #[link_name = "bootstrap.spirv_envelope::select_words"]
    fn select_words(body: *mut u32, at: u64, result_type: u32, result: u32,
                    condition: u32, yes: u32, no: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::prepare_page"]
    fn prepare_page(input: PageInputs, result_pointer: u32, host: ScalarHost) -> PageAddress;
    #[link_name = "bootstrap.spirv_envelope::physical_scalar_read"]
    fn physical_scalar_read(input: PhysicalAccess, host: MemoryHost) -> u32;
    #[link_name = "bootstrap.spirv_envelope::physical_scalar_write"]
    fn physical_scalar_write(input: PhysicalAccess, host: MemoryHost);
    #[link_name = "bootstrap.spirv_envelope::encode_raw_bits"]
    fn encode_raw_bits(scalar: ScalarCode, value: u32, host: MemoryHost) -> u32;
    #[link_name = "bootstrap.spirv_envelope::page_address"]
    fn page_address(input: PageAddress, step: u32, body: *mut u32) -> PageAddressResult;
    #[link_name = "bootstrap.spirv_envelope::summary_addresses"]
    fn emit_summary_addresses(input: PageAddress, step: u32, pointers: *mut u32, count: u32,
        body: *mut u32) -> PayloadWrite;
    #[link_name = "bootstrap.spirv_envelope::read_summary_words"]
    fn read_summary_words(body: *mut u32, type_id: u32, first: u32, pointers: *const u32, count: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::write_summary_words"]
    fn write_summary_words(body: *mut u32, pointers: *const u32, values: *const u32, count: u32) -> u64;
    #[link_name = "bootstrap.spirv_envelope::emit_payload_load"]
    fn emit_payload_load(input: PayloadShape, operands: *const u32, result: u32, host: MemoryHost) -> u32;
    #[link_name = "bootstrap.spirv_envelope::emit_payload_store"]
    fn emit_payload_store(input: PayloadShape, value: u32, pointers: *const u32,
        output: *mut u32, host: MemoryHost);
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
        less: u32, run: u32, end: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::open_tile_padding"]
    fn open_tile_padding(uint_type: u32, bool_type: u32, lane: u32, width: u32,
        capacity: u32, index: u32, test: u32, run: u32, end: u32, body: *mut u32) -> ControlWords;
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
    #[link_name = "bootstrap.spirv_envelope::emit_instruction_block"]
    fn emit_instruction_block(code: Span, targets: ControlTargets, host: StructuredHost, emit: BlockHost) -> BlockFlow;
    #[link_name = "bootstrap.spirv_envelope::lower_return_loop"]
    fn lower_return_loop(edges: *const returns::ReturnEdge, count: u32, bool_type: u32, host: StructuredHost);
    #[link_name = "bootstrap.spirv_envelope::encode_subgroup"]
    fn encode_subgroup(input: SubgroupEmission, body: *mut u32) -> SubgroupWords;
    #[link_name = "bootstrap.spirv_envelope::shader_scalars"]
    fn shader_scalars(next: u32, float64: u32, integer8: u32, integer16: u32,
        definitions: *mut u32, types: *mut u32, pointers: *mut u32) -> ScalarDeclarations;

    #[link_name = "bootstrap.spirv_envelope::shader_constant"]
    fn shader_constant(input: ShaderConstant, constants: ShaderConstants,
        definitions: *mut std::ffi::c_void, reserve: ReserveConstant) -> ConstantWords;
}

struct PrivateArray<'a> {
    start: usize,
    unit: Scalar,
    variable: u32,
    shape: &'a Ty,
}

#[repr(C)]
struct FunctionArray {
    start: usize,
    end: usize,
    fields: usize,
    shape: *const std::ffi::c_void,
    bytes: u32,
}

#[repr(C)]
struct FunctionLocals {
    start: usize,
    fields: usize,
    shape: *const std::ffi::c_void,
}

#[repr(C)]
struct FunctionStorageInput {
    rays: usize,
    next: u32,
    ray_pointer: u32,
    word_type: u32,
    byte_type: u32,
    short_type: u32,
}

#[repr(C)]
struct FunctionStorageWords {
    definitions: u64,
    body: u64,
    next: u32,
}

type FunctionStorageContext = *mut std::ffi::c_void;

#[repr(C)]
struct FunctionStorageHost {
    context: FunctionStorageContext,
    array: unsafe extern "C" fn(FunctionStorageContext) -> FunctionArray,
    scalar_group: unsafe extern "C" fn(FunctionStorageContext) -> FunctionLocals,
    width: unsafe extern "C" fn(*const std::ffi::c_void, usize) -> u32,
    pointer: unsafe extern "C" fn(FunctionStorageContext, *const std::ffi::c_void, usize) -> u32,
    retain_array: unsafe extern "C" fn(FunctionStorageContext, FunctionArray, u32, u32),
}

struct FunctionStorage<'a, 'emit> {
    stage: &'a Stage,
    pending: std::collections::btree_set::IntoIter<(usize, usize)>,
    locals: std::collections::btree_map::Iter<'a, usize, Ty>,
    pointers: &'emit ScalarIds,
    arrays: &'emit mut BTreeMap<usize, PrivateArray<'a>>,
}

unsafe extern "C" fn next_function_array(context: FunctionStorageContext) -> FunctionArray {
    let source = unsafe { &mut *context.cast::<FunctionStorage<'_, '_>>() };
    let Some((start, end)) = source.pending.next() else {
        return FunctionArray { start: 0, end: 0, fields: 0, shape: std::ptr::null(), bytes: 0 };
    };
    let shape = &source.stage.locals[&end];
    FunctionArray { start, end, fields: shape.components.len(), shape: std::ptr::from_ref(shape).cast(), bytes: shape.size }
}

unsafe extern "C" fn next_function_locals(context: FunctionStorageContext) -> FunctionLocals {
    let source = unsafe { &mut *context.cast::<FunctionStorage<'_, '_>>() };
    let arrays = &*source.arrays;
    let Some((&end, shape)) = source.locals.by_ref().find(|(end, _)| !arrays.contains_key(*end)) else {
        return FunctionLocals { start: 0, fields: 0, shape: std::ptr::null() };
    };
    let fields = shape.components.len();
    FunctionLocals { start: end - fields, fields, shape: std::ptr::from_ref(shape).cast() }
}

unsafe extern "C" fn function_component_width(shape: *const std::ffi::c_void, field: usize) -> u32 {
    unsafe { &*shape.cast::<Ty>() }.components[field].0.size()
}

unsafe extern "C" fn function_component_pointer(
    context: FunctionStorageContext, shape: *const std::ffi::c_void, field: usize,
) -> u32 {
    let source = unsafe { &*context.cast::<FunctionStorage<'_, '_>>() };
    let shape = unsafe { &*shape.cast::<Ty>() };
    source.pointers[&shape.components[field].0]
}

unsafe extern "C" fn retain_function_array(
    context: FunctionStorageContext, array: FunctionArray, bytes: u32, variable: u32,
) {
    let source = unsafe { &mut *context.cast::<FunctionStorage<'_, '_>>() };
    // The array's shape was borrowed from this stage by next_function_array.
    let shape = unsafe { &*array.shape.cast::<Ty>() };
    let unit = match bytes { 1 => Scalar::U8, 2 => Scalar::U16, _ => Scalar::U32 };
    source.arrays.insert(array.end, PrivateArray { start: array.start, unit, variable, shape });
}

struct Emitter<'a> {
    next: u32,
    types: ScalarIds,
    pointers: ScalarIds,
    constants: ConstantIds,
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
    physical_pointers: ScalarIds,
    ptr_table_u64: u32,
    u64_type: u32,
    index: u32,
    regs: Vec<u32>,
    locals: Vec<u32>,
    private_arrays: BTreeMap<usize, PrivateArray<'a>>,
    stage: &'a Stage,
    program: &'a Program,
    execution_buffers: ExecutionBuffers,
    ext: u32,
    subgroup_inputs: [u32; 3],
    ballot_type: u32,
    workgroup_width: u32,
    tile_layout: &'a crate::ir::dataflow::TileLayout,
    tile_shared64: u32,
    tile_pointer64: u32,
    tile_shared: u32,
    tile_pointer: u32,
    tile_data_words: u32,
    tile_length: u32,
    tile_lane: u32,
    current_label: u32,
    return_scopes: Vec<returns::ReturnScope>,
}
impl Emitter<'_> {
    fn id(&mut self) -> u32 {
        let v = self.next;
        self.next += 1;
        v
    }
    fn constant(&mut self, s: Scalar, bits: u32) -> u32 {
        self.constant_bits(s, Bits::from(bits))
    }
    fn constant_bits(&mut self, scalar: Scalar, bits: Bits) -> u32 {
        self.constant_code(scalar.into(), bits)
    }
    fn constant_code(&mut self, scalar: ScalarCode, bits: Bits) -> u32 {
        let input = ShaderConstant {
            scalar, type_id: self.types.0[scalar as usize], next: self.next, bits,
        };
        let start = self.defs.len();
        let definitions = std::ptr::from_mut(&mut self.defs).cast();
        let result = unsafe {
            shader_constant(input, ShaderConstants::new(&mut self.constants), definitions, reserve_constant_words)
        };
        unsafe { self.defs.set_len(start + result.words as usize); }
        self.next = result.next;
        result.id
    }
    fn declare_scalars(&mut self, float64: bool, integer8: bool, integer16: bool) -> u32 {
        let at = self.defs.len();
        self.defs.reserve(2 + 8 * self.types.0.len());
        let result = unsafe {
            shader_scalars(self.next, u32::from(float64), u32::from(integer8), u32::from(integer16),
                self.defs.as_mut_ptr().add(at), self.types.0.as_mut_ptr(), self.pointers.0.as_mut_ptr())
        };
        unsafe { self.defs.set_len(at + result.words as usize); }
        self.next = result.next;
        result.void
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
    fn declare_function_storage(&mut self, ray_pointer: u32, facts: &ShaderStageFacts) {
        let stage = self.stage;
        let spans = private_spans(&stage.code);
        let slots = stage.locals.last_key_value().map_or(0, |(end, _)| *end);
        let definitions = self.defs.len();
        let body = self.body.len();
        let rays = &facts.ray_queries;
        let first_ray = self.next;
        self.defs.reserve(12 * spans.len());
        self.body.reserve(4 * (rays.len() + slots + spans.len()));
        self.locals.resize(slots, 0);
        let result = {
            let mut storage = FunctionStorage {
                stage, pending: spans.into_iter(), locals: stage.locals.iter(),
                pointers: &self.pointers, arrays: &mut self.private_arrays,
            };
            let host = FunctionStorageHost {
                context: std::ptr::from_mut(&mut storage).cast(),
                array: next_function_array,
                scalar_group: next_function_locals,
                width: function_component_width,
                pointer: function_component_pointer,
                retain_array: retain_function_array,
            };
            let input = FunctionStorageInput {
                rays: rays.len(), next: self.next, ray_pointer,
                word_type: self.types[&Scalar::U32],
                byte_type: self.types[&Scalar::U8],
                short_type: self.types[&Scalar::U16],
            };
            unsafe {
                function_storage(input, host, ShaderConstants::new(&mut self.constants), self.locals.as_mut_ptr(),
                    self.defs.as_mut_ptr().add(definitions), self.body.as_mut_ptr().add(body))
            }
        };
        unsafe {
            self.defs.set_len(definitions + result.definitions as usize);
            self.body.set_len(body + result.body as usize);
        }
        self.next = result.next;
        for (offset, &(reg, fields)) in rays.iter().enumerate() {
            self.ray_variables.insert(reg, (first_ray + offset as u32, fields));
        }
    }
    fn declare_workgroup(&mut self, input: WorkgroupDeclaration) -> Result<WorkgroupVariables, &'static str> {
        let definitions = self.defs.len();
        let decorations = self.decorations.len();
        // Up to six built-ins, one ballot type and three shared arrays with their lengths.
        self.defs.reserve(108);
        self.decorations.reserve(24);
        let mut output = std::mem::MaybeUninit::<WorkgroupVariables>::uninit();
        let success = unsafe {
            workgroup_declarations(input, ShaderConstants::new(&mut self.constants),
                self.defs.as_mut_ptr().add(definitions), self.decorations.as_mut_ptr().add(decorations),
                output.as_mut_ptr())
        };
        if success == 0 {
            return Err("tile 共享数组超过 SPIR-V 长度范围");
        }
        let ids = unsafe { output.assume_init() };
        unsafe {
            self.defs.set_len(definitions + ids.definitions as usize);
            self.decorations.set_len(decorations + ids.decorations as usize);
        }
        self.next = ids.next;
        self.subgroup_inputs = [ids.lane, ids.size, ids.id];
        self.ballot_type = ids.ballot;
        self.tile_shared = ids.tile.variable;
        self.tile_pointer = ids.tile.pointer;
        self.tile_shared64 = ids.tile_wide.variable;
        self.tile_pointer64 = ids.tile_wide.pointer;
        self.tile_data_words = ids.tile_words;
        Ok(ids)
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
            byte_type: self.types[&Scalar::U8],
            short_type: self.types[&Scalar::U16],
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
            words: ShaderConstants::new(&mut self.constants),
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
                self.physical_pointers.0[ScalarCode::from(scalar) as usize] = pointer;
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
    fn scalar_host(&mut self) -> ScalarHost {
        ScalarHost {
            context: std::ptr::from_mut(self).cast(),
            constant: scalar_constant,
            type_id: scalar_type_id,
            ids: scalar_ids,
            reserve: scalar_reserve,
        }
    }

    fn memory_host(&mut self) -> MemoryHost {
        MemoryHost {
            scalars: self.scalar_host(),
            pointer_type: memory_pointer_type,
            finish_body: memory_finish_body,
        }
    }

    fn address_scalar(&mut self, unit: Scalar, binding: usize, index: u32, word: u32) -> u32 {
        self.page_address(unit, self.buffers[binding], index, word, 0)
    }

    fn summary_addresses(&mut self, count: usize, binding: usize, index: u32) -> Vec<u32> {
        if count == 0 {
            return Vec::new();
        }
        let step = if count > 1 { self.constant(Scalar::U64, 4) } else { 0 };
        let input = self.page_input(Scalar::U32, self.buffers[binding], index, Some(0), 0);
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

    fn page_address(&mut self, unit: Scalar, descriptor: u32, index: u32, word: u32, extra: u32) -> u32 {
        let input = self.page_input(unit, descriptor, index, Some(word), extra);
        let at = self.body.len();
        self.body.reserve(109);
        unsafe {
            let address = page_address(input, 0, self.body.as_mut_ptr().add(at));
            self.body.set_len(at + address.words as usize);
            self.next = address.next;
            address.pointer
        }
    }

    fn page_inputs(&self, descriptor: u32, index: u32, word: Option<u32>, extra: u32) -> PageInputs {
        PageInputs {
            descriptor_pointer: self.ptr_table_u64,
            page_pointer: self.physical_pointers[&Scalar::U64],
            descriptor, low: 0, high: 0, stride: 0, index,
            word: word.unwrap_or(0), has_word: u32::from(word.is_some()), extra, page_bytes: PAGE_BYTES,
        }
    }

    fn page_input(
        &mut self, unit: Scalar, descriptor: u32, index: u32, word: Option<u32>, extra: u32,
    ) -> PageAddress {
        let input = self.page_inputs(descriptor, index, word, extra);
        let result_pointer = self.physical_pointers[&unit];
        unsafe { prepare_page(input, result_pointer, self.scalar_host()) }
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
            scalar, self.buffers[source_binding], self.index, None, 0,
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

    fn open_active(&mut self, condition: u32, run: u32, merge: u32) {
        self.emit_control(9, |body| unsafe { open_selection(merge, condition, run, merge, body) });
    }

    fn open_index_branch(&mut self, test: u32, index: u32, bound: u32, less: bool, run: u32, end: u32) {
        let ty = self.types[&Scalar::Bool];
        self.emit_control(14, |body| unsafe { open_index_selection(ty, test, index, bound, u32::from(less), run, end, body) });
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

    fn block(&mut self, code: &[Inst]) -> Result<bool, &'static str> {
        let mut emission = StructuredEmission { emitter: self, error: None };
        let context = std::ptr::from_mut(&mut emission).cast();
        let flow = unsafe { structured_block(context, Span::slice(code), ControlTargets { end: 0, next: 0 }) };
        emission.error.map_or(Ok(matches!(flow, BlockFlow::Next)), Err)
    }

    fn from_words(&mut self, dst: Reg, words: &[u32], unit: Scalar) {
        let input = PayloadShape {
            access: PayloadAccess::Words, scalar: self.stage.registers[dst].into(),
            unit: unit.into(), count: words.len() as u32,
        };
        unsafe { emit_payload_load(input, words.as_ptr(), self.value(dst), self.memory_host()); }
    }
    fn pack_word_fields<I>(&mut self, values: &mut [u32], zero: u32, components: I)
    where
        I: IntoIterator<Item = (Scalar, u32, u32)>,
        I::IntoIter: ExactSizeIterator,
    {
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
    fn read_payload(&mut self, scalar: Scalar, unit: Scalar, operands: &[u32], access: PayloadAccess) -> u32 {
        let input = PayloadShape { access, scalar: scalar.into(), unit: unit.into(), count: operands.len() as u32 };
        unsafe { emit_payload_load(input, operands.as_ptr(), 0, self.memory_host()) }
    }

    fn raw_bits(&mut self, value: Reg) -> u32 {
        let scalar = self.stage.registers[value].into();
        let value = self.value(value);
        unsafe { encode_raw_bits(scalar, value, self.memory_host()) }
    }

    fn split_words(&mut self, scalar: Scalar, bits: u32, unit: Scalar) -> Vec<u32> {
        let count = scalar.size().div_ceil(unit.size());
        let input = PayloadShape { access: PayloadAccess::Words, scalar: scalar.into(), unit: unit.into(), count };
        let mut words = Vec::with_capacity(count as usize);
        unsafe {
            emit_payload_store(input, bits, std::ptr::null(), words.as_mut_ptr(), self.memory_host());
            words.set_len(count as usize);
        }
        words
    }

    fn load_raw(&mut self, scalar: Scalar, unit: Scalar, pointers: &[u32]) -> u32 {
        self.read_payload(scalar, unit, pointers, PayloadAccess::Local)
    }

    fn store_raw(&mut self, scalar: Scalar, unit: Scalar, pointers: &[u32], value: u32, physical: bool) {
        let access = if physical { PayloadAccess::Physical } else { PayloadAccess::Local };
        let input = PayloadShape { access, scalar: scalar.into(), unit: unit.into(), count: pointers.len() as u32 };
        unsafe { emit_payload_store(input, value, pointers.as_ptr(), std::ptr::null_mut(), self.memory_host()); }
    }
    fn physical_unit(scalar: Scalar) -> Scalar {
        Self::raw_scalar(if scalar.size() < 4 { scalar } else { Scalar::U32 })
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
    let (layout, inputs) = shader_stage_inputs(stage);
    let (float64, integer8, integer16) = crate::ir::dataflow::numeric_requirements(program);
    let features = ProgramShaderFeatures {
        float64, integer8, integer16,
        acceleration: !program.accelerations.is_empty(),
        descriptor_indexing: program.requires_texture_indexing(),
        buffers: program.execution_buffers(),
    };
    let (kind, parameter, word) = match pass {
        Pass::Main => (EntryPass::Main, 0, 0),
        Pass::ReductionTail => (EntryPass::ReductionTail, 0, 0),
        Pass::ScanTail => (EntryPass::ScanTail, 0, 0),
        Pass::ScanAdd => (EntryPass::ScanAdd, 0, 0),
        Pass::ScanRootAdd => (EntryPass::ScanRootAdd, 0, 0),
        Pass::SegmentedSingleGroup => (EntryPass::SegmentedSingleGroup, 0, 0),
        Pass::CompactScatter => (EntryPass::CompactScatter, 0, 0),
        Pass::SelectionArguments => (EntryPass::SelectionArguments, 0, 0),
        Pass::SelectionScan => (EntryPass::SelectionScan, 0, 0),
        Pass::SelectionScatter => (EntryPass::SelectionScatter, 0, 0),
        Pass::SortHistogram(shift) => (EntryPass::SortHistogram, shift as usize, 0),
        Pass::SortScatter(shift) => (EntryPass::SortScatter, shift as usize, 0),
        Pass::SortKey(key) => (EntryPass::SortKey, key, 0),
        Pass::SortScan => (EntryPass::SortScan, 0, 0),
        Pass::SortGather => (EntryPass::SortGather, 0, 0),
        Pass::DispatchArguments(prefix) => (EntryPass::DispatchArguments, prefix.binding, prefix.word),
    };
    let pass = unsafe { shader_pass(kind, parameter, word) };
    let reduction = prepare_reduction(stage, pass);
    emit_shader(program, stage, pass, &inputs, &layout, features, reduction.as_ref(), None, true)
        .map(|(words, _)| words)
}
fn shader_stage_inputs(
    stage: &Stage,
) -> (PlacedTiles, crate::ir::dataflow::InputFragments) {
    let mut used = BTreeSet::new();
    let layout = stage.tile_layout(true, Some(&mut used));
    let inputs = crate::ir::dataflow::input_fragments(stage, &used);
    (layout, inputs)
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
    fn custom_kernel_preparation(pass: EntryPass, segmented: u32) -> KernelPreparation;
}

struct ShaderStageFacts {
    ray_queries: Vec<(Reg, crate::ir::dataflow::RayFields)>,
    code: crate::ir::dataflow::ShaderCodeRequirements,
}

impl ShaderStageFacts {
    fn new(stage: &Stage, numeric: bool, subgroups: bool) -> Self {
        Self {
            ray_queries: crate::ir::dataflow::shader_ray_queries(stage),
            code: crate::ir::dataflow::shader_code_requirements(
                stage, numeric, subgroups,
            ),
        }
    }
}

struct PreparedReduction {
    kernel: ReductionKernel,
    layout: PlacedTiles,
    facts: ShaderStageFacts,
}

fn prepare_reduction(stage: &Stage, pass: ShaderPass) -> Option<PreparedReduction> {
    let (reduction, segmented) = match &stage.operation {
        Operation::CustomReduce(value)
        | Operation::CustomScan(CustomScan { values: value, .. }) => (value, false),
        Operation::CustomSegmentedReduce(value) => (&value.values, true),
        _ => return None,
    };
    let (project, identity) = match unsafe { custom_kernel_preparation(pass.kind, u32::from(segmented)) } {
        KernelPreparation::Original => return None,
        KernelPreparation::ProjectIdentity => (true, true),
        KernelPreparation::Identity => (false, true),
        KernelPreparation::Combine => (false, false),
    };
    let kernel = reduction.kernel(stage, project, identity);
    let layout = kernel.stage.tile_layout(true, None);
    let facts = ShaderStageFacts::new(&kernel.stage, pass.numeric != 0, pass.kind == EntryPass::Main);
    Some(PreparedReduction { kernel, layout, facts })
}

#[repr(C)]
struct ShaderBodyInput {
    operation: EntryOperation,
    pass: EntryPass,
    selection: u32,
    scan_local: u32,
    segmented_scan: u32,
    invariants: u32,
    tile_scope: u32,
    prefix_index: u32,
}

#[allow(dead_code)] // Discriminants are selected by the B stage control.
#[repr(u32)]
enum BodyStep {
    DispatchArguments, SelectionArguments, PrefixIndex, LinearIndex, Invariants,
    Local, Open, Merge, SelectionScatter, SortKey, SortGather, SortRank,
    Scan, ScanAdd, SegmentedScan, SegmentedAdd, SegmentedRoot,
    CustomSegmented, CustomScan, CustomScanAdd, CustomReduce, Reduce,
    SegmentedReduce, SegmentedReduceAdd, CompactScatter, TileInput, TilePhases,
    SelectedIndex, Map,
}

#[allow(dead_code)]
#[repr(u32)]
enum BodyScan { Original, Selection, Compact, Sort }

#[repr(u32)]
enum BodyStepResult { Error, No, Yes }

#[allow(dead_code)]
#[repr(u32)]
enum ShaderBodyEnd { Plain, Prepared, BadSortPass, BadSegmentedPass }

#[repr(C)]
struct ShaderBodySink {
    context: *mut std::ffi::c_void,
    emit: unsafe extern "C" fn(*mut std::ffi::c_void, BodyStep, BodyScan, u32) -> BodyStepResult,
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_shader_body"]
    fn emit_shader_body(input: ShaderBodyInput, sink: ShaderBodySink) -> ShaderBodyEnd;
}

struct ShaderBodyEmission<'a, 'ir> {
    emitter: &'a mut Emitter<'ir>,
    inputs: &'a crate::ir::dataflow::InputFragments,
    operation: *const std::ffi::c_void,
    selection: *const Selection,
    kernel: Option<&'ir ReductionKernel>,
    segment_head: u32,
    request: ShaderPass,
    pass: EntryPass,
    single_group: bool,
    gid: u32,
    local_index: u32,
    local_input: u32,
    num_groups: u32,
    vector_type: u32,
    labels: [u32; 2],
    shared: u32,
    shared_pointer: u32,
    subgroup_inputs: [u32; 2],
    ballot_type: u32,
    local: u32,
    count: u32,
    active: u32,
    groups: [u32; 2],
    error: Option<&'static str>,
}

unsafe extern "C" fn shader_body_step(
    context: *mut std::ffi::c_void, step: BodyStep, source: BodyScan, flag: u32,
) -> BodyStepResult {
    let emission = unsafe { &mut *context.cast::<ShaderBodyEmission<'_, '_>>() };
    match emission.emit(step, source, flag != 0) {
        Ok(true) => BodyStepResult::Yes,
        Ok(false) => BodyStepResult::No,
        Err(error) => {
            emission.error = Some(error);
            BodyStepResult::Error
        }
    }
}

impl ShaderBodyEmission<'_, '_> {
    fn emit(&mut self, step: BodyStep, source: BodyScan, flag: bool) -> Result<bool, &'static str> {
        // B selects steps using the discriminant paired with this borrowed operation.
        let operation = self.operation;
        let selection = self.selection;
        let e = &mut *self.emitter;
        let [execute, end] = self.labels;
        let (local, shared, pointer, active, count) =
            (self.local, self.shared, self.shared_pointer, self.active, self.count);
        match step {
            BodyStep::DispatchArguments => {
                let prefix = PrefixCount { binding: self.request.parameter, word: self.request.word };
                return Ok(e.prefix_arguments(Some(&prefix), self.local_index));
            }
            BodyStep::SelectionArguments => return Ok(e.prefix_arguments(e.stage.prefix.as_ref(), self.local_index)),
            BodyStep::PrefixIndex => {
                (self.count, self.active, self.groups) = e.prefix_index(
                    self.gid, self.local_index, self.num_groups, self.vector_type, flag,
                );
            }
            BodyStep::LinearIndex => {
                (self.count, self.active, self.groups) = e.linear_index(self.local_index);
            }
            BodyStep::Invariants => {
                let kernel = self.kernel.unwrap();
                e.block(&kernel.stage.code[..kernel.invariant_end])?;
            }
            BodyStep::Local => self.local = e.load_pointer(e.types[&Scalar::U32], self.local_input),
            BodyStep::Open => e.open_active(active, execute, end),
            BodyStep::Merge => e.merge_active(end, flag),
            BodyStep::SelectionScatter => e.selection_scatter(unsafe { *selection }, count, active, self.groups),
            BodyStep::SortKey => {
                let sort = unsafe { &*operation.cast::<Sorting>() };
                let key = if flag { sort.keys.len() - 1 } else { self.request.parameter };
                e.sort_initialize(sort, key, flag, active);
            }
            BodyStep::SortGather => e.sort_gather(unsafe { &*operation.cast::<Sorting>() }, active),
            BodyStep::SortRank => e.sort_rank(
                unsafe { &*operation.cast::<Sorting>() }, self.request.parameter as u32, flag,
                count, &self.subgroup_inputs, self.ballot_type, shared, pointer,
            ),
            BodyStep::Scan | BodyStep::ScanAdd => {
                let counts;
                let scan = match source {
                    BodyScan::Original => unsafe { &*operation.cast::<Scan>() },
                    BodyScan::Selection => {
                        counts = Scan::counts(unsafe { *selection }.scan(e.execution_buffers.selection()));
                        &counts
                    }
                    BodyScan::Compact => {
                        counts = Scan::counts(unsafe { &*operation.cast::<Compaction>() }.scan());
                        &counts
                    }
                    BodyScan::Sort => {
                        let sort = unsafe { &*operation.cast::<Sorting>() };
                        counts = Scan::counts(Reduction {
                            source: sort.source, source_word: 0, target: sort.target, target_word: 0,
                        });
                        &counts
                    }
                };
                if matches!(step, BodyStep::Scan) {
                    e.scan(scan, flag, local, shared, pointer, active, count);
                } else {
                    e.scan_add(scan, flag);
                }
            }
            BodyStep::SegmentedScan => {
                let scan = unsafe { &*operation.cast::<Scan>() };
                e.segmented_scan(scan, self.segment_head, flag, local, shared, pointer, active, count);
            }
            BodyStep::SegmentedAdd => e.segmented_add(unsafe { &*operation.cast::<Scan>() }, active, self.labels),
            BodyStep::SegmentedRoot => e.segmented_root_add(unsafe { &*operation.cast::<Scan>() }),
            BodyStep::CustomSegmented => {
                e.custom_segmented_reduce(
                    unsafe { &*operation.cast::<CustomSegmentedReduction>() }, self.kernel.unwrap(),
                    self.pass, self.single_group, local, shared, pointer, active, count,
                )?;
            }
            BodyStep::CustomScan => {
                e.custom_scan(unsafe { &*operation.cast::<CustomScan>() }, self.kernel.unwrap(),
                    flag, local, shared, pointer, active, count)?;
            }
            BodyStep::CustomScanAdd => {
                e.custom_scan_add(unsafe { &*operation.cast::<CustomScan>() }, self.kernel.unwrap(), flag, active)?;
            }
            BodyStep::CustomReduce => {
                e.custom_reduce(unsafe { &*operation.cast::<CustomReduction>() }, self.kernel.unwrap(),
                    flag, local, shared, pointer, active, count)?;
            }
            BodyStep::Reduce => e.reduce(
                unsafe { &*operation.cast::<ReductionSpec>() }, flag, local, shared, pointer, active, count,
            ),
            BodyStep::SegmentedReduce => e.segmented_reduce(
                unsafe { &*operation.cast::<SegmentedReduction>() }, self.pass, self.single_group,
                local, shared, pointer, active, count,
            ),
            BodyStep::SegmentedReduceAdd => {
                e.segmented_reduce_add(unsafe { &*operation.cast::<SegmentedReduction>() }, active, self.labels);
            }
            BodyStep::CompactScatter => e.compact_scatter(unsafe { &*operation.cast::<Compaction>() }, count, active),
            BodyStep::TileInput => e.tile_input(count, active, self.inputs),
            BodyStep::TilePhases => {
                let phases = e.stage.tile_phases();
                if phases.is_empty() { return Ok(false); }
                e.tile_phases(active, phases)?;
            }
            BodyStep::SelectedIndex => {
                let at = e.address_scalar(Scalar::U32, e.program.buffers.len(), e.index, 0);
                e.index = e.load_aligned_word(at);
            }
            BodyStep::Map => return e.block(&e.stage.code),
        }
        Ok(true)
    }
}

fn emit_shader(
    program: &Program, original_stage: &Stage, request: ShaderPass,
    inputs: &crate::ir::dataflow::InputFragments, layout: &PlacedTiles,
    program_features: ProgramShaderFeatures, reduction: Option<&PreparedReduction>,
    shared_original_facts: Option<&ShaderStageFacts>, publishes_summary: bool,
) -> Result<(Vec<u32>, u64), &'static str> {
    let single_group = request.kind == EntryPass::SegmentedSingleGroup;
    let pass = if single_group { EntryPass::ScanRootAdd } else { request.kind };
    let argument_pass = request.arguments != 0;
    let numeric_pass = request.numeric != 0;
    let local_facts;
    let (stage, tile_layout, facts, custom_kernel) = match reduction {
        Some(prepared) => (&prepared.kernel.stage, &prepared.layout, &prepared.facts, Some(&prepared.kernel)),
        None => {
            let facts = match shared_original_facts {
                Some(shared) => shared,
                None => {
                    local_facts = ShaderStageFacts::new(original_stage, numeric_pass, pass == EntryPass::Main);
                    &local_facts
                }
            };
            (original_stage, layout, facts, None)
        }
    };
    let mut e = Emitter {
        next: 1,
        types: ScalarIds::default(),
        pointers: ScalarIds::default(),
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
        physical_pointers: ScalarIds::default(),
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
        subgroup_inputs: [0; 3],
        ballot_type: 0,
        workgroup_width: stage.workgroup_width(),
        tile_layout: &tile_layout.words,
        tile_shared64: 0,
        tile_pointer64: 0,
        tile_shared: 0,
        tile_pointer: 0,
        tile_data_words: 0,
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
    let (operation, operation_data) = entry_operation(&stage.operation);
    let workgroup = unsafe {
        workgroup_layout(operation, request.kind, u32::from(stage.selection.is_some()), summary_words, SCAN_WIDTH)
    };
    let scan_local = workgroup.scan;
    let ranks = workgroup.ranks != 0;
    let empty_subgroups = Default::default();
    let subgroups = if pass == EntryPass::Main { &facts.code.subgroups } else { &empty_subgroups };
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
    let void = e.declare_scalars(float64, integer8, integer16);
    let u = e.types[&Scalar::U32];
    e.u64_type = e.types[&Scalar::U64];
    let resources = e.declare_shader_resources(publishes_summary, single_group, argument_pass);
    let ShaderResourceIds { ray_pointer, index_type: v3, global, num_groups, .. } = resources;
    let (narrow_cells, wide_cells, scratch_words) = if pass == EntryPass::Main {
        (tile_layout.extents.narrow as u64, tile_layout.extents.wide as u64, stage.tile_scratch_bytes() / 4)
    } else {
        (0, 0, 0)
    };
    let groups = e.declare_workgroup(WorkgroupDeclaration {
        layout: workgroup, next: e.next, word_type: u, wide_type: e.u64_type,
        subgroup_mask: u32::from(subgroups.uses_builtin(SubgroupOp::Lane))
            | (u32::from(subgroups.uses_builtin(SubgroupOp::Size)) << 1)
            | (u32::from(subgroups.uses_builtin(SubgroupOp::Id)) << 2),
        ballot: u32::from(subgroup_caps.ballot || ranks),
        width: e.workgroup_width,
        narrow_cells, wide_cells, scratch_words,
    })?;
    let local_input = groups.local;
    let subgroup_inputs = [groups.rank_id, groups.rank_lane];
    let ballot_type = groups.ballot;
    let shared = groups.reduction.variable;
    let shared_pointer = groups.reduction.pointer;
    let shared_bytes = groups.shared_bytes;
    let fn_ty = e.declare_type(TypeShape::Function, void, 0);
    let main = e.id();
    let entry = e.id();
    let execute = e.id();
    let end = e.id();
    let finish = |mut e: Emitter<'_>, prepare: bool| {
        e.emit_control(2, |body| unsafe { end_shader_body(body) });
        let global_inputs = [global, num_groups];
        let workgroup_inputs = [e.push, shared, local_input];
        let tile_inputs = [e.tile_shared, e.tile_shared64];
        let parts: [&[u32]; 8] = [
            &global_inputs, &e.buffers, &e.accelerations, &e.textures,
            &workgroup_inputs, &subgroup_inputs, &e.subgroup_inputs, &tile_inputs,
        ];
        let interface_words: usize = parts.iter().map(|part| part.len()).sum();
        let interfaces = parts.map(|part| ShaderWords {
            data: part.as_ptr(),
            count: part.len() as u64,
        });
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
        e.declare_function_storage(ray_pointer, facts);
    }
    let gid = e.id();
    let local_index = e.id();
    e.emit_body(9, |body| unsafe { read_shader_index(v3, u, global, gid, local_index, body) });
    let selection = stage.selection.as_ref().map_or(std::ptr::null(), std::ptr::from_ref);
    let segment_head = match &stage.operation {
        Operation::ScanExclusive(scan) => scan.head_word,
        _ => None,
    };
    let input = ShaderBodyInput {
        operation, pass,
        selection: u32::from(!selection.is_null()), scan_local,
        segmented_scan: u32::from(segment_head.is_some()),
        invariants: u32::from(custom_kernel.is_some()),
        tile_scope: u32::from(stage.has_tile_scope()),
        prefix_index: u32::from(num_groups != 0),
    };
    let mut emission = ShaderBodyEmission {
        emitter: &mut e, inputs, operation: operation_data, selection,
        kernel: custom_kernel, segment_head: segment_head.unwrap_or_default(),
        request, pass, single_group, gid, local_index, local_input,
        num_groups, vector_type: v3, labels: [execute, end], shared, shared_pointer,
        subgroup_inputs, ballot_type, local: 0, count: 0, active: 0, groups: [0; 2],
        error: None,
    };
    let sink = ShaderBodySink { context: std::ptr::from_mut(&mut emission).cast(), emit: shader_body_step };
    let result = unsafe { emit_shader_body(input, sink) };
    if let Some(error) = emission.error { return Err(error); }
    match result {
        ShaderBodyEnd::Plain => finish(e, false),
        ShaderBodyEnd::Prepared => finish(e, true),
        ShaderBodyEnd::BadSortPass => Err("排序操作不支持该代码生成阶段"),
        ShaderBodyEnd::BadSegmentedPass => Err("分段扫描不支持该代码生成阶段"),
    }
}
