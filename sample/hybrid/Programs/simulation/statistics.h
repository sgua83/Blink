module simulation.statistics;
import simulation.bodies;

struct BLTotal {
    f32 value;
};

void total_energy(read dataset<BLBody<float3, 3>> bodies, write dataset<BLTotal, 1> total) on compute;
void step_and_total(inout dataset<BLBody<float3, 3>> bodies, write dataset<BLTotal, 1> total, f32 dt, f32 strength) on compute;
