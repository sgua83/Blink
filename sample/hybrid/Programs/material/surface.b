module material.surface;

float3 ggx_surface(float3 normal, float3 view, float3 light, float3 base, f32 roughness, f32 ior) {
    f32 nl=dot(normal,light);
    f32 nv=dot(normal,view);
    if (nl<=0.0 || nv<=0.0) { return float3(0.0); }
    float3 half_vector=normalize(view+light);
    f32 nh=max(dot(normal,half_vector),0.0);
    f32 vh=max(dot(view,half_vector),0.0);
    f32 alpha=roughness*roughness;
    f32 a2=alpha*alpha;
    float3 tangent=cross(normal,half_vector);
    f32 denominator=dot(tangent,tangent)+a2*nh*nh;
    f32 distribution=a2/(3.14159265359*denominator*denominator);
    f32 gv=nl*sqrt(nv*nv*(1.0-a2)+a2);
    f32 gl=nv*sqrt(nl*nl*(1.0-a2)+a2);
    f32 f0=(ior-1.0)/(ior+1.0);
    f32 fresnel=f0*f0+(1.0-f0*f0)*pow(1.0-vh,5.0);
    f32 specular=distribution*fresnel*nl/(2.0*(gv+gl));
    return base*((1.0-fresnel)*nl/3.14159265359)+float3(specular);
}
