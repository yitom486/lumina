/**
 * Shared TanStack Query keys for transcripts (O1).
 *
 * Consumed by transcript / chapters / notes panels and dialogs against one
 * QueryClient: identical keys are what makes the cache actually shared.
 */

export function transcriptKey(
  path: string | null | undefined,
  choiceId: string | null | undefined,
) {
  return ["transcript", path, choiceId] as const;
}

export function subtitleChoicesKey(path: string | null | undefined) {
  return ["subtitleChoices", path] as const;
}
