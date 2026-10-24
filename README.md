# testtemp

Scratch repository. Contents:

* Image assets at the root (pre-existing, untouched).
* `agent_tools/` — browser tools for the agent, backed by
  [Obscura](https://github.com/h4ckf0r0day/obscura).
* `scripts/install_obscura.sh` — installs the official Obscura release for this
  machine's OS and architecture.
* `tests/` — unit tests for the browser tools.
* `examples/` — a deterministic browser agent and a JS-only SPA fixture.

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
```

See [docs/OBSCURA.md](docs/OBSCURA.md) for the install details, the design
notes, and the current status of the binary download in this sandbox.
