"""Forge CLI — the smallest interface that exposes every core workflow."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from . import policy
from .discovery import DiscoveryContext, DiscoveryEngine, default_sources
from .executor import execute_plan, resume_plan, run_command
from .handoff import write_handoff
from .inspect import inspect_workspace, summarise
from .planner import missing_information, plan_for
from .recover import classify, verify_repair
from .registry import Registry
from .selector import select
from .states import Capability
from .store import (Logger, RunRecord, Store, Task, new_id, now_iso)

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_WORKSPACE = ROOT
DEFAULT_HOME = ROOT / ".forge"


def _store(args) -> Store:
    return Store(args.home or DEFAULT_HOME)


def _registry(args) -> Registry:
    """The registry is a committed deliverable; the path is overridable for isolation."""
    return Registry(getattr(args, "registry", None) or (ROOT / "registry.json")).load()


# -- commands --------------------------------------------------------------

def cmd_baseline(args) -> int:
    store = _store(args)
    logger = Logger(store.home)
    ws = Path(args.workspace).resolve()
    info = inspect_workspace(ws)
    payload = {"generated": now_iso(), "workspace": str(ws), "inspection": info}
    (store.home / "baseline.json").write_text(json.dumps(payload, indent=2), encoding="utf-8")
    logger.event("baseline", workspace=str(ws), files=info.get("file_count"))
    print(f"workspace : {summarise(info)}")
    print(f"manifests : {', '.join(info.get('manifests') or []) or 'none'}")
    print(f"tests     : {len(info.get('test_files', []))} file(s); suggested: "
          f"{info.get('suggested_test_command') or 'none detected'}")
    if info.get("is_git"):
        print(f"git       : {info['branch']}@{info['head']} "
              f"({'dirty: ' + ', '.join(info['dirty'][:5]) if info['dirty'] else 'clean'})")
    print(f"written   : {store.home / 'baseline.json'}")
    return 0


def cmd_discover(args) -> int:
    store = _store(args)
    logger = Logger(store.home)
    ws = Path(args.workspace).resolve()
    ctx = DiscoveryContext(workspace=ws, home=store.home,
                           timeout=args.timeout, offline=args.offline)
    engine = DiscoveryEngine(ctx, sources=default_sources(),
                             cache_path=store.home / "discovery-cache.json",
                             cache_ttl=args.cache_ttl)
    caps, reports = engine.run(refresh=args.refresh)
    registry = _registry(args)
    notes = registry.merge(caps)
    registry.save()
    logger.event("discover", candidates=len(caps), added=sum(1 for n in notes.values() if n == "added"))

    print(f"{'SOURCE':<20} {'OK':<3} {'N':>3} {'MS':>6}  NOTE")
    for r in reports:
        note = r.error or r.limitation or ""
        print(f"{r.source_id:<20} {'yes' if r.ok else 'NO':<3} {r.candidates:>3} "
              f"{r.duration_ms:>6}  {note[:70]}")
    summary = registry.summary()
    print(f"\nregistry: {summary['total']} capabilities "
          f"({summary['operational']} operational, {summary['authorized']} authorized)")
    print(f"written : {registry.path}")
    return 0 if all(r.ok for r in reports) else 1


def cmd_registry(args) -> int:
    registry = _registry(args)
    caps = registry.query(category=args.category, state=args.state, decision=args.decision)
    if args.format == "json":
        print(json.dumps([c.to_dict() for c in caps], indent=2))
        return 0
    if args.format == "markdown":
        print(registry.to_markdown())
        return 0
    print(f"{'ID':<34} {'CATEGORY':<11} {'INSTALLED':<12} {'AUTH':<12} {'OPER':<12} {'DECISION'}")
    for c in caps:
        print(f"{c.id:<34} {c.category:<11} {c.installed_status:<12} "
              f"{c.authorization_status:<12} {c.operational_status:<12} {c.decision}")
    print(f"\n{len(caps)} item(s) of {len(registry.items)}")
    return 0


def cmd_select(args) -> int:
    registry = _registry(args)
    result = select(registry, args.need, risk=args.risk)
    if not result.ok:
        print(f"NO SELECTION — {result.reason}")
        for s in result.considered[:6]:
            if s.rejected_because:
                print(f"  rejected {s.cap_id}: {s.rejected_because}")
        return 2
    print(result.reason)
    if result.choice:
        print(f"  test procedure: {result.choice.test_command_or_procedure or 'n/a'}")
        print(f"  evidence      : {result.choice.evidence[:2] or 'none'}")
    return 0


def cmd_intake(args) -> int:
    store = _store(args)
    task = Task(id=new_id("task"), goal=args.goal,
                constraints=args.constraint or [],
                acceptance_criteria=args.criteria or [],
                workspace=str(Path(args.workspace).resolve()),
                risk_level="low")
    task.open_questions = missing_information(task)
    store.add_task(task)
    Logger(store.home).event("intake", task=task.id, goal=task.goal)
    print(f"task {task.id}: {task.goal}")
    print(f"workspace: {task.workspace}")
    if task.open_questions:
        print("open questions (safe work can still start):")
        for q in task.open_questions:
            print(f"  - {q}")
    print("next: python3 -m forge plan --task", task.id)
    return 0


def cmd_plan(args) -> int:
    store = _store(args)
    task = store.get_task(args.task)
    if not task:
        print(f"no such task: {args.task}", file=sys.stderr)
        return 2
    caps: list[Capability] = []
    if args.need:
        registry = _registry(args)
        result = select(registry, args.need)
        if result.choice:
            caps = [result.choice]
            print(f"capability: {result.choice.id} — {result.reason}")
        else:
            print(f"capability: none — {result.reason}")
    task.plan = plan_for(task, caps)
    task.status = "planning"
    store.update_task(task)
    for i, step in enumerate(task.plan, 1):
        print(f"{i}. [{step.action}] {step.notes}")
        if step.command:
            print(f"     command: {' '.join(step.command)}")
        print(f"     verify : {step.verification}")
    print(f"\n{len(task.plan)} step(s). next: python3 -m forge run --task {task.id}")
    return 0


def cmd_run(args) -> int:
    store = _store(args)
    task = store.get_task(args.task)
    if not task:
        print(f"no such task: {args.task}", file=sys.stderr)
        return 2
    if not task.plan:
        print("no plan yet; run `plan` first", file=sys.stderr)
        return 2
    task.status = "running"
    store.update_task(task)
    run = store.add_run(RunRecord(id=new_id("run"), task_id=task.id))
    run = execute_plan(store, task, run, approved=args.approve, timeout=args.timeout)
    _print_run(run)
    return 0 if run.status in ("succeeded", "partially") else 1


def cmd_resume(args) -> int:
    store = _store(args)
    run = store.get_run(args.run_id) if args.run_id else store.latest_run()
    if not run:
        print("no run to resume", file=sys.stderr)
        return 2
    task = store.get_task(run.task_id)
    if not task:
        print("run references a missing task", file=sys.stderr)
        return 2
    print(f"resuming run {run.id} at step {run.current_step}/{len(task.plan)}")
    run = resume_plan(store, task, run, approved=args.approve, timeout=args.timeout)
    _print_run(run)
    return 0 if run.status in ("succeeded", "partially") else 1


def _print_run(run: RunRecord) -> None:
    print(f"run {run.id}: {run.status}")
    for ev in run.evidence:
        excerpt = (ev.output_summary or "").replace("\n", " ")[:110]
        print(f"  [{ev.result:<7}] {ev.test_id} exit={ev.exit_code} {ev.command[:60]} — {excerpt}")
    for rec in run.recovery:
        print(f"  recovery: class={rec.failure_class} | {rec.attempted_fix}")


def cmd_verify(args) -> int:
    """Run an arbitrary verification command and record evidence honestly."""
    store = _store(args)
    allowed, reason = policy.check("exec_local")
    if not allowed:
        print(reason, file=sys.stderr)
        return 2
    result = run_command(args.command, cwd=args.cwd, timeout=args.timeout)
    Logger(store.home).event("verify", command=" ".join(args.command), result=result.result,
                             exit_code=result.exit_code)
    print(f"{result.result} exit={result.exit_code} {result.duration_ms}ms")
    if result.blocked_reason:
        print(f"blocked: {result.blocked_reason}")
    out = (result.stdout or result.stderr).strip()
    if out:
        print(out[-1500:])
    return 0 if result.result == "PASS" else 1


def cmd_recover(args) -> int:
    store = _store(args)
    runs = store._data.get("runs", {})
    if not runs:
        print("no runs recorded", file=sys.stderr)
        return 2
    run = store.get_run(args.run_id) if args.run_id else store.latest_run()
    if not run:
        print(f"no such run: {args.run_id}", file=sys.stderr)
        return 2
    if not run.recovery:
        print(f"run {run.id} has no recorded failure")
        return 0
    last = run.recovery[-1]
    print(f"run       : {run.id} ({run.status})")
    print(f"class     : {last.failure_class}")
    print(f"diagnosis : {last.diagnosis}")
    print(f"proposed  : {last.attempted_fix}")
    print(f"risk left : {last.remaining_risk}")
    if args.rerun:
        task = store.get_task(run.task_id)
        cwd = task.workspace if task else "."
        print(f"\nre-running: {' '.join(args.rerun)}")
        event = verify_repair(store, task, args.rerun, cwd=cwd)
        print(f"result    : {event.outcome}")
        for ev in event.evidence:
            print((ev.output_summary or "")[-800:])
        return 0 if event.outcome == "PASS" else 1
    print("\nre-run the failing command after the fix: python3 -m forge recover --rerun <cmd...>")
    return 0


def cmd_handoff(args) -> int:
    store = _store(args)
    path = write_handoff(store)
    print(f"handoff written: {path}")
    return 0


def cmd_status(args) -> int:
    store = _store(args)
    registry = _registry(args)
    print(f"forge home : {store.home}")
    print(f"registry   : {registry.path} ({len(registry.items)} capabilities)")
    tasks = store.list_tasks()
    print(f"tasks      : {len(tasks)}")
    for t in tasks[-5:]:
        print(f"  - {t['id']} [{t['status']}] {t['goal'][:70]}")
    run = store.latest_run()
    if run:
        print(f"latest run : {run.id} [{run.status}] step {run.current_step}")
    return 0


def cmd_doctor(args) -> int:
    """Self-check: proves the install is coherent without touching anything."""
    checks: list[tuple[str, bool, str]] = []
    checks.append(("python>=3.10", sys.version_info >= (3, 10), sys.version.split()[0]))
    ws = inspect_workspace(args.workspace)
    checks.append(("workspace readable", ws.get("exists", False), ws.get("path", "")))
    store = Store(args.home)
    checks.append(("state writable", (store.home / ".writetest").write_text("ok") is not None, str(store.home)))
    (store.home / ".writetest").unlink(missing_ok=True)
    reg = _registry(args)
    checks.append(("registry parseable", True, f"{len(reg.items)} items"))
    checks.append(("no secret in registry", not _contains_secret(reg.path), str(reg.path)))
    width = max(len(n) for n, _, _ in checks)
    for name, ok, detail in checks:
        print(f"{'PASS' if ok else 'FAIL'}  {name:<{width}}  {detail}")
    return 0 if all(ok for _, ok, _ in checks) else 1


def _contains_secret(path: Path) -> bool:
    if not path.exists():
        return False
    from .redact import redact
    text = path.read_text(encoding="utf-8", errors="replace")
    return redact(text) != text


# -- parser ----------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--home", default=argparse.SUPPRESS, help="state dir (default <forge>/.forge)")
    common.add_argument("--workspace", default=argparse.SUPPRESS,
                        help="workspace/target directory (default: the forge repo root)")
    common.add_argument("--registry", default=argparse.SUPPRESS,
                        help="registry file (default <forge>/registry.json)")

    p = argparse.ArgumentParser(prog="forge", description="Forge agent-engineering workbench")
    p.add_argument("--home", default=str(DEFAULT_HOME), help="state dir (default <forge>/.forge)")
    p.add_argument("--workspace", default=str(DEFAULT_WORKSPACE),
                   help="workspace/target directory (default: the forge repo root)")
    p.add_argument("--registry", default=str(ROOT / "registry.json"),
                   help="capability registry file")
    sub = p.add_subparsers(dest="command", required=True)

    sub.add_parser("baseline", parents=[common], help="inspect the workspace and write baseline.json").set_defaults(func=cmd_baseline)
    sub.add_parser("doctor", parents=[common], help="self-check the installation").set_defaults(func=cmd_doctor)

    d = sub.add_parser("discover", parents=[common], help="run capability discovery sources")
    d.add_argument("--refresh", action="store_true")
    d.add_argument("--offline", action="store_true")
    d.add_argument("--timeout", type=float, default=8.0)
    d.add_argument("--cache-ttl", dest="cache_ttl", type=int, default=3600)
    d.set_defaults(func=cmd_discover)

    r = sub.add_parser("registry", parents=[common], help="query the capability registry")
    r.add_argument("--category")
    r.add_argument("--state")
    r.add_argument("--decision")
    r.add_argument("--format", choices=["table", "json", "markdown"], default="table")
    r.set_defaults(func=cmd_registry)

    s = sub.add_parser("select", parents=[common], help="route a need to a capability")
    s.add_argument("--need", required=True)
    s.add_argument("--risk", default="safe", choices=["safe", "approval"])
    s.set_defaults(func=cmd_select)

    i = sub.add_parser("intake", parents=[common], help="record a task")
    i.add_argument("--goal", required=True)
    i.add_argument("--constraint", action="append")
    i.add_argument("--criteria", action="append")
    i.set_defaults(func=cmd_intake)

    pl = sub.add_parser("plan", parents=[common], help="build a plan for a task")
    pl.add_argument("--task", required=True)
    pl.add_argument("--need", help="capability need to route first")
    pl.set_defaults(func=cmd_plan)

    ru = sub.add_parser("run", parents=[common], help="execute a task plan")
    ru.add_argument("--task", required=True)
    ru.add_argument("--approve", action="store_true", help="approve risky steps")
    ru.add_argument("--timeout", type=int, default=180)
    ru.set_defaults(func=cmd_run)

    rs = sub.add_parser("resume", parents=[common], help="resume a run from its checkpoint")
    rs.add_argument("--run-id", dest="run_id")
    rs.add_argument("--approve", action="store_true")
    rs.add_argument("--timeout", type=int, default=180)
    rs.set_defaults(func=cmd_resume)

    v = sub.add_parser("verify", parents=[common], help="run a verification command")
    v.add_argument("--cwd", default=str(DEFAULT_WORKSPACE))
    v.add_argument("--timeout", type=int, default=180)
    v.add_argument("command", nargs=argparse.REMAINDER)
    v.set_defaults(func=cmd_verify)

    rec = sub.add_parser("recover", parents=[common], help="show/act on a recorded failure")
    rec.add_argument("--run-id", dest="run_id")
    rec.add_argument("--rerun", nargs=argparse.REMAINDER)
    rec.set_defaults(func=cmd_recover)

    sub.add_parser("handoff", parents=[common], help="write HANDOFF.md").set_defaults(func=cmd_handoff)
    sub.add_parser("status", parents=[common], help="show tasks/runs").set_defaults(func=cmd_status)
    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.command == "verify" and args.command == getattr(args, "command", None):
        pass
    if args.command == "verify":
        cmd = [c for c in (args.command or []) if c != "--"]
        if not cmd:
            print("verify needs a command", file=sys.stderr)
            return 2
        args.command = cmd
    if args.command == "recover" and args.rerun:
        args.rerun = [c for c in args.rerun if c != "--"]
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
