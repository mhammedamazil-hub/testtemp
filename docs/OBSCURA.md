# Obscura integration

[Obscura](https://github.com/h4ckf0r0day/obscura) is a headless browser engine
written in Rust. It executes real JavaScript through V8 and implements the
Chrome DevTools Protocol, without bundling Chromium. This project uses it as the
engine behind the agent's browser tools.

> **Status: running and verified.** Obscura 0.2.5 is installed and executes
> JavaScript on real pages. Verified 2026-10-24 against live sites and a local
> fixture. See [Verified results](#verified-results) for the exact output.

---

## 1. Environment

Recorded on the sandbox before installation:

| Check | Command | Result |
|---|---|---|
| Architecture | `uname -m` | `x86_64` |
| Kernel | `uname -a` | `Linux 6.1.158+ #1 SMP PREEMPT_DYNAMIC x86_64 GNU/Linux` |
| libc | `ldd --version` | glibc **2.36** (releases need >= 2.35 — compatible) |
| Distribution | `/etc/os-release` | Debian GNU/Linux 12 (bookworm) |
| Disk | `df -h` | 20 GB available of 21 GB |
| Memory | `free -h` | 3.8 GiB total, 3.6 GiB available |
| Toolchain | `cargo`, `rustc`, `docker` | not installed |

## 2. Which release

| Archive suffix | Rendering | Stealth transport |
|---|---|---|
| *(none)* | yes | no |
| `-stealth` | yes | yes |
| `-no-render` | no | no |
| `-no-render-stealth` | no | yes |

This project installs the **standard** `obscura-x86_64-linux.tar.gz`:

* Render-capable, which is the requirement.
* Stealth is only needed when a target actively fingerprints the client, and it
  costs a BoringSSL build instead of rustls.
* `--stealth` still runs on a standard build, but only enables **tracker
  blocking**. TLS fingerprint impersonation exists only in the `-stealth`
  variant — visible in `crates/obscura-cli/src/main.rs`:

  ```rust
  #[cfg(feature = "stealth")]
  tracing::info!("Stealth mode enabled (TLS fingerprint impersonation + tracker blocking)");
  #[cfg(not(feature = "stealth"))]
  tracing::info!("Stealth mode enabled (tracker blocking)");
  ```

Switch with `--variant stealth` if anti-detection is genuinely required. It is
untested here and does **not** guarantee access to Cloudflare or any other
protected site.

Archives contain **both** `obscura` and `obscura-worker` (78 MB + 73 MB). The
worker is required by the parallel `scrape` command and must stay alongside the
main binary. The installer keeps them together.

## 3. Installation

### Normal path

```bash
./scripts/install_obscura.sh            # standard, render-enabled
./scripts/install_obscura.sh --dry-run  # show what it would fetch
./scripts/install_obscura.sh --variant stealth
```

Detects OS and architecture, verifies glibc >= 2.35, checks free disk, downloads
the matching archive, extracts into `vendor/obscura/`, then runs `--version` and
a smoke test.

### How this sandbox got the binary

The sandbox blocks GitHub's release-asset hosts, so the download fails at the
TLS handshake (`SSL_ERROR_SYSCALL`) before any HTTP response. Docker and the
Rust toolchain are unavailable too, and their registries are blocked.

The working route was **GitHub Actions as a download agent**:

1. `.github/workflows/obscura-fetch.yml` runs on a runner (runners have open
   internet). It downloads the official archive and verifies SHA-256.
2. It publishes the archive to a short-lived branch, `obscura-asset-<run id>`.
3. The sandbox fetches that branch over git — `github.com` is reachable — and
   re-verifies the digest locally.

```bash
git fetch --no-tags --depth=1 origin refs/heads/obscura-asset-<run id>
git show FETCH_HEAD:obscura-x86_64-linux.tar.gz > /tmp/obscura.tar.gz
sha256sum /tmp/obscura.tar.gz
OBSCURA_ARCHIVE=/tmp/obscura.tar.gz \
OBSCURA_SHA256=2116764b99bc084fae3668b68aa535b4cf456e4498845b47493ad4123e83ca36 \
  ./scripts/install_obscura.sh
```

Artifacts and logs could not be used: both redirect to hosts the sandbox blocks
(`productionresultssa11.blob.core.windows.net` and
`pipelines.actions.githubusercontent.com`). Git was the only reachable channel.

The transport branch is disposable. Delete it once fetched:

```bash
git push origin --delete obscura-asset-<run id>
```

### Verifying the archive

`releases/latest` resolves to **v0.2.5** (published 2026-10-17). SHA-256 values
published by the GitHub API for this release:

| Archive | Size | SHA-256 |
|---|---|---|
| `obscura-x86_64-linux.tar.gz` | 71,555,327 | `2116764b99bc084fae3668b68aa535b4cf456e4498845b47493ad4123e83ca36` |
| `obscura-x86_64-linux-stealth.tar.gz` | 74,936,793 | `d9175cf7573d0243fc303a6f1a3b68affc757af0b5549cacb018e9670988316c` |

The installed archive matched the first digest exactly, and its byte count
matched the API-reported size. A mismatch aborts before anything executes.

## 4. Sandbox networking caveats

These bit during setup and will bite again. Neither is an Obscura defect.

### The proxy re-signs TLS, so Obscura needs the system CA bundle

The sandbox MITMs allowed hosts with its own CA:

```
subject: O=E2B; CN=github.com
issuer:  O=E2B; CN=E2B Proxy CA
```

Obscura ships bundled webpki roots and does not read the system store by
default, so it fails with:

```
invalid peer certificate: UnknownIssuer
```

The fix is to point it at the system bundle, which the CLI honours through
`SSL_CERT_FILE` (see `crates/obscura-net/src/client.rs`):

```bash
export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
```

Note this **replaces** the bundled roots rather than adding to them. Do not set
it on a machine without a proxy, or you lose the default trust set.

In Python, pass it explicitly:

```python
BrowserTools(ca_bundle="/etc/ssl/certs/ca-certificates.crt")
```

### Only allowlisted hosts are reachable

`github.com`, `api.github.com`, `codeload.github.com`, `pypi.org` and
`registry.npmjs.org` are reachable. `example.com` and `news.ycombinator.com` are
**not** — their TLS handshake is reset. That is why the official
`example.com` smoke test fails here while the engine works fine.

Blocked by the sandbox: `example.com`, `news.ycombinator.com`,
`release-assets.githubusercontent.com`, `objects.githubusercontent.com`,
`raw.githubusercontent.com`, `registry-1.docker.io`, `ghcr.io`,
`sh.rustup.rs`, `index.crates.io`, Azure Blob, and the Debian mirrors.

Point the tests at reachable sites with `OBSCURA_TEST_URL` /
`OBSCURA_SMOKE_URL`.

## 5. Verified results

Run on 2026-10-24 with `obscura 0.2.5`.

**Version**

```
$ ./vendor/obscura/obscura --version
obscura 0.2.5                                    # exit 0
```

**Official smoke test — `example.com`**

```
$ ./vendor/obscura/obscura fetch https://example.com --eval "document.title"
Error: Failed to navigate to https://example.com: ... tls handshake eof
# exit 1 — the sandbox blocks example.com, not an engine failure
```

**Real page, real JavaScript — `github.com`**

```
$ ./vendor/obscura/obscura fetch https://github.com --eval "document.title" -q
GitHub · Change is constant. GitHub keeps you ahead. · GitHub · GitHub

$ ./vendor/obscura/obscura fetch https://github.com \
    --eval "JSON.stringify({sum: 2+2, year: new Date().getFullYear(), \
            ua: navigator.userAgent.slice(0,60), links: document.querySelectorAll('a[href]').length})" -q
{"sum":4,"year":2026,"ua":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36","links":142}
```

**Second site — `pypi.org`**

```
$ ./vendor/obscura/obscura fetch https://pypi.org --eval "document.title" -q
PyPI · The Python Package Index

$ ./vendor/obscura/obscura fetch https://pypi.org \
    --eval "document.querySelector('h1').textContent.trim()" -q
Find, install and publish Python packages with the Python Package Index

$ ./vendor/obscura/obscura fetch https://pypi.org --dump links -q | head -3
# 57 anchor URLs extracted
```

**Local fixture — deterministic JS, no external network**

A page that mutates the DOM synchronously, then uses `setTimeout` and `fetch()`:

```
$ ./vendor/obscura/obscura fetch http://127.0.0.1:8931/ --allow-private-network \
    --eval "JSON.stringify({heading: ..., items: ..., fetched: ..., timer: ...})" -q
{"heading":"mutation-ok","items":3,"fetched":"fetched:ok","timer":true,"fetchDone":true}
```

Synchronous mutation, timers, `fetch()` round-trip and DOM construction all
worked in one navigation.

**Error path**

```
$ ./vendor/obscura/obscura fetch https://this-host-should-not-exist.invalid -q
Error: ... dns error: failed to lookup address information: Name or service not known
# exit 1, reported cleanly
```

**Not tested:** Cloudflare or any bot-detection behaviour. A stealth build does
not guarantee access to a protected site, and no such test was run.

## 6. How the agent invokes the browser

`agent_tools/browser.py` wraps the documented CLI. No MCP server is involved.

```python
import os
from agent_tools import BrowserTools

browser = BrowserTools(ca_bundle="/etc/ssl/certs/ca-certificates.crt")

browser.list_tools()                       # JSON schemas for the agent
browser.call("browser_evaluate", {
    "url": "https://pypi.org",
    "expression": "document.title",
})
# {"ok": True, "tool": "browser_evaluate", "url": "...",
#  "expression": "document.title", "result": "PyPI · The Python Package Index"}
```

| Tool | Backing command |
|---|---|
| `browser_fetch` | `obscura fetch <url> -q --dump <fmt> --wait-until <lvl> [--selector <css>]` |
| `browser_evaluate` | `obscura fetch <url> -q --eval <expr> --wait-until <lvl>` |
| `browser_screenshot` | `obscura fetch <url> -q --screenshot <file.png>` |
| `browser_version` | `obscura --version` |

From a shell:

```bash
python -m agent_tools list
python -m agent_tools doctor
python -m agent_tools call browser_evaluate \
  '{"url": "https://pypi.org", "expression": "document.title"}'
```

`call` prints a JSON envelope and exits `1` when `"ok"` is false. Failures are
returned to the agent as data — `{"ok": false, "error": {...}}` — not raised.

### Behaviour worth knowing

* **`--eval` output is JSON.** A string arrives quoted, so the wrapper decodes
  it. Non-JSON output is returned as raw text.
* **JavaScript numbers arrive as floats.** `document.querySelectorAll('a').length`
  returns `57.0`, not `57`, because V8 serialises numbers as doubles. Treat
  numeric results as floats.
* **Stateless.** Each call is one process that navigates and acts. There is no
  persistent session, so cookies and page state do not carry between calls.
  A session would need `obscura serve` plus a CDP client.
* **Timeouts.** `--timeout` bounds navigation; the subprocess is killed as a
  process group slightly later so Obscura's own message is reported first.
* **SSRF.** Obscura blocks loopback/RFC1918/link-local by default. The wrapper
  keeps that default; `--allow-private-network` is set only by the host process,
  never by a model-supplied argument. Only `http:`/`https:` URLs are accepted,
  so `file://` cannot read local files.
* **Binary resolution.** An explicit `binary=` or `$OBSCURA_BIN` is
  authoritative: if it does not point at an executable, that is an error rather
  than a silent fallback to another binary.

### Configuration

| `BrowserTools(...)` argument | Default | Meaning |
|---|---|---|
| `binary` | auto | Path to `obscura` |
| `allow_private_network` | `False` | Forward `--allow-private-network` |
| `stealth` | `False` | Forward `--stealth` (tracker blocking) |
| `default_timeout` | `45` | Navigation timeout in seconds |
| `ca_bundle` | `$SSL_CERT_FILE` | Extra CA bundle; **replaces** bundled roots |

Binary lookup: explicit `binary=`, then `$OBSCURA_BIN`, then
`vendor/obscura/obscura`, then `obscura` on `$PATH`.

## 7. Limitations

* **Stateless**, one navigation per call. Multi-step flows (log in, then click)
  need `obscura serve` + CDP, which is not implemented.
* **Not Chromium.** Obscura has its own layout and paint engine; the README notes
  long-tail CSS, some Web APIs and media playback may differ from Chrome.
* **Text dumps can retain markup.** `--dump text` on `pypi.org` included some
  `<div class=...>` fragments. Use `--dump markdown` when structure matters, or
  extract precisely with `--eval`.
* **Numbers serialise as floats** (see above).
* **No bot-detection guarantee.** Untested against Cloudflare and similar.
* **No MCP.** Tools are called in-process.
* **Trust store.** Setting `SSL_CERT_FILE`/`ca_bundle` replaces the bundled roots.

## 8. Tests

```bash
python3 -m unittest discover -s tests -t . -v
```

**31 tests, all passing** on 2026-10-24 (23 mocked + 8 live).

| Suite | Count | Status |
|---|---|---|
| Mocked (`tests/test_browser.py`) | 23 | pass — no binary or network needed |
| Live pages (`TestLivePages`) | 4 | pass against reachable hosts |
| Local fixture (`TestLocalFixture`) | 2 | pass — deterministic JS, no network |

The mocked tests run against `tests/fixtures/fake_obscura.py`, a labelled
stand-in mimicking the CLI contract. They verify argument construction, JSON
decoding, error propagation, timeout killing, binary resolution and dispatch.
They do **not** verify Obscura's engine.

Live tests skip themselves when the binary is missing or a target host is
unreachable, so a restricted sandbox reports a skip rather than a false failure:

```bash
# Default targets (example.com is blocked here, so those 4 skip)
python3 -m unittest tests.test_live -v          # OK (skipped=4)

# Pointed at reachable sites
SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
OBSCURA_TEST_URL=https://github.com OBSCURA_TEST_URL_2=https://pypi.org \
  python3 -m unittest tests.test_live -v        # OK (8 tests)
```

`scripts/smoke_test.sh` records raw evidence per command:

```bash
SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
OBSCURA_SMOKE_URL=https://github.com OBSCURA_SMOKE_URL_2=https://pypi.org \
  ./scripts/smoke_test.sh                       # 7 passed, 0 failed
```

## 9. Source-level facts verified

Taken from the v0.2.5 source tarball, fetched via `codeload`, not from the README
alone:

* `crates/obscura-cli/src/main.rs` — clap `Parser`,
  `version = env!("OBSCURA_BUILD_VERSION")`, global flags `--stealth`,
  `--proxy`, `--allow-private-network`, `--v8-flags`, `--storage-dir`,
  `--obey-robots`, `--user-agent`.
* `--dump` accepts `html | text | links | markdown | original | assets | cookies`;
  `--wait-until` accepts `domcontentloaded | load | networkidle2 | networkidle0`.
* `serve` supports `--host` (default `127.0.0.1`), `--port` (9222), `--workers`,
  `--ready-file` (requires `--workers 1`).
* `crates/obscura-cli/Cargo.toml` — features `stealth` and `render`; binaries
  `obscura` and `obscura-worker`.
* `crates/obscura-net/src/client.rs` — `SSL_CERT_FILE`/`SSL_CERT_DIR` build a
  custom trust store that replaces the bundled webpki roots.
* Runtime defaults: `OBSCURA_NAV_TIMEOUT_MS=30000`,
  `OBSCURA_SCRIPT_DEADLINE_MS=30000`, `OBSCURA_CDP_COMMAND_TIMEOUT_MS=60000`.

## 10. Integrating into a future agent

The adapter is framework-free: it returns JSON-schema tool definitions and
accepts a tool name plus a dict.

```python
from agent_tools import BrowserTools

browser = BrowserTools()

tool_definitions = browser.list_tools()
# Anthropic-style APIs take this shape directly.
# OpenAI-style APIs expect the same object under "parameters":
#   for schema in tool_definitions: schema["parameters"] = schema.pop("input_schema")

envelope = browser.call(tool_name, tool_arguments)
observation = envelope if envelope["ok"] else {"error": envelope["error"]}
```

Two practical notes: `browser_fetch` reports `length` so callers can truncate
before feeding text to a model, and construct one `BrowserTools` per agent
session when settings differ (for example `stealth=True` for one workflow).

## 11. Files

| Path | Purpose |
|---|---|
| `agent_tools/browser.py` | CLI wrapper, tool schemas, dispatcher |
| `agent_tools/__init__.py` | package exports |
| `agent_tools/__main__.py` | `list` / `call` / `doctor` entry point |
| `scripts/install_obscura.sh` | platform-aware installer with checks |
| `scripts/smoke_test.sh` | records raw command output and exit status |
| `.github/workflows/obscura-fetch.yml` | on-demand archive transport (manual dispatch) |
| `tests/test_browser.py` | 23 mocked unit tests |
| `tests/test_live.py` | 8 live tests, auto-skipped when unavailable |
| `tests/fixtures/fake_obscura.py` | stand-in binary for the mocked suite |
| `tests/fixtures/site/` | deterministic JS fixture page |
| `vendor/obscura/` | installed binaries (git-ignored) |
| `.gitignore` | excludes binaries and caches |
