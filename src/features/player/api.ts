/** Thin Tauri command wrappers. UI must not call invoke with raw strings elsewhere. */

import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import type { PlayerEvent, PlayerSnapshot } from "./types";

const VIDEO_FILTERS = [
  {
    name: "Video",
    extensions: ["mp4", "mkv", "webm", "avi", "mov", "m4v"],
  },
];

export async function pickVideoFile(
  defaultDirectory?: string | null,
): Promise<string | null> {
  const selected = await open({
    multiple: false,
    filters: VIDEO_FILTERS,
    defaultPath: defaultDirectory?.trim() || undefined,
  });
  if (!selected || Array.isArray(selected)) {
    return null;
  }
  return selected;
}

export function subscribePlayerEvents(
  onEvent: Channel<PlayerEvent>,
): Promise<void> {
  return invoke("player_subscribe", { onEvent });
}

export function getPlayerState(): Promise<PlayerSnapshot> {
  return invoke<PlayerSnapshot>("player_get_state");
}

export function openPlayer(path: string): Promise<PlayerSnapshot> {
  return invoke<PlayerSnapshot>("player_open", { path });
}

export function playPlayer(): Promise<PlayerSnapshot> {
  return invoke<PlayerSnapshot>("player_play");
}

export function pausePlayer(): Promise<PlayerSnapshot> {
  return invoke<PlayerSnapshot>("player_pause");
}

export function stopPlayer(): Promise<PlayerSnapshot> {
  return invoke<PlayerSnapshot>("player_stop");
}

export function seekPlayer(positionMs: number): Promise<PlayerSnapshot> {
  return invoke<PlayerSnapshot>("player_seek", { positionMs });
}

export function setPlayerVolume(volume: number): Promise<PlayerSnapshot> {
  return invoke<PlayerSnapshot>("player_set_volume", { volume });
}

export function setPlayerRate(rate: number): Promise<PlayerSnapshot> {
  return invoke<PlayerSnapshot>("player_set_rate", { rate });
}

export function setPlayerSubtitle(args: {
  source: "Embedded" | "Sidecar" | "None";
  streamIndex?: number | null;
  externalPath?: string | null;
}): Promise<PlayerSnapshot> {
  return invoke<PlayerSnapshot>("player_set_subtitle", {
    source: args.source,
    streamIndex: args.streamIndex ?? null,
    externalPath: args.externalPath ?? null,
  });
}

export function setPlayerAudio(streamIndex: number): Promise<PlayerSnapshot> {
  return invoke<PlayerSnapshot>("player_set_audio", { streamIndex });
}

export function setSurfaceBounds(bounds: {
  x: number;
  y: number;
  width: number;
  height: number;
}): Promise<void> {
  return invoke("player_set_surface_bounds", bounds);
}

/** Sorted video paths in the same directory as `path`. */
export function listSiblingVideos(path: string): Promise<string[]> {
  return invoke<string[]>("media_list_siblings", { path });
}
