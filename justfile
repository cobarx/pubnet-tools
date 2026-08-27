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
