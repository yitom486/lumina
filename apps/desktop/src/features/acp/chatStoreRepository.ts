import { invoke } from "@tauri-apps/api/core";

/**
 * SQLite 快照仓薄封装（B3 SQLite only 前端层）。
 *
 * 对齐 Rust 侧 `apps/desktop/src-tauri/src/commands/chat_store.rs` 的 6 个
 * Tauri Command（camelCase serde）：chat_snapshot_upsert/get/delete、
 * chat_hint_upsert/get/delete。表由 `lumina.sqlite3` migration 4 提供
 * （`chat_snapshots` / `acp_session_hints`），本文件不建表、不迁移。
 *
 * 约定：
 * - 只透传业务错 `{ code, message, details? }`，绝不拼接“详情：”+ details；
 *   调用方统一走 `errorMessage` / `formatPlayerError` 展示；
 * - 失败抛给调用方，由调用方记日志 + 保留内存 + 下次 trailing 重试，
 *   本层不吞错、不落 localStorage（备层由 `chatRestore` 负责）。
 */

export type ChatSnapshotDto = {
  profileId: string;
  sessionId: string;
  cwd: string | null;
  draft: string;
  turnsJson: string;
  updatedAtMs: number;
};

export type ChatSessionHintDto = {
  profileId: string;
  sessionId: string;
  cwd: string;
  updatedAtMs: number;
};

export type ChatSnapshotUpsertInput = {
  profileId: string;
  sessionId: string;
  cwd: string | null;
  draft: string;
  turnsJson: string;
};

export type ChatSnapshotKeyInput = {
  profileId: string;
  sessionId: string;
};

export type ChatHintUpsertInput = {
  profileId: string;
  sessionId: string;
  cwd: string;
};

export type ChatHintKeyInput = {
  profileId: string;
};

export function chatSnapshotUpsert(
  input: ChatSnapshotUpsertInput,
): Promise<ChatSnapshotDto> {
  return invoke<ChatSnapshotDto>("chat_snapshot_upsert", { input });
}

export function chatSnapshotGet(
  key: ChatSnapshotKeyInput,
): Promise<ChatSnapshotDto | null> {
  return invoke<ChatSnapshotDto | null>("chat_snapshot_get", { input: key });
}

export function chatSnapshotDelete(
  key: ChatSnapshotKeyInput,
): Promise<boolean> {
  return invoke<boolean>("chat_snapshot_delete", { input: key });
}

export function chatHintUpsert(
  input: ChatHintUpsertInput,
): Promise<ChatSessionHintDto> {
  return invoke<ChatSessionHintDto>("chat_hint_upsert", { input });
}

export function chatHintGet(
  key: ChatHintKeyInput,
): Promise<ChatSessionHintDto | null> {
  return invoke<ChatSessionHintDto | null>("chat_hint_get", { input: key });
}

export function chatHintDelete(key: ChatHintKeyInput): Promise<boolean> {
  return invoke<boolean>("chat_hint_delete", { input: key });
}
