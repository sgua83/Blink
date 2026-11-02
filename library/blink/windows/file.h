module blink.windows.file;
import blink.sequence;
import blink.unicode;

extern "C" address CreateFileW(address path,u32 access,u32 sharing,address security,
    u32 creation,u32 flags,address template_file) on cpu
    from "KERNEL32.dll";
extern "C" i32 ReadFile(address file,address buffer,u32 capacity,write u32 received,
    address overlapped) on cpu from "KERNEL32.dll";
extern "C" i32 WriteFile(address file,address buffer,u32 bytes,write u32 written,
    address overlapped) on cpu from "KERNEL32.dll";
extern "C" i32 CloseHandle(address handle) on cpu from "KERNEL32.dll";

enum ReadError : u32 {
    System(u32) = 1;
    Sequence(SequenceError) = 2;
};
struct CloseError { address handle; u32 code; };
struct ReadCloseError { ReadError read; CloseError close; };
enum SourceError : u32 {
    Open(u32) = 1;
    Read(ReadError) = 2;
    Close(CloseError) = 3;
    ReadAndClose(ReadCloseError) = 4;
    Decode(DecodeError) = 5;
};

result<dataset<u8>,SourceError> read_binary(address path) on cpu;
result<dataset<u8>,SourceError> read_utf8(address path) on cpu;
struct WriteSystemError { u32 completed; u32 code; };
enum WriteError : u32 {
    System(WriteSystemError) = 1;
    NoProgress(u32) = 2;
};
struct WriteCloseError { WriteError write; CloseError close; };
enum OutputError : u32 {
    Open(u32) = 1;
    Write(WriteError) = 2;
    Close(CloseError) = 3;
    WriteAndClose(WriteCloseError) = 4;
};

result<u32,WriteError> write_bytes(address file,read dataset<u8> data) on cpu;
result<u32,OutputError> write_binary(address path,read dataset<u8> data) on cpu;
result<u32,OutputError> create_new_binary(address path,read dataset<u8> data) on cpu;

extern "C" i32 SetFileInformationByHandle(address file,i32 information_class,
    address information,u32 bytes) on cpu from "KERNEL32.dll";

struct FileApplyCloseError { u32 code; CloseError close; };
enum FileChangeError : u32 {
    Open(u32) = 1;
    Apply(u32) = 2;
    Close(CloseError) = 3;
    ApplyAndClose(FileApplyCloseError) = 4;
};
enum RenameError : u32 {
    LengthOverflow = 1;
    Change(FileChangeError) = 2;
    Nul(u32) = 3;
};
result<void,RenameError> rename(address source,read dataset<u16> target) on cpu;
result<void,FileChangeError> remove_file(address path) on cpu;
