#include "map.abi.h"
#include <cstdio>

int main() try {
    float values[] = {1, 2, 3, 4};
    blink_map::Program program(blink_map::Backend::Cpu);
    program.run(values, 4);
    for (float value : values) std::printf("%.0f ", value);
    std::puts("");
    return 0;
} catch (const std::exception& e) {
    std::fprintf(stderr, "%s\n", e.what());
    return 1;
}
