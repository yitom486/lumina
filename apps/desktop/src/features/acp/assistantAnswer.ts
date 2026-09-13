import type { ChatActivity } from "./types";

const ENGLISH_BLOCK_LINE =
  /^\s*(?:\*\*)?(?:Natural\s+)?English:(?:\*\*)?\s*(.*)$/i;
const ENGLISH_BLOCK_INLINE =
  /`(?:Natural\s+)?English:[^`]+`/gi;

function hasSubstantialCjk(text: string): boolean {
  const cjk = text.match(/[\u4e00-\u9fff]/g);
  return (cjk?.length ?? 0) >= 2;
}

function isEnglishPreambleLine(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed) return false;
  if (ENGLISH_BLOCK_LINE.test(trimmed)) return true;
  if (hasSubstantialCjk(trimmed)) return false;
  return /^[A-Za-z0-9\s.,!?;:'"()\-–—/]+$/.test(trimmed);
}

/** Remove Cursor-style English translation blocks from assistant-visible text. */
export function stripEnglishTranslationBlocks(text: string): string {
  const withoutInline = text.replace(ENGLISH_BLOCK_INLINE, "");
  const lines = withoutInline.split("\n");
  const kept: string[] = [];

  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    if (!ENGLISH_BLOCK_LINE.test(line)) {
      kept.push(line);
      index += 1;
      continue;
    }

    const inline = line.match(ENGLISH_BLOCK_LINE)?.[1]?.trim() ?? "";
    if (inline && !hasSubstantialCjk(inline)) {
      index += 1;
      continue;
    }

    index += 1;
    while (index < lines.length) {
      const next = lines[index];
      if (!next.trim()) {
        index += 1;
        continue;
      }
      if (ENGLISH_BLOCK_LINE.test(next)) {
        break;
      }
      if (hasSubstantialCjk(next)) {
        break;
      }
      if (!isEnglishPreambleLine(next)) {
        break;
      }
      index += 1;
    }
  }

  return kept.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

/** Drop repeated paragraphs/lines (common when thought + answer overlap). */
export function dedupeRepeatedBlocks(text: string): string {
  const paragraphs = text.split(/\n{2,}/);
  const seen = new Set<string>();
  const uniqueParagraphs: string[] = [];

  for (const paragraph of paragraphs) {
    const trimmed = paragraph.trim();
    if (!trimmed) continue;
    const key = trimmed.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    uniqueParagraphs.push(trimmed);
  }

  if (uniqueParagraphs.length > 1) {
    return uniqueParagraphs.join("\n\n");
  }

  const lines = text.split("\n");
  const seenLines = new Set<string>();
  const uniqueLines: string[] = [];
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) {
      if (uniqueLines.length > 0 && uniqueLines[uniqueLines.length - 1] !== "") {
        uniqueLines.push("");
      }
      continue;
    }
    const key = trimmed.toLowerCase();
    if (seenLines.has(key)) continue;
    seenLines.add(key);
    uniqueLines.push(line);
  }
  return uniqueLines.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

function combinedThoughtText(activities: ChatActivity[]): string {
  return activities
    .filter((activity) => activity.kind === "thought")
    .map((activity) => activity.text?.trim() ?? "")
    .filter(Boolean)
    .join("\n\n");
}

/** Strip answer prefix that duplicates streamed thought content. */
export function stripLeadingThoughtOverlap(
  text: string,
  activities: ChatActivity[],
): string {
  const thought = combinedThoughtText(activities);
  if (!thought) return text;

  let result = text;
  if (result.startsWith(thought)) {
    result = result.slice(thought.length).trim();
  }

  const thoughtParagraphs = thought.split(/\n{2,}/).map((part) => part.trim());
  for (const paragraph of thoughtParagraphs) {
    if (paragraph.length < 24) continue;
    if (result.startsWith(paragraph)) {
      result = result.slice(paragraph.length).trim();
    }
  }
  return result;
}

/** Compose user-visible assistant answer from raw stream + activity trace. */
export function composeAssistantAnswer(
  raw: string,
  activities: ChatActivity[] = [],
): string {
  let text = raw.trim();
  if (!text) return text;

  text = stripEnglishTranslationBlocks(text);
  text = stripLeadingThoughtOverlap(text, activities);
  text = dedupeRepeatedBlocks(text);
  text = stripEnglishTranslationBlocks(text);
  return text.trim();
}
