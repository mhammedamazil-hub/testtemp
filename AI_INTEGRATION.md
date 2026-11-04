# AI integration status and safeguards

## Current native build

The native Rust Android host currently contains no provider-backed AI agent. The earlier screenshot-to-provider and visible-cursor implementation targeted the WebView editor and was removed from the Android build along with that WebView host. No AI/provider functionality is claimed in the native APK attempt.

## Requirements if a native agent is added later

These are standing safeguards, not features already implemented in the native app:

- Ask for explicit per-workflow consent before sending a screenshot, selected document data, or other project content to an external provider; explain what will be sent and where.
- Show a proposed, bounded plan and require user approval before running it.
- Keep automation visible in the native editor with an unmistakable cursor/action indicator, progress and a readily available stop/cancel control.
- Avoid arbitrary scripts and shell commands. Restrict actions to reviewed, bounded native UI/editor operations, and verify results where possible.
- Keep provider credentials out of editor code, logs, screenshots and project data; use Android Keystore-backed encryption for any stored credentials and HTTPS for requests.
- Redact diagnostics. Never log arbitrary project text, screenshots, API keys or provider responses by default.
- Make the network boundary explicit. Local editing must not send project content unless the user consents to that specific operation.
- Do not claim editor-specific coverage until representative tasks, undo/recovery and save/reopen/export outcomes are tested against each authentic upstream Rust editor on Android.

Any future implementation must be native and integrate with the Rust/egui editor sessions; it must not depend on a browser iframe or a WebView JavaScript bridge.
