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

# Resolve the real tarball URL from registry metadata instead of guessing it.
# (Learned the hard way: the npm tarball name repeats the version, and cargo's
#  npm version does NOT match the rust version -- cargo 1.88.0 ships as 0.89.0.)
fetch () { # $1 = package name (without @rustbin/), $2 = output file
  local pkg="$1" out="$2" meta tarball
  meta="$(curl -fsSL --max-time 120 "https://registry.npmjs.org/@rustbin%2f${pkg}")" || {
    echo "ERROR: cannot resolve @rustbin/${pkg}" >&2; exit 1; }
  tarball="$(printf '%s' "$meta" | python3 -c '
import sys, json
d = json.load(sys.stdin)
versions = d.get("versions") or {}
if not versions:
    sys.exit("no versions for " + d.get("name", "?"))
best = sorted(versions)[-1]
print(versions[best]["dist"]["tarball"])
')" || exit 1
  echo "fetching @rustbin/${pkg}"
  echo "          ${tarball}"
  curl -fsSL --max-time 900 -o "$DL/$out" "$tarball"
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
