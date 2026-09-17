import { createTurn } from "@lumina/chat-ui/chatTurns";

import type { ChatActivity, ChatTurn } from "./types";
import type { LoadedTranscriptEvent } from "./api";

const TOOL_PRIORITY_MARKER = "【工具优先】";
const PLAYBACK_MARKER = "【当前播放】";
const TOOL_ID_PATTERN = /lumina_[a-z_]+/;
const MARKDOWN_FILE_LINK_PATTERN = /\[[^\]]*\]\(file:\/\/[^)]*\)/g;

/**
 * 回放的结构回声（两角色通用）：resource_link 被渲染成的 markdown 文件
 * 链接原文、缀在行尾的行内标记。模型正文绝不可能长这样（模型不吐
 * file:// 链接、不裸发【当前播放】标记），所以两边都可删；用户配置的
 * Natural English 前缀不在此列，原样保留。
 */
function stripStructuralEchoes(text: string): string {
  return text
    .replace(MARKDOWN_FILE_LINK_PATTERN, "")
    .split(TOOL_PRIORITY_MARKER)
    .join("")
    .split(PLAYBACK_MARKER)
    .join("");
}

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

/**
 * Strip scaffold we inject into session/prompt (tool header, playback block,
 * resource_link echo).
 *
 * Deliberately stateless per line: history replay (`session/load`) streams
 * turns in arbitrary chunkings — the `【当前播放】` marker and its detail
 * lines can arrive in separate chunks, so a state machine keyed on “marker
 * seen” keeps the details whenever the marker lands in another chunk.
 * Load-path only; live prompts never pass through here, so a user literally
 * typing `进度：…` in history view losing that line is the accepted trade.
 */
export function stripTranscriptScaffolding(text: string): string {
  if (!text) return "";
  const kept: string[] = [];
  for (const raw of text.split("\n")) {
    const trimmed = raw.trim();
    if (!trimmed) continue;
    // 行级标记先判：必须在行内 token 清除之前，否则
    // “【工具优先】xxx”被先掏成“xxx”就漏网了。
    if (trimmed.startsWith(TOOL_PRIORITY_MARKER)) continue;
    if (trimmed.startsWith(PLAYBACK_MARKER)) continue;
    if (
      PLAYBACK_DETAIL_PREFIXES.some((prefix) => trimmed.startsWith(prefix))
    ) {
      continue;
    }
    // 回放把触发头长段换行后，续行（tool id 清单）不再以标记开头。
    // load 路径的用户文本里出现 tool id 只可能是脚手架。
    if (TOOL_ID_PATTERN.test(trimmed)) continue;
    if (/resource_link/i.test(trimmed)) continue;
    if (/^file:\/\/\S+$/.test(trimmed)) continue;
    // 行内残留：markdown 文件链接原文、缀在行尾的标记。
    // 只清回声，其余空白原样保留（代码块缩进不能动）。
    const cleaned = stripStructuralEchoes(raw);
    if (!cleaned.trim()) continue;
    kept.push(cleaned);
  }
  return kept.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

/**
 * Agent 侧只去结构回声（见 stripStructuralEchoes），其余一字不动：
 * 用户 Codex 配置的前缀、模型的英文规划小节都属于正文。
 * 纯回声的 agent 事件会被调用方判空跳过。
 */
export function stripAgentEchoes(text: string): string {
  if (!text) return "";
  const kept: string[] = [];
  for (const raw of text.split("\n")) {
    // 原有空行保留（段落结构影响 markdown 渲染）。
    if (raw.trim().length === 0) {
      kept.push(raw);
      continue;
    }
    // 纯回声行删掉，其余只清行内回声、正文一字不动。
    const cleaned = stripStructuralEchoes(raw);
    if (cleaned.trim().length === 0) continue;
    kept.push(cleaned);
  }
  return kept.join("\n").trim();
}

/**
 * Map a loaded Agent thread transcript to chat turns.
 *
 * User scaffolding is stripped; agent bodies are kept verbatim except
 * structural replay echoes (markdown file links, bare markers — never model
 * prose, including the user's own Codex config prefixes).
 * Tool events become done activities on their turn (same section as live).
 * Empty mapping returns [] for the caller to keep local turns.
 */
export function mapLoadedTranscript(
  events: LoadedTranscriptEvent[] | null | undefined,
): ChatTurn[] {
  if (!Array.isArray(events) || events.length === 0) return [];
  const seq = { n: 0 };
  let toolSeq = 0;
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
      const text = stripAgentEchoes(event.text ?? "");
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
    // tool role: restored as a done activity on the current turn, so the
    // restored history shows the same tool section (and counter) as live
    // turns. Consecutive same-title chunks share one call id (replay lost
    // the real ids; counting chunks would inflate the counter).
    // Orphan tool events open a holder that a later agent chunk fills;
    // never-filled holders are dropped by the filter below.
    const toolText = (event.text ?? "").trim();
    if (!toolText) continue;
    let holder: ChatTurn | null = current;
    if (!holder) {
      holder = createTurn(seq, "");
      holder.status = "done";
      holder.showActivities = false;
      turns.push(holder);
      current = holder;
    }
    const prevTool = [...holder.activities]
      .reverse()
      .find((item) => item.kind === "tool");
    const toolCallId =
      prevTool?.title === toolText && prevTool?.toolCallId
        ? prevTool.toolCallId
        : `loaded-call-${toolSeq}`;
    const activity: ChatActivity = {
      id: `loaded-tool-${toolSeq++}`,
      kind: "tool",
      toolCallId,
      title: toolText.slice(0, 120),
      status: "completed",
    };
    holder.activities.push(activity);
    holder.showActivities = true;
    continue;
  }
  return turns.filter(
    (turn) => turn.userText.trim() || turn.answer.trim(),
  );
}
