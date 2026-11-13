module bootstrap.metadata;
import bootstrap.names;
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
    address bindings;address binding_order;u64 binding_count;
    address frames;u64 frame_count;
};
struct BindingOrder {u64 count;u64 conflict;};
// Rows borrow original object contracts, definitions first and then imports.
// order has room for count indices. The returned prefix is in name order.
// conflict is the original input index of the first conflicting import, or count.
extern "C" BindingOrder order_bindings(address bindings,u64 definitions,u64 count,address order) on cpu;
struct MetadataLayout {u64 length;u32 error;};
extern "C" MetadataLayout layout(MetadataInput input) on cpu;
extern "C" void encode(MetadataInput input,address output) on cpu;

// A null output measures the result. Zero means the original bytes are sufficient.
extern "C" u64 rust_link_binding(MetadataBytes binding,MetadataBytes prefix,
    MetadataBytes library,address output) on cpu;

// The caller owns the output sized for the magic, two lengths, and both fields.
extern "C" void encode_build_image(u64 magic,MetadataBytes identity,MetadataBytes image,address output) on cpu;
