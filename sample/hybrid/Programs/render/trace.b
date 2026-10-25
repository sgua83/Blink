module render.trace;

float3 surface_normal(BLInstance instance, u32 primitive, float2 barycentric) on compute {
    BLTriangle triangle = unsafe_read(instance.triangles, primitive);
    float3 n0 = unsafe_read(instance.normals, triangle.normal_indices[0]).normal;
    float3 n1 = unsafe_read(instance.normals, triangle.normal_indices[1]).normal;
    float3 n2 = unsafe_read(instance.normals, triangle.normal_indices[2]).normal;
    f32 u = barycentric.x;
    f32 v = barycentric.y;
    return normalize(n0 * (1.0 - u - v) + n1 * u + n2 * v);
}

float2 surface_uv(BLInstance instance, u32 primitive, float2 barycentric) on compute {
    BLTextureTriangle triangle = unsafe_read(instance.texcoords, primitive);
    f32 u = barycentric.x;
    f32 v = barycentric.y;
    return triangle.coordinates[0] * (1.0-u-v) + triangle.coordinates[1] * u + triangle.coordinates[2] * v;
}

float4 shade<const N:u32>(
read dataset<BLInstance> instances, read dataset<BLSurface> surfaces,
read dataset<BLNormalMatrix> normal_matrices, read dataset<BLLight> lights, BLLightingSettings lighting, read acceleration scene,
read array<texture2d,N> albedos, float3 origin, float3 direction, bool floor_present, f32 ground_z, f32 near, f32 far, u32 ground, u32 base, u32 seed
) on gpu {
    f32 nearest=far;
    bool floor_hit=false;
    if (floor_present && direction.z!=0.0) {
        f32 distance=(ground_z-origin.z)/direction.z;
        if (distance>=near && distance<=far) { nearest=distance; floor_hit=true; }
    }
    float3 normal=float3(0.0,0.0,1.0);
    bool geometry_hit=false;
    float3 tint=float3(1.0);
    u32 material=ground;
    RayHit hit=trace_closest(scene,origin,near,direction,far);
    u32 instance_index=hit.instance+base;
    if (hit.exists && (!floor_hit || hit.distance<nearest)) {
        BLInstance instance=unsafe_read(instances,instance_index);
        normal=surface_normal(instance,hit.primitive,hit.barycentric);
        if (normal_matrices.length!=0) { normal=normalize(unsafe_read(normal_matrices,instance_index).basis*normal); }
        nearest=hit.distance;
        geometry_hit=true;
        tint=instance.tint.xyz;
        u32 group=unsafe_read(instance.triangles,hit.primitive).material;
        material=unsafe_read(instance.materials,group).surface;
    }
    if (!floor_hit && !geometry_hit) { return float4(lighting.background,0.0); }
    float3 position=origin+direction*nearest;
    BLSurface surface=unsafe_read(surfaces,material);
    float2 uv=float2(0.0);
    if (surface.uses_uv) {
        uv=position.xy;
        if (geometry_hit) { uv=surface_uv(unsafe_read(instances,instance_index),hit.primitive,hit.barycentric); }
    }
    uv=uv*surface.uv_scale+surface.uv_offset;
    float3 color=texture_sample(unsafe_read(albedos,surface.texture),uv,surface.lod).xyz*tint;
    float3 direct=direct_lighting(lights,scene,lighting,position,normal,-direction,color,surface.roughness,surface.ior,
        floor_present,ground_z,seed);
    f32 coverage=0.0;
    if (geometry_hit) { coverage=1.0; }
    return float4(direct+color*lighting.fill,coverage);
}

bool light_occluded(read acceleration scene, BLLightSample sample, BLLightingSettings settings,
    float3 position, float3 normal, bool floor_present, f32 ground_z) on gpu {
    float3 origin=position+normal*settings.offset;
    float3 direction=sample.direction;
    f32 far=settings.far;
    if (sample.local) {
        float3 delta=sample.target-origin;
        f32 distance_squared=dot(delta,delta);
        // 本地光源连接到同一点时，遮挡区间为空。
        if (distance_squared==0.0) { return false; }
        f32 distance=sqrt(distance_squared);
        direction=delta/distance;
        far=min(far,distance);
    }
    if (far<settings.near) { return false; }
    if (floor_present && direction.z!=0.0) {
        f32 distance=(ground_z-origin.z)/direction.z;
        if (distance>=settings.near && distance<=far) { return true; }
    }
    return trace_any(scene,origin,settings.near,direction,far);
}
float3 direct_lighting(read dataset<BLLight> lights, read acceleration scene, BLLightingSettings settings,
    float3 position, float3 normal, float3 view, float3 color, f32 roughness, f32 ior,
    bool floor_present, f32 ground_z, u32 seed) on gpu {
    float3 result=float3(0.0);
    u32 light_index=0;
    for (light in lights) {
        u32 sample=0;
        while (sample<light.samples) {
            BLLightSample incoming=light_sample(light,position,normal,sample,light_hash(seed^light_index));
            if (incoming.active) {
                if (!light_occluded(scene,incoming,settings,position,normal,floor_present,ground_z)) {
                    result+=ggx_surface(normal,view,incoming.direction,color,roughness,ior)*incoming.weight/f32(light.samples);
                }
            }
            sample+=1;
        }
        light_index+=1;
    }
    return result;
}

program trace<const N:u32,const W:u32,const H:u32,const S:u32>(
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
) on gpu {
    parallel (pixel in pixels) {
        f32 sx=((f32(index()%(W*S))+0.5)/f32(W*S)*2.0-1.0)*(f32(W)/f32(H));
        f32 sy=1.0-(f32(index()/(W*S))+0.5)/f32(H*S)*2.0;
        BLCamera camera=unsafe_read(cameras,0);
        BLRay ray=camera_ray(camera,float2(sx,sy));
        float4 result=shade(instances,surfaces,normal_matrices,lights,lighting[0],scene,albedos,ray.origin,ray.direction,
            floor_present,ground_z,camera.near,camera.far,ground,0,index());
        bool geometry_hit=result.w!=0.0;
        if (cameras.length==2) {
            BLCamera overlay_camera=unsafe_read(cameras,1);
            BLRay overlay_ray=camera_ray(overlay_camera,float2(sx,sy));
            float4 chrome=shade(instances,surfaces,normal_matrices,lights,lighting[0],overlay_scene,albedos,
                overlay_ray.origin,overlay_ray.direction,false,ground_z,overlay_camera.near,overlay_camera.far,ground,overlay_start,index());
            if (chrome.w!=0.0) { result=chrome; }
            geometry_hit=chrome.w!=0.0;
        }
        float3 radiance=result.xyz;
        f32 px=(f32(index()%(W*S))+0.5)/f32(S);
        f32 py=(f32(index()/(W*S))+0.5)/f32(S);
        for (overlay in overlays) {
            if ((overlay.background==0 || !geometry_hit) && px>=overlay.rectangle.x && py>=overlay.rectangle.y && px<overlay.rectangle.x+overlay.rectangle.z && py<overlay.rectangle.y+overlay.rectangle.w) {
                radiance=radiance*(1.0-overlay.color.w)+overlay.color.xyz*overlay.color.w;
            }
        }
        pixel.radiance=radiance;
    }
}
