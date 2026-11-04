plugins {
    id("com.android.application")
}

android {
    namespace = "ai.artcraft.studio"
    compileSdk = 35

    defaultConfig {
        applicationId = "ai.artcraft.studio"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.2.0-native"
    }

    packaging {
        jniLibs.useLegacyPackaging = false
        resources.excludes += setOf("META-INF/AL2.0", "META-INF/LGPL2.1")
    }
}
