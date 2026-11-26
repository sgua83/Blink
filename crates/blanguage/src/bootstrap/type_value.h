module bootstrap.type_value;
import codec.builtin_type;

// Descriptors borrow the actual semantic input. Host owns the original maps,
// sets, bound temporaries and final Ty containers for this synchronous call.
struct Text { address bytes; u64 length; };
struct Type { address value; Text name; u64 arguments; u32 flags; u32 canonical; };
struct Facts { u32 numeric; u32 integer; u64 components; Text name; };
struct Count { u32 present; u32 value; };
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

enum Query:u32 {
    Complete=0; Function=1; Child=2; Dataset=3; SpecializeCleanup=4; Bind=5;
    ShapeArgument=6; Family=7; VariantTag=8; Reference=9; FieldCount=10; RecordStride=11;
};
enum Stage:u32 {
    Start=0; FunctionDone=1; TaskAfterChild=2; TaskBuild=3;
    OwnedAfterChild=4; OwnedSave=5; OwnedBindResult=6; OwnedCheckResult=7;
    OwnedParameter=8; OwnedAfterParameter=9; OwnedFinish=10;
    ResultItem=11; ResultAfterChild=12; ResultSave=13;
    ArrayAfterChild=14; ArrayCount=15; ArrayBuild=16;
    RecordTypeItem=17; RecordTypeAfterChild=18; RecordTypeFamily=19; RecordTypeAccepted=20;
    RecordConstItem=21; RecordConstStore=22; RecordRepresentation=23; RecordRepresentationReady=24;
    VariantItem=25; VariantTagged=26; VariantPayload=27; VariantAfterChild=28; VariantStore=29;
    FieldItem=30; FieldBound=31; FieldCount=32; FieldReference=33; FieldStore=34; RecordStride=35;
};
// This is the existing B type query's continuation. Ty and all partially built
// native containers remain in its original Frame; descriptors only borrow them.
struct Cursor {
    Type value; BuiltinKind kind; Stage stage; u32 wrap; u64 index;
    Type temporary; Record record; u32 scalar; Count count;
};
struct Request { Query kind; Site site; Binding binding; u64 index; };

struct Host {
    cfn<void,address,Site,u64,Problem,u64> failure;
    cfn<void,address> builtin;
    cfn<void,address> unit;
    cfn<void,address> view;
    cfn<Type,address,u64> argument;
    cfn<Cleanup,address,Type> cleanup;
    cfn<void,address> owner;
    cfn<bool,address> enter_cleanup;
    cfn<void,address> leave_cleanup;
    cfn<bool,address> owner_matches;
    cfn<void,address> use_owner;
    cfn<void,address> save_payload;
    cfn<bool,address> result;
    cfn<u64,address,u32> array_size;
    cfn<void,address,u32,u32> array;
    cfn<Record,address> record;
    cfn<void,address,u64> type_argument;
    cfn<bool,address,u64,u32> const_domain;
    cfn<void,address,u64,u32> const_argument;
    cfn<void,address> arguments_done;
    cfn<bool,address> enter_record;
    cfn<void,address> leave_record;
    cfn<void,address> simple_enum;
    cfn<void,address> begin_variants;
    cfn<bool,address,u64> unique_variant_name;
    cfn<bool,address,u32> unique_variant_tag;
    cfn<void,address,u64,u32> variant;
    cfn<bool,address> variants_done;
    cfn<void,address> begin_record;
    cfn<bool,address,u64> unique_field;
    cfn<Type,address> dataset_field;
    cfn<void,address,Count> reference_dataset;
    cfn<bool,address,u64> field;
    cfn<bool,address> record_done;
    cfn<bool,address,u32> stride_accepts;
    cfn<void,address,u32> set_stride;
    cfn<void,address> use_record;
    cfn<Facts,address> facts;
    cfn<bool,address> views;
    cfn<bool,address,u32> task;
};


extern "C" Cursor type_start(Type value,BuiltinKind kind) on cpu;
extern "C" Request type_step(address state,Host host,address context) on cpu
    requires nonzero(host.failure) requires nonzero(host.builtin)
    requires nonzero(host.unit) requires nonzero(host.view) requires nonzero(host.argument) requires nonzero(host.cleanup)
    requires nonzero(host.owner) requires nonzero(host.enter_cleanup) requires nonzero(host.leave_cleanup) requires nonzero(host.owner_matches)
    requires nonzero(host.use_owner) requires nonzero(host.save_payload) requires nonzero(host.result) requires nonzero(host.array_size) requires nonzero(host.array)
    requires nonzero(host.record) requires nonzero(host.type_argument) requires nonzero(host.const_domain)
    requires nonzero(host.const_argument) requires nonzero(host.arguments_done) requires nonzero(host.enter_record)
    requires nonzero(host.leave_record) requires nonzero(host.simple_enum) requires nonzero(host.begin_variants)
    requires nonzero(host.unique_variant_name) requires nonzero(host.unique_variant_tag)
    requires nonzero(host.variant) requires nonzero(host.variants_done) requires nonzero(host.begin_record)
    requires nonzero(host.unique_field) requires nonzero(host.dataset_field) requires nonzero(host.reference_dataset) requires nonzero(host.field)
    requires nonzero(host.record_done) requires nonzero(host.stride_accepts)
    requires nonzero(host.set_stride) requires nonzero(host.use_record) requires nonzero(host.facts)
    requires nonzero(host.task) requires nonzero(host.views);
