module bootstrap.world_evaluation;

// The first word of every invocation links to its live parent.
extern "C" void enter(address top, address frame) on cpu;
extern "C" void drive(address top, address context,
    cfn<u32,address,address> poll, cfn<void,address> release) on cpu
    requires nonzero(poll) requires nonzero(release);
extern "C" void drain(address top, cfn<void,address> release) on cpu
    requires nonzero(release);
