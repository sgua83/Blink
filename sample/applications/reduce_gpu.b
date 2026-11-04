module applications.reduce_gpu;

struct Count { u32 value; };

void sum_values(read dataset<Count> input, write dataset<Count, 1> total) on compute {
    reduce_sum(input.value, total.value);
}

u32 main() on cpu {
    dataset<Count> input = dataset<Count>(4);
    dataset<Count, 1> total = dataset<Count>(1);

    parallel (item in input) {
        let k = index() % 4;
        item.value = k == 0 ? 3 : (k == 1 ? 1 : (k == 2 ? 4 : 2));
    }

    upload(input);
    upload(total);
    target(gpu, sum_values(input, total));
    download(total);

    if (!println(total[0].value)) { return 1; }
    return 0;
}
