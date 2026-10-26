module beginner.scores;

struct Result {
    u32 original;
    u32 adjusted;
    bool passed;
};
struct Number { u32 value; };
u32 main() on cpu;
u32 final_score(u32 score, u32 bonus) on compute;
program fill(write dataset<Result> results, u32 bonus) on compute;
program sum(read dataset<Result> results,
            write dataset<Number, 1> total) on compute;
program run(inout dataset<Result> results,
            write dataset<Number, 1> total, u32 bonus) on compute;
