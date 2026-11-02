module bootstrap.bindings;
import bootstrap.coff;
// Windows x64 C ABI and caller-owned array strides.
const u64 SECTION_BYTES=72u64;
const u64 DEFINITION_BYTES=40u64;
const u64 INPUT_RELOCATION_BYTES=64u64;
const u64 NODE_BYTES=72u64;
const u64 OUTPUT_RELOCATION_BYTES=24u64;
struct BindingBytes {address bytes;u64 length;};
struct BindingKey {BindingBytes first;BindingBytes second;};
struct BindingNode {BindingKey key;u64 value;u64 parent;u64 left;u64 right;u64 red;};
struct BindingSection {
    BindingBytes name;u64 flags;address relocations;u64 relocation_count;
    address associated;u64 associated_count;u64 owner;u64 symbol;
};
struct BindingDefinition {BindingBytes name;u64 section;u64 value;u64 function;};
struct BindingRelocation {u64 offset;u64 kind;u64 target;u64 section;BindingBytes name;BindingBytes dll;};
struct BoundRelocation {u64 offset;u64 symbol;u64 kind;};
struct BindingInput {BindingBytes name;u64 application;address sections;u64 section_count;address definitions;u64 definition_count;};
struct BindingWork {address symbols;address nodes;address relocations;};
struct BindingPlan {u64 symbols;u64 names;u64 relocations;u32 error;};
extern "C" BindingPlan prepare(BindingInput input,BindingWork work) on cpu;
extern "C" void materialize(BindingInput input,BindingWork work,BindingPlan plan,address names) on cpu;
