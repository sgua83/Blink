module bootstrap.bindings;
import bootstrap.coff;
// Windows x64 C ABI and caller-owned array strides.
const u64 SECTION_BYTES=72u64;
const u64 DEFINITION_BYTES=40u64;
const u64 INPUT_RELOCATION_BYTES=64u64;
const u64 NODE_BYTES=72u64;
struct BindingBytes {address bytes;u64 length;};
struct BindingKey {BindingBytes first;BindingBytes second;};
struct BindingNode {BindingKey key;u64 value;u64 parent;u64 left;u64 right;u64 red;};
struct BindingSection {
    BindingBytes name;u64 flags;address relocations;u64 relocation_count;
    address associated;u64 associated_count;u64 owner;u64 symbol;
};
struct BindingDefinition {BindingBytes name;u64 section;u64 value;u64 function;};
struct BindingRelocation {u64 offset;u64 kind;u64 target;u64 section;BindingBytes name;BindingBytes dll;};
struct BindingInput {BindingBytes name;address sections;u64 section_count;address definitions;u64 definition_count;};
struct BindingWork {
    address symbols;address nodes;
    address import_context;cfn<void,address,BindingKey> emit_import;
};
struct BindingPlan {u64 symbols;u64 names_root;u64 imports_root;u32 error;};
extern "C" BindingPlan prepare(BindingInput input,BindingWork work) on cpu;
extern "C" void write_generated_name(read CoffBindingSource binding,u64 kind,u64 source,address output) on cpu;
extern "C" void write_section_relocations(read CoffBindingSource binding,u64 index,address output) on cpu;
extern "C" u64 import_name_size(BindingBytes dll,BindingBytes name) on cpu;
extern "C" void import_name(BindingBytes dll,BindingBytes name,address output) on cpu;

struct BindingForest {address nodes;u64 count;u64 names;u64 imports;};
struct BindingInsert {u64 root;u64 node;};
BindingNode node_at(BindingForest forest,u64 index) on cpu;
i32 bytes_compare(BindingBytes left,BindingBytes right) on cpu;
u64 find(BindingForest forest,u64 root,BindingKey key) on cpu;
BindingInsert put(inout BindingForest forest,u64 root,BindingKey key,u64 value) on cpu;
u64 first(BindingForest forest,u64 root) on cpu;
u64 next(BindingForest forest,u64 index) on cpu;

struct MsvcName { BindingBytes name; u64 sdk_relative; };
struct MsvcNamePlan { u64 libraries; u64 exports; u64 needs_sdk; };

// Reuse one node per name in the larger input. Compact each borrowed group
// in place in byte order. The two groups keep independent name identities.
extern "C" MsvcNamePlan select_msvc_names(address libraries, u64 library_count,
    address exports, u64 export_count, address prefix, u64 prefix_length, address nodes) on cpu;
