module applications.map_cpu;
import lessons.map;

u32 main() on cpu {
    dataset<f32> values = dataset<f32>(4);

    target(cpu, demo(values));

    for (value in values) { if (!println(value)) { return 1; } }
    return 0;
}
