module beginner.functions;

u32 add_bonus(u32 score, u32 bonus) on compute;
u32 level_of(u32 score) on compute;
program run(write dataset<u32> levels) on compute;
