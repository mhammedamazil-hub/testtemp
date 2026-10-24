#!/usr/bin/env bash
#
# Run the official Obscura verification steps and record what actually happened.
#
#   ./scripts/smoke_test.sh
#   OBSCURA_BIN=/path/to/obscura ./scripts/smoke_test.sh
#
# Prints the environment, every command with its output, and a pass/fail table.
# Exits non-zero if any step fails. Nothing is asserted that was not run.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${OBSCURA_BIN:-$REPO_ROOT/vendor/obscura/obscura}"
# Sites are configurable: sandboxes often allowlist hosts, so the official
# example.com targets may be unreachable even when Obscura works.
URL_1="${OBSCURA_SMOKE_URL:-https://example.com}"
URL_2="${OBSCURA_SMOKE_URL_2:-https://news.ycombinator.com}"

PASS=0
FAIL=0
declare -a RESULTS=()

hr() { printf '%s\n' "------------------------------------------------------------"; }

record() {
    local name="$1" status="$2" output="$3"
    if [[ $status -eq 0 ]]; then
        PASS=$((PASS + 1)); RESULTS+=("PASS  $name")
    else
        FAIL=$((FAIL + 1)); RESULTS+=("FAIL  $name (exit $status)")
    fi
    hr
    printf 'CASE   : %s\nSTATUS : %s\nOUTPUT :\n' "$name" "$status"
    printf '%s\n' "$output" | sed 's/^/    /' | head -40
}

run_case() {
    local name="$1"; shift
    local output status
    output="$("$@" 2>&1)"; status=$?
    record "$name" "$status" "$output"
}

# For a case whose correct behaviour is a non-zero exit.
run_case_expect_fail() {
    local name="$1"; shift
    local output status
    output="$("$@" 2>&1)"; status=$?
    if [[ $status -ne 0 ]]; then
        record "$name (expected failure)" 0 "$output"
    else
        record "$name (expected failure)" 1 "$output
unexpected: the command exited 0 for an unresolvable host"
    fi
}

hr
echo "Obscura smoke test — $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
hr

echo "ENVIRONMENT"
echo "    uname -m      : $(uname -m)"
echo "    kernel        : $(uname -srm)"
echo "    libc          : $(getconf GNU_LIBC_VERSION 2>/dev/null || echo 'n/a')"
echo "    disk (repo)   : $(df -h "$REPO_ROOT" | awk 'NR==2 {print $4 " free of " $2}')"
echo "    memory        : $(free -h 2>/dev/null | awk '/^Mem:/ {print $7 " available of " $2}')"
echo "    binary        : $BIN"
echo "    site 1        : $URL_1"
echo "    site 2        : $URL_2"
echo "    SSL_CERT_FILE : ${SSL_CERT_FILE:-(unset)}"

if [[ ! -x "$BIN" ]]; then
    hr
    echo "RESULT: cannot run — no executable at $BIN"
    echo "        install it with ./scripts/install_obscura.sh"
    exit 2
fi

echo "    worker        : $( [[ -x "$(dirname "$BIN")/obscura-worker" ]] && echo present || echo MISSING )"
hr

run_case "version"                  "$BIN" --version
run_case "site 1: title"            "$BIN" fetch "$URL_1" --eval "document.title" -q
run_case "site 1: JS DOM query"     "$BIN" fetch "$URL_1" --eval "document.querySelector('h1') ? document.querySelector('h1').textContent.trim() : 'no h1'" -q
run_case "site 1: text dump"        "$BIN" fetch "$URL_1" --dump text -q
run_case "site 2: title"            "$BIN" fetch "$URL_2" --eval "document.title" -q
run_case "site 2: JS link count"    "$BIN" fetch "$URL_2" --eval "document.querySelectorAll('a[href]').length" -q
run_case_expect_fail "error path (bad host)" "$BIN" fetch https://this-host-should-not-exist.invalid -q

hr
echo "SUMMARY"
printf '    %s\n' "${RESULTS[@]}"
hr
echo "    passed: $PASS"
echo "    failed: $FAIL"
hr

[[ $FAIL -eq 0 ]] && exit 0 || exit 1
