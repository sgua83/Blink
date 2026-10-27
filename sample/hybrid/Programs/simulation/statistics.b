module simulation.statistics;

void total_energy(read dataset<BLBody<float3, 3>> bodies, write dataset<BLTotal, 1> total) on compute {
    reduce_sum(bodies.energy, total.value);
}

void step_and_total(inout dataset<BLBody<float3, 3>> bodies, write dataset<BLTotal, 1> total, f32 dt, f32 strength) on compute {
    step(bodies, dt, strength);
    total_energy(bodies, total);
}
