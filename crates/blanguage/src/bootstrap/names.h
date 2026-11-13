module bootstrap.names;
struct NameBytes {address bytes;u64 length;};
struct NameHit {NameBytes name;NameBytes owner;u64 public;u64 present;};
struct NameResult {NameBytes name;u64 count;};
i32 compare_name(NameBytes left, NameBytes right) on cpu;
// All views and the candidate buffer belong to the synchronous caller.
extern "C" NameResult resolve(NameBytes name,NameBytes module,address imports,u64 count,
    address candidate,address context,cfn<NameHit,address,NameBytes> lookup) on cpu requires nonzero(lookup);

extern "C" u64 order_imports(address names, u64 count) on cpu;
// Each row borrows two UTF-8 name spans. The unique rows occupy the prefix.
extern "C" u64 order_name_pairs(address pairs, u64 count) on cpu;

struct FunctionName {
    NameBytes name;
    u64 ordinal;
};

extern "C" void pair_functions(address names, u64 declarations, u64 implementations, address output) on cpu;

// The active path and key are borrowed from the synchronous evaluation calls.
extern "C" bool dependency_active(address path, u64 count, NameBytes key) on cpu;

void sort_rows<T>(address rows, u64 count, address context,
    cfn<bool,address,address,address> less) on cpu requires nonzero(less);

struct ModuleNameCounts { u64 local; u64 external; };

// Both caller-owned spans borrow emitted symbol names. The output prefixes
// contain unique local names and referenced names outside the local set.
extern "C" ModuleNameCounts classify_module_names(address local,u64 local_count,
    address references,u64 reference_count) on cpu;
