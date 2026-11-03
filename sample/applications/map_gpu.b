module applications.map_gpu;
import lessons.map;

u32 main() on cpu {
    dataset<f32> values = dataset<f32>(4);

    upload(values);
    target(gpu, demo(values));
    download(values);

    for (value in values) { if (!println(value)) { return 1; } }
    return 0;
}
