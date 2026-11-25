module bootstrap.call_layout;
import bootstrap.x64_instruction;
import bootstrap.ir_dataflow;

// float_size is the width in bytes of an intrinsic scalar float, or zero.
struct AbiType { u32 size; u32 alignment; u32 float_size; u32 value; };
enum ResultLocation:u32 { None=0; Integer=1; Float=2; Indirect=3; };
// flags: an error result, or an indirect target. The reader borrows the original Call.
extern "C" u64 outgoing_bytes(address context, u64 count, AbiType result, u32 flags,
    cfn<AbiType,address,u64> argument) on cpu requires nonzero(argument);
// Offsets are relative to RSP after the prologue. Indirect results use the saved address slot.
struct FunctionFrame { u64 bytes; u64 home; u64 result; ResultLocation result_location; };
extern "C" FunctionFrame function_frame(u64 data_end, AbiType result, u32 flags) on cpu;

// Only the physical register prefix survives the frame setup.
struct FunctionParameter { AbiType abi; u64 values; };
struct HomeParameter { u64 index; FunctionParameter parameter; };
enum ParameterMoveKind:u32 { Home=0; Direct=1; Indirect=2; };
struct ParameterMove {
    u64 index; u64 slot; u64 offset; u64 first; u64 count;
    u32 float_size; ParameterMoveKind kind;
};
enum FunctionAction:u32 {
    HiddenResultHome=0; ScratchSave=1; FrameAllocate=2; NonvolatileSave=3; ResultAddressSave=4;
    IndirectResult=5; FallibleResult=6; FloatResult=7; IntegerResult=8; FallibleSuccess=9;
    CloseExits=10; NonvolatileRestore=11; FrameRelease=12; ScratchRestore=13; Return=14;
};
struct FunctionHost {
    cfn<FunctionParameter,address,u64> ensures memory(read) parameter;
    cfn<void,address,ParameterMove> move_parameter;
    cfn<void,address,FunctionAction,u64,u64> action;
};
extern "C" void function_prologue(address context, u64 count, FunctionFrame frame, u32 flags, FunctionHost host)
    on cpu requires nonzero(host.parameter) requires nonzero(host.move_parameter) requires nonzero(host.action);
extern "C" u64 function_epilogue(address context, FunctionFrame frame, u32 flags,
    cfn<void,address,FunctionAction,u64,u64> action) on cpu requires nonzero(action);

// The original reference borrows its actual locations and 168 writable bytes.
extern "C" u64 emit_reference_address(Node reference, IndexedAddressHost host,
    cfn<MoveLocation,address,usize> ensures memory(read) local, address output) on cpu
    requires nonzero(host.values.ir.inspect) requires nonzero(host.values.location)
    requires nonzero(host.values.scalar) requires nonzero(host.tile_stream) requires nonzero(local);

struct CallValueLoan { Span components; Span values; address locations; };
struct CallInput {
    Node source;
    AbiType result;
    CallValueLoan values;
    usize error;
    u32 flags;
};
struct CallHost {
    IndexedAddressHost addresses;
    cfn<MoveLocation,address,usize> ensures memory(read) local;
    cfn<AbiType,address,u64> argument;
    cfn<CallValueLoan,address,u64> loan;
    cfn<AbiValueComponent,address,u64> component;
    cfn<address,address,u64> reserve;
};
// The output buffer belongs to the original emitter. Calls retain no input borrow.
extern "C" SymbolEncoding emit_call(address context, CallInput input, CallHost host) on cpu
    requires nonzero(host.addresses.values.ir.inspect)
    requires nonzero(host.addresses.values.location) requires nonzero(host.addresses.values.scalar)
    requires nonzero(host.addresses.tile_stream) requires nonzero(host.local)
    requires nonzero(host.argument) requires nonzero(host.loan)
    requires nonzero(host.component) requires nonzero(host.reserve);
