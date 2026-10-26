module beginner.generic;

T twice<T: integer>(T value) on compute {
    return value * 2;
}

u32 main() on cpu {
    println(twice(3));
    println(twice(-2));
    return 0;
}
