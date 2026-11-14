module bootstrap.scalar_fold;

enum UnaryOperation : u32 {
    Negate = 0; LogicalNot = 1; BitNot = 2; FloatNegate = 3;
};

enum BinaryOperation : u32 {
    Add = 0; Subtract = 1; Multiply = 2; Divide = 3;
    And = 4; Or = 5; Xor = 6;
    ShiftLeft = 7; ShiftRight = 8; ShiftRightSigned = 9;
    Equal = 10; NotEqual = 11; Less = 12; Greater = 13; LessEqual = 14; GreaterEqual = 15;
};

extern "C" u64 unary(UnaryOperation operation, u32 width, u64 bits) on cpu;
extern "C" u64 binary(BinaryOperation operation, u32 width, u32 signed, u64 left, u64 right) on cpu;
extern "C" u64 float_binary(BinaryOperation operation, u32 width, u64 left, u64 right) on cpu;
extern "C" u64 divide_unsigned(u64 left, u64 right, u32 remainder) on cpu requires nonzero(right);
extern "C" i64 divide_signed(i64 left, i64 right, u32 remainder) on cpu requires nonzero(right);

enum ScalarKind : u32 {
    Unsigned = 0; Signed = 1; Float = 2; Boolean = 3;
};

struct ScalarEncoding {
    ScalarKind kind;
    u32 width;
};

struct FoldedCast {
    u64 bits;
    u32 present;
};

extern "C" FoldedCast cast(ScalarEncoding source, ScalarEncoding target, u64 bits) on cpu;

// All records contain actual scalar values. The interval reader borrows the
// caller's original storage only until this synchronous query returns.
struct BitRange { u64 lower; u64 upper; };
struct BitPattern { u64 mask; u64 value; };
struct RangeQuery { u64 order_bias; BitRange range; BitPattern known; BitPattern required; };
extern "C" bool covers_range(read RangeQuery query, address intervals, u64 count,
    cfn<BitRange,address,u64> interval) on cpu requires nonzero(interval);
