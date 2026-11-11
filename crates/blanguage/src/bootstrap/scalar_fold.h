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
