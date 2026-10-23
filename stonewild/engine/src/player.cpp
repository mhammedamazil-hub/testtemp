#include "stonewild/player.hpp"

#include <algorithm>
#include <cmath>

namespace stonewild {

bool Player::collides(const World& world, const Vec3& candidate) const {
    constexpr float halfWidth = 0.30F;
    constexpr float height = 1.80F;
    constexpr float epsilon = 0.001F;
    const int minX = floorToInt(candidate.x - halfWidth);
    const int maxX = floorToInt(candidate.x + halfWidth - epsilon);
    const int minY = floorToInt(candidate.y);
    const int maxY = floorToInt(candidate.y + height - epsilon);
    const int minZ = floorToInt(candidate.z - halfWidth);
    const int maxZ = floorToInt(candidate.z + halfWidth - epsilon);
    for (int z = minZ; z <= maxZ; ++z) {
        for (int y = minY; y <= maxY; ++y) {
            for (int x = minX; x <= maxX; ++x) {
                if (blockInfo(world.blockAt(x, y, z)).solid) return true;
            }
        }
    }
    return false;
}

void Player::moveAxis(const World& world, int axis, float amount) {
    const int steps = std::max(1, static_cast<int>(std::ceil(std::abs(amount) / 0.08F)));
    const float increment = amount / static_cast<float>(steps);
    for (int index = 0; index < steps; ++index) {
        Vec3 candidate = position;
        if (axis == 0) candidate.x += increment;
        if (axis == 1) candidate.y += increment;
        if (axis == 2) candidate.z += increment;
        if (collides(world, candidate)) {
            if (axis == 0) velocity.x = 0.0F;
            if (axis == 1) {
                if (increment < 0.0F) onGround = true;
                velocity.y = 0.0F;
            }
            if (axis == 2) velocity.z = 0.0F;
            return;
        }
        position = candidate;
    }
}

void Player::update(World& world, const InputState& input, float deltaSeconds) {
    const float delta = clamp(deltaSeconds, 0.0F, 0.05F);
    yaw += input.lookX;
    pitch = clamp(pitch + input.lookY, -1.52F, 1.52F);

    const Vec3 forward{std::sin(yaw), 0.0F, -std::cos(yaw)};
    const Vec3 right{std::cos(yaw), 0.0F, std::sin(yaw)};
    Vec3 wish = forward * clamp(input.forward, -1.0F, 1.0F) +
                right * clamp(input.right, -1.0F, 1.0F);
    const float wishLength = length(wish);
    if (wishLength > 1.0F) wish = wish / wishLength;
    const float speed = input.sprint ? 7.0F : 4.4F;
    const float responsiveness = onGround ? 18.0F : 6.0F;
    const float blend = clamp(responsiveness * delta, 0.0F, 1.0F);
    velocity.x += (wish.x * speed - velocity.x) * blend;
    velocity.z += (wish.z * speed - velocity.z) * blend;

    if (input.jump && onGround) {
        velocity.y = 7.2F;
        onGround = false;
    }
    velocity.y = std::max(velocity.y - 20.0F * delta, -28.0F);

    onGround = false;
    moveAxis(world, 0, velocity.x * delta);
    moveAxis(world, 1, velocity.y * delta);
    moveAxis(world, 2, velocity.z * delta);
    if (!onGround) onGround = collides(world, position + Vec3{0.0F, -0.04F, 0.0F});

    if (position.y < -8.0F) {
        position = {0.5F, 60.0F, 0.5F};
        velocity = {};
    }
}

}  // namespace stonewild
