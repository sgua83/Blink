module applications.scan_cpu;
import lessons.scan;

u32 main() on cpu {
    dataset<Count> input = dataset<Count>(4);
    dataset<Count> output = dataset<Count>(4);

    target(cpu, demo(input, output));

    for (row in output) { if (!println(row.value)) { return 1; } }
    return 0;
}
