module bootstrap.call_layout;

// float_size is the width in bytes of an intrinsic scalar float, or zero.
struct AbiType { u32 size; u32 alignment; u32 float_size; u32 value; };
enum ResultLocation:u32 { None=0; Integer=1; Float=2; Indirect=3; };
struct CallPlan { u64 bytes; u64 result; u64 target; u64 arguments; ResultLocation result_location; };
struct ArgumentSlot { u64 index; u64 home; u64 payload; u32 indirect; u32 float_size; };
struct ParameterSlot { u64 index; u64 home; u32 indirect; u32 float_size; };

// flags: an error result, or an indirect target. The callback only borrows Call.
// positions is null for frame sizing, or holds count rows owned by the emitter.
extern "C" CallPlan plan(address context, u64 count, AbiType result, u32 flags,
    cfn<AbiType,address,u64> argument, address positions) on cpu requires nonzero(argument);

// Offsets are relative to RSP after the prologue. Indirect results use the saved address slot.
struct FunctionFrame { u64 bytes; u64 home; u64 result; ResultLocation result_location; };
extern "C" FunctionFrame function_frame(u64 data_end, AbiType result, u32 flags) on cpu;

// Home offsets are relative to the caller's first home slot.
extern "C" u64 parameter_slots(address context, u64 count, u64 first_slot,
    cfn<AbiType,address,u64> parameter, address positions) on cpu requires nonzero(parameter);
