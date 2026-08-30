# Optional ACP agents (not bundled)

Lumina is an **ACP Client**. It does **not** ship Node/bun/pnpm or Codex.

## Layers

1. **ACP** — open stdio JSON-RPC (what Lumina speaks)
2. **codex-acp** — usually a TypeScript/Node adapter (or a single-file bundle you drop here as `codex-acp.exe`)
3. **Codex App Server** — Codex harness engine started *by* the adapter (not Lumina’s main protocol)
4. **Models** — Codex providers use the **Responses API** (not legacy Chat Completions)

## Local discovery

You may place here:

- `codex-acp.exe` / `codex-acp` (adapter or bundled binary)
- `codex.exe` / `codex` (optional; sets `CODEX_PATH` for the adapter)

Or install them on `PATH`. Never required for playback, notes, or subtitles.

## Other agents

Configure additional Agent profiles in the app (Claude ACP, custom command, …).
Swapping harness = swapping the ACP process, not embedding App Server.
