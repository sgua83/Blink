module bootstrap.call_layout;
import bootstrap.x64_instruction;
import bootstrap.ir_dataflow;
import bootstrap.frame_layout;

// The original Call view lends its arguments and result; its target is a separate IR node.
u64 outgoing_bytes(View call, Host ir) on cpu requires nonzero(ir.inspect);

struct CallValueLoan { Span components; Span values; address allocation; address registers; };
// The register prefix survives frame setup as part of this one prologue.
struct FunctionParameter { Node ty; u64 values; };
enum FunctionRangeKind:u32 { HiddenHome=0; Home=1; Input=2; ResultAddress=3;
    ScratchSave=4; FrameAllocate=5; NonvolatileSave=6; IntegerResult=7; FloatResult=8;
    IndirectResult=9; SuccessStatus=10; NonvolatileRestore=11; FrameRelease=12;
    ScratchRestore=13; FunctionReturn=14; };
struct FunctionRange { u64 start; u64 end; u64 index; u64 position; FunctionRangeKind kind; };
struct FunctionHost {
    Host ir;
    cfn<FunctionParameter,address,u64> ensures memory(read) parameter;
    cfn<CallValueLoan,address,u64,u64,u64> ensures memory(read) loan;
    cfn<AbiValueComponent,address,u64> component;
    cfn<void,address,FunctionRange> publish;
};


struct CallInput {
    Node source;
    Span results;
    address allocation;
    address registers;
};

struct CallHost {
    IndexedAddressHost addresses;
    cfn<MoveLocation,address,usize> ensures memory(read) local;
    cfn<AbiValueComponent,address,u64> component;
    cfn<NativeCodeBuffer,address,u64> reserve;
    cfn<void,address,u64> commit;
};
// Returns the absolute position of the emitted symbolic target field.
extern "C" u64 emit_call(address context, CallInput input, CallHost host) on cpu
    requires nonzero(host.addresses.values.ir.inspect)
    requires nonzero(host.addresses.values.location) requires nonzero(host.addresses.values.scalar)
    requires nonzero(host.local)
    requires nonzero(host.component) requires nonzero(host.reserve) requires nonzero(host.commit);

struct FunctionCodeInput {
    u64 data_end;
    Node result;
    u32 flags;
    u64 parameters;
    u64 inputs;
    u64 outputs;
    u64 saved;
};
struct FunctionExitBuffer {
    NativeCodeBuffer code;
    SavedLayout saved;
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
    requires nonzero(host.abi.ir.inspect) requires nonzero(host.abi.parameter) requires nonzero(host.abi.loan)
    requires nonzero(host.abi.component) requires nonzero(host.abi.publish)
    requires nonzero(host.entry) requires nonzero(host.body) requires nonzero(host.reserve)
    requires nonzero(host.result) requires nonzero(host.commit) requires nonzero(host.literal)
    requires nonzero(host.exit_buffer);
