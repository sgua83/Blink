module bootstrap.application_snapshot;
import bootstrap.ir_dataflow;

enum SnapshotTruth:u32 { Unknown=0; Yes=1; No=2; };
struct SnapshotSource { address context; usize blocks; usize instructions; Span registers; };
struct RegisterHost {
    Host ir;
    cfn<Span,address,usize> ensures memory(read) block;
    cfn<u32,address,address,usize> writes_local;
    cfn<u32,address,address,address> writes_load;
    cfn<LoopEffects,address,usize> ensures memory(read) loop_effects;
    cfn<u32,address,usize> ensures memory(read) loop_local;
    cfn<LoopStorage,address> ensures memory(read) loop_storage;
    cfn<u32,address,usize> ensures memory(read) scalar_bytes;
    cfn<Word,address,usize,usize,usize> cast;
};
struct SnapshotHost {
    RegisterHost registers;
    address truth_context;
    cfn<SnapshotTruth,address,usize> truth;
};

extern "C" RegisterDefinition application_register_definition(SnapshotSource source, RegisterHost host, usize reg) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block);
extern "C" Word application_register_constant(SnapshotSource source, RegisterHost host, usize reg) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local) requires nonzero(host.cast);
extern "C" u32 application_register_equal(SnapshotSource source, RegisterHost host, usize left, usize right) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load);

// The result names an existing read in the borrowed instruction blocks.
extern "C" usize read_snapshot(SnapshotSource source, SnapshotHost host, usize register) on cpu
    requires nonzero(host.registers.ir.inspect) requires nonzero(host.registers.block)
    requires nonzero(host.registers.writes_local) requires nonzero(host.registers.loop_effects) requires nonzero(host.registers.loop_local)
    requires nonzero(host.registers.loop_storage) requires nonzero(host.registers.scalar_bytes)
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
struct LoopStorage { usize root; ApplicationByteRange range; u32 descendants; u32 has_range; };
struct OriginalAddress { usize low; usize high; };
struct IndexedByteAccess { Node source; u32 word; u32 width; };
struct OffsetPosition { u32 present; OriginalAddress base; usize index; usize stride; usize offset; };

extern "C" Word application_frame_root(SnapshotSource source, RegisterHost host, OriginalAddress address_parts) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block);
extern "C" u32 application_allocated(SnapshotSource source, RegisterHost host, usize root) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block);
extern "C" Word application_split_address(SnapshotSource source, RegisterHost host, OriginalAddress address_parts) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load);
extern "C" OffsetPosition application_offset_position(SnapshotSource source, RegisterHost host, OriginalAddress address_parts) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load);
extern "C" u32 application_same_position(SnapshotSource source, RegisterHost host, Node left, Node right, u32 exact_index) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load);

extern "C" ApplicationByteRange application_byte_range(usize index, usize stride, u32 word, u32 width) on cpu;
extern "C" u32 application_storage_includes_root(SnapshotSource source, RegisterHost host, LoopStorage written, usize root) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.writes_load) requires nonzero(host.cast)
    requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes);
extern "C" u32 application_storage_changes(SnapshotSource source, RegisterHost host, LoopEffects effects,
    Word root, ApplicationByteRange range, u32 has_range) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.writes_load) requires nonzero(host.cast)
    requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes);

extern "C" u32 application_store_overlap(IndexedByteAccess read, IndexedByteAccess written,
    Span offset, SnapshotSource source, RegisterHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.writes_local) requires nonzero(host.loop_effects) requires nonzero(host.loop_local)
    requires nonzero(host.loop_storage) requires nonzero(host.scalar_bytes) requires nonzero(host.cast)
    requires nonzero(host.writes_load);
