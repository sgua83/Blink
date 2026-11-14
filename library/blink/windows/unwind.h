module blink.windows.unwind;

const u32 IMAGE_FILE_MACHINE_AMD64=0x8664;
const u32 ADDR_MODE_FLAT=3;

struct Address64 {
    u64 offset;
    u16 segment;
    u32 mode;
};

struct KdHelp64 {
    u64 thread;
    u32 callback_stack;
    u32 callback_backing_store;
    u32 next_callback;
    u32 frame_pointer;
    u64 call_user_mode;
    u64 user_callback_dispatcher;
    u64 system_range_start;
    u64 user_exception_dispatcher;
    u64 stack_base;
    u64 stack_limit;
    u32 build_version;
    u32 retpoline_table_size;
    u64 retpoline_table;
    u32 retpoline_offset;
    u32 retpoline_size;
    array<u64,2> reserved;
};

struct StackFrame64 {
    Address64 pc;
    Address64 return_address;
    Address64 frame;
    Address64 stack;
    Address64 backing_store;
    address function_table_entry;
    array<u64,4> parameters;
    i32 far_call;
    i32 virtual_frame;
    array<u64,3> reserved;
    KdHelp64 kernel_help;
};

// Context and frame are the caller's writable native state. Callbacks and target
// storage stay valid through the synchronous call; the native SDK permits null callbacks.
extern "C" i32 StackWalk64(
    u32 machine,handle process,handle thread,inout StackFrame64 frame,address context,
    cfn<i32,handle,u64,address,u32,address> read_memory,
    cfn<address,handle,u64> function_table,
    cfn<u64,handle,u64> module_base,
    cfn<u64,handle,handle,address> translate
) on cpu from "DbgHelp.dll";

extern "C" i32 SymInitializeW(handle process,address search_path,i32 load_modules) on cpu
    from "DbgHelp.dll";
extern "C" i32 SymCleanup(handle process) on cpu from "DbgHelp.dll";
extern "C" address SymFunctionTableAccess64(handle process,u64 location) on cpu
    from "DbgHelp.dll";
extern "C" u64 SymGetModuleBase64(handle process,u64 location) on cpu from "DbgHelp.dll";
