module book.sheet;
program prepare<const W:u32,const H:u32>(write dataset<BLShadingPoint> points) on compute {
    parallel (point in points) {
        f32 x=(f32(index()%W)+0.5)/f32(W);
        f32 y=(f32(index()/W)+0.5)/f32(H);
        point.normal=normalize(float3((x-0.5)*1.4,(y-0.5)*1.4,1.0));
        point.view=float3(0.0,0.0,1.0);
        point.light=normalize(float3(-0.4,-0.3,1.0));
        point.uv=float2(x,y);
        point.lod=0.0;
        point.tint=float3(1.0);
        point.roughness=0.08+0.84*x;
        point.ior=1.5;
        point.radiance=float3(0.0);
    }
}
program materials<const W:u32,const H:u32>(inout dataset<BLShadingPoint> points, read texture2d albedo) on compute {
    prepare<W,H>(points);
    material.textured::shade(points,albedo);
}
program pack(read dataset<BLShadingPoint,1000000> points,
    write dataset<BLPixel,1000000> pixels) on compute {
    parallel (pixel in pixels) {
        pixel.radiance=points[index()].radiance;
    }
}
program image(inout dataset<BLShadingPoint,1000000> points,
    inout dataset<BLPixel,1000000> pixels,
    write dataset<BLDisplayPixel,1000000> display,
    read texture2d albedo) on gpu {
    materials<1000,1000>(points,albedo);
    pack(points,pixels);
    resolve<1000,1000,1,1000000,1000000>(pixels,display);
}
