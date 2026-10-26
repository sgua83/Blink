module beginner.shape;

u32 main() on cpu {
    dataset<u32, 4> numbers = dataset<u32>(4);
    run_cpu(run<4>(numbers));
    for (i in 0..numbers.length) {
        println(numbers[i]);
    }
    return 0;
}

program run<const N: u32>(write dataset<u32, N> numbers) on compute {
    parallel (number in numbers) {
        number = index() + 1;
    }
}
