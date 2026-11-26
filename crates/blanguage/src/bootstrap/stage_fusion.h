module bootstrap.stage_fusion;
import bootstrap.ir_dataflow;

struct BindingSpan {
    address data;
    u64 count;
};
// output contains left_count original bindings, with space for right.count more.
// The returned index starts the sorted union in that same output buffer.
extern "C" u64 merge_bindings(address output, u64 left_count, BindingSpan right) on cpu;

struct TilePlacement {
    i64 begin;
    i64 end;
    u64 first;
    u64 count;
    u64 width;
    u64 plane;
};
struct TileMember { u64 storage; u64 owner; u64 byte; u64 slot; };
struct TilePlanes { address narrow; address wide; };

// Each interval owns its original ordered members. Return the actual narrow
// extents and write slots into those same member rows.
struct TileExtents { u64 narrow; u64 wide; };
extern "C" TileExtents place_tile(address rows, u64 count, address members, address context,
    cfn<TilePlanes,address,u64,u64> reserve) on cpu requires nonzero(reserve);

struct WriteEnd { u32 end; u32 present; };
struct StageGuarantee {
    u64 binding;
    u64 start;
    u64 end;
};
enum GuaranteeKind:u32 { Prefix=0; Element=1; };
struct GuaranteeCounts { u64 prefix; u64 element; };
struct GuaranteeHost {
    address context;
    cfn<StageGuarantee,address,GuaranteeKind,u64> field;
    cfn<void,address,GuaranteeKind,u64,u64> swap_slots;
    cfn<WriteEnd,address,u64,u64> seek;
    cfn<WriteEnd,address> next;
};

// Seek to the original writes of one binding whose starts precede the field end.
// Retain original guarantee order when compacting both lists.
extern "C" GuaranteeCounts retain_stage_guarantees(address writes, GuaranteeCounts counts, GuaranteeHost host) on cpu
    requires nonzero(host.field) requires nonzero(host.swap_slots)
    requires nonzero(host.seek) requires nonzero(host.next);

struct LocalValue { u64 slot; u64 value; };

// The original IR and PrivateSlot sets stay in their native owning containers.
// Every borrowed block and temporary clone ends with this synchronous query.
enum LiveSetAction:u32 { Copy=0; Union=1; RemoveRegister=2; InsertLocal=3; RemoveLocal=4; Swap=5; };
struct LiveHost {
    Host ir;
    cfn<u32,address,usize> insert_register;
    cfn<address,address> clone_set;
    cfn<void,address> drop_set;
    cfn<void,LiveSetAction,address,address,u64> change;
};

extern "C" void private_before(Span code, address live, address break_live,
    address continue_live, address exit_live, address empty, LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.insert_register)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change);
extern "C" void private_loop(Span condition, u64 test, Span body,
    address after, address break_live, address continue_live, address exit_live,
    address result, address empty, LiveHost host) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.insert_register)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change);
// Emit borrowed code ranges and the original slot sets from the last phase to the first.
extern "C" void private_tile_phases(Span code, address live, address saved, address empty,
    LiveHost host, address context, cfn<void,address,u64,u64,address,address> emit) on cpu
    requires nonzero(host.ir.inspect) requires nonzero(host.insert_register)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change)
    requires nonzero(emit);

enum DceRemovalKind:u32 { Values=0; Division=1; DirectWrite=2; IndexedWrite=3; };
enum DceDivisionChoice:u32 { Quotient=1; Remainder=2; Both=3; };
struct InstructionStorage { address first_code; address second_code; address payload; address input; };
struct DceResults { u32 present; u32 needed; };
struct DceRemoval { DceRemovalKind kind; u64 value; u64 index; u64 length; u64 word; };
struct DceHost {
    LiveHost live;
    cfn<InstructionStorage,address> storage;
    cfn<Span,address> read_block;
    cfn<u32,address,u32,u64> contains;
    cfn<void,address,address> swap_instructions;
    cfn<void,address,u64> discard_prefix;
    cfn<void,address,address,DceRemoval> record;
    cfn<void,address,address,DceDivisionChoice,u64,u64> prune_division;
    cfn<void,address,address> prune_collective;
};

// The IR owner enumerates actual results and moves retained instructions.
// B owns every elimination and propagation decision during that borrow.
extern "C" void eliminate_code(address block,address needed,
    address break_live,address continue_live,address exit_live,address empty,
    address context,DceHost host) on cpu
    requires nonzero(host.live.ir.inspect) requires nonzero(host.storage) requires nonzero(host.read_block)
    requires nonzero(host.contains) requires nonzero(host.live.change)
    requires nonzero(host.live.clone_set) requires nonzero(host.live.drop_set)
    requires nonzero(host.swap_instructions) requires nonzero(host.discard_prefix)
    requires nonzero(host.live.insert_register) requires nonzero(host.record)
    requires nonzero(host.prune_division) requires nonzero(host.prune_collective);

enum VfRegMap:u32 { Aliases=0; Locals=1; };
enum VfDivisionChoice:u32 { Quotient=1; Remainder=2; Both=3; };
enum VfAction:u32 { Discard=0; Keep=1; Stop=2; };
enum VfReuseKind:u32 { Expression=0; LocalRead=1; IndexedStoreRead=2; IndexedRead=3; };
enum VfRole:u32 { Value=0; Quotient=1; Remainder=2; };
struct VfExpression { u32 kind; u32 operation; u64 first; u64 second; u64 third; u64 count; };
struct VfOptional { u64 value; u32 present; };
struct VfDivision { u64 instruction; u64 quotient; u64 remainder; u32 mask; };
struct VfMemory { u64 value; u64 origin; u32 present; };
struct VfReuse { address origin; VfReuseKind kind; VfRole role; u32 paired; };
struct VfResultLocation { u64 index; u64 count; ResultRole role; };
struct VfOriginHost {
    u64 stride;
    cfn<address,address,u64,u64> resize;
    cfn<void,address,u64,address,VfReuse> capture;
    cfn<void,address> discard;
};
struct VfOriginWriter {
    address sink;
    cfn<void,address,address,VfResultLocation> collect;
    cfn<void,address,address,VfReuseKind> attach;
};
struct VfRows { usize data; u64 count; u64 capacity; };
struct VfRegisters { VfRows aliases; VfRows locals; };
struct VfExpressions { VfRows entries; VfRows arguments; VfRows buckets; };
struct VfPath { VfRows rows; u32 reached; };
struct VfFlow { VfPath next; VfPath exits; };
struct VfMemoryHost {
    usize stride;
    usize origin_offset;
    cfn<address,address,u64,u64> resize;
    cfn<void,address,address,u64> move_rows;
    cfn<void,address,address> capture_origin;
    cfn<void,address,address> copy_origin;
    cfn<void,address> drop_origin;
};
struct VfHost {
    RegisterHost values;
    cfn<InstructionStorage,address> storage;
    cfn<void,address,address> remap;
    cfn<Span,address> instruction_span;
    cfn<void,address,address> swap_instructions;
    cfn<void,address,u64> truncate;
    cfn<address,address,u64,u64> resize_rows;
    cfn<address,address,u64,u64> scratch;
    cfn<void,address,VfDivisionChoice,u64,u64> rewrite_division;
    cfn<void,address,address,address,address,u64,u64,u64> pair;
    VfOriginHost origin;
    VfMemoryHost memory;
};
// All pointers borrow this invocation's actual nodes or native owning containers.
extern "C" u64 value_root(address values,u64 reg) on cpu;
extern "C" void value_copy_registers(address target,address source,u32 locals,
    cfn<address,address,u64,u64> resize) on cpu requires nonzero(resize);
extern "C" void value_release_registers(address values,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize);
extern "C" void value_merge_aliases(address target,address source,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize);
extern "C" void value_release_memory(address state,VfMemoryHost memory) on cpu
    requires nonzero(memory.resize) requires nonzero(memory.drop_origin);
extern "C" void value_copy_memory(address target,address source,VfMemoryHost memory) on cpu
    requires nonzero(memory.resize) requires nonzero(memory.copy_origin);
extern "C" void value_release_expressions(address state,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize);
extern "C" void value_order_origins(address state,u64 stride) on cpu;
extern "C" void value_retain_origins(RewriteSpan code,address state,Host read,RewriteHost write,
    u64 stride,VfOriginWriter writer) on cpu requires nonzero(read.inspect) requires nonzero(write.inspect)
    requires nonzero(writer.collect) requires nonzero(writer.attach);
extern "C" void value_take_input_origins(address state,u64 result,address sink,VfOriginHost host,
    cfn<void,address,address> accept) on cpu requires nonzero(host.discard) requires nonzero(accept);
extern "C" void value_release_origins(address state,cfn<address,address,u64,u64> resize,VfOriginHost host) on cpu
    requires nonzero(resize) requires nonzero(host.resize) requires nonzero(host.discard);
extern "C" VfFlow value_block(address block,address values,address expressions,address context,
    Span registers,VfHost host,bool keep_expressions) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.scalar) requires nonzero(host.storage)
    requires nonzero(host.remap) requires nonzero(host.resize_rows) requires nonzero(host.scratch)
    requires nonzero(host.rewrite_division) requires nonzero(host.pair)
    requires nonzero(host.origin.capture) requires nonzero(host.origin.resize)
    requires nonzero(host.memory.resize) requires nonzero(host.memory.move_rows)
    requires nonzero(host.memory.capture_origin) requires nonzero(host.memory.drop_origin) requires nonzero(host.memory.copy_origin)
    requires nonzero(host.instruction_span) requires nonzero(host.swap_instructions) requires nonzero(host.truncate);
extern "C" void value_discard_flow(VfFlow flow,cfn<address,address,u64,u64> resize) on cpu
    requires nonzero(resize);
