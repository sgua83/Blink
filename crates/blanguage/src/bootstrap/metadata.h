module bootstrap.metadata;
// Windows x64 C ABI: borrowed fields and array strides.
const u64 BYTES_BYTES=16u64;
const u64 REQUIREMENT_BYTES=24u64;
const u64 BINDING_BYTES=32u64;
const u64 FRAME_BYTES=64u64;
const u64 CALL_BYTES=32u64;
struct MetadataBytes {address bytes;u64 length;};
struct MetadataRequirement {MetadataBytes name;u64 kind;};
struct MetadataBinding {MetadataBytes name;MetadataBytes value;};
struct MetadataCall {u64 kind;u64 offset;MetadataBytes reason;};
struct MetadataFrame {
    MetadataBytes name;u64 has_coff;u64 section;u64 start;u64 local;
    address calls;u64 call_count;
};
struct MetadataResource {
    u64 present;
    MetadataBytes prefix;
    u64 types;
    u64 programs;
    MetadataBytes abi;
    address code;
    u64 code_count;
};
struct MetadataInput {
    u64 format_version;MetadataBytes name;u64 kind;
    MetadataBytes interface;u64 stack_bytes;
    MetadataBytes prefix;MetadataBytes header;MetadataBytes binding;
    address requirements;u64 requirement_count;
    address libraries;u64 library_count;
    address exports;u64 export_count;
    MetadataResource resource;
    address bindings;u64 binding_count;
    address frames;u64 frame_count;
};
struct MetadataLayout {u64 length;u32 error;};
extern "C" MetadataLayout layout(MetadataInput input) on cpu;
extern "C" void encode(MetadataInput input,address output) on cpu;

// A null output measures the result. Zero means the original bytes are sufficient.
extern "C" u64 rust_link_binding(MetadataBytes binding,MetadataBytes prefix,
    MetadataBytes library,address output) on cpu;

// The caller owns the output sized for the magic, two lengths, and both fields.
extern "C" void encode_build_image(u64 magic,MetadataBytes identity,MetadataBytes image,address output) on cpu;

struct BuildSymbol {
    MetadataBytes name; MetadataBytes contract; MetadataBytes declaration;
    u32 bound; u32 requirement;
};
struct BuildRegion { u64 start; u64 length; };
struct BuildReference {
    BuildRegion key; BuildRegion query; MetadataBytes source;
    u32 kind; u32 execution;
};
struct BuildReferences {
    address symbols; u64 symbol_count; address references; u64 reference_count;
    address encoded;
};
extern "C" u64 encode_build_references(BuildReferences input,address output) on cpu;
