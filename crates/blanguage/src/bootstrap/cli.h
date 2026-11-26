module bootstrap.cli;

// Argument bytes and the input index array are borrowed for the duration of the call.
struct CliArgument {
    address bytes;
    u64 length;
    u64 text;
};

enum LinkCommand:u32 {
    Link=0;
    Archive=1;
    HostArchive=2;
    StackReport=3;
    Help=4;
    Version=5;
};

enum LinkArgumentError:u32 {
    None=0;
    ProductConflict=1;
    MissingBudget=2;
    BudgetText=3;
    EmptyBudget=4;
    InvalidBudget=5;
    BudgetOverflow=6;
    MissingOutputOperand=7;
    BudgetWithoutReport=8;
    MissingOutput=9;
};

struct LinkCommandLine {
    LinkCommand command;
    LinkArgumentError error;
    u64 output_index;
    u64 budget;
    u64 has_budget;
};

// Reports the original argument index synchronously for each input.
extern "C" LinkCommandLine parse_link_arguments(address arguments, u64 count, address context, cfn<void,address,u64> append_input) on cpu
    requires nonzero(append_input);

enum CompileCommand:u32 {
    Unknown=0; Version=1; Help=2; Inputs=3; X64=4; Image=5; Code=6; Spirv=7;
    Layout=8; Ir=9; Object=10; Check=11; Build=12;
};

enum CompileError:u32 {
    None=0; MissingResourceOperand=1; ResourceText=2; DuplicateJobs=3; DuplicateCpus=4;
    DuplicateParameterLimit=5; EmptyNumber=6; InvalidNumber=7; Overflow=8; ZeroJobs=9;
    CpuSyntax=10; ResourceConflict=11; InputsUsage=12; CommandText=13; MissingSource=14;
    UnknownCommand=15; MissingOptionOperand=16; OptionText=17; UnknownOption=18; BudgetText=19;
    EntryText=20; InstanceText=21; BudgetWithoutReport=22; MissingOutput=23;
    CodeUsage=24; ObjectCodeUsage=25; IrUsage=26; IrModeRepeated=27; IrFunctionRepeated=28;
    IrStageRepeated=29; IrOption=30; IrFunctionMode=31; IrStageMode=32; IrInstanceMode=33;
    LayoutUsage=35; LayoutOption=36; X64Usage=37; X64Option=38;
    X64Repeated=39; ImageUsage=40; SpirvUsage=41;
};

enum EntryMode:u32 { Module=0; Application=1; Program=2; };

struct OperandSink {
    address context;
    cfn<void,address,u64> import_path;
    cfn<void,address,u64> instance_name;
    cfn<void,address,u16,u8> processor;
};

struct CompileCommandLine {
    CompileCommand command;
    CompileError prefix_error;
    CompileError error;
    u32 max_parameter_bytes;
    u64 command_index;
    u64 error_index;
    u64 source_index;
    u64 output_index;
    u64 root_index;
    u64 entry_index;
    u64 report_index;
    u64 budget;
    u64 has_budget;
    EntryMode mode;
    u64 jobs;
    u64 has_parameter_limit;
};

// Slices borrow original values during the image callback. The shader
// and initializer accessors interpret the original Rust element layout.
struct ExportSpan { address data; u64 length; };
enum ExportTarget:u32 { Compute=0; Cpu=1; Gpu=2; };
enum ExportContent:u32 {
    Spirv=0; SpirvJson=1; Native=2; NativeJson=3; Relocations=4; Layout=5; LayoutJson=6;
};

struct ExportGpuStage {
    ExportSpan main;
    ExportSpan tail;
    ExportSpan scan_add;
    ExportSpan dispatch;
    ExportSpan selector;
    ExportSpan finish_name;
    ExportSpan finish;
};

struct ExportGpuAdditional {
    ExportSpan sort;
    ExportSpan segmented_single_group;
    ExportSpan terminal_root;
    ExportSpan terminal_tail;
};

enum ExportContinuation:u32 { None=0; ReductionTail=1; ScanAdd=2; CompactScatter=3; SegmentedFinish=4; Scan=5; };

struct ExportCpuStage {
    address main;
    ExportContinuation continuation;
    address first;
    address second;
    bool custom_scan;
    ExportSpan initialize;
    address histogram;
    address scatter;
    address gather;
};

struct ExportHost {
    address context;
    cfn<bool,address,ExportSpan> gpu;
    cfn<bool,address,ExportSpan> cpu;
    cfn<ExportGpuStage,address,address,u64> gpu_stage;
    cfn<ExportGpuAdditional,address,u64> gpu_additional;
    cfn<ExportSpan,ExportSpan,u64> gpu_sort_shader;
    cfn<ExportCpuStage,address,u64> cpu_stage;
    cfn<address,ExportSpan,u64> cpu_initializer;
    cfn<bool,address,ExportSpan> prepare_spirv;
    cfn<bool,address,ExportSpan,ExportContent,ExportSpan> write;
};

extern "C" void export_program(ExportTarget target, ExportSpan name, u64 ordinal, address output, ExportHost host) on cpu
    requires nonzero(host.gpu) requires nonzero(host.cpu) requires nonzero(host.write);

extern "C" bool export_gpu_image(ExportHost host, ExportSpan stem, address image, u64 count) on cpu
    requires nonzero(host.gpu_stage) requires nonzero(host.gpu_additional) requires nonzero(host.gpu_sort_shader)
    requires nonzero(host.prepare_spirv) requires nonzero(host.write);

extern "C" bool export_cpu_image(ExportHost host, ExportSpan stem, address image, u64 count) on cpu
    requires nonzero(host.cpu_stage) requires nonzero(host.cpu_initializer) requires nonzero(host.write);

// The caller owns the operands received synchronously during parsing.
extern "C" CompileCommandLine parse_compile_arguments(address arguments, u64 count, OperandSink operands) on cpu
    requires nonzero(operands.import_path) requires nonzero(operands.instance_name) requires nonzero(operands.processor);

struct ObjectCommandHost {
    cfn<u32,address,u64> import_interface;
    cfn<void,address,CompileError,u64> diagnostic;
    cfn<void,address,address> compile;
};

// The parsed import indices and command line are borrowed for this call.
extern "C" void run_object_command(address command, address imports, u64 count,
    address context, ObjectCommandHost host) on cpu
    requires nonzero(host.import_interface) requires nonzero(host.diagnostic) requires nonzero(host.compile);

extern "C" void finish_object_command(address command, u64 arguments, address context,
    cfn<u32,address,u64,u64,u64> report, cfn<void,address> complete) on cpu
    requires nonzero(report) requires nonzero(complete);

enum CodeInput:u32 { Entry=0; Image=1; Object=2; };

struct CodeCommandLine {
    CodeInput input;
    CompileError error;
    u64 source_index;
    u64 observations_index;
    u64 function_index;
    u64 section;
    u64 range_start;
    u64 range_end;
    bool has_section;
    bool has_range;
};

// Returns indices into the original arguments for paths and the exact function name.
extern "C" CodeCommandLine parse_code_arguments(address arguments, u64 count) on cpu;

struct IrCommandLine {
    EntryMode mode;
    CompileError error;
    u64 error_index;
    u64 root_index;
    u64 entry_index;
    u64 function_index;
    u64 stage;
    u64 has_stage;
};

// Report operands as encountered, including those before a later syntax error.
extern "C" IrCommandLine parse_ir_arguments(address arguments, u64 count, OperandSink operands) on cpu
    requires nonzero(operands.import_path) requires nonzero(operands.instance_name);

struct LayoutCommandLine {
    CompileError error;
    u64 error_index;
    u64 root_index;
    u64 field_index;
};

struct X64CommandLine {
    CompileError error;
    u64 error_index;
    u64 ip;
    u64 offset;
    u64 length;
    u64 has_length;
};

// The original source operand is canonicalized before these options are consumed.
extern "C" LayoutCommandLine parse_layout_arguments(address arguments, u64 count) on cpu;
extern "C" X64CommandLine parse_x64_arguments(address arguments, u64 count) on cpu;

struct MsvcText { u64 start; u64 units; };
struct MsvcWide { address data; u64 units; };
struct MsvcGroup { address spans; u64 count; };
struct MsvcArguments {
    address text;
    MsvcWide output;
    MsvcGroup inputs;
    MsvcWide import_library;
    MsvcGroup search_paths;
    MsvcGroup libraries;
    MsvcGroup exports;
    u64 stack_reserve;
};
// Paths borrow MsvcWide rows. Libraries and exports use MsvcText ranges into
// the caller's encoded names. The caller owns those inputs and the output.
extern "C" u64 write_msvc_arguments(MsvcArguments input, address output) on cpu;

enum NumberError:u32 { None=0; Empty=1; Invalid=2; Overflow=3; };
struct ParsedNumber { u64 value; NumberError error; };

// Borrowed ASCII digits, with an optional leading plus and no radix prefix.
// Compiler literals and CLI arguments share the same numeric interpretation.
extern "C" ParsedNumber decimal_number(address bytes, u64 length, u64 maximum) on cpu;
extern "C" ParsedNumber hexadecimal_number(address bytes, u64 length, u64 maximum) on cpu;
