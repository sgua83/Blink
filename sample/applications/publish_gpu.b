module applications.publish_gpu;
import lessons.tiles;

u32 main() on cpu {
    dataset<Cell> cells = dataset<Cell>(6);

    upload(cells);
    target(gpu, demo_publish<4>(cells, 2.0));
    download(cells);

    for (cell in cells) {
        if (!println(cell.value) || !println(cell.total) ||
            !println(cell.centered)) { return 1; }
    }
    return 0;
}
