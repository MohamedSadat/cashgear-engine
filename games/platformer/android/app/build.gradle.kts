import java.util.Properties
import javax.inject.Inject
import org.gradle.process.ExecOperations

plugins {
    id("com.android.application")
}

// The Cargo workspace, the game's assets and the engine's Java activity, from here.
val workspaceDir: File = rootProject.projectDir.resolve("../../..").canonicalFile
val gameAssets: File = rootProject.projectDir.resolve("../assets").canonicalFile
val engineJava: File = workspaceDir.resolve("crates/cg_platform/android/java")

// The lowest Android version the game runs on: 8.0, where cpal's AAudio output starts.
val minApiLevel = 26

// Release signing: keystore.properties next to settings.gradle.kts (kept out of git), or
// the CG_ANDROID_* environment variables (CI secrets). Without either, release builds are
// unsigned. README.md shows how to make a keystore.
val keystore = Properties().apply {
    val file = rootProject.file("keystore.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}

fun signing(key: String, env: String): String? =
    System.getenv(env)?.takeIf { it.isNotEmpty() } ?: keystore.getProperty(key)

val releaseStore = signing("storeFile", "CG_ANDROID_KEYSTORE")

// The version: `-PversionName=1.2.3` (the release workflow passes the tag's) sets both;
// Google Play needs the code to grow with every upload (1.2.3 is 10203).
val appVersion = (project.findProperty("versionName") as String?) ?: "0.1.0"
val appVersionCode = appVersion.split('.').map { it.toIntOrNull() ?: 0 }
    .let { (it + listOf(0, 0, 0)).take(3) }
    .let { (major, minor, patch) -> major * 10000 + minor * 100 + patch }
    .coerceAtLeast(1)

android {
    namespace = "com.cashgear.platformer"
    compileSdk = 36
    ndkVersion = "29.0.14206865"

    defaultConfig {
        applicationId = "com.cashgear.platformer"
        minSdk = minApiLevel
        targetSdk = 36
        versionCode = appVersionCode
        versionName = appVersion
    }

    signingConfigs {
        if (releaseStore != null) {
            create("release") {
                storeFile = rootProject.file(releaseStore)
                storePassword = signing("storePassword", "CG_ANDROID_KEYSTORE_PASSWORD")
                keyAlias = signing("keyAlias", "CG_ANDROID_KEY_ALIAS")
                keyPassword = signing("keyPassword", "CG_ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        getByName("release") {
            signingConfig = signingConfigs.findByName("release")
        }
    }

    androidResources {
        // The bundle is compressed inside already. Stored as it is, the engine reads it
        // straight from the APK.
        noCompress += "cgpak"
    }
}

/** Builds the game's Rust library for some ABIs with cargo-ndk. */
abstract class CargoNdk : DefaultTask() {
    @get:Input abstract val abis: ListProperty<String>
    @get:Input abstract val profile: Property<String>
    @get:Input abstract val minApi: Property<Int>
    @get:Internal abstract val workspace: DirectoryProperty
    @get:Internal abstract val ndk: DirectoryProperty
    @get:OutputDirectory abstract val outputDir: DirectoryProperty
    @get:Inject abstract val exec: ExecOperations

    @TaskAction
    fun build() {
        // Start empty, so libraries of ABIs no longer asked for leave the APK.
        outputDir.get().asFile.deleteRecursively()
        val command = mutableListOf("cargo", "ndk")
        abis.get().forEach { command += listOf("-t", it) }
        command += listOf(
            "-P", minApi.get().toString(),
            "-o", outputDir.get().asFile.path,
            "build", "-p", "platformer_android", "--profile", profile.get(),
        )
        exec.exec {
            workingDir = workspace.get().asFile
            environment("ANDROID_NDK_HOME", ndk.get().asFile.path)
            commandLine(command)
        }
    }
}

/** Packs the game's assets into one bundle with cg_pack, as the web release does. */
abstract class BundleAssets : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val assets: DirectoryProperty
    @get:Internal abstract val workspace: DirectoryProperty
    @get:OutputDirectory abstract val outputDir: DirectoryProperty
    @get:Inject abstract val exec: ExecOperations

    @TaskAction
    fun bundle() {
        val out = outputDir.get().file("assets.cgpak").asFile
        exec.exec {
            workingDir = workspace.get().asFile
            commandLine(
                "cargo", "run", "--quiet", "-p", "cg_pack", "--",
                "bundle", assets.get().asFile.path, out.path,
            )
        }
    }
}

androidComponents {
    onVariants { variant ->
        val name = variant.name.replaceFirstChar { it.uppercase() }
        val release = variant.buildType == "release"
        // Phones are arm64; x86_64 is for emulators; armeabi-v7a for old 32-bit phones,
        // in release builds only. `-PcgAbis=arm64-v8a` builds fewer.
        val abiList = (project.findProperty("cgAbis") as String?)
            ?: if (release) "arm64-v8a,armeabi-v7a,x86_64" else "arm64-v8a,x86_64"

        val cargo = tasks.register<CargoNdk>("cargoNdk$name") {
            abis.set(abiList.split(',').map(String::trim).filter(String::isNotEmpty))
            profile.set(if (release) "android-release" else "dev")
            minApi.set(minApiLevel)
            workspace.set(workspaceDir)
            ndk.set(androidComponents.sdkComponents.ndkDirectory)
            // cargo knows what changed; let it decide every time.
            outputs.upToDateWhen { false }
        }
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargo, CargoNdk::outputDir)

        val bundle = tasks.register<BundleAssets>("bundleAssets$name") {
            assets.set(gameAssets)
            workspace.set(workspaceDir)
        }
        variant.sources.assets?.addGeneratedSourceDirectory(bundle, BundleAssets::outputDir)

        variant.sources.java?.addStaticSourceDirectory(engineJava.path)
    }
}
