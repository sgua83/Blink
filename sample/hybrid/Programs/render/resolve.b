module render.resolve;
void resolve<const W:u32,const H:u32,const S:u32,const DC:u32,const IC:u32>(
    read dataset<BLPixel,IC> pixels, write dataset<BLDisplayPixel,DC> display) on gpu {
    parallel (pixel in display) {
        u32 internal_width=W*S;
        u32 sample_base=(index()/W)*(internal_width*S)+(index()%W)*S;
        float3 radiance=float3(0.0);
        u32 y=0;
        while (y<S) {
            u32 x=0;
            while (x<S) {
                radiance+=unsafe_read(pixels,sample_base+y*internal_width+x).radiance;
                x+=1;
            }
            y+=1;
        }
        radiance*=1.0/f32(S*S);
        float3 mapped=pow(max(radiance/(radiance+1.0),float3(0.0)),0.454545454545);
        u32 red=u32(clamp(mapped.r*255.0+0.5,0.0,255.0));
        u32 green=u32(clamp(mapped.g*255.0+0.5,0.0,255.0));
        u32 blue=u32(clamp(mapped.b*255.0+0.5,0.0,255.0));
        pixel.bgra=(red<<16)|(green<<8)|blue|0xff000000;
    }
}
