import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  CredentialKind,
  AgentModelDiscoveryConfig,
  AgentModelDiscoveryResult,
  CredentialValidationConfig,
  CredentialValidationResult,
  LibraryIndex,
  LibraryStatus,
  LibraryScanEvent,
  LibraryWatchConfig,
  MediaMetadataContext,
  ModelDiscoveryConfig,
  ModelDiscoveryResult,
  MetadataCredentialStatus,
  MetadataMediaType,
  MetadataWriteResult,
  MediaGroup,
  PendingMediaGroup,
  ResolverPreview,
  ResolverRunConfig,
  SaveMetadataCredentialsInput,
  TmdbConfig,
  CredentialValidationItem,
  WikiEnrichmentCandidate,
  WikiEnrichmentPreview,
  WikiGroupStatus,
  WikiMatchMethod,
  WikiWriteResult,
} from "./types";

export function discoverLibraryModels(
  config: ModelDiscoveryConfig,
): Promise<ModelDiscoveryResult> {
  return invoke<ModelDiscoveryResult>("library_models_discover", { config });
}

export function discoverLibraryAgentModels(
  config: AgentModelDiscoveryConfig,
): Promise<AgentModelDiscoveryResult> {
  return invoke<AgentModelDiscoveryResult>("library_agent_models_discover", { config });
}

export function getMetadataCredentialStatus(): Promise<MetadataCredentialStatus> {
  return invoke<MetadataCredentialStatus>("library_credential_status");
}

export function saveMetadataCredentials(
  input: SaveMetadataCredentialsInput,
): Promise<MetadataCredentialStatus> {
  return invoke<MetadataCredentialStatus>("library_credentials_save", { input });
}

export function deleteMetadataCredential(
  kind: CredentialKind,
): Promise<MetadataCredentialStatus> {
  return invoke<MetadataCredentialStatus>("library_credential_delete", { kind });
}

export function validateMetadataCredentials(
  config: CredentialValidationConfig,
): Promise<CredentialValidationResult> {
  return invoke<CredentialValidationResult>("library_credentials_validate", { config });
}

export function validateTmdbCredentials(
  config: TmdbConfig,
): Promise<CredentialValidationItem> {
  return invoke<CredentialValidationItem>("library_tmdb_credentials_validate", { config });
}

export function startLibraryWatch(
  config: LibraryWatchConfig,
  onEvent?: (event: LibraryScanEvent) => void,
): Promise<LibraryStatus> {
  const channel = new Channel<LibraryScanEvent>();
  if (onEvent) channel.onmessage = onEvent;
  return invoke<LibraryStatus>("library_watch_start", { config, onEvent: channel });
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

export function setManualMediaTitle(input: {
  root: string;
  groupKey: string;
  title: string;
}): Promise<PendingMediaGroup> {
  return invoke<PendingMediaGroup>("library_set_manual_title", input);
}

export function previewMediaMatch(input: {
  root: string;
  groupKey: string;
  config: ResolverRunConfig;
}): Promise<ResolverPreview> {
  return invoke<ResolverPreview>("library_resolve_preview", input);
}

export function applyTmdbMediaMatch(input: {
  root: string;
  groupKey: string;
  tmdbId: number;
  mediaType: MetadataMediaType;
  tmdb: TmdbConfig;
}): Promise<MetadataWriteResult> {
  return invoke<MetadataWriteResult>("library_apply_tmdb_match", input);
}

export function listLibraryGroups(root: string): Promise<MediaGroup[]> {
  return invoke<MediaGroup[]>("library_list_groups", { root });
}

export function previewWikipediaEnrichment(input: {
  root: string;
  groupKey: string;
  tmdb: TmdbConfig;
}): Promise<WikiEnrichmentPreview> {
  return invoke<WikiEnrichmentPreview>("library_wikipedia_preview", input);
}

export function applyWikipediaPage(input: {
  root: string;
  groupKey: string;
  candidate: WikiEnrichmentCandidate;
  matchMethod: WikiMatchMethod;
  candidatesConsidered: number;
}): Promise<WikiWriteResult> {
  return invoke<WikiWriteResult>("library_wikipedia_apply", input);
}

export function refreshWikipediaPage(input: {
  root: string;
  groupKey: string;
}): Promise<WikiWriteResult> {
  return invoke<WikiWriteResult>("library_wikipedia_refresh", input);
}

export function listWikipediaStatuses(root: string): Promise<WikiGroupStatus[]> {
  return invoke<WikiGroupStatus[]>("library_wikipedia_statuses", { root });
}

export function getMediaMetadataContext(
  mediaPath: string,
): Promise<MediaMetadataContext | null> {
  return invoke<MediaMetadataContext | null>("library_context_for_media", {
    mediaPath,
  });
}
