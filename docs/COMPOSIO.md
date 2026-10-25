# Composio integration

The agent can search and run actions across connected apps (Gmail, GitHub,
Calendar, and others) through [Composio](https://composio.dev). It uses the
official Python SDK (`composio`). It does not use MCP.

> **Status (2026-10-25): code complete and tested offline. Not verified against
> the live Composio API from this sandbox.** The sandbox's egress allowlist
> blocks `backend.composio.dev`, and no Composio project key is configured.
> See [Live verification](#live-verification) for the exact evidence.

---

## 1. What was there before, and where Composio fits

The repository's agent is the research loop in `agent/`, driven by the native
Arena model through `agent/reason.py`. Its tools come from `agent_tools/`:

| Piece | Where | Contract |
|---|---|---|
| Model (decides) | Arena native model, via `python -m agent.reason state` / `act` | Strict JSON decisions |
| Tool surface | `agent_tools/browser.py` (`BrowserTools`) | `list_tools()` → `{name, description, input_schema}`; `call(name, args)` → `{"ok", "tool", ...}` |
| Execution | Obscura CLI through `ObscuraClient` | Failures returned as envelopes, never raised |

Composio plugs into the **tool surface**, not the model or the loop. The model
is unchanged, and the research loop is unchanged. The new `ComposioTools` class
has the same `list_tools()` / `call()` contract as `BrowserTools`, and
`AgentToolRegistry` merges the two. Any tool-calling caller that already uses
`BrowserTools` can use the registry with no other changes.

```
native model ──decision──▶ tool call (name, args)
                              │
                     AgentToolRegistry.call
                      ├─ browser_*   ─▶ BrowserTools (Obscura)      [unchanged]
                      └─ composio_*  ─▶ ComposioTools
                                          ├─ composio_policy: allowlist, risk, approval, audit
                                          └─ Composio SDK ─▶ tool router session ─▶ app action
```

## 2. Files

| File | Role |
|---|---|
| `agent_tools/composio_tools.py` | `ComposioTools`: session discovery, execution, OAuth connect, status; schema and result translation; error mapping |
| `agent_tools/composio_policy.py` | Risk classification, toolkit allowlist, human approval gate, JSONL audit log |
| `agent_tools/registry.py` | `AgentToolRegistry`: one `list_tools()` / `call()` over browser and Composio tools |
| `agent_tools/composio_cli.py` | `python -m agent_tools.composio_cli`: `tools`, `doctor`, `search`, `status`, `connect`, `execute`, `live-check` |
| `requirements-composio.txt` | `composio==0.30.0` (pinned; the version tested) |
| `tests/test_composio.py` | 32 offline tests against the real SDK with a mocked HTTP transport |
| `tests/test_composio_live.py` | One read-only action against a real connected account; skipped without credentials |

The SDK is imported lazily, so the existing browser tools and tests run
without it installed.

## 3. Session-based discovery

Composio's tool router *session* is the unit of access. One session is created
per user (`COMPOSIO_USER_ID`), and the agent works through it:

1. `session.search(query=...)`: discovery. Returns candidate action slugs with
   input schemas, plus whether each app is connected for this user.
2. `session.execute(slug, arguments=...)`: runs one action.
3. `session.authorize(toolkit)`: starts OAuth for an app.
4. `session.toolkits()`: connection state per app.

Nothing is hard-coded per app. The agent learns the actions from the search
results. Schemas are translated into the agent's format by `to_agent_tool()`.
When Composio returns only a schema reference, the tool is marked
`schema_complete: false` and the arguments are validated by Composio when the
action runs.

## 4. The agent-facing tools

| Tool | Input | Policy |
|---|---|---|
| `composio_search_tools` | `query` | none (read) |
| `composio_execute_tool` | `tool_slug`, `arguments`, optional `account` | by risk (see §5) |
| `composio_connect_app` | `toolkit`, optional `callback_url` | always needs approval (grants account access) |
| `composio_connection_status` | optional `toolkits` | none (read) |

Results use the same envelope as the browser tools:

```json
{"ok": true,  "tool": "composio_execute_tool", "action": "GMAIL_GET_PROFILE",
 "toolkit": "gmail", "risk": "read", "approval": "not_required",
 "result": {...}, "log_id": "..."}

{"ok": false, "tool": "composio_execute_tool", "action": "GMAIL_SEND_EMAIL",
 "error": {"type": "approval_required", "message": "..."}}
```

## 5. Permissions and approval

Enforced in the host process, not by the model:

1. **Toolkit allowlist.** `COMPOSIO_TOOLKITS=gmail,github` restricts use to
   those toolkits. The same list is sent to Composio when the session is
   created, so the server enforces it too.
2. **Meta tools are blocked.** `COMPOSIO_*` slugs are refused.
   `COMPOSIO_MULTI_EXECUTE_TOOL` runs other tools, and `COMPOSIO_PROXY_EXECUTE`
   makes raw authenticated HTTP calls. Either would bypass this policy.
3. **Risk classification from the verb.** Each word of the slug is checked:

   | Category | Trigger words (examples) | Needs approval |
   |---|---|---|
   | `read` | GET, LIST, FETCH, SEARCH, FIND, READ, VIEW, PROFILE, RETRIEVE, CHECK | no |
   | `write` | CREATE, UPDATE, ADD, MODIFY, PATCH, MOVE, ARCHIVE, LABEL, UPLOAD | yes |
   | `outbound` | SEND, PUBLISH, POST, SUBMIT, REPLY, FORWARD, INVITE, SHARE | yes |
   | `destructive` | DELETE, REMOVE, TRASH, PURCHASE, BUY, PAY, CHARGE, TRANSFER, CANCEL | yes |
   | `connection` | connecting a new account | yes |

   Destructive and outbound words are checked first. Any slug with no
   recognised read verb is a `write`. The default is to fail closed.
4. **Human approval on a TTY only.** The approval prompt is printed to stderr
   and requires the user to type `APPROVE` exactly. It is available only when
   stdin is a terminal. The agent's shell has no TTY and closed stdin, so the
   model cannot answer the prompt. Without a terminal, the action is refused
   with `approval_required` and nothing is sent. The prompt shows the action,
   toolkit, category, argument keys, a preview of the arguments (so the
   approver sees the recipient and subject), and a digest.

Reads run without a prompt. The gate is per call: an approval covers only the
action it was shown.

## 6. Authentication and secrets

- `COMPOSIO_API_KEY` is read from the environment and passed to the SDK in
  memory. It is never written to a file, logged, or printed. `doctor` reports
  only `set` or `unset`.
- OAuth tokens never reach this code. `composio_connect_app` returns a
  `redirect_url`. The user opens it in a browser and completes consent on
  Composio's side. The agent then checks status with
  `composio_connection_status`.
- `COMPOSIO_USER_ID` selects whose connected accounts are used. It is
  required and has no default, so a wrong account cannot be used silently.

## 7. Error handling

Every failure becomes an `ok: false` envelope with a stable `error.type`.
Failures are never reported as success:

| `error.type` | Cause |
|---|---|
| `auth_missing` | `COMPOSIO_API_KEY` not set |
| `auth_failed` | Composio returned 401 |
| `config_error` | `COMPOSIO_USER_ID` not set |
| `network_error` | could not reach `backend.composio.dev` (DNS, TLS, egress allowlist) |
| `timeout`, `rate_limited`, `api_error` | Composio API condition |
| `permission_denied` | toolkit not allowlisted, or Composio returned 403 |
| `meta_tool_blocked` | attempt to run a `COMPOSIO_*` meta tool |
| `approval_required`, `approval_denied` | the approval gate refused |
| `composio_failed` | Composio returned an `error` field, or `result_type` other than `completed` |
| `input_required` | Composio asked for input; this code does not auto-answer |
| `not_found`, `tool_not_found` | unknown slug or resource |

A success requires `result_type` to be `completed` (or absent) and `error` to
be empty.

## 8. Action log

Every call writes one JSON line to `logs/composio_actions.jsonl` (override with
`COMPOSIO_ACTION_LOG`). `logs/` is git-ignored because it can include account
identifiers.

```json
{"at":"2026-10-25T17:32:55Z","event":"call","tool":"composio_connection_status","ok":false,"error_type":"network_error"}
{"at":"...","event":"execute","action":"GMAIL_SEND_EMAIL","toolkit":"gmail","category":"outbound",
 "argument_keys":["body","to"],"arguments_sha256":"3f9c...","decision":"approval_unavailable","approval":"not_required"}
```

The log records argument **key names** and a **digest**, never values. It also
never records response data. Those can contain private mail, documents, or
payment details. The test `test_action_log_never_contains_argument_values_or_data`
checks this.

## 9. Usage

```bash
pip install -r requirements-composio.txt

export COMPOSIO_API_KEY=...          # project key from the Composio dashboard
export COMPOSIO_USER_ID=you@example.com
export COMPOSIO_TOOLKITS=gmail       # optional allowlist

python -m agent_tools.composio_cli tools
python -m agent_tools.composio_cli doctor
python -m agent_tools.composio_cli connect gmail        # prints redirect_url; needs a TTY
python -m agent_tools.composio_cli status
python -m agent_tools.composio_cli search "show my gmail profile"
python -m agent_tools.composio_cli execute GMAIL_GET_PROFILE --json '{}'
python -m agent_tools.composio_cli live-check --slug GMAIL_GET_PROFILE
```

In Python:

```python
from agent_tools import AgentToolRegistry
registry = AgentToolRegistry()           # Obscura browser tools + Composio tools
registry.call("composio_search_tools", {"query": "list my unread emails"})
```

Exit codes for the CLI: 0 success, 1 action or call failed, 2 config or usage error.

## 10. Tests

```bash
python3 -m unittest discover -s tests -t . -v          # 133 tests: 128 passed, 5 skipped
python3 -m unittest tests.test_composio -v             # 32 offline Composio tests, all pass
```

The 5 skips are: the Composio live test (no credentials here) and four
pre-existing Obscura live tests whose target sites are unreachable from this
sandbox.

The offline tests run the real SDK with its HTTP transport replaced by a mock
that imitates the tool router endpoints. They cover schema translation,
search, read and write execution, the approval paths (no TTY, declined,
approved), the allowlist, meta-tool blocking, every error type, the
`error`-field and `result_type` success rules, network failure, the audit log
contents, and the connect flow.

## 11. Live verification

The live test needs three things: a project key, the user id that owns the
connected account, and a read-only slug. Set them in your own shell and run:

```bash
COMPOSIO_API_KEY=... COMPOSIO_USER_ID=... COMPOSIO_LIVE_READ_SLUG=GMAIL_GET_PROFILE \
  python3 -m unittest tests.test_composio_live -v
```

It checks session creation and auth, that the account is connected, and that
one read-only action returns a Composio `log_id`. It refuses any non-read slug.

What this sandbox showed on 2026-10-25:

- `backend.composio.dev` is unreachable from the sandbox (`SSL_ERROR_SYSCALL` /
  TLS EOF from `curl` and from the SDK). The sandbox's egress allowlist does
  not include this host.
- No `COMPOSIO_API_KEY` is configured in the sandbox, and none was provided.
- With a placeholder key, `live-check` failed at the first step with
  `network_error`, exit 1. The failure is in `logs/composio_actions.jsonl`.

So **no live Composio call has succeeded from this sandbox.** Connected-account
testing has to run from an environment with access to `backend.composio.dev`
and a real project key. The result will appear as the `log_id` line printed by
the live test.

A separate connector check is not a Composio test: the Arena Gmail connector
was used to read the profile of `mhammedamazil@gmail.com` (read-only
`get_profile`), which confirms the account exists. Composio itself was not
involved in that call.

## 12. Limitations

- **No live success yet** (see §11).
- **Schemas may be incomplete.** If Composio returns only a reference, the
  tool's `input_schema` is permissive. Composio validates the arguments on
  execution.
- **Risk is heuristic.** It comes from slug words. A read-named action that
  changes state would run without a prompt. Mitigate with `COMPOSIO_TOOLKITS`,
  or with Composio's own `require_approval` setting, which this code does not
  use yet.
- **Approval needs a person at a terminal.** Sensitive actions cannot be run
  by an unattended agent. That is deliberate.
- **Not wired into the research loop.** `ResearchAgent` still uses only the
  browser tools. Composio is available through the tool surface and the
  registry, not as a new research action.
- **Multi-account** is supported only through the optional `account`
  argument. Account selection is not otherwise managed.
