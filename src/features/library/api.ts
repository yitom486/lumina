import { invoke } from "@tauri-apps/api/core";

import type {
  CredentialKind,
  CredentialValidationConfig,
  CredentialValidationResult,
  LibraryIndex,
  LibraryStatus,
  LibraryWatchConfig,
  MediaMetadataContext,
  ModelDiscoveryConfig,
  ModelDiscoveryResult,
  MetadataCredentialStatus,
  MetadataMediaType,
  MetadataWriteResult,
  PendingMediaGroup,
  ResolverPreview,
  ResolverRunConfig,
  SaveMetadataCredentialsInput,
  TmdbConfig,
} from "./types";

export function discoverLibraryModels(
  config: ModelDiscoveryConfig,
): Promise<ModelDiscoveryResult> {
  return invoke<ModelDiscoveryResult>("library_models_discover", { config });
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

export function getMediaMetadataContext(
  mediaPath: string,
): Promise<MediaMetadataContext | null> {
  return invoke<MediaMetadataContext | null>("library_context_for_media", {
    mediaPath,
  });
}
