module beginner.scores_gpu;

struct Result { u32 original; u32 adjusted; bool passed; };
struct Number { u32 value; };

u32 final_score(u32 score, u32 bonus) on compute {
    u32 adjusted = score + bonus;
    if (adjusted > 100) { return 100; }
    return adjusted;
}

void grade_and_sum(inout dataset<Result> results,
                   write dataset<Number, 1> total, u32 bonus) on compute {
    parallel (result in results) {
        result.original = 45 + index() * 15;
        result.adjusted = final_score(result.original, bonus);
        result.passed = result.adjusted >= 60;
    }
    reduce_sum(results.adjusted, total.value);
}

u32 main() on cpu {
    dataset<Result> results = dataset<Result>(4);
    dataset<Number, 1> total = dataset<Number>(1);
    upload(results);
    upload(total);
    target(gpu, grade_and_sum(results, total, 5));
    download(results);
    download(total);
    for (result in results) {
        println(result.original);
        println(result.adjusted);
        println(result.passed);
    }
    println(total[0].value);
    return 0;
}
