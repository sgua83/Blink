module lessons.tiles;

cooperative f32 center(tile<f32> values, f32 value) on compute {
    f32 total = tile_reduce_sum(values);
    return value - total / f32(values.length);
}

void seed(write dataset<Cell> cells) on compute {
    parallel (cell in cells) {
        cell.value = f32(index() + 1);
        cell.total = 0.0;
        cell.before = 0.0;
        cell.centered = 0.0;
    }
}

void analyze<const Width: u32>(inout dataset<Cell> cells) on compute {
    parallel (cell in cells) {
        let values = tile_input(cells.value, Width);
        f32 total = tile_reduce_sum(values);
        f32 before = tile_scan_exclusive_sum(values);
        f32 centered = cooperate center(values, cell.value);
        cell.total = total;
        cell.before = before;
        cell.centered = centered;
    }
}

void publish<const Width: u32>(inout dataset<Cell> cells, f32 factor) on compute {
    parallel (cell in cells) {
        let transformed = tile_publish(cell.value * factor, Width);
        f32 total = tile_reduce_sum(transformed);
        cell.value = transformed[transformed.lane];
        cell.total = total;
        cell.centered = cell.value - total / f32(transformed.length);
    }
}

void demo<const Width: u32>(inout dataset<Cell> cells) on compute {
    seed(cells);
    analyze<Width>(cells);
}

void demo_publish<const Width: u32>(inout dataset<Cell> cells, f32 factor) on compute {
    seed(cells);
    publish<Width>(cells, factor);
}
