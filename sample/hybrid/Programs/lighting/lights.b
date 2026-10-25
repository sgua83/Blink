module lighting.lights;

u32 light_hash(u32 value) on compute {
    u32 word=value;
    word=value^(word>>16);
    word*=0x7feb352d;
    word=word^(word>>15);
    word*=0x846ca68b;
    return word^(word>>16);
}
BLLightSample light_sample(BLLight light, float3 position, float3 normal, u32 sample, u32 seed) on compute {
    if (light.kind==0) {
        float3 direction=-normalize(light.direction);
        return BLLightSample(direction,light.energy,float3(0.0),false,dot(normal,direction)>0.0);
    }
    float3 target=light.position;
    if (light.kind==3) {
        f32 u=(f32(sample)+0.5)/f32(light.samples);
        f32 v=f32(light_hash(seed^sample)&0x00ffffff)/16777216.0;
        target+=(u*2.0-1.0)*light.edge_u+(v*2.0-1.0)*light.edge_v;
    }
    float3 delta=target-position;
    // 单面 BSDF 的背面与切向没有入射贡献；同时包含零长度连接的零测度情形。
    if (dot(normal,delta)<=0.0) { return BLLightSample(float3(0.0),float3(0.0),target,true,false); }
    f32 distance_squared=dot(delta,delta);
    float3 direction=delta/sqrt(distance_squared);
    f32 factor=1.0/distance_squared;
    if (light.kind==2) {
        f32 cosine=dot(normalize(light.direction),-direction);
        f32 inner=cos(light.inner);
        f32 outer=cos(light.outer);
        if (cosine<=outer) { return BLLightSample(direction,float3(0.0),target,true,false); }
        if (cosine<inner) {
            f32 t=(cosine-outer)/(inner-outer);
            factor*=t*t*(3.0-2.0*t);
        }
    }
    if (light.kind==3) {
        // edge_u/edge_v 是半边；面积乘出射余弦可直接由未归一化法线得到。
        f32 projected_area=4.0*dot(cross(light.edge_u,light.edge_v),-direction);
        if (projected_area<=0.0) { return BLLightSample(direction,float3(0.0),target,true,false); }
        factor*=projected_area;
    }
    return BLLightSample(direction,light.energy*factor,target,true,true);
}
