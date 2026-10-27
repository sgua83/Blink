module book.sheet;
import material.textured;
import render.resolve;
import render.trace;

void prepare<const W:u32,const H:u32>(write dataset<BLShadingPoint> points) on compute;
void materials<const W:u32,const H:u32>(inout dataset<BLShadingPoint> points, read texture2d albedo) on compute;
void image(inout dataset<BLShadingPoint,1000000> points,
    inout dataset<BLPixel,1000000> pixels,
    write dataset<BLDisplayPixel,1000000> display,
    read texture2d albedo) on gpu;
