# Viewing the virtual desktop from a phone/tablet

## What runs

`scripts/start_tablet.sh` (started automatically by `recover.sh --tablet`)
creates a two-hop bridge:

```
phone browser --HTTPS--> Arena preview :6080 (websockify + noVNC, HTTP Basic Auth)
                              | loopback only
                         x11vnc :5900 -- attached to Xvfb display :99
```

- x11vnc binds to `127.0.0.1` only — the raw VNC port is never exposed.
- websockify/noVNC binds `0.0.0.0:6080` and requires HTTP Basic auth on
  every request, including the WebSocket upgrade (verified: 401 without
  credentials, 200/101 with them).
- The session is **shared**: phone touch input and agent XTEST input act on
  the same desktop simultaneously.

## Connecting

1. The public URL is the Arena preview for port 6080:
   `https://6080-<E2B_SANDBOX_ID>.e2b.app` (sandbox id is in the environment).
2. Username/password live in `run/tablet.cred` (`user:password`). They are
   generated once per sandbox, chmod 600, and never committed (run/ is
   git-ignored). Read them with `cat run/tablet.cred`.
3. Open the URL in any phone browser; enter the credentials at the prompt.
   The landing page redirects straight into `vnc.html?autoconnect=1&resize=scale`,
   so the desktop connects and scales to the screen automatically.
4. noVNC touch gestures: one-finger tap = left click, two-finger tap =
   right click, two-finger drag = scroll, pinch = zoom. A keyboard button
   in the noVNC toolbar sends keystrokes.

## Lifecycle and caveats

- A sandbox recycle wipes `run/` (new password) and all built daemons.
  Rebuild with `bash scripts/recover.sh --tablet`; a fresh `tablet.cred`
  is generated and port 6080 comes back authenticated.
- Restart only the bridge after a desktop restart:
  `setsid bash scripts/start_tablet.sh :99 6080 > run/tablet.log 2>&1 < /dev/null &`
  (x11vnc attaches to the Xvfb process, so it must be restarted after it).
- Health checks: `curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:6080/`
  must print 401; appending `-u "$(cat run/tablet.cred)"` must print 200.
- Acceptance T10 (`tests/acceptance.sh`) verifies the 401 gate; a full
  tablet-enabled run reports PASS=13 (T11 Blender remains NOT_TESTED unless
  `--blender` was also built).
