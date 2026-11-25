module bootstrap.native_resources;
import bootstrap.bindings;
const u64 LOCAL_BYTES=40u64;
const u64 RUNTIME_BYTES=32u64;
struct ResourceModule {
    BindingBytes prefix;BindingBytes abi;u64 types;u64 programs;u64 type_value;u64 program_value;
    u64 factory_offset;u64 type_offset;u64 program_offset;
};
struct ResourceLocal {BindingBytes slot;BindingBytes name;u64 value;};
struct ResourceRuntime {BindingBytes name;u64 value;u64 relocation;};
struct ResourceInput {
    address modules;u64 module_count;address locals;u64 local_count;address runtime;u64 runtime_count;
    BindingBytes root_abi;u64 has_root_abi;u64 types;u64 programs;address order;
};
struct ResourceOutput {address data;address text;};
struct ResourcePlan {
    u64 data;u64 text;u64 data_flags;u64 text_flags;u64 error;u64 at;
    u64 factory_suffix;u64 type_suffix;u64 program_suffix;
};
extern "C" ResourcePlan prepare(ResourceInput input) on cpu;
extern "C" void encode(ResourceInput input,ResourceOutput output,ResourcePlan plan) on cpu;
extern "C" void encode_module(ResourceModule row,address data,address factory,address type_name,address program_name) on cpu;
extern "C" u64 module_prefix(address name,u64 length,address output) on cpu;
