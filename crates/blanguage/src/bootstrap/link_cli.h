module bootstrap.link_cli;

// Argument bytes and the input index array are borrowed for the duration of the call.
struct LinkArgument {
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
extern "C" LinkCommandLine parse_arguments(address arguments, u64 count, address input_indices) on cpu;
