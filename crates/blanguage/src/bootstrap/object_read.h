module bootstrap.object_read;
import blink.windows.coff;

// Borrowed rows replace the original physical-to-loaded section map.
struct ObjectSection {
    ReadBytes name;ReadBytes bytes;u64 flags;u64 relocations;u64 relocation_count;u64 mapped;u64 zero_fill;
    address decoded_relocations;u64 decoded_count;
};
struct ObjectParts { ReadBytes metadata; ReadBytes runtime; };
struct ObjectRows {address values;u64 count;};
struct ObjectSymbol {ReadBytes name;u32 value;i16 section;u8 external;u8 function;};
struct ObjectSymbols {address values;u64 count;};
struct SectionDefinition {u16 owner;u8 selection;u8 present;};
struct ObjectAssociation {u64 present;u64 owner;u32 error;};
struct ObjectTarget {
    u64 kind;u64 section;u64 value;ReadBytes dll;ReadBytes name;u32 error;
};
// parts borrows the first metadata and runtime sections of this input.
// Rows are optional for metadata consumers. Successful symbol reading writes
// one slot per physical symbol. The host callback checks the original names.
extern "C" u32 object_sections(ReadBytes input, ReadBytes headers, address rows,
    inout ObjectParts parts, address context, cfn<u32,address,ReadBytes> validate) on cpu
    requires nonzero(validate);
extern "C" u32 object_symbols(ReadBytes input, ReadSymbols table, address rows,
    address definitions, u64 definition_count, address context,
    cfn<u32,address,ReadBytes> validate) on cpu requires nonzero(validate);
extern "C" ObjectAssociation association(ObjectRows rows,address definitions,u64 index) on cpu;
struct ObjectRelocationStatus { u32 format; u32 object; };
// The accepted record borrows the original bytes only through the synchronous consumer.
extern "C" ObjectRelocationStatus object_relocations(
    ReadBytes input, ObjectRows rows, ObjectSymbols symbols, read ObjectSection row,
    address context, cfn<u32,address,u64,u32,ObjectTarget> consume
) on cpu requires nonzero(consume);
// Decoded relocations borrow each owning section's records. Offset is their
// first field and stride is the size of one complete owning record.
extern "C" u32 recover_stack(ObjectRows rows,address frames,u64 count,address calls,address nodes,u64 stride,u32 stack_output) on cpu;
