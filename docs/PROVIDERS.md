# AI providers

SourceForge ships four adapters. All of them are optional; without a provider the app
runs fully offline with deterministic extractive answers and local study generation.

| Adapter | Protocol | Endpoints used | Typical use |
|---|---|---|---|
| OpenAI-compatible | `POST /chat/completions`, `/embeddings`, `/audio/speech`, `/audio/transcriptions` | OpenAI, OpenRouter, LM Studio, vLLM, LiteLLM, Azure-style gateways | everything |
| Anthropic | `POST /v1/messages` (SSE streaming) | Claude models | chat, study |
| Gemini | `POST /v1beta/models/{model}:generateContent` (+ `:streamGenerateContent`) | Google AI Studio keys | chat, study, vision |
| Ollama | `POST /api/chat`, `/api/embeddings` (NDJSON streaming) | local models | offline-capable chat |

## Routing

Each profile has `defaultModel` plus an optional per-task override:

`chat · summary · study · audioScript · vision · embedding · tts · transcription`

`resolveModel(profile, task)` returns the override or the default. Requests that need a
capability the profile does not declare are refused before any network call, with a
message naming the missing capability (for example "This provider is not marked as
supporting speech synthesis").

## Capability matrix

```ts
ProviderCapabilities {
  text, vision, embeddings, tts, transcription, streaming: boolean
}
```

Defaults per adapter are conservative, and every flag is editable in Settings. The
Settings screen shows which features are currently usable, and the media/audio panels
check the flags before offering the action.

## Keys and headers

* Keys are entered in Settings and live in the in-memory store.
* "Remember keys on this device" (`persistKeys`) is off by default. When off, saved
  settings strip the key; after a reload the profile still exists but needs the key
  again.
* The GitHub token is treated the same way.
* `redactSecrets()` scrubs key-shaped strings from error messages before they reach a
  toast, the console or the chat transcript.
* Backups never contain keys, tokens or custom auth headers.
* The service worker does not cache cross-origin responses at all, so an API response
  cannot be replayed from Cache Storage.
* Requests are only sent to the profile's own `baseUrl`, over HTTPS unless the URL is
  localhost/127.0.0.1 (Ollama, LM Studio). The UI warns for plain-HTTP remote URLs.

## Streaming and cancellation

Adapters implement SSE (OpenAI, Anthropic, Gemini) and NDJSON (Ollama) streaming with a
line buffer, an `AbortSignal`, and a final `usage`/`model` report. The chat panel streams
deltas into the message and offers a Stop button that aborts the request. A stream that
never yields a delta falls back to the non-streaming response.

## Errors

`ProviderError` carries `code` (`auth`, `rate-limit`, `network`, `cors`, `bad-request`,
`server`, `timeout`, `cancelled`), the HTTP status, the provider's message and a hint.
Notable mappings:

* `401/403` → auth, "check the key and whether it is allowed from a browser"
* `429` → rate limit, retry-after when present
* network failure → explicit "this is usually CORS or a network block" text
* `AbortError` → cancelled, shown as a cancellation rather than an error

## Provider compatibility notes

* **CORS**: browser calls require the provider to allow the origin. OpenAI, Anthropic and
  Gemini do; self-hosted gateways often need `Access-Control-Allow-Origin`. Ollama needs
  `OLLAMA_ORIGINS` to include the app origin.
* **Anthropic browser access** requires the `anthropic-dangerous-direct-browser-access`
  header, which the adapter sends.
* **Azure OpenAI** uses a deployment path and `api-key` header; enter the full base URL
  and add the header in the profile's custom headers.
* **Gemini** keys travel in the query string (`?key=`), which is why the app never logs
  request URLs for Gemini.
* **TTS/transcription** are only offered by adapters that implement them; when a
  provider lacks them, the audio overview panel says narration is unavailable instead of
  offering a fake player.

## Do you need a provider?

No. The offline path covers chat (extractive), every study artifact, and all exports.
Provider-dependent features are labelled in the UI: narrative answers, AI study
material, narratable audio, transcription, vision, embeddings, and semantic search.
