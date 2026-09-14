/** Track orchestration helpers (no Tauri, no stores, no invoke).
 *
 * Player/subtitle switching is driven by two injected ports:
 * `setSubtitle` (native surface) and `loadOnlineTranscript` (online cache).
 * Desktop adapters pass the real implementations.
 */

import type { SubtitleChoice, Transcript } from "@lumina/contracts";

export type SetSubtitleArgs = {
  source: "Embedded" | "Sidecar" | "None";
  streamIndex?: number | null;
  externalPath?: string | null;
  /** Opaque downloaded-track id (`cache:<provider>:<lang>`); resolved to a
   * real file inside the backend so the cache path never crosses IPC. */
  choiceId?: string | null;
  /** Local media path the cached track belongs to (required with choiceId). */
  mediaPath?: string | null;
};

export async function applySubtitleChoice(
  choice: SubtitleChoice | undefined,
  setSubtitle: (args: SetSubtitleArgs) => Promise<void>,
  mediaPath?: string,
  loadOnlineTranscript?: (
    pageUrl: string,
    choiceId: string,
  ) => Promise<Transcript>,
): Promise<void> {
  if (!choice) {
    await setSubtitle({ source: "None" });
    return;
  }
  if (choice.source === "Embedded") {
    await setSubtitle({
      source: "Embedded",
      streamIndex: choice.streamIndex,
    });
    return;
  }
  if (choice.id.startsWith("cache:")) {
    // Downloaded tracks have no file beside the media (process cache):
    // hand the opaque id to the backend instead of a null externalPath
    // (which the player rejects and silently shows nothing).
    await setSubtitle({
      source: "Sidecar",
      externalPath: null,
      choiceId: choice.id,
      mediaPath: mediaPath ?? null,
    });
    return;
  }
  if (choice.id.startsWith("online:") && !choice.externalPath) {
    if (!mediaPath || !loadOnlineTranscript) return;
    const transcript = await loadOnlineTranscript(mediaPath, choice.id);
    await setSubtitle({
      source: "Sidecar",
      externalPath: transcript.sourcePath,
    });
    return;
  }
  await setSubtitle({
    source: "Sidecar",
    externalPath: choice.externalPath,
  });
}

export function audioTrackLabel(stream: {
  index: number;
  language?: string | null;
  codecName?: string | null;
  channels?: number | null;
}): string {
  const lang = stream.language ?? "und";
  const codec = stream.codecName ?? "audio";
  const ch = stream.channels ? `${stream.channels}ch` : null;
  return [lang, codec, ch].filter(Boolean).join(" · ");
}
