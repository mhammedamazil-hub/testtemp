#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

#include "stonewild/math.hpp"

namespace stonewild {

enum class Block : std::uint8_t {
    Air = 0,
    Grass,
    Soil,
    Stone,
    Sand,
    Water,
    Wood,
    Leaves,
    CoalOre,
    IronOre,
    Bedrock,
    Count,
};

struct BlockInfo {
    const char* name;
    bool solid;
    bool opaque;
    float hardness;
    Vec3 color;
    float alpha;
};

inline constexpr std::array<BlockInfo, static_cast<std::size_t>(Block::Count)> kBlockInfo{{
    {"air", false, false, 0.0F, {0.0F, 0.0F, 0.0F}, 0.0F},
    {"meadow", true, true, 0.8F, {0.24F, 0.58F, 0.18F}, 1.0F},
    {"earth", true, true, 0.7F, {0.40F, 0.25F, 0.12F}, 1.0F},
    {"slate", true, true, 2.8F, {0.43F, 0.46F, 0.49F}, 1.0F},
    {"sand", true, true, 0.6F, {0.76F, 0.68F, 0.43F}, 1.0F},
    {"water", false, false, 1000.0F, {0.12F, 0.35F, 0.67F}, 0.68F},
    {"timber", true, true, 1.7F, {0.40F, 0.27F, 0.12F}, 1.0F},
    {"canopy", true, false, 0.35F, {0.18F, 0.47F, 0.16F}, 0.90F},
    {"coal seam", true, true, 3.0F, {0.16F, 0.17F, 0.18F}, 1.0F},
    {"iron seam", true, true, 3.2F, {0.57F, 0.43F, 0.32F}, 1.0F},
    {"foundation", true, true, 100000.0F, {0.13F, 0.14F, 0.16F}, 1.0F},
}};

inline const BlockInfo& blockInfo(Block block) {
    const auto index = static_cast<std::size_t>(block);
    return kBlockInfo[index < kBlockInfo.size() ? index : 0];
}

inline bool isVisible(Block block) { return block != Block::Air; }

}  // namespace stonewild
