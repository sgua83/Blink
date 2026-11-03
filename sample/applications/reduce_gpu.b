module applications.reduce_gpu;
import lessons.scan;

u32 main() on cpu {
    dataset<Count> input = dataset<Count>(4);
    dataset<Count, 1> total = dataset<Count>(1);

    upload(input);
    upload(total);
    target(gpu, demo_sum(input, total));
    download(input);
    download(total);

    if (!println(total[0].value)) { return 1; }
    return 0;
}
