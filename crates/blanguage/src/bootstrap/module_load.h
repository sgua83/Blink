module bootstrap.module_load;
import bootstrap.ast_storage;
import bootstrap.signatures;
import bootstrap.names;

struct ModuleView {address owner;SignatureNode source;};
struct LoadInput {
    ModuleView interface;
    ModuleView implementation;
    u32 has_implementation;
    u32 compiled;
};
struct ImportList {address names;u64 count;};
struct Imported {u32 success;NameBytes name;};
enum LoadFailure:u32 {
    ModuleName=22; ImportName=24; DuplicateRecord=25; InterfaceBody=26;
    MissingBody=27; Signature=28; ImplementationBody=29; DuplicateValue=30;
    ReservedRecord=184; DuplicateSet=233; MissingRequirement=270;
};

// Callbacks access the caller's existing source, input and declaration owners.
// All views and callback results remain borrowed through this synchronous load.
// These opaque pointers borrow actual available I/O actions; B never reads
// their Rust representation. The selected action is consumed by its owner.
extern "C" address provider(address loaded,address compiled,address source) on cpu;
// prepare queries one existing namespace (record=0, set=1, value=2), keeping
// the one qualified key only until insertion or the original diagnostic.
extern "C" NameBytes load(LoadInput input,address context,
    cfn<bool,address> contains,
    cfn<void,address> enter,
    cfn<ImportList,address> imports,
    cfn<Imported,address,NameBytes> import_module,
    cfn<bool,address,NameBytes,u32> prepare,
    cfn<void,address,address,u64,u32> insert_type,
    cfn<address,address> pair,
    cfn<bool,address,address,u64,u64> signature,
    cfn<u32,address> requirement,
    cfn<void,address,address,u64,Optional<FunctionRef>,u32,u32,u32> insert_function,
    cfn<void,address,LoadFailure,address,Span,NameBytes,NameBytes> report) on cpu
    requires nonzero(contains) requires nonzero(enter) requires nonzero(imports)
    requires nonzero(import_module) requires nonzero(prepare) requires nonzero(insert_type)
    requires nonzero(pair) requires nonzero(signature) requires nonzero(requirement)
    requires nonzero(insert_function) requires nonzero(report);
