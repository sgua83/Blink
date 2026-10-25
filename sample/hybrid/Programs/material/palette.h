module material.palette;

struct BLSurface {
    u32 texture;
    f32 roughness;
    f32 ior;
    f32 lod;
    float2 uv_scale;
    float2 uv_offset;
    bool uses_uv;
};
