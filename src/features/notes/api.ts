import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";

import type {
  Note,
  NoteCreate,
  NotePreviewQuotes,
  NoteQuote,
  NoteUpdate,
} from "./types";

/** Default `{parentDir}/{seriesOrDir}.notes.md` next to the media file. */
export function suggestNotesExportPath(
  mediaPath: string,
  documentTitle?: string | null,
): string {
  const normalized = mediaPath.replace(/\\/g, "/");
  const lastSlash = normalized.lastIndexOf("/");
  const parent = lastSlash >= 0 ? normalized.slice(0, lastSlash) : "";
  const baseName = (documentTitle?.trim() || parent.slice(parent.lastIndexOf("/") + 1) || "notes")
    .replace(/[<>:"/\\|?*]/g, "_");
  const fileName = `${baseName}.notes.md`;
  if (!parent) return fileName;
  const sep = mediaPath.includes("\\") ? "\\" : "/";
  return `${parent.replace(/\//g, sep)}${sep}${fileName}`;
}

export function listNotes(mediaPath: string): Promise<Note[]> {
  return invoke("notes_list", { mediaPath });
}

export function previewNoteQuotes(
  input: NotePreviewQuotes,
): Promise<NoteQuote[]> {
  return invoke("notes_preview_quotes", { input });
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

export async function exportNotesMarkdownToFile(
  mediaPath: string,
): Promise<string | null> {
  const destPath = await save({
    title: "导出批注 Markdown",
    filters: [{ name: "Markdown", extensions: ["md"] }],
    defaultPath: suggestNotesExportPath(mediaPath),
  });
  if (!destPath) return null;
  await invoke("notes_export_markdown_to_file", { mediaPath, destPath });
  return destPath;
}
