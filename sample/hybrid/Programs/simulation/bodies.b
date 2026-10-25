module simulation.bodies;

void integrate_body<T: floating, const Power: u32 in {1, 2, 3}>(inout BLBody<T, Power> body, f32 dt, f32 strength) on compute {
    body.velocity += central_acceleration<Power>(body.position, strength) * dt;
    body.position += body.velocity * dt;
}

void measure_energy<T: floating>(write f32 target, T velocity) on compute {
    target = 0.5 * dot(velocity, velocity);
}

program integrate<B: BLSimulatedBody>(inout dataset<B> bodies, f32 dt, f32 strength) on compute {
    parallel (body in bodies) {
        integrate_body(body, dt, strength);
    }
}

program energy<B: BLSimulatedBody>(inout dataset<B> bodies) on compute {
    parallel (body in bodies) {
        measure_energy(body.energy, body.velocity);
    }
}

program step<B: BLSimulatedBody>(inout dataset<B> bodies, f32 dt, f32 strength) on compute {
    integrate(bodies, dt, strength);
    energy(bodies);
}
