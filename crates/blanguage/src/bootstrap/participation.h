module bootstrap.participation;
import bootstrap.ast_storage;
import bootstrap.signatures;
import bootstrap.names;

struct FunctionInput { NameBytes key; SignatureNode source; u32 compiled; u32 requirement; };
struct BodyState { u32 domain; u32 local; u32 host; u32 requirement; };
struct CallEdge { u64 caller; u64 callee; u64 statement; };

// World lookup and its original dependency set remain with the synchronous caller.
// The source owners and all buffers stay live through scan and solve; B retains none.
// lookup returns Unique=0, Record=1, Missing=2, Ambiguous=3, StageIntrinsic=4.
extern "C" void scan(address functions, u64 count, address states, address context,
    cfn<u32,address,u64,NameBytes,u32> lookup) on cpu requires nonzero(lookup);
extern "C" bool less_call(address left, address right, address context) on cpu;
extern "C" void solve(address functions, u64 count, address states, address calls, u64 call_count,
    cfn<bool,address,address,address> less) on cpu requires nonzero(less);
