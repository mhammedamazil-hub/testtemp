set(NATIVE_APP_GLUE_DIR "${ANDROID_NDK}/sources/android/native_app_glue")

add_library(native_app_glue STATIC
    "${NATIVE_APP_GLUE_DIR}/android_native_app_glue.c"
)
target_include_directories(native_app_glue PUBLIC "${NATIVE_APP_GLUE_DIR}")

add_library(stonewild SHARED android_main.cpp)
target_include_directories(stonewild PRIVATE "${NATIVE_APP_GLUE_DIR}")
target_link_libraries(stonewild PRIVATE
    stonewild_core
    native_app_glue
    android
    log
    EGL
    GLESv3
)
target_link_options(stonewild PRIVATE "-uANativeActivity_onCreate")
