module lessons.scan;

struct Count { u32 value; };

void seed(write dataset<Count> input) on compute;
void offsets(read dataset<Count> input, write dataset<Count> output) on compute;
void sum(read dataset<Count> input, write dataset<Count, 1> total) on compute;
void demo(inout dataset<Count> input, write dataset<Count> output) on compute;
void demo_sum(inout dataset<Count> input, write dataset<Count, 1> total) on compute;
