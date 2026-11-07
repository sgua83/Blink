module bootstrap.parser;
import bootstrap.lexer;
struct AngleEnd {u64 end;u32 present;};
extern "C" AngleEnd angle_end(address source,address tokens,u64 count,u64 at) on cpu;

struct LinkStringResult {
    u64 count;
    u32 error;
};
// Source is the quoted token interior; output has length bytes of writable storage.
// Both addresses are borrowed only for this synchronous call.
extern "C" LinkStringResult decode_link_string(address source,u64 length,address output) on cpu;
