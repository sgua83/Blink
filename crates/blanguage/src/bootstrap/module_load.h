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
struct Imported {u32 success;NameBytes requested;NameBytes name;};
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
extern "C" void load(LoadInput input,address context,
    cfn<bool,address> contains,
    cfn<void,address> enter,
    cfn<u64,address> imports,
    cfn<Imported,address,u64> import_module,
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

enum ModuleEntry:u32 { Native=0; DomainProgram=1; Check=2; Reject=3; };
struct ModulePlan { ModuleEntry entry; u32 query_domain; u32 publish; u32 schedule; };
extern "C" ModulePlan native_module_plan(SignatureNode source,u32 compiled,u32 local,u32 domain,u32 exported,u32 explicit) on cpu;

enum SourceMode:u32 { Automatic=0; SelectedProgram=1; Application=2; };
enum SourceRoute:u32 { Function=0; CheckDomain=1; ValidateProgram=2; Program=3; Application=4; };
extern "C" u32 source_module_entry(SignatureNode source,u32 local,u32 exported) on cpu;

struct SourcePlan { SourceRoute route; u32 schedule; u32 cpu_signature; };
struct SourceCandidate {
    NameBytes key;
    address definition;
    SignatureNode source;
    u32 compiled;
    u32 local;
    u32 exported;
    u32 domain;
    u32 selected;
};
enum SourceExpansion:u32 { Closed=0; Open=1; Failed=2; };
struct SourcePlanningHost {
    address context;
    cfn<SourceCandidate,address> next;
    cfn<u32,address,address> is_open;
    cfn<SourceExpansion,address,NameBytes,address,SourceRoute,u32> schedule;
    cfn<void,address,NameBytes> record_open;
    cfn<void,address,SourceRoute> selected;
};
extern "C" u32 plan_source_module(SourceMode mode,SourcePlanningHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.is_open) requires nonzero(host.schedule)
    requires nonzero(host.record_open) requires nonzero(host.selected);

enum SourceSignature:u32 { Absent=0; Present=1; Failed=2; };
struct SourceInstanceHost {
    address context;
    cfn<SourceSignature,address> signature;
    cfn<void,address> function;
    cfn<void,address> domain;
    cfn<void,address,u32> build_program;
    cfn<void,address> application;
};
extern "C" void compile_source_instance(SourceRoute route,u32 cpu_signature,SourceInstanceHost host) on cpu
    requires nonzero(host.signature) requires nonzero(host.function) requires nonzero(host.domain)
    requires nonzero(host.build_program) requires nonzero(host.application);

enum WorldCheck:u32 { Constant=0; Interface=1; External=2; Types=3; };
struct WorldFunction { NameBytes name; address definition; };
struct WorldDeclaration { SignatureNode source; WorldFunction function; u32 present; u32 paired; };
struct WorldPreparation {
    address context;
    u64 declarations;
    cfn<WorldDeclaration,address> next;
    cfn<u32,address,WorldFunction> signature;
    cfn<void,address,u32> track;
    cfn<void,address> finish_interface;
    cfn<u32,address,WorldCheck,WorldFunction> run;
};
extern "C" u64 prepare_world(WorldPreparation host) on cpu
    requires nonzero(host.next) requires nonzero(host.signature)
    requires nonzero(host.track) requires nonzero(host.finish_interface) requires nonzero(host.run);

struct ExportRange { Slice arguments; u64 remaining; };
struct ExportTypeHost {
    address context;
    cfn<void,address,ExportRange> save;
    cfn<ExportRange,address> resume;
    cfn<bool,address,NameBytes> load;
};
extern "C" u32 load_export_types(StorageView storage,Slice arguments,ExportTypeHost host) on cpu
    requires nonzero(host.save) requires nonzero(host.resume) requires nonzero(host.load);
