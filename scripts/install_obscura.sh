#!/usr/bin/env bash
#
# Install the official Obscura release for this machine's OS and architecture.
#
#   ./scripts/install_obscura.sh                 # render-enabled build (recommended)
#   ./scripts/install_obscura.sh --variant stealth
#   ./scripts/install_obscura.sh --variant no-render
#   ./scripts/install_obscura.sh --force         # reinstall even if present
#
# The archive is unpacked into vendor/obscura/ (override with $OBSCURA_INSTALL_DIR).
# Both `obscura` and `obscura-worker` are kept together: the parallel `scrape`
# command needs the worker next to the main binary.
#
# Requires no toolchain. Needs curl and tar (or unzip on Windows).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${OBSCURA_INSTALL_DIR:-$REPO_ROOT/vendor/obscura}"
VARIANT="standard"
FORCE=0
DRY_RUN=0

RELEASES_URL="https://github.com/h4ckf0r0day/obscura/releases/latest/download"

usage() {
    awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "${BASH_SOURCE[0]}"
    exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --variant) VARIANT="${2:?--variant needs a value}"; shift 2 ;;
        --dest)    DEST="${2:?--dest needs a value}"; shift 2 ;;
        --force)   FORCE=1; shift ;;
        --dry-run) DRY_RUN=1; shift ;;
        -h|--help) usage 0 ;;
        *) echo "unknown argument: $1" >&2; usage 1 ;;
    esac
done

case "$VARIANT" in
    standard|stealth|no-render|no-render-stealth) ;;
    *) echo "error: --variant must be standard, stealth, no-render, or no-render-stealth" >&2; exit 1 ;;
esac

log()  { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die()  { printf 'error: %s\n' "$*" >&2; exit 1; }

# --- detect platform ---------------------------------------------------------
UNAME_S="$(uname -s)"
UNAME_M="$(uname -m)"

case "$UNAME_S" in
    Linux)  OS="linux" ;;
    Darwin) OS="macos" ;;
    MINGW*|MSYS*|CYGWIN*) OS="windows" ;;
    *) die "unsupported operating system: $UNAME_S (Obscura ships linux, macos, windows builds)" ;;
esac

case "$UNAME_M" in
    x86_64|amd64)   ARCH="x86_64" ;;
    aarch64|arm64)  ARCH="aarch64" ;;
    *) die "unsupported architecture: $UNAME_M (Obscura ships x86_64 and aarch64 builds)" ;;
esac

if [[ "$OS" == "windows" && "$ARCH" == "aarch64" ]]; then
    die "no Windows aarch64 build is published"
fi

# --- glibc compatibility (Linux only) ---------------------------------------
# Release builds target Ubuntu 22.04, so glibc >= 2.35 is required.
if [[ "$OS" == "linux" ]]; then
    if ! command -v getconf >/dev/null 2>&1; then
        warn "getconf not found; cannot verify glibc version"
    else
        GLIBC="$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}')"
        if [[ -z "${GLIBC:-}" ]]; then
            warn "this system does not report a glibc version (musl/Alpine?)"
            warn "official Linux archives are glibc-linked and will not run here"
        else
            GLIBC_MAJOR="${GLIBC%%.*}"
            GLIBC_REST="${GLIBC#*.}"
            GLIBC_MINOR="${GLIBC_REST%%.*}"
            if (( GLIBC_MAJOR < 2 || (GLIBC_MAJOR == 2 && GLIBC_MINOR < 35) )); then
                die "glibc $GLIBC is too old; Obscura Linux builds need glibc >= 2.35 (Ubuntu 22.04 or newer)"
            fi
            log "glibc $GLIBC is compatible (>= 2.35)"
        fi
    fi
fi

# --- archive name ------------------------------------------------------------
BASE="obscura-${ARCH}-${OS}"
if [[ "$VARIANT" != "standard" ]]; then
    BASE="${BASE}-${VARIANT}"
fi
if [[ "$OS" == "windows" ]]; then
    ARCHIVE="${BASE}.zip"
else
    ARCHIVE="${BASE}.tar.gz"
fi
URL="${RELEASES_URL}/${ARCHIVE}"

log "platform : ${UNAME_S} ${UNAME_M} -> ${ARCH}-${OS}"
log "variant  : ${VARIANT}"
log "archive  : ${ARCHIVE}"
log "dest     : ${DEST}"

if [[ $DRY_RUN -eq 1 ]]; then
    log "dry run: would download $URL"
    log "dry run: would extract into $DEST and keep obscura + obscura-worker"
    exit 0
fi

if [[ -x "$DEST/obscura" && $FORCE -eq 0 ]]; then
    log "already installed: $("$DEST/obscura" --version 2>/dev/null || echo 'version check failed')"
    log "use --force to reinstall"
    exit 0
fi

# --- disk space --------------------------------------------------------------
mkdir -p "$DEST"
AVAIL_KB="$(df -Pk "$DEST" | awk 'NR==2 {print $4}')"
if [[ -n "${AVAIL_KB:-}" ]] && (( AVAIL_KB < 500000 )); then
    die "less than 500 MB free at $DEST (have $((AVAIL_KB/1024)) MB); an archive plus extraction needs ~200 MB"
fi

# --- download ----------------------------------------------------------------
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# A pre-placed archive is used instead of the network, which is the supported
# path when outbound access to GitHub's release-asset hosts is blocked.
SRC_ARCHIVE=""
if [[ -n "${OBSCURA_ARCHIVE:-}" ]]; then
    [[ -f "$OBSCURA_ARCHIVE" ]] || die "OBSCURA_ARCHIVE=$OBSCURA_ARCHIVE does not exist"
    SRC_ARCHIVE="$OBSCURA_ARCHIVE"
    log "using \$OBSCURA_ARCHIVE: $SRC_ARCHIVE"
elif [[ -f "$DEST/$ARCHIVE" ]]; then
    SRC_ARCHIVE="$DEST/$ARCHIVE"
    log "using pre-placed archive: $SRC_ARCHIVE"
fi

if [[ -z "$SRC_ARCHIVE" ]]; then
log "downloading $URL"
if ! curl -fL --retry 2 --retry-delay 2 --connect-timeout 20 \
        --max-time 900 -o "$TMP/$ARCHIVE" "$URL"; then
    cat >&2 <<'EOF'

error: download failed.

If this sandbox restricts outbound network access, GitHub release assets
(release-assets.githubusercontent.com / objects.githubusercontent.com) may be
blocked even when github.com itself is reachable. Symptoms:
  curl: (35) OpenSSL SSL_connect: SSL_ERROR_SYSCALL
  curl: (56) Recv failure: Connection reset by peer

Workarounds, in order of preference:
  1. Allow the host, then re-run this script.
  2. Download the archive on a machine with access and place it at:
         vendor/obscura/<archive-name>
     then re-run this script; it will use the local file.
  3. Upload the archive into the workspace and set OBSCURA_ARCHIVE=<path>.
EOF
    exit 1
fi
SRC_ARCHIVE="$TMP/$ARCHIVE"
fi

# --- checksum (optional, recommended) ---------------------------------------
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        return 1
    fi
}

if CHECKSUM="$(sha256_of "$SRC_ARCHIVE")"; then
    log "sha256   : $CHECKSUM"
    if [[ -n "${OBSCURA_SHA256:-}" ]]; then
        if [[ "${OBSCURA_SHA256,,}" != "$CHECKSUM" ]]; then
            die "checksum mismatch for $(basename "$SRC_ARCHIVE")
  expected: ${OBSCURA_SHA256,,}
  actual  : $CHECKSUM
The archive is not the build you asked for; do not run it."
        fi
        log "checksum verified against \$OBSCURA_SHA256"
    else
        warn "no \$OBSCURA_SHA256 set; the release digest is published by the"
        warn "GitHub API as the asset 'digest' field. Compare it manually if"
        warn "the archive did not come from this script's own download."
    fi
else
    warn "no sha256sum/shasum available; skipping checksum verification"
fi

# --- extract -----------------------------------------------------------------
log "extracting"
if [[ "$OS" == "windows" ]]; then
    command -v unzip >/dev/null 2>&1 || die "unzip is required for Windows archives"
    unzip -o -q "$SRC_ARCHIVE" -d "$DEST"
else
    tar xzf "$SRC_ARCHIVE" -C "$DEST"
fi

chmod +x "$DEST/obscura" 2>/dev/null || true
[[ -f "$DEST/obscura-worker" ]] && chmod +x "$DEST/obscura-worker" 2>/dev/null || true

[[ -x "$DEST/obscura" ]] || die "extraction did not produce an executable at $DEST/obscura"
[[ -f "$DEST/obscura-worker" ]] || warn "obscura-worker missing; the parallel 'scrape' command will not work"

# --- verify ------------------------------------------------------------------
log "installed files:"
ls -lh "$DEST" | sed 's/^/    /'

log "version check"
if ! VERSION_OUT="$("$DEST/obscura" --version 2>&1)"; then
    die "the binary cannot run on this system:

$VERSION_OUT

Common causes: glibc older than 2.35, wrong architecture, or a missing CPU
feature. Report the output above; do not retry the same install."
fi
printf '    %s\n' "$VERSION_OUT"

log "smoke test (https://example.com)"
if SMOKE_OUT="$("$DEST/obscura" fetch https://example.com --eval 'document.title' -q 2>&1)"; then
    printf '    title: %s\n' "$SMOKE_OUT"
else
    warn "smoke test failed:"
    printf '    %s\n' "$SMOKE_OUT" >&2
    warn "the binary installed but could not fetch a page (network or engine issue)"
    exit 1
fi

log "done"
echo
echo "Use it with:"
echo "    python -m agent_tools doctor"
echo "    python -m agent_tools call browser_evaluate '{\"url\": \"https://example.com\", \"expression\": \"document.title\"}'"
