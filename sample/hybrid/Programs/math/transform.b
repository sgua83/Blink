module math.transform;

T identity<T: BLMatrix>() { return T(1.0); }
T compose<T: BLSquareMatrix>(T first, T second) { return second * first; }
T scaled<T: BLMatrix>(T value, f32 factor) { return value * factor; }

float3 transform_point(float4x4 world_from_local, float3 position) {
    float4 projected = world_from_local * float4(position, 1.0);
    return projected.xyz / projected.w;
}

float3 transform_plane(float3x2 basis, float2 coordinate) {
    return basis * coordinate;
}

float3 transform_direction(float3x3 basis, float3 direction) {
    return basis * direction;
}

float3 transform_normal(float3x3 world_from_local, float3 normal) {
    return normalize(transpose(inverse(world_from_local)) * normal);
}
