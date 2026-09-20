# Batch 6 UI design QA

Date: 2026-09-20

## Target

- References: `ui/lumina-desktop-overview-reference.png`,
  `ui/lumina-settings-workspace-reference.png`,
  `ui/lumina-transcript-reading-reference.png`,
  `ui/lumina-ai-automation-settings-reference.png`.
- Outcome: dark graphite desktop shell; a labeled workspaces rail; a full Settings
  workspace with a category column; no configuration forms in the reading rail;
  native libmpv HWND always rendered as a layout sibling.

## Implemented and automated evidence

- The rail exposes only playlist, transcript, notes, chapters, library, and
  settings. Online-resource configuration is not a rail item.
- Settings categories are rendered one at a time: Playback & Interface,
  Subtitle Workshop, Library, Online Resources, and AI & Automation. Each maps
  to an existing real feature panel; inactive panels do not mount their queries
  or task listeners.
- Entering Settings keeps `VideoSurface` mounted but forces the native surface
  bounds to `0 × 0`; leaving it participates in the normal sidebar reflow again.
- `bun run lint`, `bun run test` (76 files / 326 tests), and `bun run build`
  passed. The Tauri dev build completed and launched `lumina-app.exe`.

## Manual visual QA still required

The current automation desktop did not expose the launched Tauri window, so
native screenshots and direct HWND interaction were not available. A browser
preview is not a substitute: it correctly fails before render because it has no
Tauri `Channel` runtime. On a visible desktop window, verify:

1. Settings fills the primary workspace and the native video pixels disappear
   without an overlay or a stale frame.
2. Returning to each reading workspace restores the video surface and player bar.
3. Each settings category has one scrollable content column and no duplicate
   configuration state.
4. Opening the AI dock after leaving Settings restores its existing session and
   retains the same right-side shell baseline as reading panels.

This QA record is intentionally not a completion declaration.
