module spirv.scan_roles;

enum EntryOperation : u32 {
    Map = 0; Reduce = 1; Scan = 2; Segmented = 3; Compact = 4;
    Sort = 5; CustomScan = 6; CustomSegmented = 7; CustomReduce = 8;
};

struct ScanShape { EntryOperation operation; u32 selected; u32 count; u32 fixed; u32 width; };
enum ScanShader : u32 { Summary = 0; Terminal = 1; Both = 2; };
struct ScanRoles { ScanShader root; ScanShader tail; };

extern "C" u32 histogram_count(u32 count, u32 width) on cpu requires nonzero(width);
extern "C" ScanRoles scan_roles(ScanShape shape) on cpu requires nonzero(shape.width);

struct ShaderWords { address data; u64 count; };
struct StageShaders {
    ScanShape shape;
    u64 sort_keys;
    u64 additional_count;
    ShaderWords main;
    ShaderWords tail;
    ShaderWords selector;
    u32 binding;
};
enum TerminalKind : u32 { Absent = 0; Canonical = 1; Alternate = 2; };
struct TerminalEntry { u64 index; TerminalKind kind; };
struct AdditionalEntries {
    u64 sort;
    u64 single_group;
    TerminalEntry root;
    TerminalEntry tail;
};

// single_group names an additional shader or the end of the input span.
extern "C" AdditionalEntries additional_entries(StageShaders input, address context,
    cfn<ShaderWords,address,u64> ensures memory(read) additional) on cpu
    requires nonzero(additional) requires nonzero(input.shape.width);
