module bootstrap.names;
struct NameBytes {address bytes;u64 length;};
struct NameHit {NameBytes name;u64 public;u64 present;};
struct NameResult {NameBytes name;u64 count;};
i32 compare_name(NameBytes left, NameBytes right) on cpu;
u64 qualification_separator(NameBytes name) on cpu;
// Lookup borrows either the exact name or a selected prefix and a name.
extern "C" NameResult resolve(NameBytes name, NameBytes module, u64 count, address context,
    cfn<NameBytes,address,u64> import_name, cfn<NameHit,address,NameBytes,NameBytes> lookup) on cpu
    requires nonzero(import_name) requires nonzero(lookup);

extern "C" u64 order_imports(address names, u64 count) on cpu;
// Each row borrows two UTF-8 name spans. The unique rows occupy the prefix.
extern "C" u64 order_name_pairs(address pairs, u64 count) on cpu;

struct FunctionName {
    NameBytes name;
    u64 ordinal;
};

extern "C" void pair_functions(address names, u64 declarations, u64 implementations, address output) on cpu;

// The evaluator holds this stack through the last active dependency.
struct DependencyPath { address rows; u64 count; u64 capacity; };
enum DependencyStatus:u32 { Entered=0; Recursive=1; Allocation=2; Overflow=3; };
struct DependencyEntry { DependencyStatus status; u64 bytes; };
extern "C" DependencyEntry enter_dependency(address path, NameBytes key) on cpu;
extern "C" void leave_dependency(address path) on cpu;
extern "C" void release_dependencies(address path) on cpu;

void sort_rows<T>(address rows, u64 count, address context,
    cfn<bool,address,address,address> less) on cpu requires nonzero(less);

// The ordered local names and each reference borrow the original image.
extern "C" void collect_external_module_names(address local, u64 local_count,
    address references, cfn<NameBytes,address> next, address external,
    cfn<void,address,NameBytes> push) on cpu requires nonzero(next) requires nonzero(push);
