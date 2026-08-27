#!/usr/bin/env bash
# Analyze where the pubnetchk binary's size goes.
#
# The shipped release binary is stripped (strip = true in Cargo.toml), so it
# carries no symbols for a tool to attribute size to. cargo-bloat needs those
# symbols, so this script builds the release profile with stripping disabled
# (CARGO_PROFILE_RELEASE_STRIP=false) purely for analysis — the real, stripped
# artifact is left untouched and its on-disk size is reported first for context.
#
# Usage:
#   scripts/analyze-size.sh          # by-crate, then top functions
#   scripts/analyze-size.sh 40       # show top 40 functions instead of 20
#
# Requires cargo-bloat: cargo install cargo-bloat
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if ! cargo bloat --version >/dev/null 2>&1; then
    echo "cargo-bloat is not installed. Install it with:" >&2
    echo "    cargo install cargo-bloat" >&2
    exit 1
fi

funcs="${1:-20}"

echo "===== Shipped size (stripped release binary) ====="
cargo build --release --quiet
ls -lh target/release/pubnetchk | awk '{print $5 "\t" $NF}'
echo

# strip=false only affects these analysis builds; it does not change what a
# normal `cargo build --release` produces.
export CARGO_PROFILE_RELEASE_STRIP=false

echo "===== .text size by crate ====="
cargo bloat --release --crates

echo
echo "===== Top $funcs functions ====="
cargo bloat --release -n "$funcs"

# cargo-bloat's unstripped build overwrites target/release/pubnetchk (leaving a
# ~10MB binary full of .debug_* sections on disk). Restore the real, stripped
# artifact so nothing downstream mistakes the analysis build for the shipped one.
echo
echo "===== Restoring stripped release binary ====="
unset CARGO_PROFILE_RELEASE_STRIP
touch src/main.rs
cargo build --release --quiet
ls -lh target/release/pubnetchk | awk '{print $5 "\t" $NF}'
