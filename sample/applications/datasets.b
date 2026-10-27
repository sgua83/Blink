module applications.datasets;
import lessons.pipeline;

dataset<Item> create_input(dataset<Item> storage, u32 length) on cpu {
    if (storage.length != length) {
        storage = dataset<Item>(length);
    }
    target(cpu, seed(storage));
    return storage;
}

u32 main() on cpu {
    u32 size = 1000000;
    dataset<Item> input;
    input = create_input(dataset<Item>(0), size);
    dataset<Item> selected = dataset<Item>(size);
    dataset<Item> ordered = dataset<Item>(size);
    dataset<Count, 1> count = dataset<Count>(1);
    dataset<Offset> offsets = dataset<Offset>(size);
    dataset<Count, 1> total = dataset<Count>(1);

    upload(input);
    upload(selected);
    upload(count);
    upload(ordered);
    target(gpu, select(input, selected, count));
    target(gpu, order(selected, count, ordered));
    download(ordered);
    download(count);
    target(cpu, summarize(ordered, count, offsets, total));

    if (!println(count[0].value) || !println(total[0].value)) {
        return 1;
    }
    return 0;
}
