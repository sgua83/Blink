module codec.builtin_type;

// Structural kinds carry the name identity. Only intrinsic values carry layout data.
enum BuiltinKind:u32 {
    Missing=0; Numeric=1; Address=2; Handle=3; RayHit=4;
    Array=5; Result=6; Owned=7; Task=8; TaskStartError=9; Void=10; Never=11; CFunction=12;
};
enum BuiltinScalar:u32 { F32=0; U32=1; I32=2; Bool=3; U64=4; I64=5; F64=6; U8=7; I8=8; U16=9; I16=10; };
struct BuiltinType { BuiltinKind kind; BuiltinScalar scalar; u32 rows; u32 columns; };
extern "C" BuiltinType recognize(address name, u64 length) on cpu;

u8 name_byte(address name, u64 index) on cpu;

enum CollectiveOperation:u32 { Sum=0; Min=1; Max=2; BitAnd=3; BitOr=4; BitXor=5; };
extern "C" u64 collective_identity(CollectiveOperation operation, BuiltinScalar scalar,
    u32 width, u32 signed) on cpu;
