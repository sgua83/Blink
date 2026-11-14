module bootstrap.operator_types;
// Scalar IDs are the original compiler Scalar order. These records borrow no
// native containers and carry only actual type facts and the final decision.
struct Operand {u64 components;u64 rows;u64 columns;u32 scalar;u32 flags;};
struct Plan {
    u32 status;u32 kind;u32 scalar;u32 cast_left;u32 cast_right;u32 comparison;u32 operation;
    u64 rows;u64 inner;u64 columns;u64 count;
};
struct Queries {
    address context;
    cfn<bool,address> same;
    cfn<bool,address,u32,u32,u64> builtin;
    cfn<bool,address,u32> numeric;
    cfn<bool,address,u32> views;
};
extern "C" Plan numeric(Operand a,bool a_literal,Operand b,bool b_literal) on cpu;
extern "C" Plan binary(address bytes,u64 length,Operand a,bool a_literal,Operand b,bool b_literal,Queries queries) on cpu
    requires nonzero(queries.same)
    requires nonzero(queries.builtin)
    requires nonzero(queries.numeric)
    requires nonzero(queries.views);
extern "C" Plan unary(address bytes,u64 length,Operand value,Queries queries) on cpu
    requires nonzero(queries.builtin)
    requires nonzero(queries.numeric);
