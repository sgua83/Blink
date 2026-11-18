module bootstrap.type_reference;
import bootstrap.type_families;

// Descriptors borrow the original semantic input until its real query ends.
struct Name { address bytes; u64 length; };
struct Reference { BuiltinKind kind; u32 wrappers; u32 resource; u32 count; u64 arguments; Name name; };
struct Argument { address reference; u32 wrappers; u64 arguments; };
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
    cfn<bool,address,address> append_release;
    cfn<void,address,Code,Problem,u64> fail;
    cfn<void,address,u32,Name> finish;
};

extern "C" Cursor start(Reference source) on cpu;
extern "C" Need step(address state,address context,Host host) on cpu
    requires nonzero(host.resource) requires nonzero(host.record) requires nonzero(host.reserve)
    requires nonzero(host.argument) requires nonzero(host.append_release)
    requires nonzero(host.fail) requires nonzero(host.finish);
