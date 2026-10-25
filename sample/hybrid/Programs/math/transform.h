module math.transform;

typeset BLMatrix {
    float2x2; float2x3; float2x4;
    float3x2; float3x3; float3x4;
    float4x2; float4x3; float4x4;
};
typeset BLSquareMatrix { float2x2; float3x3; float4x4; };

T identity<T: BLMatrix>();
T compose<T: BLSquareMatrix>(T first, T second);
T scaled<T: BLMatrix>(T value, f32 factor);
float3 transform_point(float4x4 world_from_local, float3 position);
float3 transform_plane(float3x2 basis, float2 coordinate);
float3 transform_direction(float3x3 basis, float3 direction);
float3 transform_normal(float3x3 world_from_local, float3 normal);
