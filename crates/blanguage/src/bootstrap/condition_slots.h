module bootstrap.condition_slots;

extern "C" void condition_slots_push(address owner,address payload) on cpu;
extern "C" void condition_slots_resize(address owner,usize width) on cpu;
extern "C" void condition_slots_clear(address owner) on cpu;
extern "C" void condition_slots_copy(address original,address result) on cpu;
extern "C" void condition_slots_set(address owner,usize index,address payload) on cpu;
extern "C" address condition_slots_take(address owner,usize index) on cpu;
extern "C" void condition_slots_conjoin(address owner,usize index,address no,address yes,cfn<u32,address> conflict) on cpu
    requires nonzero(conflict);
extern "C" void condition_slots_deny(address owner,usize index,u32 branch) on cpu;
