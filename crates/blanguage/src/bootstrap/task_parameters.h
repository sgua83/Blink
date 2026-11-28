module bootstrap.task_parameters;

struct TypeRef { address value; u32 present; };
struct Scope { usize input; usize argument; usize parent; usize first; u32 nested; };
struct Slot { usize input; usize argument; usize parent; usize component; u32 nested; };
struct Host {
    cfn<void,address,address,Scope> visit;
    cfn<usize,address> length;
    cfn<Slot,address,usize> slot;
    cfn<u32,address,usize,address> same_type;
    cfn<usize,address,Slot,address> append;
};

extern "C" usize plan_input(address context,TypeRef input_type,usize input,usize argument,usize width,Host host) on cpu
    requires nonzero(host.visit) requires nonzero(host.length);
extern "C" void plan_field(address context,Scope scope,usize component,address result,Host host) on cpu
    requires nonzero(host.length)
    requires nonzero(host.slot) requires nonzero(host.same_type) requires nonzero(host.append) requires nonzero(host.visit);
