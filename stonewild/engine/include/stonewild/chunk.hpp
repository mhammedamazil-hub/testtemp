#pragma once

#include <cstdint>
#include <functional>
#include <vector>

#include "stonewild/block.hpp"

namespace stonewild {

inline constexpr int kChunkWidth = 16;
inline constexpr int kChunkDepth = 16;
inline constexpr int kChunkHeight = 96;
inline constexpr int kSeaLevel = 34;

struct ChunkPos {
    int x = 0;
    int z = 0;

    constexpr bool operator==(const ChunkPos& other) const { return x == other.x && z == other.z; }
    constexpr bool operator!=(const ChunkPos& other) const { return !(*this == other); }
    constexpr bool operator<(const ChunkPos& other) const {
        return x < other.x || (x == other.x && z < other.z);
    }
};

struct ChunkPosHash {
    std::size_t operator()(const ChunkPos& value) const noexcept {
        const auto a = static_cast<std::uint64_t>(static_cast<std::uint32_t>(value.x));
        const auto b = static_cast<std::uint64_t>(static_cast<std::uint32_t>(value.z));
        std::uint64_t mixed = (a << 32U) ^ b;
        mixed ^= mixed >> 33U;
        mixed *= 0xff51afd7ed558ccdULL;
        mixed ^= mixed >> 33U;
        return static_cast<std::size_t>(mixed);
    }
};

inline ChunkPos chunkForBlock(int worldX, int worldZ) {
    return {floorDiv(worldX, kChunkWidth), floorDiv(worldZ, kChunkDepth)};
}

class Chunk {
public:
    explicit Chunk(ChunkPos position);

    ChunkPos position() const { return position_; }
    Block get(int x, int y, int z) const;
    bool set(int x, int y, int z, Block block);
    std::uint64_t revision() const { return revision_; }
    void markNeighborsChanged() { ++revision_; }

    static bool validLocal(int x, int y, int z);

private:
    static std::size_t index(int x, int y, int z);

    ChunkPos position_;
    std::vector<Block> blocks_;
    std::uint64_t revision_ = 1;
};

}  // namespace stonewild
