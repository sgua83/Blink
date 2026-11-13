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

struct BuildSymbol {
    DigestField name; DigestField contract; DigestField declaration;
    u32 bound; u32 requirement;
};
enum ReferenceKind : u32 { Missing = 0; Source = 1; Query = 2; Implementation = 3; };
struct BuildReference {
    DigestField name; DigestField module_name; DigestField value;
    address imports; u64 import_count;
    u32 tag; ReferenceKind kind; u32 execution; u32 resolution;
};

// The caller borrows each original field only for the synchronous digest update.
extern "C" void sha256_reference_symbol(inout DigestState state, BuildSymbol symbol) on cpu;
extern "C" void sha256_reference_count(inout DigestState state, u64 count) on cpu;
extern "C" void sha256_reference(inout DigestState state, BuildReference row) on cpu;

struct SourceRegion {
    u64 preamble_end;
    u64 include_signatures;
    u64 first_signature;
};

struct SourceDeclaration {
    u64 region;
    u64 start;
    u64 next_signature;
    DigestField name;
    DigestField signature;
};

// Declarations are in original order. A zero link ends a region's signature
// list; nonzero links are one-based indices into this call's original rows.
extern "C" void plan_source_regions(address regions,address declarations,u64 count) on cpu;

// The caller owns the original source, declaration rows and borrowed fields.
extern "C" void sha256_source(DigestField prefix, address declarations,
    u64 first_signature, address output) on cpu;
