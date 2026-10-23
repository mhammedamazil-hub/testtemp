#pragma once

#include <vector>

#include "stonewild/world.hpp"

namespace stonewild {

struct Vertex {
    float x;
    float y;
    float z;
    float nx;
    float ny;
    float nz;
    float r;
    float g;
    float b;
    float a;
    float ambientOcclusion;
};

std::vector<Vertex> buildChunkMesh(const World& world, const Chunk& chunk);

}  // namespace stonewild
