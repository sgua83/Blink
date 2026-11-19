module bootstrap.call_layout;

// float_size is the width in bytes of an intrinsic scalar float, or zero.
struct AbiType { u32 size; u32 alignment; u32 float_size; u32 value; };
enum ResultLocation:u32 { None=0; Integer=1; Float=2; Indirect=3; };
struct CallPlace { u64 index; u64 home; u64 payload; u32 copied; };
enum CallAction:u32 {
    SaveTarget=0; ResultAddress=1; LoadRegister=2; Invoke=3;
    CaptureError=4; ReadIndirect=5; ReadFloat=6; ReadInteger=7; CloseError=8;
};
struct CallHost {
    cfn<AbiType,address,u64> argument;
    cfn<bool,address,CallPlace> place;
    cfn<u64,address,CallAction,u64,u32> action;
};
// flags: an error result, or an indirect target. The reader borrows the original Call.
extern "C" u64 outgoing_bytes(address context, u64 count, AbiType result, u32 flags,
    cfn<AbiType,address,u64> argument) on cpu requires nonzero(argument);
extern "C" void emit_call(address context, u64 count, AbiType result, u32 flags, CallHost host)
    on cpu requires nonzero(host.argument) requires nonzero(host.place) requires nonzero(host.action);

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
