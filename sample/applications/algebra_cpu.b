module applications.algebra_cpu;
import lessons.algebra;

u32 main() on cpu {
    dataset<Transform> input = dataset<Transform>(4);
    dataset<Transform> before = dataset<Transform>(4);
    dataset<Transform, 1> total = dataset<Transform>(1);

    target(cpu, demo(input, before, total));

    for (row in before) {
        if (!println(row.value.scale) || !println(row.value.bias)) { return 1; }
    }
    if (!println(total[0].value.scale) || !println(total[0].value.bias)) { return 1; }
    return 0;
}
