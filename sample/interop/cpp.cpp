#include "fractal.abi.h"
#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <fstream>
#include <numeric>
#include <vector>

using Clock = std::chrono::steady_clock;
static double milliseconds(Clock::time_point begin) {
    return std::chrono::duration<double, std::milli>(Clock::now() - begin).count();
}

static void serial(std::vector<uint32_t>& pixels, uint32_t limit) {
    const uint32_t Width = 1000, Height = 1000;
    for (uint32_t i = 0; i < pixels.size(); ++i) {
        float cx = float(i % Width) / float(Width) * 3.5f - 2.5f;
        float cy = float(i / Width) / float(Height) * 2.0f - 1.0f;
        float x = 0.0f;
        float y = 0.0f;
        uint32_t iteration = 0;
        while (x * x + y * y <= 4.0f && iteration < limit) {
            float next_x = x * x - y * y + cx;
            y = 2.0f * x * y + cy;
            x = next_x;
            ++iteration;
        }
        pixels[i] = iteration;
    }
}

int main(int argc, char** argv) try {
    const auto backend = argc > 1 && std::string(argv[1]) == "gpu"
        ? blink_fractal::Backend::Gpu : blink_fractal::Backend::Cpu;
    std::vector<uint32_t> expected(FRACTAL_PIXELS_COUNT), pixels(FRACTAL_PIXELS_COUNT);
    const uint32_t limit = 256;
    auto begin = Clock::now();
    blink_fractal::Program program(backend);
    const double setup_ms = milliseconds(begin);
    // 初始化与预热独立于一次调用的计时；各方法预热两次，取九次单次调用的中位数。
    std::vector<double> serial_samples, blink_samples;
    for (int sample = -2; sample < 9; ++sample) {
        begin = Clock::now();
        serial(expected, limit);
        const double serial_ms = milliseconds(begin);
        begin = Clock::now();
        program.run(pixels.data(), uint32_t(pixels.size()), &limit);
        const double blink_ms = milliseconds(begin);
        if (sample >= 0) {
            serial_samples.push_back(serial_ms);
            blink_samples.push_back(blink_ms);
        }
    }
    std::sort(serial_samples.begin(), serial_samples.end());
    std::sort(blink_samples.begin(), blink_samples.end());
    uint32_t different = 0, max_difference = 0;
    for (size_t i = 0; i < pixels.size(); ++i) {
        if (pixels[i] != expected[i]) ++different;
        max_difference = std::max(max_difference,
            pixels[i] > expected[i] ? pixels[i] - expected[i] : expected[i] - pixels[i]);
    }
    std::printf("host=C++ backend=%s pixels=%zu limit=%u\n", backend == blink_fractal::Backend::Gpu ? "gpu" : "cpu", pixels.size(), limit);
    std::printf("setup_ms=%.3f\nserial_ms=%.3f\nblink_call_ms=%.3f\nspeedup=%.2fx\n",
        setup_ms, serial_samples[4], blink_samples[4], serial_samples[4] / blink_samples[4]);
    std::printf("different_pixels=%u max_iteration_difference=%u checksum=%llu\n", different, max_difference,
        static_cast<unsigned long long>(std::accumulate(pixels.begin(), pixels.end(), uint64_t(0))));
    // 原始计算输出，可由图像工具打开；相同算法生成图像，没有手工绘制。
    std::ofstream image("target/interop/fractal.pgm", std::ios::binary);
    image << "P5\n1000 1000\n255\n";
    for (auto pixel : pixels)
        image.put(char(pixel == limit ? 0 : 255 - std::min(pixel * 8, 254u)));
    return 0;
} catch (const std::exception& e) {
    std::fprintf(stderr, "%s\n", e.what());
    return 1;
}
