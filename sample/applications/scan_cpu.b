module applications.scan_cpu;

struct Count { u32 value; };

void exclusive_offsets(read dataset<Count> input, write dataset<Count> output) on compute {
    scan_exclusive_sum(input.value, output.value);
}

u32 main() on cpu {
    dataset<Count> input = dataset<Count>(4);
    dataset<Count> output = dataset<Count>(4);

    parallel (item in input) {
        let k = index() % 4;
        item.value = k == 0 ? 3 : (k == 1 ? 1 : (k == 2 ? 4 : 2));
    }

    exclusive_offsets(input, output);

    for (row in output) { if (!println(row.value)) { return 1; } }
    return 0;
}
