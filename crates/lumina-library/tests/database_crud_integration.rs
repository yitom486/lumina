use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lumina_library::{
    Database, DatabaseResult, NewAgentAttempt, NewAgentTask, NewChapter, NewChapterAsset,
    NewChapterRevision, NewEpisode, NewQuestionCandidate, NewSeries, NewWatchFeedItem, Repository,
};

static NEXT_DATABASE_ID: AtomicU64 = AtomicU64::new(1);

struct TemporaryDatabasePath(PathBuf);

impl TemporaryDatabasePath {
    fn new(test_name: &str) -> Self {
        let sequence = NEXT_DATABASE_ID.fetch_add(1, Ordering::Relaxed);
        let file_name = format!(
            "lumina-crud-{test_name}-{}-{sequence}.sqlite3",
            std::process::id()
        );
        Self(std::env::temp_dir().join(file_name))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn sidecar(&self, suffix: &str) -> PathBuf {
        PathBuf::from(format!("{}-{suffix}", self.0.display()))
    }
}

impl Drop for TemporaryDatabasePath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.sidecar("wal"));
        let _ = std::fs::remove_file(self.sidecar("shm"));
    }
}

fn must<T>(result: DatabaseResult<T>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected database error: {}", error.message),
    }
}

fn seed_episode(repository: &Repository<'_>, key: &str) -> i64 {
    let series_id = must(repository.insert_series(&NewSeries::new(
        format!("series:{key}"),
        "Fixture series",
        "fixture",
    )));
    must(repository.get_or_create_episode(&NewEpisode::new(
        series_id,
        format!("episode:{key}"),
        "fixture",
    )))
    .id
}

fn seed_chapter_task(repository: &Repository<'_>, episode_id: i64, key: &str) -> i64 {
    let mut input = NewAgentTask::new(
        format!("task:{key}"),
        "chapter_segmentation",
        "chapter-agent.v1",
    );
    input.episode_id = Some(episode_id);
    input.output_contract_version = Some("chapter_segment.v1".to_owned());
    must(repository.get_or_create_agent_task(&input)).id
}

#[test]
fn series_and_episode_reads_by_id_key_and_identity_survive_restart() {
    let path = TemporaryDatabasePath::new("identity");
    let (series_id, episode_id) = {
        let database = must(Database::open(path.path()));
        let repository = database.repository();
        let series_id = must(repository.insert_series(&NewSeries::new(
            "tmdb:tv:identity",
            "Identity fixture",
            "tmdb",
        )));
        let mut episode_input = NewEpisode::new(series_id, "s01e02", "tmdb");
        episode_input.season_number = Some(1);
        episode_input.episode_number = Some(2);
        episode_input.title = Some("The identity episode".to_owned());
        episode_input.duration_ms = Some(60_000);
        let episode_id = must(repository.insert_episode(&episode_input));

        assert_eq!(
            must(repository.get_series(series_id)).map(|row| row.stable_id),
            Some("tmdb:tv:identity".to_owned())
        );
        assert_eq!(
            must(repository.get_series_by_stable_id("tmdb:tv:identity")).map(|row| row.id),
            Some(series_id)
        );
        assert_eq!(
            must(repository.get_episode(episode_id)).map(|row| row.title),
            Some(Some("The identity episode".to_owned()))
        );
        assert_eq!(
            must(repository.get_episode_by_stable_id(series_id, "s01e02")).map(|row| row.id),
            Some(episode_id)
        );
        assert_eq!(
            must(repository.get_episode_by_identity("tmdb:tv:identity", "s01e02"))
                .map(|row| row.duration_ms),
            Some(Some(60_000))
        );

        (series_id, episode_id)
    };

    let database = must(Database::open(path.path()));
    let repository = database.repository();
    assert_eq!(
        must(repository.get_series(series_id)).map(|row| row.id),
        Some(series_id)
    );
    assert_eq!(
        must(repository.get_episode(episode_id)).map(|row| row.id),
        Some(episode_id)
    );
    let database_path = path.path().to_owned();
    drop(database);
    drop(path);
    assert!(!database_path.exists());
}

#[test]
fn chapter_outline_draft_updates_and_projection_reads_are_idempotent() {
    let path = TemporaryDatabasePath::new("chapters");
    {
        let database = must(Database::open(path.path()));
        let repository = database.repository();
        let episode_id = seed_episode(&repository, "chapters");
        let task_id = seed_chapter_task(&repository, episode_id, "chapters");

        let mut first_input = NewChapter::new(episode_id, "chapter:opening", 1_000, 8_000, "ai");
        first_input.title = Some("Opening outline".to_owned());
        let first =
            must(repository.upsert_draft_chapter_for_agent_task(task_id, episode_id, &first_input));
        assert_eq!(first.status, "draft");
        assert_eq!(first.title.as_deref(), Some("Opening outline"));

        let mut second_input = NewChapter::new(episode_id, "chapter:middle", 8_000, 16_000, "ai");
        second_input.title = Some("Middle outline".to_owned());
        let second = must(repository.upsert_draft_chapter_for_agent_task(
            task_id,
            episode_id,
            &second_input,
        ));
        must(repository.replace_agent_task_chapter_outline(task_id, episode_id, &[first.id]));
        assert_eq!(must(repository.get_chapter(second.id)), None);

        let mut retry_input = NewChapter::new(episode_id, "chapter:opening", 1_200, 8_400, "ai");
        retry_input.spoiler_level = "major".to_owned();
        retry_input.title = Some("Opening retry".to_owned());
        let retried =
            must(repository.upsert_draft_chapter_for_agent_task(task_id, episode_id, &retry_input));
        assert_eq!(retried.id, first.id);
        assert_eq!(retried.start_ms, 1_200);
        assert_eq!(retried.end_ms, 8_400);
        assert_eq!(retried.spoiler_level, "major");

        let updated = must(repository.update_draft_chapter_for_agent_task(
            task_id,
            episode_id,
            first.id,
            Some("Opening final"),
            Some("The evidence establishes the conflict."),
        ));
        assert_eq!(updated.id, first.id);
        assert_eq!(updated.title.as_deref(), Some("Opening final"));
        assert_eq!(
            updated.mainline.as_deref(),
            Some("The evidence establishes the conflict.")
        );
        assert!(repository
            .update_draft_chapter_for_agent_task(task_id, episode_id, first.id, None, None)
            .is_err());

        let mut asset_input = NewChapterAsset::new(
            first.id,
            "screenshot",
            "assets/opening.webp",
            "sha256:opening",
            4_000,
            "capture",
        );
        asset_input.width = Some(1_280);
        asset_input.height = Some(720);
        let asset =
            must(repository.insert_chapter_asset_for_agent_task(task_id, episode_id, &asset_input));
        let same_asset =
            must(repository.insert_chapter_asset_for_agent_task(task_id, episode_id, &asset_input));
        assert_eq!(same_asset.id, asset.id);
        assert_eq!(same_asset.content_hash, "sha256:opening");
        assert_eq!(same_asset.width, Some(1_280));
        assert_eq!(
            must(repository.list_chapter_assets_by_chapter(first.id)).len(),
            1
        );
        assert_eq!(
            must(repository.get_chapter_asset(asset.id)).map(|row| row.path),
            Some("assets/opening.webp".to_owned())
        );

        let mut revision_input = NewChapterRevision::new(
            first.id,
            1,
            "recap",
            "Initial recap",
            "chapter_agent",
            "chapter-agent.v1",
        );
        let revision = must(repository.insert_draft_revision_for_agent_task(
            task_id,
            episode_id,
            &revision_input,
        ));
        revision_input.content = "Updated recap".to_owned();
        let retried_revision = must(repository.insert_draft_revision_for_agent_task(
            task_id,
            episode_id,
            &revision_input,
        ));
        assert_eq!(retried_revision.id, revision.id);
        assert_eq!(retried_revision.content, "Updated recap");
        assert_eq!(
            must(repository.get_latest_chapter_revision(first.id)).map(|row| row.content),
            Some("Updated recap".to_owned())
        );

        let invalid = NewChapter::new(episode_id, "chapter:invalid", 9_000, 8_000, "ai");
        assert!(repository
            .upsert_draft_chapter_for_agent_task(task_id, episode_id, &invalid)
            .is_err());
    }
    let database_path = path.path().to_owned();
    let wal_path = path.sidecar("wal");
    let shm_path = path.sidecar("shm");
    drop(path);
    assert!(!database_path.exists());
    assert!(!wal_path.exists());
    assert!(!shm_path.exists());
}

#[test]
fn agent_task_claim_status_attempt_session_and_completion_follow_state_rules() {
    let path = TemporaryDatabasePath::new("agent-task");
    {
        let database = must(Database::open(path.path()));
        let repository = database.repository();
        let episode_id = seed_episode(&repository, "agent-task");
        let mut input = NewAgentTask::new(
            "task:agent-task",
            "chapter_segmentation",
            "chapter-agent.v1",
        );
        input.episode_id = Some(episode_id);
        input.max_attempts = 2;
        let first = must(repository.get_or_create_agent_task(&input));

        input.task_type = "must-not-rewrite-existing-task".to_owned();
        let same = must(repository.get_or_create_agent_task(&input));
        assert_eq!(same.id, first.id);
        assert_eq!(same.task_type, "chapter_segmentation");

        let claimed = must(repository.claim_agent_task_by_key("task:agent-task"));
        assert_eq!(
            claimed.as_ref().map(|task| task.status.as_str()),
            Some("running")
        );
        assert_eq!(claimed.as_ref().map(|task| task.attempt_count), Some(1));
        assert_eq!(
            must(repository.claim_agent_task_by_key("task:agent-task")),
            None
        );

        let attempt_one =
            NewAgentAttempt::new(first.id, 1, "initial", "running", "chapter-agent.v1", 100);
        let attempt_one_id = must(repository.insert_agent_attempt(&attempt_one));
        assert!(must(repository.update_agent_attempt_status(
            attempt_one_id,
            "validation_failure",
            Some("missing mainline"),
            200,
        )));
        assert_eq!(
            must(repository.get_agent_attempt(attempt_one_id)).map(|attempt| attempt.status),
            Some("validation_failure".to_owned())
        );

        assert!(must(repository.update_agent_task_status(
            first.id,
            "validation_failure",
            1,
            1,
            Some("missing mainline"),
        )));
        assert!(must(repository.is_agent_task_active("task:agent-task")));
        assert!(must(repository.update_agent_task_session_id(
            "task:agent-task",
            Some("session-001"),
        )));
        assert_eq!(
            must(repository.get_agent_task_by_key("task:agent-task")).map(|task| task.session_id),
            Some(Some("session-001".to_owned()))
        );

        let claimed_again = must(repository.claim_agent_task_by_key("task:agent-task"));
        assert_eq!(
            claimed_again.as_ref().map(|task| task.attempt_count),
            Some(2)
        );
        let attempt_two =
            NewAgentAttempt::new(first.id, 2, "retry", "running", "chapter-agent.v1", 300);
        let attempt_two_id = must(repository.insert_agent_attempt(&attempt_two));
        assert!(must(repository.update_agent_attempt_status(
            attempt_two_id,
            "succeeded",
            None,
            400,
        )));
        assert!(must(repository.complete_agent_task(
            first.id,
            "succeeded",
            2,
            1,
            None,
            r#"{"chapters":[]}"#,
        )));

        let completed = match must(repository.get_agent_task(first.id)) {
            Some(task) => task,
            None => panic!("completed task should remain readable"),
        };
        assert_eq!(completed.status, "succeeded");
        assert_eq!(completed.attempt_count, 2);
        assert_eq!(completed.retry_count, 1);
        assert_eq!(completed.output_json.as_deref(), Some(r#"{"chapters":[]}"#));
        assert!(!must(repository.is_agent_task_active("task:agent-task")));
        assert_eq!(
            must(repository.claim_agent_task_by_key("task:agent-task")),
            None
        );
        assert!(repository
            .update_agent_task_status(first.id, "running", 3, 0, None)
            .is_err());
        assert!(repository
            .update_agent_task_session_id("task:agent-task", Some(""))
            .is_err());
    }
    let database_path = path.path().to_owned();
    drop(path);
    assert!(!database_path.exists());
}

#[test]
fn draft_question_feed_reconciliation_and_publish_project_terminal_state() {
    let path = TemporaryDatabasePath::new("publish");
    {
        let mut database = must(Database::open(path.path()));
        let repository = database.repository();
        let episode_id = seed_episode(&repository, "publish");
        let task_id = seed_chapter_task(&repository, episode_id, "publish");
        let mut chapter_input = NewChapter::new(episode_id, "chapter:publish", 1_000, 8_000, "ai");
        chapter_input.title = Some("Publish chapter".to_owned());
        let chapter = must(repository.upsert_draft_chapter_for_agent_task(
            task_id,
            episode_id,
            &chapter_input,
        ));
        must(repository.update_draft_chapter_for_agent_task(
            task_id,
            episode_id,
            chapter.id,
            None,
            Some("A complete chapter mainline."),
        ));

        let mut keep_question = NewQuestionCandidate::new(
            "What evidence changes the interpretation?",
            "chapter_agent",
            "current_chapter",
            "question:keep",
        );
        keep_question.episode_id = Some(episode_id);
        keep_question.chapter_id = Some(chapter.id);
        keep_question.task_id = Some(task_id);
        keep_question.batch_key = Some("batch-1".to_owned());
        let keep_question = must(repository.insert_question_candidate_for_agent_task(
            task_id,
            episode_id,
            &keep_question,
        ));

        let mut stale_question = NewQuestionCandidate::new(
            "Stale question",
            "chapter_agent",
            "current_chapter",
            "question:stale",
        );
        stale_question.episode_id = Some(episode_id);
        stale_question.chapter_id = Some(chapter.id);
        stale_question.task_id = Some(task_id);
        stale_question.batch_key = Some("batch-1".to_owned());
        let stale_question = must(repository.insert_question_candidate_for_agent_task(
            task_id,
            episode_id,
            &stale_question,
        ));

        let mut selected_question = NewQuestionCandidate::new(
            "Selected question",
            "chapter_agent",
            "current_chapter",
            "question:selected",
        );
        selected_question.episode_id = Some(episode_id);
        selected_question.chapter_id = Some(chapter.id);
        selected_question.task_id = Some(task_id);
        selected_question.batch_key = Some("batch-1".to_owned());
        selected_question.selected_at_ms = Some(123);
        let selected_question = must(repository.insert_question_candidate_for_agent_task(
            task_id,
            episode_id,
            &selected_question,
        ));
        must(repository.remove_stale_draft_questions_for_agent_task(
            task_id,
            episode_id,
            chapter.id,
            "batch-1",
            std::slice::from_ref(&keep_question.dedupe_fingerprint),
        ));
        assert!(must(repository.get_question_candidate(stale_question.id)).is_none());
        assert!(must(repository.get_question_candidate(selected_question.id)).is_some());
        assert_eq!(
            must(repository.get_question_candidate(keep_question.id)).map(|row| row.question),
            Some("What evidence changes the interpretation?".to_owned())
        );

        let mut keep_feed = NewWatchFeedItem::new(
            "recap",
            "chapter_agent",
            "Draft recap",
            "current_chapter",
            "feed.v1",
            format!("chapter-feed:{task_id}:{}:draft-1:recap", chapter.id),
        );
        keep_feed.episode_id = Some(episode_id);
        keep_feed.chapter_id = Some(chapter.id);
        keep_feed.task_id = Some(task_id);
        let keep_feed =
            must(repository.insert_draft_feed_item_for_agent_task(task_id, episode_id, &keep_feed));

        let mut stale_feed = NewWatchFeedItem::new(
            "question",
            "chapter_agent",
            "Stale feed item",
            "current_chapter",
            "feed.v1",
            format!("chapter-feed:{task_id}:{}:draft-1:question", chapter.id),
        );
        stale_feed.episode_id = Some(episode_id);
        stale_feed.chapter_id = Some(chapter.id);
        stale_feed.task_id = Some(task_id);
        let stale_feed = must(repository.insert_draft_feed_item_for_agent_task(
            task_id,
            episode_id,
            &stale_feed,
        ));

        let mut feed_retry = NewWatchFeedItem::new(
            "recap",
            "chapter_agent",
            "Updated draft recap",
            "current_chapter",
            "feed.v2",
            keep_feed.dedupe_key.clone(),
        );
        feed_retry.episode_id = Some(episode_id);
        feed_retry.chapter_id = Some(chapter.id);
        feed_retry.task_id = Some(task_id);
        let retried_feed = must(repository.insert_draft_feed_item_for_agent_task(
            task_id,
            episode_id,
            &feed_retry,
        ));
        assert_eq!(retried_feed.id, keep_feed.id);
        assert_eq!(retried_feed.content, "Updated draft recap");
        must(repository.remove_stale_draft_feed_items_for_agent_task(
            task_id,
            episode_id,
            chapter.id,
            "draft-1",
            &["recap".to_owned()],
        ));
        assert!(must(repository.get_watch_feed_item(stale_feed.id)).is_none());

        let asset_input = NewChapterAsset::new(
            chapter.id,
            "screenshot",
            "assets/publish.webp",
            "sha256:publish",
            2_000,
            "capture",
        );
        must(repository.insert_chapter_asset_for_agent_task(task_id, episode_id, &asset_input));
        let revision_input = NewChapterRevision::new(
            chapter.id,
            1,
            "recap",
            "Publishable recap",
            "chapter_agent",
            "chapter-agent.v1",
        );
        must(repository.insert_draft_revision_for_agent_task(task_id, episode_id, &revision_input));

        let claimed = must(repository.claim_agent_task_by_key("task:publish"));
        assert_eq!(claimed.as_ref().map(|task| task.attempt_count), Some(1));
        let attempt =
            NewAgentAttempt::new(task_id, 1, "initial", "running", "chapter-agent.v1", 500);
        let attempt_id = must(repository.insert_agent_attempt(&attempt));
        drop(repository);

        let published = must(database.transaction(|repository| {
            repository.publish_agent_chapter_task(
                task_id,
                attempt_id,
                episode_id,
                10_000,
                r#"{"chapters":[{"stableId":"chapter:publish"}]}"#,
            )
        }));
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].status, "ready");

        let repository = database.repository();
        assert_eq!(
            must(repository.get_chapter(chapter.id)).map(|row| row.status),
            Some("ready".to_owned())
        );
        assert_eq!(
            must(repository.get_latest_chapter_revision(chapter.id)).map(|row| row.status),
            Some("accepted".to_owned())
        );
        assert!(must(repository.get_watch_feed_item(keep_feed.id))
            .and_then(|row| row.published_at_ms)
            .is_some());
        assert_eq!(
            must(repository.get_agent_task(task_id)).map(|row| row.status),
            Some("succeeded".to_owned())
        );
        assert_eq!(
            must(repository.get_agent_attempt(attempt_id)).map(|row| row.status),
            Some("succeeded".to_owned())
        );
    }
    let database_path = path.path().to_owned();
    drop(path);
    assert!(!database_path.exists());
}

#[test]
fn settings_update_is_durable_across_restart_and_cleanup_removes_sqlite_sidecars() {
    let path = TemporaryDatabasePath::new("settings");
    {
        let database = must(Database::open(path.path()));
        let repository = database.repository();
        must(repository.set_app_setting("chapter.strategy", r#"{"mode":"ai","version":1}"#));
        must(repository.set_app_setting("chapter.strategy", r#"{"mode":"container","version":2}"#));
        assert_eq!(
            must(repository.get_app_setting("chapter.strategy")),
            Some(r#"{"mode":"container","version":2}"#.to_owned())
        );
        assert_eq!(must(repository.get_app_setting("missing.setting")), None);
        assert!(repository.set_app_setting("", "{}").is_err());
    }

    assert!(path.path().exists());
    {
        let database = must(Database::open(path.path()));
        let repository = database.repository();
        assert_eq!(
            must(repository.get_app_setting("chapter.strategy")),
            Some(r#"{"mode":"container","version":2}"#.to_owned())
        );
    }

    let database_path = path.path().to_owned();
    let wal_path = path.sidecar("wal");
    let shm_path = path.sidecar("shm");
    assert!(database_path.exists());
    drop(path);
    assert!(!database_path.exists());
    assert!(!wal_path.exists());
    assert!(!shm_path.exists());
}
