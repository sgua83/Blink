module blink.windows.directory;
import blink.windows.time;

struct FindData {
    u32 attributes;
    FileTime created;
    FileTime accessed;
    FileTime written;
    u32 size_high;
    u32 size_low;
    u32 reserved0;
    u32 reserved1;
    array<u16,260> name;
    array<u16,14> alternate_name;
};

extern "C" handle FindFirstFileW(address pattern,inout FindData data) on cpu
    from "KERNEL32.dll";
extern "C" i32 FindNextFileW(handle search,inout FindData data) on cpu
    from "KERNEL32.dll";
extern "C" i32 FindClose(handle search) on cpu from "KERNEL32.dll";
