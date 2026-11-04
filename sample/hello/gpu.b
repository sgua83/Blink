module gpu;

void fill_numbers(write dataset<f32> values) on compute {
    parallel (value in values) {
        value = f32(index() + 1) * 2.0 + 1.0;
    }
}

u32 main() on cpu {
    dataset<f32> values = dataset<f32>(4);

    upload(values);
    target(gpu, fill_numbers(values));
    download(values);

    for (value in values) {
        println(value);
    }
    return 0;
}
