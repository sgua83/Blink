module beginner.generic;

T twice<T: floating>(T value) on compute;
program run(write dataset<f32> numbers) on compute;
