module beginner.scores;

u32 main() on cpu {
    dataset<Result> results = dataset<Result>(4);
    dataset<Number, 1> total = dataset<Number>(1);
    run_cpu(run(results, total, 5));
    for (i in 0..results.length) {
        println(results[i].original);
        println(results[i].adjusted);
        println(results[i].passed);
    }
    println(total[0].value);
    return 0;
}

u32 final_score(u32 score, u32 bonus) on compute {
    u32 adjusted = score + bonus;
    if (adjusted > 100) {
        return 100;
    }
    return adjusted;
}

program fill(write dataset<Result> results, u32 bonus) on compute {
    parallel (result in results) {
        result.original = 45 + index() * 15;
        result.adjusted = final_score(result.original, bonus);
        result.passed = result.adjusted >= 60;
    }
}

program sum(read dataset<Result> results,
            write dataset<Number, 1> total) on compute {
    reduce_sum(results.adjusted, total.value);
}

program run(inout dataset<Result> results,
            write dataset<Number, 1> total, u32 bonus) on compute {
    fill(results, bonus);
    sum(results, total);
}
