module codec.number_text;

enum NumberError:u32 { None=0; Empty=1; Invalid=2; Overflow=3; };
struct ParsedNumber { u64 value; NumberError error; };

// Borrowed ASCII digits, with an optional leading plus and no radix prefix.
// Compiler literals and CLI arguments share the same numeric interpretation.
extern "C" ParsedNumber decimal_number(address bytes, u64 length, u64 maximum) on cpu;
extern "C" ParsedNumber hexadecimal_number(address bytes, u64 length, u64 maximum) on cpu;
ParsedNumber unsigned_number<const Radix:u32 in {10,16}>(address bytes, u64 begin, u64 end, u64 maximum) on cpu;
