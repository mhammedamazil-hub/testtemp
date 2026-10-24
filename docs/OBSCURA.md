# Obscura integration

[Obscura](https://github.com/h4ckf0r0day/obscura) is a headless browser engine
written in Rust. It executes real JavaScript through V8 and implements the Chrome
DevTools Protocol, without bundling Chromium. This project uses it as the engine
behind the agent's browser tools.

> **Status: not yet running.** The official binary could not be downloaded in
> this sandbox. See [Blocker](#blocker-why-the-binary-is-not-installed). The
> integration code and installer are complete and unit-tested against a
> stand-in; the live smoke test has **not** been performed.

---

## 1. Environment check

Recorded on the sandbox before any install attempt:

| Check | Command | Result |
|---|---|---|
| Architecture | `uname -m` | `x86_64` |
| Kernel | `uname -a` | `Linux e2b.local 6.1.158+ #1 SMP PREEMPT_DYNAMIC x86_64 GNU/Linux` |
| libc | `ldd --version` | `ldd (Debian GLIBC 2.36-9+deb12u14) 2.36` |
| Distribution | `/etc/os-release` | Debian GNU/Linux 12 (bookworm) |
| Disk | `df -h /home/user` | 21 G total, 20 G available |
| Memory | `free -h` | 3.8 Gi total, 3.6 Gi available |
| Toolchain | `cargo`, `rustc`, `docker` | not installed (no source build possible) |
| Runtimes present | `python3`, `node`, `npm` | 3.11.2, v22.22.3, 10.9.8 |

**glibc compatibility.** Official Linux builds target Ubuntu 22.04 and need
glibc >= 2.35. This host has 2.36, so the `x86_64-linux` archive is the correct
choice and is expected to run. The installer enforces this check before
downloading.

## 2. Which release

The release matrix (from the README and the v0.2.5 asset list):

| Archive suffix | Rendering | Stealth transport |
|---|---|---|
| *(none)* | yes | no |
| `-stealth` | yes | yes |
| `-no-render` | no | no |
| `-no-render-stealth` | no | yes |

This project targets the **standard** `obscura-x86_64-linux.tar.gz`:

* It is render-capable, which is the requirement.
* Stealth is needed only when a target actively fingerprints the client, and it
  has a cost: it compiles BoringSSL/wreq instead of rustls and is ~3 MB larger.
* `--stealth` still works on a standard build, but only enables **tracker
  blocking**. TLS fingerprint impersonation is compiled in only in the
  `-stealth` variant. This is visible in `crates/obscura-cli/src/main.rs`:

  ```rust
  #[cfg(feature = "stealth")]
  tracing::info!("Stealth mode enabled (TLS fingerprint impersonation + tracker blocking)");
  #[cfg(not(feature = "stealth"))]
  tracing::info!("Stealth mode enabled (tracker blocking)");
  ```

Switch with `--variant stealth` if anti-detection is genuinely needed. It is not
tested here, and it does **not** guarantee access to Cloudflare or any other
protected site.

Release archives contain **both** `obscura` and `obscura-worker`. The worker is
required by the parallel `scrape` command and must stay in the same directory.
The installer keeps them together.

## 3. Install

```bash
./scripts/install_obscura.sh              # standard, render-enabled
./scripts/install_obscura.sh --variant stealth
./scripts/install_obscura.sh --dry-run    # show what it would fetch
./scripts/install_obscura.sh --force      # reinstall
```

The script detects OS and architecture, verifies glibc >= 2.35 on Linux, checks
free disk, downloads the matching archive, extracts `obscura` and
`obscura-worker` into `vendor/obscura/`, then runs `--version` and the
`example.com` smoke test. It exits non-zero with a diagnosis if any step fails.

Equivalent manual install:

```bash
curl -fL -o obscura.tar.gz \
  https://github.com/h4ckf0r0day/obscura/releases/latest/download/obscura-x86_64-linux.tar.gz
tar xzf obscura.tar.gz -C vendor/obscura
./vendor/obscura/obscura --version
./vendor/obscura/obscura fetch https://example.com --eval "document.title"
```

Binaries are git-ignored: each is ~70 MB and machine-specific. Do not commit them.

### Verifying the archive

`releases/latest` currently resolves to **v0.2.5** (published 2026-10-17). The
GitHub API publishes a SHA-256 `digest` for every asset; these are the values for
this release:

| Archive | Size | SHA-256 |
|---|---|---|
| `obscura-x86_64-linux.tar.gz` | 71,555,327 | `2116764b99bc084fae3668b68aa535b4cf456e4498845b47493ad4123e83ca36` |
| `obscura-x86_64-linux-stealth.tar.gz` | 74,936,793 | `d9175cf7573d0243fc303a6f1a3b68affc757af0b5549cacb018e9670988316c` |

Pass `OBSCURA_SHA256=<hex>` to the installer to enforce one. This matters most
when the archive did not come from the script's own download:

```bash
OBSCURA_ARCHIVE=~/obscura-x86_64-linux.tar.gz \
OBSCURA_SHA256=2116764b99bc084fae3668b68aa535b4cf456e4498845b47493ad4123e83ca36 \
  ./scripts/install_obscura.sh
```

A mismatch aborts the install before anything is executed.

## 4. Blocker: why the binary is not installed

The download fails at the TLS handshake, before any HTTP response:

```
curl: (35) OpenSSL SSL_connect: SSL_ERROR_SYSCALL
      in connection to release-assets.githubusercontent.com:443
```

Diagnosis, each step run separately:

| Probe | Result | Interpretation |
|---|---|---|
| `getent hosts release-assets.githubusercontent.com` | resolves to `185.199.108-111.133` | DNS is fine |
| `/dev/tcp/.../443` connect | succeeds | TCP is not blocked |
| `curl -v` | dies immediately after the TLS `Client hello` | connection is cut by the egress proxy |
| `openssl s_client` | `CONNECTED`, cert verifies `0 (ok)`, then `unexpected eof while reading` | server cert is valid; the proxy severs the stream |
| `wget` (different TLS stack) | exit 4, 0-byte file | not a curl-specific quirk |
| `api.github.com/repos/.../releases/assets/650858054` | `302` to a signed `release-assets.githubusercontent.com` URL | the API cannot proxy the bytes either |
| `codeload.github.com/.../tar.gz/refs/tags/v0.2.5` | `200 OK`, 19 MB downloaded | source archives are reachable; release assets are not |

So outbound access to GitHub's **release-asset** hosts is filtered, while
`github.com`, `api.github.com` and `codeload.github.com` are allowed. This is an
infrastructure restriction, not a fault in the archive: no bytes were received,
so the download was not retried further.

Alternatives examined and rejected:

* **NixOS / `nix-env -iA nixpkgs.obscura`** — no `nix` on this host.
* **Docker image** — no `docker` or `podman`.
* **Build from source** — no `cargo`/`rustc`, crates.io is not reachable, and an
  embedded V8 build is not viable here.
* **npm / PyPI** — the only `obscura`-named npm packages are third-party
  repackages of the binary, not the official build; the PyPI `obscura` package
  is an unrelated file-encryption library. Neither was installed.

To finish the install, supply the official archive by either:

1. Allowing `release-assets.githubusercontent.com` (and
   `objects.githubusercontent.com`) for the sandbox, then re-running the
   installer; or
2. Placing `obscura-x86_64-linux.tar.gz` at `vendor/obscura/` — the installer
   picks it up automatically — or setting `OBSCURA_ARCHIVE=/path/to/archive`.

## 5. How the agent invokes the browser

`agent_tools/browser.py` wraps the documented CLI. No MCP server is involved.

```python
from agent_tools import BrowserTools

tools = BrowserTools()                      # finds vendor/obscura/obscura, $OBSCURA_BIN, or PATH

tools.list_tools()                          # JSON schemas for the agent
tools.call("browser_evaluate", {
    "url": "https://example.com",
    "expression": "document.title",
})
# {"ok": True, "tool": "browser_evaluate", "url": "...",
#  "expression": "document.title", "result": "Example Domain"}
```

Tools exposed:

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
  '{"url": "https://example.com", "expression": "document.title"}'
```

`call` prints a JSON envelope and exits `1` when `"ok"` is false. Failures are
returned to the agent as data — `{"ok": false, "error": {...}}` — rather than
raised, so the agent can read the message and adapt.

### Behaviour worth knowing

* **`--eval` output is JSON.** A string title arrives quoted (`"Example Domain"`),
  so the wrapper decodes it. If the output is not valid JSON, it is returned as
  raw text.
* **Stateless.** Each call is one process: navigate and evaluate together. There
  is no persistent session, so cookies/localStorage do not carry between calls.
  A session would need `obscura serve` plus a CDP client.
* **Timeouts.** The navigation timeout is passed to `--timeout`; the subprocess
  is killed as a process group a little later, so Obscura's own timeout fires
  first and its message is reported.
* **SSRF.** Obscura blocks loopback/RFC1918/link-local by default. The wrapper
  keeps that default; `--allow-private-network` is set only by the host process
  (`BrowserTools(allow_private_network=True)`), never by a model-supplied
  argument. Only `http:`/`https:` URLs are accepted, so `file://` cannot be used
  to read local files.

## 6. Tests

```bash
python3 -m unittest discover -s tests -t . -v
```

21 tests pass. They run against `tests/fixtures/fake_obscura.py`, a stand-in
that mimics the CLI contract (version output, JSON-encoded `--eval`, dumps,
exit codes, a hanging URL for the timeout path). They verify argument
construction, JSON decoding, error propagation, timeout killing and tool
dispatch. **They do not verify Obscura's engine.**

Not yet run, because they need the real binary:

- `./vendor/obscura/obscura --version`
- `./vendor/obscura/obscura fetch https://example.com --eval "document.title"`
- JavaScript execution and title extraction on a second, JS-heavy site
- Any Cloudflare or bot-detection behaviour

## 7. Source-level facts verified

Taken from the v0.2.5 source tarball (fetched via `codeload`, which is reachable)
rather than from the README alone:

* `crates/obscura-cli/src/main.rs` — clap `Parser`, `version = env!("OBSCURA_BUILD_VERSION")`, global flags `--stealth`, `--proxy`, `--allow-private-network`, `--v8-flags`, `--storage-dir`, `--obey-robots`, `--user-agent`.
* `--dump` accepts `html | text | links | markdown | original | assets | cookies`; `--wait-until` accepts `domcontentloaded | load | networkidle2 | networkidle0`.
* `serve` supports `--host` (default `127.0.0.1`), `--port` (9222), `--workers`, `--ready-file` (requires `--workers 1`).
* `crates/obscura-cli/Cargo.toml` — features `stealth` and `render`; binaries `obscura` and `obscura-worker`.
* Runtime defaults: `OBSCURA_NAV_TIMEOUT_MS=30000`, `OBSCURA_SCRIPT_DEADLINE_MS=30000`, `OBSCURA_CDP_COMMAND_TIMEOUT_MS=60000`.

## 8. Files

| Path | Purpose |
|---|---|
| `agent_tools/browser.py` | CLI wrapper, tool schemas, dispatcher |
| `agent_tools/__init__.py` | package exports |
| `agent_tools/__main__.py` | `list` / `call` / `doctor` entry point |
| `scripts/install_obscura.sh` | platform-aware installer with checks |
| `tests/test_browser.py` | 21 unit tests |
| `tests/fixtures/fake_obscura.py` | stand-in binary for tests |
| `vendor/obscura/` | install target (git-ignored) |
| `.gitignore` | excludes binaries and caches |
