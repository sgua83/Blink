module bootstrap.metadata_read;
import blink.windows.coff;
// One borrowed input and the original sequential read position.
struct MetaCursor {ReadBytes input;u64 position;};
struct MetaWord {u64 value;u32 error;};
struct MetaField {ReadBytes value;u32 error;};
struct MetaHeader {
    ReadBytes name;u64 kind;ReadBytes interface;ReadBytes prefix;ReadBytes header;ReadBytes binding;
    u64 requirements;u64 stack;u32 error;
};
struct MetaRequirement {ReadBytes name;u64 kind;u32 error;};
struct MetaBinding {ReadBytes name;ReadBytes value;u32 error;};
// Borrowed format ranges; cursor continues at the first binding.
struct MetaPrefix {
    MetaHeader header;ReadBytes requirements;ReadBytes libraries;ReadBytes exports;
    ReadBytes resource;u64 binding_count;
};
extern "C" MetaPrefix prefix(inout MetaCursor cursor,u64 format) on cpu;
struct MetaFrame {ReadBytes name;u64 has_coff;u64 section;u64 start;u64 local;u64 calls;u32 error;};
struct MetaCall {u64 kind;u64 offset;ReadBytes reason;u32 error;};
struct MetaResourceHead {u64 present;ReadBytes prefix;u32 error;};
struct MetaResourceBody {u64 types;u64 programs;ReadBytes abi;u64 code_count;u32 error;};
extern "C" MetaResourceHead resource_head(inout MetaCursor cursor) on cpu;
extern "C" MetaResourceBody resource_body(inout MetaCursor cursor) on cpu;
extern "C" MetaField resource_text(inout MetaCursor cursor) on cpu;
extern "C" MetaWord word(inout MetaCursor cursor) on cpu;
extern "C" MetaField bytes(inout MetaCursor cursor) on cpu;
extern "C" MetaHeader header(inout MetaCursor cursor,u64 format) on cpu;
extern "C" MetaRequirement requirement(inout MetaCursor cursor) on cpu;
extern "C" MetaBinding binding(inout MetaCursor cursor) on cpu;
extern "C" MetaFrame frame(inout MetaCursor cursor) on cpu;
extern "C" MetaCall call(inout MetaCursor cursor) on cpu;

struct BuildImage {ReadBytes identity;ReadBytes image;u32 status;};
extern "C" BuildImage read_build_image(ReadBytes input,u64 magic) on cpu;

// BLBO readers borrow the original snapshot. Header mismatch returns no range.
extern "C" ReadBytes manifest_body(ReadBytes input, u64 magic) on cpu;
extern "C" MetaWord manifest_number(inout MetaCursor cursor, bool wide) on cpu;

enum ManifestFieldKind : u32 { Bytes = 0; Text = 1; Digest = 2; OptionalDigest = 3; };
extern "C" MetaField manifest_field(inout MetaCursor cursor, ManifestFieldKind kind) on cpu;
