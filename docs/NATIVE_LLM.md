# Using the native Arena agent LLM as the decision-maker

**Question asked:** can the Obscura research agent use *Arena's own* model to
choose its actions, instead of a separate provider and API key?

**Short answer:** not by API. Sandbox code cannot call Arena's model. But the
Arena agent *can* call into the sandbox, so the direction is inverted: the
native model drives the session turn by turn through the shell. That is
implemented, and it ran a real 7-cycle task.

---

## 1. What the environment actually exposes

Probed on 2026-10-25 inside the sandbox. This is the evidence, not an
assumption:

| Probe | Result | Conclusion |
|---|---|---|
| `env` (all variables) | `E2B_*`, `GH_TOKEN`, `GITHUB_TOKEN`, `HOME`, `PATH`… | **no model key, no API base URL** |
| `env \| grep -i 'llm\|openai\|anthropic\|model\|inference'` | no matches | nothing configured |
| Listening sockets | `:22`, `:111`, one ephemeral port | no local inference server |
| Unix sockets | only `rpcbind` | no model socket |
| `arena`, `e2b`, `llm`, `claude`, `openai` on `PATH` | all absent | no client CLI |
| `/opt/*arena*`, `~/.arena` | none | no runtime artefacts |
| `api.openai.com`, `api.anthropic.com`, `generativelanguage.googleapis.com`, `openrouter.ai`, `api.together.xyz`, `api.groq.com`, `api.mistral.ai` | all `SSL_ERROR_SYSCALL` | **no provider is reachable** |
| `arena.ai`, `api.arena.ai`, `agent.arena.ai`, `api.e2b.dev` | all blocked | no Arena gateway either |
| `host.docker.internal` | does not resolve | no host bridge |
| `E2B_EVENTS_ADDRESS` | `192.0.2.1` (TEST-NET-1 placeholder) | not a usable endpoint |
| `169.254.169.254` | `401` | cloud metadata, not a model |

The tool surface Arena gives this agent — shell, file edits, web fetch, image
and speech generation, connectors — contains **no "invoke the model" tool**.
Those tools are invoked *by* the model, and are not callable *from* sandbox
code.

### Precise limitation

> Arena executes the model in its own orchestration layer. The sandbox is a
> plain Debian VM with an egress allowlist. There is no API key, no local
> endpoint, and no route to any inference host. Sandbox Python therefore
> cannot call the model that is reasoning about it.

Even with a key for a third-party provider, the sandbox could not reach it:
every provider host is blocked at the TLS handshake.

## 2. The supported alternative

Arena's supported interface is the tool-calling loop itself. The model can run
`bash`. So the model becomes the policy, and the sandbox becomes the tool:

```
Arena agent turn  ──bash──>  python -m agent.reason state
                     <──    JSON: objective, URL, title, text, links,
                             previous actions, ledger, remaining budget
Arena agent turn  ──bash──>  python -m agent.reason act --json '{...}'
                     <──    observation + budget
                             (repeat until finish)
```

This is genuinely the native model deciding at runtime:

* The payload in step 1 is exactly what the model reads before deciding.
* Each decision is made in a real model turn, not by code.
* The heuristic is only used when no decision arrives.

It is **not** sandbox code calling a model. That distinction is the honest
boundary, and it is why no API key was needed.

## 3. Implementation

| File | Role |
|---|---|
| `agent/native.py` | session state, strict decision schema, persistence, link presentation, fallback |
| `agent/reason.py` | CLI: `start` / `state` / `act` / `fallback` / `result` |
| `agent/agent.py` | added `begin()`, `step()`, `finish_externally()` so one step can be driven from outside |
| `evals/native_trace.json` | recorded transcript of the real run |
| `tests/test_native.py` | 23 offline tests |

### What the model is given (requirement 1)

```json
{
  "objective":       {"question": "...", "start_url": "...", "max_hops": 2},
  "current_url":     "https://github.com/.../releases",
  "page_title":      "Releases · h4ckf0r0day/obscura · GitHub",
  "page_text":       "<first 4000 chars of rendered innerText>",
  "current_links":   [{"index": 167, "url": "...", "text": "Releases"}],
  "previous_actions":[{"step": 1, "action": "navigate", "reason": "..."}],
  "source_ledger":   {"claims": [], "total": 0, "verified": 0},
  "budget":          {"remaining_steps": 6, "remaining_pages": 4,
                      "remaining_seconds": 87.6},
  "last_failure":    null
}
```

### Strict decision schema (requirement 2)

Exactly one JSON object, no extra fields:

```json
{"action": "navigate", "url": "https://…", "reason": "…"}
{"action": "follow",   "link_index": 167,   "reason": "…"}
{"action": "eval",     "expression": "…",   "reason": "…"}
{"action": "finish",   "claim": "…", "evidence": "…", "summary": "…"}
```

Rejected, not coerced: unknown actions, unknown fields, non-object payloads,
non-http(s) schemes, out-of-range link indices, expressions over 2000 chars,
and — importantly — **a claim whose evidence is not verbatim page text**.

### Execution and verification (requirements 3, 4, 5)

`act` validates, executes through the existing adapter, appends the observation,
and re-persists. Every decision stores a fingerprint of the state it was based
on and a hash of its own JSON, so the transcript can be audited.

On `finish`, the claim goes into the source ledger and is then re-verified by a
second DOM query for the evidence string.

### Preserved protections (requirement 6)

* URL validation and SSRF blocking: unchanged, still in `agent_tools/browser.py`
  (http/https only; loopback/RFC1918 blocked unless the host opts in).
* Bounded retries, step/page/time budget: enforced inside `step()`, so an
  external driver cannot exceed them.
* Failure classification: unchanged — `network_blocked`, `tls_trust`,
  `dns_failure`, `bot_challenge`, `timeout`, `browser_error`.
* Heuristic policy: retained and reachable via `python -m agent.reason fallback`.

### Navigate safety

By default `navigate` only accepts the start URL or a URL observed on a page
(or surfaced by `eval`, resolved against the page it ran on). An invented URL is
rejected. `--allow-unseen-urls` exists for the case below and is recorded in the
trace when used.

## 4. The real run — 7 cycles

Task: *"What is the latest Obscura release tag, and how many downloadable assets
does that release publish?"*

| # | Action | Decided by | Result |
|---|---|---|---|
| 1 | `navigate` repo page | native model | 19,997 chars, 248 links |
| 2 | `follow` index 167 | native model | Releases page, 300 links |
| 3 | `eval` tag anchors | native model | found `/releases/tag/v0.2.5…v0.2.2` |
| 4 | `navigate` tag page | native model | "Release v0.2.5", 13,507 chars |
| 5 | `eval` asset links | native model | `[]` — assets are lazy-loaded |
| 6 | `navigate` expanded_assets | native model | 20 asset filenames |
| 7 | `finish` | native model | claim recorded + DOM-verified |

Transcript: `evals/native_trace.json`.

```
status  : complete
answer  : Release v0.2.5 publishes 20 downloadable assets, including
          obscura-x86_64-linux.tar.gz.
claim verified: re-queried DOM, evidence present (1 occurrence)
source  : https://github.com/h4ckf0r0day/obscura/releases/expanded_assets/v0.2.5
steps   : 6 of 8   pages: 6 of 6
```

**Independent cross-check**, unrelated to Obscura:

```
$ gh api repos/h4ckf0r0day/obscura/releases/tags/v0.2.5 --jq '"tag=\(.tag_name) assets=\(.assets|length)"'
tag=v0.2.5  assets=20
```

The count matches exactly.

### Problems this run exposed (and what was done)

1. **The model could not see the link it needed.** Only the first 80 of 248
   anchors were shown, and `Releases` sat at index 167. Fixed: links are now
   de-duplicated, in-page anchors dropped, and sorted so links under the start
   page's own path rank first. `Releases` became visible at index 167.
2. **`eval` results were not navigable.** A URL discovered by JavaScript was
   rejected by the navigate allowlist. Fixed: eval-discovered URLs are resolved
   against their page and added to the known set.
3. **GitHub lazy-loads release assets** through an `<include-fragment>` that the
   engine does not fetch. The asset list was unreachable through the link table.
   Cycle 6 used `--allow-unseen-urls` for the fragment URL read from the DOM,
   and the trace records that flag was used.
4. **Fallback lost its own state.** `_persist()` wrote the runner's untouched
   agent state over the fallback's history and claims. Fixed and covered by a
   regression test.
5. **One self-inflicted failure:** a session started without `SSL_CERT_FILE`
   hit the sandbox TLS proxy and was classified `tls_trust`. That is the
   classifier working, but it cost a step; the session was restarted.

## 5. Tests

```bash
python3 -m unittest discover -s tests -t .
Ran 82 tests — OK
```

| Suite | Count | Notes |
|---|---|---|
| `tests/test_browser.py` | 23 | adapter |
| `tests/test_agent.py` | 28 | v2 loop, budgets, ledger, policies |
| `tests/test_native.py` | 23 | **new**: schema, persistence, budgets, fallback |
| `tests/test_live.py` | 8 | real Obscura |

`test_native.py` covers: every rejection rule in the schema, session round-trip
across separate runner instances (as separate processes), elapsed-time survival,
budget exhaustion across processes, finished-session rejection, fallback state
preservation, link ranking, and eval-discovered URLs.

The v2 evaluation still passes 7/7.

## 6. What is and is not claimed

* **Arena's native model did the deciding** in the run above — seven real model
  turns, each recorded with the state it saw.
* **No separate provider was used.** No key was requested, created or stored.
* **No model was called from inside the sandbox**, because that is not possible
  here. The code that runs in the sandbox is a decision *gateway*, not a model
  client.
* **The LLMSelector path is still unexercised against a real model.** It exists
  for deployments that do have network access to a provider; here it correctly
  reports itself unavailable.
* **Cloudflare remains untested and unclaimed.** Both probed hosts still fail
  before any HTTP response.

## 7. Reproducing

```bash
export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

python3 -m agent.reason start \
  --url https://github.com/h4ckf0r0day/obscura \
  --question "What is the latest release tag and how many assets does it publish?" \
  --max-steps 8 --max-pages 6 --force

python3 -m agent.reason state
python3 -m agent.reason act --json '{"action":"navigate","url":"https://github.com/h4ckf0r0day/obscura","reason":"start"}'
# … repeat: state -> decide -> act …
python3 -m agent.reason result --trace evals/native_trace.json

# fallback path, no model:
python3 -m agent.reason fallback
```
