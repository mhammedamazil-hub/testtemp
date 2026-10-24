# Autonomous browser-agent run

A recorded end-to-end run of the browser agent on 2026-10-24, using the installed
Obscura 0.2.5 binary through `agent_tools`. Every output below was produced by
running the command shown; nothing is illustrative.

## What was actually run

`examples/browser_agent_demo.py` is a **deterministic, rule-based agent**. It
follows a fixed policy:

1. Try candidate sites in order, keep the first that renders.
2. Parse every anchor from the rendered DOM.
3. Score links by keyword overlap with the objective; follow the best one on
   the same host.
4. Scan the destination's rendered text for version-like strings.
5. Re-query the live DOM to confirm the chosen string is really there.

It contains no language model. It is not a general autonomous agent: swap
`choose_target` and the extraction step for model calls and it would be one.
That was deliberately left out so the run is reproducible.

**Division of labour, stated plainly:** I chose the objective and the candidate
list. The agent chose which candidate to use, which link to follow, what to
extract, and how to verify it.

## Objective

> Find the most recent release version of Obscura listed on the project's GitHub
> pages, and confirm it is present in the rendered DOM.

Candidates offered: the GitHub repo, a PyPI package page, and a Hacker News item
(expected to fail — that host is blocked here).

## Run

```bash
export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
python3 examples/browser_agent_demo.py browse
```

Agent trace (stderr):

```
OBJECTIVE: Find the most recent release version of Obscura listed on the project's GitHub pages, and confirm it is present in the rendered DOM.
CHOSE    : https://github.com/h4ckf0r0day/obscura
START    : https://github.com/h4ckf0r0day/obscura
LINKS    : 251 found on start page
FOLLOW   : https://github.com/h4ckf0r0day/obscura/releases  (Releases)
FACT     : v0.2.5  (candidates: ['v0.2.5', 'v0.2.4', 'v0.2.3', 'v0.2.2', 'v0.2.1', 'v0.2.0'])
VERIFY   : inBodyText=True occurrences=30
```

Commands executed, in order:

```bash
# 1. choose target / inspect start page
obscura fetch https://github.com/h4ckf0r0day/obscura -q --dump links --wait-until load --timeout 45

# 2. extract from the page the agent followed
obscura fetch https://github.com/h4ckf0r0day/obscura/releases -q --eval \
  "JSON.stringify({title: document.title,
    headings: Array.from(document.querySelectorAll('h1,h2,h3')).map(e=>e.textContent.trim()).filter(Boolean).slice(0,12),
    versions: Array.from(new Set((document.body.innerText||'').match(/\bv?\d+\.\d+\.\d+\b/g)||[])).slice(0,15),
    bodyLength: (document.body.innerText||'').length})" \
  --wait-until load --timeout 45

# 3. verify the claim against the live DOM
obscura fetch https://github.com/h4ckf0r0day/obscura/releases -q --eval \
  "JSON.stringify({inBodyText: (document.body.innerText||'').includes(\"v0.2.5\"),
    occurrences: ((document.body.innerHTML||'').split(\"v0.2.5\").length - 1),
    inTitle: document.title.includes(\"v0.2.5\"),
    checkedUrl: location.href})" \
  --wait-until load --timeout 45
```

Extraction result:

```json
{
  "title": "Releases · h4ckf0r0day/obscura · GitHub",
  "headings": ["Releases: h4ckf0r0day/obscura", "v0.2.5", "...", "v0.2.4", "..."],
  "versions": ["v0.2.5", "v0.2.4", "v0.2.3", "v0.2.2", "v0.2.1", "v0.2.0"],
  "bodyLength": 6249
}
```

Verification result:

```json
{
  "inBodyText": true,
  "occurrences": 30,
  "inTitle": false,
  "checkedUrl": "https://github.com/h4ckf0r0day/obscura/releases"
}
```

## Evidence for each claim

| # | Claim | Source URL | Evidence | Independent check |
|---|---|---|---|---|
| 1 | The start page rendered 251 anchors, parsed from the DOM rather than the raw HTTP body. | `https://github.com/h4ckf0r0day/obscura` | `--dump links` returned 251 lines | — |
| 2 | The agent followed the "Releases" link to `/releases`. | `https://github.com/h4ckf0r0day/obscura/releases` | link chosen by keyword score, printed in the trace | — |
| 3 | The page lists `v0.2.5` as the most recent version, followed by v0.2.4 … v0.2.0. | `https://github.com/h4ckf0r0day/obscura/releases` | extraction returned that ordered list | — |
| 4 | `v0.2.5` is present in the rendered DOM (30 occurrences). | `https://github.com/h4ckf0r0day/obscura/releases` | `document.body.innerText.includes(...)` → `true`; 30 matches in `innerHTML` | — |
| 5 | `v0.2.5` really is the latest release. | `https://api.github.com/repos/h4ckf0r0day/obscura/releases/latest` | — | API returned `tag_name=v0.2.5`, `published_at=2026-10-17T14:55:24Z` |
| 6 | The installed binary is that same version. | local | `obscura --version` → `obscura 0.2.5` | — |

Claim 5 is the one that matters: the browser's answer was checked against the
GitHub REST API, which does not go through Obscura at all. They agree.

## JavaScript execution

### Real page

```bash
obscura fetch https://github.com/h4ckf0r0day/obscura --eval \
  "JSON.stringify({title:document.title,textLength:(document.body.innerText||'').length,
    scriptTags:document.querySelectorAll('script').length,
    domNodes:document.getElementsByTagName('*').length})" -q
```

```json
{"title":"GitHub - h4ckf0r0day/obscura: The headless browser for AI agents and web scraping · GitHub",
 "textLength":22687,"scriptTags":16,"domNodes":2947}
```

0.86 s. Dynamic routes also render:

| Page | Exit | Time | Rendered text | DOM nodes |
|---|---|---|---|---|
| `github.com/h4ckf0r0day/obscura/commits/main` | 0 | 0.69 s | 8,004 chars | 2,431 |
| `github.com/h4ckf0r0day/obscura/issues` | 0 | 0.64 s | 6,396 chars | 2,004 |
| `pypi.org/project/requests/` | 0 | 1.11 s | 5,042 chars | 4,368 |

These sites server-render, so their existence does not by itself prove JS runs.
The controlled test below does.

### Controlled JS-only page

`examples/fixtures/spa/` serves an empty shell; every visible node is created by
an ES module after an async `fetch()` and a `setTimeout`.

Raw HTTP body — no JavaScript executed:

```
<!DOCTYPE html>
...
  <div id="app"></div>
  <script type="module" src="app.js"></script>
</body>
</html>
```

312 bytes, **zero** occurrences of "Releases".

Rendered DOM:

```json
{"title":"Obscura — v0.2.5",
 "h1":"Obscura",
 "tags":["v0.2.5","v0.2.4","v0.2.3"],
 "total":"Total releases: 3",
 "mounted":true,
 "nodes":17}
```

The title was set by JS, the heading was created by JS, the three release tags
came from an in-page `fetch()` of `data.json`, and the summary only appeared
after a 120 ms timer. That is a real engine exercising modules, `await`,
`fetch`, timers and DOM construction.

## Navigation failures, reported as they happened

| URL | Result | Cause |
|---|---|---|
| `https://example.com` | exit 1, `tls handshake eof` | the sandbox allowlist blocks this host — **not** an Obscura fault |
| `https://news.ycombinator.com` | exit 1, `tls handshake eof` | same |
| `https://this-host-should-not-exist.invalid` | exit 1, `dns error: Name or service not known` | correctly reported |

The official `example.com` smoke test therefore still fails here, while the
engine demonstrably works on reachable sites.

## Cloudflare-protected pages

Tested four sites known to sit behind Cloudflare. This was read-only observation
of a single GET per site; nothing was attempted to defeat a challenge.

| URL | Classification | HTTP response |
|---|---|---|
| `https://www.cloudflare.com/` | `blocked_before_http` | none |
| `https://discord.com/` | `blocked_before_http` | none |
| `https://www.reddit.com/` | `blocked_before_http` | none |
| `https://chat.openai.com/` | `blocked_before_http` | none |

All four fail at the TLS handshake with `SSL_ERROR_SYSCALL` before a single
HTTP header arrives. **No Cloudflare behaviour could be observed.**

That distinction matters:

- **Normal access** — not observed.
- **Challenge presented** (HTTP 403/503 with `cf-mitigated` / `cf-ray`) — not observed.
- **Access after a challenge** — not observed, and never attempted.

The sandbox permits only an allowlist of hosts, so a Cloudflare test would be
indistinguishable from a network block. The classifier in
`probe_cloudflare()` reads `cf-ray` and `cf-mitigated` headers to tell these
apart, but it has **never seen a real challenge** and is therefore unverified.

**No claim is made that Obscura bypasses Cloudflare, with or without stealth.**

## Limitations

- The agent is deterministic, not intelligent. It cannot read a page and decide
  what matters; it matches keywords and regexes.
- One navigation per call; no session. It cannot log in and then click.
- Sites are fixed by the candidate list; the "choice" is reachability only.
- Cloudflare behaviour is untested (see above).
- The demo is read-only and never submits a form.

## Reproduce

```bash
export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

python3 examples/browser_agent_demo.py browse      # the run above
python3 examples/browser_agent_demo.py js-heavy    # real-page timing probe
python3 examples/browser_agent_demo.py cloudflare https://www.cloudflare.com/

# JS-only fixture
(cd examples/fixtures/spa && python3 -m http.server 8942 --bind 127.0.0.1 &)
obscura fetch http://127.0.0.1:8942/ --allow-private-network \
  --eval "document.title" -q
```
