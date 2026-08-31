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

export type ModelResolverConfig = {
  baseUrl: string;
  modelId: string;
  apiKeyEnv: string;
};

export type TmdbConfig = {
  accessTokenEnv: string;
  language: string;
};

export type ResolverRunConfig = {
  privacyAcknowledged: boolean;
  model: ModelResolverConfig;
  tmdb: TmdbConfig;
};

export type ResolverIntent = {
  mediaType: MetadataMediaType;
  title: string;
  year?: number | null;
  season?: number | null;
  episode?: number | null;
  confidenceMilli: number;
};

export type TmdbCandidate = {
  tmdbId: number;
  mediaType: MetadataMediaType;
  title: string;
  year?: number | null;
  overview?: string | null;
};

export type ResolverSelection = {
  tmdbId: number;
  confidenceMilli: number;
};

export type ResolverPreview = {
  intent: ResolverIntent;
  candidates: TmdbCandidate[];
  selection?: ResolverSelection | null;
  canAutoMatch: boolean;
};

export type MetadataWriteResult = {
  root: string;
  groupKey: string;
  tmdbId: number;
  mediaType: MetadataMediaType;
  writtenFiles: string[];
};

export type StoredMetadataKind = "series" | "episode" | "movie";

export type StoredMetadata = {
  schemaVersion: number;
  kind: StoredMetadataKind;
  tmdbId: number;
  seriesTmdbId?: number | null;
  title: string;
  originalTitle?: string | null;
  overview?: string | null;
  year?: number | null;
  season?: number | null;
  episode?: number | null;
  genres: string[];
  updatedAtMs: number;
};

export type MediaMetadataContext = {
  mediaPath: string;
  group: StoredMetadata;
  item?: StoredMetadata | null;
};
