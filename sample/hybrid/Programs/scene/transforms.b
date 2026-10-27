module scene.transforms;
float3x3 normal_basis(float3x4 matrix) on compute {
    float3x3 linear = float3x3(matrix.r0.xyz, matrix.r1.xyz, matrix.r2.xyz);
    return transpose(inverse(linear));
}
void prepare(read dataset<BLAffine> transforms, write dataset<BLNormalMatrix> normals) on compute {
    parallel (normal in normals) {
        BLAffine transform = unsafe_read(transforms, index());
        normal.basis = normal_basis(transform.matrix);
    }
}
