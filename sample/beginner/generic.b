module beginner.generic;

T twice<T: floating>(T value) on compute {
    return value * 2.0;
}

program run(write dataset<f32> numbers) on compute {
    parallel (number in numbers) {
        number = twice(f32(index() + 1));
    }
}
