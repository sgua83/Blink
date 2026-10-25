module scene.transforms;
struct BLAffine { float3x4 matrix; };
struct BLNormalMatrix { float3x3 basis; };
float3x3 normal_basis(float3x4 matrix) on compute;
program prepare(read dataset<BLAffine> transforms, write dataset<BLNormalMatrix> normals) on compute;
