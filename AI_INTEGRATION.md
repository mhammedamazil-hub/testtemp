# Experimental visual-agent status

**Source status:** native provider request, per-workflow screenshot consent, plan review, visible-cursor UI-action bridge and bounded action validation have been implemented. They have **not** been built, emulator-tested, connected to a live provider or verified against real editing workflows. The agent is experimental and must not be described as a reliable editor operator yet.

## Interaction and consent model

- The selected real editor stays open in its own WebView iframe. The host loads no inactive editor.
- Before each planning workflow, Android asks permission to send the task text and a screenshot of the visible editor to the selected provider. If the user approves the plan, updated screenshots are sent for subsequent visual re-planning under that same workflow consent. Screenshots are not written to app storage by the agent; the provider may retain them under its own policy.
- The provider returns a bounded JSON plan. The user must approve that plan before any editor action. Destructive edits/exports may run after that approval without another confirmation, as requested.
- The experimental action vocabulary is limited to in-editor click, drag, scroll, short plain-text typing, a small key allowlist, wait and stop/done. Coordinates are normalized and confined to the iframe. Typed text rejects code-like punctuation and multiline input. The agent never executes generated JavaScript, shell commands, arbitrary URLs or arbitrary file paths.
- After the initial approved plan, visual re-planning is constrained to each approved step and its action type; approved typed text and keys are kept unchanged. A visible cursor, progress status, Stop control and conservative stop-on-error behavior are provided.
- These protections constrain the protocol, not the model's understanding. Synthetic DOM pointer/keyboard events may be ignored or interpreted differently by an editor. No undo/checkpoint integration or operation history is implemented. Verify the project state and save/export through the editor's own controls.

## Provider configuration

- Providers: Gemini, Anthropic/Claude and OpenAI-compatible.
- Endpoint and model are user-configurable; API endpoints must use HTTPS and cannot embed credentials in the URL.
- API keys are stored encrypted with an Android Keystore AES-GCM key and are not exposed to the WebView or written to app diagnostics. Provider requests run in the Android process.
- The Settings connection test sends a generic short prompt only; it does not include screenshots or project data.
- No model discovery, usage accounting or provider-side retention controls are implemented. Only use a vision-capable model. Live API requests have not been tested.

## Editor coverage and constraints

The native bridge targets the actual open WebView UI for all seven pinned upstream editors. It does **not** use their MCP servers or web command APIs. It therefore has no verified editor-specific capability registry, parameter schemas or operation-level verification. Coverage is presently **experimental and unverified for every editor**:

| Editor | Current source-level path | Not verified |
|---|---|---|
| PhotoCraft | Visible pointer/keyboard actions in the genuine WebAssembly UI | Tool targeting, image import/edit/save/export, undo and mobile usability |
| VectorCraft | Visible pointer/keyboard actions in the genuine WebAssembly UI | Shape/path operations, SVG round-trip and mobile usability |
| FilmCraft | Visible pointer/keyboard actions in the genuine WebAssembly UI | Timeline, video/audio, keyframe, render/export and playback workflows |
| LightCraft | Visible pointer/keyboard actions in the genuine WebAssembly UI | Library/RAW import, color development, worker/GPU behavior and export |
| PdfCraft | Visible pointer/keyboard actions plus existing host PDF save-message handling | Document open/edit/save, page operations and PDF round-trip |
| EffectCraft | Visible pointer/keyboard actions in the genuine WebAssembly UI | Node/effect editing, worker/GPU and rendering/export workflows |
| DesignCraft | Visible pointer/keyboard actions in the genuine WebAssembly UI | Layout/typography tasks and document round-trip |

The new CI smoke test checks only that each editor reaches a visible canvas and does not report a fatal startup error within its initial observation window. It does not test the AI agent or prove editing/export success. The updated workflow has not yet run. No Android emulator or physical-device runtime result is currently available.

## Security and data handling

- No API key is embedded in HTML/JavaScript/assets, logged, or sent to the WebView. Gemini credentials are placed in a request header rather than a URL query.
- No external screenshot/project upload occurs before an explicit per-workflow native consent dialog. Consent text states that follow-up screenshots are also sent if the user later approves execution.
- Local WebView diagnostics keep category-only error messages and bundled asset paths, not arbitrary console text that could include project content or secrets.
- Generated code and shell commands are not an allowed action type; text entry is bounded to short plain-language text/simple names.
- The user can approve destructive operations as part of the full plan; therefore review the plan carefully. The Stop control interrupts between visual actions where feasible, but cannot guarantee immediate interruption of a provider network request or an editor action already in progress.

## Required next validation

1. Build the updated debug APK in Actions; fix any Kotlin/Gradle errors.
2. Run the seven-editor Android emulator startup smoke test and inspect the diagnostics/report artifact.
3. Verify screenshot capture quality (including WebGL/WebGPU canvases), UI cursor alignment, and consent/cancel behavior on the emulator.
4. With consent, test one simple, reversible task in each editor; verify real UI actions, stop/recovery and editor-native undo.
5. Test actual project open/edit/save/reopen/export, including FilmCraft video/motion and LightCraft/EffectCraft resource-heavy paths.
6. Measure startup, memory and interaction on a 4 GB-class tablet such as Galaxy Tab A9 when a device becomes available.

Do not mark any editor task as supported until it has repeatable end-to-end tests against the genuine upstream engine and its real project files.
