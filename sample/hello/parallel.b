module parallel;

u32 main() on cpu {
    dataset<f32> values = dataset<f32>(4);

    parallel (value in values) {
        value = f32(index() + 1) * 2.0 + 1.0;
    }

    for (value in values) {
        println(value);
    }
    return 0;
}
