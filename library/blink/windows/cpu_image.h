module blink.windows.cpu_image;
import blink.windows.coff;

struct ImageName {
    ReadBytes library;
    ReadBytes symbol;
};
struct ImageCode {
    ReadBytes bytes;
    ReadBytes unwind;
    u64 function_end;
    u64 patch_count;
};
enum ImageTarget:u32 { Missing=0; Root=1; Section=2; External=3; Print=4; };
struct CodePatch {
    u64 offset;
    ImageTarget target;
    address service;
    ImageName name;
};
struct ImageSection {
    ReadBytes bytes;
    u64 patch_count;
    u32 flags;
    u32 unwind;
};
struct SectionPatch {
    u64 offset;
    u64 section_index;
    u64 section_value;
    u32 kind;
    u32 external;
    ImageName name;
};
struct SymbolPosition {
    u64 index;
    u64 value;
    ImageTarget target;
};
struct ImageInput {
    address context;
    u64 code_count;
    u64 section_count;
    u32 has_module;
    ReadBytes runtime_library;
    cfn<ImageCode,address,u64> code;
    cfn<CodePatch,address,u64,u64> code_patch;
    cfn<ImageSection,address,u64> section;
    cfn<SectionPatch,address,u64,u64> section_patch;
    cfn<SymbolPosition,address,ReadBytes> symbol;
    cfn<address,address,ReadBytes> resource;
};
enum ImageFailure:u32 {
    None=0; Heap=1; Windows=2; Range=3; Unwind=4; Function=5;
    Library=6; Symbol=7; Nul=8; Resource=9;
    Relocation=10; RelocationPosition=11; RelocationValue=12;
};
struct ImageError { ImageFailure kind; u32 detail; ImageName name; };
struct ImageResult {
    address owner;
    address base;
    address table;
    address factory_count;
    address factories;
    ImageError error;
};

// The input and its callbacks are synchronous borrows. The returned owner holds
// mapped pages, registered unwind tables and loaded libraries until release.
extern "C" ImageResult load_image(ImageInput input) on cpu
    requires nonzero(input.code) requires nonzero(input.code_patch)
    requires nonzero(input.section) requires nonzero(input.section_patch)
    requires nonzero(input.symbol) requires nonzero(input.resource)
   ;
extern "C" void release_image(address owner) on cpu;

struct MemoryInformation {
    address base_address;
    address allocation_base;
    u32 allocation_protect;
    u16 partition_id;
    u64 region_size;
    u32 state;
    u32 protect;
    u32 kind;
};
struct MemoryResult { MemoryInformation info; u32 error; };
extern "C" MemoryResult query_image_memory(address entry) on cpu;
