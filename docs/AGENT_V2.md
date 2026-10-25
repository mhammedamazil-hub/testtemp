# Research agent v2

An iterative browser agent on top of the Obscura adapter. It observes a page,
decides one action, executes it, inspects the result, and repeats until the
objective is answered or a budget is exhausted.

> **Status:** implemented and evaluated. **7/7 evaluation tasks pass.**
> **No LLM is connected.** Action selection is a deterministic heuristic; the
> LLM interface exists and is exercised with stubs, but no model was called.

---

## 1. What is implemented

| Requirement | State | Where |
|---|---|---|
| Adapter and old tests preserved | done — unchanged, still passing | `agent_tools/` |
| Iterative loop: observe → decide → act → inspect | done | `agent/agent.py` |
| Pluggable LLM interface | done, **not connected** | `agent/policies.py` |
| Navigation, link selection, text extraction, JS eval, bounded retries | done | `agent/agent.py` |
| Source ledger with URL + evidence per claim | done, DOM re-verified | `agent/ledger.py` |
| Step / time / page limits | done | `agent/budget.py` |
| ≥5 research tasks incl. failures | done — 7 tasks | `evals/tasks.py` |
| Network block vs browser error vs bot detection | done, separate enum | `agent/classify.py` |
| No Cloudflare claim without evidence | enforced — see §7 | — |

### The loop

```
observe(url) ──> classify failure ──> policy.choose() ──> execute
     ^                                                      │
     └──────────── history + budget check <────────────────┘
```

Each step is one decision. The policy sees the current `Observation` and the
history; it returns a single `Action`. The agent executes it, and the budget is
checked before every step and every page.

Because the adapter is stateless, "following a link" means re-navigating to that
URL in a fresh process. There is no session and no shared cookie jar.

### Actions

| Action | Effect |
|---|---|
| `navigate` | open a URL and capture title, text, links |
| `follow` | open `links[i]` from the current page |
| `eval` | run a JavaScript expression on the current page, record `js_result` |
| `finish` | end the run, optionally with a claim and its evidence |

### Failure taxonomy

`FailureKind` keeps three things apart that all look like "no page":

```
network_blocked   egress filter / allowlist / TLS reset     -> says nothing about the site
dns_failure       host does not resolve
tls_trust         certificate not trusted (proxy CA)
bot_challenge     interstitial page, or 403/503 with cf headers
timeout           exceeded its own budget
browser_error     anything else
```

A page that loads but is an interstitial ("Just a moment…") is recorded as
`bot_challenge` and **never** contributes claims.

### Source ledger

Every claim stores: text, source URL, verbatim evidence, method, step, and a
SHA-256 of the evidence.

* A claim without evidence or without a URL raises `MissingEvidence` — the agent
  refuses it and the run ends `incomplete` rather than reporting an unsourced
  answer.
* After recording, the agent re-navigates and asks the live DOM whether the
  evidence string is present. Only then is `verified` set to true.

### Limits

`Budget(max_steps, max_pages, max_seconds, max_retries_per_action)`.
Defaults in the evaluation: 8 steps, 4 pages, 75 s, 1 retry. Retries apply only
to `timeout` and `browser_error`, with linear backoff; a blocked host is not
retried, because retrying an allowlist will not help.

## 2. The LLM interface

```python
from agent import LLMSelector

selector = LLMSelector(complete=my_completion_fn, model_label="whatever")
```

`complete` is any `Callable[[str], str]`. The prompt contains the objective, the
current page (title, 2.5 kB of text, numbered links), and the last three
observations; the reply must be one JSON action.

**Nothing is auto-configured.** No API key is read from the environment, no
client is constructed, and `LLMSelector()` with no callable reports
`available() == False` and raises `SelectorUnavailable` if asked to decide.

Verified by tests (with a stub, not a model):

* unconfigured selector is unavailable and raises;
* a stubbed reply parses into an `Action`;
* non-JSON replies and unknown action names are rejected;
* the loop runs end-to-end with a `ScriptedSelector`.

**No model was connected for the evaluation.** Every result below comes from
`HeuristicSelector`.

## 3. Evaluation

```bash
export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
python3 -m evals.run_evals
```

Ground truth is fetched at run time from public APIs that never touch Obscura
(GitHub REST via `gh`, PyPI JSON via HTTPS), with recorded fallbacks if an API
is unreachable. The harness records which values were live.

### Results — 7/7

| Task | Type | Status | Answer | Steps | Claim verified |
|---|---|---|---|---|---|
| T1 github release | multi-step | complete | `v0.2.5` | 2 pages | yes |
| T2 github licence | single page | complete | `Apache-2.0` | 1 | yes |
| T3 pypi version | single page | complete | `2.34.2` | 1 | yes |
| T4 pypi requires-python | single page | complete | `>=3.10` | 1 | yes |
| T5 absent fact (negative control) | must not fabricate | not_found | — | 1 | no claims |
| T6 blocked host | expected failure | failed / `network_blocked` | — | 1 | no claims |
| T7 redirect to blocked host | expected failure | failed / `network_blocked` | — | 1 | no claims |

T1 is genuinely multi-step: the repo landing page contains no version string, so
the agent followed the `Releases` link and answered from
`https://github.com/h4ckf0r0day/obscura/releases`, then re-queried the DOM and
found the evidence 6 times.

Each answer matched the independent API value:

```
release : v0.2.5   == api.github.com/.../releases/latest .tag_name
licence : Apache-2.0 == api.github.com/... .license.spdx_id
version : 2.34.2   == pypi.org/pypi/requests/json .info.version
python  : >=3.10   == pypi.org/pypi/requests/json .info.requires_python
```

Full machine-readable output: `evals/results.json`.

### Failures found and fixed during the evaluation

The first run scored **5/7**. Both failures were real bugs, not test noise:

1. **Link index misalignment.** `Link.index` was assigned by `enumerate` before
   non-HTTP anchors were filtered out, so a policy's index pointed at the wrong
   entry. The agent followed `…/obscura#docker` twice instead of `/releases`.
   Fixed by numbering only the links that are kept; regression test added.
2. **Confusing status for an absent fact.** The negative control returned
   `complete`, which implies an answer exists. Added `Status.NOT_FOUND`.

A third issue surfaced at the same time: in-page `#fragment` links were treated
as onward navigation and burned two hops re-reading the same document. The
policy now normalises fragments, skips the current document and skips already
visited documents. Regression test added.

After the fixes the run is 7/7. The earlier 5/7 is not hidden: it is the reason
those three bugs were found.

## 4. Tests actually executed

```bash
python3 -m unittest discover -s tests -t .
Ran 59 tests in 3.7s — OK
```

| Suite | Count | Needs network | Status |
|---|---|---|---|
| `tests/test_browser.py` (adapter) | 23 | no | pass |
| `tests/test_agent.py` (loop, budget, ledger, policies) | 28 | no | pass |
| `tests/test_live.py` (real Obscura, real sites) | 8 | yes | pass |

The 28 agent tests use a `FakeBrowser`, so they are deterministic. They cover:
the observe→decide→act loop, answers found without a needless hop, incomplete
runs, the absent-fact path, claim verification and its failure case, all six
failure categories, step/page/time/retry bounds, ledger evidence rules, and the
LLM plumbing (unconfigured, stubbed, malformed).

Live adapter tests were run with `OBSCURA_TEST_URL=https://github.com` and
`OBSCURA_TEST_URL_2=https://pypi.org`, because `example.com` is blocked here.

## 5. Limitations

* **Heuristic, not intelligent.** The policy matches a regex and scores links by
  keyword. It cannot paraphrase a question or judge relevance.
* **Stateless.** One navigation per step; no cookies, no forms, no login.
* **Single answer.** The agent returns the first pattern match. It does not
  weigh conflicting sources.
* **No cross-source corroboration** beyond the eval harness comparing against an
  API. If two pages disagreed, the agent would not notice.
* **Two reachable sites only.** `github.com` and `pypi.org` are on this
  sandbox's allowlist; `example.com` and `news.ycombinator.com` are not.
* **The LLM path is unproven against a real model.** Only stubs were used.

## 6. Reproducing

```bash
export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt   # sandbox TLS proxy

python3 -m evals.run_evals            # 7/7, writes evals/results.json
python3 -m evals.run_evals --only T1  # one task
python3 -m unittest discover -s tests -t .
```

`--json path.json` writes results elsewhere. The harness exits non-zero if any
task fails, so it can gate CI.

## 7. Cloudflare: not reached, not claimed

Re-probed on 2026-10-25:

| URL | Classification | HTTP response |
|---|---|---|
| `https://www.cloudflare.com/` | `blocked_before_http` | none |
| `https://www.reddit.com/` | `blocked_before_http` | none |

Both fail at the TLS handshake before a single header arrives. No challenge page
was ever served to this agent, so:

* normal access — **not observed**
* challenge presented — **not observed**
* access after a challenge — **not attempted**

The classifier can recognise `cf-ray` / `cf-mitigated` / interstitial titles, but
it has never seen a genuine Cloudflare challenge, so that branch is **unverified
code**. No bypass is claimed, with or without a stealth build.

## 8. Files

| Path | Purpose |
|---|---|
| `agent/types.py` | Action, Observation, Objective, FailureKind, Status |
| `agent/agent.py` | the loop, retries, claim verification |
| `agent/budget.py` | step / page / time / retry limits |
| `agent/classify.py` | failure taxonomy and challenge detection |
| `agent/ledger.py` | source ledger, evidence rules |
| `agent/policies.py` | Heuristic, Scripted, LLM selectors |
| `evals/tasks.py` | 7 graded tasks |
| `evals/ground_truth.py` | independent API ground truth |
| `evals/run_evals.py` | harness, writes `results.json` |
| `evals/results.json` | recorded run output |
| `tests/test_agent.py` | 28 offline tests |
| `docs/AGENT_RUN.md` | v1 end-to-end demo (still valid) |
