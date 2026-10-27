module interop.fractal;

const u32 Width = 1000;
const u32 Height = 1000;
const u32 Count = Width * Height;

void render(write dataset<u32, Count> pixels, u32 limit) on compute;
