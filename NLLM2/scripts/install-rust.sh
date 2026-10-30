#!/usr/bin/env bash
# Install a pinned Rust 1.88.0 toolchain WITHOUT rustup, crates.io or static.rust-lang.org.
#
# Why this exists: in the Arena sandbox the outbound allowlist is
# github.com / codeload / api.github.com / registry.npmjs.org / pypi.org.
# sh.rustup.rs, static.rust-lang.org, deb.debian.org and crates.io are all blocked
# (verified: HTTP 000). The npm registry, however, publishes prebuilt rustup
# component tarballs under the @rustbin scope, so we can assemble the same
# prefix layout rustup would produce.
#
# Consequence for the project: crates.io is unreachable => Forge is a
# ZERO-DEPENDENCY Rust application (core + tests use only std).
#
# Usage: bash scripts/install-rust.sh [PREFIX]     (default /tmp/forge-rust)
set -euo pipefail

PREFIX="${1:-/tmp/forge-rust}"
VER="1.88.0"
TARGET="x86_64-unknown-linux-gnu"
BASE="https://registry.npmjs.org/@rustbin"

if [ -x "$PREFIX/bin/rustc" ] && [ -x "$PREFIX/bin/cargo" ]; then
  if "$PREFIX/bin/rustc" --version 2>/dev/null | grep -q "$VER"; then
    echo "toolchain already present: $("$PREFIX/bin/rustc" --version) at $PREFIX"
    echo "export PATH=\"$PREFIX/bin:\$PATH\""
    exit 0
  fi
fi

DL="$(mktemp -d)"
trap 'rm -rf "$DL"' EXIT
mkdir -p "$PREFIX/bin" "$PREFIX/lib"

fetch () { # $1 = package spec, $2 = output file
  local url="$BASE/$1/-/$1.tgz"
  echo "fetching $1"
  curl -fsSL --max-time 600 -o "$DL/$2" "$url"
}

fetch "rustc-$VER-$TARGET" "rustc.tgz"
fetch "cargo-$VER-$TARGET" "cargo.tgz"
fetch "rust-std-$VER-$TARGET" "std.tgz"

mkdir -p "$DL/x"
for f in rustc cargo std; do mkdir -p "$DL/x/$f"; tar xzf "$DL/$f.tgz" -C "$DL/x/$f"; done

cp -a "$DL/x/rustc/package/rustc/bin/." "$PREFIX/bin/"
cp -a "$DL/x/rustc/package/rustc/lib/." "$PREFIX/lib/"
cp -a "$DL/x/cargo/package/cargo/bin/." "$PREFIX/bin/"
mkdir -p "$PREFIX/lib/rustlib"
cp -a "$DL/x/std/package/rust-std-$TARGET/lib/rustlib/$TARGET" "$PREFIX/lib/rustlib/"
chmod +x "$PREFIX/bin/"*

"$PREFIX/bin/rustc" --version
"$PREFIX/bin/cargo" --version
echo
echo "Installed to $PREFIX ($(du -sh "$PREFIX" | cut -f1))."
echo "export PATH=\"$PREFIX/bin:\$PATH\""
echo "NOTE: crates.io is blocked, so only dependency-free crates can be built."
