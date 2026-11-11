module blink.windows.system;

struct SystemInfo {
    u16 processor_architecture;
    u16 reserved;
    u32 page_size;
    address minimum_application_address;
    address maximum_application_address;
    usize active_processor_mask;
    u32 number_of_processors;
    u32 processor_type;
    u32 allocation_granularity;
    u16 processor_level;
    u16 processor_revision;
};

extern "C" void GetNativeSystemInfo(write SystemInfo info) on cpu
    from "KERNEL32.dll";

const u32 INFINITE=0xffffffff;
const u32 WAIT_OBJECT_0=0;
const u32 WAIT_ABANDONED=0x00000080;
const u32 WAIT_TIMEOUT=0x00000102;
const u32 WAIT_FAILED=0xffffffff;
const u32 STILL_ACTIVE=259;

extern "C" u32 WaitForSingleObject(handle object,u32 milliseconds) on cpu
    from "KERNEL32.dll";

extern "C" i32 CloseHandle(handle object) on cpu
    from "KERNEL32.dll";
