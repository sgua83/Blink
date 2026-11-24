//! The B driver sequences tile collective batches and tree layers in the original emitter.
use super::{scan::{SharedTree, TreeComponent}, *};
use std::ffi::c_void;

#[repr(C)]
struct TileCollectiveInput {
    results: usize,
    width: u32,
    first_word: u32,
    lane: u32,
    sum: u32,
    exclusive: u32,
}

#[repr(C)]
struct TileCollectiveHost {
    constant: unsafe extern "C" fn(*mut c_void, u32) -> u32,
    tree: unsafe extern "C" fn(*mut c_void, usize, usize, u32) -> u32,
    seed: unsafe extern "C" fn(*mut c_void, usize, usize) -> u32,
    pad: unsafe extern "C" fn(*mut c_void, usize, u32, u32, u32),
    prepare: unsafe extern "C" fn(*mut c_void),
    layer: unsafe extern "C" fn(*mut c_void, u32, u32, u32),
    root: unsafe extern "C" fn(*mut c_void, u32, u32),
    read: unsafe extern "C" fn(*mut c_void, usize, usize, u32),
    barrier: unsafe extern "C" fn(*mut c_void, u32),
}

unsafe extern "C" {
    #[link_name = "bootstrap.spirv_envelope::emit_tile_collective"]
    fn emit_tile_collective(context: *mut c_void, input: TileCollectiveInput, host: TileCollectiveHost);
}

struct TileCollectiveEmission<'a, 'b, 'c> {
    emitter: &'a mut Emitter<'b>,
    collective: &'c TileCollective,
    operator: CollectiveOperator,
    trees: Vec<SharedTree>,
    components: Vec<TreeComponent>,
}

unsafe fn emission<'a, 'b, 'c, 'd>(context: *mut c_void) -> &'a mut TileCollectiveEmission<'b, 'c, 'd> {
    unsafe { &mut *context.cast() }
}

unsafe extern "C" fn constant(context: *mut c_void, value: u32) -> u32 {
    unsafe { emission(context) }.emitter.constant(Scalar::U32, value)
}

unsafe extern "C" fn tree(context: *mut c_void, row: usize, slot: usize, base_word: u32) -> u32 {
    let state = unsafe { emission(context) };
    if slot == 0 {
        state.trees.clear();
    }
    let (_, result) = state.collective.results[row];
    let scalar = state.emitter.stage.registers[result];
    state.trees.push(SharedTree {
        shared: state.emitter.tile_shared,
        pointer: state.emitter.tile_pointer,
        base_word, scalar, operator: state.operator,
    });
    scalar.size().div_ceil(4)
}

unsafe extern "C" fn seed(context: *mut c_void, row: usize, slot: usize) -> u32 {
    let state = unsafe { emission(context) };
    let e = &mut state.emitter;
    let tree = state.trees[slot];
    let (word, _) = state.collective.results[row];
    let identity = e.raw_constant(tree.scalar, tree.operator.identity(tree.scalar));
    let active = e.id();
    let before = e.current_label;
    let read = e.id();
    let read_end = e.id();
    e.open_index_branch(active, e.tile_lane, e.tile_length, true, read, read_end);
    let value = e.tile_read(tree.scalar, state.collective.tile, e.tile_lane, word, None);
    e.merge_active(read_end, true);
    let input = e.id();
    e.join_two(e.raw_type(tree.scalar), input, [value, read], [identity, before]);
    e.tree_store(tree, e.tile_lane, input);
    identity
}

unsafe extern "C" fn pad(context: *mut c_void, slot: usize, width: u32, padded: u32, identity: u32) {
    let state = unsafe { emission(context) };
    let e = &mut state.emitter;
    let capacity = e.constant(Scalar::U32, padded);
    let size = e.constant(Scalar::U32, width);
    let index = e.id();
    let fill = e.id();
    let run = e.id();
    let end = e.id();
    let u = e.types[&Scalar::U32];
    let b = e.types[&Scalar::Bool];
    let lane = e.tile_lane;
    e.emit_control(19, |body| unsafe {
        open_tile_padding(u, b, lane, size, capacity, index, fill, run, end, body)
    });
    e.tree_store(state.trees[slot], index, identity);
    e.merge_active(end, true);
}

unsafe extern "C" fn prepare(context: *mut c_void) {
    let state = unsafe { emission(context) };
    state.components = state.emitter.tree_components(&state.trees);
}

unsafe extern "C" fn layer(context: *mut c_void, stride: u32, padded: u32, down: u32) {
    let state = unsafe { emission(context) };
    let e = &mut state.emitter;
    e.tile_tree_layer(&state.components, e.tile_lane, stride, padded, down != 0, e.tile_length);
}

unsafe extern "C" fn root(context: *mut c_void, zero: u32, last: u32) {
    let state = unsafe { emission(context) };
    let e = &mut state.emitter;
    let first = e.id();
    let run = e.id();
    let end = e.id();
    e.open_index_branch(first, e.tile_lane, zero, false, run, end);
    let last = e.constant(Scalar::U32, last);
    for tree in &state.trees {
        let identity = e.raw_constant(tree.scalar, tree.operator.identity(tree.scalar));
        e.tree_store(*tree, last, identity);
    }
    e.merge_active(end, true);
}

unsafe extern "C" fn read(context: *mut c_void, row: usize, slot: usize, index: u32) {
    let state = unsafe { emission(context) };
    let e = &mut state.emitter;
    let tree = state.trees[slot];
    let (_, result) = state.collective.results[row];
    let raw = e.tree_load(tree, index);
    let words = e.split_words(tree.scalar, raw, Scalar::U32);
    e.from_words(result, &words, Scalar::U32);
}

unsafe extern "C" fn barrier(context: *mut c_void, semantics: u32) {
    unsafe { emission(context) }.emitter.workgroup_barrier(semantics);
}

const HOST: TileCollectiveHost = TileCollectiveHost {
    constant, tree, seed, pad, prepare, layer, root, read, barrier,
};

impl Emitter<'_> {
    pub(super) fn tile_collective(&mut self, collective: &TileCollective) {
        let operator = collective.op.operator();
        let input = TileCollectiveInput {
            results: collective.results.len(),
            width: self.stage.workgroup_width(),
            first_word: self.tile_data_words,
            lane: self.tile_lane,
            sum: u32::from(operator == CollectiveOperator::Sum),
            exclusive: u32::from(collective.op.exclusive()),
        };
        let mut state = TileCollectiveEmission {
            emitter: self, collective, operator, trees: Vec::new(), components: Vec::new(),
        };
        unsafe { emit_tile_collective(std::ptr::from_mut(&mut state).cast(), input, HOST) };
    }
}
