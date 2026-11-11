module bootstrap.value_domain;
import bootstrap.ast_storage;

// The syntax store is sealed. The caller owns one index and one value per parameter.
extern "C" u32 advance(address nodes, Slice parameters, address indices, address row, u32 first) on cpu;
