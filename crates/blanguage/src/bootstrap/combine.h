module bootstrap.combine;
const u64 OBJECT_BYTES=48u64;
const u64 SYMBOL_BYTES=32u64;
struct CombineObject {
    u64 section_start;u64 section_count;u64 symbol_start;u64 symbol_count;
    u64 frame_start;u64 frame_count;
};
struct CombineSection {
    // A null source with nonzero byte_length denotes zero fill when writing.
    address name;u64 name_length;address bytes;u64 byte_length;u64 flags;u64 object;
    u64 group;u64 begin;u64 padding;u64 relocation_start;u64 relocation_base;
};
struct CombineGroup {u64 first;u64 size;u64 relocation_count;};
struct CombineSymbol {address name;u64 name_length;u64 section;u64 value;};
struct CombineName {address name;u64 length;};
struct CombineRelocation {
    u64 source;u64 slot;u64 offset;u64 target_kind;u64 section;u64 value;address name;u64 name_length;
    u64 kind;address library;u64 library_length;
};
struct CombineFrame {u64 section;u64 start;u64 present;u64 relocation_start;};
struct CombineInput {
    address objects;u64 object_count;address sections;u64 section_count;
    address symbols;u64 symbol_count;address relocations;u64 relocation_count;
    address frames;address order;address groups;
};
struct CombinePlan {u64 groups;u64 imports;u64 absolutes;u64 error;u64 at;};
extern "C" CombinePlan prepare(CombineInput input, address local, u64 local_count, bool pe) on cpu;
CombineSection section_at(address sections,u64 i) on cpu;
CombineGroup group_at(address groups,u64 i) on cpu;
void encode_section(CombineSection row,address output) on cpu;
extern "C" void encode(address sections,u64 section_count,address outputs) on cpu;
