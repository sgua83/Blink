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

enum ResidencyQueryStep:u32 { Complete=0; Next=1; Entered=2; };
struct ResidencyQueryHost {
    cfn<u32,address> consume;
    cfn<ResidencyQueryStep,address> advance;
};
extern "C" void traverse_residency_query(address context,ResidencyQueryHost host) on cpu
    requires nonzero(host.consume)
    requires nonzero(host.advance);

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
