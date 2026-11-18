module bootstrap.stack_report;

struct StackName { address data; u64 length; };
struct StackFrame { u64 calls; u32 local; u32 positioned; };
enum StackCallKind:u32 { Reference=0; Tail=1; Indirect=2; Unreported=3; };
enum StackTarget:u32 { Section=0; Symbol=1; Import=2; };
struct StackCall { StackName name; StackName dll; u64 section; u32 start; StackCallKind kind; StackTarget target; };
struct StackPosition { u64 section; u32 start; u32 present; };
struct StackIndex { u64 value; u32 present; };
struct StackParent { u64 parent; u32 tail; };
enum StackNotice:u32 { Unreported=0; Indirect=1; UnresolvedModule=2; External=3; MissingBody=4; };
enum StackBudget:u32 { NotSupplied=0; LocalExceeds=1; UpperFits=2; UpperExceeds=3; Unknown=4; };
struct StackAmount { u64 low; u64 high; };
struct StackMetric { StackAmount amount; u32 closed; u32 upper; StackBudget capacity; };
struct StackScratch { u64 pending; u64 ready; };
struct StackHost {
    cfn<StackFrame,address,u64> frame;
    cfn<StackCall,address,u64,u64> call;
    cfn<void,address,u64> register_frame;
    cfn<void,address,u64> register_symbol;
    cfn<StackPosition,address,StackName> symbol;
    cfn<StackIndex,address,StackPosition> position;
    cfn<void,address,u64,u64,u32> edge;
    cfn<u64,address,u64> parent_count;
    cfn<StackParent,address,u64,u64> parent;
    cfn<void,address,u64,StackNotice,StackCall> notice;
    cfn<void,address,u64,u64> union_unknown;
    cfn<u32,address,u64> unknown_empty;
};
// The original object and containers stay with this synchronous caller. Metrics
// are its final rows; scratch has one pending count and queue slot per frame.
extern "C" void analyze(address context,u64 frames,u64 symbols,u32 has_budget,u64 budget,
    address metrics,address scratch,StackHost host) on cpu
    requires nonzero(host.frame) requires nonzero(host.call)
    requires nonzero(host.register_frame) requires nonzero(host.register_symbol)
    requires nonzero(host.symbol) requires nonzero(host.position) requires nonzero(host.edge)
    requires nonzero(host.parent_count) requires nonzero(host.parent) requires nonzero(host.notice)
    requires nonzero(host.union_unknown) requires nonzero(host.unknown_empty);
