module bootstrap.scalar_fold;
import codec.scalar_ranges;

enum UnaryOperation : u32 {
    Negate = 0; LogicalNot = 1; BitNot = 2; FloatNegate = 3;
};

enum BinaryOperation : u32 {
    Add = 0; Subtract = 1; Multiply = 2; Divide = 3;
    And = 4; Or = 5; Xor = 6;
    ShiftLeft = 7; ShiftRight = 8; ShiftRightSigned = 9;
    Equal = 10; NotEqual = 11; Less = 12; Greater = 13; LessEqual = 14; GreaterEqual = 15;
};

extern "C" u64 unary(UnaryOperation operation, u32 width, u64 bits) on cpu;
extern "C" u64 binary(BinaryOperation operation, u32 width, u32 signed, u64 left, u64 right) on cpu;
extern "C" u64 float_binary(BinaryOperation operation, u32 width, u64 left, u64 right) on cpu;
extern "C" u64 divide_unsigned(u64 left, u64 right, u32 remainder) on cpu requires nonzero(right);
extern "C" i64 divide_signed(i64 left, i64 right, u32 remainder) on cpu requires nonzero(right);

enum ScalarKind : u32 {
    Unsigned = 0; Signed = 1; Float = 2; Boolean = 3;
};

struct ScalarEncoding {
    ScalarKind kind;
    u32 width;
};

enum CommonRangeTypeDecision:u32 { Drop=0; Keep=1; Rebind=2; };
struct CommonRangeTypeResult {
    CommonRangeTypeDecision decision;
    u32 low;
    u32 extended;
};
extern "C" CommonRangeTypeResult common_range_cast_types(address input,u32 count,
    cfn<ScalarEncoding,address,u32> get,u32 initial_low_width,u32 initial_extended_width) on cpu requires nonzero(get);

struct FoldedCast {
    u64 bits;
    u32 present;
};

extern "C" FoldedCast cast(ScalarEncoding source, ScalarEncoding target, u64 bits) on cpu;

// Synchronous numerical results; the caller owns the original range container.
struct FloatInterval { f64 lower; f64 upper; u32 present; };
// state: 0 unknown, 1 one bounded interval, 2 empty set.
struct FloatRange { u64 lower; u64 upper; u32 state; };
struct FloatPieces { BitRange first; BitRange second; u32 count; };
extern "C" u64 float_order(u32 width,u64 value) on cpu;
extern "C" u64 float_bits(u32 width,u64 value) on cpu;
extern "C" f64 float_number(u32 width,u64 value) on cpu;
extern "C" FloatInterval float_finite(u32 width,BitRange range) on cpu;
extern "C" f64 float_rounded(u32 width,f64 value,u32 lower) on cpu;
extern "C" f64 float_sum_bound(u32 width,f64 left,f64 right,u32 lower) on cpu;
extern "C" FloatRange float_interval(u32 width,f64 low,f64 high) on cpu;
extern "C" u32 float_comparison_possible(BinaryOperation operation,u32 width,BitRange left,BitRange right,u32 truth_code) on cpu;
extern "C" FloatPieces float_bitcast(u32 width,BitRange range,u32 source_float) on cpu;
extern "C" FloatRange float_cast_range(ScalarEncoding source,ScalarEncoding target,BitRange range) on cpu;
extern "C" FloatRange float_range_arithmetic(BinaryOperation operation,u32 width,BitRange left,BitRange right) on cpu;
// math: 1 min, 2 max, 3 abs, 4 floor, 5 log, 6 log2, 7 ceil; other operations remain unknown.
extern "C" FloatRange float_math(u32 operation,u32 width,BitRange left,BitRange right) on cpu;
extern "C" FloatRange float_sum_range(u32 width,BitRange range,u64 maximum) on cpu;
extern "C" FloatRange float_negate(u32 width,BitRange range) on cpu;
extern "C" f64 float_add_up(f64 left,f64 right) on cpu;
extern "C" f64 float_multiply_up(f64 left,f64 right) on cpu;
extern "C" f64 float_divide_up(f64 left,f64 right) on cpu;
extern "C" f64 float_integer_bound(u64 value,u32 lower) on cpu;
extern "C" f64 float_conversion_error(u64 upper) on cpu;

enum IntegerBitwise : u32 { And = 0; Or = 1; Xor = 2; };
// Inputs are the original integer Scalar width/bias and ordered intervals.
extern "C" BitRange integer_bitwise_range(IntegerBitwise operation,u32 width,u64 order_bias,BitRange left,BitRange right) on cpu;

extern "C" BitPattern known_cast_path(ScalarEncoding source,address targets,u64 count,
    u64 first_required,BitPattern value) on cpu;
extern "C" BitPattern known_common(u64 target_mask,BitPattern a,u64 bm,u64 sign_b,BitPattern b) on cpu;
extern "C" BitPattern known_bitwise(IntegerBitwise operation,BitPattern a,BitPattern b) on cpu;
struct KnownDomain {u64 source_mask;u64 source_sign;BitPattern pattern;u32 present;};
extern "C" BitPattern known_proof(u64 mask,u64 bias,address ranges,u64 count,
    cfn<BitRange,address,u64> read,address domains,cfn<KnownDomain,address> next_domain) on cpu
    requires nonzero(read) requires nonzero(next_domain);
struct KnownBounds {BitRange range;u32 zero;};
struct KnownProjection {BitPattern pattern;u32 present;};
extern "C" KnownProjection known_facts(u64 mask,u64 bias,address context,u64 count,
    cfn<KnownBounds,address,u64> read_bound,cfn<KnownDomain,address> next_domain) on cpu
    requires nonzero(read_bound) requires nonzero(next_domain);
extern "C" BitPattern known_shifted(u64 mask,u64 bias,u32 direction,BitPattern value,address context,u64 count,cfn<BitRange,address,u64> read) on cpu requires nonzero(read);

enum IntegerRangeOperation : u32 {
    Minimum=0; Maximum=1; Add=2; Subtract=3; Multiply=4; Divide=5; Remainder=6;
    ShiftLeft=7; ShiftRight=8; BitAnd=9; BitOr=10; BitXor=11; Unknown=12;
};
enum IntegerRangeUnary : u32 { BitNot=0;Negate=1; };
struct IntegerWide {u64 low;u64 high;};
struct IntegerRangePieces {BitRange first;BitRange second;u32 count;};
extern "C" IntegerRangePieces integer_range(IntegerRangeOperation operation,u32 width,u32 signed,BitRange left,BitRange right) on cpu;
extern "C" IntegerRangePieces integer_range_cast(u32 source_width,u32 source_signed,u32 target_width,u64 target_bias,BitRange value) on cpu;
struct TileSumLanes { u32 minimum; u32 maximum; u32 has_nonzero; u32 nonzero; };
struct TileLane { u32 present; u32 lower; u32 upper; };
struct TileResidue { u64 period; u32 first; u32 last; };
struct TileNonzeroBound { u32 present; u32 value; };
extern "C" TileNonzeroBound tile_nonzero_bound(address context,u32 width,u32 maximum,
    cfn<TileLane,address> next_lane,cfn<TileResidue,address> next_residue) on cpu
    requires nonzero(next_lane) requires nonzero(next_residue);
extern "C" IntegerRangePieces integer_range_tile_sum(u32 width,u32 signed,BitRange values,TileSumLanes lanes) on cpu;
extern "C" IntegerRangePieces integer_range_unary(IntegerRangeUnary operation,u32 width,u32 signed,BitRange value) on cpu;
extern "C" IntegerRangePieces integer_range_counts(u32 width,u64 bias,BitRange value) on cpu;
extern "C" IntegerRangePieces integer_range_project(u32 width,u64 bias,IntegerWide lower,IntegerWide upper) on cpu;

// The original integer-range wide value is also used by relation coefficients.
struct IntegerWideChecked {u32 present;IntegerWide value;};
struct IntegerWideDivision {IntegerWide quotient;IntegerWide remainder;};
extern "C" IntegerWide range_add(IntegerWide a,IntegerWide b) on cpu;
extern "C" IntegerWide range_subtract(IntegerWide a,IntegerWide b) on cpu;
extern "C" IntegerWide range_negate(IntegerWide value) on cpu;
extern "C" bool range_less(IntegerWide a,IntegerWide b) on cpu;
extern "C" IntegerWide range_min(IntegerWide a,IntegerWide b) on cpu;
extern "C" IntegerWide range_max(IntegerWide a,IntegerWide b) on cpu;
extern "C" IntegerWide range_product(u64 a,u64 b) on cpu;
extern "C" bool integer_wide_unsigned_less(IntegerWide a,IntegerWide b) on cpu;
extern "C" IntegerWideChecked integer_wide_checked_add(IntegerWide a,IntegerWide b) on cpu;
extern "C" IntegerWideChecked integer_wide_checked_multiply(IntegerWide a,IntegerWide b) on cpu;
extern "C" IntegerWideDivision integer_wide_unsigned_divide(IntegerWide numerator,IntegerWide denominator) on cpu;

// Scalar set values borrow original native storage through synchronous reads.
struct RangeSetOutput {
    cfn<BitRange,address,u64> get;
    cfn<void,address,u64,BitRange> set;
    cfn<void,address,BitRange> push;
};
struct IntegerBinaryProof {
    IntegerRangeOperation operation;
    u32 width;
    u32 signed;
    u32 same_value;
    u64 bias;
    u64 mask;
    u64 minimum_difference;
    u64 right_bias;
    u32 right_width;
    u32 zero_quotient;
    u32 track_origin;
};
struct IntegerBinaryResult { u64 count; u32 origin; };
extern "C" IntegerBinaryResult range_set_integer_binary(RangeInput left, RangeInput right,
    RangeInput right_counts, address output, RangeSetOutput host, IntegerBinaryProof proof) on cpu
    requires nonzero(left.get) requires nonzero(right.get) requires nonzero(right_counts.get)
    requires nonzero(host.get) requires nonzero(host.set) requires nonzero(host.push);
struct RangeSetCastInfo {
    u32 source_width; u32 source_signed; u32 target_width;
    u32 source_integer; u32 same; u32 nonzero;
    u64 source_mask; u64 source_bias; u64 target_bias;
};
extern "C" u64 range_set_normalize(address output,u64 count,RangeSetOutput host) on cpu
    requires nonzero(host.get) requires nonzero(host.set);
extern "C" void range_set_sorted(address input,cfn<RangeSetNext,address> pull,address output,cfn<void,address,BitRange> push) on cpu
    requires nonzero(pull) requires nonzero(push);
extern "C" void range_set_full(address output,cfn<void,address,BitRange> push) on cpu requires nonzero(push);
extern "C" void range_set_interval(address output,cfn<void,address,BitRange> push,u64 lower,u64 upper) on cpu requires nonzero(push);
extern "C" BitRange range_set_hull(address input,u64 count,cfn<BitRange,address,u32,u64> get) on cpu requires nonzero(get);
extern "C" bool range_set_contains(address input,u64 count,cfn<BitRange,address,u32,u64> get,u64 value) on cpu requires nonzero(get);
extern "C" void range_set_union(address input,u64 left,u64 right,cfn<BitRange,address,u32,u64> get,address output,cfn<void,address,BitRange> push) on cpu
    requires nonzero(get) requires nonzero(push);
struct RangeSetProjection { u64 lower; u64 upper; u64 excluded; u32 exclude; };
extern "C" void range_set_project(address input,u64 count,cfn<BitRange,address,u32,u64> get,address output,cfn<void,address,BitRange> push,RangeSetProjection projection) on cpu
    requires nonzero(get) requires nonzero(push);
extern "C" bool range_set_project_contains(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    address output,cfn<void,address,BitRange> push,RangeSetProjection projection,u64 point) on cpu
    requires nonzero(get) requires nonzero(push);
extern "C" BitRange range_set_projected_hull(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    RangeSetProjection projection) on cpu requires nonzero(get);
struct RangeSetRequirement {
    u64 order_bias;
    BitPattern known;
    BitPattern required;
    address intervals;
    u64 count;
    cfn<BitRange,address,u64> interval;
};
extern "C" bool range_set_covers_projected(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    RangeSetProjection projection,RangeSetRequirement requirement) on cpu
    requires nonzero(get) requires nonzero(requirement.interval);
extern "C" void range_set_restrict(address input,u64 count,cfn<BitRange,address,u32,u64> get,address output,cfn<void,address,BitRange> push,u64 lower,u64 upper) on cpu
    requires nonzero(get) requires nonzero(push);
extern "C" void range_set_without(address input,u64 count,cfn<BitRange,address,u32,u64> get,address output,cfn<void,address,BitRange> push,u64 value) on cpu
    requires nonzero(get) requires nonzero(push);
extern "C" u64 range_set_cast(address input,u64 count,cfn<BitRange,address,u32,u64> get,address output,RangeSetOutput host,RangeSetCastInfo query) on cpu
    requires nonzero(get) requires nonzero(host.get) requires nonzero(host.set) requires nonzero(host.push);
struct RangeSetCastStep {
    u32 source_width; u32 source_signed; u32 target_width; u32 same;
    u64 target_bias;
};
extern "C" u64 range_set_cast_chain(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    address output,RangeSetOutput host,RangeSetCastInfo first,RangeSetCastStep second,RangeSetCastStep last) on cpu
    requires nonzero(get) requires nonzero(host.get) requires nonzero(host.set) requires nonzero(host.push);
struct RangeSetCastSummary { BitRange hull; u32 contains; };
extern "C" RangeSetCastSummary range_set_cast_summary(address input,u64 count,cfn<BitRange,address,u32,u64> get,
    RangeSetCastInfo first,RangeSetCastStep second,RangeSetCastStep last) on cpu requires nonzero(get);
extern "C" u64 range_set_counts(address input,u64 count,cfn<BitRange,address,u32,u64> get,address output,RangeSetOutput host,u32 width,u64 bias) on cpu
    requires nonzero(get) requires nonzero(host.get) requires nonzero(host.set) requires nonzero(host.push);
extern "C" u64 range_set_map(RangeInput input, address operation,
    cfn<u64,address,BitRange,address> invoke, address output, RangeSetOutput host) on cpu
    requires nonzero(input.get) requires nonzero(invoke) requires nonzero(host.get) requires nonzero(host.set);
