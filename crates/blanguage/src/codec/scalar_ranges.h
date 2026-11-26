module codec.scalar_ranges;

struct BitRange { u64 lower; u64 upper; };
struct BitPattern { u64 mask; u64 value; };
struct RangeQuery { u64 order_bias; BitRange range; BitPattern known; BitPattern required; };

// Reads borrow the original interval storage for the duration of the query.
struct RangeInput {
    address values;
    u64 count;
    cfn<BitRange,address,u64> get;
};
struct RangeSetNext { BitRange value; u32 present; };

extern "C" bool covers_range(read RangeQuery query, address intervals, u64 count,
    cfn<BitRange,address,u64> interval) on cpu requires nonzero(interval);
extern "C" bool element_ranges_within(RangeInput actual, RangeInput allowed,
    u64 order_bias, BitPattern known, BitPattern required) on cpu
    requires nonzero(actual.get) requires nonzero(allowed.get);

extern "C" u64 intersect_ranges(RangeInput left, RangeInput right,
    address output, cfn<void,address,BitRange> push) on cpu
    requires nonzero(left.get) requires nonzero(right.get) requires nonzero(push);
bool ranges_overlap(RangeInput left, RangeInput right) on cpu
    requires nonzero(left.get) requires nonzero(right.get);
extern "C" BitPattern intersect_element_ranges(RangeInput left, RangeInput right,
    cfn<void,address,u64,BitRange> write_left, cfn<void,address,u64,BitRange> write_right,
    BitPattern left_bits, BitPattern right_bits, address output, cfn<void,address,BitRange> push) on cpu
    requires nonzero(left.get) requires nonzero(right.get)
    requires nonzero(write_left) requires nonzero(write_right) requires nonzero(push);

bool range_set_less(BitRange a, BitRange b) on cpu;
bool range_set_joined(BitRange a, BitRange b) on cpu;
RangeSetNext range_set_accept(RangeSetNext pending, BitRange value, address output,
    cfn<void,address,BitRange> push) on cpu requires nonzero(push);
void sort_ranges(address values, u64 count, cfn<BitRange,address,u64> get,
    cfn<void,address,u64,BitRange> set) on cpu requires nonzero(get) requires nonzero(set);
