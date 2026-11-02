module blink.windows.path;

struct ExistsCloseError { handle handle; u32 code; };
enum ExistsError : u32 {
    Query(u32) = 1;
    Close(ExistsCloseError) = 2;
};

result<bool,ExistsError> try_exists(address path) on cpu;

enum AbsoluteError : u32 {
    System(u32) = 1;
    BufferTooSmall(u32) = 2;
    LengthOverflow = 3;
};

result<dataset<u16>,AbsoluteError> absolute(address path) on cpu;

enum CanonicalQueryError : u32 {
    System(u32) = 1;
    BufferTooSmall(u32) = 2;
};
struct CanonicalCloseError { handle handle; u32 code; };
struct CanonicalQueryCloseError { CanonicalQueryError query; CanonicalCloseError close; };
enum CanonicalError : u32 {
    Open(u32) = 1;
    Query(CanonicalQueryError) = 2;
    Close(CanonicalCloseError) = 3;
    QueryAndClose(CanonicalQueryCloseError) = 4;
    Input(AbsoluteError) = 5;
};

result<dataset<u16>,CanonicalError> canonicalize(address path) on cpu;

enum ExtensionError : u32 {
    Separator(u32) = 1;
    LengthOverflow = 2;
};
result<dataset<u16>,ExtensionError> with_extension(
    read dataset<u16> path,read dataset<u16> extension) on cpu;

enum JoinError : u32 { LengthOverflow = 1; };
result<dataset<u16>,JoinError> join(read dataset<u16> base,read dataset<u16> path) on cpu;

struct DirectoryCloseError { handle handle; u32 code; };
struct DirectoryQueryCloseError { u32 query; DirectoryCloseError close; };
enum DirectoryError : u32 {
    Input(AbsoluteError) = 1;
    Create(u32) = 2;
    Open(u32) = 3;
    Query(u32) = 4;
    Close(DirectoryCloseError) = 5;
    QueryAndClose(DirectoryQueryCloseError) = 6;
    NotDirectory = 7;
};
result<void,DirectoryError> create_dir_all(address path) on cpu;
