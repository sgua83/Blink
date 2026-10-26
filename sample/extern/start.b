module external.start;

u32 main() on cpu {
    Pair value = add(Pair(3, 5), 2);
    scale(value, 3);
    if (!println(value.left) || !println(value.right)) {
        return 1;
    }
    return 0;
}
