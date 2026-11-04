module applications.datasets;

struct Item { u32 id; u32 key; u32 units; bool keep; };
struct Count { u32 value; };
struct Offset { u32 value; };

void select(read dataset<Item> input, write dataset<Item> selected,
               write dataset<Count, 1> count) on compute
    ensures prefix(selected, count.value) {
    compact(input, input.keep, selected, count.value);
}

void order(read dataset<Item> selected, read dataset<Count, 1> count,
              write dataset<Item> ordered) on compute
    requires prefix(selected, count.value)
    ensures prefix(ordered, count.value) {
    stable_sort_by_key(prefix(selected, count.value).key, ordered);
}

void summarize(read dataset<Item> ordered, read dataset<Count, 1> count,
                  write dataset<Offset> offsets,
                  write dataset<Count, 1> total) on compute
    requires prefix(ordered, count.value)
    ensures prefix(offsets, count.value) {
    scan_exclusive_sum(prefix(ordered, count.value).units, offsets.value);
    reduce_sum(prefix(ordered, count.value).units, total.value);
}

dataset<Item> create_input(dataset<Item> storage, u32 length) on cpu {
    if (storage.length != length) {
        storage = dataset<Item>(length);
    }
    parallel (item in storage) {
        item.id = index();
        item.key = (index() * 7 + 2) % 3;
        item.units = index() + 1;
        item.keep = index() % 2 == 0;
    }
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
    summarize(ordered, count, offsets, total);

    if (!println(count[0].value) || !println(total[0].value)) {
        return 1;
    }
    return 0;
}
