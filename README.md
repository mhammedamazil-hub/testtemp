# testtemp

Scratch repository. Contents:

* Image assets at the root (pre-existing, untouched).
* `agent_tools/` — browser tools for the agent, backed by
  [Obscura](https://github.com/h4ckf0r0day/obscura).
* `scripts/install_obscura.sh` — installs the official Obscura release for this
  machine's OS and architecture.
* `tests/` — unit tests for the browser tools.
* `examples/` — a deterministic browser agent and a JS-only SPA fixture.
* `agent/` — research agent v2: iterative observe/decide/act loop, budgets,
  source ledger, pluggable (unconnected) LLM policy.
* `evals/` — 7 graded research tasks and the harness that runs them.
* `agent/native.py`, `agent/reason.py` — decision gateway that lets the native
  Arena agent drive the loop turn by turn (see `docs/NATIVE_LLM.md`).

## Quick start

```bash
./scripts/install_obscura.sh      # downloads the official binary into vendor/obscura/
python -m agent_tools doctor      # environment + version + live smoke test
python -m agent_tools list        # tool schemas

python -m agent_tools call browser_evaluate \
  '{"url": "https://example.com", "expression": "document.title"}'
```

```python
from agent_tools import BrowserTools

tools = BrowserTools()
tools.call("browser_fetch", {"url": "https://example.com", "dump": "text"})
```

Tests:

```bash
python3 -m unittest discover -s tests -t . -v   # 21 mocked tests
python3 -m unittest tests.test_live -v          # live; skipped without a binary
./scripts/smoke_test.sh                         # raw evidence, exit status per command

# End-to-end agent run (see docs/AGENT_RUN.md for recorded output)
python3 examples/browser_agent_demo.py browse
python3 examples/browser_agent_demo.py js-heavy

# Research agent v2 + evaluation (see docs/AGENT_V2.md)
python3 -m evals.run_evals

# Native-model decision gateway (see docs/NATIVE_LLM.md)
python3 -m agent.reason start --url https://pypi.org/project/requests/ \
  --question "What is the current version of requests?" --force
python3 -m agent.reason state
python3 -m agent.reason act --json '{"action":"navigate","url":"https://pypi.org/project/requests/","reason":"start"}'
```

# Real Blender WebAssembly driven by an LLM through a versioned op API
# (see docs/BLENDER_AGENT_LOOP.md)
cd blender-web-agent
python3 -m controller.cli preflight          # is the runtime usable?
python3 run_baseline.py                      # the five established E2E tasks
python3 run_delivery.py                      # five asset-delivery scenarios
python3 -m controller.cli start --task "Build a low-poly spacecraft ..."
python3 -m controller.cli state --session <id>          # what the model sees
python3 -m controller.cli plan  --session <id> --file plan.json
```

See [docs/BLENDER_AGENT_LOOP.md](docs/BLENDER_AGENT_LOOP.md) for the autonomous
workflow report: the resumable model loop, the strengthened `bw.agent.ops/v2`
API, five delivery scenarios, failure recovery, and the measured reasons the
interactive viewport and public deployment remain blocked.
See [docs/BLENDER_WEB.md](docs/BLENDER_WEB.md) for the earlier milestone (real
Blender wasm boots and exports; Obscura cannot host it; GitHub Pages cannot
serve it).

See [docs/OBSCURA.md](docs/OBSCURA.md) for the install details, the design
notes, and the current status of the binary download in this sandbox.
