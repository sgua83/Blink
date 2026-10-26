module applications.datasets;
import lessons.pipeline;

dataset<Item> create_input(u32 length) on cpu {
    dataset<Item> input = dataset<Item>(length);
    run_cpu(seed(input));
    return input;
}

u32 main() on cpu {
    u32 size = 1000000;
    let input = create_input(size);
    dataset<Item> selected = dataset<Item>(size);
    dataset<Item> ordered = dataset<Item>(size);
    dataset<Count, 1> count = dataset<Count>(1);
    dataset<Offset> offsets = dataset<Offset>(size);
    dataset<Count, 1> total = dataset<Count>(1);

    upload(input);
    upload(selected);
    upload(count);
    upload(ordered);
    run_gpu(select(input, selected, count));
    run_gpu(order(selected, count, ordered));
    download(ordered);
    download(count);
    run_cpu(summarize(ordered, count, offsets, total));

    if (!println(count[0].value) || !println(total[0].value)) {
        return 1;
    }
    return 0;
}
