module bootstrap.host_sdk;

// The original process-heap allocation owns the selected UTF-16 span.
struct HostWide {
    address data;
    u32 first;
    u32 units;
    i32 status;
};

enum HostText:u32 { LinkOutput=0; InstallationPath=1; ToolVersion=2; };

extern "C" HostWide read_sdk_root() on cpu;
extern "C" HostWide decode_host_text(address input, i32 bytes, HostText kind) on cpu;
extern "C" i32 release_host_wide(address data) on cpu;

struct SdkVersionResult { u32 units; u32 status; };

// pattern is a terminated Win32 search path. selected holds 260 UTF-16 units.
extern "C" SdkVersionResult find_sdk_version(address pattern, address selected) on cpu;
