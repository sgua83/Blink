module applications.pipeline_gpu;
import lessons.pipeline;

u32 main() on cpu {
    dataset<Item> input = dataset<Item>(8);
    dataset<Item> selected = dataset<Item>(8);
    dataset<Item> ordered = dataset<Item>(8);
    dataset<Count, 1> count = dataset<Count>(1);
    dataset<Offset> offsets = dataset<Offset>(8);
    dataset<Count, 1> total = dataset<Count>(1);

    upload(input);
    upload(selected);
    upload(ordered);
    upload(count);
    upload(offsets);
    upload(total);
    target(gpu, demo(input, selected, ordered, count, offsets, total));
    download(input);
    download(selected);
    download(ordered);
    download(count);
    download(offsets);
    download(total);

    if (!println(count[0].value) || !println(total[0].value)) { return 1; }
    return 0;
}
