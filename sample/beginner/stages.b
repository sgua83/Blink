module beginner.stages;

u32 main() on cpu {
    dataset<u32> numbers = dataset<u32>(4);
    run_cpu(run(numbers));
    for (i in 0..numbers.length) {
        println(numbers[i]);
    }
    return 0;
}

program seed(write dataset<u32> numbers) on compute {
    parallel (number in numbers) {
        number = index() + 1;
    }
}

program scale(inout dataset<u32> numbers) on compute {
    parallel (number in numbers) {
        number = number * 2 + 1;
    }
}

program run(inout dataset<u32> numbers) on compute {
    seed(numbers);
    scale(numbers);
}
