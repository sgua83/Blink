module beginner.borrow;

void add_one(inout u32 value) on compute;
void twice(u32 value, write u32 output) on compute;
program run(write dataset<u32> numbers) on compute;
