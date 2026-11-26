module bootstrap.static_uses;
import bootstrap.ast_storage;
import bootstrap.generic_bindings;

struct UseContext { StorageView storage; BindingView bindings; };
struct UseError { u32 code; Span span; };

extern "C" UseError check_function(UseContext context, FunctionRef function) on cpu;
extern "C" UseError check_type(UseContext context, TypeRef ty, Span span) on cpu;
extern "C" UseError check_expression(UseContext context, ExprRef expression) on cpu;
