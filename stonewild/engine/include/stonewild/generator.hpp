#pragma once

#include <cstdint>

#include "stonewild/chunk.hpp"

namespace stonewild {

class TerrainGenerator {
public:
    explicit TerrainGenerator(std::uint64_t seed) : seed_(seed) {}

    std::uint64_t seed() const { return seed_; }
    int terrainHeight(int worldX, int worldZ) const;
    void generate(Chunk& chunk) const;

private:
    std::uint32_t hash(int x, int y, int z, std::uint32_t salt = 0) const;
    float valueNoise(float x, float z, std::uint32_t salt) const;
    float octaveNoise(float x, float z) const;
    bool hasTree(int worldX, int worldZ) const;

    std::uint64_t seed_;
};

}  // namespace stonewild
