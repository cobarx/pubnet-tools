plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
    id("org.mozilla.rust-android-gradle.rust-android")
}

// Repo root — the Cargo workspace lives one level above the `android/` Gradle build.
val repoRoot: File = rootProject.projectDir.parentFile
val androidCrateDir: File = repoRoot.resolve("crates/pubnetchk-android")

// Pinned so local builds and the dev container (docs/context/devcontainer-setup.md)
// and CI (epic ticket 8) all resolve the same toolchain.
val pinnedNdkVersion = "27.2.12479018"

android {
    namespace = "com.cobarx.pubnetchk"
    compileSdk = 35
    ndkVersion = pinnedNdkVersion

    defaultConfig {
        applicationId = "com.cobarx.pubnetchk"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        // Ship only the ABIs the cdylib is cross-compiled for (see `cargo` block).
        // Without this, JNA contributes x86/mips stubs and the app would load on
        // an ABI with no libpubnetchk_android.so -> UnsatisfiedLinkError.
        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
    }

    // The UniFFI-generated bindings land here; see `generateUniffiBindings` below.
    sourceSets["main"].kotlin.srcDir(layout.buildDirectory.dir("generated/uniffi"))
    // The rust-android-gradle plugin drops the per-ABI cdylib here.
    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("rustJniLibs/android"))

    testOptions {
        unitTests {
            isReturnDefaultValues = true
        }
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    implementation("androidx.core:core-ktx:1.15.0")

    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")

    // UniFFI's generated Kotlin binds to the cdylib through JNA. The `@aar`
    // classifier pulls the Android-native JNA dispatch libraries.
    implementation("net.java.dev.jna:jna:5.15.0@aar")

    debugImplementation("androidx.compose.ui:ui-tooling")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.9.0")
}

// --- Rust cdylib (org.mozilla.rust-android-gradle) ---------------------------
// Cross-compiles crates/pubnetchk-android for the packaged ABIs and drops the
// per-ABI libpubnetchk_android.so into the merged jniLibs. Needs the NDK
// (ANDROID_NDK_HOME / ndk.dir) and the *-linux-android Rust targets — all
// owner-installed prerequisites, see docs/epics/pubnet-android/epic.md.
cargo {
    module = androidCrateDir.relativeTo(projectDir).path
    libname = "pubnetchk_android"
    targets = listOf("arm64", "x86_64", "arm")
    profile = "release"
    prebuiltToolchains = true
    // pubnetchk-android is one crate in the workspace; build just that package.
    extraCargoBuildArguments = listOf("--package", "pubnetchk-android")
    // Cargo writes workspace artifacts to <workspace-root>/target, not
    // <module>/target — point the plugin's artifact copy at the real location.
    targetDirectory = repoRoot.resolve("target").path
    // 16 KB ELF segment alignment for the cdylib comes from the repo-root
    // .cargo/config.toml (per-android-triple `-Wl,-z,max-page-size=16384`).
}

// --- UniFFI Kotlin bindings --------------------------------------------------
// Generated from a *debug host* build of the cdylib on purpose: the interface is
// profile-independent, and the workspace release profile's `strip = true`
// removes the metadata library-mode uniffi-bindgen reads (see
// crates/pubnetchk-android/README.md). Mirrors the `just android-bindings` recipe.
val uniffiGenDir: Provider<Directory> = layout.buildDirectory.dir("generated/uniffi")
// Host dylib suffix: the dev container / CI run Linux; macOS contributors get .dylib.
val hostLibSuffix = if (System.getProperty("os.name").startsWith("Mac")) "dylib" else "so"
val hostDebugLib: File = repoRoot.resolve("target/debug/libpubnetchk_android.$hostLibSuffix")

val generateUniffiBindings by tasks.registering(Exec::class) {
    group = "uniffi"
    description = "Generate the Kotlin bindings for crates/pubnetchk-android."

    inputs.dir(androidCrateDir.resolve("src"))
    inputs.file(androidCrateDir.resolve("Cargo.toml"))
    outputs.dir(uniffiGenDir)

    workingDir = repoRoot
    // One shell call: build the host cdylib, then run the library-mode generator.
    commandLine(
        "sh", "-c",
        "cargo build -p pubnetchk-android && " +
            "cargo run -q -p pubnetchk-android --bin uniffi-bindgen -- generate " +
            "--library --no-format --language kotlin " +
            "--out-dir '${uniffiGenDir.get().asFile}' '${hostDebugLib}'",
    )
}

tasks.named("preBuild").configure {
    dependsOn(generateUniffiBindings)
    dependsOn(tasks.named("cargoBuild"))
}

// The Rust plugin registers `cargoBuild` lazily; make sure it lands before
// anything consumes the jniLibs source dir it populates.
tasks.matching {
    it.name.matches(Regex("^merge.*(JniLibFolders|NativeLibs)$"))
}.configureEach {
    dependsOn(tasks.named("cargoBuild"))
}
