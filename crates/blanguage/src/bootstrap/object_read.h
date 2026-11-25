module bootstrap.object_read;
import blink.windows.coff;

import bootstrap.metadata_read;

// Borrowed rows replace the original physical-to-loaded section map.
struct ObjectSection {
    ReadBytes name;ReadBytes bytes;u64 flags;u64 relocations;u64 relocation_count;u64 mapped;u64 zero_fill;
    address decoded_relocations;u64 decoded_count;
};
struct ObjectParts { ReadBytes metadata; ReadBytes runtime; };
struct ObjectRows {address values;u64 count;};
// Import rows borrow the contiguous DLL/name payload and store its split in value.
// An invalid import keeps its read error in value until a relocation uses the row.
struct ObjectSymbol {ReadBytes name;u32 value;i16 section;u8 external;u8 role;};
struct ObjectSymbols {address values;u64 count;};
struct SectionDefinition {u16 owner;u8 selection;u8 present;};
struct ObjectAssociation {u64 present;u64 owner;u32 error;};
struct ObjectTarget {
    u64 kind;u64 section;u64 value;ReadBytes dll;ReadBytes name;u32 error;
};
struct ObjectScanStatus { u32 error; u32 complete; };
// Parts borrows the first metadata and runtime sections. Consumers can ask
// for full rows, one loadable byte per physical section, or only the parts.
// Table borrows from this input when already read or first used by a long name.
extern "C" ObjectScanStatus object_sections(ReadBytes input, ReadBytes headers, address rows, address loadable,
    inout ObjectParts parts, inout ReadSymbols table, address context, cfn<u32,address,ReadBytes> validate) on cpu
    requires nonzero(validate);
// The callback receives every primary name and the one-based section of a
// defined external symbol. Full object consumers also request physical rows
// and section definitions for relocation and association lookup.
extern "C" ObjectScanStatus object_symbols(ReadBytes input, ReadSymbols table, address rows,
    address definitions, u64 definition_count, address context,
    cfn<u32,address,ReadBytes,u64> consume) on cpu requires nonzero(consume);
extern "C" ObjectAssociation association(ObjectRows rows,address definitions,u64 index) on cpu;
struct ObjectRelocationStatus { u32 format; u32 object; };
// The accepted record borrows the original bytes only through the synchronous consumer.
extern "C" ObjectRelocationStatus object_relocations(
    ReadBytes input, ObjectRows rows, ObjectSymbols symbols, read ObjectSection row,
    address context, cfn<void,address,u64,u64,u32,ObjectTarget> consume
) on cpu requires nonzero(consume);
// Decoded relocations borrow each owning section's records. Offset is their
// first field and stride is the size of one complete owning record.
extern "C" u32 recover_stack(ObjectRows rows,address frames,u64 count,address calls,address nodes,u64 stride,u32 stack_output) on cpu;

struct RestoreSymbols { ObjectSymbols rows; address definitions; };
struct RestoreRelocations { address values; u64 count; };
struct RestoreStack { address frames; u64 count; address calls; address nodes; };
struct RestoreSource {
    ReadBytes input;
    ReadBytes headers;
    ReadSymbols table;
    ReadBytes native_name;
    address reused_prefix;
    MetaCursor reused_cursor;
    u64 format;
    u64 relocation_stride;
    u32 stack_output;
};
struct RestoreObjectSink {
    address context;
    address name_context;
    cfn<u32,address,ReadBytes,u64> symbol_name;
    cfn<u32,address,ReadBytes> section_name;
    cfn<RestoreSymbols,address,u64,u64> symbols;
    cfn<ObjectRows,address,u64> sections;
    cfn<void,address,ReadBytes> archive;
    cfn<void,address,ReadBytes,ReadBytes,u64,u64> section;
    cfn<void,address,u64,u64> associate;
    cfn<void,address,u64,u64,u32,ObjectTarget> relocation;
    cfn<RestoreRelocations,address,u64> relocated;
    cfn<u32,address,ReadBytes,u32> bind_import;
    cfn<u32,address,ReadBytes> take_contract;
    cfn<void,address,ObjectSymbol,u64,u32> definition;
    cfn<RestoreStack,address> stack;
    cfn<void,address,MetaFrame> frame;
    cfn<void,address,MetaCall> call;
};
struct RestoreObjectStatus { u32 format; u32 object; u32 metadata; };
extern "C" RestoreObjectStatus restore_object(RestoreSource source, RestoreObjectSink sink,
    MetaNameSink names, MetaReceiver metadata) on cpu
    requires nonzero(sink.symbol_name) requires nonzero(sink.section_name)
    requires nonzero(sink.symbols) requires nonzero(sink.sections) requires nonzero(sink.archive)
    requires nonzero(sink.section) requires nonzero(sink.associate) requires nonzero(sink.relocation)
    requires nonzero(sink.relocated) requires nonzero(sink.bind_import) requires nonzero(sink.take_contract) requires nonzero(sink.definition)
    requires nonzero(sink.stack) requires nonzero(sink.frame) requires nonzero(sink.call)
    requires nonzero(names.emit)
    requires nonzero(metadata.kind) requires nonzero(metadata.valid_text) requires nonzero(metadata.binding)
    requires nonzero(metadata.resource) requires nonzero(metadata.code) requires nonzero(metadata.frame)
    requires nonzero(metadata.call);
