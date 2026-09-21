use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lumina_library::{
    AgentTaskKeyMigration, Database, DatabaseResult, EpisodeMigrationStatus,
    LegacyEpisodeMigration, NewChapter, NewChapterAsset, NewChapterRevision, NewEpisode,
    NewQuestionCandidate, NewSeries, NewWatchFeedItem, Repository, CURRENT_SCHEMA_VERSION,
};

static NEXT_DATABASE_ID: AtomicU64 = AtomicU64::new(1);

struct TemporaryDatabasePath(PathBuf);

impl TemporaryDatabasePath {
    fn new(test_name: &str) -> Self {
        let sequence = NEXT_DATABASE_ID.fetch_add(1, Ordering::Relaxed);
        let file_name = format!(
            "lumina-{test_name}-{}-{sequence}.sqlite3",
            std::process::id()
        );
        Self(std::env::temp_dir().join(file_name))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryDatabasePath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
    }
}

fn must<T>(result: DatabaseResult<T>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected database error: {}", error.message),
    }
}

fn seed_episode(repository: &Repository<'_>, series_key: &str, episode_key: &str) -> i64 {
    let series_id =
        must(repository.insert_series(&NewSeries::new(series_key, "Fixture series", "fixture")));
    let episode =
        must(repository.get_or_create_episode(&NewEpisode::new(series_id, episode_key, "fixture")));
    episode.id
}

fn seed_authoritative_and_legacy(database: &Database) -> (i64, i64) {
    let repository = database.repository();
    let legacy_series = must(repository.insert_series(&NewSeries::new(
        "legacy:path:series",
        "Legacy series",
        "legacy",
    )));
    let authoritative_series = must(repository.insert_series(&NewSeries::new(
        "tmdb:tv:123",
        "Authoritative series",
        "tmdb",
    )));

    let legacy = must(repository.insert_episode(&NewEpisode::new(
        legacy_series,
        "legacy-episode-1",
        "legacy",
    )));
    let mut authoritative_input = NewEpisode::new(authoritative_series, "s01e01", "tmdb");
    authoritative_input.season_number = Some(1);
    authoritative_input.episode_number = Some(1);
    let authoritative = must(repository.insert_episode(&authoritative_input));
    (legacy, authoritative)
}

#[test]
fn real_sqlite_round_trip_projects_all_entities_and_survives_restart() {
    let path = TemporaryDatabasePath::new("round-trip");
    let (episode_id, chapter_id, revision_id, question_id, feed_id, asset_id, task_id) = {
        let database = must(Database::open(path.path()));
        assert_eq!(must(database.schema_version()), CURRENT_SCHEMA_VERSION);

        let repository = database.repository();
        let episode_id = seed_episode(&repository, "series:round-trip", "episode:01");

        let mut chapter_input = NewChapter::new(episode_id, "chapter:opening", 1_000, 8_000, "ai");
        chapter_input.title = Some("Opening".to_owned());
        chapter_input.mainline = Some("The evidence establishes the conflict.".to_owned());
        let chapter_id = must(repository.insert_chapter(&chapter_input));

        let mut asset_input = NewChapterAsset::new(
            chapter_id,
            "screenshot",
            "assets/opening.webp",
            "sha256:opening",
            4_000,
            "capture",
        );
        asset_input.width = Some(1280);
        asset_input.height = Some(720);
        let asset_id = must(repository.insert_chapter_asset(&asset_input));

        let mut revision_input = NewChapterRevision::new(
            chapter_id,
            1,
            "recap",
            "The opening establishes the conflict.",
            "chapter_agent",
            "1.0",
        );
        revision_input.status = "accepted".to_owned();
        let revision_id = must(repository.insert_chapter_revision(&revision_input));

        let mut question_input = NewQuestionCandidate::new(
            "What evidence changes the viewer's interpretation?",
            "chapter_agent",
            "current_chapter",
            "question:fingerprint:1",
        );
        question_input.episode_id = Some(episode_id);
        question_input.chapter_id = Some(chapter_id);
        let question_id = must(repository.insert_question_candidate(&question_input));

        let mut feed_input = NewWatchFeedItem::new(
            "recap",
            "chapter_agent",
            "A bounded recap.",
            "current_chapter",
            "1.0",
            "feed:episode-01:recap:1",
        );
        feed_input.episode_id = Some(episode_id);
        feed_input.chapter_id = Some(chapter_id);
        feed_input.revision_id = Some(revision_id);
        let feed_id = must(repository.insert_watch_feed_item(&feed_input));

        let mut task_input =
            lumina_library::NewAgentTask::new("chapter:episode-01", "chapter_segmentation", "1.0");
        task_input.episode_id = Some(episode_id);
        task_input.chapter_id = Some(chapter_id);
        task_input.output_contract_version = Some("chapter_segment.v1".to_owned());
        let task_id = must(repository.get_or_create_agent_task(&task_input)).id;

        must(repository.set_app_setting("chapter.strategy", "{\"mode\":\"ai\"}"));

        assert_eq!(
            must(repository.get_chapter(chapter_id)).map(|row| row.id),
            Some(chapter_id)
        );
        assert_eq!(
            must(repository.list_chapter_assets_by_chapter(chapter_id)).len(),
            1
        );
        assert_eq!(
            must(repository.get_latest_chapter_revision(chapter_id)).map(|row| row.id),
            Some(revision_id)
        );
        assert_eq!(
            must(repository.list_question_candidates_by_episode(episode_id))
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            vec![question_id]
        );
        assert_eq!(
            must(repository.list_watch_feed_items_by_episode(episode_id))
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            vec![feed_id]
        );
        assert_eq!(
            must(repository.get_app_setting("chapter.strategy")),
            Some("{\"mode\":\"ai\"}".to_owned())
        );

        (
            episode_id,
            chapter_id,
            revision_id,
            question_id,
            feed_id,
            asset_id,
            task_id,
        )
    };

    let reopened = must(Database::open(path.path()));
    let repository = reopened.repository();
    let episode = must(repository.get_episode(episode_id));
    assert_eq!(
        episode.map(|row| row.stable_id),
        Some("episode:01".to_owned())
    );
    assert_eq!(
        must(repository.get_chapter(chapter_id)).map(|row| row.id),
        Some(chapter_id)
    );
    assert_eq!(
        must(repository.get_chapter_asset(asset_id)).map(|row| row.content_hash),
        Some("sha256:opening".to_owned())
    );
    assert_eq!(
        must(repository.get_chapter_revision(revision_id)).map(|row| row.status),
        Some("accepted".to_owned())
    );
    assert_eq!(
        must(repository.get_question_candidate(question_id)).map(|row| row.id),
        Some(question_id)
    );
    assert_eq!(
        must(repository.get_watch_feed_item(feed_id)).map(|row| row.id),
        Some(feed_id)
    );
    assert_eq!(
        must(repository.get_agent_task(task_id)).map(|row| row.output_contract_version),
        Some(Some("chapter_segment.v1".to_owned()))
    );
}

#[test]
fn repeated_identity_writes_are_idempotent_and_duplicate_projections_are_not_created() {
    let database = must(Database::open_in_memory());
    let repository = database.repository();
    let series_id = must(repository.insert_series(&NewSeries::new(
        "series:idempotent",
        "Idempotent series",
        "fixture",
    )));

    let first_episode =
        must(repository.get_or_create_episode(&NewEpisode::new(series_id, "episode:1", "fixture")));
    let second_episode = must(repository.get_or_create_episode(&NewEpisode::new(
        series_id,
        "episode:1",
        "different-source-must-not-overwrite",
    )));
    assert_eq!(first_episode.id, second_episode.id);
    assert_eq!(second_episode.source, "fixture");

    let mut task_input =
        lumina_library::NewAgentTask::new("task:idempotent", "chapter_segmentation", "1.0");
    task_input.episode_id = Some(first_episode.id);
    let first_task = must(repository.get_or_create_agent_task(&task_input));
    task_input.task_type = "different-task-type-must-not-overwrite".to_owned();
    let second_task = must(repository.get_or_create_agent_task(&task_input));
    assert_eq!(first_task.id, second_task.id);
    assert_eq!(second_task.task_type, "chapter_segmentation");

    let chapter_id = must(repository.insert_chapter(&NewChapter::new(
        first_episode.id,
        "chapter:idempotent",
        0,
        1_000,
        "ai",
    )));
    let asset = NewChapterAsset::new(
        chapter_id,
        "screenshot",
        "assets/frame.webp",
        "hash:one",
        500,
        "capture",
    );
    let _ = must(repository.insert_chapter_asset(&asset));
    assert!(repository.insert_chapter_asset(&asset).is_err());

    let revision =
        NewChapterRevision::new(chapter_id, 1, "summary", "stable revision", "agent", "1.0");
    let _ = must(repository.insert_chapter_revision(&revision));
    assert!(repository.insert_chapter_revision(&revision).is_err());

    let mut question = NewQuestionCandidate::new("Question", "agent", "none", "question:one");
    question.episode_id = Some(first_episode.id);
    let _ = must(repository.insert_question_candidate(&question));
    assert!(repository.insert_question_candidate(&question).is_err());

    let mut feed = NewWatchFeedItem::new("outlook", "agent", "Content", "none", "1.0", "feed:one");
    feed.episode_id = Some(first_episode.id);
    let _ = must(repository.insert_watch_feed_item(&feed));
    assert!(repository.insert_watch_feed_item(&feed).is_err());

    assert_eq!(
        must(repository.list_chapter_assets_by_chapter(chapter_id)).len(),
        1
    );
    assert_eq!(
        must(repository.get_latest_chapter_revision(chapter_id)).map(|row| row.revision_number),
        Some(1)
    );
    assert_eq!(
        must(repository.list_question_candidates_by_episode(first_episode.id)).len(),
        1
    );
    assert_eq!(
        must(repository.list_watch_feed_items_by_episode(first_episode.id)).len(),
        1
    );
}

#[test]
fn legacy_to_authoritative_migration_moves_projections_and_preserves_conflicting_sources() {
    let mut database = must(Database::open_in_memory());
    let (legacy_episode, authoritative_episode) = seed_authoritative_and_legacy(&database);
    let repository = database.repository();

    let moved_chapter = must(repository.insert_chapter(&NewChapter::new(
        legacy_episode,
        "legacy:chapter:1",
        0,
        1_000,
        "legacy",
    )));
    let mut moved_question =
        NewQuestionCandidate::new("Legacy question", "legacy", "none", "legacy:q:1");
    moved_question.episode_id = Some(legacy_episode);
    moved_question.chapter_id = Some(moved_chapter);
    let _ = must(repository.insert_question_candidate(&moved_question));
    let mut moved_feed = NewWatchFeedItem::new(
        "recap",
        "legacy",
        "Legacy feed",
        "none",
        "1",
        "legacy:feed:1",
    );
    moved_feed.episode_id = Some(legacy_episode);
    moved_feed.chapter_id = Some(moved_chapter);
    let _ = must(repository.insert_watch_feed_item(&moved_feed));

    let mut task =
        lumina_library::NewAgentTask::new("legacy:task:1", "chapter_segmentation", "1.0");
    task.episode_id = Some(legacy_episode);
    let task_id = must(repository.get_or_create_agent_task(&task)).id;

    let mut migration = LegacyEpisodeMigration::new(legacy_episode, authoritative_episode);
    migration.task_key_migrations.push(AgentTaskKeyMigration {
        task_id,
        new_task_key: "authoritative:task:1".to_owned(),
    });
    let report = must(database.migrate_legacy_episode(&migration));
    assert_eq!(report.status, EpisodeMigrationStatus::Merged);
    assert_eq!(report.moved_chapters, 1);
    assert_eq!(report.moved_questions, 1);
    assert_eq!(report.moved_feed_items, 1);
    assert_eq!(report.renamed_tasks, 1);

    let repository = database.repository();
    assert_eq!(
        must(repository.list_chapters_by_episode(legacy_episode)).len(),
        0
    );
    assert_eq!(
        must(repository.list_chapters_by_episode(authoritative_episode)).len(),
        1
    );
    assert_eq!(
        must(repository.get_agent_task_by_key("authoritative:task:1")).map(|row| row.episode_id),
        Some(Some(authoritative_episode))
    );

    let conflicting_chapter =
        NewChapter::new(legacy_episode, "chapter:conflict", 2_000, 3_000, "legacy");
    let _ = must(repository.insert_chapter(&conflicting_chapter));
    let _ = must(repository.insert_chapter(&NewChapter::new(
        authoritative_episode,
        "chapter:conflict",
        2_000,
        3_000,
        "tmdb",
    )));
    let mut legacy_feed = NewWatchFeedItem::new(
        "recap",
        "legacy",
        "legacy source",
        "none",
        "1",
        "feed:conflict",
    );
    legacy_feed.episode_id = Some(legacy_episode);
    let _ = must(repository.insert_watch_feed_item(&legacy_feed));
    let mut authoritative_feed = NewWatchFeedItem::new(
        "recap",
        "tmdb",
        "authoritative source",
        "none",
        "1",
        "feed:conflict-authoritative",
    );
    authoritative_feed.episode_id = Some(authoritative_episode);
    let _ = must(repository.insert_watch_feed_item(&authoritative_feed));

    drop(repository);
    let conflict_report = must(
        database.migrate_legacy_episode(&LegacyEpisodeMigration::new(
            legacy_episode,
            authoritative_episode,
        )),
    );
    assert_eq!(conflict_report.status, EpisodeMigrationStatus::Conflict);
    assert!(conflict_report
        .diagnostics
        .iter()
        .any(|item| item.contains("章节")));
    let repository = database.repository();
    assert_eq!(
        must(repository.list_chapters_by_episode(legacy_episode)).len(),
        1
    );
    assert_eq!(
        must(repository.list_chapters_by_episode(authoritative_episode)).len(),
        2
    );
    assert_eq!(
        must(repository.list_watch_feed_items_by_episode(legacy_episode)).len(),
        1
    );
    assert_eq!(
        must(repository.list_watch_feed_items_by_episode(authoritative_episode)).len(),
        2
    );
}

#[test]
fn failed_transaction_rolls_back_every_projection_write_and_hides_sql_details_from_message() {
    let mut database = must(Database::open_in_memory());
    let existing_series = must(database.repository().insert_series(&NewSeries::new(
        "series:existing",
        "Existing",
        "fixture",
    )));

    let result: DatabaseResult<()> = database.transaction(|repository| {
        let _ = repository.insert_series(&NewSeries::new(
            "series:rolled-back",
            "Rolled back",
            "fixture",
        ))?;
        let _ = repository.insert_episode(&NewEpisode::new(
            existing_series,
            "episode:rolled-back",
            "fixture",
        ))?;
        let _ =
            repository.insert_series(&NewSeries::new("series:existing", "Duplicate", "fixture"))?;
        Ok(())
    });

    let error = match result {
        Ok(()) => panic!("duplicate series should abort the transaction"),
        Err(error) => error,
    };
    assert!(!error.message.contains("UNIQUE"));
    assert!(!error.message.contains("SQLite"));
    assert!(!error.message.contains("series:existing"));
    assert!(error.details.is_some());

    let repository = database.repository();
    assert_eq!(
        must(repository.get_series_by_stable_id("series:rolled-back")),
        None
    );
    assert_eq!(
        must(repository.get_episode_by_stable_id(existing_series, "episode:rolled-back")),
        None
    );
}

#[test]
fn error_details_are_diagnostic_only_for_an_unopenable_database_path() {
    let path = TemporaryDatabasePath::new("message-boundary");
    let error = match Database::open(path.path().join("missing-parent").join("db.sqlite3")) {
        Ok(_) => panic!("a database below a missing parent must not open"),
        Err(error) => error,
    };
    assert!(!error.message.contains("missing-parent"));
    assert!(!error.message.contains("sqlite"));
    assert!(error
        .details
        .as_deref()
        .is_some_and(|details| details.contains("missing-parent")));
}
