module bootstrap.output_bindings;

struct OutputBindingEntry { address source; address target; };

extern "C" void write_output_binding(address owner,address source,address target) on cpu;
extern "C" void collect_output_bindings(address owner,address origins,usize count,usize result,usize entry) on cpu;
extern "C" address find_output_binding(address owner,address source) on cpu;
extern "C" OutputBindingEntry next_output_binding(address owner,address cursor) on cpu;
