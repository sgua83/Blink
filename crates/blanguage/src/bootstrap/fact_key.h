module bootstrap.fact_key;
import codec.builtin_type;

enum IdentityKind:u32 { Register=0; DataLength=1; TileLength=2; ParameterContent=3; ResourceParameter=4; ParameterStorage=5; ElementComponent=6; ElementParent=7; ElementIndex=8; DatasetContent=9; MemoryValue=10; LoopValue=11; TaskResult=12; TaskCompletion=13; TaskWorkCompletion=14; TaskControl=15; TaskReturnSource=16; };
enum OperandKind:u32 { Value=0; Constant=1; };
enum ComparisonKind:u32 { Equal=0; NotEqual=1; Less=2; Greater=3; LessEqual=4; GreaterEqual=5; };
enum DomainKind:u32 { Data=0; Fixed=1; Tile=2; DataLength=3; TileLength=4; View=5; ViewLength=6; ApplicationData=7; ApplicationLength=8; Upper=9; Lower=10; Room=11; Sum=12; Offset=13; Shifted=14; Computed=15; CommonRange=16; KnownBits=17; Floating=18; };
enum FactKind:u32 { Equal=0; Index=1; Minimum=2; Maximum=3; NonZero=4; Boolean=5; Below=6; Outside=7; Zero=8; NotEqual=9; Predicate=10; };
struct NumericIdentity { IdentityKind kind; u64 first; u64 second; u64 third; };
struct NumericOperand { OperandKind kind; NumericIdentity identity; BuiltinScalar scalar; u64 bits; };
struct NumericPredicate { ComparisonKind operation; BuiltinScalar scalar; NumericOperand left; NumericOperand right; };
struct FactView { u32 present; FactKind kind; address first; address second; address payload; address scalar; u64 value; u64 mask; u32 flag; u32 fixed; };

struct FactKey { array<u64,18> words; };

extern "C" i32 order_identity(address left, address right) on cpu;
extern "C" i32 order_domain(address left, address right) on cpu;
extern "C" i32 order_operand(address left, address right) on cpu;
extern "C" i32 order_selector(address left, address right) on cpu;
extern "C" i32 order_predicate(address left, address right) on cpu;
extern "C" i32 order_expression(address left, address right) on cpu;
extern "C" i32 order_floating(address left, address right) on cpu;
extern "C" i32 order_fact<const VIEW:u32>(address left,address right) on cpu;

// C layouts of the physical keys used at the Rust ABI.
struct IdentityKey { IdentityKind kind; u32 packed; u64 first; u64 second; u64 third; };
struct OperandKey { OperandKind kind; u8 scalar; u64 first; u64 second; u64 third; u64 fourth; };
struct PredicateKey { u8 operation; u8 scalar; OperandKey left; OperandKey right; };

IdentityKey make_key_identity(NumericIdentity source) on cpu;
OperandKey make_key_operand(NumericOperand source) on cpu;
PredicateKey make_key_predicate(NumericPredicate source) on cpu;

struct DomainCastView { DomainKind kind; address low; address extended; u64 mask; u64 value; };
extern "C" DomainCastView inspect_domain_cast(address domain) on cpu;
extern "C" void write_common_range_cast(address domain, u8 low, u8 extended, u8 target) on cpu;
extern "C" void write_known_bits_cast(address domain, u8 target, u64 mask, u64 value) on cpu;

struct FactScalarView { u32 kind; u32 width; u64 mask; u64 bias; };
extern "C" BuiltinScalar fact_scalar_tag(address scalar) on cpu;
extern "C" FactScalarView inspect_fact_scalar(address scalar) on cpu;
extern "C" FactView read_fact_fields(address source) on cpu;
extern "C" FactView inspect_fact(address source) on cpu;
extern "C" FactView fact_bound(address identity, address scalar, u64 value, u32 minimum) on cpu;
extern "C" void materialize_fact(FactView source, address output) on cpu;
extern "C" NumericIdentity capture_key_identity(address source) on cpu;
extern "C" NumericOperand capture_key_operand(address source) on cpu;
extern "C" NumericPredicate capture_key_predicate(address source) on cpu;
