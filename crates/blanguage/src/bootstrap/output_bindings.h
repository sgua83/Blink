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
