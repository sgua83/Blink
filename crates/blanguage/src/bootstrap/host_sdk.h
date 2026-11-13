module bootstrap.host_sdk;
import bootstrap.ast_storage;

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

// pattern borrows units UTF-16 code units followed by NUL. selected holds 260 units.
extern "C" SdkVersionResult find_sdk_version(address pattern, usize units, address selected) on cpu;

enum HostInputKind:u32 { Written=0; OpenError=1; WriteError=2; NoProgress=3; };
struct HostInputResult { HostInputKind kind; u32 status; };

// The path and bytes are borrowed until the synchronous file operation returns.
extern "C" HostInputResult write_host_input(address path, usize units, address data, usize bytes) on cpu;
extern "C" u32 remove_host_input(address path, usize units) on cpu;

struct HostProcessOutput {
    SourceBytes stdout;
    SourceBytes stderr;
    u32 exit_code;
    u32 status;
};

// The caller borrows a NUL-terminated executable and consecutive NUL-terminated UTF-16 arguments.
extern "C" HostProcessOutput run_host_process(address executable, usize units,
    address arguments, usize argument_units) on cpu;
