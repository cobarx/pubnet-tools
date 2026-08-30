# Build debug binaries. On Windows/MSYS2 the GNU toolchain produces executables
# without the POSIX execute bit; the chmod line sets it so zsh can run them
# directly. The leading `-` makes just ignore the error on native PowerShell
# where chmod is not available.
build:
    cargo build
    -chmod +x target/debug/pubnetchk.exe target/debug/pubnetdiag.exe

# Build release binaries (same execute-bit fix).
release:
    cargo build --release
    -chmod +x target/release/pubnetchk.exe target/release/pubnetdiag.exe

# Fast workspace-wide type check (no binary produced).
check:
    cargo check --workspace

# Unit tests only — fast, no network required.
test:
    cargo test --lib

# Unit + contract tests — requires a live network.
test-all:
    cargo test

# Lint.
clippy:
    cargo clippy --all-targets

# Regenerate docs/exit-codes.md from exit_codes::TABLE in pubnetdiag.
exit-codes-doc:
    cargo run -q --bin gen_exit_codes > docs/exit-codes.md

# --- Android (pubnetchk-android UniFFI cdylib) ---
# Prerequisites are owner-installed (NDK, cargo-ndk, the Android Rust targets) —
# see docs/epics/pubnet-android/epic.md. These recipes assume they are present.

# Regenerate the UniFFI Kotlin bindings. Uses a debug build of the cdylib: the
# generated interface is profile-independent, and the debug lib keeps the
# metadata that the release profile's `strip = true` removes.
android-bindings OUT="target/uniffi-kotlin":
    cargo build -p pubnetchk-android
    cargo run -q -p pubnetchk-android --bin uniffi-bindgen -- generate \
        --library --no-format --language kotlin \
        --out-dir {{OUT}} target/debug/libpubnetchk_android.so

# Cross-compile the cdylib for the Android ABIs into an Android jniLibs tree.
# The shipped .so is stripped (no runtime metadata needed); Gradle strips again
# at packaging time.
android-lib OUT="android/app/src/main/jniLibs":
    cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o {{OUT}} \
        build -p pubnetchk-android --release
