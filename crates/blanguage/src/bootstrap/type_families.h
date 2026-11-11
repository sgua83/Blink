module bootstrap.type_families;
import bootstrap.ast_storage;

struct BuiltinName {
    array<u8,6> bytes;
    u32 length;
};

// Output has space for all 19 numeric names. Only the returned prefix is written.
extern "C" u32 builtin_members(read TypeFamilyNode family, address output) on cpu;
