#include "stonewild/game.hpp"

#include <algorithm>
#include <array>
#include <cstdio>
#include <fstream>
#include <limits>
#include <type_traits>

namespace stonewild {
namespace {

constexpr std::array<char, 8> kSaveMagic{{'S', 'T', 'O', 'N', 'E', 'W', '1', '\0'}};
constexpr std::uint32_t kSaveVersion = 1;
constexpr std::uint32_t kMaxSavedEdits = 10'000'000U;

template <typename T>
bool writeValue(std::ostream& stream, const T& value) {
    static_assert(std::is_trivially_copyable<T>::value, "binary save values must be trivial");
    stream.write(reinterpret_cast<const char*>(&value), static_cast<std::streamsize>(sizeof(T)));
    return stream.good();
}

template <typename T>
bool readValue(std::istream& stream, T& value) {
    static_assert(std::is_trivially_copyable<T>::value, "binary save values must be trivial");
    stream.read(reinterpret_cast<char*>(&value), static_cast<std::streamsize>(sizeof(T)));
    return stream.good();
}

bool fail(std::string* error, const std::string& message) {
    if (error != nullptr) *error = message;
    return false;
}

}  // namespace

Game::Game(std::uint64_t seed, int initialRenderRadius, int initialQuality) : world_(seed) {
    setRenderRadius(initialRenderRadius);
    setQuality(initialQuality);
    world_.ensureAround(player_.position, renderRadius_ + 1);
    for (int y = kChunkHeight - 2; y > 1; --y) {
        if (blockInfo(world_.blockAt(0, y, 0)).solid) {
            player_.position = {0.5F, static_cast<float>(y) + 1.01F, 0.5F};
            break;
        }
    }
}

void Game::setRenderRadius(int radius) { renderRadius_ = std::clamp(radius, 1, 6); }
void Game::setQuality(int quality) { quality_ = std::clamp(quality, 0, 2); }

void Game::chooseHotbarSlot(std::size_t slot) {
    if (slot < hotbar_.size()) selectedSlot_ = slot;
}

void Game::update(const InputState& input, float deltaSeconds) {
    const float delta = clamp(deltaSeconds, 0.0F, 0.05F);
    world_.ensureAround(player_.position, renderRadius_ + 1);
    player_.update(world_, input, delta);
    world_.ensureAround(player_.position, renderRadius_ + 1);
    world_.unloadBeyond(player_.position, renderRadius_ + 2);

    interactionCooldown_ = std::max(0.0F, interactionCooldown_ - delta);
    if (interactionCooldown_ > 0.0F) return;
    const RayHit hit = raycastBlocks(world_, player_.eyePosition(), player_.lookDirection(), 6.0F);
    if (!hit.hit) return;

    if (input.breakBlock) {
        const Block target = world_.blockAt(hit.block);
        if (target != Block::Bedrock && target != Block::Air) {
            world_.setBlock(hit.block, Block::Air);
            interactionCooldown_ = 0.16F;
        }
    } else if (input.placeBlock) {
        const Block previous = world_.blockAt(hit.previous);
        if (previous == Block::Air || previous == Block::Water) {
            world_.setBlock(hit.previous, selectedBlock());
            if (player_.collides(world_, player_.position)) {
                world_.setBlock(hit.previous, previous);
            } else {
                interactionCooldown_ = 0.16F;
            }
        }
    }
}

bool SaveSystem::save(const std::string& path, const Game& game, std::string* error) {
    const std::string temporary = path + ".tmp";
    std::ofstream stream(temporary, std::ios::binary | std::ios::trunc);
    if (!stream) return fail(error, "cannot open temporary save: " + temporary);

    stream.write(kSaveMagic.data(), static_cast<std::streamsize>(kSaveMagic.size()));
    const std::uint64_t seed = game.world().seed();
    const std::uint32_t version = kSaveVersion;
    const std::int32_t quality = game.quality();
    const std::int32_t radius = game.renderRadius();
    const std::uint32_t selected = static_cast<std::uint32_t>(game.selectedSlot());
    const Player& player = game.player();
    const std::vector<BlockEdit> edits = game.world().edits();
    const std::uint32_t editCount = static_cast<std::uint32_t>(edits.size());

    if (!writeValue(stream, version) || !writeValue(stream, seed) ||
        !writeValue(stream, player.position.x) || !writeValue(stream, player.position.y) ||
        !writeValue(stream, player.position.z) || !writeValue(stream, player.yaw) ||
        !writeValue(stream, player.pitch) || !writeValue(stream, quality) ||
        !writeValue(stream, radius) || !writeValue(stream, selected) ||
        !writeValue(stream, editCount)) {
        return fail(error, "failed while writing save header");
    }
    for (const BlockEdit& edit : edits) {
        const std::uint8_t block = static_cast<std::uint8_t>(edit.block);
        if (!writeValue(stream, edit.position.x) || !writeValue(stream, edit.position.y) ||
            !writeValue(stream, edit.position.z) || !writeValue(stream, block)) {
            return fail(error, "failed while writing block edits");
        }
    }
    stream.close();
    if (!stream) return fail(error, "failed to finish save file");
    std::remove(path.c_str());
    if (std::rename(temporary.c_str(), path.c_str()) != 0) {
        return fail(error, "cannot replace save file");
    }
    if (error != nullptr) error->clear();
    return true;
}

bool SaveSystem::load(
    const std::string& path,
    Game& game,
    std::string* error,
    int maximumRenderRadius,
    int maximumQuality) {
    std::ifstream stream(path, std::ios::binary);
    if (!stream) return fail(error, "save does not exist: " + path);
    std::array<char, 8> magic{};
    stream.read(magic.data(), static_cast<std::streamsize>(magic.size()));
    if (!stream || magic != kSaveMagic) return fail(error, "not a Stonewild save");

    std::uint32_t version = 0;
    std::uint64_t seed = 0;
    Player loadedPlayer;
    std::int32_t quality = 0;
    std::int32_t radius = 0;
    std::uint32_t selected = 0;
    std::uint32_t editCount = 0;
    if (!readValue(stream, version) || !readValue(stream, seed) ||
        !readValue(stream, loadedPlayer.position.x) || !readValue(stream, loadedPlayer.position.y) ||
        !readValue(stream, loadedPlayer.position.z) || !readValue(stream, loadedPlayer.yaw) ||
        !readValue(stream, loadedPlayer.pitch) || !readValue(stream, quality) ||
        !readValue(stream, radius) || !readValue(stream, selected) ||
        !readValue(stream, editCount)) {
        return fail(error, "truncated save header");
    }
    if (version != kSaveVersion) return fail(error, "unsupported save version");
    if (seed != game.world().seed()) return fail(error, "save belongs to a different world seed");
    if (editCount > kMaxSavedEdits) return fail(error, "save has an unreasonable edit count");

    std::vector<BlockEdit> edits;
    edits.reserve(editCount);
    for (std::uint32_t index = 0; index < editCount; ++index) {
        BlockEdit edit;
        std::uint8_t block = 0;
        if (!readValue(stream, edit.position.x) || !readValue(stream, edit.position.y) ||
            !readValue(stream, edit.position.z) || !readValue(stream, block)) {
            return fail(error, "truncated block edit table");
        }
        if (block >= static_cast<std::uint8_t>(Block::Count)) return fail(error, "invalid block id in save");
        edit.block = static_cast<Block>(block);
        edits.push_back(edit);
    }

    game.player() = loadedPlayer;
    game.player().velocity = {};
    game.setQuality(std::min(quality, std::clamp(maximumQuality, 0, 2)));
    game.setRenderRadius(std::min(radius, std::clamp(maximumRenderRadius, 1, 6)));
    game.chooseHotbarSlot(selected);
    game.world().replaceEdits(edits);
    game.world().ensureAround(game.player().position, game.renderRadius() + 1);
    if (error != nullptr) error->clear();
    return true;
}

}  // namespace stonewild
