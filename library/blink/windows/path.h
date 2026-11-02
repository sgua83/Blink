module blink.windows.path;

struct ExistsCloseError { address handle; u32 code; };
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
