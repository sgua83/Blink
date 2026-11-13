module bootstrap.build_schedule;

extern "C" u64 initialize_ready(address ready,address waiting,u64 length) on cpu;
extern "C" u64 take_ready(address ready,inout u64 count) on cpu;
extern "C" void complete_unit(address ready,inout u64 count,address waiting,address followers,u64 length) on cpu;
