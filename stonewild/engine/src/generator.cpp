#include "stonewild/generator.hpp"

#include <algorithm>
#include <cmath>

namespace stonewild {
namespace {

float smoothStep(float value) { return value * value * (3.0F - 2.0F * value); }
float mix(float a, float b, float amount) { return a + (b - a) * amount; }

}  // namespace

std::uint32_t TerrainGenerator::hash(int x, int y, int z, std::uint32_t salt) const {
    std::uint64_t value = seed_ ^ static_cast<std::uint64_t>(salt) * 0x9e3779b97f4a7c15ULL;
    value ^= static_cast<std::uint64_t>(static_cast<std::uint32_t>(x)) * 0xbf58476d1ce4e5b9ULL;
    value ^= static_cast<std::uint64_t>(static_cast<std::uint32_t>(y)) * 0x94d049bb133111ebULL;
    value ^= static_cast<std::uint64_t>(static_cast<std::uint32_t>(z)) * 0xd6e8feb86659fd93ULL;
    value ^= value >> 30U;
    value *= 0xbf58476d1ce4e5b9ULL;
    value ^= value >> 27U;
    value *= 0x94d049bb133111ebULL;
    value ^= value >> 31U;
    return static_cast<std::uint32_t>(value ^ (value >> 32U));
}

float TerrainGenerator::valueNoise(float x, float z, std::uint32_t salt) const {
    const int x0 = static_cast<int>(std::floor(x));
    const int z0 = static_cast<int>(std::floor(z));
    const float tx = smoothStep(x - static_cast<float>(x0));
    const float tz = smoothStep(z - static_cast<float>(z0));
    const auto sample = [this, salt](int sx, int sz) {
        return static_cast<float>(hash(sx, 0, sz, salt) & 0x00ffffffU) /
                   static_cast<float>(0x00ffffffU) *
                   2.0F -
               1.0F;
    };
    const float top = mix(sample(x0, z0), sample(x0 + 1, z0), tx);
    const float bottom = mix(sample(x0, z0 + 1), sample(x0 + 1, z0 + 1), tx);
    return mix(top, bottom, tz);
}

float TerrainGenerator::octaveNoise(float x, float z) const {
    float value = 0.0F;
    float amplitude = 1.0F;
    float frequency = 0.0125F;
    float total = 0.0F;
    for (std::uint32_t octave = 0; octave < 4; ++octave) {
        value += valueNoise(x * frequency, z * frequency, 0x100U + octave) * amplitude;
        total += amplitude;
        amplitude *= 0.5F;
        frequency *= 2.03F;
    }
    return value / total;
}

int TerrainGenerator::terrainHeight(int worldX, int worldZ) const {
    const float broad = octaveNoise(static_cast<float>(worldX), static_cast<float>(worldZ));
    const float ridges = std::abs(valueNoise(
        static_cast<float>(worldX) * 0.031F,
        static_cast<float>(worldZ) * 0.031F,
        0x222U));
    const int height = static_cast<int>(std::round(36.0F + broad * 15.0F + ridges * 7.0F));
    return std::clamp(height, 12, kChunkHeight - 14);
}

bool TerrainGenerator::hasTree(int worldX, int worldZ) const {
    const int height = terrainHeight(worldX, worldZ);
    if (height <= kSeaLevel + 1) return false;
    return (hash(worldX, height, worldZ, 0x7eeU) % 173U) < 3U;
}

void TerrainGenerator::generate(Chunk& chunk) const {
    const ChunkPos position = chunk.position();
    const int originX = position.x * kChunkWidth;
    const int originZ = position.z * kChunkDepth;

    for (int localZ = 0; localZ < kChunkDepth; ++localZ) {
        for (int localX = 0; localX < kChunkWidth; ++localX) {
            const int worldX = originX + localX;
            const int worldZ = originZ + localZ;
            const int surface = terrainHeight(worldX, worldZ);
            for (int y = 0; y < kChunkHeight; ++y) {
                Block block = Block::Air;
                if (y == 0) {
                    block = Block::Bedrock;
                } else if (y <= surface) {
                    const float cave = std::sin(static_cast<float>(worldX) * 0.19F + static_cast<float>(y) * 0.13F) +
                                       std::sin(static_cast<float>(worldZ) * 0.17F - static_cast<float>(y) * 0.11F) +
                                       std::sin(static_cast<float>(worldX + worldZ) * 0.07F + static_cast<float>(y) * 0.23F);
                    const bool carveCave = y > 5 && y < surface - 4 && cave > 2.45F;
                    if (!carveCave) {
                        if (y == surface) {
                            block = surface <= kSeaLevel + 1 ? Block::Sand : Block::Grass;
                        } else if (y >= surface - 3) {
                            block = surface <= kSeaLevel + 1 ? Block::Sand : Block::Soil;
                        } else {
                            const std::uint32_t ore = hash(worldX, y, worldZ, 0x0aeU) % 997U;
                            if (y < 40 && ore < 8U) {
                                block = Block::CoalOre;
                            } else if (y < 28 && ore >= 8U && ore < 13U) {
                                block = Block::IronOre;
                            } else {
                                block = Block::Stone;
                            }
                        }
                    }
                } else if (y <= kSeaLevel) {
                    block = Block::Water;
                }
                chunk.set(localX, y, localZ, block);
            }
        }
    }

    // Inspect potential tree centers just outside this chunk too, so canopies
    // continue cleanly over chunk boundaries without needing neighboring data.
    for (int treeZ = originZ - 2; treeZ < originZ + kChunkDepth + 2; ++treeZ) {
        for (int treeX = originX - 2; treeX < originX + kChunkWidth + 2; ++treeX) {
            if (!hasTree(treeX, treeZ)) continue;
            const int baseY = terrainHeight(treeX, treeZ) + 1;
            const int trunkHeight = 4 + static_cast<int>(hash(treeX, baseY, treeZ, 0xabcU) % 3U);
            for (int y = baseY; y < baseY + trunkHeight; ++y) {
                const int localX = treeX - originX;
                const int localZ = treeZ - originZ;
                if (Chunk::validLocal(localX, y, localZ)) chunk.set(localX, y, localZ, Block::Wood);
            }
            const int crownY = baseY + trunkHeight;
            for (int dz = -2; dz <= 2; ++dz) {
                for (int dx = -2; dx <= 2; ++dx) {
                    for (int dy = -2; dy <= 1; ++dy) {
                        if (std::abs(dx) + std::abs(dz) + (dy == 1 ? 1 : 0) > 3) continue;
                        const int localX = treeX + dx - originX;
                        const int localZ = treeZ + dz - originZ;
                        const int y = crownY + dy;
                        if (Chunk::validLocal(localX, y, localZ) &&
                            chunk.get(localX, y, localZ) == Block::Air) {
                            chunk.set(localX, y, localZ, Block::Leaves);
                        }
                    }
                }
            }
        }
    }
}

}  // namespace stonewild
