module applications.tiles_gpu;
import lessons.tiles;

u32 main() on cpu {
    dataset<Cell> cells = dataset<Cell>(6);

    upload(cells);
    target(gpu, demo<4>(cells));
    download(cells);

    for (cell in cells) {
        if (!println(cell.value) || !println(cell.total) ||
            !println(cell.before) || !println(cell.centered)) { return 1; }
    }
    return 0;
}
