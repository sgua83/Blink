module applications.pipeline_gpu;

struct Item { u32 id; u32 key; u32 units; bool keep; };
struct Count { u32 value; };
struct Offset { u32 value; };

void select_sort_and_sum(read dataset<Item> input, inout dataset<Item> selected,
                         inout dataset<Item> ordered, inout dataset<Count, 1> count,
                         write dataset<Offset> offsets, write dataset<Count, 1> total) on compute
    ensures prefix(ordered, count.value)
    ensures prefix(offsets, count.value) {
    compact(input, input.keep, selected, count.value);
    stable_sort_by_key(prefix(selected, count.value).key, ordered);
    scan_exclusive_sum(prefix(ordered, count.value).units, offsets.value);
    reduce_sum(prefix(ordered, count.value).units, total.value);
}

u32 main() on cpu {
    dataset<Item> input = dataset<Item>(8);
    dataset<Item> selected = dataset<Item>(8);
    dataset<Item> ordered = dataset<Item>(8);
    dataset<Count, 1> count = dataset<Count>(1);
    dataset<Offset> offsets = dataset<Offset>(8);
    dataset<Count, 1> total = dataset<Count>(1);

    parallel (item in input) {
        item.id = index();
        item.key = (index() * 7 + 2) % 3;
        item.units = index() + 1;
        item.keep = index() % 2 == 0;
    }

    upload(input);
    upload(selected);
    upload(ordered);
    upload(count);
    upload(offsets);
    upload(total);
    target(gpu, select_sort_and_sum(input, selected, ordered, count, offsets, total));
    download(count);
    download(total);

    if (!println(count[0].value) || !println(total[0].value)) { return 1; }
    return 0;
}
