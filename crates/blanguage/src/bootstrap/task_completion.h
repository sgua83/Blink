module bootstrap.task_completion;
struct Cursor {u32 phase;u64 index;u64 owner;};
struct Facts {u32 result_owns;u64 positions;u64 datasets;u32 ready;u64 owners;u32 writes;u32 nonempty;};
enum Need:u32 {Status=0;Zero=1;BeginCompleted=2;DropResult=3;Position=4;Address=5;PositionOwners=6;OwnerGuard=7;DropOwner=8;EndOwner=9;DropPosition=10;EndCompleted=11;CompletedIf=12;Dataset=13;BeginDataset=14;Descriptor=15;Guards=16;DatasetOwners=17;Guard=18;DropDataset=19;EndDataset=20;Error=21;Raise=22;Complete=23;Next=24;};
struct Step {Cursor cursor;Need need;};
extern "C" Cursor start() on cpu;
extern "C" Step step(Cursor state,Facts facts) on cpu;
