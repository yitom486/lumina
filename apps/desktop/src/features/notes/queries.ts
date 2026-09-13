/**
 * Shared TanStack Query keys for notes (O1).
 *
 * Saved notes live in Rust; the Query cache is only a read mirror plus
 * invalidation targets after mutations.
 */

export function notesKey(mediaPath: string | null | undefined) {
  return ["notes", mediaPath] as const;
}

export function noteFrameKey(noteId: string) {
  return ["note-frame", noteId] as const;
}
