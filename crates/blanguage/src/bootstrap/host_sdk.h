module bootstrap.host_sdk;

struct SdkRoot {
    address path;
    u32 units;
    i32 status;
};

extern "C" SdkRoot read_sdk_root() on cpu;
extern "C" i32 release_sdk_root(address path) on cpu;

struct SdkVersionResult { u32 units; u32 status; };

// pattern is a terminated Win32 search path. selected holds 260 UTF-16 units.
extern "C" SdkVersionResult find_sdk_version(address pattern, address selected) on cpu;
