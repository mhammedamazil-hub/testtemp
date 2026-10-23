# Stonewild changelog

## 0.2.1 — Android startup repair

- Fixed a lifecycle event-loop deadlock that could leave the NativeActivity on a black, non-responsive frame after its window and focus events arrived.
- Recomputed render readiness after every lifecycle event instead of reusing a stale blocking timeout.
- Limited Android mesh generation to one or two nearest chunks per frame instead of building every loaded chunk before presenting the first frame.
- Switched Android startup to the bounded low-memory world profile and capped loaded save settings to that profile.
- Added lifecycle and first-presented-frame diagnostics to Android logcat.
- Added a CI emulator launch test that installs the APK, starts NativeActivity, waits for a successful rendered frame, checks that the process remains alive, and rejects ANR/fatal-exception output.
- Bumped the Android package version to 2 / 0.2.1.
