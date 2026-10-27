module interop.fractal;

void render(write dataset<u32, Count> pixels, u32 limit) on compute {
    parallel (pixel in pixels) {
        u32 i = index();
        f32 cx = f32(i % Width) / f32(Width) * 3.5 - 2.5;
        f32 cy = f32(i / Width) / f32(Height) * 2.0 - 1.0;
        f32 x = 0.0;
        f32 y = 0.0;
        u32 iteration = 0;
        while (x * x + y * y <= 4.0 && iteration < limit) {
            f32 next_x = x * x - y * y + cx;
            y = 2.0 * x * y + cy;
            x = next_x;
            iteration += 1;
        }
        pixel = iteration;
    }
}
