module blink.windows.error;

extern "C" u32 GetLastError() on cpu
    from "KERNEL32.dll";

extern "C" void SetLastError(u32 code) on cpu
    from "KERNEL32.dll";
