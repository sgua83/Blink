module blink.windows.file;
import blink.sequence;
import blink.unicode;

extern "C" address CreateFileW(address path,u32 access,u32 sharing,address security,
    u32 creation,u32 flags,address template_file) on cpu
    from "KERNEL32.dll";
extern "C" i32 ReadFile(address file,address buffer,u32 capacity,write u32 received,
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

result<dataset<u8>,SourceError> read_utf8(address path) on cpu;