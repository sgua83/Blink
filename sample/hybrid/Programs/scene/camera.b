module scene.camera;

BLCamera camera_basis(float3 eye, float3 target, float3 up, f32 span, u32 projection, f32 near, f32 far) on compute {
    float3 forward=normalize(target-eye);
    float3 right=normalize(cross(forward,up));
    return BLCamera(eye,right,cross(right,forward),forward,span,projection,near,far);
}
BLRay camera_ray(BLCamera camera, float2 ndc) on compute {
    float3x2 basis=float3x2(camera.right.x,camera.up.x,camera.right.y,camera.up.y,camera.right.z,camera.up.z);
    float3 offset=transform_plane(basis,ndc*camera.tangent);
    if (camera.projection==1) { return BLRay(camera.eye+offset,camera.forward); }
    return BLRay(camera.eye,normalize(camera.forward+offset));
}
program look_at(write dataset<BLCamera,1> cameras,
    float3 eye, float3 target, float3 up, f32 span, u32 projection, f32 near, f32 far) on compute {
    parallel (camera in cameras) { camera=camera_basis(eye,target,up,span,projection,near,far); }
}
program orbit(write dataset<BLCamera,1> cameras,
    float3 eye, float3 target, float3 up, f32 span, u32 projection, f32 near, f32 far,
    f32 yaw, f32 pitch, f32 distance) on compute {
    parallel (camera in cameras) {
        BLCamera initial=camera_basis(eye,target,up,span,projection,near,far);
        float3 right=initial.right*cos(yaw)+initial.forward*sin(yaw);
        float3 back=initial.right*sin(yaw)-initial.forward*cos(yaw);
        float3 offset=back*cos(pitch)+initial.up*sin(pitch);
        f32 radius=sqrt(dot(eye-target,eye-target))*distance;
        camera=BLCamera(target+offset*radius,right,initial.up*cos(pitch)-back*sin(pitch),-offset,span,projection,near,far);
    }
}
