#pragma once

#include <cstdint>
#include <map>
#include <memory>
#include <vector>

#include "stonewild/generator.hpp"

namespace stonewild {

struct BlockEdit {
    BlockPos position;
    Block block = Block::Air;
};

class World {
public:
    explicit World(std::uint64_t seed);

    std::uint64_t seed() const { return generator_.seed(); }
    Chunk& ensureChunk(ChunkPos position);
    const Chunk* findChunk(ChunkPos position) const;
    Chunk* findChunk(ChunkPos position);

    Block blockAt(int worldX, int worldY, int worldZ) const;
    Block blockAt(const BlockPos& position) const;
    bool setBlock(const BlockPos& position, Block block, bool recordEdit = true);

    void ensureAround(const Vec3& worldPosition, int radius);
    void unloadBeyond(const Vec3& worldPosition, int radius);
    std::vector<const Chunk*> loadedChunks() const;
    std::size_t loadedChunkCount() const { return chunks_.size(); }

    std::vector<BlockEdit> edits() const;
    void replaceEdits(const std::vector<BlockEdit>& edits);

private:
    void applyKnownEdits(Chunk& chunk);
    void dirtyBorderNeighbors(const BlockPos& position);

    TerrainGenerator generator_;
    std::map<ChunkPos, std::unique_ptr<Chunk>> chunks_;
    std::map<BlockPos, Block> edits_;
};

}  // namespace stonewild
