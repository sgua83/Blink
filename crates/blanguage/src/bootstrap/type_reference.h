module bootstrap.type_reference;
import bootstrap.type_families;

// Every descriptor borrows an actual native semantic value for this call.
struct Name { address bytes; u64 length; };
struct Reference { BuiltinKind kind; u32 wrappers; u32 resource; u64 arguments; Name name; };
struct Argument { address reference; u32 wrappers; u64 arguments; };
struct Parameters { u32 present; u64 types; u64 count; };
enum Code:u32 { Owned=224; Void=228; Ordinary=232; Array=238; Result=269; };
enum Problem:u32 { Arity=0; Type=1; ReleaseType=2; ReleaseForm=3; };

extern "C" void bind(Reference source, address context,
    cfn<void,address> function,
    cfn<void,address> resource,
    cfn<Parameters,address> record,
    cfn<void,address,u64> reserve,
    cfn<Argument,address,u64> argument,
    cfn<bool,address,address> append_type,
    cfn<bool,address,address> append_release,
    cfn<bool,address,u64> append_value,
    cfn<void,address,Code,Problem,u64> fail,
    cfn<void,address,u32,Name> finish) on cpu
    requires nonzero(function) requires nonzero(resource) requires nonzero(record)
    requires nonzero(reserve) requires nonzero(argument) requires nonzero(append_type)
    requires nonzero(append_release) requires nonzero(append_value) requires nonzero(fail)
    requires nonzero(finish);
