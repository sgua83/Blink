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
const u32 INFINITE=0xffffffff;
const u32 WAIT_OBJECT_0=0;
const u32 WAIT_ABANDONED=0x00000080;
const u32 WAIT_TIMEOUT=0x00000102;
const u32 WAIT_FAILED=0xffffffff;
const u32 STILL_ACTIVE=259;

extern "C" i32 CreateProcessW(
    address application,address command_line,
    address process_attributes,address thread_attributes,
    i32 inherit_handles,u32 creation_flags,address environment,address directory,
    read StartupInfo startup,write ProcessInformation info
) on cpu ensures initialized(info,returned != 0)
    from "KERNEL32.dll";

extern "C" u32 WaitForSingleObject(handle object,u32 milliseconds) on cpu
    from "KERNEL32.dll";

extern "C" i32 GetExitCodeProcess(handle process,write u32 code) on cpu
    ensures initialized(code,returned != 0)
    from "KERNEL32.dll";

extern "C" i32 CloseHandle(handle object) on cpu
    from "KERNEL32.dll";
