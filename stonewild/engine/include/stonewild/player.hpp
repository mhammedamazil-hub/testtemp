#pragma once

#include "stonewild/world.hpp"

namespace stonewild {

struct InputState {
    float forward = 0.0F;
    float right = 0.0F;
    float lookX = 0.0F;
    float lookY = 0.0F;
    bool jump = false;
    bool sprint = false;
    bool breakBlock = false;
    bool placeBlock = false;
};

class Player {
public:
    Vec3 position{0.5F, 48.0F, 0.5F};
    Vec3 velocity{};
    float yaw = 0.0F;
    float pitch = -0.15F;
    bool onGround = false;

    Vec3 eyePosition() const { return position + Vec3{0.0F, 1.62F, 0.0F}; }
    Vec3 lookDirection() const { return directionFromYawPitch(yaw, pitch); }

    void update(World& world, const InputState& input, float deltaSeconds);
    bool collides(const World& world, const Vec3& candidate) const;

private:
    void moveAxis(const World& world, int axis, float amount);
};

}  // namespace stonewild
