#include "stonewild/mesh.hpp"

#include <array>

namespace stonewild {
namespace {

struct Int3 {
    int x;
    int y;
    int z;
};

struct Face {
    Int3 normal;
    Int3 tangentA;
    Int3 tangentB;
    std::array<Vec3, 4> corners;
};

constexpr std::array<Face, 6> kFaces{{
    {{1, 0, 0}, {0, 1, 0}, {0, 0, 1}, {{{1, 0, 0}, {1, 1, 0}, {1, 1, 1}, {1, 0, 1}}}},
    {{-1, 0, 0}, {0, 1, 0}, {0, 0, 1}, {{{0, 0, 1}, {0, 1, 1}, {0, 1, 0}, {0, 0, 0}}}},
    {{0, 1, 0}, {1, 0, 0}, {0, 0, 1}, {{{0, 1, 1}, {1, 1, 1}, {1, 1, 0}, {0, 1, 0}}}},
    {{0, -1, 0}, {1, 0, 0}, {0, 0, 1}, {{{0, 0, 0}, {1, 0, 0}, {1, 0, 1}, {0, 0, 1}}}},
    {{0, 0, 1}, {1, 0, 0}, {0, 1, 0}, {{{1, 0, 1}, {1, 1, 1}, {0, 1, 1}, {0, 0, 1}}}},
    {{0, 0, -1}, {1, 0, 0}, {0, 1, 0}, {{{0, 0, 0}, {0, 1, 0}, {1, 1, 0}, {1, 0, 0}}}},
}};

bool occupied(const World& world, int x, int y, int z) {
    return blockInfo(world.blockAt(x, y, z)).solid;
}

float vertexAo(const World& world, int x, int y, int z, const Face& face, const Vec3& corner) {
    const int signA = (face.tangentA.x != 0 ? corner.x : (face.tangentA.y != 0 ? corner.y : corner.z)) > 0.5F ? 1 : -1;
    const int signB = (face.tangentB.x != 0 ? corner.x : (face.tangentB.y != 0 ? corner.y : corner.z)) > 0.5F ? 1 : -1;
    const int outsideX = x + face.normal.x;
    const int outsideY = y + face.normal.y;
    const int outsideZ = z + face.normal.z;
    const bool sideA = occupied(
        world,
        outsideX + face.tangentA.x * signA,
        outsideY + face.tangentA.y * signA,
        outsideZ + face.tangentA.z * signA);
    const bool sideB = occupied(
        world,
        outsideX + face.tangentB.x * signB,
        outsideY + face.tangentB.y * signB,
        outsideZ + face.tangentB.z * signB);
    const bool diagonal = occupied(
        world,
        outsideX + face.tangentA.x * signA + face.tangentB.x * signB,
        outsideY + face.tangentA.y * signA + face.tangentB.y * signB,
        outsideZ + face.tangentA.z * signA + face.tangentB.z * signB);
    if (sideA && sideB) return 0.52F;
    const int occluders = static_cast<int>(sideA) + static_cast<int>(sideB) + static_cast<int>(diagonal);
    return 1.0F - static_cast<float>(occluders) * 0.13F;
}

Vertex makeVertex(const World& world, int x, int y, int z, const Face& face, const Vec3& corner, const BlockInfo& info) {
    return {
        static_cast<float>(x) + corner.x,
        static_cast<float>(y) + corner.y,
        static_cast<float>(z) + corner.z,
        static_cast<float>(face.normal.x),
        static_cast<float>(face.normal.y),
        static_cast<float>(face.normal.z),
        info.color.x,
        info.color.y,
        info.color.z,
        info.alpha,
        vertexAo(world, x, y, z, face, corner),
    };
}

}  // namespace

std::vector<Vertex> buildChunkMesh(const World& world, const Chunk& chunk) {
    std::vector<Vertex> vertices;
    vertices.reserve(8192);
    const int originX = chunk.position().x * kChunkWidth;
    const int originZ = chunk.position().z * kChunkDepth;
    constexpr std::array<int, 6> indices{{0, 1, 2, 0, 2, 3}};

    for (int y = 0; y < kChunkHeight; ++y) {
        for (int localZ = 0; localZ < kChunkDepth; ++localZ) {
            for (int localX = 0; localX < kChunkWidth; ++localX) {
                const Block block = chunk.get(localX, y, localZ);
                if (!isVisible(block)) continue;
                const BlockInfo& info = blockInfo(block);
                const int worldX = originX + localX;
                const int worldZ = originZ + localZ;
                for (const Face& face : kFaces) {
                    const Block neighbor = world.blockAt(
                        worldX + face.normal.x,
                        y + face.normal.y,
                        worldZ + face.normal.z);
                    const bool hiddenByOpaque = blockInfo(neighbor).opaque;
                    const bool hiddenBySameTransparent = !info.opaque && neighbor == block;
                    if (hiddenByOpaque || hiddenBySameTransparent) continue;
                    for (int index : indices) {
                        vertices.push_back(makeVertex(
                            world,
                            worldX,
                            y,
                            worldZ,
                            face,
                            face.corners[static_cast<std::size_t>(index)],
                            info));
                    }
                }
            }
        }
    }
    return vertices;
}

}  // namespace stonewild
