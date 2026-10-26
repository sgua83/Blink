module beginner.names;

program run(write dataset<u32> numbers) on compute {
    parallel (number in numbers) {
        u32 total = 10;
        let bonus = 3;
        total += bonus;
        total *= 2;
        number = total;
    }
}
