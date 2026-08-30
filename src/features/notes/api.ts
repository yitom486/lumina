import { invoke } from "@tauri-apps/api/core";

import type { Note, NoteCreate, NoteUpdate } from "./types";

export function listNotes(mediaPath: string): Promise<Note[]> {
  return invoke("notes_list", { mediaPath });
}

export function createNote(input: NoteCreate): Promise<Note> {
  return invoke("notes_create", { input });
}

export function updateNote(input: NoteUpdate): Promise<Note> {
  return invoke("notes_update", { input });
}

export function deleteNote(id: string): Promise<void> {
  return invoke("notes_delete", { id });
}

export function exportNotesMarkdown(mediaPath: string): Promise<string> {
  return invoke("notes_export_markdown", { mediaPath });
}
