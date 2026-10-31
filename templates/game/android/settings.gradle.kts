// The Android app: the game's Rust library (built by cargo-ndk), the engine's
// Java activity and the asset bundle, packaged by the Android Gradle plugin. See README.md.

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "{{project-name}}"
include(":app")
