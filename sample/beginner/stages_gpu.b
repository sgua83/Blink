module beginner.stages_gpu;
import beginner.stages;

u32 main() on cpu {
    dataset<u32> numbers = dataset<u32>(4);
    upload(numbers);
    run_gpu(beginner.stages::run(numbers));
    download(numbers);
    for (i in 0..numbers.length) {
        println(numbers[i]);
    }
    return 0;
}
