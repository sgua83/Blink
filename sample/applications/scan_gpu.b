module applications.scan_gpu;
import lessons.scan;

u32 main() on cpu {
    dataset<Count> input = dataset<Count>(4);
    dataset<Count> output = dataset<Count>(4);

    upload(input);
    upload(output);
    target(gpu, demo(input, output));
    download(input);
    download(output);

    for (row in output) { if (!println(row.value)) { return 1; } }
    return 0;
}
