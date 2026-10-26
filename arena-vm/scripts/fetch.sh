#!/usr/bin/env bash
# Shallow-clone every manifest repo and pin its exact commit in pins.env.
# Usage: fetch.sh [name ...]   (no args = everything)
set -euo pipefail
source "$(dirname "$0")/env.sh"
MANIFEST="$VM_ROOT/scripts/manifest.env"
PINS="$VM_ROOT/pins.env"
touch "$PINS"

want=("$@")
FAILURES=()
while IFS='|' read -r name repo purpose; do
  [ -z "${name:-}" ] && continue
  case "$name" in \#*) continue;; esac
  if [ "${#want[@]}" -gt 0 ]; then
    ok=0; for w in "${want[@]}"; do [ "$w" = "$name" ] && ok=1; done
    [ "$ok" = 0 ] && continue
  fi
  dst="$VM_SRC/$name"
  if [ -d "$dst/.git" ]; then
    log "exists: $name"
  else
    log "clone $repo -> $name"
    if ! git clone --depth 1 "https://github.com/$repo.git" "$dst" 2>&1 | tail -2; then
      FAILURES+=("$name|$repo")
      continue
    fi
  fi
  sha=$(git -C "$dst" rev-parse HEAD)
  # update pin in place
  if grep -q "^PIN_${name}=" "$PINS"; then
    sed -i "s|^PIN_${name}=.*|PIN_${name}=$sha|" "$PINS"
  else
    echo "PIN_${name}=$sha   # $repo" >> "$PINS"
  fi
done < "$MANIFEST"
log "pins written to $PINS"
if [ "${#FAILURES[@]}" -gt 0 ]; then
  echo "CLONE FAILURES:" >&2
  printf '  %s\n' "${FAILURES[@]}" >&2
  exit 2
fi
