#include "stonewild/world.hpp"

#include <algorithm>
#include <cmath>

namespace stonewild {

World::World(std::uint64_t seed) : generator_(seed) {}

Chunk& World::ensureChunk(ChunkPos position) {
    const auto existing = chunks_.find(position);
    if (existing != chunks_.end()) return *existing->second;

    auto created = std::make_unique<Chunk>(position);
    generator_.generate(*created);
    applyKnownEdits(*created);
    Chunk& result = *created;
    chunks_.emplace(position, std::move(created));
    if (Chunk* neighbor = findChunk({position.x - 1, position.z})) neighbor->markNeighborsChanged();
    if (Chunk* neighbor = findChunk({position.x + 1, position.z})) neighbor->markNeighborsChanged();
    if (Chunk* neighbor = findChunk({position.x, position.z - 1})) neighbor->markNeighborsChanged();
    if (Chunk* neighbor = findChunk({position.x, position.z + 1})) neighbor->markNeighborsChanged();
    return result;
}

const Chunk* World::findChunk(ChunkPos position) const {
    const auto iterator = chunks_.find(position);
    return iterator == chunks_.end() ? nullptr : iterator->second.get();
}

Chunk* World::findChunk(ChunkPos position) {
    const auto iterator = chunks_.find(position);
    return iterator == chunks_.end() ? nullptr : iterator->second.get();
}

Block World::blockAt(int worldX, int worldY, int worldZ) const {
    if (worldY < 0) return Block::Bedrock;
    if (worldY >= kChunkHeight) return Block::Air;
    const ChunkPos chunkPosition = chunkForBlock(worldX, worldZ);
    const Chunk* chunk = findChunk(chunkPosition);
    if (chunk == nullptr) return Block::Air;
    return chunk->get(floorMod(worldX, kChunkWidth), worldY, floorMod(worldZ, kChunkDepth));
}

Block World::blockAt(const BlockPos& position) const {
    return blockAt(position.x, position.y, position.z);
}

bool World::setBlock(const BlockPos& position, Block block, bool recordEdit) {
    if (position.y <= 0 || position.y >= kChunkHeight) return false;
    const ChunkPos chunkPosition = chunkForBlock(position.x, position.z);
    Chunk& chunk = ensureChunk(chunkPosition);
    const bool changed = chunk.set(
        floorMod(position.x, kChunkWidth),
        position.y,
        floorMod(position.z, kChunkDepth),
        block);
    if (!changed) return false;
    if (recordEdit) edits_[position] = block;
    dirtyBorderNeighbors(position);
    return true;
}

void World::ensureAround(const Vec3& worldPosition, int radius) {
    const ChunkPos center = chunkForBlock(floorToInt(worldPosition.x), floorToInt(worldPosition.z));
    const int safeRadius = std::clamp(radius, 1, 8);
    for (int z = center.z - safeRadius; z <= center.z + safeRadius; ++z) {
        for (int x = center.x - safeRadius; x <= center.x + safeRadius; ++x) {
            ensureChunk({x, z});
        }
    }
}

void World::unloadBeyond(const Vec3& worldPosition, int radius) {
    const ChunkPos center = chunkForBlock(floorToInt(worldPosition.x), floorToInt(worldPosition.z));
    const int safeRadius = std::clamp(radius, 1, 10);
    for (auto iterator = chunks_.begin(); iterator != chunks_.end();) {
        const int dx = std::abs(iterator->first.x - center.x);
        const int dz = std::abs(iterator->first.z - center.z);
        if (dx > safeRadius || dz > safeRadius) {
            const ChunkPos removed = iterator->first;
            if (Chunk* neighbor = findChunk({removed.x - 1, removed.z})) neighbor->markNeighborsChanged();
            if (Chunk* neighbor = findChunk({removed.x + 1, removed.z})) neighbor->markNeighborsChanged();
            if (Chunk* neighbor = findChunk({removed.x, removed.z - 1})) neighbor->markNeighborsChanged();
            if (Chunk* neighbor = findChunk({removed.x, removed.z + 1})) neighbor->markNeighborsChanged();
            iterator = chunks_.erase(iterator);
        } else {
            ++iterator;
        }
    }
}

std::vector<const Chunk*> World::loadedChunks() const {
    std::vector<const Chunk*> result;
    result.reserve(chunks_.size());
    for (const auto& entry : chunks_) result.push_back(entry.second.get());
    return result;
}

std::vector<BlockEdit> World::edits() const {
    std::vector<BlockEdit> result;
    result.reserve(edits_.size());
    for (const auto& entry : edits_) result.push_back({entry.first, entry.second});
    return result;
}

void World::replaceEdits(const std::vector<BlockEdit>& edits) {
    edits_.clear();
    for (const BlockEdit& edit : edits) {
        if (edit.position.y <= 0 || edit.position.y >= kChunkHeight) continue;
        edits_[edit.position] = edit.block;
        const ChunkPos position = chunkForBlock(edit.position.x, edit.position.z);
        if (Chunk* chunk = findChunk(position)) {
            chunk->set(
                floorMod(edit.position.x, kChunkWidth),
                edit.position.y,
                floorMod(edit.position.z, kChunkDepth),
                edit.block);
            dirtyBorderNeighbors(edit.position);
        }
    }
}

void World::applyKnownEdits(Chunk& chunk) {
    const int minX = chunk.position().x * kChunkWidth;
    const int minZ = chunk.position().z * kChunkDepth;
    const int maxX = minX + kChunkWidth;
    const int maxZ = minZ + kChunkDepth;
    for (const auto& entry : edits_) {
        const BlockPos& position = entry.first;
        if (position.x >= minX && position.x < maxX && position.z >= minZ && position.z < maxZ) {
            chunk.set(position.x - minX, position.y, position.z - minZ, entry.second);
        }
    }
}

void World::dirtyBorderNeighbors(const BlockPos& position) {
    const int localX = floorMod(position.x, kChunkWidth);
    const int localZ = floorMod(position.z, kChunkDepth);
    const ChunkPos center = chunkForBlock(position.x, position.z);
    if (localX == 0) {
        if (Chunk* neighbor = findChunk({center.x - 1, center.z})) neighbor->markNeighborsChanged();
    }
    if (localX == kChunkWidth - 1) {
        if (Chunk* neighbor = findChunk({center.x + 1, center.z})) neighbor->markNeighborsChanged();
    }
    if (localZ == 0) {
        if (Chunk* neighbor = findChunk({center.x, center.z - 1})) neighbor->markNeighborsChanged();
    }
    if (localZ == kChunkDepth - 1) {
        if (Chunk* neighbor = findChunk({center.x, center.z + 1})) neighbor->markNeighborsChanged();
    }
}

}  // namespace stonewild
