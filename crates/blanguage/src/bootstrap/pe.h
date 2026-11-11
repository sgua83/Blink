module bootstrap.pe;
const u64 SECTION_BYTES=64u64;
const u64 RELOCATION_BYTES=48u64;
const u64 IMPORT_BYTES=80u64;
const u64 ABSOLUTE_BYTES=24u64;
struct PeSection {
    address name;u64 name_length;address bytes;u64 byte_length;
    u64 relocation_start;u64 relocation_count;u32 flags;u32 rva;u64 raw;
};
struct PeRelocation {
    u64 offset;u32 kind;u32 target_kind;u64 section;u64 value;
    u64 source;u64 target;
};
struct PeImport {
    address dll;u64 dll_length;address name;u64 name_length;u64 relocation;
    u64 lookup;u64 iat;u64 hint;u64 dll_at;u64 directory;
};
struct PeAbsolute {u64 page;u64 value;u64 order;};
struct PeInput {
    address sections;u64 section_count;address relocations;
    address imports;u64 import_count;address absolutes;u64 absolute_count;u64 stack_bytes;
};
struct PeLayout {
    u64 length;u64 header;u64 idata;u64 idata_length;u64 import_directory_size;
    u64 iat_start;u64 iat_size;u64 reloc;u64 reloc_length;u64 pdata;u64 pdata_length;
    u64 section_count;u64 image_size;u64 image_base;u64 error;
};
extern "C" PeLayout layout(PeInput input) on cpu;
extern "C" void encode(PeInput input,PeLayout layout,address output) on cpu;
extern "C" u64 stack_reserve(u32 stack_bytes) on cpu;
