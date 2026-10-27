module lessons.tiles;

struct Cell { f32 value; f32 total; f32 before; f32 centered; };

cooperative f32 center(tile<f32> values, f32 value) on compute;
void seed(write dataset<Cell> cells) on compute;
void analyze<const Width: u32>(inout dataset<Cell> cells) on compute;
void publish<const Width: u32>(inout dataset<Cell> cells, f32 factor) on compute;
void demo<const Width: u32>(inout dataset<Cell> cells) on compute;
void demo_publish<const Width: u32>(inout dataset<Cell> cells, f32 factor) on compute;
