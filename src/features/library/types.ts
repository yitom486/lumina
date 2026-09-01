import type { AgentProfilesHint } from "@/features/acp/types";

export type LibraryWatchConfig = {
  roots: string[];
  pollIntervalSecs: number;
};

export type LibraryStatus = {
  running: boolean;
  roots: string[];
  pollIntervalSecs: number;
  lastScanAtMs?: number | null;
  lastScanError?: LibraryScanIssue | null;
  indexedFiles: number;
  pendingGroups: number;
};

/** Safe background-scan feedback. Diagnostic details remain in Rust logs. */
export type LibraryScanIssue = {
  code: string;
  message: string;
};

export type LibraryScanEvent =
  | { type: "Started"; payload: { rootCount: number } }
  | { type: "Progress"; payload: { rootsCompleted: number; rootCount: number; indexedFiles: number } }
  | { type: "Finished"; payload: { indexedFiles: number; pendingGroups: number } }
  | { type: "Failed"; payload: { code: string; message: string } };

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

export type ModelDiscoveryConfig = {
  baseUrl: string;
  apiKeyEnv: string;
};

export type ModelDiscoveryResult = {
  connected: boolean;
  models: string[];
  message: string;
};

export type TmdbConfig = {
  accessTokenEnv: string;
  language: string;
};

export type ResolverProviderConfig =
  | {
      kind: "acpAgent";
      profileId: string;
      profiles: AgentProfilesHint;
      modelId?: string;
      reasoningEffort?: string;
    }
  | { kind: "directApi"; model: ModelResolverConfig };

export type AgentModelDiscoveryConfig = {
  profileId: string;
  profiles: AgentProfilesHint;
};

export type AcpSessionOption = {
  value: string;
  name: string;
  description?: string | null;
};

export type AgentModelDiscoveryResult = {
  connected: boolean;
  options: {
    models: AcpSessionOption[];
    reasoningEfforts: AcpSessionOption[];
    currentModelId?: string | null;
    currentReasoningEffort?: string | null;
  };
  message: string;
};

export type ResolverRunConfig = {
  privacyAcknowledged: boolean;
  provider: ResolverProviderConfig;
  tmdb: TmdbConfig;
};

export type CredentialKind = "modelApiKey" | "tmdbAccessToken";

export type MetadataCredentialStatus = {
  modelApiKeySaved: boolean;
  tmdbAccessTokenSaved: boolean;
};

export type SaveMetadataCredentialsInput = {
  modelApiKey?: string;
  tmdbAccessToken?: string;
};

export type CredentialValidationConfig = {
  provider: ResolverProviderConfig;
  tmdb: TmdbConfig;
};

export type CredentialValidationItem = {
  verified: boolean;
  message: string;
};

export type CredentialValidationResult = {
  model: CredentialValidationItem;
  tmdb: CredentialValidationItem;
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

export type MetadataCastMember = {
  name: string;
  character: string;
  order: number;
};

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
  cast?: MetadataCastMember[];
  creators?: string[];
  network?: string | null;
  status?: string | null;
  updatedAtMs: number;
};

export type MediaMetadataContext = {
  mediaPath: string;
  group: StoredMetadata;
  item?: StoredMetadata | null;
  wiki?: WikiMetadata | null;
  merged?: MergedMediaContext | null;
};

export type WikiMatchMethod = "wikidata" | "search" | "userSelected";

export type WikiCandidateSource = "wikidata" | "search";

export type WikiMatchInfo = {
  method: WikiMatchMethod;
  candidatesConsidered: number;
};

export type WikiCharacter = {
  name: string;
  actor: string;
  bio?: string | null;
};

export type WikiEpisodeSummary = {
  season: number;
  episode: number;
  title?: string | null;
  plot: string;
};

export type WikiMetadata = {
  schemaVersion: number;
  wikidataId?: string | null;
  pageLang: string;
  pageTitle: string;
  pageUrl: string;
  extract: string;
  attribution: string;
  license: string;
  matchInfo: WikiMatchInfo;
  characters?: WikiCharacter[];
  episodes?: WikiEpisodeSummary[];
  relationships?: string | null;
  updatedAtMs: number;
};

export type WikiEnrichmentCandidate = {
  pageLang: string;
  pageTitle: string;
  pageUrl: string;
  wikidataId?: string | null;
  extract?: string | null;
  source: WikiCandidateSource;
};

export type WikiEnrichmentPreview = {
  wikidataCandidate?: WikiEnrichmentCandidate | null;
  searchCandidates: WikiEnrichmentCandidate[];
  recommended?: WikiEnrichmentCandidate | null;
  needsUserPick: boolean;
  conflict: boolean;
};

export type WikiWriteResult = {
  root: string;
  groupKey: string;
  writtenFile: string;
};

export type MergedMediaContext = {
  overview?: string | null;
  synopsis?: string | null;
  episodeOverview?: string | null;
  characters?: WikiCharacter[] | null;
  wikiEpisodePlot?: string | null;
  wikiAttribution?: string | null;
  wikiPageUrl?: string | null;
};
