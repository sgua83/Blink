module beginner.totals;

u32 main() on cpu {
    dataset<Number> numbers = dataset<Number>(4);
    dataset<Number> before = dataset<Number>(4);
    dataset<Number, 1> total = dataset<Number>(1);
    run_cpu(run(numbers, before, total));
    for (i in 0..before.length) {
        println(before[i].value);
    }
    println(total[0].value);
    return 0;
}

program seed(write dataset<Number> numbers) on compute {
    parallel (number in numbers) {
        number.value = index() + 1;
    }
}

program summarize(read dataset<Number> numbers,
                  write dataset<Number> before,
                  write dataset<Number, 1> total) on compute {
    scan_exclusive_sum(numbers.value, before.value);
    reduce_sum(numbers.value, total.value);
}

program run(inout dataset<Number> numbers,
            write dataset<Number> before,
            write dataset<Number, 1> total) on compute {
    seed(numbers);
    summarize(numbers, before, total);
}
