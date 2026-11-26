module bootstrap.call_layout;
import bootstrap.x64_instruction;
import bootstrap.ir_dataflow;
import bootstrap.frame_layout;

// float_size is the width in bytes of an intrinsic scalar float, or zero.
struct AbiType { u32 size; u32 alignment; u32 float_size; u32 value; };
// Flags mark an error result and an indirect target in the original call.
struct CallAbi { AbiType result; u32 flags; };
u64 outgoing_bytes(address context, u64 count, CallAbi abi,
    cfn<AbiType,address,u64> argument) on cpu requires nonzero(argument);

struct CallValueLoan { Span components; Span values; address allocation; address registers; };
// The register prefix survives frame setup as part of this one prologue.
struct FunctionParameter { AbiType abi; u64 values; };
struct HomeParameter { u64 index; FunctionParameter parameter; };
enum FunctionRangeKind:u32 { HiddenHome=0; Home=1; Input=2; ResultAddress=3;
    ScratchSave=4; FrameAllocate=5; NonvolatileSave=6; IntegerResult=7; FloatResult=8;
    IndirectResult=9; SuccessStatus=10; NonvolatileRestore=11; FrameRelease=12;
    ScratchRestore=13; FunctionReturn=14; };
struct FunctionRange { u64 start; u64 end; u64 index; u64 position; FunctionRangeKind kind; };
struct FunctionHost {
    cfn<FunctionParameter,address,u64> ensures memory(read) parameter;
    cfn<CallValueLoan,address,u64,u64,u64> ensures memory(read) loan;
    cfn<AbiValueComponent,address,u64> component;
    cfn<void,address,FunctionRange> publish;
};


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

struct FunctionCodeInput {
    u64 data_end;
    AbiType result;
    u32 flags;
    u64 parameters;
    u64 inputs;
    u64 result_fields;
    u64 outputs;
    u64 saved;
};
struct FunctionExitBuffer {
    NativeCodeBuffer code;
    address saved;
    u64 saved_count;
};
struct FunctionCodeHost {
    FunctionHost abi;
    cfn<FrameOutput,address,u64,u64> entry;
    cfn<u32,address,FrameEntry,ControlTargets> body;
    cfn<NativeCodeBuffer,address,u64> reserve;
    cfn<CallValueLoan,address> result;
    cfn<void,address,u64> commit;
    cfn<void,address,u64,u64,u64> literal;
    cfn<FunctionExitBuffer,address,u64> exit_buffer;
};
struct FunctionCode { u64 stack; u32 oversized; };

extern "C" FunctionCode emit_function_code(address context, FunctionCodeInput input, FunctionCodeHost host) on cpu
    requires nonzero(host.abi.parameter) requires nonzero(host.abi.loan)
    requires nonzero(host.abi.component) requires nonzero(host.abi.publish)
    requires nonzero(host.entry) requires nonzero(host.body) requires nonzero(host.reserve)
    requires nonzero(host.result) requires nonzero(host.commit) requires nonzero(host.literal)
    requires nonzero(host.exit_buffer);
