module blink.windows.path_core;
import blink.windows.path;
import blink.windows.file;

// Synchronous borrowed UTF16. The caller keeps all input, scratch and output
// storage alive through preparation and encoding; no address is retained.
struct UnitSpan { address data; u64 length; };
struct LexicalPrefix { u64 end; bool literal; };
struct ExtensionScan {
    bool found; u64 start; u64 stem; u64 size; bool only_dots;
    bool has_dot; u64 dot; u64 end;
};
struct JoinComponent { bool from_base; u64 start; u64 length; u64 kind; };
struct JoinPlan { u64 length; u64 components; u64 base_length; u64 separator; };
struct ExtensionPlan { u64 length; u64 copy; u64 append; };
extern "C" u32 GetFullPathNameW(address path,u32 capacity,address buffer,
    address file_part) on cpu from "KERNEL32.dll";

bool lexical_separator(u16 unit,bool literal) on cpu;
LexicalPrefix lexical_prefix(UnitSpan path,bool nt_prefix) on cpu;
u64 component_capacity(UnitSpan path,bool literal) on cpu;
u64 join_components(UnitSpan path,u64 body,bool from_base,bool literal,
    address parts,u64 count) on cpu;
// Number of JoinComponent rows needed during this call, zero for direct copies.
extern "C" u64 join_scratch(UnitSpan base,UnitSpan path) on cpu;
extern "C" JoinPlan join_prepare(UnitSpan base,UnitSpan path,address scratch) on cpu;
extern "C" void join_encode(UnitSpan base,UnitSpan path,address scratch,
    JoinPlan plan,address output) on cpu;
extern "C" UnitSpan file_stem(UnitSpan path) on cpu;
extern "C" ExtensionPlan extension_prepare(UnitSpan path,u64 extension_length) on cpu;
extern "C" void extension_encode(UnitSpan path,UnitSpan extension,
    ExtensionPlan plan,address output) on cpu;
// name has two writable units after its original length.
extern "C" void module_tail(address name,u64 length,u16 suffix) on cpu;

// The input retains its NUL terminator. A nonempty allocation belongs to this call.
struct NativePath { address name; address allocation; usize units; u32 status; };
extern "C" NativePath absolute_path(address path,usize units) on cpu;
NativePath native_path(address path,usize units,bool prefer_verbatim) on cpu;
extern "C" void release_native_path(NativePath path) on cpu;
result<void,DirectoryError> directory_tree(address path,usize units,bool prefer_verbatim) on cpu;
extern "C" u32 create_output_directories(address path,usize units) on cpu;

result<void,FileChangeError> change_file(address path,i32 information_class,
    address information,u32 bytes,u32 flags) on cpu;
result<void,RenameError> rename_path(address source,UnitSpan target) on cpu;
