module blink.windows.debug;

const u32 EXCEPTION_DEBUG_EVENT=1;
const u32 CREATE_THREAD_DEBUG_EVENT=2;
const u32 CREATE_PROCESS_DEBUG_EVENT=3;
const u32 EXIT_THREAD_DEBUG_EVENT=4;
const u32 EXIT_PROCESS_DEBUG_EVENT=5;
const u32 LOAD_DLL_DEBUG_EVENT=6;
const u32 UNLOAD_DLL_DEBUG_EVENT=7;
const u32 OUTPUT_DEBUG_STRING_EVENT=8;
const u32 RIP_EVENT=9;

const u32 EXCEPTION_BREAKPOINT=0x80000003;
const u32 EXCEPTION_SINGLE_STEP=0x80000004;
const u32 DBG_CONTINUE=0x00010002;
const u32 DBG_EXCEPTION_NOT_HANDLED=0x80010001;

struct DebugEvent {
    u32 code;
    u32 process_id;
    u32 thread_id;
    array<usize,20> payload;
};

extern "C" i32 DebugActiveProcess(u32 process_id) on cpu from "KERNEL32.dll";
extern "C" i32 DebugActiveProcessStop(u32 process_id) on cpu from "KERNEL32.dll";
extern "C" i32 DebugSetProcessKillOnExit(i32 kill_on_exit) on cpu from "KERNEL32.dll";
extern "C" i32 DebugBreakProcess(handle process) on cpu from "KERNEL32.dll";
extern "C" i32 WaitForDebugEventEx(inout DebugEvent event,u32 milliseconds) on cpu
    from "KERNEL32.dll";
extern "C" i32 ContinueDebugEvent(u32 process_id,u32 thread_id,u32 status) on cpu
    from "KERNEL32.dll";
