module bootstrap.type_layout;
import bootstrap.cli;

struct VectorLayout {
    u64 size;
    u32 alignment;
};

extern "C" VectorLayout vector(u32 scalar_size, u32 scalar_alignment, u64 count) on cpu;

struct FieldPlacement {
    u64 offset;
    u64 end;
    u32 alignment;
};

extern "C" FieldPlacement place(u32 size, u32 alignment, u32 field_size, u32 field_alignment)
    on cpu requires nonzero(field_alignment);
extern "C" u64 natural_size(u32 size, u32 alignment) on cpu requires nonzero(alignment);
extern "C" bool accepts_stride(u32 size, u32 alignment, u32 stride)
    on cpu requires nonzero(alignment);

struct ArrayLayout {
    u64 size;
    u64 stride;
};

extern "C" ArrayLayout fixed_array(u32 element_size, u32 alignment, u32 count)
    on cpu requires nonzero(alignment);

struct VariantLayout {
    u64 size;
    u32 alignment;
};

extern "C" VariantLayout tagged_variant(u32 payload_size, u32 payload_alignment) on cpu;

enum Property:u32 { Dataset=0; Resources=1; Cleanup=2; Arrays=3; Views=4; Borrows=5; };
enum Child:u32 { Variant=0; Array=1; Field=2; };
struct TypeNode {
    u32 task; u32 cleanup; u32 dataset; u32 view; u32 array; u32 address;
    usize variants; usize fields; u32 count; u32 alignment;
};
struct TypeHost {
    cfn<TypeNode,address> ensures memory(read) inspect;
    cfn<address,address,Child,usize> ensures memory(read) child;
};
extern "C" u32 contains(address value, Property property, TypeHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.child);

struct TypePart { address value; u32 offset; };
struct FieldHost {
    TypeHost types;
    cfn<TypePart,address,usize> ensures memory(read) part;
};
extern "C" void borrow_fields(address value, u32 base, FieldHost host, address context,
    cfn<void,address,u32,address> emit) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.part) requires nonzero(emit);

extern "C" void task_output_owners(address value, u32 base, FieldHost host, address context,
    cfn<void,address,u32,address> emit) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.part) requires nonzero(emit);

extern "C" u32 zero_valid(address value, TypeHost host,
    cfn<u32,address,usize> ensures memory(read) discriminant) on cpu
    requires nonzero(host.inspect) requires nonzero(host.child) requires nonzero(discriminant);

struct Text { address bytes; usize length; };
struct PathEntry { Text name; TypePart part; u32 discriminant; };
struct PathHost {
    FieldHost fields;
    cfn<PathEntry,address,Child,usize> ensures memory(read) entry;
    cfn<u32,address,u32> ensures memory(read) tag;
};
extern "C" TypePart field(address value, Text path, address words, usize word_count,
    u32 active, PathHost host) on cpu
    requires nonzero(host.fields.types.inspect) requires nonzero(host.fields.part)
    requires nonzero(host.entry) requires nonzero(host.tag);

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
