module bootstrap.coff;
// Windows x64 C ABI; all borrowed-view fields occupy eight bytes.
const u64 SECTION_BYTES=72u64;
const u64 SYMBOL_BYTES=64u64;
const u64 RELOCATION_BYTES=24u64;
struct CoffSection {
    address name;u64 name_length;address bytes;u64 byte_length;u64 flags;
    address relocations;u64 relocation_count;u64 offset;u64 relocation_offset;
};
struct CoffSymbol {
    address name;u64 name_length;u64 section;u64 value;
    u64 function;u64 external;u64 auxiliary;u64 owner;
};
struct CoffRelocation {u64 offset;u64 symbol;u64 kind;};
struct CoffInput {address sections;u64 section_count;address symbols;u64 symbol_count;};
struct CoffLayout {u64 length;u64 symbols;u64 strings;u64 symbol_count;u32 error;};
extern "C" CoffLayout layout(CoffInput input) on cpu;
extern "C" void encode(CoffInput input,CoffLayout layout,address output) on cpu;
