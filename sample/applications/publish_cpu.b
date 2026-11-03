module applications.publish_cpu;
import lessons.tiles;

u32 main() on cpu {
    dataset<Cell> cells = dataset<Cell>(6);

    target(cpu, demo_publish<4>(cells, 2.0));

    for (cell in cells) {
        if (!println(cell.value) || !println(cell.total) ||
            !println(cell.centered)) { return 1; }
    }
    return 0;
}
