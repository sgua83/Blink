module lighting.lights;

struct BLLight {
    u32 kind; float3 position; float3 direction; float3 edge_u; float3 edge_v;
    float3 energy; f32 inner; f32 outer; u32 samples;
};
struct BLLightingSettings {
    float3 background; float3 fill; f32 offset; f32 near; f32 far;
};
struct BLLightSample {
    float3 direction; float3 weight; float3 target; bool local; bool active;
};
u32 light_hash(u32 value) on compute;
BLLightSample light_sample(BLLight light, float3 position, float3 normal, u32 sample, u32 seed) on compute;
