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
