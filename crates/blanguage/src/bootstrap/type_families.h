module bootstrap.type_families;
import bootstrap.ast_storage;
import bootstrap.names;
import codec.builtin_type;

struct BuiltinName {
    array<u8,6> bytes;
    u32 length;
};

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
