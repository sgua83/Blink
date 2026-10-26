module applications.floating;

u32 main() on cpu {
    f32 ratio = 1.0 / 3.0;
    if (!println(ratio) || !println(sqrt(2.0)) || !println(-0.0)) {
        return 1;
    }
    return 0;
}
