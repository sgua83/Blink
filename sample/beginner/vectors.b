module beginner.vectors;

u32 main() on cpu {
    float2 position = float2(1.0, 2.0);
    float2 speed = float2(0.0, 2.0);
    float2 moved = position + speed * 0.5;
    println(moved.x == 1.0);
    println(moved.y == 3.0);
    float2x2 turn = float2x2(0.0, -1.0, 1.0, 0.0);
    float2 rotated = turn * position;
    println(rotated.x == -2.0);
    println(rotated.y == 1.0);
    return 0;
}
