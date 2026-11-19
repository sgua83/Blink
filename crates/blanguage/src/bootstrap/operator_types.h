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

enum FixedProjectionKind:u32 { Components=0; Vector=1; Array=2; };
struct FixedProjection { FixedProjectionKind kind; usize first; };
extern "C" FixedProjection fixed_projection(u32 array, usize width, u32 known, usize index) on cpu;

enum MatrixAction:u32 { Copy=0; Multiply=1; Add=2; Subtract=3; Divide=4; Negate=5; };
struct MatrixOperand { u32 slot; u32 take; };
struct MatrixStep {
    MatrixAction action;
    MatrixOperand left;
    MatrixOperand right;
    u32 destination;
};
struct MatrixEmitter {
    address context;
    cfn<bool,address,MatrixStep> emit;
    cfn<void,address,u32> output;
};
extern "C" bool expand_square_matrix(u32 width, bool inverse, MatrixEmitter emitter) on cpu
    requires nonzero(emitter.emit) requires nonzero(emitter.output);
