module bootstrap.host_sdk;
import bootstrap.ast_storage;
import bootstrap.metadata;
import bootstrap.native_select;
import blink.windows.path_core;

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

struct HostFileOpen { handle file; u32 status; };

// The caller owns the returned handle, with only shared reading permitted.
extern "C" HostFileOpen open_shared_read(address path, usize units) on cpu;

struct SdkVersionResult { u32 units; u32 status; };

// pattern borrows units UTF-16 code units followed by NUL. selected holds 260 units.
extern "C" SdkVersionResult find_sdk_version(address pattern, usize units, address selected) on cpu;

enum HostWriteKind:u32 { Written=0; OpenError=1; WriteError=2; NoProgress=3; CloseError=4; };
struct HostWriteResult { HostWriteKind kind; u32 status; };

// The path and bytes are borrowed until the synchronous file operation returns.
extern "C" HostWriteResult write_host_input(address path, usize units, address data, usize bytes) on cpu;
extern "C" HostWriteResult write_host_output(address path, usize units, address data, usize bytes) on cpu;
extern "C" HostWriteResult create_host_output(address path, usize units, address data, usize bytes) on cpu;
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

struct HostFilePart { address data; usize bytes; };

struct LinkBindings { MetadataBytes header; MetadataBytes rust; MetadataBytes prefix; };
enum LinkOutputFailure:u32 { None=0; Native=1; MissingArchive=2; LibraryName=3; NoProduct=4; };
struct LinkOutputResult { HostWriteResult written; LinkOutputFailure failure; };

// Paths are NUL-terminated borrows from the same resolved output.
// The selected bindings and callback data remain live for this operation.
extern "C" LinkOutputResult deliver_link_output(UnitSpan path, UnitSpan parent, EntryPlan plan, LinkBindings bindings, address context,
    cfn<HostFilePart,address,u64> pe, cfn<u32,address,u64,u32> msvc) on cpu
    requires nonzero(pe) requires nonzero(msvc);

enum MsvcInputKind:u32 { Archive=0; Code=1; Imports=2; Resources=3; };
struct MsvcLinkInput { UnitSpan filename; HostFilePart bytes; u32 failed; };
struct MsvcLinkRequest {
    UnitSpan directory;
    UnitSpan executable;
    usize archives;
    usize objects;
    address paths;
    address context;
};
struct MsvcLinkResult { HostWriteResult written; HostProcessOutput process; u32 cleanup; };

// One path array holds the created prefix and feeds argument encoding.
// Input bytes are borrowed through their write. Argument bytes live through the process.
extern "C" MsvcLinkResult link_msvc_inputs(MsvcLinkRequest request,
    cfn<MsvcLinkInput,address,MsvcInputKind,usize> input,
    cfn<UnitSpan,address,address,usize> arguments) on cpu
    requires nonzero(input) requires nonzero(arguments);

struct HostPublishResult { HostWriteResult written; u32 replacement; u32 cleanup; };
extern "C" HostPublishResult publish_host_file(address target,usize target_units,
    address temporary,usize temporary_units,address parts,usize count) on cpu;

struct HostProcessExit { u32 exit_code; u32 status; };
// Arguments are consecutive NUL-terminated UTF-16 spans. stdio borrows three handles.
// A zero mask keeps the ordinary process choice; otherwise the Job binds the selected group.
extern "C" HostProcessExit run_compiler_process(address directory,address arguments,
    usize argument_units,address stdio,usize mask,u16 group) on cpu;

// Paths own the original Windows heap allocations, including the terminating NUL.
// The link caller borrows them through argument construction and process completion.
enum MsvcDiscovery:u32 {
    Found=0; System=1; Locator=2; MissingTools=3; FileRead=4;
    VersionText=5; SdkRoot=6; MissingSdk=7; InvalidPath=8; TextLength=9;
};
struct MsvcTools {
    array<UnitSpan,4> paths;
    HostProcessOutput output;
    MsvcDiscovery failure;
    u32 status;
};
extern "C" MsvcTools discover_msvc_tools(UnitSpan program_files) on cpu;
