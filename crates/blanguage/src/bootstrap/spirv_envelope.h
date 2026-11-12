module bootstrap.spirv_envelope;
import bootstrap.type_families;
import bootstrap.scalar_fold;

struct Features {
    u32 atomic64;
    u32 float64;
    u32 integer8;
    u32 integer16;
    u32 preserve32;
    u32 preserve64;
    u32 subgroup_basic;
    u32 subgroup_vote;
    u32 subgroup_arithmetic;
    u32 subgroup_ballot;
    u32 subgroup_shuffle;
    u32 subgroup_shuffle_relative;
    u32 acceleration;
    u32 position_fetch;
    u32 texture;
    u32 descriptor_indexing;
    u32 maximal_reconvergence;
};
struct Envelope {
    u32 bound;
    u32 ext;
    u32 entry;
    u32 workgroup_width;
    Features features;
};
struct Words { address data; u64 count; };

// Fixed-word capacity excludes the interface IDs. The slices are borrowed in
// the declared interface order. A zero ID is an absent optional variable.
extern "C" u64 envelope_capacity() on cpu;
extern "C" u64 encode(Envelope input, address interfaces, u64 count, address words) on cpu;

// The compiler supplies an existing concrete scalar and already allocated IDs.
// Storage has room for 8 type/pointer words or 5 constant words respectively.
// Constant bits are the original Scalar-truncated cache key.
extern "C" u64 scalar_declaration(BuiltinScalar scalar, u32 type_id, u32 pointer_id, address words) on cpu;
extern "C" u64 constant_declaration(BuiltinScalar scalar, u32 type_id, u32 id, u64 bits, address words) on cpu;

enum TypeShape:u32 { Void=0; Vector=1; Matrix=2; SampledImage=3; Array=4; Function=5; Acceleration=6; RayQuery=7; Image=8; Pointer=9; };
struct ResourceBlock { u32 element; u32 length; u32 array; u32 block; u32 pointer; };

// The existing shader supplies result IDs and type operands. Output has room for 9 words.
extern "C" u64 type_declaration(TypeShape shape, u32 id, u32 first, u32 second, address words) on cpu;
// A zero length ID selects the runtime array for the page table.
// Definitions have room for 11 words, and the 12 decoration words are always written.
extern "C" u64 resource_block(ResourceBlock input, address definitions, address decorations) on cpu;

enum StorageClass:u32 {
    UniformConstant=0; Input=1; Workgroup=4; Function=7; PushConstant=9; StorageBuffer=12; PhysicalStorageBuffer=5349;
};
enum VariableAnnotation:u32 { Descriptor=0; PageTable=1; BuiltIn=2; };

// The caller owns four definition words for a variable without an initializer.
extern "C" void variable_declaration(StorageClass storage, u32 pointer, u32 id, address words) on cpu;
// Output has room for four definition words and eleven decoration words.
// Operand is the assigned descriptor binding or the built-in enumerant.
extern "C" u64 annotated_variable(VariableAnnotation annotation, u32 pointer, u32 id, u32 operand, address definitions, address decorations) on cpu;
enum FlagDecoration:u32 { NoContraction=42; NonUniform=5300; };
// Output has room for three decoration words.
extern "C" void flag_decoration(FlagDecoration decoration, u32 id, address decorations) on cpu;

struct BinaryEmission {
    BinaryOperation operation;
    BuiltinScalar scalar;
    u32 result_type;
    u32 result;
    u32 left;
    u32 right;
    u32 mask;
    u32 masked;
};
struct BinaryWords { u64 body; u64 decorations; };
struct CastEmission {
    BuiltinScalar from;
    BuiltinScalar to;
    u32 result_type;
    u32 result;
    u32 value;
    u32 zero;
    u32 one;
};

// The original emitter owns the IDs and buffers. A masked ID is present for shifts.
// Storage has room for ten body words and three decoration words.
extern "C" BinaryWords scalar_binary(BinaryEmission input, address body, address decorations) on cpu;
// Zero and one are the original constant IDs for a conversion involving bool.
// Storage has room for six body words.
extern "C" u64 scalar_cast(CastEmission input, address body) on cpu;

struct DivisionEmission {
    u32 integer_type;
    u32 magnitude_type;
    u32 numerator;
    u32 denominator;
    u32 quotient;
    u32 remainder;
    u32 sign_shift;
    u32 next;
};
struct DivisionWords { u32 body; u32 next; };

// A zero result ID denotes the unused quotient or remainder. A zero sign-shift
// ID denotes unsigned inputs. The caller supplies the original next ID and
// holds ten (unsigned) or eighty-one (signed) output words.
extern "C" DivisionWords integer_division(DivisionEmission input, address body) on cpu;

// Operation identities follow the original IR UnaryOp and MathOp.
enum UnaryOperation:u32 {
    FloatNegate=0; SignedNegate=1; UnsignedNegate=2; LogicalNot=3; BitNot=4; Bitcast=5;
};
enum MathOperation:u32 {
    Abs=0; Floor=1; Sin=2; Cos=3; Pow=4; Exp=5; Log=6; Exp2=7; Log2=8; Sqrt=9; Min=10; Max=11;
};
struct FloatSignOperands { u32 raw_type; u32 mask; u32 temporary; };
struct UnaryEmission {
    UnaryOperation operation;
    u32 result_type;
    u32 result;
    u32 value;
    u32 zero;
    FloatSignOperands sign;
};
struct MathEmission {
    MathOperation operation;
    u32 result_type;
    u32 result;
    u32 left;
    u32 right;
    u32 extended_set;
    FloatSignOperands sign;
};
// The emitter supplies consecutive temporary IDs for the sign operation and
// thirteen output words. A zero right operand ID denotes a unary math operation.
extern "C" u64 scalar_unary(UnaryEmission input, address body) on cpu;
extern "C" u64 scalar_math(MathEmission input, address body) on cpu;

struct PageAddress {
    u32 u32_type;
    u32 u64_type;
    u32 descriptor_pointer;
    u32 page_pointer;
    u32 result_pointer;
    u32 descriptor;
    u32 low;
    u32 high;
    u32 stride;
    u32 index;
    u32 word;
    u32 extra;
    u32 zero;
    u32 one;
    u32 high_shift;
    u32 page_shift;
    u32 page_bytes;
    u32 next;
};
struct PageAddressResult { u64 words; u32 next; u32 pointer; };
// A descriptor ID selects the dataset header. Otherwise the input contains the
// view page table and stride IDs. The extra byte offset is an optional ID.
// The emitter owns the next ID and up to 109 output words.
extern "C" PageAddressResult page_address(PageAddress input, address body) on cpu;
