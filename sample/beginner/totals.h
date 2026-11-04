module beginner.totals;

struct Number { u32 value; };
u32 main() on cpu;
void summarize(read dataset<Number> numbers,
                  write dataset<Number> before,
                  write dataset<Number, 1> total) on compute;
