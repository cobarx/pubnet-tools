# pubnetchk — Android app

A Jetpack Compose front-end that runs the `pubnetchk` audit on the Wi-Fi /
network this phone is joined to, reusing the Rust engine through a UniFFI
`cdylib` (`crates/pubnetchk-android`). Design: `docs/epics/pubnet-android/` and
`docs/decisions/2026-08-30-android-app-architecture.md`.

**Walking-skeleton scope:** only **topology** and **security** run. Reliability
needs an unprivileged-ICMP path and speed needs the NDT7 client validated over
rustls (epic tickets 6–7). The DoH sub-probe works, validating against the
`webpki-roots` CA bundle (`crates/pubnetchk/src/tls.rs`) — reqwest's `rustls`
feature hard-links `rustls-platform-verifier`, which `abort()`s the process on
Android with no JVM `Context`, so reqwest is handed a preconfigured
`ClientConfig` instead. Using the device trust store is epic ticket 9. The
DNS-leak verdict stays `uncertain` regardless — the engine cannot see the
resolver's egress IP on Android (same as macOS / Windows).

## Prerequisites

The dev container (`.devcontainer/`, or `docs/context/devcontainer-setup.md`)
already has all of this. Installing by hand:

| Tool | Version | Notes |
|---|---|---|
| JDK | 21 (LTS) | the Android Gradle Plugin does not support JDK 25 |
| Android SDK | platform `android-35`, build-tools `35.0.0` | `ANDROID_HOME` / `ANDROID_SDK_ROOT` |
| Android NDK | `27.2.12479018` | `ANDROID_NDK_HOME`; pinned in `app/build.gradle.kts` (`pinnedNdkVersion`) — keep in sync with the dev container |
| `cargo-ndk` | `cargo install cargo-ndk` | used by `just android-lib` |
| Rust targets | `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android` | `rustup target add …` |

`cmake` and the NDK's `clang` must be on `PATH` when the cdylib cross-compiles
(`aws-lc-sys`, the rustls crypto backend, is a C build).

Do **not** commit `android/local.properties` (it points at the local SDK/NDK)
or anything under `app/src/main/jniLibs/` (cross-compiled `.so`).

## Build

```bash
cd android
./gradlew :app:assembleDebug        # -> app/build/outputs/apk/debug/app-debug.apk
./gradlew :app:testDebugUnitTest    # JVM unit tests (report-JSON parser)
```

`assembleDebug` is self-contained: the `cargo` block in `app/build.gradle.kts`
(the `org.mozilla.rust-android-gradle` plugin) cross-compiles
`crates/pubnetchk-android` for `arm64-v8a`, `armeabi-v7a`, and `x86_64` and
drops each `libpubnetchk_android.so` into the merged `jniLibs`; the
`generateUniffiBindings` task emits the Kotlin bindings into
`app/build/generated/uniffi`. Both are wired as `preBuild` dependencies.

The workspace-root `justfile` has the same steps for iterating on the Rust side
without Gradle:

```bash
just android-lib        # cargo ndk -> android/app/src/main/jniLibs/<abi>/libpubnetchk_android.so
just android-bindings   # uniffi-bindgen -> target/uniffi-kotlin/uniffi/pubnetchk_android/pubnetchk_android.kt
```

### UniFFI bindings

Generated from a **debug host** build of the cdylib: the interface is
profile-independent, and the workspace release profile's `strip = true` removes
the metadata library-mode `uniffi-bindgen` reads. The shipped (release) `.so`
only needs the `extern "C"` entry points, which survive stripping. See
`crates/pubnetchk-android/README.md`.

The generated Kotlin binds to the `.so` through JNA
(`net.java.dev.jna:jna:…@aar`). `runAuditJson` **blocks** for the length of the
audit — `AuditViewModel` calls it on `Dispatchers.IO`.

## 16 KB page size

The repo-root `.cargo/config.toml` passes `-Wl,-z,max-page-size=16384` to the
linker for the `*-linux-android` triples so the cdylib's ELF LOAD segments are
16 KB aligned — Android 15+ on 16 KB-page devices rejects 4 KB-aligned native
libs. NDK r27 needs the flag; r28 makes it the default. Desktop triples are
untouched. Verify: `llvm-readelf -l …/libpubnetchk_android.so` shows `Align
0x4000`, and `zipalign -c -P 16 -v 4 app-debug.apk`.

## Configuration cache

`org.gradle.configuration-cache` is **off**: `rust-android-gradle` 0.9.6 reads
`Task.project` at execution time, which the configuration cache forbids. Re-enable
it once the plugin is fixed.

## Not covered here

On-device / emulator runs (the dev container has no `adb` device and no KVM),
Play Store signing/release, and the polished multi-section UI (this screen is
deliberately thin). See the epic.
