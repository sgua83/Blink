module bootstrap.type_reference;
import codec.builtin_type;
import bootstrap.ast_storage;

// Descriptors borrow the original semantic input until its real query ends.
struct Name { address bytes; u64 length; };
struct Reference { BuiltinKind kind; u32 wrappers; u32 resource; u32 pending_count; u64 arguments; Name name; };
struct Argument { address reference; u32 wrappers; u64 arguments; Optional<u32> literal; };
struct Parameters { u32 present; u64 types; u64 count; };
enum Code:u32 { Owned=224; Void=228; Ordinary=232; Array=238; Result=269; };
enum Problem:u32 { Arity=0; Type=1; ReleaseType=2; ReleaseForm=3; };
enum Need:u32 { Done=0; Function=1; Type=2; Value=3; Count=4; };
struct Cursor { Reference source; u64 types; u64 count; u64 index; Code code; u32 canonical; u32 stage; address reference; };
struct Host {
    cfn<void,address> resource;
    cfn<Parameters,address> record;
    cfn<void,address,u64> reserve;
    cfn<Argument,address,u64> argument;
    cfn<void,address,u32> append_value;
    cfn<bool,address,address> append_release;
    cfn<void,address,Code,Problem,u64> fail;
    cfn<void,address,u32,Name> finish;
};

extern "C" Cursor start(Reference source) on cpu;
extern "C" Need step(address state,address context,Host host) on cpu
    requires nonzero(host.resource) requires nonzero(host.record) requires nonzero(host.reserve)
    requires nonzero(host.argument) requires nonzero(host.append_value) requires nonzero(host.append_release)
    requires nonzero(host.fail) requires nonzero(host.finish);

enum PathStart:u32 { Bind=0; FunctionNumeric=1; FunctionField=2; };
struct ContractPath { u64 index; u64 count; };
enum PathMember:u32 { Field=0; Variant=1; MissingType=2; MissingField=3; };
enum PathAction:u32 {
    Layout=0; Specialize=1; Builtin=2; Length=3; Variant=4; LengthTail=5; VariantTail=6;
    MissingType=7; MissingField=8; RecordNumeric=9;
};
struct PathInput { BuiltinKind kind; u32 wrappers; u32 builtin; Name field; };
struct PathHost {
    address context;
    cfn<PathMember,address,Name> member;
    cfn<u32,address> ordinary_record;
};

extern "C" PathStart contract_path_start(BuiltinKind kind,u32 wrappers,u64 remaining) on cpu;
extern "C" PathAction contract_path_next(address path,PathInput input,PathHost host) on cpu
    requires nonzero(host.member) requires nonzero(host.ordinary_record);
