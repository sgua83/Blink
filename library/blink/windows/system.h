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
