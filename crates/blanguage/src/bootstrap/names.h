module bootstrap.names;
struct NameBytes {address bytes;u64 length;};
struct NameHit {NameBytes name;NameBytes owner;u64 public;u64 present;};
struct NameResult {NameBytes name;u64 count;};
// All views and the candidate buffer belong to the synchronous caller.
extern "C" NameResult resolve(NameBytes name,NameBytes module,address imports,u64 count,
    address candidate,address context,cfn<NameHit,address,NameBytes> lookup) on cpu requires nonzero(lookup);

extern "C" u64 order_imports(address names, u64 count) on cpu;
