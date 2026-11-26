module bootstrap.generic_bindings;
import bootstrap.ast_storage;
import bootstrap.names;

struct BindingView { address rows; u64 count; };
struct BindingEntry { NameBytes name; address type_value; u32 const_value; u32 kind; };

extern "C" bool bind_type(address table, NameBytes name, address type_value, u32 wrapped,
    cfn<void,address> release_type) on cpu requires nonzero(release_type);
extern "C" bool bind_value(address table, NameBytes name, u32 value) on cpu;
extern "C" address bound_type(BindingView table, NameBytes name) on cpu;
extern "C" Optional<u32> bound_value(BindingView table, NameBytes name) on cpu;
extern "C" BindingEntry entry_at(BindingView table, u64 index) on cpu;
extern "C" void release(BindingView table, cfn<void,address> release_type) on cpu requires nonzero(release_type);

NameBytes binding_text(StorageView storage, Text text) on cpu;
u32 use_kind(BindingView table, NameBytes name, u32 kind) on cpu;
