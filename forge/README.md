# Forge

A reusable agent-engineering workbench: discover what this environment can actually do,
record it honestly, route a task to the right capability, plan bounded steps, execute them
with real evidence, recover from failures, and leave a resumable handoff.

Built to the specification in [`../NLLM1/MASTER.md`](../NLLM1/MASTER.md).
Python 3.11 **standard library only** — no third-party dependency, no service, no database.

## Why it exists

Most "agent frameworks" confuse three different things: a capability existing in a catalog,
being installed here, and working right now. Forge keeps them in separate fields backed by
separate evidence, and will refuse to use something that is not proven operational.

## Quickstart

```bash
cd forge

python3 -m forge doctor                       # self-check
python3 -m forge --workspace /path/to/repo baseline     # workspace recon
python3 -m forge --workspace /path/to/repo discover     # audit capabilities (cached 1h)
python3 -m forge registry --category connector          # query what was found
python3 -m forge select --need "run python unit tests"  # route a need

python3 -m forge intake --workspace /path/to/repo \
    --goal "fix the failing pricing test" --criteria "unittest exits 0"
python3 -m forge plan   --task <id> --need "run python unit tests"
python3 -m forge run    --task <id>          # executes, records evidence, stops on failure
python3 -m forge recover                     # classify the failure + proposed bounded fix
python3 -m forge resume                      # continue from the checkpoint
python3 -m forge handoff                     # write HANDOFF.md
python3 -m unittest discover -v              # 53 tests
```

## Architecture

| Module | Responsibility |
|---|---|
| `forge/cli.py` | the interface (`baseline, discover, registry, select, intake, plan, run, resume, verify, recover, handoff, status, doctor`) |
| `forge/states.py` | `Capability` model + the lifecycle states |
| `forge/discovery.py` | engine with per-source isolation, caching, TTL; sources: host tools, network, skills, MCP, connectors, REA |
| `forge/registry.py` | persistence + merge rules that refuse to invent or downgrade proven state |
| `forge/selector.py` | token-based routing with a transparent rubric and explicit "none available" |
| `forge/planner.py` | intake → bounded plan with verification per step |
| `forge/executor.py` | bounded subprocess execution, redacted evidence, checkpoints, resume |
| `forge/recover.py` | failure classification and bounded repair proposals |
| `forge/policy.py` | approval gates, path containment, untrusted-text quarantine |
| `forge/redact.py` | secret removal by value shape **and** by sensitive key |
| `forge/inspect.py` | workspace recon (empty, fixture, or real repo) |
| `forge/store.py` | JSON state, atomic writes, structured JSONL logs |
| `forge/handoff.py` | durable handoff with evidence and exactly one next action |

Data lives in `forge/.forge/` (git-ignored, disposable). The **registry** (`forge/registry.json`)
and `HANDOFF.md` are committed because they are evidence.

## Capability states

`not_checked → found → available → installed → configured → authorized → operational → tested`,
plus `partially`, `blocked`, `rejected`.

Rules enforced in code, not by prompt:

- discovery may set `installed`/`operational` only from a real probe;
- discovery never sets `authorized` or `tested`;
- `mark --authorized/--tested` requires an evidence string or it raises;
- a proven state is never silently downgraded by a weaker source;
- routing refuses any capability that is not operational (and not authorized for risky needs).

## Adding a discovery source

Implement two attributes and one method, then register it in `default_sources()`:

```python
class MySource:
    id = "my-source"
    category = "custom"

    def discover(self, ctx):
        # do real work; never claim state you did not observe
        return [Capability(id="custom:thing", name="thing", category="custom")], \
               SourceReport(source_id=self.id, method="how it was checked")
```

A source that raises is contained: the engine records the failure and continues.

## Host-mediated capabilities

The agent host (not Forge) can query connectors and skills. Those observations are written to
`.forge/host_connectors.json` with `observed_at` and evidence, and Forge treats them as **data
with a TTL** (24 h): stale observations degrade to `not_checked` instead of being presented as
current truth. A missing manifest is reported as a gap, never invented.

## Limitations (verified, not guessed)

- No host skill loader exists here, so a `SKILL.md` is readable instructions, not an auto-invoked
  capability; the registry marks such skills `operational=blocked`.
- No MCP configuration or MCP client CLI exists; MCP is audited and reported as a gap.
- The connector catalog is only reachable through the host agent; Forge cannot call it directly.
- REA is unresolved (no CLI, package, or config; public searches returned unrelated projects).
  It stays an optional adapter slot — see `registry.json` `rea:unresolved`.
- Execution is local-only; there is no deployment/CI integration.
- A same-size source edit within the same second can reuse stale Python bytecode; fixtures must
  change file size or clear `__pycache__` (observed during development, documented rather than hidden).
