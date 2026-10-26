module beginner.vectors;

program move(write dataset<float2> points) on compute {
    parallel (point in points) {
        float2 position = float2(1.0, 2.0);
        float2 speed = float2(0.0, 2.0);
        point = position + speed * 0.5;
    }
}

program rotate(write dataset<float2> points) on compute {
    parallel (point in points) {
        float2x2 turn = float2x2(0.0, -1.0, 1.0, 0.0);
        point = turn * float2(1.0, 2.0);
    }
}
