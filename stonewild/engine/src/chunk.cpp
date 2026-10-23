#include "stonewild/chunk.hpp"

namespace stonewild {

Chunk::Chunk(ChunkPos position)
    : position_(position),
      blocks_(static_cast<std::size_t>(kChunkWidth * kChunkHeight * kChunkDepth), Block::Air) {}

bool Chunk::validLocal(int x, int y, int z) {
    return x >= 0 && x < kChunkWidth && y >= 0 && y < kChunkHeight && z >= 0 && z < kChunkDepth;
}

std::size_t Chunk::index(int x, int y, int z) {
    return static_cast<std::size_t>((y * kChunkDepth + z) * kChunkWidth + x);
}

Block Chunk::get(int x, int y, int z) const {
    return validLocal(x, y, z) ? blocks_[index(x, y, z)] : Block::Air;
}

bool Chunk::set(int x, int y, int z, Block block) {
    if (!validLocal(x, y, z)) return false;
    Block& destination = blocks_[index(x, y, z)];
    if (destination == block) return false;
    destination = block;
    ++revision_;
    return true;
}

}  // namespace stonewild
