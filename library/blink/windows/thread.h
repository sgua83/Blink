module blink.windows.thread;

extern "C" handle CreateThread(
    address attributes,usize stack_bytes,cfn<u32,address> start,
    address parameter,u32 flags,address thread_id
) on cpu requires nonzero(start)
    from "KERNEL32.dll";

extern "C" u32 GetCurrentThreadId() on cpu ensures memory(read) from "KERNEL32.dll";
extern "C" u32 GetThreadId(handle thread) on cpu from "KERNEL32.dll";

extern "C" i32 GetExitCodeThread(handle thread,write u32 code) on cpu
    ensures initialized(code,returned != 0)
    from "KERNEL32.dll";
