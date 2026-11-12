module bootstrap.host_sdk;

struct SdkRoot {
    address path;
    u32 units;
    i32 status;
};

extern "C" SdkRoot read_sdk_root() on cpu;
extern "C" i32 release_sdk_root(address path) on cpu;
