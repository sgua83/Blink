module bootstrap.task_callbacks;
struct Cursor {u32 phase;u64 index;u64 part;};
struct Step<T> {Cursor cursor;T need;};
enum CallbackNeed:u32 {Builder=0;Frame=1;Registers=2;Inputs=3;Abi=4;Functions=5;Emit=6;Finish=7;Publish=8;RuntimeContract=9;Complete=10;Next=11;};
enum SelectionNeed:u32 {Zero=0;Packet=1;Result=2;Returned=3;Guard=4;Owners=5;Ready=6;Complete=7;Next=8;};
struct SelectionFacts {u64 packet_components;u64 packet_registers;u64 result_components;u64 outputs;u64 owners;};
extern "C" Cursor start() on cpu;
extern "C" Step<CallbackNeed> callback_step(Cursor state,u32 frame) on cpu;
extern "C" Step<SelectionNeed> selection_step(Cursor state,SelectionFacts facts) on cpu;
