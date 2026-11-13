module bootstrap.type_value;
import bootstrap.type_families;

// Descriptors borrow the actual semantic input. Host owns the original maps,
// sets, bound temporaries and final Ty containers for this synchronous call.
struct Text { address bytes; u64 length; };
struct Type { address value; Text name; u64 arguments; u32 flags; u32 canonical; };
struct Facts { u32 numeric; u32 integer; u64 components; Text name; };
struct Value { u32 ready; u32 value; };
struct Count { u32 ready; u32 present; u32 value; };
struct Extent { u64 size; u64 stride; };
struct Record { u32 ready; u64 types; u64 values; u64 fields; u64 variants; u32 enumeration; u32 stride; };
struct Cleanup { u32 ready; u32 ordinary; u32 cpu; u64 parameters; u32 value_parameter; };
enum Site:u32 { Root=0; Record=1; Field=2; Variant=3; CleanupParameter=4; };
enum Binding:u32 { Representation=0; Field=1; Variant=2; CleanupResult=3; CleanupParameter=4; };
enum Problem:u32 {
    Contracts=0; Texture=1; Void=2; Never=3; Tile=4; TaskArguments=5; TaskOverflow=6;
    OwnedArguments=7; ReleaseForm=8; ReleaseShape=9; DatasetView=10; OwnedCycle=11;
    ReleaseResult=12; ReleasePayload=13; ResultArguments=14; ResultType=15; ResultTile=16;
    ResultOverflow=17; ArrayArguments=18; ArrayType=19; ArrayTile=20; ArrayLength=21;
    ArrayOverflow=22; BuiltinArguments=23; RecordArguments=24; RecordType=25; RecordTile=26;
    Family=27; RecordValue=28; RecordDomain=29; RecordCycle=30; EnumScalar=31;
    VariantScalar=32; DuplicateVariantName=33; DuplicateTag=34; VariantTile=35;
    VariantOverflow=36; DuplicateField=37; FieldOverflow=38; AlignedOverflow=39;
    Stride=40; DatasetViewSpaced=41;
};

struct Host {
    cfn<void,address,Site,u64,Problem,u64> failure;
    cfn<bool,address> function;
    cfn<void,address> builtin;
    cfn<void,address> unit;
    cfn<void,address> view;
    cfn<Type,address,u64> argument;
    cfn<bool,address,Type,Site,u64> child;
    cfn<bool,address,Type,Site,u64> dataset;
    cfn<Cleanup,address,Type> cleanup;
    cfn<void,address> owner;
    cfn<bool,address> enter_cleanup;
    cfn<bool,address> specialize_cleanup;
    cfn<Type,address,Binding,u64> bind;
    cfn<void,address> leave_cleanup;
    cfn<bool,address> owner_matches;
    cfn<void,address> use_owner;
    cfn<void,address> save_payload;
    cfn<bool,address> result;
    cfn<Value,address,u64> shape_argument;
    cfn<Extent,address,u32> array_layout;
    cfn<void,address,u32,u32> array;
    cfn<u32,address,u64> component_offset;
    cfn<void,address,u64,u32> array_component;
    cfn<Record,address> record;
    cfn<Value,address,u64> family;
    cfn<void,address,u64> type_argument;
    cfn<bool,address,u64,u32> const_domain;
    cfn<void,address,u64,u32> const_argument;
    cfn<void,address> arguments_done;
    cfn<bool,address> enter_record;
    cfn<void,address> leave_record;
    cfn<void,address> simple_enum;
    cfn<void,address> begin_variants;
    cfn<bool,address,u64> unique_variant_name;
    cfn<Value,address,u64> variant_tag;
    cfn<bool,address,u32> unique_variant_tag;
    cfn<void,address,u64,u32> variant;
    cfn<bool,address> variants_done;
    cfn<void,address> begin_record;
    cfn<bool,address,u64> unique_field;
    cfn<Type,address> dataset_field;
    cfn<bool,address,Type,Site,u64> reference;
    cfn<Count,address,u64> field_count;
    cfn<void,address,Count> reference_dataset;
    cfn<bool,address,u64> field;
    cfn<bool,address> record_done;
    cfn<Value,address> record_stride;
    cfn<bool,address,u32> stride_accepts;
    cfn<void,address,u32> set_stride;
    cfn<void,address> use_record;
    cfn<Facts,address> facts;
    cfn<bool,address> views;
    cfn<bool,address,u32> task;
};

extern "C" bool resolve(Type value, BuiltinKind kind, Host host, address context) on cpu
    requires nonzero(host.failure) requires nonzero(host.function) requires nonzero(host.builtin)
    requires nonzero(host.unit) requires nonzero(host.view) requires nonzero(host.argument)
    requires nonzero(host.child) requires nonzero(host.dataset) requires nonzero(host.cleanup)
    requires nonzero(host.owner) requires nonzero(host.enter_cleanup) requires nonzero(host.specialize_cleanup)
    requires nonzero(host.bind) requires nonzero(host.leave_cleanup) requires nonzero(host.owner_matches)
    requires nonzero(host.use_owner) requires nonzero(host.save_payload) requires nonzero(host.result)
    requires nonzero(host.shape_argument) requires nonzero(host.array_layout) requires nonzero(host.array)
    requires nonzero(host.component_offset) requires nonzero(host.array_component) requires nonzero(host.record)
    requires nonzero(host.family) requires nonzero(host.type_argument) requires nonzero(host.const_domain)
    requires nonzero(host.const_argument) requires nonzero(host.arguments_done) requires nonzero(host.enter_record)
    requires nonzero(host.leave_record) requires nonzero(host.simple_enum) requires nonzero(host.begin_variants)
    requires nonzero(host.unique_variant_name) requires nonzero(host.variant_tag) requires nonzero(host.unique_variant_tag)
    requires nonzero(host.variant) requires nonzero(host.variants_done) requires nonzero(host.begin_record)
    requires nonzero(host.unique_field) requires nonzero(host.dataset_field) requires nonzero(host.reference)
    requires nonzero(host.field_count) requires nonzero(host.reference_dataset) requires nonzero(host.field)
    requires nonzero(host.record_done) requires nonzero(host.record_stride) requires nonzero(host.stride_accepts)
    requires nonzero(host.set_stride) requires nonzero(host.use_record) requires nonzero(host.facts)
    requires nonzero(host.task) requires nonzero(host.views);
