module lessons.scan;

struct Count { u32 value; };

program seed(write dataset<Count> input) on compute;
program offsets(read dataset<Count> input, write dataset<Count> output) on compute;
program sum(read dataset<Count> input, write dataset<Count, 1> total) on compute;
program demo(inout dataset<Count> input, write dataset<Count> output) on compute;
program demo_sum(inout dataset<Count> input, write dataset<Count, 1> total) on compute;
