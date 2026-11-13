//! 按 SPIR-V 1.4 标准直接编码；不调用 HLSL/GLSL 或第三方编译器。
use crate::ir::*;
use crate::builtin_types::{ScalarBinaryOp, ScalarCode};
use super::{PAGE_BYTES, PREFIX_LEVELS, ProgramImage, ShaderCode, StageCode};
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
use std::collections::BTreeMap;

fn op(out: &mut Vec<u32>, code: u16, args: &[u32]) {
    out.push(((args.len() as u32 + 1) << 16) | code as u32);
    out.extend_from_slice(args);
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
    SampledImage,
    Array,
    Function,
    Acceleration,
    RayQuery,
    Image,
    Pointer,
}

#[repr(C)]
struct ResourceBlock {
    element: u32,
    length: u32,
    array: u32,
    block: u32,
    pointer: u32,
}

#[repr(u32)]
enum StorageClass {
    UniformConstant = 0,
    Input = 1,
    Workgroup = 4,
    Function = 7,
    PushConstant = 9,
    StorageBuffer = 12,
    PhysicalStorageBuffer = 5349,
}

#[repr(u32)]
enum VariableAnnotation {
    Descriptor,
    PageTable,
    BuiltIn,
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

#[repr(C)]
#[derive(Clone, Copy)]
struct LoopLabels {
    header: u32,
    check: u32,
    run: u32,
    next: u32,
    end: u32,
}

#[repr(C)]
struct LoopReturn {
    bool_type: u32,
    result: u32,
    target: u32,
    merge: u32,
    not_returned: u32,
    returned: u32,
    edge_count: u32,
}

#[repr(C)]
struct SubgroupEmission {
    operation: u32,
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
    levels: u32,
    collective: u32,
    bins: u32,
    next: u32,
}

impl DispatchGrid {
    fn word_capacity(&self) -> usize {
        // Page header, optional histogram count and up to 167 words per record.
        25 + 167 * self.levels as usize * (1 + usize::from(self.bins != 0))
    }
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
    #[link_name = "bootstrap.spirv_envelope::encode_sort_rank"]
    fn encode_sort_rank(input: SortRankEmission, body: *mut u32) -> StageWords;
    #[link_name = "bootstrap.spirv_envelope::encode_dispatch_grid"]
    fn encode_dispatch_grid(input: DispatchGrid, offsets: *const u32, body: *mut u32) -> DispatchValue;
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
    #[link_name = "bootstrap.spirv_envelope::resource_block"]
    fn resource_block(input: ResourceBlock, definitions: *mut u32, decorations: *mut u32) -> u64;
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
    #[link_name = "bootstrap.spirv_envelope::payload_read"]
    fn payload_read(input: PayloadEmission, operands: *const u32, shifts: *const u32, body: *mut u32) -> PayloadRead;
    #[link_name = "bootstrap.spirv_envelope::payload_write"]
    fn payload_write(
        input: PayloadEmission, value: u32, pointers: *const u32, shifts: *const u32,
        output: *mut u32, body: *mut u32,
    ) -> PayloadWrite;
    #[link_name = "bootstrap.spirv_envelope::open_selection"]
    fn open_selection(merge: u32, condition: u32, yes: u32, no: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::next_control_block"]
    fn next_control_block(target: u32, label: u32, reaches: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::close_selection"]
    fn close_selection(merge: u32, yes_reaches: u32, no_reaches: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::open_loop"]
    fn open_loop(labels: LoopLabels, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::test_loop"]
    fn test_loop(labels: LoopLabels, test: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::close_loop"]
    fn close_loop(labels: LoopLabels, reaches: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::open_function_scope"]
    fn open_function_scope(merge: u32, selector: u32, begin: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::leave_control"]
    fn leave_control(target: u32, body: *mut u32) -> ControlWords;
    #[link_name = "bootstrap.spirv_envelope::dispatch_loop_return"]
    fn dispatch_loop_return(input: LoopReturn, edges: *const returns::ReturnEdge, body: *mut u32) -> ControlWords;
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
    texture_types: BTreeMap<crate::ast::TextureDimension, (u32, u32)>,
    vec4_type: u32,
    acceleration_type: u32,
    ray_variables: BTreeMap<Reg, u32>,
    ray_position_type: u32,
    ray_transform_type: u32,
    position_queries: std::collections::BTreeSet<Reg>,
    transform_queries: [std::collections::BTreeSet<Reg>; 2],
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
    ext: u32,
    subgroup_inputs: BTreeMap<SubgroupOp, u32>,
    ballot_type: u32,
    workgroup_width: u32,
    tile_layout: BTreeMap<SharedWord, (Scalar, usize, u32)>,
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
    fn declare_resource_block(&mut self, element: u32, length: u32) -> u32 {
        let input = ResourceBlock {
            element, length,
            array: self.id(), block: self.id(), pointer: self.id(),
        };
        let pointer = input.pointer;
        let defs = self.defs.len();
        let annotations = self.decorations.len();
        self.defs.reserve(11);
        self.decorations.reserve(12);
        unsafe {
            let used = resource_block(input, self.defs.as_mut_ptr().add(defs),
                self.decorations.as_mut_ptr().add(annotations));
            self.defs.set_len(defs + used as usize);
            self.decorations.set_len(annotations + 12);
        }
        pointer
    }
    fn emit(&mut self, code: u16, args: &[u32]) {
        if code == 248 {
            self.current_label = args[0];
        }
        op(&mut self.body, code, args);
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

    fn emit_control(&mut self, capacity: usize, encode: impl FnOnce(*mut u32) -> ControlWords) -> bool {
        let at = self.body.len();
        self.body.reserve(capacity);
        let emitted = encode(unsafe { self.body.as_mut_ptr().add(at) });
        unsafe { self.body.set_len(at + emitted.words as usize); }
        if emitted.label != 0 {
            self.current_label = emitted.label;
        }
        emitted.reaches_next != 0
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
                InstKind::TileLength(dst) => {
                    self.emit(83, &[self.ty(*dst), self.value(*dst), self.tile_length])
                }
                InstKind::TileLane(dst) => {
                    self.emit(83, &[self.ty(*dst), self.value(*dst), self.tile_lane])
                }
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
                InstKind::Index(dst) => self.emit(83, &[self.ty(*dst), self.value(*dst), self.index]),
                InstKind::Uniform(dst, offset) => {
                    let zero = self.constant(Scalar::U32, 0);
                    let mut words = Vec::new();
                    for part in 0..self.stage.registers[*dst].size().div_ceil(4) {
                        let word = self.constant(Scalar::U32, offset / 4 + part);
                        let ptr = self.id();
                        self.emit(65, &[self.ptr_push_u32, ptr, self.push, zero, word]);
                        let mut raw = self.id();
                        self.emit(61, &[self.types[&Scalar::U32], raw, ptr, 2, 4]);
                        if offset % 4 != 0 {
                            let shift = self.constant(Scalar::U32, offset % 4 * 8);
                            let shifted = self.id();
                            self.emit(194, &[self.types[&Scalar::U32], shifted, raw, shift]);
                            raw = shifted;
                        }
                        words.push(raw);
                    }
                    self.from_words(*dst, &words, Scalar::U32);
                }
                InstKind::LoadLocal(dst, slot) => {
                    let pointer = self.locals[*slot];
                    if pointer == 0 {
                        self.load_private(*dst, *slot, None, 0);
                    } else {
                        self.emit(61, &[self.ty(*dst), self.value(*dst), pointer]);
                    }
                }
                InstKind::StoreLocal(slot, src) => {
                    let pointer = self.locals[*slot];
                    if pointer == 0 {
                        self.store_private(*src, *slot, None, 0);
                    } else {
                        self.emit(62, &[pointer, self.value(*src)]);
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
                    let yes_label = self.id();
                    let no_label = self.id();
                    let merge = self.id();
                    let condition = self.value(*condition);
                    self.emit_control(9, |body| unsafe { open_selection(merge, condition, yes_label, no_label, body) });
                    let yes_reaches = u32::from(self.block(yes, loop_targets)?);
                    self.emit_control(4, |body| unsafe { next_control_block(merge, no_label, yes_reaches, body) });
                    let no_reaches = u32::from(self.block(no, loop_targets)?);
                    if !self.emit_control(4, |body| unsafe { close_selection(merge, yes_reaches, no_reaches, body) }) {
                        return Ok(false);
                    }
                }
                InstKind::Loop { condition, test, body } => {
                    let labels = LoopLabels {
                        header: self.id(),
                        check: self.id(),
                        run: self.id(),
                        next: self.id(),
                        end: self.id(),
                    };
                    let targets = Some((labels.end, labels.next));
                    let returning = self.begin_return_loop(body, labels.end);
                    self.emit_control(12, |words| unsafe { open_loop(labels, words) });
                    self.block(condition, targets)?;
                    self.loop_exit_edge(labels.end, self.current_label);
                    let test = self.value(*test);
                    self.emit_control(6, |words| unsafe { test_loop(labels, test, words) });
                    let reaches = u32::from(self.block(body, targets)?);
                    self.emit_control(8, |words| unsafe { close_loop(labels, reaches, words) });
                    if returning {
                        self.finish_return_loop();
                    }
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
    fn from_words(&mut self, dst: Reg, words: &[u32], unit: Scalar) {
        let scalar = self.stage.registers[dst];
        let raw = self.join_words(scalar, words, unit);
        if scalar == Scalar::Bool {
            let zero = self.constant(Scalar::U32, 0);
            self.emit(171, &[self.ty(dst), self.value(dst), raw, zero]);
        } else {
            self.emit(if scalar == Self::raw_scalar(scalar) { 83 } else { 124 }, &[self.ty(dst), self.value(dst), raw]);
        }
    }
    fn pack_words(&mut self, size: u32, components: impl IntoIterator<Item = (Scalar, u32, u32)>) -> Vec<u32> {
        let zero = self.constant(Scalar::U32, 0);
        let mut values = vec![zero; size.div_ceil(4) as usize];
        for (scalar, offset, value) in components {
            for (part, mut word) in self.split_words(scalar, value, Scalar::U32).into_iter().enumerate() {
                let at = (offset / 4) as usize + part;
                if offset % 4 != 0 {
                    let shift = self.constant(Scalar::U32, offset % 4 * 8);
                    let shifted = self.id();
                    self.emit(196, &[self.types[&Scalar::U32], shifted, word, shift]);
                    word = shifted;
                }
                if values[at] == zero { values[at] = word; } else {
                    let joined = self.id();
                    self.emit(197, &[self.types[&Scalar::U32], joined, values[at], word]);
                    values[at] = joined;
                }
            }
        }
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
        if scalar == Scalar::Bool {
            let bits = self.id();
            let one = self.constant(Scalar::U32, 1);
            let zero = self.constant(Scalar::U32, 0);
            self.select(self.types[&Scalar::U32], bits, value, one, zero);
            bits
        } else if scalar != Self::raw_scalar(scalar) {
            let bits = self.id();
            self.emit(124, &[self.raw_type(scalar), bits, value]);
            bits
        } else {
            value
        }
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
        let positions = self.position_queries.contains(&hit);
        let object_to_world = self.transform_queries[0].contains(&hit);
        let world_to_object = self.transform_queries[1].contains(&hit);
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
            query: self.ray_variables[&hit],
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
fn ray_results(code: &[Inst], out: &mut Vec<Reg>) {
    for i in code {
        match &(i).kind {
            InstKind::RayQuery { result, .. } => out.push(result.registers()[0]),
            InstKind::Scope(body) => ray_results(body, out),
            InstKind::If { yes, no, .. } => {
                ray_results(yes, out);
                ray_results(no, out);
            }
            InstKind::Loop {
                condition, body, ..
            } => {
                ray_results(condition, out);
                ray_results(body, out);
            }
            _ => {}
        }
    }
}
pub fn emit_pass(
    program: &Program,
    stage_index: usize,
    pass: Pass,
) -> Result<Vec<u32>, &'static str> {
    emit_shader(program, stage_index, pass).map(|(words, _)| words)
}
fn emit_shader(
    program: &Program, stage_index: usize, pass: Pass,
) -> Result<(Vec<u32>, u64), &'static str> {
    fn oversized_view(ty: &Ty) -> bool {
        if ty.has_variants() {
            ty.variants.iter().any(|variant| oversized_view(&variant.ty))
        } else if let Some(element) = &ty.view {
            element.size > PAGE_BYTES
        } else if let Some(element) = &ty.array {
            ty.values[0] != 0 && oversized_view(element)
        } else {
            ty.fields.iter().any(|(_, _, field)| oversized_view(field))
        }
    }
    if program.buffers.iter().any(|b| b.stride > PAGE_BYTES || oversized_view(&b.ty)) {
        return Err("GPU 页寻址要求单条记录能容纳于一个物理页");
    }
    let stage = &program.stages[stage_index];
    let argument_pass = matches!(pass, Pass::DispatchArguments(_) | Pass::SelectionArguments);
    let custom_kernel =
        if !argument_pass
            && let Operation::CustomReduce(r)
        | Operation::CustomScan(CustomScan { values: r, .. })
        | Operation::CustomSegmentedReduce(CustomSegmentedReduction { values: r, .. }) =
            &stage.operation
        {
            Some(r.kernel(
                stage,
                pass == Pass::Main
                    || matches!(stage.operation, Operation::CustomSegmentedReduce(_))
                        && pass == Pass::ScanRootAdd,
                pass != Pass::ScanAdd
                    && (pass != Pass::ScanRootAdd
                        || matches!(stage.operation, Operation::CustomSegmentedReduce(_))),
            ))
        } else {
            None
        };
    let stage = custom_kernel.as_ref().map(|k| &k.stage).unwrap_or(stage);
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
        texture_types: BTreeMap::new(),
        vec4_type: 0,
        acceleration_type: 0,
        ray_variables: BTreeMap::new(),
        ray_position_type: 0,
        ray_transform_type: 0,
        position_queries: stage.ray_result_queries(6..15),
        transform_queries: [
            stage.ray_result_queries(15..27),
            stage.ray_result_queries(27..39),
        ],
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
        ext: 0,
        subgroup_inputs: BTreeMap::new(),
        ballot_type: 0,
        workgroup_width: stage.workgroup_width(),
        tile_layout: stage.tile_layout(true),
        tile_shared64: 0,
        tile_pointer64: 0,
        tile_shared: 0,
        tile_pointer: 0,
        tile_length: 0,
        tile_lane: 0,
        current_label: 0,
        return_scopes: Vec::new(),
    };
    let requirements = program.requirements();
    let (atomic64, shared_atomic64) = stage.atomic64_requirements();
    let float64 = requirements.float64;
    let integer8 = requirements.integer8;
    let integer16 = requirements.integer16;
    let numeric_pass = matches!(
        pass, Pass::Main | Pass::ReductionTail | Pass::ScanTail | Pass::ScanAdd | Pass::ScanRootAdd
    );
    let float32_preserve = numeric_pass && stage.requires_float_preserve(Scalar::F32);
    let float64_preserve = numeric_pass && stage.requires_float_preserve(Scalar::F64);
    let ranks = matches!(pass, Pass::SortHistogram(_) | Pass::SortScatter(_));
    let subgroup_operations = if pass == Pass::Main {
        stage.subgroup_operations()
    } else {
        Default::default()
    };
    let subgroup_caps = SubgroupCapabilities::from_operations(subgroup_operations.iter().copied());
    let features = ShaderFeatures {
        atomic64: u32::from(atomic64 || shared_atomic64),
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
        acceleration: u32::from(!program.accelerations.is_empty()),
        position_fetch: u32::from(!e.position_queries.is_empty()),
        descriptor_indexing: u32::from(program.requires_texture_indexing()),
        maximal_reconvergence: u32::from(pass == Pass::Main && stage.requires_maximal_reconvergence()),
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
    let f = e.types[&Scalar::F32];
    e.vec3_type = e.declare_type(TypeShape::Vector, f, 3);
    e.vec2_type = e.declare_type(TypeShape::Vector, f, 2);
    if !program.textures.is_empty() {
        e.vec4_type = e.declare_type(TypeShape::Vector, f, 4);
        for i in 0..program.textures.len() {
            let dimension = program.textures[i].dimension;
            if !e.texture_types.contains_key(&dimension) {
                let image = e.declare_type(TypeShape::Image, f, dimension.components() as u32);
                let sampled = e.declare_type(TypeShape::SampledImage, image, 0);
                let pointer = e.declare_pointer(StorageClass::UniformConstant, sampled);
                e.texture_types.insert(dimension, (sampled, pointer));
            }
            let (sampled, pointer) = e.texture_types[&dimension];
            let pointer = if let Some(count) = program.textures[i].count {
                let length = e.constant(Scalar::U32, count);
                let array = e.declare_type(TypeShape::Array, sampled, length);
                e.declare_pointer(StorageClass::UniformConstant, array)
            } else { pointer };
            let binding = (program.buffers.len() + program.accelerations.len() + i) as u32;
            let id = e.declare_annotated(VariableAnnotation::Descriptor, pointer, binding);
            e.textures.push(id);
        }
    }
    let mut ray_pointer = 0;
    if !program.accelerations.is_empty() {
        e.acceleration_type = e.declare_type(TypeShape::Acceleration, 0, 0);
        let pointer = e.declare_pointer(StorageClass::UniformConstant, e.acceleration_type);
        for i in 0..program.accelerations.len() {
            let binding = (program.buffers.len() + i) as u32;
            let id = e.declare_annotated(VariableAnnotation::Descriptor, pointer, binding);
            e.accelerations.push(id);
        }
        let query_type = e.declare_type(TypeShape::RayQuery, 0, 0);
        ray_pointer = e.declare_pointer(StorageClass::Function, query_type);
    }
    e.u64_type = e.types[&Scalar::U64];
    let v3 = e.declare_type(TypeShape::Vector, u, 3);
    let ptr_input = e.declare_pointer(StorageClass::Input, v3);
    let global = e.declare_annotated(VariableAnnotation::BuiltIn, ptr_input, 28);
    let mut num_groups = 0;
    if (stage.prefix.is_some() || stage.selection.is_some())
        && !argument_pass
    {
        num_groups = e.declare_annotated(VariableAnnotation::BuiltIn, ptr_input, 24);
    }
    let ptr = e.declare_resource_block(e.u64_type, 0);
    for scalar in [Scalar::U32, Scalar::U64, Scalar::U8, Scalar::U16] {
        if let Some(ty) = e.types.get(&scalar).copied() {
            let pointer = e.declare_pointer(StorageClass::PhysicalStorageBuffer, ty);
            e.physical_pointers.insert(scalar, pointer);
        }
    }
    e.ptr_table_u64 = e.declare_pointer(StorageClass::StorageBuffer, e.u64_type);
    for i in 0..program.buffers.len() + program.auxiliary_buffers() {
        let binding = if i >= program.buffers.len() {
            (i + program.accelerations.len() + program.textures.len()) as u32
        } else {
            i as u32
        };
        let id = e.declare_annotated(VariableAnnotation::PageTable, ptr, binding);
        e.buffers.push(id);
    }
    let length = e.constant(Scalar::U32, program.uniform_bytes / 4);
    let push_ptr = e.declare_resource_block(u, length);
    e.push = e.declare_variable(StorageClass::PushConstant, push_ptr);
    e.ptr_push_u32 = e.declare_pointer(StorageClass::PushConstant, u);
    let scan_local = (matches!(
        stage.operation,
        Operation::ScanExclusive(_) | Operation::CustomScan(_) | Operation::Compact(_)
    ) && matches!(pass, Pass::Main | Pass::ScanTail))
        || (stage.selection.is_some() && matches!(pass, Pass::SelectionScan | Pass::ScanTail))
        || (matches!(stage.operation, Operation::StableSort(_))
            && matches!(pass, Pass::SortScan | Pass::ScanTail))
        || (matches!(
            stage.operation,
            Operation::SegmentedReduce(_) | Operation::CustomSegmentedReduce(_)
        ) && matches!(pass, Pass::Main | Pass::ScanTail | Pass::ScanRootAdd));
    let reduction = (matches!(
        stage.operation,
        Operation::Reduce(_) | Operation::CustomReduce(_)
    ) && matches!(pass, Pass::Main | Pass::ReductionTail))
        || scan_local;
    let shared_scalar = Scalar::U32;
    let mut local_input = 0;
    let mut shared = 0;
    let mut shared_pointer = 0;
    if reduction {
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
        if subgroup_operations.contains(&operation) {
            let input_ptr = e.declare_pointer(StorageClass::Input, u);
            let variable = e.declare_annotated(VariableAnnotation::BuiltIn, input_ptr, builtin);
            e.subgroup_inputs.insert(operation, variable);
        }
    }
    let mut shared_bytes = 0u64;
    if reduction || ranks {
        let words =
            if ranks {
                32
            } else if let Operation::Reduce(r) = &stage.operation {
                64 * r.ty.size.div_ceil(4)
            } else if let Operation::CustomReduce(r) = &stage.operation {
                64 * r.ty.size.div_ceil(4)
            } else if let Operation::CustomScan(r) = &stage.operation {
                64 * r.summary_words() as u32
            } else if let Operation::CustomSegmentedReduce(r) = &stage.operation {
                64 * r.summary_words() as u32
            } else if let Operation::ScanExclusive(r) = &stage.operation {
                64 * r.summary_words() as u32
            } else if let Operation::SegmentedReduce(r) = &stage.operation {
                64 * r.summary_words() as u32
            } else {
                64
            };
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
        e.emit(253, &[]);
        e.emit(56, &[]);
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
        let input = ShaderEnvelope { bound: e.next, ext: e.ext, entry: main, workgroup_width: e.workgroup_width, features };
        let mut out = Vec::with_capacity(
            unsafe { envelope_capacity() } as usize + interface_words
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
    e.emit(54, &[void, main, 0, fn_ty]);
    e.emit(248, &[entry]);
    if !argument_pass {
        e.regs = (0..stage.registers.len()).map(|_| e.id()).collect();
        let mut rays = Vec::new();
        ray_results(&stage.code, &mut rays);
        for result in rays {
            let id = e.declare_function_variable(ray_pointer);
            e.ray_variables.insert(result, id);
        }
        e.locals.resize(stage.locals.last_key_value().map_or(0, |(end, _)| *end), 0);
        for span in private_spans(&stage.code) {
            let ty = &stage.locals[&span.end];
            let size = ty.components.iter().map(|(s, _)| s.size()).min().unwrap().min(4);
            let unit = match size { 1 => Scalar::U8, 2 => Scalar::U16, _ => Scalar::U32 };
            let length = e.constant(Scalar::U32, ty.size / size);
            let array = e.declare_type(TypeShape::Array, e.types[&unit], length);
            let pointer = e.declare_pointer(StorageClass::Function, array);
            let variable = e.declare_function_variable(pointer);
            e.private_arrays.insert(span.end, PrivateArray { start: span.start, unit, variable });
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
    e.emit(61, &[v3, gid, global]);
    let local_index = e.id();
    e.emit(81, &[u, local_index, gid, 0]);
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
        let zero = e.constant(Scalar::U32, 0);
        let one = e.constant(Scalar::U32, 1);
        let base_ptr = e.id();
        e.emit(65, &[e.ptr_push_u32, base_ptr, e.push, zero, one]);
        let base = e.id();
        e.emit(61, &[u, base, base_ptr]);
        let wide = e.u64_type;
        let local_wide = e.id();
        e.emit(113, &[wide, local_wide, local_index]);
        let base_wide = e.id();
        e.emit(113, &[wide, base_wide, base]);
        let index_wide = e.id();
        e.emit(128, &[wide, index_wide, local_wide, base_wide]);
        let ptr = e.id();
        e.emit(65, &[e.ptr_push_u32, ptr, e.push, zero, zero]);
        let count = e.id();
        e.emit(61, &[u, count, ptr]);
        let count_wide = e.id();
        e.emit(113, &[wide, count_wide, count]);
        let active = e.id();
        e.emit(
            176,
            &[e.types[&Scalar::Bool], active, index_wide, count_wide],
        );
        e.index = e.id();
        e.emit(113, &[u, e.index, index_wide]);
        (count, active, [0; 2])
    };
    if let Some(kernel) = &custom_kernel {
        e.block(&kernel.stage.code[..kernel.invariant_end], None)?;
    }
    if let Some(selection) = stage.selection
        && pass != Pass::Main
    {
        if pass == Pass::SelectionScatter {
            e.selection_scatter(selection, count, active, groups);
        } else if scan_local {
            let local = e.id();
            e.emit(61, &[u, local, local_input]);
            e.scan(
                &Scan::counts(selection.scan(program.selection_buffer())),
                pass == Pass::ScanTail,
                local,
                shared,
                shared_pointer,
                active,
                count,
            );
        } else {
            e.emit(247, &[end, 0]);
            e.emit(250, &[active, execute, end]);
            e.emit(248, &[execute]);
            e.scan_add(
                &Scan::counts(selection.scan(program.selection_buffer())),
                false,
            );
            e.emit(249, &[end]);
            e.emit(248, &[end]);
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
                let local = e.id();
                e.emit(61, &[u, local, local_input]);
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
                e.emit(247, &[end, 0]);
                e.emit(250, &[active, execute, end]);
                e.emit(248, &[execute]);
                e.scan_add(
                    &Scan::counts(Reduction {
                        source: s.source,
                        source_word: 0,
                        target: s.target,
                        target_word: 0,
                    }),
                    false,
                );
                e.emit(249, &[end]);
                e.emit(248, &[end]);
            }
            _ => return Err("排序操作不支持该代码生成阶段"),
        }
    } else if let Operation::CustomSegmentedReduce(r) = &stage.operation {
        let local = if scan_local {
            let local = e.id();
            e.emit(61, &[u, local, local_input]);
            local
        } else {
            0
        };
        e.custom_segmented_reduce(
            r,
            custom_kernel.as_ref().unwrap(),
            pass,
            local,
            shared,
            shared_pointer,
            active,
            count,
        )?;
    } else if let Operation::CustomScan(r) = &stage.operation {
        let kernel = custom_kernel.as_ref().unwrap();
        if scan_local {
            let local = e.id();
            e.emit(61, &[u, local, local_input]);
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
            e.emit(247, &[end, 0]);
            e.emit(250, &[active, execute, end]);
            e.emit(248, &[execute]);
            e.custom_scan_add(r, kernel, pass == Pass::ScanRootAdd)?;
            e.emit(249, &[end]);
            e.emit(248, &[end]);
        }
    } else if let Operation::CustomReduce(r) = &stage.operation {
        let local = e.id();
        e.emit(61, &[u, local, local_input]);
        e.custom_reduce(
            r,
            custom_kernel.as_ref().unwrap(),
            pass == Pass::ReductionTail,
            local,
            shared,
            shared_pointer,
            active,
            count,
        )?;
    } else if let Operation::Reduce(r) = &stage.operation {
        let local = e.id();
        e.emit(61, &[u, local, local_input]);
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
        let local = if scan_local {
            let local = e.id();
            e.emit(61, &[u, local, local_input]);
            local
        } else {
            0
        };
        e.segmented_reduce(r, pass, local, shared, shared_pointer, active, count);
    } else if let Operation::Compact(c) = &stage.operation {
        if pass == Pass::CompactScatter {
            e.compact_scatter(c, count, active);
        } else if scan_local {
            let local = e.id();
            e.emit(61, &[u, local, local_input]);
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
            e.emit(247, &[end, 0]);
            e.emit(250, &[active, execute, end]);
            e.emit(248, &[execute]);
            e.scan_add(&Scan::counts(c.scan()), false);
            e.emit(249, &[end]);
            e.emit(248, &[end]);
        }
    } else if let Operation::ScanExclusive(r) = &stage.operation {
        if let Some(head) = r.head_word {
            if scan_local {
                let local = e.id();
                e.emit(61, &[u, local, local_input]);
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
                e.emit(247, &[end, 0]);
                e.emit(250, &[active, execute, end]);
                e.emit(248, &[execute]);
                match pass {
                    Pass::ScanAdd => e.segmented_add(r),
                    Pass::ScanRootAdd => e.segmented_root_add(r),
                    _ => return Err("分段扫描不支持该代码生成阶段"),
                }
                e.emit(249, &[end]);
                e.emit(248, &[end]);
            }
        } else {
            if scan_local {
                let local = e.id();
                e.emit(61, &[u, local, local_input]);
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
                e.emit(247, &[end, 0]);
                e.emit(250, &[active, execute, end]);
                e.emit(248, &[execute]);
                e.scan_add(r, pass == Pass::ScanRootAdd);
                e.emit(249, &[end]);
                e.emit(248, &[end]);
            }
        }
    } else {
        if stage.has_tile_scope() {
            e.tile_input(count, active);
        }
        if stage.has_tile_steps() {
            e.tile_phases(active)?;
        } else {
            e.emit(247, &[end, 0]);
            e.emit(250, &[active, execute, end]);
            e.emit(248, &[execute]);
            if stage.selection.is_some() {
                let at = e.address_scalar(Scalar::U32, program.buffers.len(), e.index, 0);
                let original = e.id();
                e.emit(61, &[u, original, at, 2, 4]);
                e.index = original;
            }
            if e.block(&stage.code, None)? {
                e.emit(249, &[end]);
            }
            e.emit(248, &[end]);
        }
    }
    finish(e)
}
