module applications.tiles_cpu;

struct Cell { f32 value; f32 total; f32 before; f32 centered; };

cooperative f32 center(tile<f32> values, f32 value) on compute {
    f32 total = tile_reduce_sum(values);
    return value - total / f32(values.length);
}

u32 main() on cpu {
    dataset<Cell> cells = dataset<Cell>(6);

    parallel (cell in cells) {
        cell.value = f32(index() + 1);
    }

    parallel (cell in cells) {
        let values = tile_input(cells.value, 4);
        f32 total = tile_reduce_sum(values);
        f32 before = tile_scan_exclusive_sum(values);
        f32 centered = cooperate center(values, cell.value);
        cell.total = total;
        cell.before = before;
        cell.centered = centered;
    }

    for (cell in cells) {
        if (!println(cell.value) || !println(cell.total) ||
            !println(cell.before) || !println(cell.centered)) { return 1; }
    }
    return 0;
}
