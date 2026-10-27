module render.resolve;
import render.trace;
struct BLDisplayPixel { u32 bgra; };
void resolve<const W:u32,const H:u32,const S:u32,const DC:u32,const IC:u32>(
    read dataset<BLPixel,IC> pixels, write dataset<BLDisplayPixel,DC> display) on gpu;
