module bootstrap.application_snapshot;
import bootstrap.ir_dataflow;
import bootstrap.source_writes;

enum SnapshotTruth:u32 { Unknown=0; Yes=1; No=2; };
struct SnapshotSource { address context; usize blocks; usize instructions; Span registers; };
struct RegisterHost {
    Host ir;
    cfn<Span,address,usize> ensures memory(read) block;
    cfn<u32,address,address,usize> writes_local;
    cfn<u32,address,address,address> writes_load;
    cfn<LoopEffects,address,usize> ensures memory(read) loop_effects;
    cfn<u32,address,usize> ensures memory(read) scalar_bytes;
    cfn<Word,address,usize,usize,usize> cast;
};
struct SnapshotHost {
    RegisterHost registers;
    address truth_context;
    cfn<SnapshotTruth,address,usize> truth;
};


extern "C" Word application_register_constant(SnapshotSource source, RegisterHost host, usize reg) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.cast);
extern "C" u32 application_register_equal(SnapshotSource source, RegisterHost host, usize left, usize right) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects)
    requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load);

// The result names an existing read in the borrowed instruction blocks.
extern "C" usize read_snapshot(SnapshotSource source, SnapshotHost host, usize register) on cpu
    requires nonzero(host.registers.ir.inspect) requires nonzero(host.registers.block)
    requires nonzero(host.registers.writes_local) requires nonzero(host.registers.loop_effects)
    requires nonzero(host.registers.scalar_bytes)
    requires nonzero(host.registers.cast) requires nonzero(host.registers.writes_load)
 requires nonzero(host.truth);

enum ApplicationCallEffect:u32 { Preserves=0; Writes=1; FunctionContract=2; };
extern "C" ApplicationCallEffect application_call_fallback(Node source, Host ir) on cpu
    requires nonzero(ir.inspect);
extern "C" u32 application_call_writes_memory(Node source, Host ir) on cpu
    requires nonzero(ir.inspect);

enum WriteQueryKind:u32 { Local=0; Indexed=1; Condition=2; };
enum WriteEffectKind:u32 { Call=0; Store=1; Reference=2; };
struct WriteQuery { address context; usize slot; WriteQueryKind kind; u32 has_slot; };
struct WriteEffect { address node; address component; WriteEffectKind kind; };
struct WriteHost { Host ir; cfn<u32,address,WriteEffect> overlaps; };

extern "C" u32 application_instruction_writes(address item, WriteQuery query, WriteHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.overlaps);
extern "C" u32 application_block_writes(Span code, WriteQuery query, WriteHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.overlaps);

struct ApplicationByteOffset { u64 low; u64 high; };
struct ApplicationByteRange { ApplicationByteOffset start; ApplicationByteOffset end; };
struct LoopEffects { address locals; Span storage; u32 external; u32 unknown_storage; };
struct OriginalAddress { usize low; usize high; };
struct IndexedByteAccess { Node source; u32 word; u32 width; };
struct DatasetFrameLayout {
    usize root;
    usize declared_stride;
    usize parameter_stride;
    usize parameter_offset;
    u32 parameter;
    u32 wide;
};
struct NestedFrameLoad { DatasetFrameLayout layout; u32 word; u32 scalar; };
struct FrameIndex { address context; cfn<u32,address,usize> matches; };
enum DatasetFramePlace:u32 { Value=0; Local=1; Other=2; };
struct StoredDatasetFrame { DatasetFrameLayout layout; LoopEffects writes; Span slots; DatasetFramePlace place; };

enum ResourceStorageNeed:u32 { Root=0; ResolveParent=1; ResolveElement=2; Complete=3; Unavailable=4; };
enum ResourceStoragePhase:u32 { Begin=0; Edges=1; Parents=2; Emit=3; };
enum ResourceEdgeKind:u32 { End=0; Inactive=1; Element=2; Other=3; };
enum ResourceParentKind:u32 { Unavailable=0; End=1; Frame=2; Descendants=3; };
struct ResourceStorageCursor {
    address request;
    usize root;
    u32 word;
    u32 indexed;
    u32 produced;
    ResourceStoragePhase phase;
};
struct ResourceParent { address ty; Span fields; usize root; ResourceParentKind kind; u32 indexed; };
struct ResourceField { address ty; u32 word; };
struct ResourceStorageHost {
    address context;
    cfn<ResourceEdgeKind,address> edge;
    cfn<ResourceParent,address> parent;
    cfn<ResourceField,address,usize> ensures memory(read) field;
    cfn<address,address> ensures memory(read) element_type;
    cfn<NestedFrameLoad,address,u32> ensures memory(read) load;
    cfn<u32,address,usize> index_matches;
    cfn<void,address,usize,u32> emit;
};

extern "C" ResourceStorageNeed application_resource_storage_step(inout ResourceStorageCursor cursor,
    SnapshotSource source, RegisterHost host, ResourceStorageHost storage, u32 root_matched, u32 unresolved) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects)
    requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load)
    requires nonzero(storage.edge) requires nonzero(storage.parent)
    requires nonzero(storage.field) requires nonzero(storage.element_type)
    requires nonzero(storage.load) requires nonzero(storage.index_matches) requires nonzero(storage.emit);
extern "C" u32 application_dataset_position(SnapshotSource source, RegisterHost host, Node source_node, StoredDatasetFrame frame, FrameIndex index) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects)
    requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load) requires nonzero(index.matches);

extern "C" Word application_frame_root(SnapshotSource source, RegisterHost host, OriginalAddress address_parts) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block);
extern "C" u32 application_allocated(SnapshotSource source, RegisterHost host, usize root) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block);


extern "C" u32 application_same_position(SnapshotSource source, RegisterHost host, Node left, Node right, u32 exact_index) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects)
    requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load);

extern "C" ApplicationByteRange application_byte_range(usize index, usize stride, u32 word, u32 width) on cpu;
extern "C" u32 application_storage_includes_root(SnapshotSource source, RegisterHost host, StorageWrite written, usize root) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.writes_load) requires nonzero(host.cast)
    requires nonzero(host.loop_effects)
    requires nonzero(host.scalar_bytes);
extern "C" u32 application_storage_changes(SnapshotSource source, RegisterHost host, LoopEffects effects,
    Word root, ApplicationByteRange range, u32 has_range) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.writes_load) requires nonzero(host.cast)
    requires nonzero(host.loop_effects)
    requires nonzero(host.scalar_bytes);

extern "C" u32 application_store_overlap(IndexedByteAccess read, IndexedByteAccess written,
    Span offset, SnapshotSource source, RegisterHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects)
    requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load);
