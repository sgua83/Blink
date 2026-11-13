module blink.windows.time;

struct FileTime { u32 low; u32 high; };

extern "C" i32 QueryPerformanceCounter(write i64 count) on cpu
    ensures initialized(count, returned != 0)
    from "KERNEL32.dll";

extern "C" i32 QueryPerformanceFrequency(write i64 frequency) on cpu
    ensures initialized(frequency, returned != 0)
    from "KERNEL32.dll";

extern "C" i32 GetThreadTimes(
    handle thread,write FileTime created,write FileTime exited,
    write FileTime kernel,write FileTime user
) on cpu
    ensures initialized(created, returned != 0)
    ensures initialized(exited, false)
    ensures initialized(kernel, returned != 0)
    ensures initialized(user, returned != 0)
    from "KERNEL32.dll";

extern "C" i32 GetProcessTimes(
    handle process,write FileTime created,write FileTime exited,
    write FileTime kernel,write FileTime user
) on cpu
    ensures initialized(created, returned != 0)
    ensures initialized(exited, false)
    ensures initialized(kernel, returned != 0)
    ensures initialized(user, returned != 0)
    from "KERNEL32.dll";

u64 file_time_ticks(FileTime value) on cpu;
