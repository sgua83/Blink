module beginner.totals;

struct Number { u32 value; };
u32 main() on cpu;
void seed(write dataset<Number> numbers) on compute;
void summarize(read dataset<Number> numbers,
                  write dataset<Number> before,
                  write dataset<Number, 1> total) on compute;
void run(inout dataset<Number> numbers,
            write dataset<Number> before,
            write dataset<Number, 1> total) on compute;
