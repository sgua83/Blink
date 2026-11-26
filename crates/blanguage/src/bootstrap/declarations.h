module bootstrap.declarations;
import bootstrap.names;

struct Catalog { address slots; u64 capacity; u64 count; address ordered; };
struct Entry { NameBytes name; address value; };
struct Insertion { address replaced; u32 success; };
struct Resolution { NameBytes name; u64 count; };

extern "C" Insertion insert(address catalog, NameBytes name, address value) on cpu;
extern "C" address find(address catalog, NameBytes name) on cpu;
extern "C" Entry entry(address node) on cpu;
extern "C" bool seal(address catalog) on cpu;
extern "C" Entry ordered_entry(address ordered, u64 index) on cpu;
extern "C" void release(address catalog, cfn<void,address> drop_value) on cpu
    requires nonzero(drop_value);
extern "C" Resolution resolve(address catalog, NameBytes name, NameBytes module, address imports,
    u64 count, cfn<NameBytes,address,u64> import_name, cfn<u32,address> visible) on cpu
    requires nonzero(import_name) requires nonzero(visible);
