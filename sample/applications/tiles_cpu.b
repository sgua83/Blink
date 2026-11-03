module applications.tiles_cpu;
import lessons.tiles;

u32 main() on cpu {
    dataset<Cell> cells = dataset<Cell>(6);

    target(cpu, demo<4>(cells));

    for (cell in cells) {
        if (!println(cell.value) || !println(cell.total) ||
            !println(cell.before) || !println(cell.centered)) { return 1; }
    }
    return 0;
}
