module bootstrap.bindings;
import bootstrap.coff;
// Windows x64 C ABI and caller-owned array strides.
const u64 NODE_BYTES=72u64;
struct BindingBytes {address bytes;u64 length;};
struct BindingKey {BindingBytes first;BindingBytes second;};
struct BindingNode {BindingKey key;u64 value;u64 parent;u64 left;u64 right;u64 red;};
struct BindingDefinition {BindingBytes name;u64 section;u64 value;u64 function;};
struct BindingRelocation {u64 offset;u16 kind;u16 target;u32 value;u64 section;BindingBytes name;BindingBytes dll;};
struct BindingStorage {address symbols;address nodes;};
struct BindingPlan {u64 symbols;u64 names_root;u64 imports_root;u32 error;};
BindingPlan prepare(CoffSource source,CoffWork work,BindingStorage storage) on cpu
    requires nonzero(source.next_definition);
extern "C" u64 relocation_symbol(read CoffBindingSource binding,u64 index,u64 relocation) on cpu;
extern "C" void write_generated_name(read CoffBindingSource binding,u64 kind,u64 source,address output) on cpu;
extern "C" void write_section_relocations(read CoffBindingSource binding,u64 index,address output) on cpu;
extern "C" u64 import_name_size(BindingBytes dll,BindingBytes name) on cpu;
extern "C" void import_name(BindingBytes dll,BindingBytes name,address output) on cpu;

struct BindingForest {address nodes;u64 count;u64 names;u64 imports;};
struct BindingInsert {u64 root;bool inserted;};
BindingNode node_at(BindingForest forest,u64 index) on cpu;
i32 bytes_compare(BindingBytes left,BindingBytes right) on cpu;
u64 find(BindingForest forest,u64 root,BindingKey key) on cpu;
// A matching key keeps its node and only replaces the value when requested.
BindingInsert insert_binding(inout BindingForest forest,u64 root,BindingKey key,u64 value,bool replace) on cpu;
u64 first(BindingForest forest,u64 root) on cpu;
u64 next(BindingForest forest,u64 index) on cpu;

struct MsvcName { BindingBytes name; u64 sdk_relative; };
struct MsvcNamePlan { u64 libraries; u64 exports; u64 needs_sdk; };

// Reuse one node per name in the larger input. Compact each borrowed group
// in place in byte order. The two groups keep independent name identities.
extern "C" MsvcNamePlan select_msvc_names(address libraries, u64 library_count,
    address exports, u64 export_count, address prefix, u64 prefix_length, address nodes) on cpu;
