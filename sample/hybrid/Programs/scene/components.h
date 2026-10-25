module scene.components;
import scene.hierarchy;
import lighting.lights;
struct BLSceneCamera {float3 eye;float3 target;float3 up;f32 span;u32 projection;f32 near;f32 far;};
struct BLAttachedCamera {u32 node;BLSceneCamera camera;};
struct BLAttachedLight {u32 node;BLLight light;};
program cameras(read dataset<BLWorldTransform> worlds,read dataset<BLAttachedCamera> locals,
                write dataset<BLSceneCamera> output) on compute;
program lights(read dataset<BLWorldTransform> worlds,read dataset<BLAttachedLight> locals,
               write dataset<BLLight> output) on compute;
program evaluate(read dataset<BLHierarchyNode> nodes,inout dataset<BLWorldTransform> worlds,
                 read dataset<BLAttachedCamera> local_cameras,write dataset<BLSceneCamera> world_cameras,
                 read dataset<BLAttachedLight> local_lights,write dataset<BLLight> world_lights) on compute;
