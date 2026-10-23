cmake_minimum_required(VERSION 3.20)
project(Stonewild VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_library(stonewild_core STATIC
    engine/src/chunk.cpp
    engine/src/game.cpp
    engine/src/generator.cpp
    engine/src/mesh.cpp
    engine/src/player.cpp
    engine/src/raycast.cpp
    engine/src/world.cpp
)
target_include_directories(stonewild_core PUBLIC engine/include)
set_target_properties(stonewild_core PROPERTIES POSITION_INDEPENDENT_CODE ON)
target_compile_options(stonewild_core PRIVATE
    $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic>
)

if(ANDROID)
    add_subdirectory(android/app/src/main/cpp)
else()
    enable_testing()
    add_executable(stonewild_core_tests engine/tests/test_engine.cpp)
    target_link_libraries(stonewild_core_tests PRIVATE stonewild_core)
    target_compile_options(stonewild_core_tests PRIVATE
        $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic>
    )
    add_test(NAME stonewild_core_tests COMMAND stonewild_core_tests)
endif()
