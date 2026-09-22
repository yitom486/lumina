//! Application-owned SQLite storage for chapter, agent, question, and watch-feed data.
//!
//! This module deliberately stays below the Tauri and UI layers.  Callers own
//! the database path, while this module owns connection setup, schema
//! migrations, and the small repository surface needed by the first data
//! pipeline migration.

use std::fmt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{GroupResolution, LibraryIndex, MediaGroupKind, MetadataMediaType};

/// The latest migration included in this crate.
pub const CURRENT_SCHEMA_VERSION: u32 = 4;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Stable business error categories for the SQLite boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseErrorCode {
    OpenFailed,
    MigrationFailed,
    QueryFailed,
    ConstraintViolation,
    InvalidInput,
    UnsupportedSchema,
}

/// Error returned by the SQLite foundation.
///
/// The user-facing message never contains a path, SQL statement, or SQLite
/// diagnostic.  Technical context is retained in `details` for logging and
/// diagnostics at a higher layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseError {
    pub code: DatabaseErrorCode,
    pub message: String,
    pub details: Option<String>,
}

pub type DatabaseResult<T> = Result<T, DatabaseError>;

impl DatabaseError {
    fn new(code: DatabaseErrorCode, message: &'static str, details: Option<String>) -> Self {
        Self {
            code,
            message: message.to_string(),
            details,
        }
    }

    fn sqlite(
        code: DatabaseErrorCode,
        message: &'static str,
        operation: &str,
        error: rusqlite::Error,
    ) -> Self {
        Self::new(code, message, Some(format!("{operation}: {error}")))
    }

    fn invalid_input(message: &'static str) -> Self {
        Self::new(DatabaseErrorCode::InvalidInput, message, None)
    }

    fn max_attempts_exceeded() -> Self {
        Self::invalid_input("任务已达到最大尝试次数，无法继续执行")
    }

    fn unsupported_schema(version: u32) -> Self {
        Self::new(
            DatabaseErrorCode::UnsupportedSchema,
            "数据库版本不受支持，请升级应用",
            Some(format!("schema version {version}")),
        )
    }
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DatabaseError {}

/// A configured SQLite connection with the current schema applied.
pub struct Database {
    connection: Connection,
}

impl Database {
    /// Open or create a file-backed database and apply all pending migrations.
    pub fn open(path: impl AsRef<Path>) -> DatabaseResult<Self> {
        let path_ref = path.as_ref();
        let connection = Connection::open(path_ref).map_err(|error| {
            DatabaseError::sqlite(
                DatabaseErrorCode::OpenFailed,
                "无法打开应用数据存储",
                &format!("open {}", path_ref.display()),
                error,
            )
        })?;
        let mut database = Self { connection };
        database.configure(false)?;
        database.migrate()?;
        Ok(database)
    }

    /// Open an isolated in-memory database, useful for tests and short-lived
    /// application work.
    pub fn open_in_memory() -> DatabaseResult<Self> {
        let connection = Connection::open_in_memory().map_err(|error| {
            DatabaseError::sqlite(
                DatabaseErrorCode::OpenFailed,
                "无法打开应用数据存储",
                "open in-memory database",
                error,
            )
        })?;
        let mut database = Self { connection };
        database.configure(true)?;
        database.migrate()?;
        Ok(database)
    }

    /// Apply migrations again.  Every migration is idempotent, so callers may
    /// safely invoke this during startup or recovery.
    pub fn migrate(&mut self) -> DatabaseResult<()> {
        let transaction = self.connection.transaction().map_err(|error| {
            DatabaseError::sqlite(
                DatabaseErrorCode::MigrationFailed,
                "应用数据存储初始化失败，请重试",
                "begin migration transaction",
                error,
            )
        })?;

        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at_ms INTEGER NOT NULL
                );",
            )
            .map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::MigrationFailed,
                    "应用数据存储初始化失败，请重试",
                    "create migration table",
                    error,
                )
            })?;

        let current_version = transaction
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::MigrationFailed,
                    "应用数据存储初始化失败，请重试",
                    "read schema version",
                    error,
                )
            })?;
        let current_version = u32::try_from(current_version)
            .map_err(|_| DatabaseError::unsupported_schema(current_version.max(0) as u32))?;

        if current_version > CURRENT_SCHEMA_VERSION {
            return Err(DatabaseError::unsupported_schema(current_version));
        }

        if current_version < 1 {
            transaction
                .execute_batch(MIGRATION_1_SQL)
                .map_err(|error| {
                    DatabaseError::sqlite(
                        DatabaseErrorCode::MigrationFailed,
                        "应用数据存储初始化失败，请重试",
                        "apply initial schema",
                        error,
                    )
                })?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO schema_migrations(version, applied_at_ms)
                     VALUES (?1, ?2)",
                    params![1_i64, now_ms()],
                )
                .map_err(|error| {
                    DatabaseError::sqlite(
                        DatabaseErrorCode::MigrationFailed,
                        "应用数据存储初始化失败，请重试",
                        "record initial schema",
                        error,
                    )
                })?;
        }

        if current_version < 2 {
            transaction
                .execute_batch("ALTER TABLE agent_tasks ADD COLUMN output_json TEXT;")
                .map_err(|error| {
                    DatabaseError::sqlite(
                        DatabaseErrorCode::MigrationFailed,
                        "应用数据存储初始化失败，请重试",
                        "add agent task output column",
                        error,
                    )
                })?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO schema_migrations(version, applied_at_ms)
                     VALUES (?1, ?2)",
                    params![2_i64, now_ms()],
                )
                .map_err(|error| {
                    DatabaseError::sqlite(
                        DatabaseErrorCode::MigrationFailed,
                        "应用数据存储初始化失败，请重试",
                        "record output migration",
                        error,
                    )
                })?;
        }

        if current_version < 3 {
            transaction
                .execute_batch(MIGRATION_3_SQL)
                .map_err(|error| {
                    DatabaseError::sqlite(
                        DatabaseErrorCode::MigrationFailed,
                        "应用数据存储初始化失败，请重试",
                        "add chapter task scope migration",
                        error,
                    )
                })?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO schema_migrations(version, applied_at_ms)
                     VALUES (?1, ?2)",
                    params![3_i64, now_ms()],
                )
                .map_err(|error| {
                    DatabaseError::sqlite(
                        DatabaseErrorCode::MigrationFailed,
                        "应用数据存储初始化失败，请重试",
                        "record chapter task scope migration",
                        error,
                    )
                })?;
        }

        if current_version < 4 {
            transaction
                .execute_batch(MIGRATION_4_SQL)
                .map_err(|error| {
                    DatabaseError::sqlite(
                        DatabaseErrorCode::MigrationFailed,
                        "应用数据存储初始化失败，请重试",
                        "add chat snapshot storage migration",
                        error,
                    )
                })?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO schema_migrations(version, applied_at_ms)
                     VALUES (?1, ?2)",
                    params![4_i64, now_ms()],
                )
                .map_err(|error| {
                    DatabaseError::sqlite(
                        DatabaseErrorCode::MigrationFailed,
                        "应用数据存储初始化失败，请重试",
                        "record chat snapshot storage migration",
                        error,
                    )
                })?;
        }

        transaction.commit().map_err(|error| {
            DatabaseError::sqlite(
                DatabaseErrorCode::MigrationFailed,
                "应用数据存储初始化失败，请重试",
                "commit migrations",
                error,
            )
        })
    }

    pub fn schema_version(&self) -> DatabaseResult<u32> {
        let version = self
            .connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::QueryFailed,
                    "应用数据存储读取失败，请重试",
                    "read schema version",
                    error,
                )
            })?;
        u32::try_from(version).map_err(|_| DatabaseError::unsupported_schema(version.max(0) as u32))
    }

    /// Return whether a named table exists.  This is intentionally small and
    /// read-only, and is useful for diagnostics and migration verification.
    pub fn table_exists(&self, table_name: &str) -> DatabaseResult<bool> {
        self.connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
                )",
                params![table_name],
                |row| row.get(0),
            )
            .map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::QueryFailed,
                    "应用数据存储读取失败，请重试",
                    "check table existence",
                    error,
                )
            })
    }

    pub fn repository(&self) -> Repository<'_> {
        Repository {
            connection: &self.connection,
        }
    }

    /// Resolve the stable identity represented by an indexed media file.
    ///
    /// Only an authoritative TV match and an explicit season/episode pair are
    /// sufficient to cross the SQLite boundary.  Pending/ignored groups and
    /// files without a complete season/episode address return `None` instead
    /// of deriving a series from a path or guessing missing metadata.
    pub fn episode_identity_for_media(
        &self,
        index: &LibraryIndex,
        media_path: &Path,
    ) -> DatabaseResult<Option<(String, String)>> {
        episode_identity_for_media(index, media_path)
    }

    /// Find an existing episode through the library identity seam.
    ///
    /// This is deliberately read-only.  Creation remains a separate explicit
    /// operation so a caller cannot accidentally persist guessed series data.
    pub fn find_episode_for_media(
        &self,
        index: &LibraryIndex,
        media_path: &Path,
    ) -> DatabaseResult<Option<EpisodeRecord>> {
        let Some((series_stable_id, episode_stable_id)) =
            self.episode_identity_for_media(index, media_path)?
        else {
            return Ok(None);
        };
        self.repository()
            .get_episode_by_identity(&series_stable_id, &episode_stable_id)
    }

    /// Run repository operations atomically.  Any returned error causes the
    /// transaction to be rolled back before the error is returned.
    pub fn transaction<T, F>(&mut self, operation: F) -> DatabaseResult<T>
    where
        F: FnOnce(&Repository<'_>) -> DatabaseResult<T>,
    {
        let transaction = self.connection.transaction().map_err(|error| {
            DatabaseError::sqlite(
                DatabaseErrorCode::QueryFailed,
                "应用数据存储写入失败，请重试",
                "begin transaction",
                error,
            )
        })?;
        let result = {
            let repository = Repository {
                connection: &transaction,
            };
            operation(&repository)
        };

        match result {
            Ok(value) => transaction.commit().map(|()| value).map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::QueryFailed,
                    "应用数据存储写入失败，请重试",
                    "commit transaction",
                    error,
                )
            }),
            Err(error) => {
                // Dropping the transaction also rolls it back.  Keeping the
                // original domain error is more useful to the caller.
                drop(transaction);
                Err(error)
            }
        }
    }

    /// Migrate legacy path-based episode projections to an already-created
    /// authoritative episode in one SQLite transaction.
    ///
    /// The migration is deliberately conservative.  It never deletes the
    /// legacy episode, never invents task keys, and returns a conflict report
    /// without changing rows when a chapter identity, watch position, or
    /// requested task-key rename cannot be proven safe.
    pub fn migrate_legacy_episode(
        &mut self,
        input: &LegacyEpisodeMigration,
    ) -> DatabaseResult<EpisodeMigrationReport> {
        validate_episode_migration(input)?;
        self.transaction(|repository| repository.migrate_legacy_episode(input))
    }

    /// Reconcile tasks that were running when the application last exited.
    ///
    /// A desktop process can disappear between an ACP prompt and the final
    /// result write.  These tasks must become explicitly retryable (or
    /// terminal when their attempt budget is exhausted) before the next
    /// worker can claim them; leaving them in `running` would strand them
    /// forever.
    pub fn recover_interrupted_agent_tasks(&mut self) -> DatabaseResult<usize> {
        const RECOVERY_REPORT: &str = "应用关闭时章节任务中断，可重新执行";
        let transaction = self.connection.transaction().map_err(|error| {
            DatabaseError::sqlite(
                DatabaseErrorCode::QueryFailed,
                "应用数据存储写入失败，请重试",
                "begin interrupted task recovery",
                error,
            )
        })?;
        transaction
            .execute(
                "UPDATE agent_attempts
                 SET status = 'interrupted', validation_report = ?1, finished_at_ms = ?2
                 WHERE status = 'running'
                   AND task_id IN (
                       SELECT id FROM agent_tasks
                       WHERE status = 'running' AND task_type = 'chapter_segmentation'
                   )",
                params![RECOVERY_REPORT, now_ms()],
            )
            .map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::QueryFailed,
                    "应用数据存储写入失败，请重试",
                    "recover running attempts",
                    error,
                )
            })?;
        let retryable = transaction
            .execute(
                "UPDATE agent_tasks
                 SET status = 'validation_failure', validation_report = ?1, updated_at_ms = ?2
                 WHERE status = 'running' AND task_type = 'chapter_segmentation'
                   AND attempt_count < max_attempts",
                params![RECOVERY_REPORT, now_ms()],
            )
            .map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::QueryFailed,
                    "应用数据存储写入失败，请重试",
                    "recover retryable tasks",
                    error,
                )
            })?;
        let terminal = transaction
            .execute(
                "UPDATE agent_tasks
                 SET status = 'failed', validation_report = ?1, updated_at_ms = ?2
                 WHERE status = 'running' AND task_type = 'chapter_segmentation'
                   AND attempt_count >= max_attempts",
                params![RECOVERY_REPORT, now_ms()],
            )
            .map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::QueryFailed,
                    "应用数据存储写入失败，请重试",
                    "recover exhausted tasks",
                    error,
                )
            })?;
        transaction.commit().map_err(|error| {
            DatabaseError::sqlite(
                DatabaseErrorCode::QueryFailed,
                "应用数据存储写入失败，请重试",
                "commit interrupted task recovery",
                error,
            )
        })?;
        Ok(retryable + terminal)
    }

    /// Insert or replace the chat snapshot owned by one profile/session pair.
    ///
    /// The caller prunes `turns_json` before crossing this boundary; this
    /// method only guards empty and overlong payloads and rejects malformed
    /// JSON so a corrupt snapshot can never silently replace a good one.
    pub fn snapshot_upsert(
        &self,
        profile_id: &str,
        session_id: &str,
        cwd: Option<&str>,
        draft: &str,
        turns_json: &str,
    ) -> DatabaseResult<ChatSnapshotRecord> {
        validate_snapshot_identity(profile_id, session_id)?;
        validate_snapshot_payload(draft, turns_json)?;
        let cwd = normalize_optional_text(cwd);
        self.connection
            .execute(
                "INSERT INTO chat_snapshots(
                    profile_id, session_id, cwd, draft, turns_json, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(profile_id, session_id) DO UPDATE SET
                    cwd = excluded.cwd,
                    draft = excluded.draft,
                    turns_json = excluded.turns_json,
                    updated_at_ms = excluded.updated_at_ms",
                params![profile_id, session_id, cwd, draft, turns_json, now_ms()],
            )
            .map_err(|error| map_write_error("upsert chat snapshot", error))?;
        self.snapshot_get(profile_id, session_id)?.ok_or_else(|| {
            DatabaseError::new(
                DatabaseErrorCode::QueryFailed,
                "应用数据存储读取失败，请重试",
                Some("chat snapshot disappeared after upsert".to_string()),
            )
        })
    }

    /// Read the chat snapshot owned by one profile/session pair.
    pub fn snapshot_get(
        &self,
        profile_id: &str,
        session_id: &str,
    ) -> DatabaseResult<Option<ChatSnapshotRecord>> {
        validate_snapshot_identity(profile_id, session_id)?;
        self.connection
            .query_row(
                "SELECT profile_id, session_id, cwd, draft, turns_json, updated_at_ms
                 FROM chat_snapshots WHERE profile_id = ?1 AND session_id = ?2",
                params![profile_id, session_id],
                map_chat_snapshot_row,
            )
            .optional()
            .map_err(|error| map_read_error("read chat snapshot", error))
    }

    /// Delete the chat snapshot owned by one profile/session pair.
    pub fn snapshot_delete(&self, profile_id: &str, session_id: &str) -> DatabaseResult<bool> {
        validate_snapshot_identity(profile_id, session_id)?;
        let changed = self
            .connection
            .execute(
                "DELETE FROM chat_snapshots WHERE profile_id = ?1 AND session_id = ?2",
                params![profile_id, session_id],
            )
            .map_err(|error| map_write_error("delete chat snapshot", error))?;
        Ok(changed == 1)
    }

    /// Insert or replace the single resume hint owned by one profile.
    ///
    /// Each profile keeps at most one row; a newer session overwrites the
    /// previous hint instead of accumulating history.
    pub fn hint_upsert(
        &self,
        profile_id: &str,
        session_id: &str,
        cwd: &str,
    ) -> DatabaseResult<AcpSessionHintRecord> {
        require_text(profile_id, "Agent 配置标识不能为空")?;
        require_text(session_id, "Agent 会话标识不能为空")?;
        require_key_length(profile_id, "Agent 配置标识过长，无法保存")?;
        require_key_length(session_id, "Agent 会话标识过长，无法保存")?;
        require_cwd_length(cwd)?;
        self.connection
            .execute(
                "INSERT INTO acp_session_hints(
                    profile_id, session_id, cwd, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(profile_id) DO UPDATE SET
                    session_id = excluded.session_id,
                    cwd = excluded.cwd,
                    updated_at_ms = excluded.updated_at_ms",
                params![profile_id, session_id, cwd, now_ms()],
            )
            .map_err(|error| map_write_error("upsert session hint", error))?;
        self.hint_get(profile_id)?.ok_or_else(|| {
            DatabaseError::new(
                DatabaseErrorCode::QueryFailed,
                "应用数据存储读取失败，请重试",
                Some("session hint disappeared after upsert".to_string()),
            )
        })
    }

    /// Read the resume hint owned by one profile.
    pub fn hint_get(&self, profile_id: &str) -> DatabaseResult<Option<AcpSessionHintRecord>> {
        require_text(profile_id, "Agent 配置标识不能为空")?;
        self.connection
            .query_row(
                "SELECT profile_id, session_id, cwd, updated_at_ms
                 FROM acp_session_hints WHERE profile_id = ?1",
                params![profile_id],
                map_session_hint_row,
            )
            .optional()
            .map_err(|error| map_read_error("read session hint", error))
    }

    /// Delete the resume hint owned by one profile.
    pub fn hint_delete(&self, profile_id: &str) -> DatabaseResult<bool> {
        require_text(profile_id, "Agent 配置标识不能为空")?;
        let changed = self
            .connection
            .execute(
                "DELETE FROM acp_session_hints WHERE profile_id = ?1",
                params![profile_id],
            )
            .map_err(|error| map_write_error("delete session hint", error))?;
        Ok(changed == 1)
    }

    fn configure(&mut self, in_memory: bool) -> DatabaseResult<()> {
        self.connection
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::OpenFailed,
                    "无法初始化应用数据存储",
                    "set busy timeout",
                    error,
                )
            })?;
        self.connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::OpenFailed,
                    "无法初始化应用数据存储",
                    "enable foreign keys",
                    error,
                )
            })?;
        self.connection
            .pragma_update(
                None,
                "journal_mode",
                if in_memory { "MEMORY" } else { "WAL" },
            )
            .map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::OpenFailed,
                    "无法初始化应用数据存储",
                    "configure journal mode",
                    error,
                )
            })?;
        self.connection
            .pragma_update(None, "synchronous", "NORMAL")
            .map_err(|error| {
                DatabaseError::sqlite(
                    DatabaseErrorCode::OpenFailed,
                    "无法初始化应用数据存储",
                    "configure synchronous mode",
                    error,
                )
            })
    }
}

/// Repository methods operate on either a regular connection or the
/// connection borrowed from `Database::transaction`.
pub struct Repository<'connection> {
    connection: &'connection Connection,
}

/// A caller-provided rename for a legacy task key.
///
/// Task keys are globally unique and are also used by the worker as an
/// idempotency boundary.  The migration therefore never invents a new key;
/// a rename is performed only when the caller explicitly supplies it and the
/// target key is unused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskKeyMigration {
    pub task_id: i64,
    pub new_task_key: String,
}

/// Input for the conservative legacy episode migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyEpisodeMigration {
    pub legacy_episode_id: i64,
    pub authoritative_episode_id: i64,
    pub task_key_migrations: Vec<AgentTaskKeyMigration>,
}

impl LegacyEpisodeMigration {
    pub fn new(legacy_episode_id: i64, authoritative_episode_id: i64) -> Self {
        Self {
            legacy_episode_id,
            authoritative_episode_id,
            task_key_migrations: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpisodeMigrationStatus {
    LegacyMissing,
    AuthoritativeMissing,
    AlreadyConverged,
    Merged,
    MergedWithPreservedTasks,
    Conflict,
}

/// A safe, user-facing migration result.  Diagnostics intentionally contain
/// business reasons only; SQL, paths and SQLite details stay in
/// `DatabaseError::details` or tracing logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeMigrationReport {
    pub status: EpisodeMigrationStatus,
    pub legacy_episode_id: i64,
    pub authoritative_episode_id: i64,
    pub moved_chapters: u64,
    pub moved_questions: u64,
    pub moved_feed_items: u64,
    pub renamed_tasks: u64,
    pub preserved_task_keys: Vec<String>,
    pub diagnostics: Vec<String>,
}

impl EpisodeMigrationReport {
    fn new(input: &LegacyEpisodeMigration, status: EpisodeMigrationStatus) -> Self {
        Self {
            status,
            legacy_episode_id: input.legacy_episode_id,
            authoritative_episode_id: input.authoritative_episode_id,
            moved_chapters: 0,
            moved_questions: 0,
            moved_feed_items: 0,
            renamed_tasks: 0,
            preserved_task_keys: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

impl Repository<'_> {
    /// Execute the data-only part of [`Database::migrate_legacy_episode`].
    ///
    /// This method is also available to an existing worker transaction, so a
    /// successful chapter projection and identity migration can share the
    /// same commit boundary.
    pub fn migrate_legacy_episode(
        &self,
        input: &LegacyEpisodeMigration,
    ) -> DatabaseResult<EpisodeMigrationReport> {
        validate_episode_migration(input)?;
        let mut report = EpisodeMigrationReport::new(input, EpisodeMigrationStatus::Conflict);

        let legacy = self.get_episode(input.legacy_episode_id)?;
        let authoritative = self.get_episode(input.authoritative_episode_id)?;
        let (legacy, authoritative) = match (legacy, authoritative) {
            (Some(legacy), Some(authoritative)) => (legacy, authoritative),
            (None, _) => {
                report.status = EpisodeMigrationStatus::LegacyMissing;
                report
                    .diagnostics
                    .push("旧剧集记录不存在，未执行迁移".to_string());
                return Ok(report);
            }
            (_, None) => {
                report.status = EpisodeMigrationStatus::AuthoritativeMissing;
                report
                    .diagnostics
                    .push("权威剧集记录不存在，保留旧数据".to_string());
                return Ok(report);
            }
        };

        if legacy.id == authoritative.id {
            report.status = EpisodeMigrationStatus::AlreadyConverged;
            report
                .diagnostics
                .push("旧剧集与权威剧集已是同一条记录".to_string());
            return Ok(report);
        }

        if !self.is_authoritative_episode(&authoritative)? {
            report
                .diagnostics
                .push("权威剧集身份未通过校验，保留双源".to_string());
            return Ok(report);
        }

        let mut conflicts = Vec::new();
        if let (Some(legacy_season), Some(authoritative_season)) =
            (legacy.season_number, authoritative.season_number)
        {
            if legacy_season != authoritative_season {
                conflicts.push("季编号存在冲突".to_string());
            }
        }
        if let (Some(legacy_episode), Some(authoritative_episode)) =
            (legacy.episode_number, authoritative.episode_number)
        {
            if legacy_episode != authoritative_episode {
                conflicts.push("集编号存在冲突".to_string());
            }
        }
        if legacy.watch_position_ms > 0
            && authoritative.watch_position_ms > 0
            && legacy.watch_position_ms != authoritative.watch_position_ms
        {
            conflicts.push("观看进度存在冲突".to_string());
        }

        let chapter_collision = self
            .connection
            .query_row(
                "SELECT EXISTS(
                SELECT 1
                FROM episode_chapters legacy_chapter
                INNER JOIN episode_chapters authoritative_chapter
                    ON authoritative_chapter.episode_id = ?2
                   AND authoritative_chapter.stable_id = legacy_chapter.stable_id
                WHERE legacy_chapter.episode_id = ?1
            )",
                params![input.legacy_episode_id, input.authoritative_episode_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| map_read_error("check episode chapter migration conflict", error))?;
        if chapter_collision {
            conflicts.push("章节标识存在冲突".to_string());
        }

        let watch_feed_collision = self
            .connection
            .query_row(
                "SELECT EXISTS(
                SELECT 1
                FROM watch_feed_items legacy_feed
                INNER JOIN watch_feed_items authoritative_feed
                    ON authoritative_feed.episode_id = ?2
                   AND authoritative_feed.dedupe_key = legacy_feed.dedupe_key
                WHERE legacy_feed.episode_id = ?1
            )",
                params![input.legacy_episode_id, input.authoritative_episode_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| map_read_error("check watch feed migration conflict", error))?;
        if watch_feed_collision {
            conflicts.push("观剧流幂等键存在冲突".to_string());
        }

        let legacy_tasks = self.list_task_keys_by_episode(input.legacy_episode_id)?;
        let mut mapped_task_ids = std::collections::BTreeSet::new();
        let mut mapped_target_keys = std::collections::BTreeSet::new();
        for migration in &input.task_key_migrations {
            require_text(&migration.new_task_key, "迁移后的任务标识不能为空")?;
            if !mapped_task_ids.insert(migration.task_id) {
                conflicts.push("同一旧任务被重复指定迁移".to_string());
                continue;
            }
            if !mapped_target_keys.insert(migration.new_task_key.as_str()) {
                conflicts.push("多个旧任务指定了同一个新任务标识".to_string());
                continue;
            }
            if !legacy_tasks.iter().any(|(id, _)| *id == migration.task_id) {
                conflicts.push("任务迁移范围与旧剧集不一致".to_string());
                continue;
            }
            let existing_task_id = self
                .connection
                .query_row(
                    "SELECT id FROM agent_tasks WHERE task_key = ?1",
                    params![migration.new_task_key],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(|error| {
                    map_read_error("check agent task key migration conflict", error)
                })?;
            if existing_task_id.is_some_and(|id| id != migration.task_id) {
                conflicts.push("迁移后的任务标识已被占用".to_string());
            }
        }

        if !conflicts.is_empty() {
            report.diagnostics = conflicts;
            return Ok(report);
        }

        if legacy.watch_position_ms > authoritative.watch_position_ms {
            self.connection
                .execute(
                    "UPDATE episodes SET watch_position_ms = ?1, updated_at_ms = ?2 WHERE id = ?3",
                    params![legacy.watch_position_ms, now_ms(), authoritative.id],
                )
                .map_err(|error| map_write_error("merge episode watch position", error))?;
        }

        report.moved_chapters = self
            .connection
            .execute(
                "UPDATE episode_chapters SET episode_id = ?1 WHERE episode_id = ?2",
                params![authoritative.id, legacy.id],
            )
            .map_err(|error| map_write_error("migrate episode chapters", error))?
            as u64;
        report.moved_questions = self
            .connection
            .execute(
                "UPDATE question_candidates SET episode_id = ?1 WHERE episode_id = ?2",
                params![authoritative.id, legacy.id],
            )
            .map_err(|error| map_write_error("migrate question candidates", error))?
            as u64;
        report.moved_feed_items = self
            .connection
            .execute(
                "UPDATE watch_feed_items SET episode_id = ?1 WHERE episode_id = ?2",
                params![authoritative.id, legacy.id],
            )
            .map_err(|error| map_write_error("migrate watch feed items", error))?
            as u64;

        for migration in &input.task_key_migrations {
            self.connection
                .execute(
                    "UPDATE agent_tasks
                     SET task_key = ?1, episode_id = ?2, updated_at_ms = ?3
                     WHERE id = ?4 AND episode_id = ?5",
                    params![
                        migration.new_task_key,
                        authoritative.id,
                        now_ms(),
                        migration.task_id,
                        legacy.id
                    ],
                )
                .map_err(|error| map_write_error("migrate agent task key", error))?;
            report.renamed_tasks += 1;
        }

        for (task_id, task_key) in legacy_tasks {
            if !mapped_task_ids.contains(&task_id) {
                report.preserved_task_keys.push(task_key);
            }
        }
        report.status = if report.preserved_task_keys.is_empty() {
            EpisodeMigrationStatus::Merged
        } else {
            report
                .diagnostics
                .push("未提供安全的新任务标识，旧任务保留原标识以保证重试兼容".to_string());
            EpisodeMigrationStatus::MergedWithPreservedTasks
        };
        Ok(report)
    }

    fn is_authoritative_episode(&self, episode: &EpisodeRecord) -> DatabaseResult<bool> {
        let Some(series) = self.get_series(episode.series_id)? else {
            return Ok(false);
        };
        let Some(tmdb_id) = series.stable_id.strip_prefix("tmdb:tv:") else {
            return Ok(false);
        };
        if tmdb_id.parse::<u64>().ok().filter(|id| *id > 0).is_none() {
            return Ok(false);
        }
        let (Some(season), Some(number)) = (episode.season_number, episode.episode_number) else {
            return Ok(false);
        };
        Ok(season > 0
            && number > 0
            && episode.stable_id == format!("s{season:02}e{number:02}")
            && episode.source == "tmdb")
    }

    fn list_task_keys_by_episode(&self, episode_id: i64) -> DatabaseResult<Vec<(i64, String)>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, task_key FROM agent_tasks WHERE episode_id = ?1 ORDER BY id ASC")
            .map_err(|error| map_read_error("prepare episode task migration", error))?;
        let rows = statement
            .query_map(params![episode_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| map_read_error("list episode task migration", error))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| map_read_error("read episode task migration", error))
    }

    pub fn insert_series(&self, input: &NewSeries) -> DatabaseResult<i64> {
        require_text(&input.stable_id, "剧集标识不能为空")?;
        require_text(&input.title, "剧集标题不能为空")?;
        require_text(&input.source, "剧集来源不能为空")?;
        self.connection
            .execute(
                "INSERT INTO series(stable_id, title, source, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![input.stable_id, input.title, input.source, now_ms()],
            )
            .map(|_| self.connection.last_insert_rowid())
            .map_err(|error| map_write_error("insert series", error))
    }

    pub fn get_series(&self, id: i64) -> DatabaseResult<Option<SeriesRecord>> {
        self.connection
            .query_row(
                "SELECT id, stable_id, title, source, created_at_ms, updated_at_ms
                 FROM series WHERE id = ?1",
                params![id],
                map_series_row,
            )
            .optional()
            .map_err(|error| map_read_error("read series", error))
    }

    pub fn get_series_by_stable_id(&self, stable_id: &str) -> DatabaseResult<Option<SeriesRecord>> {
        require_text(stable_id, "剧集标识不能为空")?;
        self.connection
            .query_row(
                "SELECT id, stable_id, title, source, created_at_ms, updated_at_ms
                 FROM series WHERE stable_id = ?1",
                params![stable_id],
                map_series_row,
            )
            .optional()
            .map_err(|error| map_read_error("read series by stable id", error))
    }

    pub fn insert_episode(&self, input: &NewEpisode) -> DatabaseResult<i64> {
        require_text(&input.stable_id, "剧集集数标识不能为空")?;
        require_text(&input.source, "剧集来源不能为空")?;
        self.connection
            .execute(
                "INSERT INTO episodes(
                    series_id, stable_id, season_number, episode_number, title,
                    duration_ms, source, watch_position_ms, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8)",
                params![
                    input.series_id,
                    input.stable_id,
                    input.season_number,
                    input.episode_number,
                    input.title,
                    input.duration_ms,
                    input.source,
                    now_ms(),
                ],
            )
            .map(|_| self.connection.last_insert_rowid())
            .map_err(|error| map_write_error("insert episode", error))
    }

    pub fn get_episode(&self, id: i64) -> DatabaseResult<Option<EpisodeRecord>> {
        self.connection
            .query_row(
                "SELECT id, series_id, stable_id, season_number, episode_number, title,
                        duration_ms, source, watch_position_ms, created_at_ms, updated_at_ms
                 FROM episodes WHERE id = ?1",
                params![id],
                map_episode_row,
            )
            .optional()
            .map_err(|error| map_read_error("read episode", error))
    }

    pub fn get_episode_by_stable_id(
        &self,
        series_id: i64,
        stable_id: &str,
    ) -> DatabaseResult<Option<EpisodeRecord>> {
        require_text(stable_id, "剧集集数标识不能为空")?;
        self.connection
            .query_row(
                "SELECT id, series_id, stable_id, season_number, episode_number, title,
                        duration_ms, source, watch_position_ms, created_at_ms, updated_at_ms
                 FROM episodes WHERE series_id = ?1 AND stable_id = ?2",
                params![series_id, stable_id],
                map_episode_row,
            )
            .optional()
            .map_err(|error| map_read_error("read episode by stable id", error))
    }

    /// Look up an episode using the stable series and episode identities.
    ///
    /// The media path is intentionally absent from this API.  A path may be
    /// used to locate an index entry, but it is not a durable identity.
    pub fn get_episode_by_identity(
        &self,
        series_stable_id: &str,
        episode_stable_id: &str,
    ) -> DatabaseResult<Option<EpisodeRecord>> {
        require_text(series_stable_id, "剧集标识不能为空")?;
        require_text(episode_stable_id, "剧集集数标识不能为空")?;
        self.connection
            .query_row(
                "SELECT e.id, e.series_id, e.stable_id, e.season_number, e.episode_number,
                        e.title, e.duration_ms, e.source, e.watch_position_ms,
                        e.created_at_ms, e.updated_at_ms
                 FROM episodes e
                 INNER JOIN series s ON s.id = e.series_id
                 WHERE s.stable_id = ?1 AND e.stable_id = ?2",
                params![series_stable_id, episode_stable_id],
                map_episode_row,
            )
            .optional()
            .map_err(|error| map_read_error("read episode by identity", error))
    }

    /// Insert an episode only when its explicit series/episode identity is
    /// absent, then return the durable record.
    ///
    /// Existing metadata is never overwritten.  This makes repeated worker
    /// execution and path changes safe while keeping the caller responsible
    /// for supplying a real `series_id` and stable episode key.
    pub fn get_or_create_episode(&self, input: &NewEpisode) -> DatabaseResult<EpisodeRecord> {
        validate_new_episode(input)?;
        self.connection
            .execute(
                "INSERT INTO episodes(
                    series_id, stable_id, season_number, episode_number, title,
                    duration_ms, source, watch_position_ms, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8)
                 ON CONFLICT(series_id, stable_id) DO NOTHING",
                params![
                    input.series_id,
                    input.stable_id,
                    input.season_number,
                    input.episode_number,
                    input.title,
                    input.duration_ms,
                    input.source,
                    now_ms(),
                ],
            )
            .map_err(|error| map_write_error("get or create episode", error))?;

        self.get_episode_by_stable_id(input.series_id, &input.stable_id)?
            .ok_or_else(|| {
                DatabaseError::new(
                    DatabaseErrorCode::QueryFailed,
                    "应用数据存储读取失败，请重试",
                    Some("episode disappeared after idempotent insert".to_string()),
                )
            })
    }

    pub fn insert_chapter(&self, input: &NewChapter) -> DatabaseResult<i64> {
        validate_time_bounds(input.start_ms, input.end_ms)?;
        require_text(&input.stable_id, "章节标识不能为空")?;
        require_text(&input.source, "章节来源不能为空")?;
        require_text(&input.spoiler_level, "章节剧透等级不能为空")?;
        self.connection
            .execute(
                "INSERT INTO episode_chapters(
                    episode_id, stable_id, source, start_ms, end_ms, spoiler_level,
                    title, mainline, status, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
                params![
                    input.episode_id,
                    input.stable_id,
                    input.source,
                    input.start_ms,
                    input.end_ms,
                    input.spoiler_level,
                    input.title,
                    input.mainline,
                    input.status,
                    now_ms(),
                ],
            )
            .map(|_| self.connection.last_insert_rowid())
            .map_err(|error| map_write_error("insert chapter", error))
    }

    pub fn get_chapter(&self, id: i64) -> DatabaseResult<Option<ChapterRecord>> {
        self.connection
            .query_row(
                "SELECT id, episode_id, stable_id, source, start_ms, end_ms,
                        spoiler_level, title, mainline, status, created_at_ms, updated_at_ms
                 FROM episode_chapters WHERE id = ?1",
                params![id],
                map_chapter_row,
            )
            .optional()
            .map_err(|error| map_read_error("read chapter", error))
    }

    /// Read all chapters belonging to an episode in playback order.
    pub fn list_chapters_by_episode(&self, episode_id: i64) -> DatabaseResult<Vec<ChapterRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, episode_id, stable_id, source, start_ms, end_ms,
                        spoiler_level, title, mainline, status, created_at_ms, updated_at_ms
                 FROM episode_chapters
                 WHERE episode_id = ?1
                 ORDER BY start_ms ASC, end_ms ASC, id ASC",
            )
            .map_err(|error| map_read_error("prepare chapters by episode", error))?;
        let rows = statement
            .query_map(params![episode_id], map_chapter_row)
            .map_err(|error| map_read_error("list chapters by episode", error))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| map_read_error("read chapters by episode", error))
    }

    /// Return one chapter by its episode-local stable id.
    pub fn get_chapter_by_stable_id(
        &self,
        episode_id: i64,
        stable_id: &str,
    ) -> DatabaseResult<Option<ChapterRecord>> {
        require_text(stable_id, "章节标识不能为空")?;
        self.connection
            .query_row(
                "SELECT id, episode_id, stable_id, source, start_ms, end_ms,
                        spoiler_level, title, mainline, status, created_at_ms, updated_at_ms
                 FROM episode_chapters
                 WHERE episode_id = ?1 AND stable_id = ?2",
                params![episode_id, stable_id],
                map_chapter_row,
            )
            .optional()
            .map_err(|error| map_read_error("read chapter by stable id", error))
    }

    /// Assert and record the task-to-chapter relationship used by Chapter
    /// Agent tools.  The relationship is separate from `agent_tasks.chapter_id`
    /// because one outline task may own more than one chapter.
    pub fn ensure_agent_task_chapter_scope(
        &self,
        task_id: i64,
        episode_id: i64,
        chapter_id: i64,
    ) -> DatabaseResult<()> {
        let task = self
            .get_agent_task(task_id)?
            .ok_or_else(|| DatabaseError::invalid_input("章节任务不存在"))?;
        if !task.task_type.starts_with("chapter") || task.episode_id != Some(episode_id) {
            return Err(DatabaseError::invalid_input("章节任务与集数不匹配"));
        }
        let chapter = self
            .get_chapter(chapter_id)?
            .ok_or_else(|| DatabaseError::invalid_input("章节不存在"))?;
        if chapter.episode_id != episode_id {
            return Err(DatabaseError::invalid_input("章节不属于当前集数"));
        }

        let other_task = self
            .connection
            .query_row(
                "SELECT task_id FROM agent_task_chapters
                 WHERE chapter_id = ?1 AND task_id <> ?2
                 LIMIT 1",
                params![chapter_id, task_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| map_read_error("read chapter task ownership", error))?;
        if other_task.is_some() {
            return Err(DatabaseError::invalid_input("章节已属于其他章节任务"));
        }

        self.connection
            .execute(
                "INSERT OR IGNORE INTO agent_task_chapters(task_id, chapter_id, created_at_ms)
                 VALUES (?1, ?2, ?3)",
                params![task_id, chapter_id, now_ms()],
            )
            .map(|_| ())
            .map_err(|error| map_write_error("link chapter task scope", error))
    }

    pub fn list_chapters_by_agent_task(
        &self,
        task_id: i64,
        episode_id: i64,
    ) -> DatabaseResult<Vec<ChapterRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT c.id, c.episode_id, c.stable_id, c.source, c.start_ms, c.end_ms,
                        c.spoiler_level, c.title, c.mainline, c.status,
                        c.created_at_ms, c.updated_at_ms
                 FROM episode_chapters c
                 INNER JOIN agent_task_chapters tc ON tc.chapter_id = c.id
                 WHERE tc.task_id = ?1 AND c.episode_id = ?2
                 ORDER BY c.start_ms ASC, c.end_ms ASC, c.id ASC",
            )
            .map_err(|error| map_read_error("prepare chapters by agent task", error))?;
        let rows = statement
            .query_map(params![task_id, episode_id], map_chapter_row)
            .map_err(|error| map_read_error("list chapters by agent task", error))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| map_read_error("read chapters by agent task", error))
    }

    /// Replace the draft chapter set owned by a task. Chapters omitted by a
    /// newer outline are removed only while still draft; published rows are
    /// protected so a retry cannot silently rewrite history.
    pub fn replace_agent_task_chapter_outline(
        &self,
        task_id: i64,
        episode_id: i64,
        keep_chapter_ids: &[i64],
    ) -> DatabaseResult<()> {
        let chapters = self.list_chapters_by_agent_task(task_id, episode_id)?;
        for chapter in chapters {
            if keep_chapter_ids.contains(&chapter.id) {
                continue;
            }
            if matches!(chapter.status.as_str(), "ready" | "published" | "accepted") {
                return Err(DatabaseError::invalid_input("已发布章节不能从大纲中移除"));
            }
            self.connection
                .execute(
                    "DELETE FROM episode_chapters
                     WHERE id = ?1 AND episode_id = ?2 AND status = 'draft'",
                    params![chapter.id, episode_id],
                )
                .map_err(|error| map_write_error("remove stale draft chapter", error))?;
        }
        Ok(())
    }

    /// Idempotently create or update a draft outline row and attach it to the
    /// task.  Published chapters cannot be rewritten by an outline retry.
    pub fn upsert_draft_chapter_for_agent_task(
        &self,
        task_id: i64,
        episode_id: i64,
        input: &NewChapter,
    ) -> DatabaseResult<ChapterRecord> {
        if input.episode_id != episode_id {
            return Err(DatabaseError::invalid_input("章节不属于当前集数"));
        }
        validate_time_bounds(input.start_ms, input.end_ms)?;
        require_text(&input.stable_id, "章节标识不能为空")?;
        require_text(&input.source, "章节来源不能为空")?;
        require_text(&input.spoiler_level, "章节剧透等级不能为空")?;
        let existing = self.get_chapter_by_stable_id(episode_id, &input.stable_id)?;
        let chapter_id = if let Some(chapter) = existing {
            self.ensure_agent_task_chapter_scope(task_id, episode_id, chapter.id)?;
            if matches!(chapter.status.as_str(), "ready" | "published" | "accepted") {
                return Err(DatabaseError::invalid_input("已发布章节不能被大纲重写"));
            }
            self.connection
                .execute(
                    "UPDATE episode_chapters
                     SET source = ?1, start_ms = ?2, end_ms = ?3, spoiler_level = ?4,
                         title = ?5, status = 'draft', updated_at_ms = ?6
                     WHERE id = ?7",
                    params![
                        input.source,
                        input.start_ms,
                        input.end_ms,
                        input.spoiler_level,
                        input.title,
                        now_ms(),
                        chapter.id,
                    ],
                )
                .map_err(|error| map_write_error("update draft chapter outline", error))?;
            chapter.id
        } else {
            let chapter_id = self.insert_chapter(input)?;
            self.ensure_agent_task_chapter_scope(task_id, episode_id, chapter_id)?;
            chapter_id
        };
        self.get_chapter(chapter_id)?.ok_or_else(|| {
            DatabaseError::new(
                DatabaseErrorCode::QueryFailed,
                "应用数据存储读取失败，请重试",
                Some("chapter disappeared after outline upsert".to_string()),
            )
        })
    }

    pub fn update_draft_chapter_for_agent_task(
        &self,
        task_id: i64,
        episode_id: i64,
        chapter_id: i64,
        title: Option<&str>,
        mainline: Option<&str>,
    ) -> DatabaseResult<ChapterRecord> {
        self.ensure_agent_task_chapter_scope(task_id, episode_id, chapter_id)?;
        if title.is_none() && mainline.is_none() {
            return Err(DatabaseError::invalid_input("章节草稿至少需要一个字段"));
        }
        if let Some(value) = title {
            require_text(value, "章节标题不能为空")?;
        }
        if let Some(value) = mainline {
            require_text(value, "章节主线不能为空")?;
        }
        let chapter = self
            .get_chapter(chapter_id)?
            .ok_or_else(|| DatabaseError::invalid_input("章节不存在"))?;
        if matches!(chapter.status.as_str(), "ready" | "published" | "accepted") {
            return Err(DatabaseError::invalid_input("已发布章节不能被草稿更新"));
        }
        self.connection
            .execute(
                "UPDATE episode_chapters
                 SET title = COALESCE(?1, title), mainline = COALESCE(?2, mainline),
                     status = 'draft', updated_at_ms = ?3
                 WHERE id = ?4",
                params![title, mainline, now_ms(), chapter_id],
            )
            .map_err(|error| map_write_error("update draft chapter", error))?;
        self.get_chapter(chapter_id)?.ok_or_else(|| {
            DatabaseError::new(
                DatabaseErrorCode::QueryFailed,
                "应用数据存储读取失败，请重试",
                Some("chapter disappeared after draft update".to_string()),
            )
        })
    }

    pub fn insert_chapter_asset(&self, input: &NewChapterAsset) -> DatabaseResult<i64> {
        require_text(&input.asset_type, "章节资源类型不能为空")?;
        require_text(&input.path, "章节资源路径不能为空")?;
        require_text(&input.content_hash, "章节资源哈希不能为空")?;
        require_text(&input.source, "章节资源来源不能为空")?;
        self.connection
            .execute(
                "INSERT INTO chapter_assets(
                    chapter_id, asset_type, path, content_hash, captured_at_ms,
                    width, height, source, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    input.chapter_id,
                    input.asset_type,
                    input.path,
                    input.content_hash,
                    input.captured_at_ms,
                    input.width,
                    input.height,
                    input.source,
                    now_ms(),
                ],
            )
            .map(|_| self.connection.last_insert_rowid())
            .map_err(|error| map_write_error("insert chapter asset", error))
    }

    pub fn get_chapter_asset(&self, id: i64) -> DatabaseResult<Option<ChapterAssetRecord>> {
        self.connection
            .query_row(
                "SELECT id, chapter_id, asset_type, path, content_hash, captured_at_ms,
                        width, height, source, created_at_ms
                 FROM chapter_assets WHERE id = ?1",
                params![id],
                map_chapter_asset_row,
            )
            .optional()
            .map_err(|error| map_read_error("read chapter asset", error))
    }

    pub fn insert_chapter_asset_for_agent_task(
        &self,
        task_id: i64,
        episode_id: i64,
        input: &NewChapterAsset,
    ) -> DatabaseResult<ChapterAssetRecord> {
        self.ensure_agent_task_chapter_scope(task_id, episode_id, input.chapter_id)?;
        require_text(&input.asset_type, "章节资源类型不能为空")?;
        require_text(&input.path, "章节资源路径不能为空")?;
        require_text(&input.content_hash, "章节资源哈希不能为空")?;
        require_text(&input.source, "章节资源来源不能为空")?;
        self.connection
            .execute(
                "INSERT OR IGNORE INTO chapter_assets(
                    chapter_id, asset_type, path, content_hash, captured_at_ms,
                    width, height, source, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    input.chapter_id,
                    input.asset_type,
                    input.path,
                    input.content_hash,
                    input.captured_at_ms,
                    input.width,
                    input.height,
                    input.source,
                    now_ms(),
                ],
            )
            .map_err(|error| map_write_error("insert chapter asset for task", error))?;
        self.connection
            .query_row(
                "SELECT id, chapter_id, asset_type, path, content_hash, captured_at_ms,
                        width, height, source, created_at_ms
                 FROM chapter_assets
                 WHERE chapter_id = ?1 AND content_hash = ?2",
                params![input.chapter_id, input.content_hash],
                map_chapter_asset_row,
            )
            .map_err(|error| map_read_error("read chapter asset after idempotent insert", error))
    }

    /// Read chapter assets without exposing their local paths to callers.
    /// The repository returns records; the desktop boundary decides how to
    /// turn them into opaque resource references.
    pub fn list_chapter_assets_by_chapter(
        &self,
        chapter_id: i64,
    ) -> DatabaseResult<Vec<ChapterAssetRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, chapter_id, asset_type, path, content_hash, captured_at_ms,
                        width, height, source, created_at_ms
                 FROM chapter_assets
                 WHERE chapter_id = ?1
                 ORDER BY captured_at_ms ASC, id ASC",
            )
            .map_err(|error| map_read_error("prepare chapter assets", error))?;
        let rows = statement
            .query_map(params![chapter_id], map_chapter_asset_row)
            .map_err(|error| map_read_error("list chapter assets", error))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| map_read_error("read chapter assets", error))
    }

    pub fn insert_chapter_revision(&self, input: &NewChapterRevision) -> DatabaseResult<i64> {
        require_text(&input.revision_type, "章节版本类型不能为空")?;
        require_text(&input.content, "章节版本内容不能为空")?;
        require_text(&input.source, "章节版本来源不能为空")?;
        require_text(&input.prompt_version, "提示词版本不能为空")?;
        self.connection
            .execute(
                "INSERT INTO chapter_revisions(
                    chapter_id, revision_number, revision_type, content, source,
                    prompt_version, validation_report, status, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    input.chapter_id,
                    input.revision_number,
                    input.revision_type,
                    input.content,
                    input.source,
                    input.prompt_version,
                    input.validation_report,
                    input.status,
                    now_ms(),
                ],
            )
            .map(|_| self.connection.last_insert_rowid())
            .map_err(|error| map_write_error("insert chapter revision", error))
    }

    pub fn get_chapter_revision(&self, id: i64) -> DatabaseResult<Option<ChapterRevisionRecord>> {
        self.connection
            .query_row(
                "SELECT id, chapter_id, revision_number, revision_type, content, source,
                        prompt_version, validation_report, status, created_at_ms
                 FROM chapter_revisions WHERE id = ?1",
                params![id],
                map_chapter_revision_row,
            )
            .optional()
            .map_err(|error| map_read_error("read chapter revision", error))
    }

    /// Read the latest revision for the chapter.  The ordering is deterministic
    /// for legacy rows that may share a revision number.
    pub fn get_latest_chapter_revision(
        &self,
        chapter_id: i64,
    ) -> DatabaseResult<Option<ChapterRevisionRecord>> {
        self.connection
            .query_row(
                "SELECT id, chapter_id, revision_number, revision_type, content, source,
                        prompt_version, validation_report, status, created_at_ms
                 FROM chapter_revisions
                 WHERE chapter_id = ?1
                 ORDER BY revision_number DESC, id DESC
                 LIMIT 1",
                params![chapter_id],
                map_chapter_revision_row,
            )
            .optional()
            .map_err(|error| map_read_error("read latest chapter revision", error))
    }

    pub fn insert_draft_revision_for_agent_task(
        &self,
        task_id: i64,
        episode_id: i64,
        input: &NewChapterRevision,
    ) -> DatabaseResult<ChapterRevisionRecord> {
        self.ensure_agent_task_chapter_scope(task_id, episode_id, input.chapter_id)?;
        let existing = self
            .connection
            .query_row(
                "SELECT id, chapter_id, revision_number, revision_type, content, source,
                        prompt_version, validation_report, status, created_at_ms
                 FROM chapter_revisions
                 WHERE chapter_id = ?1 AND revision_type = ?2
                 ORDER BY id DESC LIMIT 1",
                params![input.chapter_id, input.revision_type],
                map_chapter_revision_row,
            )
            .optional()
            .map_err(|error| map_read_error("read stable draft revision", error))?;
        if let Some(existing) = existing {
            if existing.status == "accepted" {
                return Err(DatabaseError::invalid_input("已发布章节版本不能被重试覆盖"));
            }
            self.connection
                .execute(
                    "UPDATE chapter_revisions
                     SET content = ?1, source = ?2, prompt_version = ?3,
                         validation_report = ?4, status = 'draft'
                     WHERE id = ?5",
                    params![
                        input.content,
                        input.source,
                        input.prompt_version,
                        input.validation_report,
                        existing.id,
                    ],
                )
                .map_err(|error| map_write_error("update stable draft revision", error))?;
            return self.get_chapter_revision(existing.id)?.ok_or_else(|| {
                DatabaseError::new(
                    DatabaseErrorCode::QueryFailed,
                    "应用数据存储读取失败，请重试",
                    Some("stable draft revision disappeared after update".to_string()),
                )
            });
        }
        let revision_id = self.insert_chapter_revision(input)?;
        self.get_chapter_revision(revision_id)?.ok_or_else(|| {
            DatabaseError::new(
                DatabaseErrorCode::QueryFailed,
                "应用数据存储读取失败，请重试",
                Some("revision disappeared after draft insert".to_string()),
            )
        })
    }

    pub fn insert_agent_task(&self, input: &NewAgentTask) -> DatabaseResult<i64> {
        validate_new_agent_task(input)?;
        self.connection
            .execute(
                "INSERT INTO agent_tasks(
                    task_key, task_type, episode_id, chapter_id, status, session_id,
                    prompt_version, output_contract_version, attempt_count, retry_count,
                    max_attempts, validation_report, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 0, ?9, ?10, ?11, ?11)",
                params![
                    input.task_key,
                    input.task_type,
                    input.episode_id,
                    input.chapter_id,
                    input.status,
                    input.session_id,
                    input.prompt_version,
                    input.output_contract_version,
                    input.max_attempts,
                    input.validation_report,
                    now_ms(),
                ],
            )
            .map(|_| self.connection.last_insert_rowid())
            .map_err(|error| map_write_error("insert agent task", error))
    }

    /// Read an agent task using its stable, caller-owned idempotency key.
    pub fn get_agent_task_by_key(&self, task_key: &str) -> DatabaseResult<Option<AgentTaskRecord>> {
        require_text(task_key, "任务标识不能为空")?;
        self.connection
            .query_row(
                "SELECT id, task_key, task_type, episode_id, chapter_id, status, session_id,
                        prompt_version, output_contract_version, attempt_count, retry_count,
                        max_attempts, validation_report, output_json, created_at_ms, updated_at_ms
                 FROM agent_tasks WHERE task_key = ?1",
                params![task_key],
                map_agent_task_row,
            )
            .optional()
            .map_err(|error| map_read_error("read agent task by key", error))
    }

    /// Return the task when it is still eligible to occupy the active slot.
    /// Validation failures remain active while another attempt is available.
    pub fn get_active_agent_task_by_key(
        &self,
        task_key: &str,
    ) -> DatabaseResult<Option<AgentTaskRecord>> {
        let task = self.get_agent_task_by_key(task_key)?;
        Ok(task.filter(is_active_agent_task))
    }

    pub fn is_agent_task_active(&self, task_key: &str) -> DatabaseResult<bool> {
        Ok(self.get_active_agent_task_by_key(task_key)?.is_some())
    }

    /// Atomically claim an eligible task for execution.
    ///
    /// The conditional update is deliberately a single SQLite statement.  It
    /// prevents two callers from both observing an eligible task and moving it
    /// to `running`; only the statement that updates one row receives the
    /// updated record.  Validation failures can be claimed again while an
    /// attempt remains, but a running, succeeded, or failed task cannot be
    /// claimed a second time.
    pub fn claim_agent_task_by_key(
        &self,
        task_key: &str,
    ) -> DatabaseResult<Option<AgentTaskRecord>> {
        require_text(task_key, "任务标识不能为空")?;
        self.connection
            .query_row(
                "UPDATE agent_tasks
                 SET status = 'running',
                     attempt_count = attempt_count + 1,
                     updated_at_ms = ?1
                 WHERE task_key = ?2
                   AND status IN ('pending', 'validation_failure')
                   AND attempt_count < max_attempts
                 RETURNING id, task_key, task_type, episode_id, chapter_id, status,
                           session_id, prompt_version, output_contract_version,
                           attempt_count, retry_count, max_attempts, validation_report,
                           output_json, created_at_ms, updated_at_ms",
                params![now_ms(), task_key],
                map_agent_task_row,
            )
            .optional()
            .map_err(|error| map_write_error("claim agent task", error))
    }

    /// Create a task only when its key has not been seen before, then return
    /// the durable row.  The unique key and conflict clause make repeated
    /// clicks idempotent without moving UI policy into this crate.
    pub fn get_or_create_agent_task(
        &self,
        input: &NewAgentTask,
    ) -> DatabaseResult<AgentTaskRecord> {
        validate_new_agent_task(input)?;
        self.connection
            .execute(
                "INSERT INTO agent_tasks(
                    task_key, task_type, episode_id, chapter_id, status, session_id,
                    prompt_version, output_contract_version, attempt_count, retry_count,
                    max_attempts, validation_report, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 0, ?9, ?10, ?11, ?11)
                 ON CONFLICT(task_key) DO NOTHING",
                params![
                    input.task_key,
                    input.task_type,
                    input.episode_id,
                    input.chapter_id,
                    input.status,
                    input.session_id,
                    input.prompt_version,
                    input.output_contract_version,
                    input.max_attempts,
                    input.validation_report,
                    now_ms(),
                ],
            )
            .map_err(|error| map_write_error("create agent task idempotently", error))?;

        self.get_agent_task_by_key(&input.task_key)?.ok_or_else(|| {
            DatabaseError::new(
                DatabaseErrorCode::QueryFailed,
                "应用数据存储读取失败，请重试",
                Some("agent task was not available after idempotent create".to_string()),
            )
        })
    }

    pub fn get_agent_task(&self, id: i64) -> DatabaseResult<Option<AgentTaskRecord>> {
        self.connection
            .query_row(
                "SELECT id, task_key, task_type, episode_id, chapter_id, status, session_id,
                        prompt_version, output_contract_version, attempt_count, retry_count,
                        max_attempts, validation_report, output_json, created_at_ms, updated_at_ms
                 FROM agent_tasks WHERE id = ?1",
                params![id],
                map_agent_task_row,
            )
            .optional()
            .map_err(|error| map_read_error("read agent task", error))
    }

    pub fn update_agent_task_status(
        &self,
        id: i64,
        status: &str,
        attempt_count: i64,
        retry_count: i64,
        validation_report: Option<&str>,
    ) -> DatabaseResult<bool> {
        require_text(status, "任务状态不能为空")?;
        if attempt_count < 0 || retry_count < 0 {
            return Err(DatabaseError::invalid_input("任务尝试次数不能为负数"));
        }
        let max_attempts = self
            .connection
            .query_row(
                "SELECT max_attempts FROM agent_tasks WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| map_read_error("read agent task attempt limit", error))?;
        if max_attempts.is_some_and(|max_attempts| attempt_count > max_attempts) {
            return Err(DatabaseError::max_attempts_exceeded());
        }
        let changed = self
            .connection
            .execute(
                "UPDATE agent_tasks
                 SET status = ?1, attempt_count = ?2, retry_count = ?3,
                     validation_report = ?4, updated_at_ms = ?5
                 WHERE id = ?6",
                params![
                    status,
                    attempt_count,
                    retry_count,
                    validation_report,
                    now_ms(),
                    id
                ],
            )
            .map_err(|error| map_write_error("update agent task", error))?;
        Ok(changed == 1)
    }

    pub fn update_agent_task_status_by_key(
        &self,
        task_key: &str,
        status: &str,
        attempt_count: i64,
        retry_count: i64,
        validation_report: Option<&str>,
    ) -> DatabaseResult<bool> {
        let task = self.get_agent_task_by_key(task_key)?;
        let Some(task) = task else {
            return Ok(false);
        };
        self.update_agent_task_status(
            task.id,
            status,
            attempt_count,
            retry_count,
            validation_report,
        )
    }

    pub fn update_agent_task_scope(
        &self,
        id: i64,
        episode_id: Option<i64>,
        chapter_id: Option<i64>,
    ) -> DatabaseResult<bool> {
        let changed = self
            .connection
            .execute(
                "UPDATE agent_tasks
                 SET episode_id = ?1, chapter_id = ?2, updated_at_ms = ?3
                 WHERE id = ?4",
                params![episode_id, chapter_id, now_ms(), id],
            )
            .map_err(|error| map_write_error("update agent task scope", error))?;
        Ok(changed == 1)
    }

    /// Store a validated structured result together with the terminal task
    /// state. The caller validates the JSON against the task contract first.
    pub fn complete_agent_task(
        &self,
        id: i64,
        status: &str,
        attempt_count: i64,
        retry_count: i64,
        validation_report: Option<&str>,
        output_json: &str,
    ) -> DatabaseResult<bool> {
        require_text(status, "任务状态不能为空")?;
        require_text(output_json, "任务输出不能为空")?;
        if attempt_count < 0 || retry_count < 0 {
            return Err(DatabaseError::invalid_input("任务尝试次数不能为负数"));
        }
        let max_attempts = self
            .connection
            .query_row(
                "SELECT max_attempts FROM agent_tasks WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| map_read_error("read agent task attempt limit", error))?;
        if max_attempts.is_some_and(|max_attempts| attempt_count > max_attempts) {
            return Err(DatabaseError::max_attempts_exceeded());
        }
        let changed = self
            .connection
            .execute(
                "UPDATE agent_tasks
                 SET status = ?1, attempt_count = ?2, retry_count = ?3,
                     validation_report = ?4, output_json = ?5, updated_at_ms = ?6
                 WHERE id = ?7",
                params![
                    status,
                    attempt_count,
                    retry_count,
                    validation_report,
                    output_json,
                    now_ms(),
                    id
                ],
            )
            .map_err(|error| map_write_error("complete agent task", error))?;
        Ok(changed == 1)
    }

    pub fn update_agent_task_session_id(
        &self,
        task_key: &str,
        session_id: Option<&str>,
    ) -> DatabaseResult<bool> {
        require_text(task_key, "任务标识不能为空")?;
        if let Some(session_id) = session_id {
            require_text(session_id, "Agent 会话标识不能为空")?;
        }
        let changed = self
            .connection
            .execute(
                "UPDATE agent_tasks
                 SET session_id = ?1, updated_at_ms = ?2
                 WHERE task_key = ?3",
                params![session_id, now_ms(), task_key],
            )
            .map_err(|error| map_write_error("update agent task session", error))?;
        Ok(changed == 1)
    }

    pub fn insert_agent_attempt(&self, input: &NewAgentAttempt) -> DatabaseResult<i64> {
        require_text(&input.attempt_kind, "任务尝试类型不能为空")?;
        require_text(&input.status, "任务尝试状态不能为空")?;
        require_text(&input.prompt_version, "提示词版本不能为空")?;
        if input.attempt_number < 1 {
            return Err(DatabaseError::invalid_input("任务尝试序号必须大于零"));
        }
        let max_attempts = self
            .connection
            .query_row(
                "SELECT max_attempts FROM agent_tasks WHERE id = ?1",
                params![input.task_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| map_read_error("read agent task attempt limit", error))?;
        if max_attempts.is_some_and(|max_attempts| input.attempt_number > max_attempts) {
            return Err(DatabaseError::max_attempts_exceeded());
        }
        self.connection
            .execute(
                "INSERT INTO agent_attempts(
                    task_id, attempt_number, attempt_kind, status, prompt_version,
                    validation_report, started_at_ms, finished_at_ms, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    input.task_id,
                    input.attempt_number,
                    input.attempt_kind,
                    input.status,
                    input.prompt_version,
                    input.validation_report,
                    input.started_at_ms,
                    input.finished_at_ms,
                    now_ms(),
                ],
            )
            .map(|_| self.connection.last_insert_rowid())
            .map_err(|error| map_write_error("insert agent attempt", error))
    }

    pub fn update_agent_attempt_status(
        &self,
        id: i64,
        status: &str,
        validation_report: Option<&str>,
        finished_at_ms: i64,
    ) -> DatabaseResult<bool> {
        require_text(status, "任务尝试状态不能为空")?;
        let changed = self
            .connection
            .execute(
                "UPDATE agent_attempts
                 SET status = ?1, validation_report = ?2, finished_at_ms = ?3
                 WHERE id = ?4",
                params![status, validation_report, finished_at_ms, id],
            )
            .map_err(|error| map_write_error("update agent attempt", error))?;
        Ok(changed == 1)
    }

    pub fn get_agent_attempt(&self, id: i64) -> DatabaseResult<Option<AgentAttemptRecord>> {
        self.connection
            .query_row(
                "SELECT id, task_id, attempt_number, attempt_kind, status, prompt_version,
                        validation_report, started_at_ms, finished_at_ms, created_at_ms
                 FROM agent_attempts WHERE id = ?1",
                params![id],
                map_agent_attempt_row,
            )
            .optional()
            .map_err(|error| map_read_error("read agent attempt", error))
    }

    pub fn insert_question_candidate(&self, input: &NewQuestionCandidate) -> DatabaseResult<i64> {
        require_text(&input.question, "问题候选不能为空")?;
        require_text(&input.source, "问题候选来源不能为空")?;
        require_text(&input.spoiler_level, "问题候选剧透等级不能为空")?;
        require_text(&input.dedupe_fingerprint, "问题候选指纹不能为空")?;
        self.connection
            .execute(
                "INSERT INTO question_candidates(
                    episode_id, chapter_id, task_id, question, source, spoiler_level,
                    batch_key, dedupe_fingerprint, is_user_defined, selected_at_ms, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    input.episode_id,
                    input.chapter_id,
                    input.task_id,
                    input.question,
                    input.source,
                    input.spoiler_level,
                    input.batch_key,
                    input.dedupe_fingerprint,
                    input.is_user_defined,
                    input.selected_at_ms,
                    now_ms(),
                ],
            )
            .map(|_| self.connection.last_insert_rowid())
            .map_err(|error| map_write_error("insert question candidate", error))
    }

    pub fn get_question_candidate(
        &self,
        id: i64,
    ) -> DatabaseResult<Option<QuestionCandidateRecord>> {
        self.connection
            .query_row(
                "SELECT id, episode_id, chapter_id, task_id, question, source, spoiler_level,
                        batch_key, dedupe_fingerprint, is_user_defined, selected_at_ms, created_at_ms
                 FROM question_candidates WHERE id = ?1",
                params![id],
                map_question_candidate_row,
            )
            .optional()
            .map_err(|error| map_read_error("read question candidate", error))
    }

    /// Read all question candidates projected for one episode.
    pub fn list_question_candidates_by_episode(
        &self,
        episode_id: i64,
    ) -> DatabaseResult<Vec<QuestionCandidateRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, episode_id, chapter_id, task_id, question, source, spoiler_level,
                        batch_key, dedupe_fingerprint, is_user_defined, selected_at_ms, created_at_ms
                 FROM question_candidates
                 WHERE episode_id = ?1
                 ORDER BY created_at_ms ASC, id ASC",
            )
            .map_err(|error| map_read_error("prepare question candidates", error))?;
        let rows = statement
            .query_map(params![episode_id], map_question_candidate_row)
            .map_err(|error| map_read_error("list question candidates", error))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| map_read_error("read question candidates", error))
    }

    /// Read question candidates for one chapter without requiring callers to
    /// load and filter the entire episode projection.  The chapter detail
    /// boundary uses this method to keep the UI projection episode-scoped and
    /// chapter-specific while preserving the existing schema.
    pub fn list_question_candidates_by_chapter(
        &self,
        chapter_id: i64,
    ) -> DatabaseResult<Vec<QuestionCandidateRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, episode_id, chapter_id, task_id, question, source, spoiler_level,
                        batch_key, dedupe_fingerprint, is_user_defined, selected_at_ms, created_at_ms
                 FROM question_candidates
                 WHERE chapter_id = ?1
                 ORDER BY created_at_ms ASC, id ASC",
            )
            .map_err(|error| map_read_error("prepare question candidates by chapter", error))?;
        let rows = statement
            .query_map(params![chapter_id], map_question_candidate_row)
            .map_err(|error| map_read_error("list question candidates by chapter", error))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| map_read_error("read question candidates by chapter", error))
    }

    pub fn insert_question_candidate_for_agent_task(
        &self,
        task_id: i64,
        episode_id: i64,
        input: &NewQuestionCandidate,
    ) -> DatabaseResult<QuestionCandidateRecord> {
        let chapter_id = input
            .chapter_id
            .ok_or_else(|| DatabaseError::invalid_input("问题候选必须关联章节"))?;
        self.ensure_agent_task_chapter_scope(task_id, episode_id, chapter_id)?;
        if input.episode_id != Some(episode_id) || input.task_id != Some(task_id) {
            return Err(DatabaseError::invalid_input("问题候选超出章节任务范围"));
        }
        require_text(&input.dedupe_fingerprint, "问题候选指纹不能为空")?;
        self.connection
            .execute(
                "INSERT INTO question_candidates(
                    episode_id, chapter_id, task_id, question, source, spoiler_level,
                    batch_key, dedupe_fingerprint, is_user_defined, selected_at_ms, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(dedupe_fingerprint) DO UPDATE SET
                    episode_id = excluded.episode_id,
                    chapter_id = excluded.chapter_id,
                    task_id = excluded.task_id,
                    question = excluded.question,
                    source = excluded.source,
                    spoiler_level = excluded.spoiler_level,
                    batch_key = excluded.batch_key,
                    is_user_defined = excluded.is_user_defined,
                    selected_at_ms = excluded.selected_at_ms",
                params![
                    input.episode_id,
                    input.chapter_id,
                    input.task_id,
                    input.question,
                    input.source,
                    input.spoiler_level,
                    input.batch_key,
                    input.dedupe_fingerprint,
                    input.is_user_defined,
                    input.selected_at_ms,
                    now_ms(),
                ],
            )
            .map_err(|error| map_write_error("insert question candidate for task", error))?;
        self.connection
            .query_row(
                "SELECT id, episode_id, chapter_id, task_id, question, source, spoiler_level,
                        batch_key, dedupe_fingerprint, is_user_defined, selected_at_ms, created_at_ms
                 FROM question_candidates WHERE dedupe_fingerprint = ?1",
                params![input.dedupe_fingerprint],
                map_question_candidate_row,
            )
            .map_err(|error| map_read_error("read question candidate after idempotent insert", error))
    }

    pub fn remove_stale_draft_questions_for_agent_task(
        &self,
        task_id: i64,
        episode_id: i64,
        chapter_id: i64,
        batch_key: &str,
        keep_fingerprints: &[String],
    ) -> DatabaseResult<()> {
        let candidates = self.list_question_candidates_by_episode(episode_id)?;
        for candidate in candidates {
            if candidate.task_id == Some(task_id)
                && candidate.chapter_id == Some(chapter_id)
                && candidate.batch_key.as_deref() == Some(batch_key)
                && candidate.selected_at_ms.is_none()
                && !keep_fingerprints.contains(&candidate.dedupe_fingerprint)
            {
                self.connection
                    .execute(
                        "DELETE FROM question_candidates WHERE id = ?1",
                        params![candidate.id],
                    )
                    .map_err(|error| map_write_error("remove stale draft question", error))?;
            }
        }
        Ok(())
    }

    pub fn insert_watch_feed_item(&self, input: &NewWatchFeedItem) -> DatabaseResult<i64> {
        require_text(&input.item_type, "信息流项目类型不能为空")?;
        require_text(&input.source, "信息流项目来源不能为空")?;
        require_text(&input.content, "信息流项目内容不能为空")?;
        require_text(&input.spoiler_level, "信息流项目剧透等级不能为空")?;
        require_text(&input.content_version, "信息流内容版本不能为空")?;
        require_text(&input.dedupe_key, "信息流项目幂等键不能为空")?;
        self.connection
            .execute(
                "INSERT INTO watch_feed_items(
                    episode_id, chapter_id, revision_id, task_id, item_type, source,
                    content, spoiler_level, content_version, dedupe_key, published_at_ms, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    input.episode_id,
                    input.chapter_id,
                    input.revision_id,
                    input.task_id,
                    input.item_type,
                    input.source,
                    input.content,
                    input.spoiler_level,
                    input.content_version,
                    input.dedupe_key,
                    input.published_at_ms,
                    now_ms(),
                ],
            )
            .map(|_| self.connection.last_insert_rowid())
            .map_err(|error| map_write_error("insert watch feed item", error))
    }

    pub fn get_watch_feed_item(&self, id: i64) -> DatabaseResult<Option<WatchFeedItemRecord>> {
        self.connection
            .query_row(
                "SELECT id, episode_id, chapter_id, revision_id, task_id, item_type, source,
                        content, spoiler_level, content_version, dedupe_key, published_at_ms, created_at_ms
                 FROM watch_feed_items WHERE id = ?1",
                params![id],
                map_watch_feed_item_row,
            )
            .optional()
            .map_err(|error| map_read_error("read watch feed item", error))
    }

    /// Read the durable watch-feed projection for one episode only.
    pub fn list_watch_feed_items_by_episode(
        &self,
        episode_id: i64,
    ) -> DatabaseResult<Vec<WatchFeedItemRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, episode_id, chapter_id, revision_id, task_id, item_type, source,
                        content, spoiler_level, content_version, dedupe_key, published_at_ms, created_at_ms
                 FROM watch_feed_items
                 WHERE episode_id = ?1
                 ORDER BY COALESCE(published_at_ms, created_at_ms) ASC, id ASC",
            )
            .map_err(|error| map_read_error("prepare watch feed items", error))?;
        let rows = statement
            .query_map(params![episode_id], map_watch_feed_item_row)
            .map_err(|error| map_read_error("list watch feed items", error))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| map_read_error("read watch feed items", error))
    }

    pub fn insert_draft_feed_item_for_agent_task(
        &self,
        task_id: i64,
        episode_id: i64,
        input: &NewWatchFeedItem,
    ) -> DatabaseResult<WatchFeedItemRecord> {
        let chapter_id = input
            .chapter_id
            .ok_or_else(|| DatabaseError::invalid_input("章节信息流必须关联章节"))?;
        self.ensure_agent_task_chapter_scope(task_id, episode_id, chapter_id)?;
        if input.episode_id != Some(episode_id) || input.task_id != Some(task_id) {
            return Err(DatabaseError::invalid_input("信息流草稿超出章节任务范围"));
        }
        let existing = self
            .connection
            .query_row(
                "SELECT id, episode_id, chapter_id, revision_id, task_id, item_type, source,
                        content, spoiler_level, content_version, dedupe_key, published_at_ms, created_at_ms
                 FROM watch_feed_items WHERE dedupe_key = ?1",
                params![input.dedupe_key],
                map_watch_feed_item_row,
            )
            .optional()
            .map_err(|error| map_read_error("read stable draft feed item", error))?;
        if existing
            .as_ref()
            .is_some_and(|item| item.published_at_ms.is_some())
        {
            return Err(DatabaseError::invalid_input(
                "已发布信息流不能被草稿重试覆盖",
            ));
        }
        if existing.is_some() {
            self.connection
                .execute(
                    "UPDATE watch_feed_items
                     SET episode_id = ?1, chapter_id = ?2, revision_id = ?3,
                         task_id = ?4, item_type = ?5, source = ?6, content = ?7,
                         spoiler_level = ?8, content_version = ?9, published_at_ms = NULL
                     WHERE dedupe_key = ?10",
                    params![
                        input.episode_id,
                        input.chapter_id,
                        input.revision_id,
                        input.task_id,
                        input.item_type,
                        input.source,
                        input.content,
                        input.spoiler_level,
                        input.content_version,
                        input.dedupe_key,
                    ],
                )
                .map(|_| ())
                .map_err(|error| map_write_error("update draft feed item", error))?;
        } else {
            self.connection
                .execute(
                    "INSERT INTO watch_feed_items(
                        episode_id, chapter_id, revision_id, task_id, item_type, source,
                        content, spoiler_level, content_version, dedupe_key, published_at_ms, created_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        input.episode_id,
                        input.chapter_id,
                        input.revision_id,
                        input.task_id,
                        input.item_type,
                        input.source,
                        input.content,
                        input.spoiler_level,
                        input.content_version,
                        input.dedupe_key,
                        input.published_at_ms,
                        now_ms(),
                    ],
                )
                .map(|_| ())
                .map_err(|error| map_write_error("insert draft feed item for task", error))?;
        }
        self.connection
            .query_row(
                "SELECT id, episode_id, chapter_id, revision_id, task_id, item_type, source,
                        content, spoiler_level, content_version, dedupe_key, published_at_ms, created_at_ms
                 FROM watch_feed_items WHERE dedupe_key = ?1",
                params![input.dedupe_key],
                map_watch_feed_item_row,
            )
            .map_err(|error| map_read_error("read feed item after idempotent insert", error))
    }

    pub fn remove_stale_draft_feed_items_for_agent_task(
        &self,
        task_id: i64,
        episode_id: i64,
        chapter_id: i64,
        draft_key: &str,
        keep_item_types: &[String],
    ) -> DatabaseResult<()> {
        let prefix = format!("chapter-feed:{task_id}:{chapter_id}:{draft_key}:");
        let items = self.list_watch_feed_items_by_episode(episode_id)?;
        for item in items {
            if item.task_id == Some(task_id)
                && item.chapter_id == Some(chapter_id)
                && item.dedupe_key.starts_with(&prefix)
                && item.published_at_ms.is_none()
                && !keep_item_types.contains(&item.item_type)
            {
                self.connection
                    .execute(
                        "DELETE FROM watch_feed_items WHERE id = ?1",
                        params![item.id],
                    )
                    .map_err(|error| map_write_error("remove stale draft feed item", error))?;
            }
        }
        Ok(())
    }

    /// Validate and atomically publish every chapter owned by the task.  The
    /// caller must invoke this inside `Database::transaction`.
    pub fn publish_agent_chapter_task(
        &self,
        task_id: i64,
        attempt_id: i64,
        episode_id: i64,
        duration_ms: i64,
        output_json: &str,
    ) -> DatabaseResult<Vec<ChapterRecord>> {
        if duration_ms <= 0 {
            return Err(DatabaseError::invalid_input("媒体时长必须大于零"));
        }
        let task = self
            .get_agent_task(task_id)?
            .ok_or_else(|| DatabaseError::invalid_input("章节任务不存在"))?;
        if !task.task_type.starts_with("chapter") || task.episode_id != Some(episode_id) {
            return Err(DatabaseError::invalid_input("章节任务与集数不匹配"));
        }
        let attempt = self
            .get_agent_attempt(attempt_id)?
            .ok_or_else(|| DatabaseError::invalid_input("章节任务尝试不存在"))?;
        if attempt.task_id != task_id {
            return Err(DatabaseError::invalid_input("任务尝试不属于当前章节任务"));
        }
        require_text(output_json, "章节任务输出不能为空")?;

        let chapters = self.list_chapters_by_agent_task(task_id, episode_id)?;
        if chapters.is_empty() {
            return Err(DatabaseError::invalid_input("章节任务尚未创建章节大纲"));
        }
        let mut previous_end = 0_i64;
        for chapter in &chapters {
            if chapter.start_ms < previous_end || chapter.end_ms > duration_ms {
                return Err(DatabaseError::invalid_input("章节时间范围无效或互相重叠"));
            }
            if chapter
                .title
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
                || chapter
                    .mainline
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty())
            {
                return Err(DatabaseError::invalid_input("章节草稿缺少标题或主线"));
            }
            if self.list_chapter_assets_by_chapter(chapter.id)?.is_empty() {
                return Err(DatabaseError::invalid_input("章节草稿缺少画面证据"));
            }
            if self.get_latest_chapter_revision(chapter.id)?.is_none() {
                return Err(DatabaseError::invalid_input("章节草稿缺少版本记录"));
            }
            previous_end = chapter.end_ms;
        }

        self.connection
            .execute(
                "UPDATE episode_chapters
                 SET status = 'ready', updated_at_ms = ?1
                 WHERE id IN (SELECT chapter_id FROM agent_task_chapters WHERE task_id = ?2)
                   AND episode_id = ?3",
                params![now_ms(), task_id, episode_id],
            )
            .map_err(|error| map_write_error("publish chapter rows", error))?;
        self.connection
            .execute(
                "UPDATE chapter_revisions SET status = 'accepted'
                 WHERE chapter_id IN (SELECT chapter_id FROM agent_task_chapters WHERE task_id = ?1)",
                params![task_id],
            )
            .map_err(|error| map_write_error("publish chapter revisions", error))?;
        self.connection
            .execute(
                "UPDATE watch_feed_items
                 SET published_at_ms = COALESCE(published_at_ms, ?1)
                 WHERE task_id = ?2 AND episode_id = ?3",
                params![now_ms(), task_id, episode_id],
            )
            .map_err(|error| map_write_error("publish chapter feed", error))?;
        if !self.complete_agent_task(
            task_id,
            "succeeded",
            task.attempt_count,
            task.retry_count,
            None,
            output_json,
        )? {
            return Err(DatabaseError::invalid_input("章节任务状态更新失败"));
        }
        if !self.update_agent_attempt_status(attempt_id, "succeeded", None, now_ms())? {
            return Err(DatabaseError::invalid_input("章节任务尝试状态更新失败"));
        }
        self.list_chapters_by_agent_task(task_id, episode_id)
    }

    pub fn set_app_setting(&self, key: &str, value_json: &str) -> DatabaseResult<()> {
        require_text(key, "设置键不能为空")?;
        require_text(value_json, "设置值不能为空")?;
        self.connection
            .execute(
                "INSERT INTO app_settings(key, value_json, updated_at_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                 updated_at_ms = excluded.updated_at_ms",
                params![key, value_json, now_ms()],
            )
            .map(|_| ())
            .map_err(|error| map_write_error("write app setting", error))
    }

    pub fn get_app_setting(&self, key: &str) -> DatabaseResult<Option<String>> {
        self.connection
            .query_row(
                "SELECT value_json FROM app_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| map_read_error("read app setting", error))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSeries {
    pub stable_id: String,
    pub title: String,
    pub source: String,
}

impl NewSeries {
    pub fn new(
        stable_id: impl Into<String>,
        title: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            stable_id: stable_id.into(),
            title: title.into(),
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEpisode {
    pub series_id: i64,
    pub stable_id: String,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub title: Option<String>,
    pub duration_ms: Option<i64>,
    pub source: String,
}

impl NewEpisode {
    pub fn new(series_id: i64, stable_id: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            series_id,
            stable_id: stable_id.into(),
            season_number: None,
            episode_number: None,
            title: None,
            duration_ms: None,
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChapter {
    pub episode_id: i64,
    pub stable_id: String,
    pub source: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub spoiler_level: String,
    pub title: Option<String>,
    pub mainline: Option<String>,
    pub status: String,
}

impl NewChapter {
    pub fn new(
        episode_id: i64,
        stable_id: impl Into<String>,
        start_ms: i64,
        end_ms: i64,
        source: impl Into<String>,
    ) -> Self {
        Self {
            episode_id,
            stable_id: stable_id.into(),
            source: source.into(),
            start_ms,
            end_ms,
            spoiler_level: "none".to_string(),
            title: None,
            mainline: None,
            status: "draft".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChapterAsset {
    pub chapter_id: i64,
    pub asset_type: String,
    pub path: String,
    pub content_hash: String,
    pub captured_at_ms: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub source: String,
}

impl NewChapterAsset {
    pub fn new(
        chapter_id: i64,
        asset_type: impl Into<String>,
        path: impl Into<String>,
        content_hash: impl Into<String>,
        captured_at_ms: i64,
        source: impl Into<String>,
    ) -> Self {
        Self {
            chapter_id,
            asset_type: asset_type.into(),
            path: path.into(),
            content_hash: content_hash.into(),
            captured_at_ms,
            width: None,
            height: None,
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChapterRevision {
    pub chapter_id: i64,
    pub revision_number: i64,
    pub revision_type: String,
    pub content: String,
    pub source: String,
    pub prompt_version: String,
    pub validation_report: Option<String>,
    pub status: String,
}

impl NewChapterRevision {
    pub fn new(
        chapter_id: i64,
        revision_number: i64,
        revision_type: impl Into<String>,
        content: impl Into<String>,
        source: impl Into<String>,
        prompt_version: impl Into<String>,
    ) -> Self {
        Self {
            chapter_id,
            revision_number,
            revision_type: revision_type.into(),
            content: content.into(),
            source: source.into(),
            prompt_version: prompt_version.into(),
            validation_report: None,
            status: "draft".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentTask {
    pub task_key: String,
    pub task_type: String,
    pub episode_id: Option<i64>,
    pub chapter_id: Option<i64>,
    pub status: String,
    pub session_id: Option<String>,
    pub prompt_version: String,
    pub output_contract_version: Option<String>,
    pub max_attempts: i64,
    pub validation_report: Option<String>,
}

impl NewAgentTask {
    pub fn new(
        task_key: impl Into<String>,
        task_type: impl Into<String>,
        prompt_version: impl Into<String>,
    ) -> Self {
        Self {
            task_key: task_key.into(),
            task_type: task_type.into(),
            episode_id: None,
            chapter_id: None,
            status: "pending".to_string(),
            session_id: None,
            prompt_version: prompt_version.into(),
            output_contract_version: None,
            max_attempts: 3,
            validation_report: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentAttempt {
    pub task_id: i64,
    pub attempt_number: i64,
    pub attempt_kind: String,
    pub status: String,
    pub prompt_version: String,
    pub validation_report: Option<String>,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
}

impl NewAgentAttempt {
    pub fn new(
        task_id: i64,
        attempt_number: i64,
        attempt_kind: impl Into<String>,
        status: impl Into<String>,
        prompt_version: impl Into<String>,
        started_at_ms: i64,
    ) -> Self {
        Self {
            task_id,
            attempt_number,
            attempt_kind: attempt_kind.into(),
            status: status.into(),
            prompt_version: prompt_version.into(),
            validation_report: None,
            started_at_ms,
            finished_at_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewQuestionCandidate {
    pub episode_id: Option<i64>,
    pub chapter_id: Option<i64>,
    pub task_id: Option<i64>,
    pub question: String,
    pub source: String,
    pub spoiler_level: String,
    pub batch_key: Option<String>,
    pub dedupe_fingerprint: String,
    pub is_user_defined: bool,
    pub selected_at_ms: Option<i64>,
}

impl NewQuestionCandidate {
    pub fn new(
        question: impl Into<String>,
        source: impl Into<String>,
        spoiler_level: impl Into<String>,
        dedupe_fingerprint: impl Into<String>,
    ) -> Self {
        Self {
            episode_id: None,
            chapter_id: None,
            task_id: None,
            question: question.into(),
            source: source.into(),
            spoiler_level: spoiler_level.into(),
            batch_key: None,
            dedupe_fingerprint: dedupe_fingerprint.into(),
            is_user_defined: false,
            selected_at_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWatchFeedItem {
    pub episode_id: Option<i64>,
    pub chapter_id: Option<i64>,
    pub revision_id: Option<i64>,
    pub task_id: Option<i64>,
    pub item_type: String,
    pub source: String,
    pub content: String,
    pub spoiler_level: String,
    pub content_version: String,
    pub dedupe_key: String,
    pub published_at_ms: Option<i64>,
}

impl NewWatchFeedItem {
    pub fn new(
        item_type: impl Into<String>,
        source: impl Into<String>,
        content: impl Into<String>,
        spoiler_level: impl Into<String>,
        content_version: impl Into<String>,
        dedupe_key: impl Into<String>,
    ) -> Self {
        Self {
            episode_id: None,
            chapter_id: None,
            revision_id: None,
            task_id: None,
            item_type: item_type.into(),
            source: source.into(),
            content: content.into(),
            spoiler_level: spoiler_level.into(),
            content_version: content_version.into(),
            dedupe_key: dedupe_key.into(),
            published_at_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesRecord {
    pub id: i64,
    pub stable_id: String,
    pub title: String,
    pub source: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeRecord {
    pub id: i64,
    pub series_id: i64,
    pub stable_id: String,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub title: Option<String>,
    pub duration_ms: Option<i64>,
    pub source: String,
    pub watch_position_ms: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterRecord {
    pub id: i64,
    pub episode_id: i64,
    pub stable_id: String,
    pub source: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub spoiler_level: String,
    pub title: Option<String>,
    pub mainline: Option<String>,
    pub status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterAssetRecord {
    pub id: i64,
    pub chapter_id: i64,
    pub asset_type: String,
    pub path: String,
    pub content_hash: String,
    pub captured_at_ms: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub source: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterRevisionRecord {
    pub id: i64,
    pub chapter_id: i64,
    pub revision_number: i64,
    pub revision_type: String,
    pub content: String,
    pub source: String,
    pub prompt_version: String,
    pub validation_report: Option<String>,
    pub status: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskRecord {
    pub id: i64,
    pub task_key: String,
    pub task_type: String,
    pub episode_id: Option<i64>,
    pub chapter_id: Option<i64>,
    pub status: String,
    pub session_id: Option<String>,
    pub prompt_version: String,
    pub output_contract_version: Option<String>,
    pub attempt_count: i64,
    pub retry_count: i64,
    pub max_attempts: i64,
    pub validation_report: Option<String>,
    pub output_json: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAttemptRecord {
    pub id: i64,
    pub task_id: i64,
    pub attempt_number: i64,
    pub attempt_kind: String,
    pub status: String,
    pub prompt_version: String,
    pub validation_report: Option<String>,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionCandidateRecord {
    pub id: i64,
    pub episode_id: Option<i64>,
    pub chapter_id: Option<i64>,
    pub task_id: Option<i64>,
    pub question: String,
    pub source: String,
    pub spoiler_level: String,
    pub batch_key: Option<String>,
    pub dedupe_fingerprint: String,
    pub is_user_defined: bool,
    pub selected_at_ms: Option<i64>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchFeedItemRecord {
    pub id: i64,
    pub episode_id: Option<i64>,
    pub chapter_id: Option<i64>,
    pub revision_id: Option<i64>,
    pub task_id: Option<i64>,
    pub item_type: String,
    pub source: String,
    pub content: String,
    pub spoiler_level: String,
    pub content_version: String,
    pub dedupe_key: String,
    pub published_at_ms: Option<i64>,
    pub created_at_ms: i64,
}

/// Durable chat snapshot owned by one Agent profile/session pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSnapshotRecord {
    pub profile_id: String,
    pub session_id: String,
    pub cwd: Option<String>,
    pub draft: String,
    pub turns_json: String,
    pub updated_at_ms: i64,
}

/// Single resume hint owned by one Agent profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpSessionHintRecord {
    pub profile_id: String,
    pub session_id: String,
    pub cwd: String,
    pub updated_at_ms: i64,
}

fn map_series_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SeriesRecord> {
    Ok(SeriesRecord {
        id: row.get(0)?,
        stable_id: row.get(1)?,
        title: row.get(2)?,
        source: row.get(3)?,
        created_at_ms: row.get(4)?,
        updated_at_ms: row.get(5)?,
    })
}

fn map_episode_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EpisodeRecord> {
    Ok(EpisodeRecord {
        id: row.get(0)?,
        series_id: row.get(1)?,
        stable_id: row.get(2)?,
        season_number: row.get(3)?,
        episode_number: row.get(4)?,
        title: row.get(5)?,
        duration_ms: row.get(6)?,
        source: row.get(7)?,
        watch_position_ms: row.get(8)?,
        created_at_ms: row.get(9)?,
        updated_at_ms: row.get(10)?,
    })
}

fn map_chapter_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChapterRecord> {
    Ok(ChapterRecord {
        id: row.get(0)?,
        episode_id: row.get(1)?,
        stable_id: row.get(2)?,
        source: row.get(3)?,
        start_ms: row.get(4)?,
        end_ms: row.get(5)?,
        spoiler_level: row.get(6)?,
        title: row.get(7)?,
        mainline: row.get(8)?,
        status: row.get(9)?,
        created_at_ms: row.get(10)?,
        updated_at_ms: row.get(11)?,
    })
}

fn map_chapter_asset_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChapterAssetRecord> {
    Ok(ChapterAssetRecord {
        id: row.get(0)?,
        chapter_id: row.get(1)?,
        asset_type: row.get(2)?,
        path: row.get(3)?,
        content_hash: row.get(4)?,
        captured_at_ms: row.get(5)?,
        width: row.get(6)?,
        height: row.get(7)?,
        source: row.get(8)?,
        created_at_ms: row.get(9)?,
    })
}

fn map_chapter_revision_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChapterRevisionRecord> {
    Ok(ChapterRevisionRecord {
        id: row.get(0)?,
        chapter_id: row.get(1)?,
        revision_number: row.get(2)?,
        revision_type: row.get(3)?,
        content: row.get(4)?,
        source: row.get(5)?,
        prompt_version: row.get(6)?,
        validation_report: row.get(7)?,
        status: row.get(8)?,
        created_at_ms: row.get(9)?,
    })
}

fn map_agent_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentTaskRecord> {
    Ok(AgentTaskRecord {
        id: row.get(0)?,
        task_key: row.get(1)?,
        task_type: row.get(2)?,
        episode_id: row.get(3)?,
        chapter_id: row.get(4)?,
        status: row.get(5)?,
        session_id: row.get(6)?,
        prompt_version: row.get(7)?,
        output_contract_version: row.get(8)?,
        attempt_count: row.get(9)?,
        retry_count: row.get(10)?,
        max_attempts: row.get(11)?,
        validation_report: row.get(12)?,
        output_json: row.get(13)?,
        created_at_ms: row.get(14)?,
        updated_at_ms: row.get(15)?,
    })
}

fn map_agent_attempt_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentAttemptRecord> {
    Ok(AgentAttemptRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        attempt_number: row.get(2)?,
        attempt_kind: row.get(3)?,
        status: row.get(4)?,
        prompt_version: row.get(5)?,
        validation_report: row.get(6)?,
        started_at_ms: row.get(7)?,
        finished_at_ms: row.get(8)?,
        created_at_ms: row.get(9)?,
    })
}

fn map_question_candidate_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<QuestionCandidateRecord> {
    Ok(QuestionCandidateRecord {
        id: row.get(0)?,
        episode_id: row.get(1)?,
        chapter_id: row.get(2)?,
        task_id: row.get(3)?,
        question: row.get(4)?,
        source: row.get(5)?,
        spoiler_level: row.get(6)?,
        batch_key: row.get(7)?,
        dedupe_fingerprint: row.get(8)?,
        is_user_defined: row.get(9)?,
        selected_at_ms: row.get(10)?,
        created_at_ms: row.get(11)?,
    })
}

fn map_watch_feed_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WatchFeedItemRecord> {
    Ok(WatchFeedItemRecord {
        id: row.get(0)?,
        episode_id: row.get(1)?,
        chapter_id: row.get(2)?,
        revision_id: row.get(3)?,
        task_id: row.get(4)?,
        item_type: row.get(5)?,
        source: row.get(6)?,
        content: row.get(7)?,
        spoiler_level: row.get(8)?,
        content_version: row.get(9)?,
        dedupe_key: row.get(10)?,
        published_at_ms: row.get(11)?,
        created_at_ms: row.get(12)?,
    })
}

fn map_chat_snapshot_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatSnapshotRecord> {
    Ok(ChatSnapshotRecord {
        profile_id: row.get(0)?,
        session_id: row.get(1)?,
        cwd: row.get(2)?,
        draft: row.get(3)?,
        turns_json: row.get(4)?,
        updated_at_ms: row.get(5)?,
    })
}

fn map_session_hint_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AcpSessionHintRecord> {
    Ok(AcpSessionHintRecord {
        profile_id: row.get(0)?,
        session_id: row.get(1)?,
        cwd: row.get(2)?,
        updated_at_ms: row.get(3)?,
    })
}

fn episode_identity_for_media(
    index: &LibraryIndex,
    media_path: &Path,
) -> DatabaseResult<Option<(String, String)>> {
    require_text(&index.root, "媒体库根目录不能为空")?;
    let media_key = normalized_path_key(media_path);
    let Some(file) = index.files.iter().find(|file| {
        normalized_path_key(&Path::new(&index.root).join(&file.relative_path)) == media_key
    }) else {
        return Ok(None);
    };

    let Some(group) = index
        .groups
        .iter()
        .find(|group| group.key == file.group_key)
    else {
        return Ok(None);
    };
    if group.kind != MediaGroupKind::Series {
        return Ok(None);
    }

    let GroupResolution::Matched {
        tmdb_id,
        media_type: MetadataMediaType::Tv,
    } = &group.resolution
    else {
        // A local group name or a media path is not enough to prove series
        // identity.  Leave it unresolved until the library has an explicit
        // match or another trusted identity source.
        return Ok(None);
    };
    let (Some(season), Some(episode)) = (file.season, file.episode) else {
        // Do not collapse a bare episode number into season 1 here.  That
        // naming convention is useful for display, but is not authoritative
        // enough for a durable cross-path identity.
        return Ok(None);
    };
    if season == 0 || episode == 0 {
        return Ok(None);
    }

    Ok(Some((
        format!("tmdb:tv:{tmdb_id}"),
        format!("s{season:02}e{episode:02}"),
    )))
}

fn normalized_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn validate_episode_migration(input: &LegacyEpisodeMigration) -> DatabaseResult<()> {
    if input.legacy_episode_id <= 0 || input.authoritative_episode_id <= 0 {
        return Err(DatabaseError::invalid_input("剧集迁移标识无效"));
    }
    if input.legacy_episode_id == input.authoritative_episode_id {
        return Err(DatabaseError::invalid_input(
            "旧剧集与权威剧集不能是同一条记录",
        ));
    }
    for migration in &input.task_key_migrations {
        if migration.task_id <= 0 {
            return Err(DatabaseError::invalid_input("任务迁移标识无效"));
        }
        require_text(&migration.new_task_key, "迁移后的任务标识不能为空")?;
    }
    Ok(())
}

fn validate_new_agent_task(input: &NewAgentTask) -> DatabaseResult<()> {
    require_text(&input.task_key, "任务标识不能为空")?;
    require_text(&input.task_type, "任务类型不能为空")?;
    require_text(&input.status, "任务状态不能为空")?;
    require_text(&input.prompt_version, "提示词版本不能为空")?;
    if input.max_attempts < 1 {
        return Err(DatabaseError::invalid_input("任务最大尝试次数必须大于零"));
    }
    Ok(())
}

fn validate_new_episode(input: &NewEpisode) -> DatabaseResult<()> {
    if input.series_id <= 0 {
        return Err(DatabaseError::invalid_input("剧集所属标识无效"));
    }
    require_text(&input.stable_id, "剧集集数标识不能为空")?;
    require_text(&input.source, "剧集来源不能为空")?;
    if input.season_number.is_some_and(|season| season <= 0)
        || input.episode_number.is_some_and(|episode| episode <= 0)
    {
        return Err(DatabaseError::invalid_input("剧集季集编号必须大于零"));
    }
    Ok(())
}

fn is_active_agent_task(task: &AgentTaskRecord) -> bool {
    matches!(
        task.status.as_str(),
        "pending" | "running" | "validation_failure"
    ) && task.attempt_count < task.max_attempts
}

fn map_write_error(operation: &str, error: rusqlite::Error) -> DatabaseError {
    if error
        .to_string()
        .to_ascii_lowercase()
        .contains("constraint")
    {
        DatabaseError::sqlite(
            DatabaseErrorCode::ConstraintViolation,
            "应用数据存储约束不满足，请检查输入",
            operation,
            error,
        )
    } else {
        DatabaseError::sqlite(
            DatabaseErrorCode::QueryFailed,
            "应用数据存储写入失败，请重试",
            operation,
            error,
        )
    }
}

fn map_read_error(operation: &str, error: rusqlite::Error) -> DatabaseError {
    DatabaseError::sqlite(
        DatabaseErrorCode::QueryFailed,
        "应用数据存储读取失败，请重试",
        operation,
        error,
    )
}

fn require_text(value: &str, message: &'static str) -> DatabaseResult<()> {
    if value.trim().is_empty() {
        Err(DatabaseError::invalid_input(message))
    } else {
        Ok(())
    }
}

/// Snapshot keys are short caller-owned identifiers, never paths.
const MAX_SNAPSHOT_KEY_LEN: usize = 256;
/// Working directories stay small; overlong values are rejected, not trimmed.
const MAX_CWD_LEN: usize = 4096;
/// Drafts are plain text edited by the user; anything larger is a caller bug.
const MAX_SNAPSHOT_DRAFT_LEN: usize = 200_000;
/// Turn history is pruned by the caller; the store only rejects absurd sizes.
const MAX_SNAPSHOT_TURNS_LEN: usize = 1_000_000;

fn validate_snapshot_identity(profile_id: &str, session_id: &str) -> DatabaseResult<()> {
    require_text(profile_id, "Agent 配置标识不能为空")?;
    require_key_length(profile_id, "Agent 配置标识过长，无法保存")?;
    require_key_length(session_id, "Agent 会话标识过长，无法保存")?;
    Ok(())
}

fn validate_snapshot_payload(draft: &str, turns_json: &str) -> DatabaseResult<()> {
    if draft.trim().is_empty() {
        return Err(DatabaseError::invalid_input("聊天草稿不能为空"));
    }
    if draft.len() > MAX_SNAPSHOT_DRAFT_LEN {
        return Err(DatabaseError::invalid_input("聊天草稿过长，无法保存"));
    }
    if turns_json.trim().is_empty() {
        return Err(DatabaseError::invalid_input("聊天记录不能为空"));
    }
    if turns_json.len() > MAX_SNAPSHOT_TURNS_LEN {
        return Err(DatabaseError::invalid_input("聊天记录过长，无法保存"));
    }
    let parsed: serde_json::Value = serde_json::from_str(turns_json)
        .map_err(|_| DatabaseError::invalid_input("聊天记录格式无效，无法保存"))?;
    if !parsed.is_array() {
        return Err(DatabaseError::invalid_input("聊天记录格式无效，无法保存"));
    }
    Ok(())
}

fn require_key_length(value: &str, message: &'static str) -> DatabaseResult<()> {
    if value.len() > MAX_SNAPSHOT_KEY_LEN {
        Err(DatabaseError::invalid_input(message))
    } else {
        Ok(())
    }
}

fn require_cwd_length(cwd: &str) -> DatabaseResult<()> {
    if cwd.len() > MAX_CWD_LEN {
        Err(DatabaseError::invalid_input("工作目录无效，无法保存"))
    } else {
        Ok(())
    }
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn validate_time_bounds(start_ms: i64, end_ms: i64) -> DatabaseResult<()> {
    if start_ms < 0 || end_ms <= start_ms {
        return Err(DatabaseError::invalid_input("章节结束时间必须晚于开始时间"));
    }
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().min(i64::MAX as u128) as i64
        })
}

const MIGRATION_1_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS series (
    id INTEGER PRIMARY KEY,
    stable_id TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    source TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS episodes (
    id INTEGER PRIMARY KEY,
    series_id INTEGER NOT NULL,
    stable_id TEXT NOT NULL,
    season_number INTEGER,
    episode_number INTEGER,
    title TEXT,
    duration_ms INTEGER,
    source TEXT NOT NULL,
    watch_position_ms INTEGER NOT NULL DEFAULT 0 CHECK (watch_position_ms >= 0),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY (series_id) REFERENCES series(id) ON DELETE CASCADE,
    UNIQUE (series_id, stable_id)
);

CREATE TABLE IF NOT EXISTS episode_chapters (
    id INTEGER PRIMARY KEY,
    episode_id INTEGER NOT NULL,
    stable_id TEXT NOT NULL,
    source TEXT NOT NULL,
    start_ms INTEGER NOT NULL CHECK (start_ms >= 0),
    end_ms INTEGER NOT NULL CHECK (end_ms > start_ms),
    spoiler_level TEXT NOT NULL,
    title TEXT,
    mainline TEXT,
    status TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY (episode_id) REFERENCES episodes(id) ON DELETE CASCADE,
    UNIQUE (episode_id, stable_id)
);

CREATE TABLE IF NOT EXISTS chapter_assets (
    id INTEGER PRIMARY KEY,
    chapter_id INTEGER NOT NULL,
    asset_type TEXT NOT NULL,
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    captured_at_ms INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (chapter_id) REFERENCES episode_chapters(id) ON DELETE CASCADE,
    UNIQUE (chapter_id, content_hash)
);

CREATE TABLE IF NOT EXISTS chapter_revisions (
    id INTEGER PRIMARY KEY,
    chapter_id INTEGER NOT NULL,
    revision_number INTEGER NOT NULL CHECK (revision_number > 0),
    revision_type TEXT NOT NULL,
    content TEXT NOT NULL,
    source TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    validation_report TEXT,
    status TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (chapter_id) REFERENCES episode_chapters(id) ON DELETE CASCADE,
    UNIQUE (chapter_id, revision_number)
);

CREATE TABLE IF NOT EXISTS agent_tasks (
    id INTEGER PRIMARY KEY,
    task_key TEXT NOT NULL UNIQUE,
    task_type TEXT NOT NULL,
    episode_id INTEGER,
    chapter_id INTEGER,
    status TEXT NOT NULL,
    session_id TEXT,
    prompt_version TEXT NOT NULL,
    output_contract_version TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    max_attempts INTEGER NOT NULL DEFAULT 3 CHECK (max_attempts > 0),
    validation_report TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY (episode_id) REFERENCES episodes(id) ON DELETE CASCADE,
    FOREIGN KEY (chapter_id) REFERENCES episode_chapters(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS agent_attempts (
    id INTEGER PRIMARY KEY,
    task_id INTEGER NOT NULL,
    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
    attempt_kind TEXT NOT NULL,
    status TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    validation_report TEXT,
    started_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (task_id) REFERENCES agent_tasks(id) ON DELETE CASCADE,
    UNIQUE (task_id, attempt_number)
);

CREATE TABLE IF NOT EXISTS question_candidates (
    id INTEGER PRIMARY KEY,
    episode_id INTEGER,
    chapter_id INTEGER,
    task_id INTEGER,
    question TEXT NOT NULL,
    source TEXT NOT NULL,
    spoiler_level TEXT NOT NULL,
    batch_key TEXT,
    dedupe_fingerprint TEXT NOT NULL UNIQUE,
    is_user_defined INTEGER NOT NULL DEFAULT 0 CHECK (is_user_defined IN (0, 1)),
    selected_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (episode_id) REFERENCES episodes(id) ON DELETE CASCADE,
    FOREIGN KEY (chapter_id) REFERENCES episode_chapters(id) ON DELETE CASCADE,
    FOREIGN KEY (task_id) REFERENCES agent_tasks(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS watch_feed_items (
    id INTEGER PRIMARY KEY,
    episode_id INTEGER,
    chapter_id INTEGER,
    revision_id INTEGER,
    task_id INTEGER,
    item_type TEXT NOT NULL,
    source TEXT NOT NULL,
    content TEXT NOT NULL,
    spoiler_level TEXT NOT NULL,
    content_version TEXT NOT NULL,
    dedupe_key TEXT NOT NULL UNIQUE,
    published_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (episode_id) REFERENCES episodes(id) ON DELETE CASCADE,
    FOREIGN KEY (chapter_id) REFERENCES episode_chapters(id) ON DELETE CASCADE,
    FOREIGN KEY (revision_id) REFERENCES chapter_revisions(id) ON DELETE SET NULL,
    FOREIGN KEY (task_id) REFERENCES agent_tasks(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_episodes_series ON episodes(series_id);
CREATE INDEX IF NOT EXISTS idx_chapters_episode_time ON episode_chapters(episode_id, start_ms, end_ms);
CREATE INDEX IF NOT EXISTS idx_assets_chapter ON chapter_assets(chapter_id);
CREATE INDEX IF NOT EXISTS idx_revisions_chapter ON chapter_revisions(chapter_id, revision_number);
CREATE INDEX IF NOT EXISTS idx_agent_tasks_status ON agent_tasks(status, updated_at_ms);
CREATE INDEX IF NOT EXISTS idx_agent_attempts_task ON agent_attempts(task_id, attempt_number);
CREATE INDEX IF NOT EXISTS idx_questions_chapter ON question_candidates(chapter_id, created_at_ms);
CREATE INDEX IF NOT EXISTS idx_feed_episode_published ON watch_feed_items(episode_id, published_at_ms);
"#;

const MIGRATION_3_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS agent_task_chapters (
    task_id INTEGER NOT NULL,
    chapter_id INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (task_id, chapter_id),
    FOREIGN KEY (task_id) REFERENCES agent_tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (chapter_id) REFERENCES episode_chapters(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_task_chapters_chapter
    ON agent_task_chapters(chapter_id, task_id);
"#;

const MIGRATION_4_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS chat_snapshots (
    profile_id TEXT NOT NULL,
    session_id TEXT NOT NULL DEFAULT '',
    cwd TEXT,
    draft TEXT NOT NULL DEFAULT '',
    turns_json TEXT NOT NULL DEFAULT '[]',
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (profile_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_chat_snapshots_profile_updated
    ON chat_snapshots(profile_id, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS acp_session_hints (
    profile_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    cwd TEXT NOT NULL DEFAULT '',
    updated_at_ms INTEGER NOT NULL
);
"#;

#[cfg(test)]
#[allow(clippy::drop_non_drop)]
mod tests {
    use super::*;
    use crate::model::{IndexedMediaFile, MediaGroup};
    use rusqlite::params;

    fn value_or_panic<T>(result: DatabaseResult<T>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("unexpected database error: {}", error.message),
        }
    }

    fn sqlite_or_panic<T>(result: rusqlite::Result<T>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("unexpected sqlite error: {error}"),
        }
    }

    #[test]
    fn migration_is_repeatable_and_creates_the_initial_schema() {
        let mut database = value_or_panic(Database::open_in_memory());
        assert_eq!(
            value_or_panic(database.schema_version()),
            CURRENT_SCHEMA_VERSION
        );
        assert!(database.migrate().is_ok());
        assert_eq!(
            value_or_panic(database.schema_version()),
            CURRENT_SCHEMA_VERSION
        );

        for table in [
            "series",
            "episodes",
            "episode_chapters",
            "chapter_assets",
            "chapter_revisions",
            "agent_tasks",
            "agent_attempts",
            "agent_task_chapters",
            "question_candidates",
            "watch_feed_items",
            "app_settings",
            "chat_snapshots",
            "acp_session_hints",
        ] {
            assert!(
                value_or_panic(database.table_exists(table)),
                "missing {table}"
            );
        }
    }

    #[test]
    fn authoritative_tv_match_produces_path_independent_episode_identity() {
        let database = value_or_panic(Database::open_in_memory());
        let index = LibraryIndex {
            schema_version: 1,
            root: "D:/Media".into(),
            updated_at_ms: 1,
            files: vec![IndexedMediaFile {
                relative_path: "Example.Show/S01E02.mkv".into(),
                file_name: "S01E02.mkv".into(),
                size_bytes: 1,
                modified_at_ms: 1,
                group_key: "Example.Show".into(),
                season: Some(1),
                episode: Some(2),
            }],
            groups: vec![MediaGroup {
                key: "Example.Show".into(),
                display_name: "Example Show".into(),
                kind: MediaGroupKind::Series,
                files: vec!["Example.Show/S01E02.mkv".into()],
                manual_title: None,
                resolution: GroupResolution::Matched {
                    tmdb_id: 42,
                    media_type: MetadataMediaType::Tv,
                },
            }],
        };

        let identity = value_or_panic(
            database
                .episode_identity_for_media(&index, Path::new("d:/media/example.show/S01E02.mkv")),
        );
        assert_eq!(identity, Some(("tmdb:tv:42".into(), "s01e02".into())));
    }

    #[test]
    fn unresolved_group_never_falls_back_to_a_path_identity() {
        let database = value_or_panic(Database::open_in_memory());
        let index = LibraryIndex {
            schema_version: 1,
            root: "D:/Media".into(),
            updated_at_ms: 1,
            files: vec![IndexedMediaFile {
                relative_path: "Unresolved/S01E02.mkv".into(),
                file_name: "S01E02.mkv".into(),
                size_bytes: 1,
                modified_at_ms: 1,
                group_key: "Unresolved".into(),
                season: Some(1),
                episode: Some(2),
            }],
            groups: vec![MediaGroup {
                key: "Unresolved".into(),
                display_name: "Unresolved".into(),
                kind: MediaGroupKind::Series,
                files: vec!["Unresolved/S01E02.mkv".into()],
                manual_title: None,
                resolution: GroupResolution::Pending,
            }],
        };

        let identity = value_or_panic(
            database
                .episode_identity_for_media(&index, Path::new("D:/Media/Unresolved/S01E02.mkv")),
        );
        assert_eq!(identity, None);
    }

    #[test]
    fn get_or_create_episode_is_idempotent_and_identity_lookup_ignores_paths() {
        let database = value_or_panic(Database::open_in_memory());
        let repository = database.repository();
        let series_id = value_or_panic(repository.insert_series(&NewSeries::new(
            "tmdb:tv:42",
            "Example Show",
            "library",
        )));
        let mut input = NewEpisode::new(series_id, "s01e02", "local");
        input.season_number = Some(1);
        input.episode_number = Some(2);

        let first = value_or_panic(repository.get_or_create_episode(&input));
        let second = value_or_panic(repository.get_or_create_episode(&input));
        assert_eq!(first.id, second.id);
        assert_eq!(
            value_or_panic(repository.get_episode_by_identity("tmdb:tv:42", "s01e02"))
                .map(|episode| episode.id),
            Some(first.id)
        );
    }

    #[test]
    fn v1_task_data_survives_output_json_migration() {
        let connection = sqlite_or_panic(Connection::open_in_memory());
        sqlite_or_panic(connection.execute_batch(MIGRATION_1_SQL));
        sqlite_or_panic(connection.execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at_ms INTEGER NOT NULL
            );",
        ));
        sqlite_or_panic(connection.execute(
            "INSERT INTO schema_migrations(version, applied_at_ms) VALUES (1, 1)",
            [],
        ));
        sqlite_or_panic(connection.execute(
            "INSERT INTO agent_tasks(
                task_key, task_type, episode_id, chapter_id, status, session_id,
                prompt_version, output_contract_version, attempt_count, retry_count,
                max_attempts, validation_report, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, NULL, NULL, ?3, NULL, ?4, NULL, 0, 0, 3, NULL, 1, 1)",
            params!["legacy-task", "chapter_segment", "pending", "v1"],
        ));

        let mut database = Database { connection };
        value_or_panic(database.migrate());
        let task = value_or_panic(database.repository().get_agent_task_by_key("legacy-task"));
        let task = match task {
            Some(task) => task,
            None => panic!("legacy task was not preserved"),
        };
        assert_eq!(task.task_key, "legacy-task");
        assert_eq!(task.status, "pending");
        assert_eq!(task.output_json, None);
    }

    #[test]
    fn future_schema_is_rejected_without_running_a_partial_migration() {
        let connection = sqlite_or_panic(Connection::open_in_memory());
        sqlite_or_panic(connection.execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at_ms INTEGER NOT NULL
            );
             INSERT INTO schema_migrations(version, applied_at_ms) VALUES (99, 1);",
        ));
        let mut database = Database { connection };
        let error = match database.migrate() {
            Ok(()) => panic!("future schema was accepted"),
            Err(error) => error,
        };
        assert_eq!(error.code, DatabaseErrorCode::UnsupportedSchema);
        assert!(value_or_panic(database.table_exists("schema_migrations")));
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let database = value_or_panic(Database::open_in_memory());
        let result = database.repository().insert_episode(&NewEpisode::new(
            999,
            "episode-without-series",
            "local",
        ));
        assert!(result.is_err());
        let error = match result {
            Ok(_) => panic!("foreign-key insert unexpectedly succeeded"),
            Err(error) => error,
        };
        assert_eq!(error.code, DatabaseErrorCode::ConstraintViolation);
        assert!(!error.message.contains("FOREIGN KEY"));
    }

    #[test]
    fn chapter_bounds_and_agent_task_status_round_trip() {
        let database = value_or_panic(Database::open_in_memory());
        let repository = database.repository();
        let series_id = value_or_panic(repository.insert_series(&NewSeries::new(
            "series-1",
            "示例剧集",
            "online",
        )));
        let episode_id = value_or_panic(repository.insert_episode(&NewEpisode::new(
            series_id,
            "episode-1",
            "online",
        )));

        let invalid_chapter = repository.insert_chapter(&NewChapter::new(
            episode_id,
            "chapter-invalid",
            100,
            100,
            "ai",
        ));
        assert!(invalid_chapter.is_err());
        assert_eq!(
            invalid_chapter.err().map(|error| error.message).as_deref(),
            Some("章节结束时间必须晚于开始时间")
        );

        let chapter_id = value_or_panic(repository.insert_chapter(&NewChapter::new(
            episode_id,
            "chapter-1",
            100,
            1_000,
            "ai",
        )));
        let chapter = value_or_panic(repository.get_chapter(chapter_id));
        let chapter = match chapter {
            Some(chapter) => chapter,
            None => panic!("chapter was not persisted"),
        };
        assert_eq!((chapter.start_ms, chapter.end_ms), (100, 1_000));

        let mut task = NewAgentTask::new("task-1", "chapter_segment", "prompt-v1");
        task.episode_id = Some(episode_id);
        task.chapter_id = Some(chapter_id);
        let task_id = value_or_panic(repository.insert_agent_task(&task));
        assert!(value_or_panic(repository.update_agent_task_status(
            task_id,
            "validation_failure",
            2,
            1,
            Some("missing title"),
        )));
        let task = value_or_panic(repository.get_agent_task(task_id));
        let task = match task {
            Some(task) => task,
            None => panic!("task was not persisted"),
        };
        assert_eq!(task.status, "validation_failure");
        assert_eq!((task.attempt_count, task.retry_count), (2, 1));
        assert_eq!(task.max_attempts, 3);
        assert_eq!(task.validation_report.as_deref(), Some("missing title"));
    }

    #[test]
    fn chapter_task_scope_is_idempotent_and_rejects_cross_task_reuse() {
        let mut database = value_or_panic(Database::open_in_memory());
        let repository = database.repository();
        let series_id = value_or_panic(repository.insert_series(&NewSeries::new(
            "series-scope",
            "Scope",
            "local",
        )));
        let episode_id = value_or_panic(repository.insert_episode(&NewEpisode::new(
            series_id,
            "episode-scope",
            "local",
        )));
        let mut task_input =
            NewAgentTask::new("chapter-scope-1", "chapter_generation", "prompt-v1");
        task_input.episode_id = Some(episode_id);
        task_input.status = "running".into();
        let task_id = value_or_panic(repository.insert_agent_task(&task_input));
        let attempt_id = value_or_panic(repository.insert_agent_attempt(&NewAgentAttempt::new(
            task_id,
            1,
            "initial",
            "running",
            "prompt-v1",
            1,
        )));
        assert!(attempt_id > 0);
        drop(repository);

        let mut chapter = NewChapter::new(episode_id, "outline-1", 0, 1_000, "chapter_agent_mcp");
        chapter.title = Some("Opening".into());
        let first = value_or_panic(database.transaction(|repo| {
            repo.upsert_draft_chapter_for_agent_task(task_id, episode_id, &chapter)
        }));
        let second = value_or_panic(database.transaction(|repo| {
            repo.upsert_draft_chapter_for_agent_task(task_id, episode_id, &chapter)
        }));
        assert_eq!(first.id, second.id);
        assert_eq!(
            value_or_panic(
                database
                    .repository()
                    .list_chapters_by_agent_task(task_id, episode_id)
            )
            .len(),
            1
        );

        let mut replacement_input =
            NewChapter::new(episode_id, "outline-2", 1_000, 2_000, "chapter_agent_mcp");
        replacement_input.title = Some("Replacement".into());
        let replacement = value_or_panic(database.transaction(|repo| {
            let replacement =
                repo.upsert_draft_chapter_for_agent_task(task_id, episode_id, &replacement_input)?;
            repo.replace_agent_task_chapter_outline(task_id, episode_id, &[replacement.id])?;
            Ok(replacement)
        }));
        assert!(value_or_panic(database.repository().get_chapter(first.id)).is_none());
        assert_eq!(
            value_or_panic(
                database
                    .repository()
                    .list_chapters_by_agent_task(task_id, episode_id)
            )
            .iter()
            .map(|chapter| chapter.id)
            .collect::<Vec<_>>(),
            vec![replacement.id]
        );

        let _asset = value_or_panic(database.transaction(|repo| {
            repo.insert_chapter_asset_for_agent_task(
                task_id,
                episode_id,
                &NewChapterAsset::new(
                    replacement.id,
                    "jpeg_frame",
                    "chapter-assets/frame.jpg",
                    "hash-1",
                    1_200,
                    "chapter_agent_mcp",
                ),
            )
        }));
        let mut first_revision_input = NewChapterRevision::new(
            replacement.id,
            1,
            "chapter_draft:retry-key",
            "old draft",
            "chapter_agent_mcp",
            "prompt-v1",
        );
        let first_revision = value_or_panic(database.transaction(|repo| {
            repo.insert_draft_revision_for_agent_task(task_id, episode_id, &first_revision_input)
        }));
        first_revision_input.validation_report = Some("updated".into());
        first_revision_input.content = "new draft".into();
        first_revision_input.revision_number = 2;
        let second_revision = value_or_panic(database.transaction(|repo| {
            repo.insert_draft_revision_for_agent_task(task_id, episode_id, &first_revision_input)
        }));
        assert_eq!(first_revision.id, second_revision.id);
        assert_eq!(second_revision.content, "new draft");

        let mut first_feed = NewWatchFeedItem::new(
            "mainline",
            "chapter_agent_mcp",
            "old feed",
            "episode",
            "prompt-v1",
            "chapter-feed:retry-key",
        );
        first_feed.episode_id = Some(episode_id);
        first_feed.chapter_id = Some(replacement.id);
        first_feed.revision_id = Some(first_revision.id);
        first_feed.task_id = Some(task_id);
        let first_feed_record = value_or_panic(database.transaction(|repo| {
            repo.insert_draft_feed_item_for_agent_task(task_id, episode_id, &first_feed)
        }));
        let mut second_feed = first_feed.clone();
        second_feed.content = "new feed".into();
        second_feed.revision_id = Some(second_revision.id);
        let second_feed_record = value_or_panic(database.transaction(|repo| {
            repo.insert_draft_feed_item_for_agent_task(task_id, episode_id, &second_feed)
        }));
        assert_eq!(first_feed_record.id, second_feed_record.id);
        assert_eq!(
            value_or_panic(
                database
                    .repository()
                    .list_watch_feed_items_by_episode(episode_id)
            )
            .into_iter()
            .find(|item| item.dedupe_key == "chapter-feed:retry-key")
            .map(|item| item.content),
            Some("new feed".into())
        );

        let mut other = NewAgentTask::new("chapter-scope-2", "chapter_generation", "prompt-v1");
        other.episode_id = Some(episode_id);
        other.status = "running".into();
        let other_id = value_or_panic(database.repository().insert_agent_task(&other));
        let result = database.transaction(|repo| {
            repo.upsert_draft_chapter_for_agent_task(other_id, episode_id, &replacement_input)
        });
        assert!(result.is_err());
    }

    #[test]
    fn agent_task_key_lifecycle_is_idempotent_and_persists_session() {
        let database = value_or_panic(Database::open_in_memory());
        let repository = database.repository();
        let mut input = NewAgentTask::new("episode-1:chapter-1:segment", "chapter_segment", "v1");
        input.max_attempts = 3;

        let first = value_or_panic(repository.get_or_create_agent_task(&input));
        let second = value_or_panic(repository.get_or_create_agent_task(&input));
        assert_eq!(first.id, second.id);
        assert!(value_or_panic(
            repository.is_agent_task_active(&input.task_key)
        ));

        assert!(value_or_panic(repository.update_agent_task_session_id(
            &input.task_key,
            Some("session-1"),
        )));
        assert!(value_or_panic(repository.update_agent_task_status_by_key(
            &input.task_key,
            "validation_failure",
            1,
            1,
            Some("标题缺失"),
        )));

        let task = value_or_panic(repository.get_agent_task_by_key(&input.task_key));
        let task = match task {
            Some(task) => task,
            None => panic!("task was not persisted by key"),
        };
        assert_eq!(task.id, first.id);
        assert_eq!(task.session_id.as_deref(), Some("session-1"));
        assert_eq!((task.attempt_count, task.retry_count), (1, 1));
        assert_eq!(task.validation_report.as_deref(), Some("标题缺失"));
        assert!(value_or_panic(repository.get_active_agent_task_by_key(&input.task_key)).is_some());
    }

    #[test]
    fn completed_agent_task_persists_structured_output() {
        let database = value_or_panic(Database::open_in_memory());
        let repository = database.repository();
        let input = NewAgentTask::new("output-task", "chapter_segmentation", "1.0");
        let created = value_or_panic(repository.get_or_create_agent_task(&input));
        let claimed = value_or_panic(repository.claim_agent_task_by_key(&input.task_key));
        assert_eq!(claimed.as_ref().map(|task| task.attempt_count), Some(1));

        assert!(value_or_panic(repository.complete_agent_task(
            created.id,
            "succeeded",
            1,
            0,
            None,
            r#"{"chapters":[]}"#,
        )));
        let persisted = value_or_panic(repository.get_agent_task(created.id));
        let persisted = match persisted {
            Some(task) => task,
            None => panic!("completed task was not persisted"),
        };
        assert_eq!(persisted.status, "succeeded");
        assert_eq!(persisted.output_json.as_deref(), Some(r#"{"chapters":[]}"#));
    }

    #[test]
    fn read_projection_lists_are_episode_scoped_and_select_latest_revision() {
        let database = value_or_panic(Database::open_in_memory());
        let repository = database.repository();
        let series_id = value_or_panic(repository.insert_series(&NewSeries::new(
            "projection-series",
            "投影测试",
            "local",
        )));
        let episode_id = value_or_panic(repository.insert_episode(&NewEpisode::new(
            series_id,
            "episode-1",
            "local",
        )));
        let other_episode_id = value_or_panic(repository.insert_episode(&NewEpisode::new(
            series_id,
            "episode-2",
            "local",
        )));
        let chapter_id = value_or_panic(repository.insert_chapter(&NewChapter::new(
            episode_id,
            "chapter-1",
            1_000,
            2_000,
            "ai",
        )));
        let second_chapter_id = value_or_panic(repository.insert_chapter(&NewChapter::new(
            episode_id,
            "chapter-2",
            2_000,
            3_000,
            "ai",
        )));
        let other_chapter_id = value_or_panic(repository.insert_chapter(&NewChapter::new(
            other_episode_id,
            "chapter-other",
            0,
            1_000,
            "ai",
        )));

        let first_revision_id = value_or_panic(repository.insert_chapter_revision(
            &NewChapterRevision::new(chapter_id, 1, "generated", "旧版本", "ai", "v1"),
        ));
        let mut latest_revision =
            NewChapterRevision::new(chapter_id, 2, "revision", "新版本", "ai", "v2");
        latest_revision.status = "accepted".to_string();
        let latest_revision_id =
            value_or_panic(repository.insert_chapter_revision(&latest_revision));

        let _screenshot_id =
            value_or_panic(repository.insert_chapter_asset(&NewChapterAsset::new(
                chapter_id,
                "screenshot",
                "opaque-screenshot-resource",
                "hash-screenshot",
                1_500,
                "ai",
            )));
        let _cover_id = value_or_panic(repository.insert_chapter_asset(&NewChapterAsset::new(
            chapter_id,
            "cover",
            "opaque-cover-resource",
            "hash-cover",
            1_600,
            "ai",
        )));

        let mut candidate = NewQuestionCandidate::new(
            "这一章的看点是什么？",
            "ai",
            "current_chapter",
            "projection-question",
        );
        candidate.episode_id = Some(episode_id);
        candidate.chapter_id = Some(chapter_id);
        let _candidate_id = value_or_panic(repository.insert_question_candidate(&candidate));

        let mut first_feed = NewWatchFeedItem::new(
            "chapter",
            "ai",
            "章节主线",
            "current_chapter",
            "v1",
            "projection-feed-1",
        );
        first_feed.episode_id = Some(episode_id);
        first_feed.chapter_id = Some(chapter_id);
        first_feed.revision_id = Some(first_revision_id);
        first_feed.published_at_ms = Some(20);
        value_or_panic(repository.insert_watch_feed_item(&first_feed));

        let mut second_feed =
            NewWatchFeedItem::new("recap", "ai", "前情提要", "none", "v1", "projection-feed-2");
        second_feed.episode_id = Some(episode_id);
        second_feed.chapter_id = Some(second_chapter_id);
        second_feed.published_at_ms = Some(10);
        value_or_panic(repository.insert_watch_feed_item(&second_feed));

        let mut other_feed = NewWatchFeedItem::new(
            "chapter",
            "ai",
            "其他集内容",
            "full_media",
            "v1",
            "projection-feed-other",
        );
        other_feed.episode_id = Some(other_episode_id);
        other_feed.chapter_id = Some(other_chapter_id);
        value_or_panic(repository.insert_watch_feed_item(&other_feed));

        let chapters = value_or_panic(repository.list_chapters_by_episode(episode_id));
        assert_eq!(
            chapters
                .iter()
                .map(|chapter| chapter.id)
                .collect::<Vec<_>>(),
            vec![chapter_id, second_chapter_id]
        );
        let feed = value_or_panic(repository.list_watch_feed_items_by_episode(episode_id));
        assert_eq!(feed.len(), 2);
        assert_eq!(feed[0].content, "前情提要");
        assert_eq!(feed[1].content, "章节主线");
        assert_eq!(
            value_or_panic(repository.list_watch_feed_items_by_episode(other_episode_id)).len(),
            1
        );

        let latest = value_or_panic(repository.get_latest_chapter_revision(chapter_id));
        assert_eq!(
            latest.as_ref().map(|revision| revision.id),
            Some(latest_revision_id)
        );
        assert_eq!(
            value_or_panic(repository.list_question_candidates_by_episode(episode_id)).len(),
            1
        );
        assert_eq!(
            value_or_panic(repository.list_question_candidates_by_chapter(chapter_id)).len(),
            1
        );
        assert!(
            value_or_panic(repository.list_question_candidates_by_chapter(second_chapter_id))
                .is_empty()
        );
        let assets = value_or_panic(repository.list_chapter_assets_by_chapter(chapter_id));
        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].asset_type, "screenshot");
        assert_eq!(assets[1].asset_type, "cover");
    }

    fn seed_episode_pair(database: &Database) -> (i64, i64) {
        let repository = database.repository();
        let legacy_series = value_or_panic(repository.insert_series(&NewSeries::new(
            "media-series:C:/media/show/s01e01.mkv",
            "Legacy Show",
            "local",
        )));
        let authoritative_series = value_or_panic(repository.insert_series(&NewSeries::new(
            "tmdb:tv:42",
            "Authoritative Show",
            "tmdb",
        )));
        let legacy_episode = value_or_panic(repository.insert_episode(&NewEpisode::new(
            legacy_series,
            "s01e01",
            "local",
        )));
        let mut authoritative_input = NewEpisode::new(authoritative_series, "s01e01", "tmdb");
        authoritative_input.season_number = Some(1);
        authoritative_input.episode_number = Some(1);
        authoritative_input.title = Some("Episode 1".to_string());
        let authoritative_episode = value_or_panic(repository.insert_episode(&authoritative_input));
        (legacy_episode, authoritative_episode)
    }

    #[test]
    fn legacy_episode_migration_handles_missing_source_without_writes() {
        let mut database = value_or_panic(Database::open_in_memory());
        let legacy_series = value_or_panic(database.repository().insert_series(&NewSeries::new(
            "media-series:legacy",
            "Legacy",
            "local",
        )));
        let legacy_episode =
            value_or_panic(database.repository().insert_episode(&NewEpisode::new(
                legacy_series,
                "s01e01",
                "local",
            )));

        let missing_authoritative = LegacyEpisodeMigration::new(legacy_episode, 9_002);
        let report = value_or_panic(database.migrate_legacy_episode(&missing_authoritative));
        assert_eq!(report.status, EpisodeMigrationStatus::AuthoritativeMissing);
        assert!(value_or_panic(database.repository().get_episode(legacy_episode)).is_some());

        let mut authoritative_only = value_or_panic(Database::open_in_memory());
        let authoritative_series = value_or_panic(
            authoritative_only
                .repository()
                .insert_series(&NewSeries::new("tmdb:tv:42", "Show", "tmdb")),
        );
        let mut authoritative_input = NewEpisode::new(authoritative_series, "s01e01", "tmdb");
        authoritative_input.season_number = Some(1);
        authoritative_input.episode_number = Some(1);
        let authoritative_episode = value_or_panic(
            authoritative_only
                .repository()
                .insert_episode(&authoritative_input),
        );
        let report = value_or_panic(
            authoritative_only
                .migrate_legacy_episode(&LegacyEpisodeMigration::new(9_001, authoritative_episode)),
        );
        assert_eq!(report.status, EpisodeMigrationStatus::LegacyMissing);
    }

    #[test]
    fn legacy_episode_migration_moves_projection_and_preserves_unrewritable_task() {
        let mut database = value_or_panic(Database::open_in_memory());
        let (legacy_episode, authoritative_episode) = seed_episode_pair(&database);
        let repository = database.repository();

        let mut chapter = NewChapter::new(legacy_episode, "chapter-1", 0, 10_000, "ai");
        chapter.title = Some("主线".to_string());
        let chapter_id = value_or_panic(repository.insert_chapter(&chapter));
        let _asset_id = value_or_panic(repository.insert_chapter_asset(&NewChapterAsset::new(
            chapter_id,
            "cover",
            "opaque-cover",
            "hash-1",
            1,
            "ai",
        )));
        let _revision_id = value_or_panic(repository.insert_chapter_revision(
            &NewChapterRevision::new(chapter_id, 1, "initial", "content", "ai", "v1"),
        ));

        let mut task_input = NewAgentTask::new("legacy-task-key", "chapter_segmentation", "v1");
        task_input.episode_id = Some(legacy_episode);
        let task = value_or_panic(repository.get_or_create_agent_task(&task_input));
        let _attempt_id = value_or_panic(repository.insert_agent_attempt(&NewAgentAttempt::new(
            task.id,
            1,
            "chapter_segmentation",
            "succeeded",
            "v1",
            1,
        )));

        let mut question = NewQuestionCandidate::new("接下来会发生什么？", "ai", "none", "q-1");
        question.episode_id = Some(legacy_episode);
        question.chapter_id = Some(chapter_id);
        question.task_id = Some(task.id);
        let _question_id = value_or_panic(repository.insert_question_candidate(&question));

        let mut feed = NewWatchFeedItem::new("chapter", "ai", "主线", "none", "v1", "feed-1");
        feed.episode_id = Some(legacy_episode);
        feed.chapter_id = Some(chapter_id);
        feed.task_id = Some(task.id);
        let _feed_id = value_or_panic(repository.insert_watch_feed_item(&feed));
        drop(repository);

        let report = value_or_panic(
            database.migrate_legacy_episode(&LegacyEpisodeMigration::new(
                legacy_episode,
                authoritative_episode,
            )),
        );
        assert_eq!(
            report.status,
            EpisodeMigrationStatus::MergedWithPreservedTasks
        );
        assert_eq!(report.moved_chapters, 1);
        assert_eq!(report.moved_questions, 1);
        assert_eq!(report.moved_feed_items, 1);
        assert_eq!(report.preserved_task_keys, vec!["legacy-task-key"]);

        let repository = database.repository();
        let chapters = value_or_panic(repository.list_chapters_by_episode(authoritative_episode));
        assert_eq!(chapters.len(), 1);
        assert_eq!(
            value_or_panic(repository.list_chapter_assets_by_chapter(chapter_id)).len(),
            1
        );
        assert!(value_or_panic(repository.get_latest_chapter_revision(chapter_id)).is_some());
        assert_eq!(
            value_or_panic(repository.list_question_candidates_by_episode(authoritative_episode))
                .len(),
            1
        );
        assert_eq!(
            value_or_panic(repository.list_watch_feed_items_by_episode(authoritative_episode))
                .len(),
            1
        );
        let preserved_task = value_or_panic(repository.get_agent_task_by_key("legacy-task-key"));
        assert_eq!(
            preserved_task.as_ref().and_then(|task| task.episode_id),
            Some(legacy_episode)
        );
        drop(repository);

        let repeated = value_or_panic(database.migrate_legacy_episode(
            &LegacyEpisodeMigration::new(legacy_episode, authoritative_episode),
        ));
        assert_eq!(
            repeated.status,
            EpisodeMigrationStatus::MergedWithPreservedTasks
        );
        assert_eq!(repeated.moved_chapters, 0);
        assert_eq!(repeated.moved_questions, 0);
        assert_eq!(repeated.moved_feed_items, 0);
    }

    #[test]
    fn legacy_episode_migration_can_rename_only_an_explicitly_safe_task_key() {
        let mut database = value_or_panic(Database::open_in_memory());
        let (legacy_episode, authoritative_episode) = seed_episode_pair(&database);
        let repository = database.repository();
        let mut task_input = NewAgentTask::new("legacy-task-key", "chapter_segmentation", "v1");
        task_input.episode_id = Some(legacy_episode);
        let task = value_or_panic(repository.get_or_create_agent_task(&task_input));
        drop(repository);

        let mut migration = LegacyEpisodeMigration::new(legacy_episode, authoritative_episode);
        migration.task_key_migrations.push(AgentTaskKeyMigration {
            task_id: task.id,
            new_task_key: "chapter-segmentation:identity:tmdb:tv:42:s01e01".to_string(),
        });
        let report = value_or_panic(database.migrate_legacy_episode(&migration));
        assert_eq!(report.status, EpisodeMigrationStatus::Merged);
        assert_eq!(report.renamed_tasks, 1);
        let moved = value_or_panic(
            database
                .repository()
                .get_agent_task_by_key("chapter-segmentation:identity:tmdb:tv:42:s01e01"),
        );
        assert_eq!(
            moved.as_ref().and_then(|task| task.episode_id),
            Some(authoritative_episode)
        );
    }

    #[test]
    fn legacy_episode_migration_keeps_both_tasks_when_new_key_is_occupied() {
        let mut database = value_or_panic(Database::open_in_memory());
        let (legacy_episode, authoritative_episode) = seed_episode_pair(&database);
        let repository = database.repository();
        let mut legacy_input = NewAgentTask::new("legacy-task-key", "chapter_segmentation", "v1");
        legacy_input.episode_id = Some(legacy_episode);
        let legacy_task = value_or_panic(repository.get_or_create_agent_task(&legacy_input));
        let mut authoritative_input = NewAgentTask::new(
            "chapter-segmentation:identity:tmdb:tv:42:s01e01",
            "chapter_segmentation",
            "v1",
        );
        authoritative_input.episode_id = Some(authoritative_episode);
        let _authoritative_task =
            value_or_panic(repository.get_or_create_agent_task(&authoritative_input));
        drop(repository);

        let mut migration = LegacyEpisodeMigration::new(legacy_episode, authoritative_episode);
        migration.task_key_migrations.push(AgentTaskKeyMigration {
            task_id: legacy_task.id,
            new_task_key: authoritative_input.task_key,
        });
        let report = value_or_panic(database.migrate_legacy_episode(&migration));
        assert_eq!(report.status, EpisodeMigrationStatus::Conflict);
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic == "迁移后的任务标识已被占用"));
        let preserved = value_or_panic(
            database
                .repository()
                .get_agent_task_by_key("legacy-task-key"),
        );
        assert_eq!(
            preserved.as_ref().and_then(|task| task.episode_id),
            Some(legacy_episode)
        );
    }

    #[test]
    fn legacy_episode_migration_conflict_keeps_both_projection_sources() {
        let mut database = value_or_panic(Database::open_in_memory());
        let (legacy_episode, authoritative_episode) = seed_episode_pair(&database);
        let repository = database.repository();
        let _legacy_chapter = value_or_panic(repository.insert_chapter(&NewChapter::new(
            legacy_episode,
            "same-stable-id",
            0,
            10_000,
            "ai",
        )));
        let _authoritative_chapter = value_or_panic(repository.insert_chapter(&NewChapter::new(
            authoritative_episode,
            "same-stable-id",
            20_000,
            30_000,
            "ai",
        )));
        drop(repository);

        let report = value_or_panic(
            database.migrate_legacy_episode(&LegacyEpisodeMigration::new(
                legacy_episode,
                authoritative_episode,
            )),
        );
        assert_eq!(report.status, EpisodeMigrationStatus::Conflict);
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic == "章节标识存在冲突"));
        assert_eq!(
            value_or_panic(
                database
                    .repository()
                    .list_chapters_by_episode(legacy_episode)
            )
            .len(),
            1
        );
        assert_eq!(
            value_or_panic(
                database
                    .repository()
                    .list_chapters_by_episode(authoritative_episode)
            )
            .len(),
            1
        );
    }

    #[test]
    fn legacy_episode_migration_detects_duplicate_watch_feed_key_before_writes() {
        let mut database = value_or_panic(Database::open_in_memory());
        let (legacy_episode, authoritative_episode) = seed_episode_pair(&database);

        // This test fixture emulates an older database that allowed duplicate
        // feed keys. The production schema remains globally UNIQUE; the
        // migration must still protect such legacy data before reparenting.
        sqlite_or_panic(database.connection.execute_batch(
            "PRAGMA foreign_keys = OFF;
             ALTER TABLE watch_feed_items RENAME TO watch_feed_items_unique;
             CREATE TABLE watch_feed_items (
                 id INTEGER PRIMARY KEY,
                 episode_id INTEGER,
                 chapter_id INTEGER,
                 revision_id INTEGER,
                 task_id INTEGER,
                 item_type TEXT NOT NULL,
                 source TEXT NOT NULL,
                 content TEXT NOT NULL,
                 spoiler_level TEXT NOT NULL,
                 content_version TEXT NOT NULL,
                 dedupe_key TEXT NOT NULL,
                 published_at_ms INTEGER,
                 created_at_ms INTEGER NOT NULL,
                 FOREIGN KEY (episode_id) REFERENCES episodes(id) ON DELETE CASCADE,
                 FOREIGN KEY (chapter_id) REFERENCES episode_chapters(id) ON DELETE CASCADE,
                 FOREIGN KEY (revision_id) REFERENCES chapter_revisions(id) ON DELETE SET NULL,
                 FOREIGN KEY (task_id) REFERENCES agent_tasks(id) ON DELETE SET NULL
             );
             INSERT INTO watch_feed_items
             SELECT id, episode_id, chapter_id, revision_id, task_id, item_type, source,
                    content, spoiler_level, content_version, dedupe_key,
                    published_at_ms, created_at_ms
             FROM watch_feed_items_unique;
             DROP TABLE watch_feed_items_unique;
             PRAGMA foreign_keys = ON;",
        ));

        let repository = database.repository();
        let mut legacy_feed =
            NewWatchFeedItem::new("chapter", "legacy", "旧来源", "none", "v1", "same-feed-key");
        legacy_feed.episode_id = Some(legacy_episode);
        value_or_panic(repository.insert_watch_feed_item(&legacy_feed));
        let mut authoritative_feed = NewWatchFeedItem::new(
            "chapter",
            "authoritative",
            "新来源",
            "none",
            "v1",
            "same-feed-key",
        );
        authoritative_feed.episode_id = Some(authoritative_episode);
        value_or_panic(repository.insert_watch_feed_item(&authoritative_feed));
        drop(repository);

        let report = value_or_panic(
            database.migrate_legacy_episode(&LegacyEpisodeMigration::new(
                legacy_episode,
                authoritative_episode,
            )),
        );
        assert_eq!(report.status, EpisodeMigrationStatus::Conflict);
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic == "观剧流幂等键存在冲突"));
        let legacy_items = value_or_panic(
            database
                .repository()
                .list_watch_feed_items_by_episode(legacy_episode),
        );
        let authoritative_items = value_or_panic(
            database
                .repository()
                .list_watch_feed_items_by_episode(authoritative_episode),
        );
        assert_eq!(legacy_items.len(), 1);
        assert_eq!(authoritative_items.len(), 1);
    }

    #[test]
    fn legacy_episode_migration_rolls_back_when_outer_transaction_is_interrupted() {
        let mut database = value_or_panic(Database::open_in_memory());
        let (legacy_episode, authoritative_episode) = seed_episode_pair(&database);
        let repository = database.repository();
        let _chapter_id = value_or_panic(repository.insert_chapter(&NewChapter::new(
            legacy_episode,
            "chapter-1",
            0,
            10_000,
            "ai",
        )));
        drop(repository);

        let result: DatabaseResult<()> = database.transaction(|repository| {
            let report = repository.migrate_legacy_episode(&LegacyEpisodeMigration::new(
                legacy_episode,
                authoritative_episode,
            ))?;
            assert_eq!(report.status, EpisodeMigrationStatus::Merged);
            Err(DatabaseError::invalid_input("模拟事务中断"))
        });
        assert!(result.is_err());
        assert_eq!(
            value_or_panic(
                database
                    .repository()
                    .list_chapters_by_episode(legacy_episode)
            )
            .len(),
            1
        );
        assert!(value_or_panic(
            database
                .repository()
                .list_chapters_by_episode(authoritative_episode)
        )
        .is_empty());

        let recovered = value_or_panic(database.migrate_legacy_episode(
            &LegacyEpisodeMigration::new(legacy_episode, authoritative_episode),
        ));
        assert_eq!(recovered.status, EpisodeMigrationStatus::Merged);
    }

    #[test]
    fn startup_recovery_releases_only_interrupted_chapter_tasks() {
        let mut database = value_or_panic(Database::open_in_memory());
        let chapter_input = NewAgentTask::new("recover-chapter", "chapter_segmentation", "1.0");
        let chapter_task = value_or_panic(
            database
                .repository()
                .get_or_create_agent_task(&chapter_input),
        );
        let chapter_claim = value_or_panic(
            database
                .repository()
                .claim_agent_task_by_key(&chapter_input.task_key),
        );
        assert!(chapter_claim.is_some());
        let chapter_attempt = NewAgentAttempt::new(
            chapter_task.id,
            1,
            "chapter_segmentation",
            "running",
            "1.0",
            1,
        );
        let chapter_attempt_id =
            value_or_panic(database.repository().insert_agent_attempt(&chapter_attempt));

        let other_input = NewAgentTask::new("recover-other", "subtitle_translation", "1.0");
        let other_task =
            value_or_panic(database.repository().get_or_create_agent_task(&other_input));
        let other_claim = value_or_panic(
            database
                .repository()
                .claim_agent_task_by_key(&other_input.task_key),
        );
        assert!(other_claim.is_some());

        assert_eq!(
            value_or_panic(database.recover_interrupted_agent_tasks()),
            1
        );
        let chapter = value_or_panic(database.repository().get_agent_task(chapter_task.id));
        assert_eq!(
            chapter.as_ref().map(|task| task.status.as_str()),
            Some("validation_failure")
        );
        assert_eq!(
            chapter
                .as_ref()
                .and_then(|task| task.validation_report.as_deref()),
            Some("应用关闭时章节任务中断，可重新执行")
        );
        let attempt = value_or_panic(database.repository().get_agent_attempt(chapter_attempt_id));
        assert_eq!(
            attempt.as_ref().map(|attempt| attempt.status.as_str()),
            Some("interrupted")
        );

        let other = value_or_panic(database.repository().get_agent_task(other_task.id));
        assert_eq!(
            other.as_ref().map(|task| task.status.as_str()),
            Some("running")
        );
    }

    #[test]
    fn claim_agent_task_by_key_claims_pending_once_and_rejects_duplicate_claim() {
        let database = value_or_panic(Database::open_in_memory());
        let repository = database.repository();
        let input = NewAgentTask::new("claim-once", "chapter_segment", "v1");
        let created = value_or_panic(repository.get_or_create_agent_task(&input));

        let claimed = value_or_panic(repository.claim_agent_task_by_key(&input.task_key));
        let claimed = match claimed {
            Some(task) => task,
            None => panic!("pending task was not claimed"),
        };
        assert_eq!(claimed.id, created.id);
        assert_eq!(claimed.status, "running");
        assert_eq!(claimed.attempt_count, 1);

        assert!(value_or_panic(repository.claim_agent_task_by_key(&input.task_key)).is_none());
        let persisted = value_or_panic(repository.get_agent_task_by_key(&input.task_key));
        let persisted = match persisted {
            Some(task) => task,
            None => panic!("claimed task was not persisted"),
        };
        assert_eq!(persisted.attempt_count, 1);
        assert_eq!(persisted.status, "running");
    }

    #[test]
    fn claim_agent_task_by_key_can_reclaim_validation_failure() {
        let database = value_or_panic(Database::open_in_memory());
        let repository = database.repository();
        let input = NewAgentTask::new("claim-validation-failure", "chapter_segment", "v1");
        value_or_panic(repository.get_or_create_agent_task(&input));

        let first = value_or_panic(repository.claim_agent_task_by_key(&input.task_key));
        assert_eq!(first.as_ref().map(|task| task.attempt_count), Some(1));
        assert!(value_or_panic(repository.update_agent_task_status_by_key(
            &input.task_key,
            "validation_failure",
            1,
            1,
            Some("缺少章节证据"),
        )));

        let second = value_or_panic(repository.claim_agent_task_by_key(&input.task_key));
        let second = match second {
            Some(task) => task,
            None => panic!("validation failure task was not reclaimed"),
        };
        assert_eq!(second.status, "running");
        assert_eq!(second.attempt_count, 2);
        assert_eq!(second.retry_count, 1);
    }

    #[test]
    fn claim_agent_task_by_key_rejects_claim_after_max_attempts() {
        let database = value_or_panic(Database::open_in_memory());
        let repository = database.repository();
        let mut input = NewAgentTask::new("claim-max-attempts", "chapter_segment", "v1");
        input.max_attempts = 2;
        value_or_panic(repository.get_or_create_agent_task(&input));

        let first = value_or_panic(repository.claim_agent_task_by_key(&input.task_key));
        assert_eq!(first.as_ref().map(|task| task.attempt_count), Some(1));
        assert!(value_or_panic(repository.update_agent_task_status_by_key(
            &input.task_key,
            "validation_failure",
            1,
            1,
            Some("需要重试"),
        )));

        let second = value_or_panic(repository.claim_agent_task_by_key(&input.task_key));
        assert_eq!(second.as_ref().map(|task| task.attempt_count), Some(2));
        assert!(value_or_panic(repository.update_agent_task_status_by_key(
            &input.task_key,
            "validation_failure",
            2,
            2,
            Some("仍然缺少证据"),
        )));

        assert!(value_or_panic(repository.claim_agent_task_by_key(&input.task_key)).is_none());
        let persisted = value_or_panic(repository.get_agent_task_by_key(&input.task_key));
        let persisted = match persisted {
            Some(task) => task,
            None => panic!("max-attempt task was not persisted"),
        };
        assert_eq!(persisted.attempt_count, persisted.max_attempts);
        assert_eq!(persisted.status, "validation_failure");
    }

    #[test]
    fn agent_task_max_attempts_allows_boundary_and_rejects_overflow() {
        let database = value_or_panic(Database::open_in_memory());
        let repository = database.repository();
        let mut input = NewAgentTask::new("task-with-limit", "chapter_segment", "v1");
        input.max_attempts = 2;
        let task_id = value_or_panic(repository.insert_agent_task(&input));

        let attempt_two =
            NewAgentAttempt::new(task_id, 2, "validation_retry", "running", "v1", 200);
        assert!(value_or_panic(repository.insert_agent_attempt(&attempt_two)) > 0);
        assert!(value_or_panic(repository.update_agent_task_status(
            task_id,
            "validation_failure",
            2,
            1,
            Some("仍缺少标题"),
        )));
        assert!(!value_or_panic(
            repository.is_agent_task_active(&input.task_key)
        ));

        let overflow = repository.update_agent_task_status(task_id, "running", 3, 2, None);
        let error = match overflow {
            Ok(_) => panic!("attempt limit was not enforced"),
            Err(error) => error,
        };
        assert_eq!(error.code, DatabaseErrorCode::InvalidInput);
        assert_eq!(error.message, "任务已达到最大尝试次数，无法继续执行");
        assert!(error.details.is_none());

        let overflow_attempt =
            NewAgentAttempt::new(task_id, 3, "validation_retry", "running", "v1", 300);
        let error = match repository.insert_agent_attempt(&overflow_attempt) {
            Ok(_) => panic!("attempt insert limit was not enforced"),
            Err(error) => error,
        };
        assert_eq!(error.code, DatabaseErrorCode::InvalidInput);
        assert_eq!(error.message, "任务已达到最大尝试次数，无法继续执行");

        let task = value_or_panic(repository.get_agent_task_by_key(&input.task_key));
        let task = match task {
            Some(task) => task,
            None => panic!("task disappeared after rejected update"),
        };
        assert_eq!(task.attempt_count, 2);
        assert_eq!(task.status, "validation_failure");
    }

    #[test]
    fn failed_chapter_task_transaction_rolls_back_all_related_writes() {
        let mut database = value_or_panic(Database::open_in_memory());
        let repository = database.repository();
        let series_id = value_or_panic(repository.insert_series(&NewSeries::new(
            "rollback-series",
            "回滚剧集",
            "local",
        )));
        let episode_id = value_or_panic(repository.insert_episode(&NewEpisode::new(
            series_id,
            "rollback-episode",
            "local",
        )));
        let chapter_id = value_or_panic(repository.insert_chapter(&NewChapter::new(
            episode_id,
            "rollback-chapter",
            0,
            1_000,
            "ai",
        )));
        let task_id = value_or_panic(repository.insert_agent_task(&NewAgentTask::new(
            "rollback-task",
            "chapter_segment",
            "v1",
        )));

        let result: DatabaseResult<()> = database.transaction(|repository| {
            let revision_id = repository.insert_chapter_revision(&NewChapterRevision::new(
                chapter_id,
                1,
                "generated",
                "章节主线",
                "ai",
                "v1",
            ))?;
            let mut feed_item =
                NewWatchFeedItem::new("recap", "ai", "前情提要", "none", "v1", "rollback-feed");
            feed_item.episode_id = Some(episode_id);
            feed_item.chapter_id = Some(chapter_id);
            feed_item.revision_id = Some(revision_id);
            feed_item.task_id = Some(task_id);
            let _ = repository.insert_watch_feed_item(&feed_item)?;
            repository.update_agent_task_status(task_id, "succeeded", 1, 0, None)?;
            Err(DatabaseError::invalid_input("故意失败"))
        });
        assert!(result.is_err());

        let repository = database.repository();
        let task = value_or_panic(repository.get_agent_task(task_id));
        let task = match task {
            Some(task) => task,
            None => panic!("task was unexpectedly removed"),
        };
        assert_eq!(task.status, "pending");
        assert_eq!(task.attempt_count, 0);
        assert!(value_or_panic(repository.get_chapter_revision(1)).is_none());
        assert!(value_or_panic(repository.get_watch_feed_item(1)).is_none());
    }

    #[test]
    fn failed_transaction_rolls_back_all_repository_writes() {
        let mut database = value_or_panic(Database::open_in_memory());
        let result: DatabaseResult<()> = database.transaction(|repository| {
            let _ = repository.insert_series(&NewSeries::new("rolled-back", "回滚", "local"))?;
            Err(DatabaseError::invalid_input("故意失败"))
        });
        assert!(result.is_err());

        let series = value_or_panic(database.repository().get_series(1));
        assert!(series.is_none());
    }

    #[test]
    fn chat_snapshot_upsert_replaces_and_isolates_by_composite_key() {
        let database = value_or_panic(Database::open_in_memory());

        let first = value_or_panic(database.snapshot_upsert(
            "codex",
            "session-a",
            Some("D:/work/show"),
            "草稿一",
            r#"[{"role":"user","text":"你好"}]"#,
        ));
        assert_eq!(first.profile_id, "codex");
        assert_eq!(first.session_id, "session-a");
        assert_eq!(first.cwd.as_deref(), Some("D:/work/show"));
        assert_eq!(first.draft, "草稿一");

        let second = value_or_panic(database.snapshot_upsert(
            "codex",
            "session-a",
            None,
            "草稿二",
            r#"[{"role":"user","text":"继续"}]"#,
        ));
        assert_eq!(second.draft, "草稿二");
        assert_eq!(second.turns_json, r#"[{"role":"user","text":"继续"}]"#);
        assert_eq!(second.cwd, None);

        let other_session = value_or_panic(database.snapshot_upsert(
            "codex",
            "session-b",
            Some("D:/work/other"),
            "另一会话草稿",
            r#"[]"#,
        ));
        assert_eq!(other_session.session_id, "session-b");

        let other_profile = value_or_panic(database.snapshot_upsert(
            "claude",
            "session-a",
            None,
            "其他配置草稿",
            r#"[]"#,
        ));
        assert_eq!(other_profile.profile_id, "claude");

        let loaded = value_or_panic(database.snapshot_get("codex", "session-a"));
        let loaded = match loaded {
            Some(snapshot) => snapshot,
            None => panic!("snapshot was not persisted"),
        };
        assert_eq!(loaded.draft, "草稿二");

        assert!(value_or_panic(
            database.snapshot_delete("codex", "session-a")
        ));
        assert!(
            value_or_panic(database.snapshot_get("codex", "session-a")).is_none(),
            "deleted snapshot must disappear"
        );
        assert!(!value_or_panic(
            database.snapshot_delete("codex", "session-a")
        ));
        // Composite-key isolation: deleting one pair keeps the others.
        assert!(
            value_or_panic(database.snapshot_get("codex", "session-b")).is_some(),
            "other session must survive"
        );
        assert!(
            value_or_panic(database.snapshot_get("claude", "session-a")).is_some(),
            "other profile must survive"
        );
    }

    #[test]
    fn chat_snapshot_rejects_empty_draft_and_malformed_turns() {
        let database = value_or_panic(Database::open_in_memory());

        let empty_draft = database.snapshot_upsert("codex", "s1", None, "   ", r#"[]"#);
        assert_eq!(
            empty_draft.err().map(|error| error.message).as_deref(),
            Some("聊天草稿不能为空")
        );

        let dirty_json = database.snapshot_upsert("codex", "s1", None, "草稿", "{不是json");
        assert_eq!(
            dirty_json.err().map(|error| error.message).as_deref(),
            Some("聊天记录格式无效，无法保存")
        );

        let non_array = database.snapshot_upsert("codex", "s1", None, "草稿", r#"{"a":1}"#);
        assert_eq!(
            non_array.err().map(|error| error.message).as_deref(),
            Some("聊天记录格式无效，无法保存")
        );

        let empty_turns = database.snapshot_upsert("codex", "s1", None, "草稿", "  ");
        assert_eq!(
            empty_turns.err().map(|error| error.message).as_deref(),
            Some("聊天记录不能为空")
        );

        let empty_profile = database.snapshot_upsert("", "s1", None, "草稿", r#"[]"#);
        assert_eq!(
            empty_profile.err().map(|error| error.message).as_deref(),
            Some("Agent 配置标识不能为空")
        );

        assert!(
            value_or_panic(database.snapshot_get("codex", "s1")).is_none(),
            "rejected payloads must not leave rows behind"
        );
    }

    #[test]
    fn chat_session_hint_keeps_single_row_per_profile() {
        let database = value_or_panic(Database::open_in_memory());

        let first = value_or_panic(database.hint_upsert("codex", "session-1", "D:/work/a"));
        assert_eq!(first.session_id, "session-1");

        let second = value_or_panic(database.hint_upsert("codex", "session-2", "D:/work/b"));
        assert_eq!(second.profile_id, "codex");
        assert_eq!(second.session_id, "session-2");
        assert_eq!(second.cwd, "D:/work/b");

        let loaded = value_or_panic(database.hint_get("codex"));
        let loaded = match loaded {
            Some(hint) => hint,
            None => panic!("hint was not persisted"),
        };
        assert_eq!(loaded.session_id, "session-2");

        // Other profiles are independent rows.
        let _ = value_or_panic(database.hint_upsert("claude", "session-9", ""));
        assert_eq!(
            value_or_panic(database.hint_get("codex")).map(|hint| hint.session_id),
            Some("session-2".to_string())
        );

        assert!(value_or_panic(database.hint_delete("codex")));
        assert!(value_or_panic(database.hint_get("codex")).is_none());
        assert!(!value_or_panic(database.hint_delete("codex")));
        assert!(
            value_or_panic(database.hint_get("claude")).is_some(),
            "other profile hint must survive"
        );

        let empty_session = database.hint_upsert("codex", "  ", "");
        assert_eq!(
            empty_session.err().map(|error| error.message).as_deref(),
            Some("Agent 会话标识不能为空")
        );
    }
}
