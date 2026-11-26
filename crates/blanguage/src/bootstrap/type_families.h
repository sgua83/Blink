module bootstrap.type_families;
import bootstrap.ast_storage;
import bootstrap.names;

struct BuiltinName {
    array<u8,6> bytes;
    u32 length;
};

// Structural kinds carry the name identity. Only intrinsic values carry layout data.
enum BuiltinKind:u32 {
    Missing=0; Numeric=1; Address=2; Handle=3; RayHit=4;
    Array=5; Result=6; Owned=7; Task=8; TaskStartError=9; Void=10; Never=11; CFunction=12;
};
enum BuiltinScalar:u32 { F32=0; U32=1; I32=2; Bool=3; U64=4; I64=5; F64=6; U8=7; I8=8; U16=9; I16=10; };
struct BuiltinType { BuiltinKind kind; BuiltinScalar scalar; u32 rows; u32 columns; };
extern "C" BuiltinType recognize(address name, u64 length) on cpu;

// Output has space for all 19 numeric names. Only the returned prefix is written.
extern "C" u32 builtin_members(read TypeFamilyNode family, address output) on cpu;
extern "C" bool builtin_has_member(read TypeFamilyNode family, address name, usize length) on cpu;

// The live declaration owns the storage borrowed by the cursor.
enum NamedSetUse:u32 { Validate=0; Match=1; Enumerate=2; };
enum NamedSetNeed:u32 { Empty=0; Enter=1; Recursive=2; Wrapped=3; Bind=4; Done=5; };
struct NamedSetCursor {
    StorageView storage;
    Slice members;
    u64 next;
    NamedSetUse use;
    u32 phase;
    u32 matched;
};
struct NamedSetStep { NamedSetNeed need; u32 matched; TypeRef member; };
extern "C" NamedSetStep named_set_next(address cursor, u32 entered) on cpu;
// Return whether the bound value has an enumeration consumer.
extern "C" u32 named_set_member(address cursor, NameBytes expected, NameBytes actual) on cpu;
