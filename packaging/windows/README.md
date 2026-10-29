# SourceForge for Windows (portable)

This folder ships with a release ZIP built by
`.github/workflows/platforms.yml` (job `windows-portable`).

```
sourceforge-<version>-windows-portable/
├─ app/                     # the same production build served by the PWA
├─ Start-SourceForge.cmd    # double-click launcher
├─ Start-SourceForge.ps1    # .NET HttpListener static server
└─ README.txt
```

## Use

1. Unzip anywhere (no installer, no admin rights).
2. Double-click `Start-SourceForge.cmd`.
3. Your browser opens `http://127.0.0.1:8787/` with the full app.

Notebooks are stored in the browser profile's IndexedDB. Use **Export backup**
inside the app before clearing browser data or switching machines.

## Status

* **Built in CI**: yes — the ZIP is produced from the verified web build.
* **Executed on Windows**: **not verified**. This build environment has no
  Windows host and no PowerShell, so the launcher is static-checked only. If it
  fails, any static file server works as a fallback:

  ```
  npx --yes serve app
  ```

## Why a launcher at all?

The app is a static PWA with ES modules and a service worker. Browsers refuse
module scripts and service workers on `file://`, so the files must be served
over HTTP. The launcher is that HTTP server — it does not proxy any content or
add a backend.

## Not built here

* `.msi` / `.exe` installers (need WiX/NSIS on a Windows runner)
* A Tauri or Electron shell (needs Rust or Electron on a Windows runner)

Those are deliberately out of scope rather than faked.
