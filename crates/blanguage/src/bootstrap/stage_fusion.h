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
    u64 width;
    u64 plane;
    u64 slot;
};

// Rows are in the original lifetime order. Each scratch plane has capacity
// for the sum of its row widths. Only assigned slots have been initialized.
extern "C" void place_tile(address rows, u64 count, address narrow, address wide) on cpu;

struct StageWrite {
    u64 binding;
    u32 start;
    u32 end;
};
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
};

// Writes arrive ordered by binding, start and end. Surviving guarantees move
// to the front of each original list in their original order.
extern "C" GuaranteeCounts retain_stage_guarantees(address writes, u64 write_count,
    GuaranteeCounts counts, GuaranteeHost host) on cpu
    requires nonzero(host.field) requires nonzero(host.swap_slots);

struct LocalValue { u64 slot; u64 value; };

// Both inputs are sorted by slot. The left buffer receives their identical
// slot/value pairs in slot order and the return value is its new length.
extern "C" u64 intersect_local_values(address left, u64 left_count,
    address right, u64 right_count) on cpu;

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

enum VfMap:u32 { Aliases=0; Expressions=1; Memory=2; Locals=3; };
enum VfRegMap:u32 { Aliases=0; Locals=1; };
enum VfStateOp:u32 { New=0; Clone=1; Drop=2; Move=3; Copy=4; Clear=5; Extend=6; };
enum VfDivisionOp:u32 { Clear=0; Take=1; Put=2; };
enum VfDivisionChoice:u32 { Quotient=1; Remainder=2; Both=3; };
enum VfAction:u32 { Discard=0; Keep=1; Stop=2; };
enum VfReuseKind:u32 { Expression=0; LocalRead=1; IndexedRead=2; IndexedStoreRead=3; };
enum VfRole:u32 { Value=0; Quotient=1; Remainder=2; };
struct VfExpression { u32 kind; u32 operation; u64 first; u64 second; u64 third; address arguments; u64 count; };
struct VfOptional { u64 value; u32 present; };
struct VfDivision { address instruction; u64 quotient; u64 remainder; u32 mask; u32 present; };
struct VfMemory { u64 value; address origin; u32 present; };
struct VfReuse { address origin; VfReuseKind kind; VfRole role; u32 paired; };
struct VfFlow { address next; address exits; };
struct VfHost {
    RegisterHost values;
    cfn<InstructionStorage,address> storage;
    cfn<void,address,address> remap;
    cfn<VfFlow,address,address,address> block;
    cfn<Span,address> instruction_span;
    cfn<void,address,address> swap_instructions;
    cfn<void,address,u64> truncate;
    cfn<address,VfStateOp,address,address,VfMap> state;
    cfn<VfOptional,address,VfRegMap,u64> get;
    cfn<void,address,VfRegMap,u64,u64> put;
    cfn<void,address,u64,u64> remove_range;
    cfn<address,address> snapshot;
    cfn<void,address,address> intersect;
    cfn<void,address> drop_snapshot;
    cfn<void,address,address> apply;
    cfn<VfOptional,address,VfExpression,u64> expression;
    cfn<VfDivision,address,VfDivisionOp,u64,u64,u64,VfDivision> division;
    cfn<void,address,VfDivisionChoice,u64,u64> rewrite_division;
    cfn<void,address,address,address,address,u64,u64,u64> pair;
    cfn<void,address,u64,address,VfReuse> note;
    cfn<VfMemory,address,u64,u64,u32> memory_get;
    cfn<void,address,u64,u64,u32,u64,address> memory_put;
    cfn<void,address,u32,u64,u64,u32> memory_filter;
    cfn<void,address,usize> remove_local;
};
// All pointers borrow this invocation's actual nodes or native owning containers.
extern "C" u64 value_root(address values,u64 reg,cfn<VfOptional,address,VfRegMap,u64> get) on cpu requires nonzero(get);
extern "C" u32 value_memory_keeps(u32 mode,u64 binding,u64 index,u32 word,u64 target,u64 at,u32 field) on cpu;
extern "C" VfFlow value_block(address block,address values,address context,Span registers,VfHost host) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.scalar) requires nonzero(host.storage)
    requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.remove_local)
    requires nonzero(host.instruction_span) requires nonzero(host.swap_instructions) requires nonzero(host.truncate);
extern "C" void value_discard_flow(VfFlow flow,VfHost host) on cpu 
    requires nonzero(host.drop_snapshot);
