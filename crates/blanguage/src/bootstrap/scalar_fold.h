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


// Synchronous numerical results; the caller owns the original range container.
struct FloatInterval { f64 lower; f64 upper; u32 present; };
// state: 0 unknown, 1 one bounded interval, 2 empty set.
struct FloatRange { u64 lower; u64 upper; u32 state; };
struct FloatPieces { BitRange first; BitRange second; u32 count; };
extern "C" u64 float_order(u32 width,u64 value) on cpu;
extern "C" u64 float_bits(u32 width,u64 value) on cpu;
extern "C" f64 float_number(u32 width,u64 value) on cpu;
extern "C" FloatInterval float_finite(u32 width,BitRange range) on cpu;
extern "C" f64 float_rounded(u32 width,f64 value,u32 lower) on cpu;
extern "C" f64 float_sum_bound(u32 width,f64 left,f64 right,u32 lower) on cpu;
extern "C" FloatRange float_interval(u32 width,f64 low,f64 high) on cpu;
extern "C" u32 float_comparison_possible(BinaryOperation operation,u32 width,BitRange left,BitRange right,u32 truth_code) on cpu;
extern "C" FloatPieces float_bitcast(u32 width,BitRange range,u32 source_float) on cpu;
extern "C" FloatRange float_cast_range(ScalarEncoding source,ScalarEncoding target,BitRange range) on cpu;
extern "C" FloatRange float_range_arithmetic(BinaryOperation operation,u32 width,BitRange left,BitRange right) on cpu;
// math: 1 min, 2 max, 3 abs, 4 floor, 5 log, 6 log2; other operations remain unknown.
extern "C" FloatRange float_math(u32 operation,u32 width,BitRange left,BitRange right) on cpu;
extern "C" FloatRange float_sum_range(u32 width,BitRange range,u64 maximum) on cpu;
extern "C" FloatRange float_negate(u32 width,BitRange range) on cpu;
extern "C" f64 float_add_up(f64 left,f64 right) on cpu;
extern "C" f64 float_multiply_up(f64 left,f64 right) on cpu;
extern "C" f64 float_divide_up(f64 left,f64 right) on cpu;
extern "C" f64 float_integer_bound(u64 value,u32 lower) on cpu;
extern "C" f64 float_conversion_error(u64 upper) on cpu;
