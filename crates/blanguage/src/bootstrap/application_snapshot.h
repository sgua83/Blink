module bootstrap.application_snapshot;
import bootstrap.ir_dataflow;

enum SnapshotTruth:u32 { Unknown=0; Yes=1; No=2; };
struct SnapshotSource { address context; usize blocks; usize instructions; Span registers; };
struct SnapshotHost {
    Host ir;
    cfn<Span,address,usize> ensures memory(read) block;
    cfn<RegisterDefinition,address,usize> ensures memory(read) definition;
    cfn<u32,address,address,usize> writes_local;
    cfn<SnapshotTruth,address,usize> truth;
    cfn<u32,address,address,address> writes_load;
    cfn<u32,address,address,address> same_load;
};

// The result names an existing read in the borrowed instruction blocks.
extern "C" usize read_snapshot(SnapshotSource source, SnapshotHost host, usize register) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.block)
    requires nonzero(host.definition) requires nonzero(host.writes_local)
    requires nonzero(host.truth) requires nonzero(host.writes_load) requires nonzero(host.same_load);

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
struct OriginalAddress { usize low; usize high; };
struct IndexedByteAccess { Node source; u32 word; u32 width; };
struct AddressOverlapHost {
    Host ir;
    cfn<Word,address,usize> constant;
    cfn<u32,address,usize,usize> same;
    cfn<address,address,OriginalAddress> definition;
    cfn<Word,address,OriginalAddress> frame;
    cfn<u32,address,usize> allocated;
    cfn<u32,address,usize> defined;
    cfn<u32,address,usize,usize> descendant;
};

extern "C" ApplicationByteRange application_byte_range(usize index, usize stride, u32 word, u32 width) on cpu;
extern "C" u32 application_ranges_overlap(ApplicationByteRange left, ApplicationByteRange right) on cpu;
extern "C" u32 application_store_overlap(IndexedByteAccess read, IndexedByteAccess written,
    Span offset, address context, AddressOverlapHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.constant) requires nonzero(host.same)
    requires nonzero(host.definition) requires nonzero(host.frame) requires nonzero(host.allocated)
    requires nonzero(host.defined) requires nonzero(host.descendant);
