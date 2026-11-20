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

struct ManifestIdentity {
    ReadBytes path;
    ReadBytes root;
    ReadBytes working_directory;
    ReadBytes module_name;
    ReadBytes compiler;
    u32 max_parameter_bytes;
};
struct ManifestVariant { ReadBytes first; ReadBytes second; u32 tag; };
struct ManifestReadSource { ReadBytes module_name; ReadBytes path; ReadBytes scope; address fingerprint; };
struct ManifestReadRequirement { ReadBytes name; ReadBytes kind; };
struct ManifestReadResult { ManifestIdentity identity; address imports_digest; address native_digest; u32 status; };

// The receiver builds the original records synchronously. All fields borrow input.
struct ManifestReceiver {
    address context;
    cfn<bool,address,ReadBytes> valid_text;
    cfn<void,address,ReadBytes> name;
    cfn<void,address,ManifestVariant> selection;
    cfn<void,address,ManifestReadSource> source;
    cfn<void,address,ManifestReadRequirement> requirement;
    cfn<void,address,ManifestVariant> declaration;
};
extern "C" ManifestReadResult read_manifest(ReadBytes input, u64 magic, ManifestReceiver receiver) on cpu
    requires nonzero(receiver.valid_text) requires nonzero(receiver.name)
    requires nonzero(receiver.selection) requires nonzero(receiver.source)
    requires nonzero(receiver.requirement) requires nonzero(receiver.declaration);
