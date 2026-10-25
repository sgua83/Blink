module scene.hierarchy;
struct BLHierarchyNode {float3x4 local;u32 parent;bool has_parent;bool active;};
struct BLWorldTransform {float3x4 matrix;};
struct BLSubtree {bool member;};
float3x4 compose_affine(float3x4 parent,float3x4 child) on compute;
program resolve(read dataset<BLHierarchyNode> nodes,write dataset<BLWorldTransform> worlds) on compute;
program descendants(read dataset<BLHierarchyNode> nodes,write dataset<BLSubtree> members,u32 root) on compute;
