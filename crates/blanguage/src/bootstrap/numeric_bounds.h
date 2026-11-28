module bootstrap.numeric_bounds;
import codec.scalar_ranges;

extern "C" void numeric_bounds(address source,address roots,address output) on cpu;
extern "C" BitRange numeric_limits(address owner,address identity,address scalar) on cpu;
