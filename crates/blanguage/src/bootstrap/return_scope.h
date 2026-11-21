module bootstrap.return_scope;
import bootstrap.ast_storage;

// The original sealed syntax owner lends these three regions and the body
// index slice for this synchronous query. No source or analysis state is kept.
extern "C" bool early_returns(address nodes,address source,address stored,Slice body) on cpu;

// Finds a possible exit from this loop. A seek may reach the bound.
extern "C" bool loop_exit(address nodes,Slice body) on cpu;
