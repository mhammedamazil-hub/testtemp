#include <chrono>
#include <cmath>
#include <filesystem>
#include <iostream>
#include <stdexcept>
#include <string>

#include "stonewild/game.hpp"
#include "stonewild/mesh.hpp"

namespace {

int gAssertions = 0;

void expect(bool condition, const std::string& message) {
    ++gAssertions;
    if (!condition) throw std::runtime_error(message);
}

void testCoordinateMath() {
    using namespace stonewild;
    expect(floorDiv(0, 16) == 0, "zero floor division");
    expect(floorDiv(15, 16) == 0, "positive floor division");
    expect(floorDiv(16, 16) == 1, "positive next chunk");
    expect(floorDiv(-1, 16) == -1, "negative floor division");
    expect(floorDiv(-16, 16) == -1, "negative exact chunk");
    expect(floorDiv(-17, 16) == -2, "negative previous chunk");
    expect(floorMod(-1, 16) == 15, "negative floor modulus");
    expect(chunkForBlock(-1, 16) == ChunkPos{-1, 1}, "negative block to chunk");
    expect(sizeof(Block) == 1, "block storage must remain byte-sized for low-memory systems");
}

void testDeterministicTerrain() {
    using namespace stonewild;
    TerrainGenerator first(123456789ULL);
    TerrainGenerator second(123456789ULL);
    TerrainGenerator different(987654321ULL);
    Chunk a({-2, 3});
    Chunk b({-2, 3});
    first.generate(a);
    second.generate(b);
    for (int y = 0; y < kChunkHeight; y += 7) {
        for (int z = 0; z < kChunkDepth; z += 3) {
            for (int x = 0; x < kChunkWidth; x += 3) {
                expect(a.get(x, y, z) == b.get(x, y, z), "same seed must generate the same chunk");
            }
        }
    }
    bool foundDifferentHeight = false;
    for (int x = -30; x < 30; ++x) {
        if (first.terrainHeight(x, x / 2) != different.terrainHeight(x, x / 2)) {
            foundDifferentHeight = true;
            break;
        }
    }
    expect(foundDifferentHeight, "different seeds should vary terrain");
    expect(a.get(0, 0, 0) == Block::Bedrock, "world floor should be foundation");
}

void testWorldStreamingAndEdits() {
    using namespace stonewild;
    World world(42);
    world.ensureAround({0.0F, 50.0F, 0.0F}, 1);
    expect(world.loadedChunkCount() == 9, "radius one should load a 3x3 chunk square");
    const BlockPos edit{-1, 75, -1};
    expect(world.setBlock(edit, Block::Wood), "first edit should change a block");
    expect(world.blockAt(edit) == Block::Wood, "edited block should be visible");
    expect(world.edits().size() == 1, "edit should be persisted");
    world.unloadBeyond({1000.0F, 50.0F, 1000.0F}, 1);
    expect(world.loadedChunkCount() == 0, "distant chunks should unload");
    world.ensureChunk(chunkForBlock(edit.x, edit.z));
    expect(world.blockAt(edit) == Block::Wood, "edit should survive chunk regeneration");
}

void testRaycastAndMeshing() {
    using namespace stonewild;
    World world(77);
    world.ensureAround({0.0F, 70.0F, 0.0F}, 1);
    const BlockPos target{3, 78, 3};
    world.setBlock(target, Block::Stone);
    const RayHit hit = raycastBlocks(world, {0.5F, 78.5F, 3.5F}, {1.0F, 0.0F, 0.0F}, 6.0F);
    expect(hit.hit, "ray should hit the synthetic block");
    expect(hit.block == target, "ray should report the exact target");
    expect(hit.previous == BlockPos{2, 78, 3}, "ray should preserve placement cell");

    const Chunk* chunk = world.findChunk({0, 0});
    expect(chunk != nullptr, "test chunk should be loaded");
    const auto mesh = buildChunkMesh(world, *chunk);
    expect(!mesh.empty(), "generated chunk should have a mesh");
    expect(mesh.size() % 6 == 0, "every face should emit two triangles");
    for (const auto& vertex : mesh) {
        expect(vertex.ambientOcclusion >= 0.5F && vertex.ambientOcclusion <= 1.0F, "AO must stay bounded");
    }
}

void testPlayerPhysics() {
    using namespace stonewild;
    World world(88);
    world.ensureAround({0.0F, 50.0F, 0.0F}, 1);
    Player player;
    player.position = {0.5F, 80.0F, 0.5F};
    for (int x = -1; x <= 1; ++x) {
        for (int z = -1; z <= 1; ++z) world.setBlock({x, 78, z}, Block::Stone);
    }
    for (int index = 0; index < 180; ++index) player.update(world, {}, 1.0F / 60.0F);
    expect(player.onGround, "player should land on the synthetic floor");
    expect(std::abs(player.position.y - 79.0F) < 0.08F, "player feet should rest on top of floor");

    InputState jump;
    jump.jump = true;
    player.update(world, jump, 1.0F / 60.0F);
    expect(player.velocity.y > 0.0F, "jump should create upward velocity");
}

void testSaveRoundTrip() {
    using namespace stonewild;
    constexpr std::uint64_t seed = 0x1234abcdULL;
    Game source(seed, 1, 0);
    expect(source.renderRadius() == 1, "initial low-memory render radius should apply before streaming");
    expect(source.quality() == 0, "initial low-memory quality should apply before rendering");
    expect(source.world().loadedChunkCount() == 25, "low-memory startup should load only a 5x5 staging area");
    source.player().position = {12.25F, 67.0F, -8.75F};
    source.player().yaw = 1.25F;
    source.setQuality(2);
    source.setRenderRadius(3);
    source.chooseHotbarSlot(4);
    const BlockPos edit{4, 80, 5};
    source.world().setBlock(edit, Block::Wood);

    const auto stamp = std::chrono::steady_clock::now().time_since_epoch().count();
    const std::filesystem::path path = std::filesystem::temp_directory_path() /
        ("stonewild-engine-test-" + std::to_string(stamp) + ".sav");
    std::string error;
    expect(SaveSystem::save(path.string(), source, &error), "save failed: " + error);

    Game loaded(seed);
    expect(SaveSystem::load(path.string(), loaded, &error), "load failed: " + error);
    expect(std::abs(loaded.player().position.x - 12.25F) < 0.001F, "player x should round trip");
    expect(std::abs(loaded.player().position.z + 8.75F) < 0.001F, "player z should round trip");
    expect(std::abs(loaded.player().yaw - 1.25F) < 0.001F, "yaw should round trip");
    expect(loaded.quality() == 2, "quality should round trip");
    expect(loaded.renderRadius() == 3, "render radius should round trip");
    expect(loaded.selectedSlot() == 4, "hotbar slot should round trip");
    expect(loaded.world().blockAt(edit) == Block::Wood, "world edit should round trip");
    std::filesystem::remove(path);
}

}  // namespace

int main() {
    try {
        testCoordinateMath();
        testDeterministicTerrain();
        testWorldStreamingAndEdits();
        testRaycastAndMeshing();
        testPlayerPhysics();
        testSaveRoundTrip();
        std::cout << "Stonewild core tests passed: " << gAssertions << " assertions\n";
        return 0;
    } catch (const std::exception& exception) {
        std::cerr << "Stonewild core test failure: " << exception.what() << '\n';
        return 1;
    }
}
