module lessons.tiles;

struct Cell { f32 value; f32 total; f32 before; f32 centered; };

cooperative f32 center(tile<f32> values, f32 value) on compute;
program seed(write dataset<Cell> cells) on compute;
program analyze<const Width: u32>(inout dataset<Cell> cells) on compute;
program publish<const Width: u32>(inout dataset<Cell> cells, f32 factor) on compute;
program demo<const Width: u32>(inout dataset<Cell> cells) on compute;
program demo_publish<const Width: u32>(inout dataset<Cell> cells, f32 factor) on compute;
