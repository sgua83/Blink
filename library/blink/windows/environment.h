module blink.windows.environment;

extern "C" address GetEnvironmentStringsW() on cpu
    from "KERNEL32.dll";

extern "C" i32 FreeEnvironmentStringsW(address block) on cpu
    from "KERNEL32.dll";

extern "C" u32 GetEnvironmentVariableW(address name,address buffer,u32 capacity) on cpu
    from "KERNEL32.dll";

extern "C" i32 SetEnvironmentVariableW(address name,address value) on cpu
    from "KERNEL32.dll";

enum VariableError : u32 {
    System(u32) = 1;
    BufferTooSmall(u32) = 2;
};

result<dataset<u16>,VariableError> variable(address name) on cpu;
