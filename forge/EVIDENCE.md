# Forge test evidence

All results below were produced by real execution in this sandbox
(Debian 12, Python 3.11.2, 2 vCPU, 3.9 GB RAM). Nothing here is mocked except where explicitly
labelled. Registry facts were collected by `discover` against the live machine.

## 1. Automated suite

Command: `cd forge && python3 -m unittest discover -v`
Result: **53 tests, OK, exit 0**. Re-runnable and deterministic.

| Matrix requirement (§10.2) | Test | Result |
|---|---|---|
| 1. task parsed and stored | `tests.test_core.TestTaskStorage` | PASS |
| 2. workspace inspection valid + empty | `tests.test_e2e.TestWorkspaceInspection` (3 tests) | PASS |
| 3. discovery handles results and unavailable sources | `tests.test_discovery.TestEngineResilience` (3) | PASS |
| 4. found ≠ installed ≠ working | `tests.test_core.TestRegistryHonesty` (6) | PASS |
| 5. missing permission not used as authorized | `tests.test_core.TestPolicy`, `TestSelection.test_risky_need_requires_authorization` | PASS |
| 6. selector returns capability or clear none | `tests.test_selection` (7) | PASS |
| 7. failed command recorded honestly | `tests.test_execution.TestRunCommand` (5) | PASS |
| 8. recovery preserves original evidence | `tests.test_execution.test_failed_run_records_evidence_and_recovery` | PASS |
| 9. resume from checkpoint | `tests.test_execution.test_resume_skips_completed_steps` | PASS |
| 10. logs redact secrets | `tests.test_core.TestRedaction` (5) | PASS |
| 11. untrusted description cannot override policy | `tests.test_core.TestInjectionDefence` (4) | PASS |
| 12. pass/fail/skip/blocked distinguished | `tests.test_execution.TestReportingDistinctions` | PASS |

Also: `tests.test_e2e.TestEndToEndRecovery` (full cycle) and `TestCliSmoke` (real CLI invocation).

## 2. Controlled failure / recovery benchmark (§10.3)

Disposable fixture `/tmp/forge-bench-4FHzv4/shop` (a pricing module with a genuine bug:
`apply_discount` accepted out-of-range percentages). Never touched user data.

| Step | Command | Observed |
|---|---|---|
| baseline failure | `python3 -m unittest discover` | `FAILED (failures=1)` — `test_invalid_percent_rejected` |
| intake | `forge intake --goal ... --criteria ...` | task stored, 1 open question none |
| plan | `forge plan --task <id> --need "run python unit tests"` | 6 steps; routed `cli:python3` |
| run | `forge run --task <id>` | **failed**, exit 1, recovery `class=test_assumption` |
| recover | `forge recover` | diagnosis includes the real FAIL text; proposed bounded fix |
| repair | one file edited (`pricing.py` validation) | — |
| resume | `forge resume` | re-ran only unfinished steps: **PASS** x3, status `partially` |
| re-verify | `forge recover --rerun python3 -m unittest discover -v` | `Ran 4 tests ... OK` |
| handoff | `forge handoff` | `HANDOFF.md` contains FAIL then PASS evidence + next action |

The original failure evidence remains in the run record after the repair — recovery never erases it.

## 3. Environment audit evidence (real probes)

| Check | Command | Result |
|---|---|---|
| connector catalog | host connector API | `gmail` enabled (28 tools), `notion` disconnected, `github` unsupported |
| skill dirs | filesystem scan | none of `~/.claude/skills`, `~/.config/skills`, `~/.arena/skills` exist; one `SKILL.md` in `arena-vm/` |
| MCP | config scan + PATH | no `~/.claude.json`, no `~/.mcp.json`, no `mcp`/`claude`/`codex` CLI |
| REA | PATH, pip list, npm -g, filesystem, pypi, GitHub search | no product identified; pypi `rea` = X11 astronomy app (GPLv3, unrelated); GitHub search returned 5 unrelated repos |
| network | HTTPS GET with 8 s timeout | pypi 200, npm 200, github 200, api.github.com reachable, example.com blocked (`000`) |
| runtime | version probes | python 3.11.2, node 22.22.3, npm 10.9.8, yarn 1.22.22, gcc 12.2, git 2.39.5, jq 1.6, rg 13.0.0; no rust/go/java/dotnet/docker |
| self-check | `forge doctor` | 5/5 PASS incl. "no secret in registry" |

## 4. Defects found and fixed during development

Recorded because the loop is meant to catch them, not hide them.

1. Executor passed a *risk level* into a function expecting an *action kind* → every step blocked.
   Fixed with `policy.check_risk()`.
2. Resume skipped the failed step. Fixed: only `done` steps are skipped.
3. Handoff was written to the repo instead of the run's state dir. Fixed by passing the store.
4. `redact()` ignored JSON-style `"password": "..."` keys; `redact_obj` never inspected keys.
   Fixed with a key-based rule.
5. Selector used substring matching ("run" matched "run**time**", "to" matched "**to**ol") and
   routed wrongly. Replaced with token-based relevance + partial-match disclosure.
6. CLI `--workspace` only parsed before the subcommand; registry path was not overridable,
   so tests hit the real registry. Fixed with shared parent arguments + `--registry`.
7. Stale `.pyc` on a same-size, same-second source edit produced a phantom test failure —
   diagnosed, fixed in the fixture, documented as a known environment hazard.

## 5. Known gaps

- No browser/computer-use capability in this sandbox (no browser binary): UI verification is `NOT RUN`.
- No live MCP server was configured or tested (none available; not worth inventing).
- Connector operations were audited but **not exercised** beyond reading the catalog — deliberately,
  because exercising `gmail.send_message` would be an external write requiring approval.
- REA integration: `BLOCKED` (unidentified), not implemented.
