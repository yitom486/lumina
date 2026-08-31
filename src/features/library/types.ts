export type LibraryWatchConfig = {
  roots: string[];
  pollIntervalSecs: number;
};

export type LibraryStatus = {
  running: boolean;
  roots: string[];
  pollIntervalSecs: number;
  lastScanAtMs?: number | null;
  indexedFiles: number;
  pendingGroups: number;
};

export type MediaGroupKind = "movie" | "series" | "unknown";
export type MetadataMediaType = "movie" | "tv";

export type GroupResolution =
  | { state: "pending" }
  | { state: "matched"; tmdbId: number; mediaType: MetadataMediaType }
  | { state: "ignored" };

export type IndexedMediaFile = {
  relativePath: string;
  fileName: string;
  sizeBytes: number;
  modifiedAtMs: number;
  groupKey: string;
  season?: number | null;
  episode?: number | null;
};

export type MediaGroup = {
  key: string;
  displayName: string;
  kind: MediaGroupKind;
  files: string[];
  manualTitle?: string | null;
  resolution: GroupResolution;
};

export type LibraryIndex = {
  schemaVersion: number;
  root: string;
  updatedAtMs: number;
  files: IndexedMediaFile[];
  groups: MediaGroup[];
};

export type PendingMediaGroup = {
  root: string;
  group: MediaGroup;
};

export type LibraryErrorDto = {
  code: string;
  message: string;
  details?: string;
};
