import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import type { SubtitleTrackInfo, Transcript } from "./types";

export function listSubtitleTracks(path: string): Promise<SubtitleTrackInfo[]> {
  return invoke("subtitle_list_tracks", { path });
}

export function loadTranscript(
  path: string,
  streamIndex: number,
): Promise<Transcript> {
  return invoke("subtitle_load_transcript", { path, streamIndex });
}

export function loadExternalTranscript(path: string): Promise<Transcript> {
  return invoke("subtitle_load_external", { path });
}

export async function pickExternalSubtitle(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    filters: [
      {
        name: "Subtitles",
        extensions: ["srt", "vtt", "ass", "ssa"],
      },
    ],
  });
  if (!selected || Array.isArray(selected)) return null;
  return selected;
}
