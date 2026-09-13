/**
 * Shared TanStack Query key for cached yt-dlp resolve results (Online Source).
 *
 * The tuple shape is frozen: OnlineSourcePanel, TranscriptPanel,
 * useVideoPromptContext and ChaptersPanel share one cache entry per URL,
 * so the factory must keep returning ["ytdl-resolve", url].
 * (Pre-existing kebab-case is kept for cache compatibility; do NOT
 * introduce a second camelCase variant for the same data.)
 */

export function ytdlResolveKey(url: string | null | undefined) {
  return ["ytdl-resolve", url] as const;
}
