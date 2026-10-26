#!/usr/bin/env python3
"""vmc - Arena agent control surface for the virtual X11 desktop.

All operations use XTEST extension injection against the live Xvfb display,
so input reaches real applications exactly as a physical keyboard/mouse would.

Subcommands:
  info                              screen geometry + pointer position
  shot [file.png]                   capture the screen (prints path)
  windows                           list windows (id, title, geometry)
  focus <window_id>                 raise+focus a window
  close <window_id>                 send WM_DELETE_WINDOW
  move <x> <y>                      warp pointer
  mousedown [button] / mouseup [button]
  click [button] [x] [y]            button 1=left 2=middle 3=right (default 1)
  doubleclick [x] [y]
  scroll <clicks>                   positive=down, negative=up (wheel)
  key <keysym> [keysym...]          e.g. key Return | key ctrl shift t
  type <string>                     type arbitrary text
  launch <program> [args...]        start a GUI program on the display
  wait-title <substring> [seconds]  poll until a window title contains text
  exec <shell command>              run a shell command with desktop env set
  keysyms                           list available keysym names (first 80)
"""
import os
import sys
import time
import subprocess

from Xlib import X, XK, display
from Xlib.ext import xtest
from Xlib.protocol.event import ClientMessage

VM_PREFIX = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "local"))
DISPLAY = os.environ.setdefault("DISPLAY", ":99")
os.environ.setdefault("LD_LIBRARY_PATH", VM_PREFIX + "/lib")
os.environ.setdefault("FONTCONFIG_PATH", VM_PREFIX + "/etc/fonts")
os.environ.setdefault("TERM", "xterm-256color")

# US-layout unshifted punctuation -> keysym name
NAME_MAP = {
    '-': 'minus', '=': 'equal', '[': 'bracketleft', ']': 'bracketright',
    '\\': 'backslash', ';': 'semicolon', "'": 'apostrophe', '`': 'grave',
    ',': 'comma', '.': 'period', '/': 'slash', ' ': 'space',
}
# US-layout shifted symbols -> unshifted keysym name
SHIFT_MAP = {
    '~': 'grave', '!': '1', '@': '2', '#': '3', '$': '4', '%': '5', '^': '6',
    '&': '7', '*': '8', '(': '9', ')': '0', '_': 'minus', '+': 'equal',
    '{': 'bracketleft', '}': 'bracketright', '|': 'bar', ':': 'semicolon',
    '"': 'quotedbl', '<': 'comma', '>': 'period', '?': 'slash',
    'A': 'a', 'B': 'b', 'C': 'c', 'D': 'd', 'E': 'e', 'F': 'f', 'G': 'g',
    'H': 'h', 'I': 'i', 'J': 'j', 'K': 'k', 'L': 'l', 'M': 'm', 'N': 'n',
    'O': 'o', 'P': 'p', 'Q': 'q', 'R': 'r', 'S': 's', 'T': 't', 'U': 'u',
    'V': 'v', 'W': 'w', 'X': 'x', 'Y': 'y', 'Z': 'z',
}


class Desktop:
    def __init__(self):
        self.d = display.Display(DISPLAY)
        if not self.d.has_extension("XTEST"):
            sys.exit("XTEST extension unavailable on " + DISPLAY)
        self.root = self.d.screen().root
        # spare keycode used as a scratch mapping for keysyms without one
        self._scratch_keycode = 255
        self._scratch_restore = None

    # ---------- basics ----------
    def screen_size(self):
        g = self.root.get_geometry()
        return g.width, g.height

    def pointer(self):
        p = self.root.query_pointer()
        return p.root_x, p.root_y

    def sync(self):
        self.d.sync()
        time.sleep(0.05)

    # ---------- screenshots ----------
    def shot(self, path):
        import mss
        from PIL import Image
        with mss.MSS(display=DISPLAY) as sct:
            mon = sct.monitors[0]
            raw = sct.grab(mon)
            img = Image.frombytes("RGB", raw.size, raw.bgra, "raw", "BGRX")
        os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
        img.save(path)
        return os.path.abspath(path)

    # ---------- mouse ----------
    def move(self, x, y):
        xtest.fake_input(self.d, X.MotionNotify, x=x, y=y)
        self.sync()

    def button(self, button, down=True):
        ev = X.ButtonPress if down else X.ButtonRelease
        xtest.fake_input(self.d, ev, button)
        self.sync()

    def click(self, button=1, x=None, y=None):
        if x is not None and y is not None:
            self.move(x, y)
        self.button(button, True)
        self.button(button, False)

    def doubleclick(self, x=None, y=None):
        self.click(1, x, y)
        self.click(1)

    def scroll(self, clicks):
        for _ in range(abs(int(clicks))):
            self.button(5 if clicks > 0 else 4)  # 5=down 4=up

    # ---------- keyboard ----------
    def _keycode(self, keysym_name):
        ks = XK.string_to_keysym(keysym_name)
        if ks == 0:
            raise ValueError(f"unknown keysym {keysym_name!r}")
        kc = self.d.keysym_to_keycode(ks)
        if kc:
            return kc, False
        # no physical key: temporarily bind the keysym to the scratch keycode
        mapping = self.d.get_keyboard_mapping(self._scratch_keycode, 1)
        if self._scratch_restore is None:
            self._scratch_restore = mapping
        mapping[0] = (ks, 0, 0, 0, 0, 0, 0, 0, 0)
        self.d.change_keyboard_mapping(self._scratch_keycode, [mapping[0]])
        self.sync()
        return self._scratch_keycode, True

    KEY_ALIASES = {
        "ctrl": "Control_L", "control": "Control_L", "ctl": "Control_L",
        "shift": "Shift_L", "alt": "Alt_L", "mod1": "Alt_L",
        "super": "Super_L", "win": "Super_L", "mod4": "Super_L",
        "esc": "Escape", "return": "Return", "enter": "Return",
        "ret": "Return", "del": "Delete", "ins": "Insert",
        "pgup": "Prior", "pgdn": "Next", "pageup": "Prior", "pagedown": "Next",
        "bs": "BackSpace", "backspace": "BackSpace", "spacebar": "space",
        "left": "Left", "right": "Right", "up": "Up", "down": "Down",
        "tab": "Tab", "caps": "Caps_Lock", "print": "Print",
    }

    def key(self, *names):
        """Press a chord or single key, e.g. key('ctrl','shift','t')."""
        names = [self.KEY_ALIASES.get(n.lower(), n) for n in names]
        codes = [self._keycode(n) for n in names]
        for kc, _ in codes:
            xtest.fake_input(self.d, X.KeyPress, kc)
        self.sync()
        for kc, _ in reversed(codes):
            xtest.fake_input(self.d, X.KeyRelease, kc)
        self.sync()

    def type(self, text):
        for ch in text:
            if ch == "\n":
                self.key("Return")
                continue
            if ch == "\t":
                self.key("Tab")
                continue
            if ch == " ":
                self.key("space")
                continue
            shifted = ch in SHIFT_MAP
            name = SHIFT_MAP.get(ch, NAME_MAP.get(ch, ch))
            kc, scratch = self._keycode(name)
            if shifted:
                xtest.fake_input(self.d, X.KeyPress, self.d.keysym_to_keycode(XK.string_to_keysym("Shift_L")))
            xtest.fake_input(self.d, X.KeyPress, kc)
            self.sync()
            xtest.fake_input(self.d, X.KeyRelease, kc)
            if shifted:
                xtest.fake_input(self.d, X.KeyRelease, self.d.keysym_to_keycode(XK.string_to_keysym("Shift_L")))
            self.sync()

    # ---------- windows ----------
    def windows(self):
        out = []

        def walk(win, depth=0):
            try:
                attrs = win.get_attributes()
                if attrs.map_state != X.IsViewable:
                    return
                name = win.get_wm_name()
                if not name:
                    name = ""
                if isinstance(name, bytes):
                    name = name.decode("utf-8", "replace")
                g = win.get_geometry()
                out.append((win.id, name, g.x, g.y, g.width, g.height))
                for child in win.query_tree().children:
                    walk(child, depth + 1)
            except Exception:
                pass

        for child in self.root.query_tree().children:
            walk(child)
        return out

    def focus(self, wid):
        win = self.d.create_resource_object("window", int(wid, 0))
        win.set_input_focus(X.RevertToPointerRoot, X.CurrentTime)
        self.d.sync()

    def close(self, wid):
        win = self.d.create_resource_object("window", int(wid, 0))
        wm_protocols = self.d.intern_atom("WM_PROTOCOLS")
        wm_delete = self.d.intern_atom("WM_DELETE_WINDOW")
        ev = ClientMessage(display=self.d, window=win, client_type=wm_protocols,
                           data=(32, [wm_delete, X.CurrentTime, 0, 0, 0]))
        win.send_event(ev)
        self.d.sync()

    # ---------- launching ----------
    def launch(self, argv):
        env = dict(os.environ)
        env["DISPLAY"] = DISPLAY
        p = subprocess.Popen(argv, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL,
                             start_new_session=True, env=env)
        return p.pid

    def wait_title(self, substring, timeout=10):
        deadline = time.time() + timeout
        while time.time() < deadline:
            for wid, name, *_ in self.windows():
                if substring.lower() in name.lower():
                    return wid
            time.sleep(0.3)
        return None


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)
    cmd, args = sys.argv[1], sys.argv[2:]
    desk = Desktop()

    if cmd == "info":
        w, h = desk.screen_size()
        x, y = desk.pointer()
        print(f"display={DISPLAY} screen={w}x{h} pointer=({x},{y})")
    elif cmd == "shot":
        path = args[0] if args else os.path.join(
            os.path.dirname(__file__), "..", "evidence", "shot.png")
        print(desk.shot(path))
    elif cmd == "windows":
        rows = desk.windows()
        if not rows:
            print("(no viewable windows)")
        for wid, name, x, y, w, h in rows:
            print(f"0x{wid:08x}  {w}x{h}+{x}+{y}  {name}")
    elif cmd == "focus":
        desk.focus(args[0])
    elif cmd == "close":
        desk.close(args[0])
    elif cmd == "move":
        desk.move(int(args[0]), int(args[1]))
    elif cmd == "mousedown":
        desk.button(int(args[0] if args else 1), True)
    elif cmd == "mouseup":
        desk.button(int(args[0] if args else 1), False)
    elif cmd == "click":
        b = int(args[0]) if len(args) >= 1 and args[0].isdigit() else 1
        if len(args) >= 3:
            desk.click(b, int(args[-2]), int(args[-1]))
        else:
            desk.click(b)
    elif cmd == "doubleclick":
        if len(args) >= 2:
            desk.doubleclick(int(args[0]), int(args[1]))
        else:
            desk.doubleclick()
    elif cmd == "scroll":
        desk.scroll(int(args[0]))
    elif cmd == "key":
        desk.key(*args)
    elif cmd == "type":
        desk.type(args[0])
    elif cmd == "launch":
        print("pid", desk.launch(args))
    elif cmd == "wait-title":
        to = float(args[1]) if len(args) > 1 else 10
        wid = desk.wait_title(args[0], to)
        print(f"0x{wid:08x}" if wid else "TIMEOUT")
        sys.exit(0 if wid else 1)
    elif cmd == "exec":
        os.execvp("bash", ["bash", "-c", args[0]])
    elif cmd == "keysyms":
        names = [n for n in dir(XK) if n.startswith("XK_")][:80]
        print("\n".join(n[3:] for n in names))
    else:
        print(f"unknown command: {cmd}", file=sys.stderr)
        print(__doc__)
        sys.exit(2)


if __name__ == "__main__":
    main()
