module bootstrap.static_uses;
import bootstrap.ast_storage;

struct BoundName { address bytes; u64 length; u32 kind; u32 wrapped; };
struct UseContext { StorageView storage; address bindings; u64 count; };
struct UseError { u32 code; Span span; };

extern "C" UseError check_function(UseContext context, FunctionRef function) on cpu;
extern "C" UseError check_type(UseContext context, TypeRef ty, Span span) on cpu;
extern "C" UseError check_expression(UseContext context, ExprRef expression) on cpu;
