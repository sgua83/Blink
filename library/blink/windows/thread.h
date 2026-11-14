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

struct GroupAffinity {
    usize mask;
    u16 group;
    array<u16,3> reserved;
};

extern "C" handle GetCurrentThread() on cpu ensures memory(read) from "KERNEL32.dll";
extern "C" i32 SetThreadGroupAffinity(handle thread,read GroupAffinity affinity,address previous) on cpu
    from "KERNEL32.dll";

const u32 THREAD_SUSPEND_RESUME=0x00000002;
const u32 THREAD_GET_CONTEXT=0x00000008;
const u32 THREAD_SET_CONTEXT=0x00000010;
const u32 THREAD_QUERY_INFORMATION=0x00000040;

extern "C" handle OpenThread(u32 access,i32 inherit_handle,u32 thread_id) on cpu
    from "KERNEL32.dll";
extern "C" u32 SuspendThread(handle thread) on cpu from "KERNEL32.dll";
extern "C" u32 ResumeThread(handle thread) on cpu from "KERNEL32.dll";
