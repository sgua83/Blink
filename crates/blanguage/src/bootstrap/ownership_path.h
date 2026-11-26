module bootstrap.ownership_path;
struct Cursor {u32 phase;u64 index;};
enum ResidencyNeed:u32 {Inspect=0;Query=1;Yes=2;No=3;Left=4;Right=5;Conditional=6;Copy=7;Complete=8;Next=9;};
struct ResidencyFacts {u32 selected;u32 truth;};
struct ResidencyStep {Cursor cursor;ResidencyNeed need;};
enum ModelNeed:u32 {Inspect=0;Ownership=1;Residency=2;Initialization=3;Child=4;Recurse=5;Complete=6;Next=7;};
struct ModelFacts {u32 kind;u32 initialization;u32 present;u64 count;};
struct ModelStep {Cursor cursor;ModelNeed need;};
enum MetadataNeed:u32 {Inspect=0;Availability=1;ResolvedAvailability=2;Initialize=3;Forget=4;Child=5;Project=6;Recurse=7;Complete=8;Next=9;};
struct MetadataFacts {u32 kind;u32 forget;u32 presence;u32 available;u32 place;u32 owns;u32 resolved;u64 count;};
struct MetadataStep {Cursor cursor;MetadataNeed need;};
enum IntersectNeed:u32 {Contains=0;Child=1;Yes=2;No=3;Next=4;};
struct IntersectFacts {u32 hit;u32 indexed;u32 child;u64 count;};
struct IntersectStep {Cursor cursor;IntersectNeed need;};
extern "C" Cursor start() on cpu;
extern "C" u32 ownership(u32 kind,u32 truth) on cpu;
extern "C" u32 possible(u32 truth) on cpu;
extern "C" ResidencyStep residency_step(Cursor state,ResidencyFacts f) on cpu;
extern "C" ModelStep model_step(Cursor state,ModelFacts f) on cpu;
extern "C" MetadataStep metadata_step(Cursor state,MetadataFacts f) on cpu;
extern "C" IntersectStep intersect_step(Cursor state,IntersectFacts f) on cpu;

struct ResidencyPair { address left; address right; address path; };
struct ResidencyBranch { address condition; address yes; address no; };
struct ResidencyCompareHost {
    cfn<u32,address> possible;
    cfn<u32,address,address> equivalent;
    cfn<ResidencyBranch,address> selected;
    cfn<u32,address,ResidencyPair> project;
    cfn<u32,address,ResidencyPair,address,u32> branch;
    cfn<u32,address,ResidencyPair> leaf;
};
extern "C" u32 compare_residency(address context,ResidencyPair pair,ResidencyCompareHost host) on cpu
    requires nonzero(host.possible)
    requires nonzero(host.equivalent)
    requires nonzero(host.selected)
    requires nonzero(host.project)
    requires nonzero(host.leaf);
extern "C" u32 compare_projected_residency(address context,ResidencyPair pair,ResidencyCompareHost host) on cpu
    requires nonzero(host.equivalent)
    requires nonzero(host.selected)
    requires nonzero(host.branch)
    requires nonzero(host.leaf);

enum ResidencyCheckMode:u32 { Known=0; Require=1; Query=2; };
struct ResidencyVisit {
    address value;
    address path;
    address wanted;
    u32 target;
    ResidencyCheckMode mode;
    u32 deferred;
};
struct ResidencyVisitResult { u32 accepted; u32 deferred; };
struct ResidencyCheckNode { ResidencyBranch branch; address origins; };
enum ResidencyOriginKind:u32 { End=0; Cpu=1; Gpu=2; Input=3; Output=4; };
struct ResidencyOriginItem { ResidencyOriginKind kind; address value; };
struct ResidencyCheckHost {
    cfn<u32,address> possible;
    cfn<ResidencyCheckNode,address> inspect;
    cfn<ResidencyVisitResult,address,ResidencyVisit,address,u32> branch;
    cfn<ResidencyVisitResult,address,ResidencyVisit,address> origins;
};
struct ResidencyOriginsHost {
    cfn<ResidencyOriginItem,address> next;
    cfn<u32,address> overlaps;
    cfn<u32,address,address> same_input;
    cfn<u32,address,address,u32,address> insert;
};
extern "C" ResidencyVisitResult check_residency(address context,ResidencyVisit visit,ResidencyCheckHost host) on cpu
    requires nonzero(host.possible) requires nonzero(host.inspect)
    requires nonzero(host.branch) requires nonzero(host.origins);
extern "C" ResidencyVisitResult check_residency_origins(address context,ResidencyVisit visit,address iterator,ResidencyOriginsHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.overlaps)
    requires nonzero(host.same_input) requires nonzero(host.insert);

struct ArgumentDescendant { u64 fields; u32 bound; };
struct ArgumentBindingHost {
    cfn<void,address> bind_root;
    cfn<ArgumentDescendant,address> next_descendant;
    cfn<u32,address,u64> declared_at;
    cfn<void,address> bind_child;
};
extern "C" void bind_argument(address context,u64 prefix,ArgumentBindingHost host) on cpu
    requires nonzero(host.bind_root)
    requires nonzero(host.next_descendant)
    requires nonzero(host.declared_at)
    requires nonzero(host.bind_child);

struct ResidencyQueryHost {
    cfn<u32,address> consume;
    cfn<address,address> next_child;
    cfn<u32,address> leave;
    cfn<u32,address,address> project;
};
struct ResidencyQueryChildHost {
    cfn<u32,address> possible;
    cfn<u32,address,address,address> enter;
    cfn<void,address> release;
};
extern "C" void traverse_residency_query(address context,ResidencyQueryHost host) on cpu
    requires nonzero(host.consume) requires nonzero(host.next_child)
    requires nonzero(host.leave) requires nonzero(host.project);
extern "C" u32 enter_residency_query_child(address context,address child,address path,ResidencyQueryChildHost host) on cpu
    requires nonzero(host.possible) requires nonzero(host.enter) requires nonzero(host.release);

struct SourceVisit { address value; address path; address result; };
struct ResidencySourceHost {
    cfn<u32,address> possible;
    cfn<ResidencyBranch,address> selected;
    cfn<u32,address,address> truth;
    cfn<u32,address,SourceVisit,address,u32> under;
    cfn<void,address,SourceVisit,ResidencyBranch> both;
    cfn<u32,address,SourceVisit> origins;
};
extern "C" u32 resolve_residency_source(address context,SourceVisit visit,ResidencySourceHost host) on cpu
    requires nonzero(host.possible)
    requires nonzero(host.selected)
    requires nonzero(host.truth)
    requires nonzero(host.under)
    requires nonzero(host.both)
    requires nonzero(host.origins);
extern "C" u32 resolve_viable_source(address context,SourceVisit visit,ResidencySourceHost host) on cpu
    requires nonzero(host.selected)
    requires nonzero(host.truth)
    requires nonzero(host.under)
    requires nonzero(host.both)
    requires nonzero(host.origins);

enum ResourceOriginKind:u32 { End=0; Concrete=1; Output=2; };
struct ResourceOrigin { ResourceOriginKind kind; address value; };
struct ResourceOriginHost {
    cfn<ResourceOrigin,address> next;
    cfn<void,address,address> append;
    cfn<u32,address,address> output;
};
extern "C" u32 resolve_resource_origins(address cursor,u32 collect,ResourceOriginHost host) on cpu
    requires nonzero(host.next)
    requires nonzero(host.append)
    requires nonzero(host.output);

enum DependencyItemKind:u32 { End=0; Reference=1; Model=2; Residency=3; Concrete=4; };
enum DependencyList:u32 { Arguments=0; Origins=1; Payloads=2; };
struct DependencyItem { DependencyItemKind kind; address value; address arguments; };
struct ReturnedResidency { address residency; address elements; address payloads; u64 maximum; };
struct DependencyHost {
    cfn<DependencyItem,address> next;
    cfn<void,address,address> insert;
    cfn<ReturnedResidency,address> model;
    cfn<ResidencyCheckNode,address> residency;
    cfn<void,address,DependencyList,address> walk;
};
extern "C" void collect_residency_dependencies(address names,address cursor,DependencyHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.insert)
    requires nonzero(host.model) requires nonzero(host.residency)
    requires nonzero(host.walk);

enum ChoiceAction:u32 { Absent=0; Yes=1; No=2; Join=3; Select=4; };
enum JoinAction:u32 { Keep=0; Copy=1; Clear=2; Extend=3; Select=4; };
struct ChoicePlan { ChoiceAction action; u64 yes_depth; u64 no_depth; u32 split_yes; u32 split_no; };
struct JoinPlan { JoinAction action; u32 split_left; address condition; address right_yes; address right_no; };
struct ChoiceFacts { u32 impossible; u32 literal; u32 unconstrained; };
struct ResidencyAlgebraHost {
    cfn<ResidencyCheckNode,address> inspect;
    cfn<u32,address,address> equal;
    cfn<u32,address,address> condition_equal;
    cfn<i32,address,address> order;
    cfn<ChoiceFacts,address> facts;
    cfn<u32,address> exclusive;
    cfn<u32,address> empty;
};
extern "C" ChoicePlan choose_residency(address when,address yes,address no,ResidencyAlgebraHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.equal)
    requires nonzero(host.condition_equal) requires nonzero(host.order) requires nonzero(host.facts) requires nonzero(host.exclusive);
extern "C" JoinPlan join_residency(address left,address right,ResidencyAlgebraHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.equal)
    requires nonzero(host.condition_equal) requires nonzero(host.order) requires nonzero(host.exclusive) requires nonzero(host.empty);

struct QueryValuePath { address value; address path; };
struct QueryValueKeyHost {
    cfn<ResidencyCheckNode,address> inspect;
    cfn<void,address,QueryValuePath,address,u32> branch;
    cfn<void,address,address,address> collect;
};
extern "C" void collect_query_value_key(address context,QueryValuePath visit,QueryValueKeyHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.branch) requires nonzero(host.collect);

enum KeyOriginProjection:u32 { Concrete=0; Output=1; Input=2; };
struct QueryOriginKeyHost {
    cfn<ResidencyOriginItem,address> next;
    cfn<u32,address,address> matches;
    cfn<void,address,KeyOriginProjection,address> append;
};
extern "C" void collect_query_origin_keys(address context,address cursor,address wanted,QueryOriginKeyHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.matches) requires nonzero(host.append);

struct OutputSnapshotHost {
    cfn<ReturnedResidency,address> inspect;
    cfn<void,address,address> insert;
    cfn<void,address,address> elements;
    cfn<u64,address> field_count;
    cfn<void,address,address,u64> field;
};
extern "C" void collect_output_paths(address context,address value,OutputSnapshotHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.insert) requires nonzero(host.elements)
    requires nonzero(host.field_count) requires nonzero(host.field);

struct ResidencyPrefix { address residency; u64 fields; };
extern "C" ResidencyPrefix find_residency_prefix(address context,u64 fields,cfn<address,address,u64> lookup) on cpu
    requires nonzero(lookup);

enum AvailabilityKind:u32 { Value=0; Dataset=1; Aggregate=2; Variant=3; };
struct AvailabilityNode { AvailabilityKind kind; address model; u64 fields; };
struct OutputAvailabilityHost {
    cfn<AvailabilityNode,address> inspect;
    cfn<u32,address,address> present;
    cfn<u32,address> maximum;
    cfn<address,address> element;
    cfn<address,address,u64> field;
    cfn<void,address,u64> discard;
};
extern "C" u32 resolve_output_availability(address source,address value,OutputAvailabilityHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.present)
    requires nonzero(host.maximum) requires nonzero(host.element)
    requires nonzero(host.field) requires nonzero(host.discard);

struct ResidencyBinding { address arguments; address result; u32 materialize; };
struct ResidencyBindingHost {
    cfn<ResidencyCheckNode,address> inspect;
    cfn<u32,ResidencyBinding,ResidencyBranch> choice;
    cfn<u32,ResidencyBinding,address> origins;
};
struct ResidencyBindingOriginsHost {
    cfn<ResidencyOriginItem,address> next;
    cfn<u32,address> bound;
    cfn<void,address> unknown;
    cfn<u32,ResidencyBinding,address> input;
    cfn<void,ResidencyBinding,address> output;
    cfn<void,address,address> concrete;
};
extern "C" u32 bind_residency(ResidencyBinding visit,address value,ResidencyBindingHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.choice) requires nonzero(host.origins);
extern "C" u32 bind_residency_origins(ResidencyBinding visit,address iterator,u64 count,ResidencyBindingOriginsHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.bound) requires nonzero(host.unknown)
    requires nonzero(host.input) requires nonzero(host.output) requires nonzero(host.concrete);
