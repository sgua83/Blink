module simulation.bodies;
import math.field;

struct BLBody<T: floating, const Power: u32 in {1, 2, 3}> {
    T position;
    T velocity;
    f32 energy;
};

typeset BLSimulatedBody {
    BLBody<f32, 1>;
    BLBody<f32, 2>;
    BLBody<f32, 3>;
    BLBody<float2, 1>;
    BLBody<float2, 2>;
    BLBody<float2, 3>;
    BLBody<float3, 1>;
    BLBody<float3, 2>;
    BLBody<float3, 3>;
    BLBody<float4, 1>;
    BLBody<float4, 2>;
    BLBody<float4, 3>;
};

void integrate_body<T: floating, const Power: u32 in {1, 2, 3}>(inout BLBody<T, Power> body, f32 dt, f32 strength) on compute;
void measure_energy<T: floating>(write f32 target, T velocity) on compute;

void integrate<B: BLSimulatedBody>(inout dataset<B> bodies, f32 dt, f32 strength) on compute;
void energy<B: BLSimulatedBody>(inout dataset<B> bodies) on compute;
void step<B: BLSimulatedBody>(inout dataset<B> bodies, f32 dt, f32 strength) on compute;
