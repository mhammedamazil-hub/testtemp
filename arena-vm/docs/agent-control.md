# Agent control surface (`control/vmc.py`)

The Arena agent drives the desktop with ordinary shell calls. No extra LLM,
daemon, or network API is involved — every command speaks X11 directly to the
live Xvfb display via the XTEST extension and python-xlib.

## Prerequisites per shell

```bash
cd /home/user/testtemp/arena-vm
source scripts/env.sh          # PATH, PKG_CONFIG_PATH, flags
export DISPLAY=:99
PY=local/venv/bin/python
```

## Operations

| Goal | Command |
|---|---|
| screen size + pointer | `$PY control/vmc.py info` |
| screenshot | `$PY control/vmc.py shot evidence/x.png` |
| list windows | `$PY control/vmc.py windows` |
| move mouse | `$PY control/vmc.py move 400 300` |
| left click (optionally at x,y) | `$PY control/vmc.py click 1 [x y]` |
| middle/right click | `$PY control/vmc.py click 2` / `click 3` |
| double click | `$PY control/vmc.py doubleclick [x y]` |
| mouse buttons down/up | `$PY control/vmc.py mousedown 1` / `mouseup 1` |
| scroll down/up N | `$PY control/vmc.py scroll 3` / `scroll -3` |
| key / chord | `$PY control/vmc.py key Return` / `key ctrl shift t` |
| type literal text | `$PY control/vmc.py type "echo hello > /tmp/f" ` |
| launch a GUI app | `$PY control/vmc.py launch local/bin/dillo file:///tmp/page.html` |
| raise + focus window | `$PY control/vmc.py focus 0x00600004` |
| close window gracefully | `$PY control/vmc.py close 0x00600004` |
| wait for a window title | `$PY control/vmc.py wait-title Dillo 10` |
| run a shell command with desktop env | `$PY control/vmc.py exec 'xdpyinfo | head'` |

Aliases understood by `key`: ctrl/control, shift, alt, super/win, esc,
return/enter, tab, bs/backspace, del, ins, pgup/pgdn, arrows, caps, print.
Typing handles shifted punctuation and capitals on a US layout.

## The closed feedback loop (verified end-to-end)

1. agent: `vmc.py shot` → reads the returned PNG with its own vision.
2. agent: decides a target coordinate or text.
3. agent: `vmc.py click/type/key` → XTEST events enter the real X server.
4. the real application (st/dillo/vim) changes state.
5. agent: `vmc.py shot` again → verifies visually, or checks the filesystem
   for side effects (e.g. a file the GUI app wrote).
6. on failure: `vmc.py windows`, `run/*.log`, and retries drive diagnosis.

## Example: write a file through the GUI terminal

```bash
$PY control/vmc.py launch local/bin/st
sleep 2
WID=$($PY control/vmc.py windows | awk '/\+0\+19/{print $1; exit}')
$PY control/vmc.py focus "$WID"
$PY control/vmc.py type "echo AGENT_CONTROL_OK > /tmp/agent_test.txt"
$PY control/vmc.py key Return
cat /tmp/agent_test.txt      # -> AGENT_CONTROL_OK
```

## Manual handoff (tablet)

x11vnc runs in `-shared` mode: the agent's XTEST input and a human's VNC
input coexist on the same display. Safe-handoff pattern: the agent pauses
input injection while the human types; screenshots still work for both.
