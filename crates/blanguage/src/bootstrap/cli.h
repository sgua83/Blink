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
    u64 input_count;
    u64 budget;
    u64 has_budget;
};

// input_indices has room for count u64 indices.
extern "C" LinkCommandLine parse_link_arguments(address arguments, u64 count, address input_indices) on cpu;

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
    CodeUsage=24; ObjectCodeUsage=25;
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
    u64 program_index;
    u64 report_index;
    u64 budget;
    u64 has_budget;
    u64 as_module;
    u64 jobs;
    u64 cpu_count;
    u64 has_parameter_limit;
};

// The caller owns count role bytes and enough CPU slots for the longest argument.
extern "C" CompileCommandLine parse_compile_arguments(address arguments, u64 count, address roles, address cpus) on cpu;

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
