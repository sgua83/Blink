module bootstrap.return_scope;
import bootstrap.ast_storage;

// The original sealed syntax owner lends these three regions and the body
// index slice for this synchronous query. No source or analysis state is kept.
extern "C" bool early_returns(address nodes,address source,address stored,Slice body) on cpu;
