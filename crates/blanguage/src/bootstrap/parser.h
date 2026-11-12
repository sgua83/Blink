module bootstrap.parser;
import bootstrap.lexer;
import bootstrap.ast_storage;
struct AngleEnd {u64 end;u32 present;};
extern "C" AngleEnd angle_end(address source,address tokens,u64 count,u64 at) on cpu;

struct LinkStringResult {
    u64 count;
    u32 error;
};
// Source is the quoted token interior; output has length bytes of writable storage.
// Both addresses are borrowed only for this synchronous call.
extern "C" LinkStringResult decode_link_string(address source,u64 length,address output) on cpu;

// One synchronous parser cursor borrows the original Source and lexer spans.
// Storage owns all completed syntax; no interior storage pointer is retained.
struct ParseError {u32 kind;u32 detail;StorageError storage;StorageError cleanup;Span span;Text expected;};
struct ParseInputs {address source;address tokens;u64 count;address storage;};
struct ParseState {u64 at;ParseError error;};
extern "C" Text parse_type_name(read ParseInputs input,inout ParseState state) on cpu;
struct LiteralResult {ExprNode expression;ParseError error;};
struct ParsedParameters {Slice types;Slice values;};
extern "C" ExprRef parse_expression(read ParseInputs input,inout ParseState state,u32 minimum,u32 type_argument,
    cfn<LiteralResult,address,Span> literal,address context) on cpu requires nonzero(literal);
extern "C" TypeRef parse_type(read ParseInputs input,inout ParseState state,cfn<LiteralResult,address,Span> literal,address context) on cpu requires nonzero(literal);
extern "C" ContractRef parse_contract(read ParseInputs input,inout ParseState state,cfn<LiteralResult,address,Span> literal,address context) on cpu requires nonzero(literal);
extern "C" ParsedParameters parse_parameters(read ParseInputs input,inout ParseState state,cfn<LiteralResult,address,Span> literal,address context) on cpu requires nonzero(literal);
extern "C" Slice parse_generic_arguments(read ParseInputs input,inout ParseState state,cfn<LiteralResult,address,Span> literal,address context) on cpu requires nonzero(literal);
extern "C" u32 looks_like_declaration(read ParseInputs input,read ParseState state) on cpu;

extern "C" Text parse_qualified(read ParseInputs input,inout ParseState state) on cpu;
extern "C" ShapeRef parse_shape(read ParseInputs input,inout ParseState state,cfn<LiteralResult,address,Span> literal,address context) on cpu requires nonzero(literal);

extern "C" Slice parse_block(read ParseInputs input,inout ParseState state,cfn<LiteralResult,address,Span> literal,address context) on cpu requires nonzero(literal);
