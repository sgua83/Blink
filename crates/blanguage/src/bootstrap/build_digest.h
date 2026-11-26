module bootstrap.build_digest;

// The caller borrows the input and owns the 32 output bytes.
extern "C" void sha256(address input,u64 length,address output) on cpu;

struct DigestField { address bytes; u64 length; };
extern "C" void sha256_fields(address fields,u64 count,address output) on cpu;

// One caller-owned state per input stream. Update borrows the bytes only for the call.
struct DigestState {
    array<u32,8> words;
    array<u64,16> tail;
    u64 pending;
    u64 length;
};
extern "C" void digest_field(inout DigestState state, DigestField field) on cpu;
extern "C" DigestState sha256_start() on cpu;
extern "C" void sha256_update(inout DigestState state,address input,u64 length) on cpu;
extern "C" void sha256_finish(inout DigestState state,address output) on cpu;

struct CompiledSymbol {
    DigestField name;
    DigestField provider;
    DigestField contract;
    DigestField declaration;
};

struct ReferenceKey {
    DigestField name;
    DigestField module_name;
    address imports;
    u64 import_count;
    u32 tag;
};

struct FunctionReference {
    DigestField declaration;
    u32 constant;
    u32 external;
    u32 implementation;
    u32 execution;
};

enum ReferenceQuery : u32 { Value = 0; Record = 1; TypeSet = 2; };

struct ReferenceHost {
    address symbol_stream;
    address declaration_stream;
    address context;
    cfn<DigestField,address> next_symbol;
    cfn<ReferenceKey,address> next_declaration;
    cfn<DigestField,address,u64> ensures memory(read) name;
    cfn<CompiledSymbol,address,DigestField> ensures memory(read) symbol;
    cfn<u32,address,DigestField,DigestField> ensures memory(read) requirement;
    cfn<FunctionReference,address,DigestField> ensures memory(read) function;
    cfn<DigestField,address,DigestField> ensures memory(read) record;
    cfn<DigestField,address,DigestField> ensures memory(read) type_set;
    cfn<DigestField,address,DigestField,DigestField> ensures memory(read) contract;
    cfn<void,address,address,ReferenceKey,ReferenceQuery> query;
};

// The source owner resolves the requested namespace. A returned name is borrowed
// only until it has been added to the current digest.
extern "C" void digest_name_resolution(inout DigestState state, u32 resolution, DigestField name) on cpu;

extern "C" void dependency_digest(u64 declaration_count, u32 imported, ReferenceHost host, address output) on cpu
    requires nonzero(host.next_symbol) requires nonzero(host.next_declaration) requires nonzero(host.name)
    requires nonzero(host.symbol) requires nonzero(host.requirement) requires nonzero(host.function)
    requires nonzero(host.record) requires nonzero(host.type_set) requires nonzero(host.contract)
    requires nonzero(host.query);

extern "C" bool matches_dependency_reuse(u64 declarations, u32 imported, ReferenceHost host, address expected) on cpu
    requires nonzero(host.next_symbol) requires nonzero(host.next_declaration) requires nonzero(host.name)
    requires nonzero(host.symbol) requires nonzero(host.requirement) requires nonzero(host.function)
    requires nonzero(host.record) requires nonzero(host.type_set) requires nonzero(host.contract)
    requires nonzero(host.query);

struct ObjectReuse {
    address imports;
    address runtime;
    address context;
    cfn<u32,address,address> load_runtime;
};
extern "C" u32 matches_object_reuse(u64 declarations, u32 imported, ReferenceHost host, ObjectReuse reuse) on cpu
    requires nonzero(host.next_symbol) requires nonzero(host.next_declaration) requires nonzero(host.name)
    requires nonzero(host.symbol) requires nonzero(host.requirement) requires nonzero(host.function)
    requires nonzero(host.record) requires nonzero(host.type_set) requires nonzero(host.contract)
    requires nonzero(host.query) requires nonzero(reuse.load_runtime);

struct SourceExtent { u64 preamble_end; u32 include_signatures; };
struct SourceSignature { DigestField name; DigestField signature; };
struct SignatureSpan { address signatures; u64 count; };

struct SourceFile {
    address source; DigestField name; DigestField text;
    u32 root; u32 implementation; u32 shares_interface;
};

struct SourceDefinition {
    address source; address function; DigestField name; u64 start; u32 constant; u32 external;
};

struct SourceRequirement { DigestField name; u32 public; u32 root; u32 execution; };
enum SourceScope : u32 { Full = 0; Preamble = 1; Signatures = 2; };

struct SourceHost {
    address output;
    address files;
    address declarations;
    address requirements;
    cfn<SourceFile,address> next_file;
    cfn<void,address> rewind_files;
    cfn<SourceDefinition,address> next_definition;
    cfn<SourceRequirement,address> next_requirement;
    cfn<address,address,address> register_region;
    cfn<address,address,address> region;
    cfn<void,address,SourceDefinition> signature;
    cfn<DigestField,address,u64> ensures memory(read) prefix;
    cfn<SignatureSpan,address> ensures memory(read) signatures;
    cfn<void,address,SourceFile,SourceScope,address> emit_source;
    cfn<void,address,DigestField,u32> emit_requirement;
};

extern "C" void capture_sources(SourceHost host, u32 exports) on cpu
    requires nonzero(host.next_file) requires nonzero(host.rewind_files)
    requires nonzero(host.next_definition) requires nonzero(host.next_requirement)
    requires nonzero(host.register_region) requires nonzero(host.region) requires nonzero(host.signature)
    requires nonzero(host.prefix) requires nonzero(host.signatures) requires nonzero(host.emit_source)
    requires nonzero(host.emit_requirement);

// The receiver borrows original cached input entries for one build.
struct DeclarationHost {
    cfn<address,address> next;
    cfn<bool,address,address> retain;
    cfn<void,address,address> expand;
};
extern "C" void complete_declarations(address context, DeclarationHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.retain) requires nonzero(host.expand);
