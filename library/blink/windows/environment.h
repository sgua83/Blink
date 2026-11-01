module blink.windows.environment;

extern "C" address GetEnvironmentStringsW() on cpu
    from "KERNEL32.dll";

extern "C" i32 FreeEnvironmentStringsW(address block) on cpu
    from "KERNEL32.dll";
