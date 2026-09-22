//! Tauri boundary for durable chat snapshots and ACP session hints.
//!
//! B1 covers the Rust side only: the commands below persist caller-pruned
//! snapshots into the shared `lumina.sqlite3` (migration 4) through
//! [`lumina_library::Database`]. No new database file, no frontend changes.

use std::path::PathBuf;

use lumina_library::{AcpSessionHintRecord, ChatSnapshotRecord, Database, DatabaseError};
use serde::{Deserialize, Serialize};

/// Stable business error shape for the six chat-store commands.
///
/// The UI only renders `message`; `details` carries diagnostics for logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStoreError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl ChatStoreError {
    fn storage(error: DatabaseError) -> Self {
        tracing::warn!(
            code = ?error.code,
            details = ?error.details,
            "chat store storage operation failed"
        );
        if error.code == lumina_library::DatabaseErrorCode::InvalidInput {
            return Self {
                code: "InvalidInput".to_string(),
                message: error.message,
                details: None,
            };
        }
        Self {
            code: "StorageError".to_string(),
            message: "聊天记录暂时不可用，请重试".to_string(),
            details: error.details,
        }
    }

    fn internal(details: impl Into<String>) -> Self {
        let details = details.into();
        tracing::error!(details = %details, "chat store command failed");
        Self {
            code: "InternalError".to_string(),
            message: "内部错误，请重试".to_string(),
            details: Some(details),
        }
    }
}

/// Caller-pruned snapshot payload. `sessionId` defaults to `""` to match the
/// `chat_snapshots` composite primary key; `turnsJson` must already be pruned
/// by the caller and arrive as a JSON array string.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSnapshotUpsertInput {
    pub profile_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: Option<String>,
    pub draft: String,
    pub turns_json: String,
}

/// Composite key for one profile/session snapshot.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSnapshotKeyInput {
    pub profile_id: String,
    #[serde(default)]
    pub session_id: String,
}

/// Resume-hint payload. Each profile keeps a single row; a newer session
/// overwrites the previous hint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHintUpsertInput {
    pub profile_id: String,
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
}

/// Key for the single resume hint owned by one profile.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHintKeyInput {
    pub profile_id: String,
}

/// Durable snapshot projection returned to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSnapshotDto {
    pub profile_id: String,
    pub session_id: String,
    pub cwd: Option<String>,
    pub draft: String,
    pub turns_json: String,
    pub updated_at_ms: i64,
}

/// Resume-hint projection returned to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionHintDto {
    pub profile_id: String,
    pub session_id: String,
    pub cwd: String,
    pub updated_at_ms: i64,
}

#[tauri::command]
pub async fn chat_snapshot_upsert(
    input: ChatSnapshotUpsertInput,
) -> Result<ChatSnapshotDto, ChatStoreError> {
    tauri::async_runtime::spawn_blocking(move || upsert_snapshot(input))
        .await
        .map_err(|error| ChatStoreError::internal(format!("chat snapshot upsert join: {error}")))?
}

#[tauri::command]
pub async fn chat_snapshot_get(
    input: ChatSnapshotKeyInput,
) -> Result<Option<ChatSnapshotDto>, ChatStoreError> {
    tauri::async_runtime::spawn_blocking(move || load_snapshot(input))
        .await
        .map_err(|error| ChatStoreError::internal(format!("chat snapshot get join: {error}")))?
}

#[tauri::command]
pub async fn chat_snapshot_delete(input: ChatSnapshotKeyInput) -> Result<bool, ChatStoreError> {
    tauri::async_runtime::spawn_blocking(move || delete_snapshot(input))
        .await
        .map_err(|error| ChatStoreError::internal(format!("chat snapshot delete join: {error}")))?
}

#[tauri::command]
pub async fn chat_hint_upsert(
    input: ChatHintUpsertInput,
) -> Result<ChatSessionHintDto, ChatStoreError> {
    tauri::async_runtime::spawn_blocking(move || upsert_hint(input))
        .await
        .map_err(|error| ChatStoreError::internal(format!("chat hint upsert join: {error}")))?
}

#[tauri::command]
pub async fn chat_hint_get(
    input: ChatHintKeyInput,
) -> Result<Option<ChatSessionHintDto>, ChatStoreError> {
    tauri::async_runtime::spawn_blocking(move || load_hint(input))
        .await
        .map_err(|error| ChatStoreError::internal(format!("chat hint get join: {error}")))?
}

#[tauri::command]
pub async fn chat_hint_delete(input: ChatHintKeyInput) -> Result<bool, ChatStoreError> {
    tauri::async_runtime::spawn_blocking(move || delete_hint(input))
        .await
        .map_err(|error| ChatStoreError::internal(format!("chat hint delete join: {error}")))?
}

fn upsert_snapshot(input: ChatSnapshotUpsertInput) -> Result<ChatSnapshotDto, ChatStoreError> {
    let database = open_database()?;
    database
        .snapshot_upsert(
            &input.profile_id,
            &input.session_id,
            input.cwd.as_deref(),
            &input.draft,
            &input.turns_json,
        )
        .map(snapshot_dto)
        .map_err(ChatStoreError::storage)
}

fn load_snapshot(input: ChatSnapshotKeyInput) -> Result<Option<ChatSnapshotDto>, ChatStoreError> {
    let database = open_database()?;
    database
        .snapshot_get(&input.profile_id, &input.session_id)
        .map(|snapshot| snapshot.map(snapshot_dto))
        .map_err(ChatStoreError::storage)
}

fn delete_snapshot(input: ChatSnapshotKeyInput) -> Result<bool, ChatStoreError> {
    let database = open_database()?;
    database
        .snapshot_delete(&input.profile_id, &input.session_id)
        .map_err(ChatStoreError::storage)
}

fn upsert_hint(input: ChatHintUpsertInput) -> Result<ChatSessionHintDto, ChatStoreError> {
    let database = open_database()?;
    database
        .hint_upsert(&input.profile_id, &input.session_id, &input.cwd)
        .map(hint_dto)
        .map_err(ChatStoreError::storage)
}

fn load_hint(input: ChatHintKeyInput) -> Result<Option<ChatSessionHintDto>, ChatStoreError> {
    let database = open_database()?;
    database
        .hint_get(&input.profile_id)
        .map(|hint| hint.map(hint_dto))
        .map_err(ChatStoreError::storage)
}

fn delete_hint(input: ChatHintKeyInput) -> Result<bool, ChatStoreError> {
    let database = open_database()?;
    database
        .hint_delete(&input.profile_id)
        .map_err(ChatStoreError::storage)
}

fn snapshot_dto(record: ChatSnapshotRecord) -> ChatSnapshotDto {
    ChatSnapshotDto {
        profile_id: record.profile_id,
        session_id: record.session_id,
        cwd: record.cwd,
        draft: record.draft,
        turns_json: record.turns_json,
        updated_at_ms: record.updated_at_ms,
    }
}

fn hint_dto(record: AcpSessionHintRecord) -> ChatSessionHintDto {
    ChatSessionHintDto {
        profile_id: record.profile_id,
        session_id: record.session_id,
        cwd: record.cwd,
        updated_at_ms: record.updated_at_ms,
    }
}

fn database_path() -> Result<PathBuf, ChatStoreError> {
    let base = super::system::data_dir()
        .ok_or_else(|| ChatStoreError::internal("application data directory unavailable"))?;
    Ok(base.join("lumina").join("lumina.sqlite3"))
}

fn open_database() -> Result<Database, ChatStoreError> {
    let path = database_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            ChatStoreError::internal(format!("create chat store data directory: {error}"))
        })?;
    }
    Database::open(path).map_err(ChatStoreError::storage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_and_outputs_use_camel_case() {
        let input: ChatSnapshotUpsertInput = match serde_json::from_str(
            r#"{"profileId":"codex","sessionId":"s1","cwd":"D:/work","draft":"草稿","turnsJson":"[]"}"#,
        ) {
            Ok(input) => input,
            Err(error) => panic!("upsert input should deserialize: {error}"),
        };
        assert_eq!(input.profile_id, "codex");
        assert_eq!(input.session_id, "s1");
        assert_eq!(input.cwd.as_deref(), Some("D:/work"));

        let key: ChatSnapshotKeyInput =
            match serde_json::from_str(r#"{"profileId":"codex","sessionId":"s1"}"#) {
                Ok(key) => key,
                Err(error) => panic!("key input should deserialize: {error}"),
            };
        assert_eq!(key.profile_id, "codex");

        let hint: ChatHintUpsertInput =
            match serde_json::from_str(r#"{"profileId":"codex","sessionId":"s1","cwd":"D:/work"}"#)
            {
                Ok(hint) => hint,
                Err(error) => panic!("hint input should deserialize: {error}"),
            };
        assert_eq!(hint.session_id, "s1");

        let dto = ChatSnapshotDto {
            profile_id: "codex".to_string(),
            session_id: "s1".to_string(),
            cwd: None,
            draft: "草稿".to_string(),
            turns_json: "[]".to_string(),
            updated_at_ms: 7,
        };
        let value = match serde_json::to_value(&dto) {
            Ok(value) => value,
            Err(error) => panic!("snapshot dto should serialize: {error}"),
        };
        assert_eq!(
            value.get("profileId").and_then(|v| v.as_str()),
            Some("codex")
        );
        assert_eq!(value.get("turnsJson").and_then(|v| v.as_str()), Some("[]"));
        assert_eq!(value.get("updatedAtMs").and_then(|v| v.as_i64()), Some(7));

        let hint_dto_value = match serde_json::to_value(ChatSessionHintDto {
            profile_id: "codex".to_string(),
            session_id: "s1".to_string(),
            cwd: String::new(),
            updated_at_ms: 9,
        }) {
            Ok(value) => value,
            Err(error) => panic!("hint dto should serialize: {error}"),
        };
        assert_eq!(
            hint_dto_value.get("sessionId").and_then(|v| v.as_str()),
            Some("s1")
        );
        assert!(hint_dto_value.get("session_id").is_none());
        assert!(hint_dto_value.get("session-id").is_none());
    }

    #[test]
    fn storage_error_mapping_keeps_business_message_and_hides_diagnostics() {
        let invalid = ChatStoreError::storage(DatabaseError {
            code: lumina_library::DatabaseErrorCode::InvalidInput,
            message: "聊天草稿不能为空".to_string(),
            details: None,
        });
        assert_eq!(invalid.code, "InvalidInput");
        assert_eq!(invalid.message, "聊天草稿不能为空");
        assert_eq!(invalid.details, None);

        let storage = ChatStoreError::storage(DatabaseError {
            code: lumina_library::DatabaseErrorCode::OpenFailed,
            message: "无法打开应用数据存储".to_string(),
            details: Some("open C:\\private\\lumina.sqlite3: sqlite detail".to_string()),
        });
        assert_eq!(storage.code, "StorageError");
        assert_eq!(storage.message, "聊天记录暂时不可用，请重试");
        assert!(!storage.message.contains("sqlite"));
        assert!(!storage.message.contains("C:\\"));
        let value = match serde_json::to_value(&storage) {
            Ok(value) => value,
            Err(error) => panic!("error should serialize: {error}"),
        };
        assert_eq!(
            value.get("code").and_then(|v| v.as_str()),
            Some("StorageError")
        );
        assert!(value.get("details").is_some());
    }
}
