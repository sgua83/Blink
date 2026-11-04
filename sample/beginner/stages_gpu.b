module beginner.stages_gpu;

void initialize_and_scale(write dataset<u32> numbers) on compute {
    parallel (number in numbers) { number = index() + 1; }
    parallel (number in numbers) { number = number * 2 + 1; }
}

u32 main() on cpu {
    dataset<u32> numbers = dataset<u32>(4);
    upload(numbers);
    target(gpu, initialize_and_scale(numbers));
    download(numbers);
    for (number in numbers) { println(number); }
    return 0;
}
