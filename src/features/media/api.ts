import { invoke } from "@tauri-apps/api/core";

import type { MediaInfo } from "./types";

export function inspectMedia(path: string): Promise<MediaInfo> {
  return invoke<MediaInfo>("media_inspect", { path });
}
