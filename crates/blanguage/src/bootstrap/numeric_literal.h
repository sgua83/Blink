module bootstrap.numeric_literal;
import bootstrap.cli;

enum LiteralKind:u32 { F32=0; F64=1; U32=2; U64=3; I32=4; I64=5; };
enum LiteralError:u32 { None=0; Integer=1; Float=2; NonFinite=3; NegativeRange=4; };
struct Literal { u64 bits; LiteralKind kind; LiteralError error; };

// Source bytes stay borrowed until return. Negative is the original unary
// minus, applied after the literal's own type and validity have been selected.
extern "C" Literal decode(address bytes, u64 length, bool negative) on cpu;
