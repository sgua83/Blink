module beginner.stages;

u32 main() on cpu;
void seed(write dataset<u32> numbers) on compute;
void scale(inout dataset<u32> numbers) on compute;
void run(inout dataset<u32> numbers) on compute;
