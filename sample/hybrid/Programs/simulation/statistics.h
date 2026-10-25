module simulation.statistics;
import simulation.bodies;

struct BLTotal {
    f32 value;
};

program total_energy(read dataset<BLBody<float3, 3>> bodies, write dataset<BLTotal, 1> total) on compute;
program step_and_total(inout dataset<BLBody<float3, 3>> bodies, write dataset<BLTotal, 1> total, f32 dt, f32 strength) on compute;
