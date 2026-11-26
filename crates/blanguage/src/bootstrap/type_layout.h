module bootstrap.type_layout;
import codec.type_layout;

enum StorageMode:u32 { Borrow=0; CleanupBorrow=1; CleanupRequirement=2; };
enum StorageNeed:u32 {
    Absent=0; Present=1; Enter=2; Contract=3; Field=4; Variant=5; Array=6; Dataset=7;
};
struct StorageCursor { usize next; StorageMode mode; };
struct StorageStep { usize index; StorageNeed need; };
extern "C" StorageStep storage_step(inout StorageCursor cursor, read TypeNode node,
    u32 empty_dataset, u32 found) on cpu;

enum ReferenceNeed:u32 {
    Done=0; Enter=1; Signature=2; Parameter=3; Result=4;
    Argument=5; Field=6; Variant=7; Array=8; View=9; Dataset=10;
};
struct ReferenceCursor { usize next; u32 phase; };
struct ReferenceShape {
    usize arguments; usize fields; usize variants; usize parameters;
    u32 signature; u32 result; u32 array; u32 view; u32 dataset;
};
struct ReferenceStep { usize index; ReferenceNeed need; };
extern "C" ReferenceStep reference_step(inout ReferenceCursor cursor, read ReferenceShape shape) on cpu;

extern "C" usize dataset_count(address value, TypeHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.child);
extern "C" u32 dataset_path(address value, usize ordinal, TypeHost host, address context,
    cfn<void,address,usize> push) on cpu
    requires nonzero(host.inspect) requires nonzero(host.child) requires nonzero(push);

struct DatasetPathsHost {
    TypeHost types;
    cfn<void,address,usize> push;
    cfn<void,address> pop;
    cfn<void,address> emit;
};
extern "C" void dataset_paths(address value, address context, DatasetPathsHost host) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.push) requires nonzero(host.pop) requires nonzero(host.emit);

extern "C" void borrow_fields(address value, u32 base, FieldHost host, address context,
    cfn<void,address,u32,address> emit) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.part) requires nonzero(emit);

extern "C" void snapshot_addresses(address value, FieldHost host, address context,
    cfn<void,address,u32,address> emit) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.part) requires nonzero(emit);

struct MetadataRange { u32 start; u32 end; };
struct SnapshotColumnHost {
    cfn<u32,address,usize> ensures memory(read) word;
    cfn<MetadataRange,address,usize> ensures memory(read) metadata;
};
extern "C" void select_snapshot_columns(address columns, usize count, usize ranges, u32 write,
    address context, SnapshotColumnHost host) on cpu
    requires nonzero(host.word) requires nonzero(host.metadata);

extern "C" u32 zero_valid(address value, TypeHost host,
    cfn<u32,address,usize> ensures memory(read) discriminant) on cpu
    requires nonzero(host.inspect) requires nonzero(host.child) requires nonzero(discriminant);

enum MemberKind:u32 {
    Missing=0;
    Variant=1;
    HiddenView=2;
    Length=3;
    Declared=4;
    Swizzle=5;
    UnknownComponent=6;
    ComponentBounds=7;
};
struct MemberSelection { MemberKind kind; u32 count; TypePart part; array<u32,4> lanes; };
struct MemberHost {
    cfn<TypeNode,address> ensures memory(read) inspect;
    cfn<PathEntry,address,Child,usize> ensures memory(read) entry;
    cfn<usize,address> ensures memory(read) swizzle_width;
};
extern "C" MemberSelection source_member(address value, Text name, MemberHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.entry) requires nonzero(host.swizzle_width);

struct VariantMember { u32 present; u32 discriminant; };
extern "C" VariantMember declared_variant(address value, Text name, usize count,
    cfn<PathEntry,address,Child,usize> ensures memory(read) entry) on cpu requires nonzero(entry);
