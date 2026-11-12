module bootstrap.parser;
import bootstrap.ast_storage;

// One synchronous parser cursor borrows the original Source and lexer spans.
// Storage owns all completed syntax; no interior storage pointer is retained.
struct ParseError {u32 kind;u32 detail;StorageError storage;StorageError cleanup;Span span;Text expected;};
struct ParseInputs {address source;address tokens;u64 count;address storage;};
struct ParseState {u64 at;ParseError error;};
struct ParsedEntry {Text name;Optional<Slice> arguments;};
extern "C" ModuleRef parse_module(read ParseInputs input,inout ParseState state) on cpu;
extern "C" ParsedEntry parse_entry(read ParseInputs input,inout ParseState state) on cpu;
extern "C" TypeRef parse_type_query(read ParseInputs input,inout ParseState state) on cpu;
