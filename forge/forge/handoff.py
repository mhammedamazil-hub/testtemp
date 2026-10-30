"""Durable session handoff: enough for a fresh session to resume without chat history."""

from __future__ import annotations

from pathlib import Path

from .store import RunRecord, Store, Task, now_iso

TEMPLATE = """# Forge handoff

Generated: {generated}
Workspace: `{workspace}`

## Current state

- Task: `{task_id}` — {goal}
- Status: **{status}**
- Run: `{run_id}` ({run_status}), checkpoint: {checkpoint}

## Completed

{completed}

## Evidence (real commands actually run)

{evidence}

## Blockers / limitations

{blockers}

## Next action

{next_action}
"""


def write_handoff(store: Store, task: Task | None = None,
                  run: RunRecord | None = None,
                  path: str | Path | None = None) -> Path:
    runs = store._data.get("runs", {})  # inspectable store, used read-only here
    if run is None and runs:
        run = store.latest_run()
    if task is None and run is not None:
        task = store.get_task(run.task_id)

    target = Path(path or (Path(store.home).parent / "HANDOFF.md"))

    if task is None:
        completed = "- (no task recorded yet)"
        evidence = "- (run `python3 -m forge baseline` and `discover` to populate)"
        blockers = "- nothing recorded"
        next_action = "run `python3 -m forge baseline`"
        task_id, goal, status = "-", "(none)", "no task"
    else:
        done = [f"- `{s.id}` {s.action}: {s.notes} ({s.status})"
                for s in task.plan if s.status in ("done", "skipped", "pending_manual", "failed")]
        completed = "\n".join(done) or "- nothing completed yet"
        lines: list[str] = []
        source = (run.evidence if run else []) or [e for s in task.plan for e in s.evidence]
        for ev in source:
            excerpt = (ev.output_summary or "").replace("\n", " ")[:160]
            lines.append(f"- `{ev.test_id}` **{ev.result}** exit={ev.exit_code} "
                         f"({ev.real_or_mocked}) `{ev.command[:90]}` — {excerpt}")
        evidence = "\n".join(lines) or "- no evidence captured yet"
        blockers = "\n".join(f"- {s.notes}" for s in task.plan if s.status == "blocked")
        blockers = blockers or "- none recorded"
        next_action = _next_action(task, run)
        task_id, goal, status = task.id, task.goal, task.status

    text = TEMPLATE.format(
        generated=now_iso(),
        workspace=str(task.workspace if task else ""),
        task_id=task_id, goal=goal, status=status,
        run_id=getattr(run, "id", "-"), run_status=getattr(run, "status", "-"),
        checkpoint=getattr(run, "checkpoint", {}) or "-",
        completed=completed, evidence=evidence, blockers=blockers,
        next_action=next_action,
    )
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding="utf-8")
    return target


def _next_action(task: Task, run: RunRecord | None) -> str:
    if run and run.status == "failed" and run.recovery:
        last = run.recovery[-1]
        return (f"apply the proposed bounded fix for **{last.failure_class}**: {last.attempted_fix}; "
                f"then re-run `{last.evidence[-1].command if last.evidence else 'the failing command'}`")
    failed = [s for s in task.plan if s.status == "failed"]
    if not failed and run and run.status in ("succeeded", "partially"):
        return ("review the passing evidence, commit the verified change, and record any "
                "remaining limitation (no failing steps remain)")
    for step in task.plan:
        if step.status == "pending_manual":
            return f"complete manual step `{step.id}` ({step.notes}), then resume the run"
        if step.status in ("pending", "running"):
            return f"resume the run at step `{step.id}` ({step.action})"
    if task.status == "succeeded":
        return "review evidence, commit the verified change, and record any new limitation"
    return "start a run: `python3 -m forge run --task <id>`"
