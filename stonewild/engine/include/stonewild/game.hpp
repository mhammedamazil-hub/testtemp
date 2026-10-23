#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <string>

#include "stonewild/player.hpp"
#include "stonewild/raycast.hpp"

namespace stonewild {

class Game {
public:
    explicit Game(std::uint64_t seed = 0x53544f4e4557494cULL);

    void update(const InputState& input, float deltaSeconds);
    void chooseHotbarSlot(std::size_t slot);

    World& world() { return world_; }
    const World& world() const { return world_; }
    Player& player() { return player_; }
    const Player& player() const { return player_; }
    Block selectedBlock() const { return hotbar_[selectedSlot_]; }
    std::size_t selectedSlot() const { return selectedSlot_; }
    int renderRadius() const { return renderRadius_; }
    void setRenderRadius(int radius);
    int quality() const { return quality_; }
    void setQuality(int quality);

private:
    World world_;
    Player player_;
    std::array<Block, 9> hotbar_{{
        Block::Grass,
        Block::Soil,
        Block::Stone,
        Block::Sand,
        Block::Wood,
        Block::Leaves,
        Block::Water,
        Block::CoalOre,
        Block::IronOre,
    }};
    std::size_t selectedSlot_ = 0;
    int renderRadius_ = 2;
    int quality_ = 1;
    float interactionCooldown_ = 0.0F;
};

class SaveSystem {
public:
    static bool save(const std::string& path, const Game& game, std::string* error = nullptr);
    static bool load(const std::string& path, Game& game, std::string* error = nullptr);
};

}  // namespace stonewild
