#pragma once

#include <algorithm>
#include <cmath>
#include <cstdint>

namespace stonewild {

struct Vec3 {
    float x = 0.0F;
    float y = 0.0F;
    float z = 0.0F;

    constexpr Vec3() = default;
    constexpr Vec3(float xValue, float yValue, float zValue) : x(xValue), y(yValue), z(zValue) {}

    constexpr Vec3 operator+(const Vec3& other) const { return {x + other.x, y + other.y, z + other.z}; }
    constexpr Vec3 operator-(const Vec3& other) const { return {x - other.x, y - other.y, z - other.z}; }
    constexpr Vec3 operator*(float scale) const { return {x * scale, y * scale, z * scale}; }
    constexpr Vec3 operator/(float scale) const { return {x / scale, y / scale, z / scale}; }
    Vec3& operator+=(const Vec3& other) {
        x += other.x;
        y += other.y;
        z += other.z;
        return *this;
    }
};

inline float dot(const Vec3& a, const Vec3& b) { return a.x * b.x + a.y * b.y + a.z * b.z; }
inline float lengthSquared(const Vec3& value) { return dot(value, value); }
inline float length(const Vec3& value) { return std::sqrt(lengthSquared(value)); }
inline Vec3 normalize(const Vec3& value) {
    const float magnitude = length(value);
    return magnitude > 0.00001F ? value / magnitude : Vec3{};
}
inline float clamp(float value, float low, float high) { return std::max(low, std::min(value, high)); }

struct BlockPos {
    int x = 0;
    int y = 0;
    int z = 0;

    constexpr bool operator==(const BlockPos& other) const {
        return x == other.x && y == other.y && z == other.z;
    }
    constexpr bool operator<(const BlockPos& other) const {
        if (x != other.x) return x < other.x;
        if (y != other.y) return y < other.y;
        return z < other.z;
    }
};

inline int floorToInt(float value) { return static_cast<int>(std::floor(value)); }
inline int floorDiv(int value, int divisor) {
    const int quotient = value / divisor;
    const int remainder = value % divisor;
    return remainder < 0 ? quotient - 1 : quotient;
}
inline int floorMod(int value, int divisor) {
    const int remainder = value % divisor;
    return remainder < 0 ? remainder + divisor : remainder;
}

inline Vec3 directionFromYawPitch(float yaw, float pitch) {
    const float cosinePitch = std::cos(pitch);
    return normalize({
        std::sin(yaw) * cosinePitch,
        std::sin(pitch),
        -std::cos(yaw) * cosinePitch,
    });
}

}  // namespace stonewild
