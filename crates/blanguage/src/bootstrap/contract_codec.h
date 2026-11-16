module bootstrap.contract_codec;
// Synchronous typed projections of the original contract values. They borrow
// actual fields only; no node registry, schema interpreter or copied type tree.
struct Bytes { address data; u64 count; };
struct Span { address data; u64 count; u64 stride; };
struct Component { u32 scalar; u32 word; };
struct Field { Bytes name; u32 word; address ty; };
struct Variant { Bytes name; u32 discriminant; address ty; };
struct Parameter { u32 access; address ty; };
struct Function { address result; Span parameters; Span contracts; };
struct Type {
 Bytes name; Span arguments; Span values; u32 size; u32 alignment;
 Span components; Span fields; address view; address array;
 u32 enumeration; u32 result; Span variants; address dataset;
 Bytes cleanup; u32 cleanup_present; address function;
};
struct TypeName {
 Bytes name; u32 canonical; Span arguments; u32 dataset; u32 view; u32 tile;
 address count; Span contracts; u32 contracts_present;
};
struct Argument { u32 kind; address value; };
struct Shape { u32 kind; u32 literal; Bytes name; address expression; };
struct Expression {
 u32 kind; u32 literal; Bytes name; address a; address b; address c;
 Span arguments; Span types; u32 types_present; address ty; u32 target;
 u64 start; u64 end;
};
struct Predicate {
 u32 kind; Bytes first; Bytes second; Bytes third; address condition;
 Span intervals; u32 access; u32 access_present;
};
struct Interval { address lower; address upper; };
struct Condition { u32 kind; u32 literal; Bytes name; address a; address b; address constant; };
struct Constant { address ty; Span bits; };
struct Host {
 cfn<void,address,address,u64> append;
 cfn<Type,address> ensures memory(read) ty;
 cfn<Component,address> ensures memory(read) component;
 cfn<Field,address> ensures memory(read) field;
 cfn<Variant,address> ensures memory(read) variant;
 cfn<Function,address> ensures memory(read) function;
 cfn<Parameter,address> ensures memory(read) parameter;
 cfn<TypeName,address> ensures memory(read) type_name;
 cfn<Argument,address> ensures memory(read) argument;
 cfn<Shape,address> ensures memory(read) shape;
 cfn<Expression,address> ensures memory(read) expression;
 cfn<Predicate,address> ensures memory(read) predicate;
 cfn<Interval,address> ensures memory(read) interval;
 cfn<Condition,address> ensures memory(read) condition;
 cfn<Constant,address> ensures memory(read) constant;
};
extern "C" void write_type(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant);
