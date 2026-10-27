module render.trace;
import scene.camera;
import lighting.lights;
import material.surface;
import material.palette;
import math.transform;
import scene.transforms;

struct BLTriangle {
    array<u32,3> normal_indices;
    u32 material;
};

struct BLNormal {
    float3 normal;
};

struct BLTextureTriangle {
    array<float2,3> coordinates;
};

struct BLMaterialSlot {u32 surface;};
struct BLInstance {
    view<BLTriangle> triangles;
    view<BLNormal> normals;
    view<BLTextureTriangle> texcoords;
    view<BLMaterialSlot> materials;
    float4 tint;
};

struct BLOverlay {
    float4 rectangle;
    float4 color;
    u32 background;
};

struct BLPixel {
    float3 radiance;
};

float3 surface_normal(BLInstance instance, u32 primitive, float2 barycentric) on compute;
float2 surface_uv(BLInstance instance, u32 primitive, float2 barycentric) on compute;

float4 shade<const N:u32>(
read dataset<BLInstance> instances, read dataset<BLSurface> surfaces,
read dataset<BLNormalMatrix> normal_matrices, read dataset<BLLight> lights, BLLightingSettings lighting, read acceleration scene,
read array<texture2d,N> albedos, float3 origin, float3 direction, bool floor_present, f32 ground_z, f32 near, f32 far, u32 ground, u32 base, u32 seed
) on gpu;

bool light_occluded(read acceleration scene, BLLightSample sample, BLLightingSettings settings,
    float3 position, float3 normal, bool floor_present, f32 ground_z) on gpu;
float3 direct_lighting(read dataset<BLLight> lights, read acceleration scene, BLLightingSettings settings,
    float3 position, float3 normal, float3 view, float3 color, f32 roughness, f32 ior,
    bool floor_present, f32 ground_z, u32 seed) on gpu;

void trace<const N:u32,const W:u32,const H:u32,const S:u32>(
write dataset<BLPixel> pixels,
read dataset<BLInstance> instances,
read dataset<BLOverlay> overlays,
read dataset<BLSurface> surfaces,
read dataset<BLNormalMatrix> normal_matrices,
read dataset<BLCamera> cameras,
read dataset<BLLight> lights,
read dataset<BLLightingSettings,1> lighting,
read acceleration scene,
read acceleration overlay_scene,
read array<texture2d,N> albedos,
bool floor_present, f32 ground_z, u32 ground, u32 overlay_start
) on gpu;
