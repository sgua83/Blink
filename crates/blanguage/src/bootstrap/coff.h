module bootstrap.coff;
// Windows x64 C ABI; all borrowed-view fields occupy eight bytes.
const u64 SYMBOL_BYTES=80u64;
const u64 COFF_RELOCATION_RECORD_BYTES=10u64;
struct CoffSection {
    address name;u64 name_length;u64 byte_length;u64 flags;
    u64 relocation_count;u64 uninitialized;u64 offset;u64 relocation_offset;
};
struct CoffSymbol {
    address name;u64 name_length;u64 section;u64 value;
    u64 function;u64 external;u64 auxiliary;u64 owner;u64 generated;u64 source;
};
struct CoffInput {address sections;u64 section_count;address symbols;u64 symbol_count;};
struct CoffLayout {u64 length;u64 symbols;u64 strings;u64 symbol_count;u32 error;};
extern "C" CoffLayout layout(CoffInput input) on cpu;
struct CoffBindingSource {
    address module_name;u64 module_name_length;address sections;address nodes;u64 names_root;u64 imports_root;
};
extern "C" void encode(CoffInput input,CoffLayout layout,CoffBindingSource binding,address output) on cpu;
extern "C" void host_headers(address output, u64 sections, u64 symbols, u64 cache_end, u64 cache_length) on cpu;
