module material.textured;

float3 textured_ggx(read texture2d albedo, float2 uv, f32 lod, float3 normal, float3 view, float3 light, float3 tint, f32 roughness, f32 ior) on compute {
    float4 sample = texture_sample(albedo, uv, lod);
    return ggx_surface(normal, view, light, sample.xyz * tint, roughness, ior);
}

void shade(inout dataset<BLShadingPoint> points, read texture2d albedo) on compute {
    parallel (point in points) {
        point.radiance = textured_ggx(albedo, point.uv, point.lod, point.normal,
            point.view, point.light, point.tint, point.roughness, point.ior);
    }
}

float3 indexed_ggx<const N: u32>(read array<texture2d,N> albedos, u32 material, BLShadingPoint point) on compute requires index(material,albedos) {
    return textured_ggx(albedos[material], point.uv, point.lod, point.normal,
        point.view, point.light, point.tint, point.roughness, point.ior);
}
