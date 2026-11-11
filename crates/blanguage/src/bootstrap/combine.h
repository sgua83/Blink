module bootstrap.combine;
const u64 OBJECT_BYTES=56u64;
const u64 SECTION_BYTES=88u64;
const u64 SYMBOL_BYTES=32u64;
const u64 RELOCATION_BYTES=56u64;
const u64 FRAME_BYTES=40u64;
struct CombineObject {
    u64 section_start;u64 section_count;u64 symbol_start;u64 symbol_count;
    u64 frame_start;u64 frame_count;u64 stack_bytes;
};
struct CombineSection {
    address name;u64 name_length;address bytes;u64 byte_length;u64 flags;u64 object;
    u64 owner;u64 group;u64 begin;u64 size;u64 padding;
};
struct CombineSymbol {address name;u64 name_length;u64 section;u64 value;};
struct CombineRelocation {
    u64 source;u64 offset;u64 target_kind;u64 section;u64 value;address name;u64 name_length;
};
struct CombineFrame {u64 section;u64 start;u64 call_start;u64 call_count;u64 present;};
struct CombineInput {
    address objects;u64 object_count;address sections;u64 section_count;
    address symbols;u64 symbol_count;address relocations;u64 relocation_count;
    address frames;address calls;address order;
};
struct CombinePlan {u64 groups;u64 symbols;u64 stack_bytes;u64 error;u64 at;};
extern "C" CombinePlan prepare(CombineInput input) on cpu;
extern "C" void encode(CombineInput input,address outputs) on cpu;
