# Shared AI integration — evidence and status

**Overall:** NOT STARTED for real editor execution. The provider configuration UI is a source scaffold; it is not yet an editing agent.

## Provider settings implemented in source

- User-editable provider: Gemini, Claude/Anthropic, or OpenAI-compatible.
- User-editable HTTPS endpoint and model ID (no guessed model is hardcoded).
- API key encrypted with an Android Keystore AES-GCM key; key is never written to logs.
- User-triggered generic connectivity request; no project content is sent by that screen.
- Status: **IMPLEMENTED in source, NOT BUILT or VERIFIED**. No live provider connection has been tested.
- Not implemented: model discovery, token/usage accounting, assistant conversation, tool calling, consent dialogs for project data, action planning/execution, operation history, or undo integration.

## Upstream automation surfaces inspected

| Editor | Existing source evidence | Adapter state in ArtCraft Studio |
|---|---|---|
| PhotoCraft | Desktop UI/CLI/MCP share an engine command registry. Its inspected web frontend has browser file open/save but no JavaScript command bridge. | **BLOCKED** pending a real web/engine adapter; do not emulate editor clicks. |
| VectorCraft | `window.vectorcraft` web API exposes command execution/discovery, UI inspection, screenshots and file capture/readback. | **INVESTIGATING**; no Android adapter or action tests yet. |
| FilmCraft | `window.filmcraft` web API and command catalog exist. Web build has limitations: single-threaded per instance and some operations report unavailable. | **INVESTIGATING**; no Android adapter or action tests yet. |
| LightCraft | `lightcraft.command(id, params)` is documented for the WebAssembly UI; browser storage and worker rendering are editor-managed. | **INVESTIGATING**; no Android adapter or action tests yet. |
| PdfCraft | Desktop CLI/MCP automation exists. The web host `postMessage` bridge handles open/save/dirty state, not the full command catalog. | **BLOCKED** pending real engine/web command adapter. |
| EffectCraft | `window.effectcraft` web API exposes editor commands and render/file operations. Web jobs/workers have platform-specific limitations. | **INVESTIGATING**; no Android adapter or action tests yet. |
| DesignCraft | Desktop command/control/MCP support exists. The upstream explicitly says its web build has no control channel. | **BLOCKED** pending real engine/web command adapter. |

These findings describe source surfaces, not tested capabilities. A command is not eligible for the AI capability registry until its exact ID, parameter schema, validation, cancellation behavior, undo/checkpoint behavior and output verification have tests against the real editor session.

## Required execution design

1. Ask before any external provider request that includes project content or selected assets; default to sending only the user prompt and tested capability metadata.
2. Query the active editor adapter for real typed capabilities. Never let the model invent executable code or arbitrary filesystem/network operations.
3. Have the model return a multi-step plan/tool request with strict editor-specific JSON Schema.
4. Validate every command/parameter locally against the active editor's current command catalog and project state.
5. Ask explicit approval for destructive operations; checkpoint using the editor's native history/save model where supported.
6. Execute one operation at a time through the actual upstream command API, report progress, support cancellation and verify each result from the engine response/project state.
7. Keep provider credentials in native secure storage and call providers from the Android process. Do not expose keys to the WebView or embed secrets in JS, assets, logs, workflow files or Git.
8. Record redacted operation history and provide a user-facing undo/recovery path. Do not claim undo if an editor cannot restore the operation.
9. Cross-editor workflows require explicit asset handoff, native project preservation and verification at every boundary.

## Status vocabulary

- Provider fields/key storage/connection-test code: **IMPLEMENTED in source**, not built/tested.
- Shared agent UI and task planning: **NOT STARTED**.
- Seven-editor capability registry: **NOT STARTED**.
- Real tool execution adapters: **NOT STARTED** (four existing web APIs identified; three need adapter work).
- Destructive approval, undo/checkpoint, cancellation and execution history: **NOT STARTED**.
- Cross-editor automation: **NOT STARTED**.
