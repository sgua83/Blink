module external.start;

struct Pair { u32 left; u32 right; };

extern "C" Pair add(Pair value, u32 amount) on cpu
    from "blink_cpp.dll" as "add_values";
extern "C" void scale(inout Pair value, u32 factor) on cpu
    from "blink_rust.dll" as "scale_values";

u32 main() on cpu;
