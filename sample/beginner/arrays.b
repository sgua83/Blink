module beginner.arrays;

program run(write dataset<u32> numbers) on compute {
    parallel (number in numbers) {
        array<u32, 3> values = array<u32, 3>(1, 2, 3);
        u32 total = 0;
        for (i in 0..values.length) {
            total += values[i];
        }
        number = total;
    }
}
