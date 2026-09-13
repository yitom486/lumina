import { invoke } from "@tauri-apps/api/core";

import type { MediaInfo, MediaToolStatus } from "./types";

export function inspectMedia(path: string): Promise<MediaInfo> {
  return invoke<MediaInfo>("media_inspect", { path });
}

/** ffprobe presence for settings/degrade UI. No media file needed. */
export function getMediaToolStatus(): Promise<MediaToolStatus> {
  return invoke<MediaToolStatus>("media_tool_status");
}
