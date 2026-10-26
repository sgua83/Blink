module beginner.modules;

program run(write dataset<u32> numbers, u32 extra) on compute {
    parallel (number in numbers) {
        number = add_base(index() + 1) + extra;
    }
}
