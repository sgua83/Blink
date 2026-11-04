module applications.map_gpu;

void scale_values(inout dataset<f32> values) on compute {
    parallel (value in values) { value = value * 2.0 + 1.0; }
}

u32 main() on cpu {
    dataset<f32> values = dataset<f32>(4);

    parallel (value in values) { value = f32(index() + 1); }

    upload(values);
    target(gpu, scale_values(values));
    download(values);

    for (value in values) { if (!println(value)) { return 1; } }
    return 0;
}
