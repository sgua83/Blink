module blink.windows.process;

struct ArgumentRange { u32 start; u32 length; };
struct Arguments { dataset<u16> units; dataset<ArgumentRange> ranges; };

dataset<u16> command_line() on cpu;
Arguments parse_command_line(read dataset<u16> line) on cpu;
Arguments arguments() on cpu;

struct StartupInfo {
    u32 cb;
    address reserved;
    address desktop;
    address title;
    u32 x;
    u32 y;
    u32 x_size;
    u32 y_size;
    u32 x_count_chars;
    u32 y_count_chars;
    u32 fill_attribute;
    u32 flags;
    u16 show_window;
    u16 reserved2_bytes;
    address reserved2;
    handle std_input;
    handle std_output;
    handle std_error;
};

struct ProcessInformation {
    handle process;
    handle thread;
    u32 process_id;
    u32 thread_id;
};

const u32 CREATE_UNICODE_ENVIRONMENT=0x00000400;
const u32 CREATE_NO_WINDOW=0x08000000;
const u32 STARTF_USESTDHANDLES=0x00000100;

const u32 STD_INPUT_HANDLE=0xfffffff6;
const u32 STD_OUTPUT_HANDLE=0xfffffff5;
const u32 STD_ERROR_HANDLE=0xfffffff4;

extern "C" handle GetStdHandle(u32 standard_handle) on cpu from "KERNEL32.dll";

extern "C" i32 CreateProcessW(
    address application,address command_line,
    address process_attributes,address thread_attributes,
    i32 inherit_handles,u32 creation_flags,address environment,address directory,
    read StartupInfo startup,write ProcessInformation info
) on cpu ensures initialized(info,returned != 0)
    from "KERNEL32.dll";

extern "C" handle GetCurrentProcess() on cpu ensures memory(read) from "KERNEL32.dll";

extern "C" i32 GetExitCodeProcess(handle process,write u32 code) on cpu
    ensures initialized(code,returned != 0)
    from "KERNEL32.dll";

const u32 HANDLE_FLAG_INHERIT=0x00000001;

extern "C" i32 SetHandleInformation(handle object,u32 mask,u32 flags) on cpu
    from "KERNEL32.dll";

const u32 PROCESS_VM_READ=0x00000010;
const u32 PROCESS_QUERY_INFORMATION=0x00000400;
const u32 PROCESS_QUERY_LIMITED_INFORMATION=0x00001000;

extern "C" handle OpenProcess(u32 access,i32 inherit_handle,u32 process_id) on cpu
    from "KERNEL32.dll";

extern "C" i32 ReadProcessMemory(
    handle process,address source,address buffer,usize bytes,inout usize transferred
) on cpu from "KERNEL32.dll";

const u32 MEM_COMMIT=0x00001000;
const u32 MEM_RESERVE=0x00002000;
const u32 MEM_FREE=0x00010000;
const u32 MEM_PRIVATE=0x00020000;
const u32 MEM_MAPPED=0x00040000;
const u32 MEM_IMAGE=0x01000000;

struct MemoryBasicInformation {
    address base_address;
    address allocation_base;
    u32 allocation_protect;
    u16 partition_id;
    usize region_size;
    u32 state;
    u32 protect;
    u32 type;
};

extern "C" usize VirtualQueryEx(
    handle process,address location,inout MemoryBasicInformation info,usize bytes
) on cpu from "KERNEL32.dll";

struct ProcessMemoryCountersEx {
    u32 size;
    u32 page_fault_count;
    usize peak_working_set_bytes;
    usize working_set_bytes;
    usize peak_paged_pool_bytes;
    usize paged_pool_bytes;
    usize peak_nonpaged_pool_bytes;
    usize nonpaged_pool_bytes;
    usize commit_bytes;
    usize peak_commit_bytes;
    usize private_commit_bytes;
};

result<ProcessMemoryCountersEx,u32> process_memory_counters(handle process) on cpu;
