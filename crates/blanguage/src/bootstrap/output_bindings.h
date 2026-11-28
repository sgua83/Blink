module bootstrap.output_bindings;

struct OutputBindingEntry { address source; address target; };

extern "C" void write_output_binding(address owner,address source,address target) on cpu;
extern "C" void collect_output_bindings(address owner,address rows,address registers,usize count,usize entry) on cpu;
extern "C" address find_output_binding(address owner,address source) on cpu;
extern "C" OutputBindingEntry next_output_binding(address owner,address cursor) on cpu;

extern "C" void copy_output_bindings(address original,address output) on cpu;
extern "C" u32 same_output_bindings(address left,address right) on cpu;
extern "C" u32 extend_equality_roots(address roots,address context,cfn<address,address> next) on cpu
    requires nonzero(next);
extern "C" void resolve_computed_roots(address roots,address sources,usize count) on cpu;

struct EqualityMembers { address records; usize count; };
extern "C" EqualityMembers build_equality_members(address roots) on cpu;
extern "C" void release_equality_members(address records) on cpu;
extern "C" void project_alias_facts(address environment,address facts,address roots,EqualityMembers members,
    address previous,address output) on cpu;
