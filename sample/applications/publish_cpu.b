module applications.publish_cpu;

struct Cell { f32 value; f32 total; f32 before; f32 centered; };

u32 main() on cpu {
    dataset<Cell> cells = dataset<Cell>(6);

    parallel (cell in cells) {
        cell.value = f32(index() + 1);
    }

    parallel (cell in cells) {
        let transformed = tile_publish(cell.value * 2.0, 4);
        f32 total = tile_reduce_sum(transformed);
        cell.value = transformed[transformed.lane];
        cell.total = total;
        cell.centered = cell.value - total / f32(transformed.length);
    }

    for (cell in cells) {
        if (!println(cell.value) || !println(cell.total) ||
            !println(cell.centered)) { return 1; }
    }
    return 0;
}
