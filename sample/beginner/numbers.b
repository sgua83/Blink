module beginner.numbers;

u32 main() on cpu {
    dataset<u32> numbers = dataset<u32>(4);
    run_cpu(run(numbers));
    for (i in 0..numbers.length) {
        println(numbers[i]);
    }
    return 0;
}

program run(write dataset<u32> numbers) on compute {
    parallel (number in numbers) {
        u32 value = index() + 1;
        number = value * 2;
    }
}
