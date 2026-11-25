module bootstrap.call_layout;
import bootstrap.x64_instruction;
import bootstrap.ir_dataflow;
import bootstrap.frame_layout;

// float_size is the width in bytes of an intrinsic scalar float, or zero.
struct AbiType { u32 size; u32 alignment; u32 float_size; u32 value; };
enum ResultLocation:u32 { None=0; Integer=1; Float=2; Indirect=3; };
// flags: an error result, or an indirect target. The reader borrows the original Call.
extern "C" u64 outgoing_bytes(address context, u64 count, AbiType result, u32 flags,
    cfn<AbiType,address,u64> argument) on cpu requires nonzero(argument);
// Offsets are relative to RSP after the prologue. Indirect results use the saved address slot.
struct FunctionFrame { u64 bytes; u64 home; u64 result; u64 stack; ResultLocation result_location; };
extern "C" FunctionFrame function_frame(u64 data_end, AbiType result, u32 flags) on cpu;

struct CallValueLoan { Span components; Span values; address allocation; address stage; };
// The register prefix survives frame setup as part of this one prologue.
struct FunctionParameter { AbiType abi; u64 values; };
struct HomeParameter { u64 index; FunctionParameter parameter; };
enum FunctionRangeKind:u32 { HiddenHome=0; Home=1; Input=2; ResultAddress=3;
    ScratchSave=4; FrameAllocate=5; NonvolatileSave=6; };
struct FunctionRange { u64 start; u64 end; u64 index; u64 position; FunctionRangeKind kind; };
struct FunctionHost {
    cfn<FunctionParameter,address,u64> ensures memory(read) parameter;
    cfn<CallValueLoan,address,u64,u64,u64> ensures memory(read) loan;
    cfn<AbiValueComponent,address,u64> component;
    cfn<void,address,FunctionRange> publish;
};
struct FunctionPrologue { u64 length; u64 probe_field; u64 operations; u8 prolog; };
extern "C" FunctionPrologue function_prologue(address context, u64 count, FunctionFrame frame,
    u32 flags, FrameOutput output, FunctionHost host) on cpu
    requires nonzero(host.parameter) requires nonzero(host.loan)
    requires nonzero(host.component) requires nonzero(host.publish);

struct FunctionReturn { u64 length; u64 value_length; u64 literal; u64 literal_field; };
extern "C" FunctionReturn function_result(FunctionFrame frame, u32 flags, CallValueLoan value,
    cfn<AbiValueComponent,address,u64> component, address output)
    on cpu requires nonzero(component);

struct CallInput {
    Node source;
    AbiType result;
    CallValueLoan values;
    usize error;
    u32 flags;
};
struct CallOperand { AbiType abi; CallValueLoan value; };

struct CallHost {
    IndexedAddressHost addresses;
    cfn<MoveLocation,address,usize> ensures memory(read) local;
    cfn<AbiType,address,u64> argument;
    cfn<CallOperand,address,u64> operand;
    cfn<AbiValueComponent,address,u64> component;
    cfn<NativeCodeBuffer,address,u64> reserve;
    cfn<void,address,u64> commit;
};
// Returns the absolute position of the emitted symbolic target field.
extern "C" u64 emit_call(address context, CallInput input, CallHost host) on cpu
    requires nonzero(host.addresses.values.ir.inspect)
    requires nonzero(host.addresses.values.location) requires nonzero(host.addresses.values.scalar)
    requires nonzero(host.addresses.tile_stream) requires nonzero(host.local)
    requires nonzero(host.argument) requires nonzero(host.operand)
    requires nonzero(host.component) requires nonzero(host.reserve) requires nonzero(host.commit);
