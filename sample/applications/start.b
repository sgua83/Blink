module applications.start;

u32 main() on cpu {
    if (!println(1 + 2)) {
        return 1;
    }
    return 0;
}
