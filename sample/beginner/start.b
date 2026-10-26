module beginner.start;

program run(write dataset<u32> numbers) on compute {
    parallel (number in numbers) {
        number = 1 + 2;
    }
}
