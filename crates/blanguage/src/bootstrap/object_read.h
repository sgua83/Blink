module bootstrap.object_read;
import bootstrap.coff_read;

// Borrowed rows replace the original physical-to-loaded section map.
const u64 OBJECT_SECTION_BYTES=72u64;
struct ObjectSection {
    ReadBytes name;ReadBytes bytes;u64 flags;u64 relocations;u64 relocation_count;u64 mapped;u64 zero_fill;
};
struct ObjectSectionResult {ObjectSection value;u32 error;};
struct ObjectRows {address values;u64 count;};
struct ObjectSymbol {ReadBytes name;u32 value;i16 section;u8 external;u8 function;};
struct ObjectSymbols {address values;u64 count;};
struct ObjectAssociation {u64 present;u64 owner;u32 error;};
struct ObjectTarget {
    u64 kind;u64 section;u64 value;ReadBytes dll;ReadBytes name;u32 error;
};
extern "C" ObjectSectionResult logical_section(ReadBytes input,ReadBytes headers,u64 index,inout u64 mapped) on cpu;
extern "C" ObjectAssociation association(ObjectRows rows,u64 selection,u64 owner) on cpu;
struct ObjectRelocation {ReadRelocation record;ObjectTarget target;};
extern "C" ObjectRelocation object_relocation(
    ReadBytes input,ObjectRows rows,ObjectSymbols symbols,u64 table,u64 index
) on cpu;
extern "C" u32 recover_stack(ReadBytes input,ObjectRows rows,address frames,u64 count,address calls,address nodes) on cpu;
