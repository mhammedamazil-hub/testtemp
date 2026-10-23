#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES3/gl3.h>
#include <android/input.h>
#include <android/keycodes.h>
#include <android/log.h>
#include <android/native_activity.h>
#include <android_native_app_glue.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <map>
#include <set>
#include <string>
#include <thread>
#include <vector>

#include "stonewild/game.hpp"
#include "stonewild/mesh.hpp"

#define SW_LOGI(...) __android_log_print(ANDROID_LOG_INFO, "Stonewild", __VA_ARGS__)
#define SW_LOGE(...) __android_log_print(ANDROID_LOG_ERROR, "Stonewild", __VA_ARGS__)

namespace {

using stonewild::Block;
using stonewild::Chunk;
using stonewild::ChunkPos;
using stonewild::Game;
using stonewild::InputState;
using stonewild::SaveSystem;
using stonewild::Vec3;
using stonewild::Vertex;
using stonewild::buildChunkMesh;
using stonewild::clamp;
using stonewild::dot;
using stonewild::normalize;

constexpr float kPi = 3.14159265358979323846F;

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
            float total = 0.0F;
            for (int inner = 0; inner < 4; ++inner) {
                total += a.value[static_cast<std::size_t>(inner * 4 + row)] *
                         b.value[static_cast<std::size_t>(column * 4 + inner)];
            }
            result.value[static_cast<std::size_t>(column * 4 + row)] = total;
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
    if (compiled == GL_FALSE) {
        GLint length = 0;
        glGetShaderiv(shader, GL_INFO_LOG_LENGTH, &length);
        std::string log(static_cast<std::size_t>(std::max(length, 1)), '\0');
        glGetShaderInfoLog(shader, length, nullptr, log.data());
        SW_LOGE("shader compile failed: %s", log.c_str());
        glDeleteShader(shader);
        return 0;
    }
    return shader;
}

GLuint createProgram(const char* vertexSource, const char* fragmentSource) {
    const GLuint vertex = compileShader(GL_VERTEX_SHADER, vertexSource);
    const GLuint fragment = compileShader(GL_FRAGMENT_SHADER, fragmentSource);
    if (vertex == 0 || fragment == 0) {
        if (vertex != 0) glDeleteShader(vertex);
        if (fragment != 0) glDeleteShader(fragment);
        return 0;
    }
    const GLuint program = glCreateProgram();
    glAttachShader(program, vertex);
    glAttachShader(program, fragment);
    glLinkProgram(program);
    glDeleteShader(vertex);
    glDeleteShader(fragment);
    GLint linked = GL_FALSE;
    glGetProgramiv(program, GL_LINK_STATUS, &linked);
    if (linked == GL_FALSE) {
        GLint length = 0;
        glGetProgramiv(program, GL_INFO_LOG_LENGTH, &length);
        std::string log(static_cast<std::size_t>(std::max(length, 1)), '\0');
        glGetProgramInfoLog(program, length, nullptr, log.data());
        SW_LOGE("shader link failed: %s", log.c_str());
        glDeleteProgram(program);
        return 0;
    }
    return program;
}

struct GpuMesh {
    GLuint vertexArray = 0;
    GLuint vertexBuffer = 0;
    GLsizei vertexCount = 0;
    std::uint64_t revision = 0;
};

struct OverlayVertex {
    float x;
    float y;
    float r;
    float g;
    float b;
    float a;
};

class Renderer {
public:
    bool initialize(ANativeWindow* window) {
        destroy();
        display_ = eglGetDisplay(EGL_DEFAULT_DISPLAY);
        if (display_ == EGL_NO_DISPLAY || eglInitialize(display_, nullptr, nullptr) == EGL_FALSE) {
            SW_LOGE("EGL display initialization failed");
            return false;
        }
        constexpr EGLint attributes[] = {
            EGL_SURFACE_TYPE, EGL_WINDOW_BIT,
            EGL_RENDERABLE_TYPE, EGL_OPENGL_ES3_BIT_KHR,
            EGL_RED_SIZE, 8,
            EGL_GREEN_SIZE, 8,
            EGL_BLUE_SIZE, 8,
            EGL_ALPHA_SIZE, 8,
            EGL_DEPTH_SIZE, 24,
            EGL_NONE,
        };
        EGLConfig config = nullptr;
        EGLint count = 0;
        if (eglChooseConfig(display_, attributes, &config, 1, &count) == EGL_FALSE || count < 1) {
            SW_LOGE("no GLES 3 EGL configuration");
            destroy();
            return false;
        }
        EGLint visual = 0;
        eglGetConfigAttrib(display_, config, EGL_NATIVE_VISUAL_ID, &visual);
        ANativeWindow_setBuffersGeometry(window, 0, 0, visual);
        surface_ = eglCreateWindowSurface(display_, config, window, nullptr);
        constexpr EGLint contextAttributes[] = {EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE};
        context_ = eglCreateContext(display_, config, EGL_NO_CONTEXT, contextAttributes);
        if (surface_ == EGL_NO_SURFACE || context_ == EGL_NO_CONTEXT ||
            eglMakeCurrent(display_, surface_, surface_, context_) == EGL_FALSE) {
            SW_LOGE("EGL surface/context creation failed");
            destroy();
            return false;
        }
        eglSwapInterval(display_, 1);
        if (!initializePrograms()) {
            destroy();
            return false;
        }
        glEnable(GL_DEPTH_TEST);
        glEnable(GL_CULL_FACE);
        glCullFace(GL_BACK);
        glEnable(GL_BLEND);
        glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
        ready_ = true;
        resize();
        SW_LOGI(
            "renderer initialized: %dx%d, GL=%s",
            width_,
            height_,
            reinterpret_cast<const char*>(glGetString(GL_VERSION)));
        return true;
    }

    void destroy() {
        if (display_ != EGL_NO_DISPLAY && context_ != EGL_NO_CONTEXT) {
            eglMakeCurrent(display_, surface_, surface_, context_);
            for (auto& entry : meshes_) destroyMesh(entry.second);
            meshes_.clear();
            if (overlayBuffer_ != 0) glDeleteBuffers(1, &overlayBuffer_);
            if (overlayArray_ != 0) glDeleteVertexArrays(1, &overlayArray_);
            if (worldProgram_ != 0) glDeleteProgram(worldProgram_);
            if (overlayProgram_ != 0) glDeleteProgram(overlayProgram_);
        }
        overlayBuffer_ = 0;
        overlayArray_ = 0;
        worldProgram_ = 0;
        overlayProgram_ = 0;
        if (display_ != EGL_NO_DISPLAY) {
            eglMakeCurrent(display_, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
            if (context_ != EGL_NO_CONTEXT) eglDestroyContext(display_, context_);
            if (surface_ != EGL_NO_SURFACE) eglDestroySurface(display_, surface_);
            eglTerminate(display_);
        }
        display_ = EGL_NO_DISPLAY;
        surface_ = EGL_NO_SURFACE;
        context_ = EGL_NO_CONTEXT;
        ready_ = false;
    }

    ~Renderer() { destroy(); }

    bool ready() const { return ready_; }
    int width() const { return width_; }
    int height() const { return height_; }

    void render(const Game& game) {
        if (!ready_) return;
        resize();
        syncMeshes(game);
        glViewport(0, 0, width_, height_);
        glClearColor(0.40F, 0.66F, 0.88F, 1.0F);
        glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);

        const Vec3 eye = game.player().eyePosition();
        const Vec3 direction = game.player().lookDirection();
        const float aspect = static_cast<float>(std::max(width_, 1)) /
                             static_cast<float>(std::max(height_, 1));
        const Mat4 projection = perspective(68.0F * kPi / 180.0F, aspect, 0.08F, 220.0F);
        const Mat4 view = lookAt(eye, eye + direction, {0.0F, 1.0F, 0.0F});
        const Mat4 viewProjection = multiply(projection, view);

        glEnable(GL_DEPTH_TEST);
        glEnable(GL_CULL_FACE);
        glUseProgram(worldProgram_);
        glUniformMatrix4fv(worldViewProjection_, 1, GL_FALSE, viewProjection.value.data());
        glUniform3f(worldCamera_, eye.x, eye.y, eye.z);
        glUniform1i(worldQuality_, game.quality());
        for (const auto& entry : meshes_) {
            if (entry.second.vertexCount == 0) continue;
            glBindVertexArray(entry.second.vertexArray);
            glDrawArrays(GL_TRIANGLES, 0, entry.second.vertexCount);
        }
        glBindVertexArray(0);
        renderOverlay(game.selectedSlot());
        if (eglSwapBuffers(display_, surface_) == EGL_FALSE) {
            SW_LOGE("eglSwapBuffers failed: 0x%x", eglGetError());
        }
    }

private:
    bool initializePrograms() {
        constexpr const char* worldVertex = R"GLSL(#version 300 es
precision highp float;
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
        constexpr const char* worldFragment = R"GLSL(#version 300 es
precision highp float;
in vec3 vPosition;
in vec3 vNormal;
in vec4 vColor;
in float vAo;
uniform vec3 uCamera;
uniform int uQuality;
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
    float fogStart = uQuality == 0 ? 42.0 : 58.0;
    float fog = smoothstep(fogStart, 110.0, distanceToCamera);
    vec3 sky = vec3(0.40, 0.66, 0.88);
    fragColor = vec4(mix(color, sky, fog), vColor.a);
}
)GLSL";
        constexpr const char* overlayVertex = R"GLSL(#version 300 es
precision mediump float;
layout(location=0) in vec2 aPosition;
layout(location=1) in vec4 aColor;
out vec4 vColor;
void main() {
    vColor = aColor;
    gl_Position = vec4(aPosition, 0.0, 1.0);
}
)GLSL";
        constexpr const char* overlayFragment = R"GLSL(#version 300 es
precision mediump float;
in vec4 vColor;
out vec4 fragColor;
void main() { fragColor = vColor; }
)GLSL";
        worldProgram_ = createProgram(worldVertex, worldFragment);
        overlayProgram_ = createProgram(overlayVertex, overlayFragment);
        if (worldProgram_ == 0 || overlayProgram_ == 0) return false;
        worldViewProjection_ = glGetUniformLocation(worldProgram_, "uViewProjection");
        worldCamera_ = glGetUniformLocation(worldProgram_, "uCamera");
        worldQuality_ = glGetUniformLocation(worldProgram_, "uQuality");
        glGenVertexArrays(1, &overlayArray_);
        glGenBuffers(1, &overlayBuffer_);
        glBindVertexArray(overlayArray_);
        glBindBuffer(GL_ARRAY_BUFFER, overlayBuffer_);
        glEnableVertexAttribArray(0);
        glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, sizeof(OverlayVertex), reinterpret_cast<void*>(0));
        glEnableVertexAttribArray(1);
        glVertexAttribPointer(
            1,
            4,
            GL_FLOAT,
            GL_FALSE,
            sizeof(OverlayVertex),
            reinterpret_cast<void*>(offsetof(OverlayVertex, r)));
        glBindVertexArray(0);
        return true;
    }

    void resize() {
        EGLint width = 0;
        EGLint height = 0;
        eglQuerySurface(display_, surface_, EGL_WIDTH, &width);
        eglQuerySurface(display_, surface_, EGL_HEIGHT, &height);
        width_ = std::max(width, 1);
        height_ = std::max(height, 1);
    }

    void destroyMesh(GpuMesh& mesh) {
        if (mesh.vertexBuffer != 0) glDeleteBuffers(1, &mesh.vertexBuffer);
        if (mesh.vertexArray != 0) glDeleteVertexArrays(1, &mesh.vertexArray);
        mesh = {};
    }

    void uploadMesh(GpuMesh& gpu, const std::vector<Vertex>& vertices, std::uint64_t revision) {
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
        glVertexAttribPointer(
            3,
            1,
            GL_FLOAT,
            GL_FALSE,
            sizeof(Vertex),
            reinterpret_cast<void*>(offsetof(Vertex, ambientOcclusion)));
        glBindVertexArray(0);
        gpu.vertexCount = static_cast<GLsizei>(vertices.size());
        gpu.revision = revision;
    }

    void syncMeshes(const Game& game) {
        std::set<ChunkPos> current;
        for (const Chunk* chunk : game.world().loadedChunks()) {
            current.insert(chunk->position());
            GpuMesh& gpu = meshes_[chunk->position()];
            if (gpu.revision != chunk->revision()) {
                uploadMesh(gpu, buildChunkMesh(game.world(), *chunk), chunk->revision());
            }
        }
        for (auto iterator = meshes_.begin(); iterator != meshes_.end();) {
            if (current.count(iterator->first) == 0) {
                destroyMesh(iterator->second);
                iterator = meshes_.erase(iterator);
            } else {
                ++iterator;
            }
        }
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

    void addCircle(
        std::vector<OverlayVertex>& vertices,
        float centerX,
        float centerY,
        float radiusX,
        const std::array<float, 4>& color) const {
        constexpr int segments = 24;
        const float radiusY = radiusX * static_cast<float>(width_) / static_cast<float>(height_);
        for (int segment = 0; segment < segments; ++segment) {
            const float angleA = 2.0F * kPi * static_cast<float>(segment) / static_cast<float>(segments);
            const float angleB = 2.0F * kPi * static_cast<float>(segment + 1) / static_cast<float>(segments);
            vertices.push_back({centerX, centerY, color[0], color[1], color[2], color[3]});
            vertices.push_back({
                centerX + std::cos(angleA) * radiusX,
                centerY + std::sin(angleA) * radiusY,
                color[0], color[1], color[2], color[3]});
            vertices.push_back({
                centerX + std::cos(angleB) * radiusX,
                centerY + std::sin(angleB) * radiusY,
                color[0], color[1], color[2], color[3]});
        }
    }

    void renderOverlay(std::size_t selectedSlot) {
        std::vector<OverlayVertex> vertices;
        vertices.reserve(512);
        addCircle(vertices, -0.70F, -0.55F, 0.16F, {0.08F, 0.10F, 0.12F, 0.34F});
        addCircle(vertices, 0.76F, -0.30F, 0.075F, {0.18F, 0.45F, 0.90F, 0.58F});
        addCircle(vertices, 0.55F, -0.62F, 0.075F, {0.88F, 0.24F, 0.18F, 0.58F});
        addCircle(vertices, 0.82F, -0.68F, 0.075F, {0.20F, 0.65F, 0.88F, 0.58F});
        addRectangle(vertices, -0.012F, -0.0015F, 0.012F, 0.0015F, {1.0F, 1.0F, 1.0F, 0.9F});
        addRectangle(vertices, -0.0015F, -0.018F, 0.0015F, 0.018F, {1.0F, 1.0F, 1.0F, 0.9F});

        constexpr float slotWidth = 0.105F;
        const float start = -slotWidth * 4.5F;
        for (std::size_t slot = 0; slot < 9; ++slot) {
            const float left = start + static_cast<float>(slot) * slotWidth;
            const bool selected = slot == selectedSlot;
            addRectangle(
                vertices,
                left + 0.004F,
                0.84F,
                left + slotWidth - 0.004F,
                0.96F,
                selected ? std::array<float, 4>{0.95F, 0.78F, 0.24F, 0.72F}
                         : std::array<float, 4>{0.07F, 0.08F, 0.10F, 0.46F});
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
            GL_DYNAMIC_DRAW);
        glDrawArrays(GL_TRIANGLES, 0, static_cast<GLsizei>(vertices.size()));
        glBindVertexArray(0);
        glEnable(GL_CULL_FACE);
        glEnable(GL_DEPTH_TEST);
    }

    EGLDisplay display_ = EGL_NO_DISPLAY;
    EGLSurface surface_ = EGL_NO_SURFACE;
    EGLContext context_ = EGL_NO_CONTEXT;
    bool ready_ = false;
    int width_ = 1;
    int height_ = 1;
    GLuint worldProgram_ = 0;
    GLuint overlayProgram_ = 0;
    GLint worldViewProjection_ = -1;
    GLint worldCamera_ = -1;
    GLint worldQuality_ = -1;
    GLuint overlayArray_ = 0;
    GLuint overlayBuffer_ = 0;
    std::map<ChunkPos, GpuMesh> meshes_;
};

enum class TouchRole {
    Move,
    Look,
    Jump,
    Break,
    Place,
};

struct TouchPoint {
    TouchRole role = TouchRole::Look;
    float originX = 0.0F;
    float originY = 0.0F;
    float lastX = 0.0F;
    float lastY = 0.0F;
};

class TouchControls {
public:
    bool handle(AInputEvent* event, int width, int height) {
        const int action = AMotionEvent_getAction(event);
        const int masked = action & AMOTION_EVENT_ACTION_MASK;
        const std::size_t actionIndex = static_cast<std::size_t>(
            (action & AMOTION_EVENT_ACTION_POINTER_INDEX_MASK) >> AMOTION_EVENT_ACTION_POINTER_INDEX_SHIFT);
        if (masked == AMOTION_EVENT_ACTION_CANCEL) {
            points_.clear();
            state_ = {};
            return true;
        }
        if (masked == AMOTION_EVENT_ACTION_DOWN || masked == AMOTION_EVENT_ACTION_POINTER_DOWN) {
            beginPointer(event, actionIndex, width, height);
            return true;
        }
        if (masked == AMOTION_EVENT_ACTION_UP || masked == AMOTION_EVENT_ACTION_POINTER_UP) {
            endPointer(event, actionIndex);
            return true;
        }
        if (masked == AMOTION_EVENT_ACTION_MOVE) {
            const std::size_t count = AMotionEvent_getPointerCount(event);
            for (std::size_t index = 0; index < count; ++index) updatePointer(event, index, width);
            return true;
        }
        return false;
    }

    InputState consume() {
        InputState result = state_;
        state_.lookX = 0.0F;
        state_.lookY = 0.0F;
        return result;
    }

    int consumeSelectedSlot() {
        const int result = selectedSlot_;
        selectedSlot_ = -1;
        return result;
    }

private:
    static bool inCircle(float x, float y, float cx, float cy, float radius) {
        const float dx = x - cx;
        const float dy = y - cy;
        return dx * dx + dy * dy <= radius * radius;
    }

    void beginPointer(AInputEvent* event, std::size_t index, int width, int height) {
        const int id = AMotionEvent_getPointerId(event, index);
        const float x = AMotionEvent_getX(event, index);
        const float y = AMotionEvent_getY(event, index);
        if (y < static_cast<float>(height) * 0.14F) {
            selectedSlot_ = std::clamp(static_cast<int>(x / (static_cast<float>(width) / 9.0F)), 0, 8);
            return;
        }
        TouchRole role = TouchRole::Look;
        const float radius = static_cast<float>(std::min(width, height)) * 0.10F;
        if (x < static_cast<float>(width) * 0.46F) {
            role = TouchRole::Move;
        } else if (inCircle(x, y, width * 0.88F, height * 0.65F, radius)) {
            role = TouchRole::Jump;
            state_.jump = true;
        } else if (inCircle(x, y, width * 0.78F, height * 0.81F, radius)) {
            role = TouchRole::Break;
            state_.breakBlock = true;
        } else if (inCircle(x, y, width * 0.91F, height * 0.83F, radius)) {
            role = TouchRole::Place;
            state_.placeBlock = true;
        }
        points_[id] = {role, x, y, x, y};
    }

    void updatePointer(AInputEvent* event, std::size_t index, int width) {
        const int id = AMotionEvent_getPointerId(event, index);
        const auto iterator = points_.find(id);
        if (iterator == points_.end()) return;
        TouchPoint& point = iterator->second;
        const float x = AMotionEvent_getX(event, index);
        const float y = AMotionEvent_getY(event, index);
        if (point.role == TouchRole::Move) {
            const float radius = static_cast<float>(width) * 0.12F;
            state_.right = clamp((x - point.originX) / radius, -1.0F, 1.0F);
            state_.forward = clamp((point.originY - y) / radius, -1.0F, 1.0F);
        } else if (point.role == TouchRole::Look) {
            state_.lookX += (x - point.lastX) * 0.0032F;
            state_.lookY -= (y - point.lastY) * 0.0032F;
        }
        point.lastX = x;
        point.lastY = y;
    }

    void endPointer(AInputEvent* event, std::size_t index) {
        const int id = AMotionEvent_getPointerId(event, index);
        const auto iterator = points_.find(id);
        if (iterator == points_.end()) return;
        switch (iterator->second.role) {
        case TouchRole::Move:
            state_.forward = 0.0F;
            state_.right = 0.0F;
            break;
        case TouchRole::Jump:
            state_.jump = false;
            break;
        case TouchRole::Break:
            state_.breakBlock = false;
            break;
        case TouchRole::Place:
            state_.placeBlock = false;
            break;
        case TouchRole::Look:
            break;
        }
        points_.erase(iterator);
    }

    std::map<int, TouchPoint> points_;
    InputState state_{};
    int selectedSlot_ = -1;
};

struct EngineState {
    explicit EngineState(android_app* application)
        : app(application), savePath(std::string(application->activity->internalDataPath) + "/stonewild-world.sav") {}

    void save() {
        std::string error;
        if (!SaveSystem::save(savePath, game, &error)) SW_LOGE("save failed: %s", error.c_str());
    }

    android_app* app;
    Game game;
    Renderer renderer;
    TouchControls touch;
    InputState hardware{};
    bool focused = false;
    std::string savePath;
};

void handleAppCommand(android_app* app, int32_t command) {
    auto* state = static_cast<EngineState*>(app->userData);
    if (state == nullptr) return;
    switch (command) {
    case APP_CMD_INIT_WINDOW:
        if (app->window != nullptr) state->renderer.initialize(app->window);
        break;
    case APP_CMD_TERM_WINDOW:
        state->renderer.destroy();
        break;
    case APP_CMD_GAINED_FOCUS:
        state->focused = true;
        break;
    case APP_CMD_LOST_FOCUS:
        state->focused = false;
        state->save();
        break;
    case APP_CMD_PAUSE:
    case APP_CMD_SAVE_STATE:
        state->save();
        break;
    default:
        break;
    }
}

void updateKey(InputState& input, int32_t keyCode, bool pressed) {
    switch (keyCode) {
    case AKEYCODE_W:
    case AKEYCODE_DPAD_UP:
        input.forward = pressed ? 1.0F : 0.0F;
        break;
    case AKEYCODE_S:
    case AKEYCODE_DPAD_DOWN:
        input.forward = pressed ? -1.0F : 0.0F;
        break;
    case AKEYCODE_A:
    case AKEYCODE_DPAD_LEFT:
        input.right = pressed ? -1.0F : 0.0F;
        break;
    case AKEYCODE_D:
    case AKEYCODE_DPAD_RIGHT:
        input.right = pressed ? 1.0F : 0.0F;
        break;
    case AKEYCODE_SPACE:
    case AKEYCODE_BUTTON_A:
        input.jump = pressed;
        break;
    case AKEYCODE_SHIFT_LEFT:
    case AKEYCODE_SHIFT_RIGHT:
    case AKEYCODE_BUTTON_THUMBL:
        input.sprint = pressed;
        break;
    case AKEYCODE_F:
    case AKEYCODE_BUTTON_R1:
        input.breakBlock = pressed;
        break;
    case AKEYCODE_G:
    case AKEYCODE_BUTTON_L1:
        input.placeBlock = pressed;
        break;
    default:
        break;
    }
}

int32_t handleInput(android_app* app, AInputEvent* event) {
    auto* state = static_cast<EngineState*>(app->userData);
    if (state == nullptr) return 0;
    const int32_t type = AInputEvent_getType(event);
    const int32_t source = AInputEvent_getSource(event);
    if (type == AINPUT_EVENT_TYPE_MOTION &&
        (source & AINPUT_SOURCE_TOUCHSCREEN) == AINPUT_SOURCE_TOUCHSCREEN) {
        return state->touch.handle(event, state->renderer.width(), state->renderer.height()) ? 1 : 0;
    }
    if (type == AINPUT_EVENT_TYPE_MOTION &&
        ((source & AINPUT_SOURCE_JOYSTICK) == AINPUT_SOURCE_JOYSTICK ||
         (source & AINPUT_SOURCE_GAMEPAD) == AINPUT_SOURCE_GAMEPAD)) {
        state->hardware.right = AMotionEvent_getAxisValue(event, AMOTION_EVENT_AXIS_X, 0);
        state->hardware.forward = -AMotionEvent_getAxisValue(event, AMOTION_EVENT_AXIS_Y, 0);
        state->hardware.lookX = AMotionEvent_getAxisValue(event, AMOTION_EVENT_AXIS_Z, 0);
        state->hardware.lookY = -AMotionEvent_getAxisValue(event, AMOTION_EVENT_AXIS_RZ, 0);
        return 1;
    }
    if (type == AINPUT_EVENT_TYPE_KEY) {
        const int32_t action = AKeyEvent_getAction(event);
        if (action == AKEY_EVENT_ACTION_DOWN || action == AKEY_EVENT_ACTION_UP) {
            updateKey(state->hardware, AKeyEvent_getKeyCode(event), action == AKEY_EVENT_ACTION_DOWN);
            return 1;
        }
    }
    return 0;
}

InputState combinedInput(EngineState& state, float deltaSeconds) {
    InputState input = state.touch.consume();
    input.forward = clamp(input.forward + state.hardware.forward, -1.0F, 1.0F);
    input.right = clamp(input.right + state.hardware.right, -1.0F, 1.0F);
    input.lookX += state.hardware.lookX * deltaSeconds * 2.2F;
    input.lookY += state.hardware.lookY * deltaSeconds * 2.2F;
    input.jump = input.jump || state.hardware.jump;
    input.sprint = input.sprint || state.hardware.sprint;
    input.breakBlock = input.breakBlock || state.hardware.breakBlock;
    input.placeBlock = input.placeBlock || state.hardware.placeBlock;
    return input;
}

}  // namespace

void android_main(android_app* app) {
    app_dummy();
    ANativeActivity_setWindowFlags(
        app->activity,
        AWINDOW_FLAG_FULLSCREEN | AWINDOW_FLAG_KEEP_SCREEN_ON,
        0);
    EngineState state(app);
    app->userData = &state;
    app->onAppCmd = handleAppCommand;
    app->onInputEvent = handleInput;

    std::string loadError;
    if (SaveSystem::load(state.savePath, state.game, &loadError)) {
        SW_LOGI("loaded saved world");
    } else {
        SW_LOGI("starting world without save: %s", loadError.c_str());
    }

    auto previous = std::chrono::steady_clock::now();
    auto lastSave = previous;
    while (app->destroyRequested == 0) {
        const bool active = state.focused && state.renderer.ready();
        int events = 0;
        android_poll_source* source = nullptr;
        int identifier = 0;
        while ((identifier = ALooper_pollOnce(active ? 0 : -1, nullptr, &events, reinterpret_cast<void**>(&source))) >= 0) {
            (void)identifier;
            if (source != nullptr) source->process(app, source);
            if (app->destroyRequested != 0) break;
        }
        if (!active || app->destroyRequested != 0) continue;

        const auto now = std::chrono::steady_clock::now();
        const float delta = std::chrono::duration<float>(now - previous).count();
        previous = now;
        const int selected = state.touch.consumeSelectedSlot();
        if (selected >= 0) state.game.chooseHotbarSlot(static_cast<std::size_t>(selected));
        state.game.update(combinedInput(state, delta), delta);
        state.renderer.render(state.game);

        if (now - lastSave > std::chrono::seconds(20)) {
            state.save();
            lastSave = now;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
    state.save();
    state.renderer.destroy();
}
