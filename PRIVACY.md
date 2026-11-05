# Privacy

Kaku is a local desktop application. It has no servers, accounts, telemetry or analytics.

## What leaves your computer

- **The text you act on**, sent to the AI provider *you* configure (Google Gemini or an OpenAI-compatible endpoint such as OpenAI, a corporate gateway, or a local Ollama/LM Studio server), together with the action's instruction prompt. That provider's own privacy policy applies.
- **Update checks** (if enabled): a request to `github.com` for the `latest.json` release manifest. No identifiers are sent.

Nothing else is transmitted.

## What is stored locally

| Data | Where | Notes |
|---|---|---|
| API keys | OS credential vault (Windows Credential Manager, Secret Service on Linux) | Never written to files or exposed to the UI |
| Settings | `settings.json` in the app config directory | No secrets |
| Usage metrics | `kaku.db` (SQLite) in the app data directory | Timestamp, action, provider, model, token counts, latency, success/error code — **no text** |
| History (optional) | same SQLite database | Input and output text. Off → existing history is deleted. Retention configurable |
| Logs | app log directory | Metadata only, never text content; rotated (3 × 2 MB) |

## Clipboard

To read a selection and paste a result, Kaku briefly uses the system clipboard and restores your previous text afterwards. On Windows, these temporary entries are excluded from clipboard history and cloud clipboard sync.

## Removing your data

Uninstall the app, then delete the app config/data directories (shown in **Settings › About**) and the `io.github.klysman08.kaku` entries from your credential vault.
