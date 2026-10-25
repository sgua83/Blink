module scene.camera;
import math.transform;
struct BLCamera {
    float3 eye; float3 right; float3 up; float3 forward;
    f32 tangent; u32 projection; f32 near; f32 far;
};
struct BLRay { float3 origin; float3 direction; };
BLCamera camera_basis(float3 eye, float3 target, float3 up, f32 span, u32 projection, f32 near, f32 far) on compute;
BLRay camera_ray(BLCamera camera, float2 ndc) on compute;
program look_at(write dataset<BLCamera,1> cameras,
    float3 eye, float3 target, float3 up, f32 span, u32 projection, f32 near, f32 far) on compute;
program orbit(write dataset<BLCamera,1> cameras,
    float3 eye, float3 target, float3 up, f32 span, u32 projection, f32 near, f32 far,
    f32 yaw, f32 pitch, f32 distance) on compute;
