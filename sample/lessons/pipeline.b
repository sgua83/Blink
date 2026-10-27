module lessons.pipeline;

void seed(write dataset<Item> input) on compute {
    parallel (item in input) {
        item.id = index();
        item.key = (index() * 7 + 2) % 3;
        item.units = index() + 1;
        item.keep = index() % 2 == 0;
    }
}

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

void run(read dataset<Item> input, inout dataset<Item> selected,
            inout dataset<Item> ordered, inout dataset<Count, 1> count,
            write dataset<Offset> offsets, write dataset<Count, 1> total) on compute
    ensures prefix(ordered, count.value)
    ensures prefix(offsets, count.value) {
    select(input, selected, count);
    order(selected, count, ordered);
    summarize(ordered, count, offsets, total);
}

void demo(inout dataset<Item> input, inout dataset<Item> selected,
             inout dataset<Item> ordered, inout dataset<Count, 1> count,
             write dataset<Offset> offsets, write dataset<Count, 1> total) on compute
    ensures prefix(ordered, count.value)
    ensures prefix(offsets, count.value) {
    seed(input);
    run(input, selected, ordered, count, offsets, total);
}
