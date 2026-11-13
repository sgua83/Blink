module bootstrap.call_layout;

struct AbiType { u32 size; u32 alignment; u64 value; };
struct CallPlan { u64 bytes; u64 result; u64 target; u64 hidden; u64 arguments; };
struct ArgumentSlot { u64 index; u64 home; u64 payload; u64 indirect; };

extern "C" bool indirect(u32 size, u32 alignment) on cpu;
// flags: an error result, or an indirect target. The callback only borrows Call.
// positions is null for frame sizing, or holds count rows owned by the emitter.
extern "C" CallPlan plan(address context, u64 count, AbiType result, u32 flags,
    cfn<AbiType,address,u64> argument, address positions) on cpu requires nonzero(argument);

// Offsets are relative to RSP after the prologue. hidden marks the saved result address.
struct FunctionFrame { u64 bytes; u64 home; u64 result; u64 hidden; };
extern "C" FunctionFrame function_frame(u64 data_end, AbiType result, u32 flags) on cpu;
