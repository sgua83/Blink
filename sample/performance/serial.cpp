#include <algorithm>
#include <cmath>
#include <cstdint>

using u32 = std::uint32_t;
struct Item
{
    u32 id, key, units, keep;
};
struct Affine
{
    u32 scale, bias;
};
static Affine compose(Affine a, Affine b)
{
    return {b.scale * a.scale, b.scale * a.bias + b.bias};
}
static float *row(u32 *data, u32 stride, u32 i)
{
    return reinterpret_cast<float *>(data + stride * i);
}
struct V3
{
    float x, y, z;
};
static V3 add(V3 a, V3 b)
{
    return {a.x + b.x, a.y + b.y, a.z + b.z};
}
static V3 mul(V3 a, float b)
{
    return {a.x * b, a.y * b, a.z * b};
}
static float dot(V3 a, V3 b)
{
    return a.x * b.x + a.y * b.y + a.z * b.z;
}
static V3 cross(V3 a, V3 b)
{
    return {a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x};
}
static V3 load(const float *p)
{
    return {p[0], p[1], p[2]};
}
static void store(float *p, V3 a)
{
    p[0] = a.x;
    p[1] = a.y;
    p[2] = a.z;
}
static V3 normalize(V3 a)
{
    return mul(a, 1.0f / std::sqrt(dot(a, a)));
}
static V3 sample(const float *texture, float u, float v)
{
    float x = std::clamp(u, 0.0f, 1.0f) * 256.0f - 0.5f;
    float y = std::clamp(v, 0.0f, 1.0f) * 256.0f - 0.5f;
    int ix = int(std::floor(x)), iy = int(std::floor(y));
    float fx = x - std::floor(x), fy = y - std::floor(y);
    auto texel = [&](int a, int b) {
        return load(texture + 4 * (std::clamp(b, 0, 255) * 256 + std::clamp(a, 0, 255)));
    };
    V3 a = add(mul(texel(ix, iy), 1.0f - fx), mul(texel(ix + 1, iy), fx));
    V3 b = add(mul(texel(ix, iy + 1), 1.0f - fx), mul(texel(ix + 1, iy + 1), fx));
    return add(mul(a, 1.0f - fy), mul(b, fy));
}
static V3 ggx(V3 normal, V3 view, V3 light, V3 base, float roughness, float ior)
{
    float nl = dot(normal, light), nv = dot(normal, view);
    if (nl <= 0.0f || nv <= 0.0f)
        return {};
    V3 half_vector = normalize(add(view, light));
    float nh = std::max(dot(normal, half_vector), 0.0f), vh = std::max(dot(view, half_vector), 0.0f);
    float alpha = roughness * roughness, a2 = alpha * alpha;
    V3 tangent = cross(normal, half_vector);
    float denominator = dot(tangent, tangent) + a2 * nh * nh;
    float distribution = a2 / (3.14159265359f * denominator * denominator);
    float gv = nl * std::sqrt(nv * nv * (1.0f - a2) + a2);
    float gl = nv * std::sqrt(nl * nl * (1.0f - a2) + a2);
    float f0 = (ior - 1.0f) / (ior + 1.0f);
    float fresnel = f0 * f0 + (1.0f - f0 * f0) * std::pow(1.0f - vh, 5.0f);
    float specular = distribution * fresnel * nl / (2.0f * (gv + gl));
    return add(mul(base, (1.0f - fresnel) * nl / 3.14159265359f), {specular, specular, specular});
}

// 每个分支是一种串行算法；调用者在计时区间外分配数据并读取最终结果。
extern "C" void book_serial(u32 kind, u32 n, u32 **data, const u32 *stride, const u32 *field, const float *texture)
{
    if (kind == 0)
    {
        float *values = reinterpret_cast<float *>(data[0]);
        for (u32 i = 0; i < n; ++i)
            values[i] = values[i] * 2.0f + 1.0f;
    }
    else if (kind == 1)
    {
        u32 total = 0;
        for (u32 i = 0; i < n; ++i)
            total += data[0][i];
        data[1][0] = total;
    }
    else if (kind == 2)
    {
        u32 total = 0;
        for (u32 i = 0; i < n; ++i)
        {
            data[1][i] = total;
            total += data[0][i];
        }
    }
    else if (kind == 3 || kind == 4)
    {
        auto input = reinterpret_cast<Item *>(data[0]);
        auto selected = reinterpret_cast<Item *>(data[1]);
        u32 count = 0;
        for (u32 i = 0; i < n; ++i)
            if (input[i].keep)
                selected[count++] = input[i];
        if (kind == 3)
        {
            data[2][0] = count;
            return;
        }
        auto ordered = reinterpret_cast<Item *>(data[2]);
        std::copy_n(selected, count, ordered);
        std::stable_sort(ordered, ordered + count, [](Item a, Item b) { return a.key < b.key; });
        data[3][0] = count;
        u32 total = 0;
        for (u32 i = 0; i < count; ++i)
        {
            data[4][i] = total;
            total += ordered[i].units;
        }
        data[5][0] = total;
    }
    else if (kind == 5 || kind == 6)
    {
        constexpr u32 Width = 32;
        for (u32 begin = 0; begin < n; begin += Width)
        {
            u32 length = std::min(Width, n - begin);
            float total = 0.0f;
            for (u32 k = 0; k < length; ++k)
            {
                float *cell = row(data[0], stride[0], begin + k);
                if (kind == 6)
                    cell[0] *= 2.0f;
                total += cell[0];
            }
            float before = 0.0f;
            for (u32 k = 0; k < length; ++k)
            {
                float *cell = row(data[0], stride[0], begin + k);
                cell[1] = total;
                if (kind == 5)
                    cell[2] = before;
                cell[3] = cell[0] - total / float(length);
                before += cell[0];
            }
        }
    }
    else if (kind == 7)
    {
        auto input = reinterpret_cast<Affine *>(data[0]);
        auto before = reinterpret_cast<Affine *>(data[1]);
        Affine total{1, 0};
        for (u32 i = 0; i < n; ++i)
        {
            before[i] = total;
            total = compose(total, input[i]);
        }
        *reinterpret_cast<Affine *>(data[2]) = total;
    }
    else if (kind == 8 || kind == 12)
    {
        for (u32 i = 0; i < n; ++i)
        {
            float *body = row(data[0], stride[0], i);
            V3 position = load(body + field[0]), velocity = load(body + field[1]);
            float radius_squared = dot(position, position) + 0.25f;
            V3 acceleration = mul(position, -0.5f / (radius_squared * std::sqrt(radius_squared)));
            velocity = add(velocity, mul(acceleration, 0.001f));
            position = add(position, mul(velocity, 0.001f));
            store(body + field[0], position);
            store(body + field[1], velocity);
            body[field[2]] = 0.5f * dot(velocity, velocity);
        }
        if (kind == 12)
        {
            float total = 0.0f;
            for (u32 i = 0; i < n; ++i)
                total += row(data[0], stride[0], i)[field[2]];
            *reinterpret_cast<float *>(data[1]) = total;
        }
    }
    else if (kind == 9)
    {
        for (u32 i = 0; i < n; ++i)
        {
            const float *m = row(data[0], stride[0], i);
            float *output = row(data[1], stride[1], i);
            V3 a = load(m), b = load(m + 4), c = load(m + 8);
            V3 x = cross(b, c), y = cross(c, a), z = cross(a, b);
            float determinant = dot(a, x);
            store(output, mul(x, 1.0f / determinant));
            store(output + 4, mul(y, 1.0f / determinant));
            store(output + 8, mul(z, 1.0f / determinant));
        }
    }
    else if (kind == 10)
    {
        for (u32 i = 0; i < n; ++i)
        {
            float *p = row(data[0], stride[0], i);
            V3 albedo = sample(texture, p[field[3]], p[field[3] + 1]);
            V3 tint = load(p + field[4]);
            V3 base{albedo.x * tint.x, albedo.y * tint.y, albedo.z * tint.z};
            store(p + field[7],
                  ggx(load(p + field[0]), load(p + field[1]), load(p + field[2]), base, p[field[5]], p[field[6]]));
        }
    }
    else if (kind == 11)
    {
        constexpr u32 W = 1000, InternalWidth = W * 2;
        for (u32 i = 0; i < n; ++i)
        {
            u32 base = (i / W) * (InternalWidth * 2) + (i % W) * 2;
            V3 radiance{};
            for (u32 y = 0; y < 2; ++y)
                for (u32 x = 0; x < 2; ++x)
                    radiance = add(radiance, load(row(data[0], stride[0], base + y * InternalWidth + x)));
            radiance = mul(radiance, 0.25f);
            auto channel = [](float v) {
                return u32(std::clamp(std::pow(std::max(v / (v + 1.0f), 0.0f), 0.454545454545f) * 255.0f + 0.5f, 0.0f,
                                      255.0f));
            };
            data[1][i] = (channel(radiance.x) << 16) | (channel(radiance.y) << 8) | channel(radiance.z) | 0xff000000;
        }
    }
}
