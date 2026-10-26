#!/usr/bin/env bash
# Shallow-clone every manifest repo and pin its exact commit in pins.env.
# Manifest columns: name | owner/repo | ref (- = default branch) | purpose.
# Usage: fetch.sh [name ...]   (no args = everything)
set -euo pipefail
source "$(dirname "$0")/env.sh"
MANIFEST="$VM_ROOT/scripts/manifest.env"
PINS="$VM_ROOT/pins.env"
touch "$PINS"

want=("$@")
FAILURES=()
while IFS='|' read -r name repo ref purpose; do
  [ -z "${name:-}" ] && continue
  case "$name" in \#*) continue;; esac
  name="$(echo "$name" | tr -d ' ')"
  repo="$(echo "$repo" | tr -d ' ')"
  ref="$(echo "$ref" | tr -d ' ')"
  if [ "${#want[@]}" -gt 0 ]; then
    ok=0; for w in "${want[@]}"; do [ "$w" = "$name" ] && ok=1; done
    [ "$ok" = 0 ] && continue
  fi
  # noVNC installs under the prefix, every other checkout under src/
  if [ "$name" = "novnc" ]; then
    dst="$VM_PREFIX/share/novnc"
  else
    dst="$VM_SRC/$name"
  fi
  if [ -d "$dst/.git" ]; then
    log "exists: $name"
  else
    log "clone $repo ($ref) -> $name"
    if [ "$ref" = "-" ]; then
      git clone --depth 1 "https://github.com/$repo.git" "$dst" 2>&1 | tail -2 || FAILURES+=("$name|$repo")
    else
      git clone --depth 1 --branch "$ref" "https://github.com/$repo.git" "$dst" 2>&1 | tail -2 || FAILURES+=("$name|$repo")
    fi
  fi
  [ -d "$dst/.git" ] || continue
  sha=$(git -C "$dst" rev-parse HEAD)
  if grep -q "^PIN_${name}=" "$PINS"; then
    sed -i "s|^PIN_${name}=.*|PIN_${name}=$sha   # $repo ($ref)|" "$PINS"
  else
    echo "PIN_${name}=$sha   # $repo ($ref)" >> "$PINS"
  fi
done < "$MANIFEST"
log "pins written to $PINS"
if [ "${#FAILURES[@]}" -gt 0 ]; then
  echo "CLONE FAILURES:" >&2
  printf '  %s\n' "${FAILURES[@]}" >&2
  exit 2
fi
