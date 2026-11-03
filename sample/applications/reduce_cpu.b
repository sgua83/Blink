module applications.reduce_cpu;
import lessons.scan;

u32 main() on cpu {
    dataset<Count> input = dataset<Count>(4);
    dataset<Count, 1> total = dataset<Count>(1);

    target(cpu, demo_sum(input, total));

    if (!println(total[0].value)) { return 1; }
    return 0;
}
