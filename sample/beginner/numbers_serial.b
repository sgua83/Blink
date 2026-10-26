module beginner.numbers_serial;

u32 main() on cpu {
    dataset<u32> numbers = dataset<u32>(4);
    for (i in 0..numbers.length) {
        u32 value = i + 1;
        numbers[i] = value * 2;
    }
    for (i in 0..numbers.length) {
        println(numbers[i]);
    }
    return 0;
}
