module codec.type_layout;
import codec.builtin_type;
import codec.number_text;

struct MipExtent { u32 width; u32 height; u32 depth; };
extern "C" u32 mip_level_count(MipExtent base) on cpu;
extern "C" MipExtent mip_extent(MipExtent base, u32 level) on cpu;

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

enum Property:u32 { Dataset=0; Resources=1; Arrays=3; Views=4; Borrows=5; };
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

struct NumericNode {
    address name;usize length;u32 function;u32 enumeration;usize variants;
    u32 view;u32 array;usize fields;usize components;
};
struct NumericHost {
    TypeHost types;
    cfn<NumericNode,address> ensures memory(read) inspect;
    cfn<u32,address,usize> ensures memory(read) scalar;
};
extern "C" u32 numeric(address value,NumericHost host) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.inspect) requires nonzero(host.scalar);

struct TypePart { address value; u32 offset; };
struct FieldHost {
    TypeHost types;
    cfn<TypePart,address,usize> ensures memory(read) part;
};
extern "C" void task_output_owners(address value, u32 base, FieldHost host, address context,
    cfn<void,address,u32,address> emit) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.part) requires nonzero(emit);

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


u8 path_byte(Text text, usize index) on cpu;
bool same_name(Text a, Text b) on cpu;
PathEntry named_entry(address value, Text name, Child kind, usize count,
    cfn<PathEntry,address,Child,usize> ensures memory(read) entry) on cpu requires nonzero(entry);

extern "C" void linear_positions(u32 stride, u64 count, address context,
    cfn<void,address,u64,u32> emit) on cpu requires nonzero(emit);
extern "C" VectorLayout ray_hit_fields(address context,
    cfn<void,address,Text,u32,BuiltinType> emit) on cpu requires nonzero(emit);
extern "C" address field_layout_at(address value, u32 offset, address expected, PathHost host,
    cfn<u32,address,address> ensures memory(read) equal) on cpu
    requires nonzero(host.fields.types.inspect) requires nonzero(host.entry) requires nonzero(equal);

struct LengthPair { u64 left; u64 right; };
struct LengthHost {
    cfn<u64,address,u64> ensures memory(read) fixed_count;
    cfn<LengthPair,address,u64> ensures memory(read) relation;
    cfn<u64,address,u64,u64> fixed_binding;
    cfn<address,address> allocate;
};

extern "C" u32 same_length(LengthPair query, u64 relations, address context, LengthHost host) on cpu
    requires nonzero(host.fixed_count) requires nonzero(host.relation)
    requires nonzero(host.fixed_binding) requires nonzero(host.allocate);
