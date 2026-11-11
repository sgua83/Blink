module bootstrap.instance_domain;
import bootstrap.ast_storage;

struct InstanceDomains {
    address nodes;
    Slice parameters;
    address type_sizes;
    u64 type_count;
};

struct InstanceRow {
    address indices;
    address values;
};

// Types precede the const parameters. The caller owns one index per axis
// and one value per const parameter, and keeps the syntax store sealed.
extern "C" u32 advance(InstanceDomains domains, InstanceRow row, u32 first) on cpu;

struct NameBytes {
    address bytes;
    u64 length;
};

struct InstanceName {
    NameBytes base;
    address types;
    u64 type_count;
    address values;
    u64 value_count;
};

// All input ranges and the final output belong to this synchronous caller.
extern "C" u64 key_size(InstanceName name) on cpu;
extern "C" void write_key(InstanceName name, address output) on cpu;
