#include "stonewild/raycast.hpp"

#include <cmath>
#include <limits>

namespace stonewild {
namespace {

float firstBoundary(float origin, int cell, int step, float direction) {
    if (step == 0) return std::numeric_limits<float>::infinity();
    const float boundary = static_cast<float>(cell + (step > 0 ? 1 : 0));
    return (boundary - origin) / direction;
}

}  // namespace

RayHit raycastBlocks(const World& world, const Vec3& origin, const Vec3& rawDirection, float maxDistance) {
    const Vec3 direction = normalize(rawDirection);
    if (lengthSquared(direction) < 0.5F || maxDistance <= 0.0F) return {};

    BlockPos current{floorToInt(origin.x), floorToInt(origin.y), floorToInt(origin.z)};
    BlockPos previous = current;
    const int stepX = direction.x > 0.0F ? 1 : (direction.x < 0.0F ? -1 : 0);
    const int stepY = direction.y > 0.0F ? 1 : (direction.y < 0.0F ? -1 : 0);
    const int stepZ = direction.z > 0.0F ? 1 : (direction.z < 0.0F ? -1 : 0);
    const float infinite = std::numeric_limits<float>::infinity();
    const float deltaX = stepX == 0 ? infinite : std::abs(1.0F / direction.x);
    const float deltaY = stepY == 0 ? infinite : std::abs(1.0F / direction.y);
    const float deltaZ = stepZ == 0 ? infinite : std::abs(1.0F / direction.z);
    float maxX = firstBoundary(origin.x, current.x, stepX, direction.x);
    float maxY = firstBoundary(origin.y, current.y, stepY, direction.y);
    float maxZ = firstBoundary(origin.z, current.z, stepZ, direction.z);
    float distance = 0.0F;

    while (distance <= maxDistance) {
        if (isVisible(world.blockAt(current))) return {true, current, previous, distance};
        previous = current;
        if (maxX <= maxY && maxX <= maxZ) {
            current.x += stepX;
            distance = maxX;
            maxX += deltaX;
        } else if (maxY <= maxZ) {
            current.y += stepY;
            distance = maxY;
            maxY += deltaY;
        } else {
            current.z += stepZ;
            distance = maxZ;
            maxZ += deltaZ;
        }
    }
    return {};
}

}  // namespace stonewild
