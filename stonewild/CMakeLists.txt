cmake_minimum_required(VERSION 3.20)
project(Stonewild VERSION 0.2.0 LANGUAGES C CXX)

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
    include(CTest)
    if(BUILD_TESTING)
        add_executable(stonewild_core_tests engine/tests/test_engine.cpp)
        target_link_libraries(stonewild_core_tests PRIVATE stonewild_core)
        target_compile_options(stonewild_core_tests PRIVATE
            $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic>
        )
        add_test(NAME stonewild_core_tests COMMAND stonewild_core_tests)
    endif()

    if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
        option(STONEWILD_BUILD_DESKTOP "Build the SDL2/OpenGL Linux game" ON)
    else()
        option(STONEWILD_BUILD_DESKTOP "Build the SDL2/OpenGL Linux game" OFF)
    endif()

    if(STONEWILD_BUILD_DESKTOP)
        find_package(SDL2 CONFIG REQUIRED)
        find_package(OpenGL REQUIRED)
        add_executable(stonewild desktop/src/main.cpp)
        target_link_libraries(stonewild PRIVATE stonewild_core SDL2::SDL2 OpenGL::GL)
        target_compile_options(stonewild PRIVATE
            $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic>
        )

        include(GNUInstallDirs)
        install(TARGETS stonewild RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
        install(
            FILES packaging/linux/game.stonewild.Stonewild.desktop
            DESTINATION ${CMAKE_INSTALL_DATADIR}/applications
        )
        install(
            FILES packaging/linux/game.stonewild.Stonewild.svg
            DESTINATION ${CMAKE_INSTALL_DATADIR}/icons/hicolor/scalable/apps
        )
        install(
            FILES README.md ROADMAP.md CHANGELOG.md
            DESTINATION ${CMAKE_INSTALL_DATADIR}/doc/stonewild
        )

        set(CPACK_GENERATOR "DEB")
        set(CPACK_PACKAGE_NAME "stonewild")
        set(CPACK_PACKAGE_VENDOR "Stonewild Contributors")
        set(CPACK_PACKAGE_CONTACT "Stonewild Contributors <noreply@stonewild.invalid>")
        set(CPACK_PACKAGE_DESCRIPTION_SUMMARY "Original low-memory voxel exploration game")
        set(CPACK_PACKAGE_DESCRIPTION
            "Stonewild is an original clean-room voxel exploration game. This package includes an adaptive low-memory mode for 4 GB systems, keyboard and mouse controls, gamepad support, procedural worlds, and local saves."
        )
        set(CPACK_PACKAGE_VERSION ${PROJECT_VERSION})
        set(CPACK_PACKAGING_INSTALL_PREFIX "/usr")
        set(CPACK_DEBIAN_FILE_NAME DEB-DEFAULT)
        set(CPACK_DEBIAN_PACKAGE_SECTION "games")
        set(CPACK_DEBIAN_PACKAGE_PRIORITY "optional")
        set(CPACK_DEBIAN_PACKAGE_DEPENDS "libsdl2-2.0-0 (>= 2.0.10), libgl1")
        set(CPACK_DEBIAN_PACKAGE_HOMEPAGE "https://github.com/mhammedamazil-hub/testtemp")
        set(CPACK_DEBIAN_PACKAGE_SHLIBDEPS ON)
        include(CPack)
    endif()
endif()
