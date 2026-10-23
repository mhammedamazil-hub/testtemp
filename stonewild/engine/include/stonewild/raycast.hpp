#pragma once

#include "stonewild/world.hpp"

namespace stonewild {

struct RayHit {
    bool hit = false;
    BlockPos block{};
    BlockPos previous{};
    float distance = 0.0F;
};

RayHit raycastBlocks(const World& world, const Vec3& origin, const Vec3& direction, float maxDistance);

}  // namespace stonewild
