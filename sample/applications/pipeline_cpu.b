module applications.pipeline_cpu;
import lessons.pipeline;

u32 main() on cpu {
    dataset<Item> input = dataset<Item>(8);
    dataset<Item> selected = dataset<Item>(8);
    dataset<Item> ordered = dataset<Item>(8);
    dataset<Count, 1> count = dataset<Count>(1);
    dataset<Offset> offsets = dataset<Offset>(8);
    dataset<Count, 1> total = dataset<Count>(1);

    target(cpu, demo(input, selected, ordered, count, offsets, total));

    if (!println(count[0].value) || !println(total[0].value)) { return 1; }
    return 0;
}
