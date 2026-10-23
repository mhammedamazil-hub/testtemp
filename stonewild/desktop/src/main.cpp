#define GL_GLEXT_PROTOTYPES 1
#include <SDL2/SDL.h>
#include <SDL2/SDL_opengl.h>
#include <sys/sysinfo.h>
#include <unistd.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <map>
#include <memory>
#include <set>
#include <sstream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

#include "stonewild/game.hpp"
#include "stonewild/mesh.hpp"

namespace {

using stonewild::Block;
using stonewild::Chunk;
using stonewild::ChunkPos;
using stonewild::Game;
using stonewild::InputState;
using stonewild::RayHit;
using stonewild::SaveSystem;
using stonewild::Vec3;
using stonewild::Vertex;
using stonewild::blockInfo;
using stonewild::buildChunkMesh;
using stonewild::clamp;
using stonewild::dot;
using stonewild::normalize;
using stonewild::raycastBlocks;

constexpr float kPi = 3.14159265358979323846F;
constexpr std::string_view kVersion = "0.2.0";
constexpr std::uint64_t kDefaultSeed = 0x53544f4e4557494cULL;

struct Mat4 {
    std::array<float, 16> value{};
};

Vec3 crossProduct(const Vec3& a, const Vec3& b) {
    return {
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    };
}

Mat4 multiply(const Mat4& a, const Mat4& b) {
    Mat4 result{};
    for (int column = 0; column < 4; ++column) {
        for (int row = 0; row < 4; ++row) {
            for (int inner = 0; inner < 4; ++inner) {
                result.value[static_cast<std::size_t>(column * 4 + row)] +=
                    a.value[static_cast<std::size_t>(inner * 4 + row)] *
                    b.value[static_cast<std::size_t>(column * 4 + inner)];
            }
        }
    }
    return result;
}

Mat4 perspective(float fieldOfViewRadians, float aspect, float nearPlane, float farPlane) {
    Mat4 result{};
    const float focal = 1.0F / std::tan(fieldOfViewRadians * 0.5F);
    result.value[0] = focal / aspect;
    result.value[5] = focal;
    result.value[10] = (farPlane + nearPlane) / (nearPlane - farPlane);
    result.value[11] = -1.0F;
    result.value[14] = (2.0F * farPlane * nearPlane) / (nearPlane - farPlane);
    return result;
}

Mat4 lookAt(const Vec3& eye, const Vec3& target, const Vec3& up) {
    const Vec3 forward = normalize(target - eye);
    const Vec3 side = normalize(crossProduct(forward, up));
    const Vec3 cameraUp = crossProduct(side, forward);
    Mat4 result{};
    result.value[0] = side.x;
    result.value[4] = side.y;
    result.value[8] = side.z;
    result.value[12] = -dot(side, eye);
    result.value[1] = cameraUp.x;
    result.value[5] = cameraUp.y;
    result.value[9] = cameraUp.z;
    result.value[13] = -dot(cameraUp, eye);
    result.value[2] = -forward.x;
    result.value[6] = -forward.y;
    result.value[10] = -forward.z;
    result.value[14] = dot(forward, eye);
    result.value[15] = 1.0F;
    return result;
}

GLuint compileShader(GLenum type, const char* source) {
    const GLuint shader = glCreateShader(type);
    glShaderSource(shader, 1, &source, nullptr);
    glCompileShader(shader);
    GLint compiled = GL_FALSE;
    glGetShaderiv(shader, GL_COMPILE_STATUS, &compiled);
    if (compiled == GL_TRUE) return shader;
    GLint length = 0;
    glGetShaderiv(shader, GL_INFO_LOG_LENGTH, &length);
    std::string log(static_cast<std::size_t>(std::max(length, 1)), '\0');
    glGetShaderInfoLog(shader, length, nullptr, log.data());
    glDeleteShader(shader);
    throw std::runtime_error("OpenGL shader compile failed: " + log);
}

GLuint createProgram(const char* vertexSource, const char* fragmentSource) {
    const GLuint vertex = compileShader(GL_VERTEX_SHADER, vertexSource);
    const GLuint fragment = compileShader(GL_FRAGMENT_SHADER, fragmentSource);
    const GLuint program = glCreateProgram();
    glAttachShader(program, vertex);
    glAttachShader(program, fragment);
    glLinkProgram(program);
    glDeleteShader(vertex);
    glDeleteShader(fragment);
    GLint linked = GL_FALSE;
    glGetProgramiv(program, GL_LINK_STATUS, &linked);
    if (linked == GL_TRUE) return program;
    GLint length = 0;
    glGetProgramiv(program, GL_INFO_LOG_LENGTH, &length);
    std::string log(static_cast<std::size_t>(std::max(length, 1)), '\0');
    glGetProgramInfoLog(program, length, nullptr, log.data());
    glDeleteProgram(program);
    throw std::runtime_error("OpenGL shader link failed: " + log);
}

struct GpuMesh {
    GLuint vertexArray = 0;
    GLuint vertexBuffer = 0;
    GLsizei vertexCount = 0;
    std::uint64_t revision = 0;
    std::size_t bytes = 0;
};

struct OverlayVertex {
    float x;
    float y;
    float r;
    float g;
    float b;
    float a;
};

class DesktopRenderer {
public:
    void initialize() {
        constexpr const char* worldVertex = R"GLSL(#version 330 core
layout(location=0) in vec3 aPosition;
layout(location=1) in vec3 aNormal;
layout(location=2) in vec4 aColor;
layout(location=3) in float aAo;
uniform mat4 uViewProjection;
out vec3 vPosition;
out vec3 vNormal;
out vec4 vColor;
out float vAo;
void main() {
    vPosition = aPosition;
    vNormal = aNormal;
    vColor = aColor;
    vAo = aAo;
    gl_Position = uViewProjection * vec4(aPosition, 1.0);
}
)GLSL";
        constexpr const char* worldFragment = R"GLSL(#version 330 core
in vec3 vPosition;
in vec3 vNormal;
in vec4 vColor;
in float vAo;
uniform vec3 uCamera;
uniform int uQuality;
uniform float uFogNear;
uniform float uFogFar;
out vec4 fragColor;
float materialNoise(vec3 p) {
    vec3 cell = floor(p * (uQuality > 1 ? 7.0 : 3.0));
    return fract(sin(dot(cell, vec3(12.9898, 78.233, 37.719))) * 43758.5453);
}
void main() {
    vec3 sun = normalize(vec3(0.45, 0.88, 0.24));
    float diffuse = max(dot(normalize(vNormal), sun), 0.0);
    float light = (0.34 + diffuse * 0.66) * vAo;
    float detail = 1.0;
    if (uQuality > 0) detail = mix(0.86, 1.12, materialNoise(vPosition));
    if (uQuality > 1) detail *= mix(0.94, 1.06, materialNoise(vPosition * 2.71));
    vec3 color = vColor.rgb * light * detail;
    float distanceToCamera = length(vPosition - uCamera);
    float fog = smoothstep(uFogNear, uFogFar, distanceToCamera);
    vec3 sky = vec3(0.40, 0.66, 0.88);
    fragColor = vec4(mix(color, sky, fog), vColor.a);
}
)GLSL";
        constexpr const char* flatVertex = R"GLSL(#version 330 core
layout(location=0) in vec3 aPosition;
uniform mat4 uViewProjection;
void main() { gl_Position = uViewProjection * vec4(aPosition, 1.0); }
)GLSL";
        constexpr const char* flatFragment = R"GLSL(#version 330 core
uniform vec4 uColor;
out vec4 fragColor;
void main() { fragColor = uColor; }
)GLSL";
        constexpr const char* overlayVertex = R"GLSL(#version 330 core
layout(location=0) in vec2 aPosition;
layout(location=1) in vec4 aColor;
out vec4 vColor;
void main() { vColor = aColor; gl_Position = vec4(aPosition, 0.0, 1.0); }
)GLSL";
        constexpr const char* overlayFragment = R"GLSL(#version 330 core
in vec4 vColor;
out vec4 fragColor;
void main() { fragColor = vColor; }
)GLSL";

        worldProgram_ = createProgram(worldVertex, worldFragment);
        flatProgram_ = createProgram(flatVertex, flatFragment);
        overlayProgram_ = createProgram(overlayVertex, overlayFragment);
        worldViewProjection_ = glGetUniformLocation(worldProgram_, "uViewProjection");
        worldCamera_ = glGetUniformLocation(worldProgram_, "uCamera");
        worldQuality_ = glGetUniformLocation(worldProgram_, "uQuality");
        worldFogNear_ = glGetUniformLocation(worldProgram_, "uFogNear");
        worldFogFar_ = glGetUniformLocation(worldProgram_, "uFogFar");
        flatViewProjection_ = glGetUniformLocation(flatProgram_, "uViewProjection");
        flatColor_ = glGetUniformLocation(flatProgram_, "uColor");

        glGenVertexArrays(1, &overlayArray_);
        glGenBuffers(1, &overlayBuffer_);
        glBindVertexArray(overlayArray_);
        glBindBuffer(GL_ARRAY_BUFFER, overlayBuffer_);
        glEnableVertexAttribArray(0);
        glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, sizeof(OverlayVertex), nullptr);
        glEnableVertexAttribArray(1);
        glVertexAttribPointer(1, 4, GL_FLOAT, GL_FALSE, sizeof(OverlayVertex), reinterpret_cast<void*>(offsetof(OverlayVertex, r)));

        glGenVertexArrays(1, &outlineArray_);
        glGenBuffers(1, &outlineBuffer_);
        glBindVertexArray(outlineArray_);
        glBindBuffer(GL_ARRAY_BUFFER, outlineBuffer_);
        glEnableVertexAttribArray(0);
        glVertexAttribPointer(0, 3, GL_FLOAT, GL_FALSE, sizeof(Vec3), nullptr);
        glBindVertexArray(0);

        glEnable(GL_DEPTH_TEST);
        glEnable(GL_CULL_FACE);
        glCullFace(GL_BACK);
        glEnable(GL_BLEND);
        glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
    }

    void shutdown() {
        for (auto& entry : meshes_) destroyMesh(entry.second);
        meshes_.clear();
        if (outlineBuffer_ != 0) glDeleteBuffers(1, &outlineBuffer_);
        if (outlineArray_ != 0) glDeleteVertexArrays(1, &outlineArray_);
        if (overlayBuffer_ != 0) glDeleteBuffers(1, &overlayBuffer_);
        if (overlayArray_ != 0) glDeleteVertexArrays(1, &overlayArray_);
        if (worldProgram_ != 0) glDeleteProgram(worldProgram_);
        if (flatProgram_ != 0) glDeleteProgram(flatProgram_);
        if (overlayProgram_ != 0) glDeleteProgram(overlayProgram_);
        outlineBuffer_ = 0;
        outlineArray_ = 0;
        overlayBuffer_ = 0;
        overlayArray_ = 0;
        worldProgram_ = 0;
        flatProgram_ = 0;
        overlayProgram_ = 0;
        gpuBytes_ = 0;
    }

    ~DesktopRenderer() = default;

    void render(SDL_Window* window, const Game& game) {
        int width = 1;
        int height = 1;
        SDL_GL_GetDrawableSize(window, &width, &height);
        width = std::max(width, 1);
        height = std::max(height, 1);
        syncMeshes(game);

        glViewport(0, 0, width, height);
        glClearColor(0.40F, 0.66F, 0.88F, 1.0F);
        glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);
        const Vec3 eye = game.player().eyePosition();
        const Mat4 projection = perspective(
            68.0F * kPi / 180.0F,
            static_cast<float>(width) / static_cast<float>(height),
            0.08F,
            180.0F);
        const Mat4 view = lookAt(eye, eye + game.player().lookDirection(), {0.0F, 1.0F, 0.0F});
        const Mat4 viewProjection = multiply(projection, view);
        const float fogFar = static_cast<float>((game.renderRadius() + 1) * stonewild::kChunkWidth);

        glEnable(GL_DEPTH_TEST);
        glEnable(GL_CULL_FACE);
        glUseProgram(worldProgram_);
        glUniformMatrix4fv(worldViewProjection_, 1, GL_FALSE, viewProjection.value.data());
        glUniform3f(worldCamera_, eye.x, eye.y, eye.z);
        glUniform1i(worldQuality_, game.quality());
        glUniform1f(worldFogNear_, fogFar * 0.58F);
        glUniform1f(worldFogFar_, fogFar);
        const float drawDistance = fogFar + 24.0F;
        for (const auto& entry : meshes_) {
            const float centerX = (static_cast<float>(entry.first.x) + 0.5F) * stonewild::kChunkWidth;
            const float centerZ = (static_cast<float>(entry.first.z) + 0.5F) * stonewild::kChunkDepth;
            const float dx = centerX - eye.x;
            const float dz = centerZ - eye.z;
            if (dx * dx + dz * dz > drawDistance * drawDistance || entry.second.vertexCount == 0) continue;
            glBindVertexArray(entry.second.vertexArray);
            glDrawArrays(GL_TRIANGLES, 0, entry.second.vertexCount);
        }
        glBindVertexArray(0);
        renderTargetOutline(game, viewProjection);
        renderOverlay(game, width, height);
    }

    std::size_t gpuBytes() const { return gpuBytes_; }

private:
    void destroyMesh(GpuMesh& mesh) {
        if (mesh.vertexBuffer != 0) glDeleteBuffers(1, &mesh.vertexBuffer);
        if (mesh.vertexArray != 0) glDeleteVertexArrays(1, &mesh.vertexArray);
        gpuBytes_ -= std::min(gpuBytes_, mesh.bytes);
        mesh = {};
    }

    void uploadMesh(GpuMesh& gpu, const std::vector<Vertex>& vertices, std::uint64_t revision) {
        gpuBytes_ -= std::min(gpuBytes_, gpu.bytes);
        if (gpu.vertexArray == 0) glGenVertexArrays(1, &gpu.vertexArray);
        if (gpu.vertexBuffer == 0) glGenBuffers(1, &gpu.vertexBuffer);
        glBindVertexArray(gpu.vertexArray);
        glBindBuffer(GL_ARRAY_BUFFER, gpu.vertexBuffer);
        glBufferData(
            GL_ARRAY_BUFFER,
            static_cast<GLsizeiptr>(vertices.size() * sizeof(Vertex)),
            vertices.empty() ? nullptr : vertices.data(),
            GL_STATIC_DRAW);
        glEnableVertexAttribArray(0);
        glVertexAttribPointer(0, 3, GL_FLOAT, GL_FALSE, sizeof(Vertex), reinterpret_cast<void*>(offsetof(Vertex, x)));
        glEnableVertexAttribArray(1);
        glVertexAttribPointer(1, 3, GL_FLOAT, GL_FALSE, sizeof(Vertex), reinterpret_cast<void*>(offsetof(Vertex, nx)));
        glEnableVertexAttribArray(2);
        glVertexAttribPointer(2, 4, GL_FLOAT, GL_FALSE, sizeof(Vertex), reinterpret_cast<void*>(offsetof(Vertex, r)));
        glEnableVertexAttribArray(3);
        glVertexAttribPointer(3, 1, GL_FLOAT, GL_FALSE, sizeof(Vertex), reinterpret_cast<void*>(offsetof(Vertex, ambientOcclusion)));
        glBindVertexArray(0);
        gpu.vertexCount = static_cast<GLsizei>(vertices.size());
        gpu.revision = revision;
        gpu.bytes = vertices.size() * sizeof(Vertex);
        gpuBytes_ += gpu.bytes;
    }

    void syncMeshes(const Game& game) {
        struct Candidate {
            const Chunk* chunk;
            float distanceSquared;
        };
        std::set<ChunkPos> current;
        std::vector<Candidate> candidates;
        const Vec3 player = game.player().position;
        for (const Chunk* chunk : game.world().loadedChunks()) {
            current.insert(chunk->position());
            const auto iterator = meshes_.find(chunk->position());
            if (iterator != meshes_.end() && iterator->second.revision == chunk->revision()) continue;
            const float centerX = (static_cast<float>(chunk->position().x) + 0.5F) * stonewild::kChunkWidth;
            const float centerZ = (static_cast<float>(chunk->position().z) + 0.5F) * stonewild::kChunkDepth;
            const float dx = centerX - player.x;
            const float dz = centerZ - player.z;
            candidates.push_back({chunk, dx * dx + dz * dz});
        }
        for (auto iterator = meshes_.begin(); iterator != meshes_.end();) {
            if (current.count(iterator->first) == 0) {
                destroyMesh(iterator->second);
                iterator = meshes_.erase(iterator);
            } else {
                ++iterator;
            }
        }
        std::sort(candidates.begin(), candidates.end(), [](const Candidate& a, const Candidate& b) {
            return a.distanceSquared < b.distanceSquared;
        });
        const std::size_t meshBudget = game.quality() == 0 ? 1U : (game.quality() == 1 ? 2U : 4U);
        const std::size_t count = std::min(meshBudget, candidates.size());
        for (std::size_t index = 0; index < count; ++index) {
            const Chunk& chunk = *candidates[index].chunk;
            uploadMesh(meshes_[chunk.position()], buildChunkMesh(game.world(), chunk), chunk.revision());
        }
    }

    void renderTargetOutline(const Game& game, const Mat4& viewProjection) {
        const RayHit hit = raycastBlocks(
            game.world(), game.player().eyePosition(), game.player().lookDirection(), 6.0F);
        if (!hit.hit) return;
        constexpr float inset = 0.0025F;
        const float x0 = static_cast<float>(hit.block.x) - inset;
        const float y0 = static_cast<float>(hit.block.y) - inset;
        const float z0 = static_cast<float>(hit.block.z) - inset;
        const float x1 = static_cast<float>(hit.block.x + 1) + inset;
        const float y1 = static_cast<float>(hit.block.y + 1) + inset;
        const float z1 = static_cast<float>(hit.block.z + 1) + inset;
        const std::array<Vec3, 8> corner{{
            {x0, y0, z0}, {x1, y0, z0}, {x1, y1, z0}, {x0, y1, z0},
            {x0, y0, z1}, {x1, y0, z1}, {x1, y1, z1}, {x0, y1, z1},
        }};
        constexpr std::array<int, 24> edge{{
            0, 1, 1, 2, 2, 3, 3, 0, 4, 5, 5, 6, 6, 7, 7, 4, 0, 4, 1, 5, 2, 6, 3, 7,
        }};
        std::array<Vec3, 24> lines{};
        for (std::size_t index = 0; index < edge.size(); ++index) lines[index] = corner[static_cast<std::size_t>(edge[index])];
        glUseProgram(flatProgram_);
        glUniformMatrix4fv(flatViewProjection_, 1, GL_FALSE, viewProjection.value.data());
        glUniform4f(flatColor_, 1.0F, 0.86F, 0.28F, 1.0F);
        glBindVertexArray(outlineArray_);
        glBindBuffer(GL_ARRAY_BUFFER, outlineBuffer_);
        glBufferData(GL_ARRAY_BUFFER, sizeof(lines), lines.data(), GL_STREAM_DRAW);
        glLineWidth(2.0F);
        glDrawArrays(GL_LINES, 0, static_cast<GLsizei>(lines.size()));
        glBindVertexArray(0);
    }

    static void addRectangle(
        std::vector<OverlayVertex>& vertices,
        float left,
        float bottom,
        float right,
        float top,
        const std::array<float, 4>& color) {
        const std::array<OverlayVertex, 6> rectangle{{
            {left, bottom, color[0], color[1], color[2], color[3]},
            {right, bottom, color[0], color[1], color[2], color[3]},
            {right, top, color[0], color[1], color[2], color[3]},
            {left, bottom, color[0], color[1], color[2], color[3]},
            {right, top, color[0], color[1], color[2], color[3]},
            {left, top, color[0], color[1], color[2], color[3]},
        }};
        vertices.insert(vertices.end(), rectangle.begin(), rectangle.end());
    }

    void renderOverlay(const Game& game, int width, int height) {
        std::vector<OverlayVertex> vertices;
        vertices.reserve(160);
        const float pixelX = 2.0F / static_cast<float>(width);
        const float pixelY = 2.0F / static_cast<float>(height);
        addRectangle(vertices, -9.0F * pixelX, -1.0F * pixelY, 9.0F * pixelX, pixelY, {1.0F, 1.0F, 1.0F, 0.90F});
        addRectangle(vertices, -pixelX, -9.0F * pixelY, pixelX, 9.0F * pixelY, {1.0F, 1.0F, 1.0F, 0.90F});

        const float slotWidth = std::min(0.105F, 100.0F * pixelX);
        const float slotHeight = std::min(0.16F, 72.0F * pixelY);
        const float start = -slotWidth * 4.5F;
        for (std::size_t slot = 0; slot < 9; ++slot) {
            const float left = start + static_cast<float>(slot) * slotWidth;
            const bool selected = slot == game.selectedSlot();
            addRectangle(
                vertices,
                left + 0.003F,
                -0.96F,
                left + slotWidth - 0.003F,
                -0.96F + slotHeight,
                selected ? std::array<float, 4>{0.96F, 0.78F, 0.25F, 0.88F}
                         : std::array<float, 4>{0.04F, 0.05F, 0.07F, 0.72F});
            const auto& color = blockInfo(game.hotbarBlock(slot)).color;
            addRectangle(
                vertices,
                left + slotWidth * 0.23F,
                -0.96F + slotHeight * 0.23F,
                left + slotWidth * 0.77F,
                -0.96F + slotHeight * 0.77F,
                {color.x, color.y, color.z, 1.0F});
        }

        glDisable(GL_DEPTH_TEST);
        glDisable(GL_CULL_FACE);
        glUseProgram(overlayProgram_);
        glBindVertexArray(overlayArray_);
        glBindBuffer(GL_ARRAY_BUFFER, overlayBuffer_);
        glBufferData(
            GL_ARRAY_BUFFER,
            static_cast<GLsizeiptr>(vertices.size() * sizeof(OverlayVertex)),
            vertices.data(),
            GL_STREAM_DRAW);
        glDrawArrays(GL_TRIANGLES, 0, static_cast<GLsizei>(vertices.size()));
        glBindVertexArray(0);
        glEnable(GL_CULL_FACE);
        glEnable(GL_DEPTH_TEST);
    }

    GLuint worldProgram_ = 0;
    GLuint flatProgram_ = 0;
    GLuint overlayProgram_ = 0;
    GLint worldViewProjection_ = -1;
    GLint worldCamera_ = -1;
    GLint worldQuality_ = -1;
    GLint worldFogNear_ = -1;
    GLint worldFogFar_ = -1;
    GLint flatViewProjection_ = -1;
    GLint flatColor_ = -1;
    GLuint overlayArray_ = 0;
    GLuint overlayBuffer_ = 0;
    GLuint outlineArray_ = 0;
    GLuint outlineBuffer_ = 0;
    std::map<ChunkPos, GpuMesh> meshes_;
    std::size_t gpuBytes_ = 0;
};

struct LaunchOptions {
    int renderRadius = 2;
    int quality = 1;
    int windowWidth = 1280;
    int windowHeight = 720;
    int maxRenderRadius = 4;
    bool fullscreen = false;
    bool lowMemory = false;
    bool smokeTest = false;
    std::uint64_t seed = kDefaultSeed;
};

std::uint64_t physicalMemoryBytes() {
    struct sysinfo info {};
    if (sysinfo(&info) != 0) return 0;
    return static_cast<std::uint64_t>(info.totalram) * static_cast<std::uint64_t>(info.mem_unit);
}

std::size_t residentMemoryBytes() {
    std::ifstream stream("/proc/self/statm");
    std::size_t totalPages = 0;
    std::size_t residentPages = 0;
    if (!(stream >> totalPages >> residentPages)) return 0;
    (void)totalPages;
    const long pageSize = sysconf(_SC_PAGESIZE);
    return pageSize > 0 ? residentPages * static_cast<std::size_t>(pageSize) : 0;
}

void applyAutomaticProfile(LaunchOptions& options) {
    const std::uint64_t memory = physicalMemoryBytes();
    constexpr std::uint64_t gibibyte = 1024ULL * 1024ULL * 1024ULL;
    if (memory != 0 && memory <= 6ULL * gibibyte) {
        options.renderRadius = 1;
        options.quality = 0;
        options.maxRenderRadius = 3;
        options.lowMemory = true;
    } else if (memory >= 12ULL * gibibyte) {
        options.renderRadius = 3;
        options.quality = 2;
        options.maxRenderRadius = 6;
    }
}

void printHelp() {
    std::cout
        << "Stonewild " << kVersion << "\n"
        << "Usage: stonewild [options]\n\n"
        << "  --low                 force the 4 GB low-memory profile\n"
        << "  --balanced            use balanced visuals and distance\n"
        << "  --high                use high detail (8 GB+ recommended)\n"
        << "  --render-distance N   chunk radius from 1 to 6\n"
        << "  --quality N           shader detail from 0 to 2\n"
        << "  --seed N              unsigned 64-bit world seed\n"
        << "  --fullscreen          start fullscreen\n"
        << "  --smoke-test          render 90 frames and exit (CI/diagnostics)\n"
        << "  --version             print version and exit\n"
        << "  --help                show this help\n";
}

int parseInteger(const char* text, const char* option) {
    try {
        std::size_t used = 0;
        const int result = std::stoi(text, &used, 10);
        if (used != std::string(text).size()) throw std::invalid_argument("trailing");
        return result;
    } catch (const std::exception&) {
        throw std::runtime_error(std::string("invalid value for ") + option + ": " + text);
    }
}

LaunchOptions parseOptions(int argc, char** argv) {
    LaunchOptions options;
    applyAutomaticProfile(options);
    for (int index = 1; index < argc; ++index) {
        const std::string argument = argv[index];
        if (argument == "--help") {
            printHelp();
            std::exit(0);
        }
        if (argument == "--version") {
            std::cout << "Stonewild " << kVersion << '\n';
            std::exit(0);
        }
        if (argument == "--low") {
            options.renderRadius = 1;
            options.quality = 0;
            options.maxRenderRadius = 3;
            options.lowMemory = true;
        } else if (argument == "--balanced") {
            options.renderRadius = 2;
            options.quality = 1;
            options.maxRenderRadius = 4;
            options.lowMemory = false;
        } else if (argument == "--high") {
            options.renderRadius = 4;
            options.quality = 2;
            options.maxRenderRadius = 6;
            options.lowMemory = false;
        } else if (argument == "--fullscreen") {
            options.fullscreen = true;
        } else if (argument == "--smoke-test") {
            options.smokeTest = true;
        } else if (argument == "--render-distance" || argument == "--quality" || argument == "--seed") {
            if (++index >= argc) throw std::runtime_error("missing value for " + argument);
            if (argument == "--seed") {
                try {
                    std::size_t used = 0;
                    options.seed = std::stoull(argv[index], &used, 0);
                    if (used != std::string(argv[index]).size()) throw std::invalid_argument("trailing");
                } catch (const std::exception&) {
                    throw std::runtime_error(std::string("invalid value for --seed: ") + argv[index]);
                }
            } else {
                const int value = parseInteger(argv[index], argument.c_str());
                if (argument == "--render-distance") options.renderRadius = std::clamp(value, 1, 6);
                if (argument == "--quality") options.quality = std::clamp(value, 0, 2);
            }
        } else {
            throw std::runtime_error("unknown option: " + argument);
        }
    }
    return options;
}

std::filesystem::path dataDirectory() {
    if (const char* xdg = std::getenv("XDG_DATA_HOME")) return std::filesystem::path(xdg) / "stonewild";
    if (const char* home = std::getenv("HOME")) return std::filesystem::path(home) / ".local" / "share" / "stonewild";
    return std::filesystem::current_path() / ".stonewild";
}

std::string profileName(const Game& game) {
    if (game.renderRadius() == 1 && game.quality() == 0) return "Low-memory";
    if (game.renderRadius() >= 4 || game.quality() == 2) return "High";
    return "Balanced";
}

float controllerAxis(Sint16 raw, float deadzone = 0.16F) {
    const float value = raw >= 0 ? static_cast<float>(raw) / 32767.0F : static_cast<float>(raw) / 32768.0F;
    const float magnitude = std::abs(value);
    if (magnitude <= deadzone) return 0.0F;
    return std::copysign((magnitude - deadzone) / (1.0F - deadzone), value);
}

void captureMouse(bool capture) {
    SDL_SetRelativeMouseMode(capture ? SDL_TRUE : SDL_FALSE);
    SDL_ShowCursor(capture ? SDL_DISABLE : SDL_ENABLE);
}

void setWindowIcon(SDL_Window* window) {
    constexpr int size = 32;
    std::array<std::uint32_t, size * size> pixels{};
    for (int y = 0; y < size; ++y) {
        for (int x = 0; x < size; ++x) {
            const bool top = y < 10;
            const bool edge = x < 2 || y < 2 || x >= size - 2 || y >= size - 2;
            const std::uint8_t r = edge ? 34 : (top ? 62 : 99);
            const std::uint8_t g = edge ? 40 : (top ? 142 : 88);
            const std::uint8_t b = edge ? 46 : (top ? 55 : 58);
            pixels[static_cast<std::size_t>(y * size + x)] =
                0xff000000U | (static_cast<std::uint32_t>(r) << 16U) |
                (static_cast<std::uint32_t>(g) << 8U) | static_cast<std::uint32_t>(b);
        }
    }
    SDL_Surface* icon = SDL_CreateRGBSurfaceFrom(
        pixels.data(), size, size, 32, size * 4, 0x00ff0000U, 0x0000ff00U, 0x000000ffU, 0xff000000U);
    if (icon != nullptr) {
        SDL_SetWindowIcon(window, icon);
        SDL_FreeSurface(icon);
    }
}

void saveGame(const std::filesystem::path& path, const Game& game) {
    std::string error;
    if (!SaveSystem::save(path.string(), game, &error)) {
        std::cerr << "Stonewild: save failed: " << error << '\n';
    }
}

int runGame(const LaunchOptions& options) {
    if (SDL_Init(SDL_INIT_VIDEO | SDL_INIT_GAMECONTROLLER | SDL_INIT_EVENTS | SDL_INIT_TIMER) != 0) {
        throw std::runtime_error(std::string("SDL initialization failed: ") + SDL_GetError());
    }
    struct SdlGuard {
        ~SdlGuard() { SDL_Quit(); }
    } sdlGuard;

    SDL_GL_SetAttribute(SDL_GL_CONTEXT_MAJOR_VERSION, 3);
    SDL_GL_SetAttribute(SDL_GL_CONTEXT_MINOR_VERSION, 3);
    SDL_GL_SetAttribute(SDL_GL_CONTEXT_PROFILE_MASK, SDL_GL_CONTEXT_PROFILE_CORE);
    SDL_GL_SetAttribute(SDL_GL_DOUBLEBUFFER, 1);
    SDL_GL_SetAttribute(SDL_GL_DEPTH_SIZE, 24);
    SDL_GL_SetAttribute(SDL_GL_STENCIL_SIZE, 0);
    SDL_GL_SetAttribute(SDL_GL_FRAMEBUFFER_SRGB_CAPABLE, 0);
    const Uint32 flags = SDL_WINDOW_OPENGL | SDL_WINDOW_RESIZABLE | SDL_WINDOW_ALLOW_HIGHDPI |
                         (options.fullscreen ? static_cast<Uint32>(SDL_WINDOW_FULLSCREEN_DESKTOP) : 0U);
    std::unique_ptr<SDL_Window, decltype(&SDL_DestroyWindow)> window(
        SDL_CreateWindow(
            "Stonewild",
            SDL_WINDOWPOS_CENTERED,
            SDL_WINDOWPOS_CENTERED,
            options.windowWidth,
            options.windowHeight,
            flags),
        SDL_DestroyWindow);
    if (!window) throw std::runtime_error(std::string("window creation failed: ") + SDL_GetError());
    setWindowIcon(window.get());
    SDL_GLContext context = SDL_GL_CreateContext(window.get());
    if (context == nullptr) throw std::runtime_error(std::string("OpenGL 3.3 context failed: ") + SDL_GetError());
    struct ContextGuard {
        SDL_GLContext context;
        ~ContextGuard() { SDL_GL_DeleteContext(context); }
    } contextGuard{context};
    if (SDL_GL_MakeCurrent(window.get(), context) != 0) {
        throw std::runtime_error(std::string("OpenGL activation failed: ") + SDL_GetError());
    }
    if (SDL_GL_SetSwapInterval(-1) != 0) SDL_GL_SetSwapInterval(1);

    const std::filesystem::path directory = dataDirectory();
    std::filesystem::create_directories(directory);
    std::ostringstream seedName;
    seedName << "world-" << std::hex << options.seed << ".sav";
    const std::filesystem::path savePath = directory / seedName.str();
    auto game = std::make_unique<Game>(options.seed, options.renderRadius, options.quality);
    std::string loadError;
    if (std::filesystem::exists(savePath) &&
        !SaveSystem::load(
            savePath.string(),
            *game,
            &loadError,
            options.lowMemory ? options.renderRadius : 6,
            options.lowMemory ? options.quality : 2)) {
        std::cerr << "Stonewild: save was not loaded: " << loadError << '\n';
    }
    if (options.lowMemory) {
        game->setRenderRadius(std::min(game->renderRadius(), options.renderRadius));
        game->setQuality(std::min(game->quality(), options.quality));
    }

    DesktopRenderer renderer;
    renderer.initialize();
    struct RendererGuard {
        DesktopRenderer& renderer;
        ~RendererGuard() { renderer.shutdown(); }
    } rendererGuard{renderer};

    captureMouse(!options.smokeTest);
    bool mouseCaptured = !options.smokeTest;
    bool running = true;
    bool leftMouse = false;
    bool rightMouse = false;
    bool fullscreen = options.fullscreen;
    float accumulatedLookX = 0.0F;
    float accumulatedLookY = 0.0F;
    SDL_GameController* controller = nullptr;
    for (int index = 0; index < SDL_NumJoysticks(); ++index) {
        if (SDL_IsGameController(index) == SDL_TRUE) {
            controller = SDL_GameControllerOpen(index);
            if (controller != nullptr) break;
        }
    }

    auto previous = std::chrono::steady_clock::now();
    auto titleTime = previous;
    auto saveTime = previous;
    int framesSinceTitle = 0;
    int totalFrames = 0;
    while (running) {
        SDL_Event event{};
        while (SDL_PollEvent(&event) != 0) {
            if (event.type == SDL_QUIT) running = false;
            if (event.type == SDL_WINDOWEVENT && event.window.event == SDL_WINDOWEVENT_FOCUS_LOST) {
                leftMouse = false;
                rightMouse = false;
            }
            if (event.type == SDL_MOUSEMOTION && mouseCaptured) {
                accumulatedLookX += static_cast<float>(event.motion.xrel) * 0.0022F;
                accumulatedLookY -= static_cast<float>(event.motion.yrel) * 0.0022F;
            }
            if (event.type == SDL_MOUSEBUTTONDOWN) {
                if (!mouseCaptured) {
                    captureMouse(true);
                    mouseCaptured = true;
                } else if (event.button.button == SDL_BUTTON_LEFT) {
                    leftMouse = true;
                } else if (event.button.button == SDL_BUTTON_RIGHT) {
                    rightMouse = true;
                }
            }
            if (event.type == SDL_MOUSEBUTTONUP) {
                if (event.button.button == SDL_BUTTON_LEFT) leftMouse = false;
                if (event.button.button == SDL_BUTTON_RIGHT) rightMouse = false;
            }
            if (event.type == SDL_MOUSEWHEEL) {
                const int direction = event.wheel.y > 0 ? -1 : (event.wheel.y < 0 ? 1 : 0);
                const int slot = (static_cast<int>(game->selectedSlot()) + direction + 9) % 9;
                game->chooseHotbarSlot(static_cast<std::size_t>(slot));
            }
            if (event.type == SDL_CONTROLLERDEVICEADDED && controller == nullptr) {
                controller = SDL_GameControllerOpen(event.cdevice.which);
            }
            if (event.type == SDL_CONTROLLERDEVICEREMOVED && controller != nullptr &&
                SDL_JoystickInstanceID(SDL_GameControllerGetJoystick(controller)) == event.cdevice.which) {
                SDL_GameControllerClose(controller);
                controller = nullptr;
            }
            if (event.type == SDL_CONTROLLERBUTTONDOWN) {
                if (event.cbutton.button == SDL_CONTROLLER_BUTTON_DPAD_LEFT ||
                    event.cbutton.button == SDL_CONTROLLER_BUTTON_DPAD_RIGHT) {
                    const int direction = event.cbutton.button == SDL_CONTROLLER_BUTTON_DPAD_LEFT ? -1 : 1;
                    const int slot = (static_cast<int>(game->selectedSlot()) + direction + 9) % 9;
                    game->chooseHotbarSlot(static_cast<std::size_t>(slot));
                } else if (event.cbutton.button == SDL_CONTROLLER_BUTTON_BACK) {
                    captureMouse(!mouseCaptured);
                    mouseCaptured = !mouseCaptured;
                    leftMouse = false;
                    rightMouse = false;
                }
            }
            if (event.type == SDL_KEYDOWN && event.key.repeat == 0) {
                const SDL_Keycode key = event.key.keysym.sym;
                const Uint16 modifiers = event.key.keysym.mod;
                if (key == SDLK_ESCAPE) {
                    if (mouseCaptured) {
                        captureMouse(false);
                        mouseCaptured = false;
                        leftMouse = false;
                        rightMouse = false;
                    } else {
                        running = false;
                    }
                } else if (key == SDLK_q && (modifiers & KMOD_CTRL) != 0U) {
                    running = false;
                } else if (key >= SDLK_1 && key <= SDLK_9) {
                    game->chooseHotbarSlot(static_cast<std::size_t>(key - SDLK_1));
                } else if (key == SDLK_F5) {
                    saveGame(savePath, *game);
                } else if (key == SDLK_F2) {
                    game->setQuality((game->quality() + 1) % 3);
                } else if (key == SDLK_F3) {
                    const int next = game->renderRadius() >= options.maxRenderRadius ? 1 : game->renderRadius() + 1;
                    game->setRenderRadius(next);
                } else if (key == SDLK_F11) {
                    fullscreen = !fullscreen;
                    SDL_SetWindowFullscreen(
                        window.get(),
                        fullscreen ? static_cast<Uint32>(SDL_WINDOW_FULLSCREEN_DESKTOP) : 0U);
                }
            }
        }

        const auto now = std::chrono::steady_clock::now();
        const float delta = std::chrono::duration<float>(now - previous).count();
        previous = now;
        InputState input{};
        const Uint8* keys = SDL_GetKeyboardState(nullptr);
        input.forward = static_cast<float>(keys[SDL_SCANCODE_W] || keys[SDL_SCANCODE_UP]) -
                        static_cast<float>(keys[SDL_SCANCODE_S] || keys[SDL_SCANCODE_DOWN]);
        input.right = static_cast<float>(keys[SDL_SCANCODE_D] || keys[SDL_SCANCODE_RIGHT]) -
                      static_cast<float>(keys[SDL_SCANCODE_A] || keys[SDL_SCANCODE_LEFT]);
        input.jump = keys[SDL_SCANCODE_SPACE] != 0;
        input.sprint = keys[SDL_SCANCODE_LSHIFT] != 0 || keys[SDL_SCANCODE_RSHIFT] != 0;
        input.breakBlock = leftMouse;
        input.placeBlock = rightMouse;
        input.lookX = accumulatedLookX;
        input.lookY = accumulatedLookY;
        accumulatedLookX = 0.0F;
        accumulatedLookY = 0.0F;

        if (controller != nullptr) {
            input.right = clamp(input.right + controllerAxis(SDL_GameControllerGetAxis(controller, SDL_CONTROLLER_AXIS_LEFTX)), -1.0F, 1.0F);
            input.forward = clamp(input.forward - controllerAxis(SDL_GameControllerGetAxis(controller, SDL_CONTROLLER_AXIS_LEFTY)), -1.0F, 1.0F);
            input.lookX += controllerAxis(SDL_GameControllerGetAxis(controller, SDL_CONTROLLER_AXIS_RIGHTX)) * delta * 2.4F;
            input.lookY -= controllerAxis(SDL_GameControllerGetAxis(controller, SDL_CONTROLLER_AXIS_RIGHTY)) * delta * 2.0F;
            input.jump = input.jump || SDL_GameControllerGetButton(controller, SDL_CONTROLLER_BUTTON_A) != 0;
            input.sprint = input.sprint || SDL_GameControllerGetButton(controller, SDL_CONTROLLER_BUTTON_LEFTSTICK) != 0;
            input.breakBlock = input.breakBlock || SDL_GameControllerGetButton(controller, SDL_CONTROLLER_BUTTON_RIGHTSHOULDER) != 0;
            input.placeBlock = input.placeBlock || SDL_GameControllerGetButton(controller, SDL_CONTROLLER_BUTTON_LEFTSHOULDER) != 0;
        }

        if (mouseCaptured || options.smokeTest) game->update(input, delta);
        renderer.render(window.get(), *game);
        SDL_GL_SwapWindow(window.get());
        ++framesSinceTitle;
        ++totalFrames;
        if (options.smokeTest && totalFrames >= 90) running = false;

        const float titleElapsed = std::chrono::duration<float>(now - titleTime).count();
        if (titleElapsed >= 0.5F) {
            const int fps = static_cast<int>(std::lround(static_cast<float>(framesSinceTitle) / titleElapsed));
            const std::size_t rssMiB = residentMemoryBytes() / (1024U * 1024U);
            const std::size_t gpuMiB = renderer.gpuBytes() / (1024U * 1024U);
            std::ostringstream title;
            title << "Stonewild | " << fps << " FPS | " << profileName(*game)
                  << " | RAM " << rssMiB << " MiB | GPU mesh " << gpuMiB << " MiB"
                  << " | " << game->world().loadedChunkCount() << " chunks"
                  << " | [" << game->selectedSlot() + 1 << "] " << blockInfo(game->selectedBlock()).name
                  << " | F2 detail  F3 distance  F5 save";
            SDL_SetWindowTitle(window.get(), title.str().c_str());
            framesSinceTitle = 0;
            titleTime = now;
        }
        if (now - saveTime >= std::chrono::seconds(30)) {
            saveGame(savePath, *game);
            saveTime = now;
        }
    }

    saveGame(savePath, *game);
    if (controller != nullptr) SDL_GameControllerClose(controller);
    captureMouse(false);
    return 0;
}

}  // namespace

int main(int argc, char** argv) {
    try {
        return runGame(parseOptions(argc, argv));
    } catch (const std::exception& exception) {
        std::cerr << "Stonewild: " << exception.what() << '\n';
        if (SDL_WasInit(SDL_INIT_VIDEO) != 0U) {
            SDL_ShowSimpleMessageBox(SDL_MESSAGEBOX_ERROR, "Stonewild could not start", exception.what(), nullptr);
        }
        return 1;
    }
}
