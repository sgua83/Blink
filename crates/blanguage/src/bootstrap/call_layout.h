module bootstrap.call_layout;

struct AbiType { u32 size; u32 alignment; u64 value; };
struct CallPlan { u64 bytes; u64 result; u64 copies; u64 target; u64 hidden; };
struct ValueCopy { u64 offset; u64 end; u64 present; };

extern "C" bool indirect(u32 size, u32 alignment) on cpu;
extern "C" ValueCopy copy_value(u64 start, u32 size, u32 alignment) on cpu;
// flags: an error result, or an indirect target. The callback only borrows Call.
extern "C" CallPlan plan(address context, u64 count, AbiType result, u32 flags,
    cfn<AbiType,address,u64> argument) on cpu requires nonzero(argument);

// Offsets are relative to RSP after the prologue. hidden marks the saved result address.
struct FunctionFrame { u64 bytes; u64 home; u64 result; u64 hidden; };
extern "C" FunctionFrame function_frame(u64 data_end, AbiType result, u32 flags) on cpu;
