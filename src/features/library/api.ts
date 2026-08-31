import { invoke } from "@tauri-apps/api/core";

import type {
  LibraryIndex,
  LibraryStatus,
  LibraryWatchConfig,
  PendingMediaGroup,
} from "./types";

export function startLibraryWatch(config: LibraryWatchConfig): Promise<LibraryStatus> {
  return invoke<LibraryStatus>("library_watch_start", { config });
}

export function stopLibraryWatch(): Promise<LibraryStatus> {
  return invoke<LibraryStatus>("library_watch_stop");
}

export function getLibraryStatus(): Promise<LibraryStatus> {
  return invoke<LibraryStatus>("library_status");
}

export function scanLibraryNow(): Promise<LibraryIndex[]> {
  return invoke<LibraryIndex[]>("library_scan_now");
}

export function listPendingMediaGroups(): Promise<PendingMediaGroup[]> {
  return invoke<PendingMediaGroup[]>("library_pending_groups");
}
