module beginner.types;

program integer(write dataset<u32> numbers) on compute {
    parallel (number in numbers) {
        number = 7 / 2;
    }
}

program decimal(write dataset<f32> numbers) on compute {
    parallel (number in numbers) {
        number = 7.0 / 2.0;
    }
}

program signed(write dataset<i32> numbers) on compute {
    parallel (number in numbers) {
        i32 balance = -7;
        number = balance / 2;
    }
}

program flag(write dataset<bool> numbers) on compute {
    parallel (number in numbers) {
        number = 7 > 2;
    }
}
