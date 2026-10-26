#include <cstdint>

struct Pair {
    std::uint32_t left;
    std::uint32_t right;
};

extern "C" __declspec(dllexport) Pair add_values(Pair value, std::uint32_t amount) {
    value.left += amount;
    value.right += amount;
    return value;
}
