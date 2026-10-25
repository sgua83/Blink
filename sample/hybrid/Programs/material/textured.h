module material.textured;
import material.surface;

struct BLShadingPoint {
    float3 normal;
    float3 view;
    float3 light;
    float2 uv;
    f32 lod;
    float3 tint;
    f32 roughness;
    f32 ior;
    float3 radiance;
};

float3 textured_ggx(read texture2d albedo, float2 uv, f32 lod, float3 normal, float3 view, float3 light, float3 tint, f32 roughness, f32 ior) on compute;
float3 indexed_ggx<const N: u32>(read array<texture2d,N> albedos, u32 material, BLShadingPoint point) on compute requires index(material,albedos);
program shade(inout dataset<BLShadingPoint> points, read texture2d albedo) on compute;
