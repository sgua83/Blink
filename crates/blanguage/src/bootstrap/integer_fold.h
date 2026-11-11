module bootstrap.integer_fold;

enum UnaryOperation : u32 {
    Negate = 0; LogicalNot = 1; BitNot = 2;
};

enum BinaryOperation : u32 {
    Add = 0; Subtract = 1; Multiply = 2;
    And = 3; Or = 4; Xor = 5;
    ShiftLeft = 6; ShiftRight = 7; ShiftRightSigned = 8;
    Equal = 9; NotEqual = 10; Less = 11; Greater = 12; LessEqual = 13; GreaterEqual = 14;
};

extern "C" u64 unary(UnaryOperation operation, u32 width, u64 bits) on cpu;
extern "C" u64 binary(BinaryOperation operation, u32 width, u32 signed, u64 left, u64 right) on cpu;
extern "C" u64 divide_unsigned(u64 left, u64 right, u32 remainder) on cpu requires nonzero(right);
extern "C" i64 divide_signed(i64 left, i64 right, u32 remainder) on cpu requires nonzero(right);
