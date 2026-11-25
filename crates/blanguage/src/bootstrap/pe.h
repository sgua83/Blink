module bootstrap.pe;
const u64 IMPORT_BYTES=64u64;
struct PeSection {
    u64 rva;u64 data_offset;u64 relocation_start;
};
// Compacted rows reuse the relocation index for a relative thunk. Only a
// DLL's first written row retains its DLL source and directory location.
struct PeImport {
    address dll;u64 dll_length;address name;u64 name_length;u64 relocation_or_thunk;
    u64 hint;u64 dll_at;u64 directory;
};
// The original order is used only for sorting. Layout then stores the
// exclusive end index in the first row of each page.
struct PeAbsolute {u64 location;u64 order_or_page_end;};
struct PeInput {
    address sections;u64 group_count;address relocations;
    address imports;u64 import_count;address absolutes;u64 absolute_count;u64 stack_reserve;
};
struct PeLayout {
    u64 length;u64 header;u64 idata;u64 idata_length;u64 import_directory_size;u64 import_count;
    u64 iat_start;u64 iat_size;u64 reloc;u64 reloc_length;u64 pdata;u64 pdata_length;
    u64 section_count;u64 image_size;u64 image_base;u64 error;
    u64 idata_file;u64 reloc_file;
};
struct PeRelocationResult { u32 status; u32 kind; };

extern "C" PeLayout layout(PeInput input,address rows,address groups,u64 relocation_count) on cpu;
// Initializes the complete file range before returning success.
extern "C" PeRelocationResult encode(PeInput input,PeLayout layout,address rows,address groups,u64 row_count,address output) on cpu;
