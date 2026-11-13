module bootstrap.object_read;
import bootstrap.coff_read;

// Borrowed rows replace the original physical-to-loaded section map.
struct ObjectSection {
    ReadBytes name;ReadBytes bytes;u64 flags;u64 relocations;u64 relocation_count;u64 mapped;u64 zero_fill;
    address decoded_relocations;u64 decoded_count;
};
struct ObjectSectionResult {ObjectSection value;u32 error;};
struct ObjectParts { ReadBytes metadata; ReadBytes runtime; };
struct ObjectRows {address values;u64 count;};
struct ObjectSymbol {ReadBytes name;u32 value;i16 section;u8 external;u8 function;};
struct ObjectSymbols {address values;u64 count;};
struct ObjectSymbolResult {ObjectSymbol value;u64 next;u32 error;};
struct SectionDefinition {u16 owner;u8 selection;u8 present;};
struct ObjectAssociation {u64 present;u64 owner;u32 error;};
struct ObjectTarget {
    u64 kind;u64 section;u64 value;ReadBytes dll;ReadBytes name;u32 error;
};
// parts borrows the first metadata and runtime sections of this same input.
extern "C" ObjectSectionResult logical_section(ReadBytes input, ReadBytes headers, u64 index,
    inout u64 mapped, inout ObjectParts parts) on cpu;
extern "C" ObjectSymbolResult object_symbol(
    ReadBytes input,ReadSymbols table,u64 index,address definitions,u64 definition_count
) on cpu;
extern "C" ObjectAssociation association(ObjectRows rows,address definitions,u64 index) on cpu;
struct ObjectRelocation {ReadRelocation record;ObjectTarget target;};
extern "C" ObjectRelocation object_relocation(
    ReadBytes input,ObjectRows rows,ObjectSymbols symbols,u64 table,u64 index
) on cpu;
// Decoded relocations borrow each owning section's records. Offset is their
// first field and stride is the size of one complete owning record.
extern "C" u32 recover_stack(ObjectRows rows,address frames,u64 count,address calls,address nodes,u64 stride) on cpu;
