module render.frame;
import render.trace;
import render.resolve;
import material.palette;
import scene.transforms;
import scene.camera;
import lighting.lights;

void frame<const N:u32,const W:u32,const H:u32,const S:u32,const DC:u32,const IC:u32>(
inout dataset<BLPixel,IC> pixels,
write dataset<BLDisplayPixel,DC> display,
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
