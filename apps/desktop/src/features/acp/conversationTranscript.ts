import { createTurn } from "@lumina/chat-ui/chatTurns";

import type { ChatTurn } from "./types";
import type { LoadedTranscriptEvent } from "./api";

const TOOL_PRIORITY_MARKER = "【工具优先】";
const PLAYBACK_MARKER = "【当前播放】";

const PLAYBACK_DETAIL_PREFIXES = [
  "媒体：",
  "媒体:",
  "进度：",
  "进度:",
  "时长：",
  "时长:",
  "集数：",
  "集数:",
  "季数：",
  "季数:",
  "字幕轨道：",
  "字幕轨道:",
  "本集标题：",
  "本集标题:",
  "本集剧情：",
  "本集剧情:",
];

/** Strip scaffold we inject into session/prompt (tool header, playback block, resource_link echo). */
export function stripTranscriptScaffolding(text: string): string {
  if (!text) return "";
  const lines = text.split("\n");
  const kept: string[] = [];
  let inPlayback = false;
  for (const raw of lines) {
    const trimmed = raw.trim();
    if (!trimmed) {
      if (inPlayback) continue;
      kept.push("");
      continue;
    }
    if (trimmed.startsWith(TOOL_PRIORITY_MARKER)) {
      inPlayback = false;
      continue;
    }
    if (trimmed.startsWith(PLAYBACK_MARKER)) {
      inPlayback = true;
      continue;
    }
    if (inPlayback) {
      if (
        PLAYBACK_DETAIL_PREFIXES.some((prefix) => trimmed.startsWith(prefix))
      ) {
        continue;
      }
      inPlayback = false;
    }
    if (/resource_link/i.test(trimmed)) continue;
    if (/^file:\/\/\S+$/.test(trimmed)) continue;
    kept.push(raw);
  }
  return kept.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

/**
 * Map a loaded Agent thread transcript to chat turns.
 *
 * User scaffolding is stripped; agent bodies are kept verbatim (including any
 * Natural English prefix, which belongs to the user's own Codex config).
 * Tool messages are dropped. Empty mapping returns [] for the caller to keep
 * local turns.
 */
export function mapLoadedTranscript(
  events: LoadedTranscriptEvent[] | null | undefined,
): ChatTurn[] {
  if (!Array.isArray(events) || events.length === 0) return [];
  const seq = { n: 0 };
  const turns: ChatTurn[] = [];
  let current: ChatTurn | null = null;
  for (const event of events) {
    if (!event) continue;
    if (event.role === "user") {
      const cleaned = stripTranscriptScaffolding(event.text ?? "");
      if (!cleaned) continue;
      const turn = createTurn(seq, cleaned);
      turn.status = "done";
      turn.showActivities = false;
      turns.push(turn);
      current = turn;
      continue;
    }
    if (event.role === "agent") {
      const text = (event.text ?? "").trim();
      if (!text) continue;
      if (!current) {
        const turn = createTurn(seq, "");
        turn.status = "done";
        turn.showActivities = false;
        turn.answer = text;
        turns.push(turn);
        current = turn;
        continue;
      }
      current.answer = current.answer ? `${current.answer}\n\n${text}` : text;
      current.status = "done";
      continue;
    }
    // tool role: dropped (no user-visible transcript row).
  }
  return turns.filter(
    (turn) => turn.userText.trim() || turn.answer.trim(),
  );
}
