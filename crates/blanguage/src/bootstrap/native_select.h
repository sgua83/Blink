module bootstrap.native_select;
import bootstrap.bindings;
const u64 OBJECT_BYTES=192u64;
const u64 SECTION_BYTES=64u64;
const u64 RELOCATION_BYTES=56u64;
const u64 SYMBOL_BYTES=56u64;
const u64 IMPORT_BYTES=32u64;
const u64 EXPORT_BYTES=24u64;
const u64 FRAME_BYTES=32u64;
const u64 LOCAL_BYTES=32u64;
struct SelectObject {
    u64 kind;BindingBytes name;u64 section_start;u64 section_count;u64 symbol_start;u64 symbol_count;
    u64 import_start;u64 import_count;u64 export_start;u64 export_count;u64 code_start;u64 code_count;
    u64 frame_start;u64 frame_count;BindingBytes prefix;u64 archive;u64 has_resources;u64 selected;u64 active_archive;u64 keep_archive;u64 borrowed;u64 stack_bytes;
};
struct SelectSection {u64 owner;u64 flags;address associated;u64 associated_count;u64 relocation_start;u64 relocation_count;u64 live;u64 mapping;};
struct SelectRelocation {u64 kind;u64 section;BindingBytes name;BindingBytes dll;u64 rewrite;};
struct SelectSymbol {BindingBytes name;BindingBytes contract;u64 owner;u64 section;u64 keep;};
struct SelectImport {BindingBytes name;BindingBytes contract;};
struct SelectExport {BindingBytes name;u64 keep;};
struct SelectFrame {u64 section;u64 present;u64 keep;u64 mapping;};
struct SelectLocal {BindingBytes slot;BindingBytes name;};
struct SelectRuntime {BindingBytes name;u64 used;};
struct SelectRoot {BindingBytes name;BindingBytes contract;};
extern "C" u64 bind_root_contracts(address roots,u64 count,address context,u64 sources,
    cfn<BindingBytes,address,u64,BindingBytes> lookup) on cpu requires nonzero(lookup);
struct SelectView {
    address objects;u64 object_count;address sections;u64 section_count;address relocations;
    address symbols;address imports;address exports;address code;address frames;address nodes;address pending;
    address locals;address runtime;u64 runtime_count;u64 library_count;
};
struct SelectParams {u64 native_mode;address roots;u64 root_count;u64 types;u64 programs;BindingBytes runtime_library;};
struct SelectReply {SelectView view;u64 object;u64 error;};
struct SelectResource {address runtime;u64 runtime_count;address locals;u64 local_count;u64 types;u64 programs;};
enum EntryProduct:u32 { None=0; Pe=1; HostApplication=2; Dll=3; DllWithoutArchive=4; };
struct EntryPlan {EntryProduct product;u32 uses_runtime;u64 stack_reserve;};
struct SelectResult {u64 error;BindingBytes name;BindingBytes owner;u64 entry;EntryPlan plan;};
extern "C" SelectResult choose(SelectView input,SelectParams params,address context,
    cfn<SelectReply,address,u64,BindingBytes> extract,cfn<SelectReply,address,u64> native_archive,
    cfn<SelectReply,address,SelectResource> resources,cfn<BindingBytes,address,BindingBytes> slot) on cpu
    requires nonzero(extract) requires nonzero(native_archive) requires nonzero(resources) requires nonzero(slot);
