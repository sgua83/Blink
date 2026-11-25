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

struct SourceSignature {
    DigestField name;
    DigestField signature;
};

// Each original declaration contributes its start.
extern "C" void record_source_declaration(inout u64 preamble_end, u64 start) on cpu;

// Signatures are borrowed from the original source in declaration order.
extern "C" void sha256_source(DigestField prefix, address signatures,
    u64 count, address output) on cpu;

// The receiver borrows original cached input entries for one build.
struct DeclarationHost {
    cfn<address,address> next;
    cfn<bool,address,address> retain;
    cfn<void,address,address> expand;
};
extern "C" void complete_declarations(address context, DeclarationHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.retain) requires nonzero(host.expand);
