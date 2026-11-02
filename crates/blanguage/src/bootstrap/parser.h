module bootstrap.parser;
import bootstrap.lexer;
struct AngleEnd {u64 end;u32 present;};
extern "C" AngleEnd angle_end(address source,address tokens,u64 count,u64 at) on cpu;
