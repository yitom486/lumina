use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::library::error::LibraryError;
use crate::library::model::{
    GroupResolution, LibraryIndex, LibraryScanEvent, LibraryScanIssue, LibraryStatus, LibraryWatchConfig,
    MediaGroup, MediaMetadataContext, MetadataMediaType, MetadataWriteResult, PendingMediaGroup,
    ResolverPreview, ResolverRunConfig, TmdbConfig, WikiEnrichmentCandidate, WikiEnrichmentPreview,
    WikiGroupStatus, WikiMatchMethod, WikiWriteResult, TmdbGroupStatus,
};
use crate::library::{metadata, scanner, store, RemoteResolver};

struct WatchWorker {
    cancel: Arc<AtomicBool>,
    join: thread::JoinHandle<()>,
}

#[derive(Default)]
struct Runtime {
    config: LibraryWatchConfig,
    last_scan_at_ms: Option<u128>,
    last_scan_error: Option<LibraryScanIssue>,
    indexed_files: usize,
    pending_groups: usize,
}

pub struct MediaLibraryService {
    runtime: Arc<Mutex<Runtime>>,
    worker: Mutex<Option<WatchWorker>>,
    scan_lock: Arc<Mutex<()>>,
}

impl MediaLibraryService {
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(Mutex::new(Runtime::default())),
            worker: Mutex::new(None),
            scan_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn start(&self, config: LibraryWatchConfig) -> Result<LibraryStatus, LibraryError> {
        self.start_with_progress(config, |_| {})
    }

    pub fn start_with_progress<F>(&self, config: LibraryWatchConfig, mut on_event: F) -> Result<LibraryStatus, LibraryError>
    where
        F: FnMut(LibraryScanEvent),
    {
        let roots = validate_config(&config)?;
        self.stop()?;
        {
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| LibraryError::internal(Some("library runtime mutex poisoned")))?;
            runtime.config = LibraryWatchConfig {
                roots: roots
                    .iter()
                    .map(|root| root.to_string_lossy().to_string())
                    .collect(),
                poll_interval_secs: config.poll_interval_secs.max(5),
            };
        }
        self.scan_now_with_progress(&mut on_event)?;

        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let runtime = Arc::clone(&self.runtime);
        let scan_lock = Arc::clone(&self.scan_lock);
        let interval = config.poll_interval_secs.max(5);
        let join = thread::Builder::new()
            .name("media-library-watch".into())
            .spawn(move || watch_loop(runtime, scan_lock, worker_cancel, interval))
            .map_err(|error| {
                LibraryError::internal(Some(&format!("spawn library watcher: {error}")))
            })?;
        let mut worker = self
            .worker
            .lock()
            .map_err(|_| LibraryError::internal(Some("library worker mutex poisoned")))?;
        *worker = Some(WatchWorker { cancel, join });
        drop(worker);
        self.status()
    }

    pub fn stop(&self) -> Result<LibraryStatus, LibraryError> {
        let worker = self
            .worker
            .lock()
            .map_err(|_| LibraryError::internal(Some("library worker mutex poisoned")))?
            .take();
        if let Some(worker) = worker {
            worker.cancel.store(true, Ordering::SeqCst);
            let _ = worker.join.join();
        }
        self.status()
    }

    pub fn scan_now(&self) -> Result<Vec<LibraryIndex>, LibraryError> {
        self.scan_now_with_progress(&mut |_| {})
    }

    fn scan_now_with_progress(
        &self,
        on_event: &mut dyn FnMut(LibraryScanEvent),
    ) -> Result<Vec<LibraryIndex>, LibraryError> {
        let _guard = self
            .scan_lock
            .lock()
            .map_err(|_| LibraryError::internal(Some("library scan mutex poisoned")))?;
        scan_runtime(&self.runtime, on_event)
    }

    pub fn status(&self) -> Result<LibraryStatus, LibraryError> {
        let runtime = self
            .runtime
            .lock()
            .map_err(|_| LibraryError::internal(Some("library runtime mutex poisoned")))?;
        let running = self
            .worker
            .lock()
            .map_err(|_| LibraryError::internal(Some("library worker mutex poisoned")))?
            .is_some();
        Ok(LibraryStatus {
            running,
            roots: runtime.config.roots.clone(),
            poll_interval_secs: runtime.config.poll_interval_secs,
            last_scan_at_ms: runtime.last_scan_at_ms,
            last_scan_error: runtime.last_scan_error.clone(),
            indexed_files: runtime.indexed_files,
            pending_groups: runtime.pending_groups,
        })
    }

    pub fn pending_groups(&self) -> Result<Vec<PendingMediaGroup>, LibraryError> {
        let roots = self
            .runtime
            .lock()
            .map_err(|_| LibraryError::internal(Some("library runtime mutex poisoned")))?
            .config
            .roots
            .clone();
        let mut pending = Vec::new();
        for root in roots {
            let path = PathBuf::from(&root);
            let Some(index) = store::load(&path)? else {
                continue;
            };
            pending.extend(
                index
                    .groups
                    .into_iter()
                    .filter(|group| matches!(group.resolution, GroupResolution::Pending))
                    .map(|group| PendingMediaGroup {
                        root: root.clone(),
                        group,
                    }),
            );
        }
        Ok(pending)
    }

    /// Records the user's fallback title for a group. It intentionally resets
    /// the group to pending so a later resolver run treats it as a new search.
    pub fn set_manual_title(
        &self,
        root: String,
        group_key: String,
        title: String,
    ) -> Result<PendingMediaGroup, LibraryError> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(LibraryError::invalid_input("请填写要匹配的作品名称"));
        }
        self.ensure_configured_root(&root)?;
        let path = PathBuf::from(&root);
        let mut index = store::load(&path)?.ok_or_else(|| {
            LibraryError::group_not_found(Some("library index is not available for root"))
        })?;
        let group = index
            .groups
            .iter_mut()
            .find(|group| group.key == group_key)
            .ok_or_else(|| LibraryError::group_not_found(Some(&group_key)))?;
        group.manual_title = Some(title);
        group.resolution = GroupResolution::Pending;
        let updated = group.clone();
        store::save_if_changed(&path, &index)?;
        Ok(PendingMediaGroup {
            root,
            group: updated,
        })
    }

    /// Runs the optional remote resolver for one group without persisting its
    /// choice. A later reviewed-match operation owns durable TMDb writes.
    pub fn resolve_preview(
        &self,
        root: String,
        group_key: String,
        config: ResolverRunConfig,
    ) -> Result<ResolverPreview, LibraryError> {
        self.ensure_configured_root(&root)?;
        let path = PathBuf::from(&root);
        let index = store::load(&path)?.ok_or_else(|| {
            LibraryError::group_not_found(Some("library index is not available for root"))
        })?;
        let group = index
            .groups
            .into_iter()
            .find(|group| group.key == group_key)
            .ok_or_else(|| LibraryError::group_not_found(Some(&group_key)))?;
        RemoteResolver::new(config)?.preview(&group)
    }

    pub fn apply_tmdb_match(
        &self,
        root: String,
        group_key: String,
        tmdb_id: u64,
        media_type: MetadataMediaType,
        tmdb: TmdbConfig,
    ) -> Result<MetadataWriteResult, LibraryError> {
        self.ensure_configured_root(&root)?;
        let _guard = self
            .scan_lock
            .lock()
            .map_err(|_| LibraryError::internal(Some("library scan mutex poisoned")))?;
        let path = PathBuf::from(&root);
        let mut index = store::load(&path)?.ok_or_else(|| {
            LibraryError::group_not_found(Some("library index is not available for root"))
        })?;
        let group = index
            .groups
            .iter()
            .find(|group| group.key == group_key)
            .cloned()
            .ok_or_else(|| LibraryError::group_not_found(Some(&group_key)))?;
        let result =
            metadata::write_confirmed_metadata(&path, &index, &group, tmdb_id, media_type, &tmdb)?;
        let stored = index
            .groups
            .iter_mut()
            .find(|item| item.key == group_key)
            .ok_or_else(|| LibraryError::group_not_found(Some(&group_key)))?;
        stored.resolution = GroupResolution::Matched {
            tmdb_id,
            media_type,
        };
        store::save_if_changed(&path, &index)?;
        Ok(result)
    }

    pub fn context_for_media(
        &self,
        media_path: String,
    ) -> Result<Option<MediaMetadataContext>, LibraryError> {
        let media_path = PathBuf::from(&media_path);
        let roots = self
            .runtime
            .lock()
            .map_err(|_| LibraryError::internal(Some("library runtime mutex poisoned")))?
            .config
            .roots
            .clone();
        let root = roots
            .into_iter()
            .map(PathBuf::from)
            .filter(|root| media_path.strip_prefix(root).is_ok())
            .max_by_key(|root| root.as_os_str().len());
        let Some(root) = root else {
            return Ok(None);
        };
        let Some(index) = store::load(&root)? else {
            return Ok(None);
        };
        metadata::load_context(&root, &index, &media_path)
    }

    pub fn library_root_for_media(&self, media_path: &str) -> Option<PathBuf> {
        let media_path = PathBuf::from(media_path);
        let roots = self
            .runtime
            .lock()
            .ok()?
            .config
            .roots
            .clone();
        roots
            .into_iter()
            .map(PathBuf::from)
            .filter(|root| media_path.strip_prefix(root).is_ok())
            .max_by_key(|root| root.as_os_str().len())
    }

    pub fn list_groups(&self, root: String) -> Result<Vec<MediaGroup>, LibraryError> {
        self.ensure_configured_root(&root)?;
        let path = PathBuf::from(&root);
        let index = store::load(&path)?.ok_or_else(|| {
            LibraryError::group_not_found(Some("library index is not available for root"))
        })?;
        Ok(index.groups)
    }

    pub fn preview_wikipedia_enrichment(
        &self,
        root: String,
        group_key: String,
        tmdb: TmdbConfig,
    ) -> Result<WikiEnrichmentPreview, LibraryError> {
        self.ensure_configured_root(&root)?;
        let path = PathBuf::from(&root);
        let index = store::load(&path)?.ok_or_else(|| {
            LibraryError::group_not_found(Some("library index is not available for root"))
        })?;
        let group = index
            .groups
            .iter()
            .find(|group| group.key == group_key)
            .ok_or_else(|| LibraryError::group_not_found(Some(&group_key)))?;
        let GroupResolution::Matched {
            tmdb_id,
            media_type,
        } = group.resolution
        else {
            return Err(LibraryError::invalid_input("请先完成 TMDb 匹配"));
        };
        metadata::preview_wikipedia_enrichment(&path, &group_key, tmdb_id, media_type, &tmdb)
    }

    pub fn apply_wikipedia_page(
        &self,
        root: String,
        group_key: String,
        candidate: WikiEnrichmentCandidate,
        match_method: WikiMatchMethod,
        candidates_considered: u32,
    ) -> Result<WikiWriteResult, LibraryError> {
        self.ensure_configured_root(&root)?;
        let path = PathBuf::from(&root);
        let index = store::load(&path)?.ok_or_else(|| {
            LibraryError::group_not_found(Some("library index is not available for root"))
        })?;
        if !index.groups.iter().any(|group| group.key == group_key) {
            return Err(LibraryError::group_not_found(Some(&group_key)));
        }
        metadata::apply_wikipedia_page(
            &path,
            &group_key,
            candidate,
            match_method,
            candidates_considered,
        )
    }

    pub fn refresh_wikipedia_page(
        &self,
        root: String,
        group_key: String,
    ) -> Result<WikiWriteResult, LibraryError> {
        self.ensure_configured_root(&root)?;
        let path = PathBuf::from(&root);
        let index = store::load(&path)?.ok_or_else(|| {
            LibraryError::group_not_found(Some("library index is not available for root"))
        })?;
        if !index.groups.iter().any(|group| group.key == group_key) {
            return Err(LibraryError::group_not_found(Some(&group_key)));
        }
        metadata::refresh_wikipedia_page(&path, &group_key)
    }

    pub fn wikipedia_statuses(&self, root: String) -> Result<Vec<WikiGroupStatus>, LibraryError> {
        self.ensure_configured_root(&root)?;
        let path = PathBuf::from(&root);
        let index = store::load(&path)?.ok_or_else(|| {
            LibraryError::group_not_found(Some("library index is not available for root"))
        })?;
        metadata::wikipedia_statuses_for_root(&path, &index)
    }

    pub fn refresh_tmdb_metadata(
        &self,
        root: String,
        group_key: String,
        tmdb: TmdbConfig,
    ) -> Result<MetadataWriteResult, LibraryError> {
        self.ensure_configured_root(&root)?;
        let path = PathBuf::from(&root);
        let index = store::load(&path)?.ok_or_else(|| {
            LibraryError::group_not_found(Some("library index is not available for root"))
        })?;
        let group = index
            .groups
            .iter()
            .find(|group| group.key == group_key)
            .cloned()
            .ok_or_else(|| LibraryError::group_not_found(Some(&group_key)))?;
        metadata::refresh_tmdb_metadata(&path, &index, &group, &tmdb)
    }

    pub fn tmdb_statuses(&self, root: String) -> Result<Vec<TmdbGroupStatus>, LibraryError> {
        self.ensure_configured_root(&root)?;
        let path = PathBuf::from(&root);
        let index = store::load(&path)?.ok_or_else(|| {
            LibraryError::group_not_found(Some("library index is not available for root"))
        })?;
        metadata::tmdb_statuses_for_root(&path, &index, &index.groups)
    }

    fn ensure_configured_root(&self, root: &str) -> Result<(), LibraryError> {
        let configured = self
            .runtime
            .lock()
            .map_err(|_| LibraryError::internal(Some("library runtime mutex poisoned")))?
            .config
            .roots
            .iter()
            .any(|configured_root| configured_root == root);
        if !configured {
            return Err(LibraryError::invalid_input("请先启用该媒体目录"));
        }
        Ok(())
    }
}

fn validate_config(config: &LibraryWatchConfig) -> Result<Vec<PathBuf>, LibraryError> {
    if config.roots.is_empty() {
        return Err(LibraryError::invalid_directory(Some(
            "no media roots configured",
        )));
    }
    config
        .roots
        .iter()
        .map(PathBuf::from)
        .map(|path| {
            if path.is_dir() {
                Ok(path)
            } else {
                Err(LibraryError::invalid_directory(Some(
                    &path.display().to_string(),
                )))
            }
        })
        .collect()
}

fn watch_loop(
    runtime: Arc<Mutex<Runtime>>,
    scan_lock: Arc<Mutex<()>>,
    cancel: Arc<AtomicBool>,
    interval_secs: u64,
) {
    tracing::info!(interval_secs, "media library watcher started");
    while !cancel.load(Ordering::SeqCst) {
        for _ in 0..interval_secs {
            if cancel.load(Ordering::SeqCst) {
                tracing::info!("media library watcher stopped");
                return;
            }
            thread::sleep(Duration::from_secs(1));
        }
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let result = scan_lock
            .lock()
            .map_err(|_| LibraryError::internal(Some("library scan mutex poisoned")))
            .and_then(|_guard| scan_runtime(&runtime, &mut |_| {}));
        if let Err(error) = result {
            tracing::warn!(code = ?error.code, details = ?error.details, "media library periodic scan failed");
        }
    }
    tracing::info!("media library watcher stopped");
}

fn scan_runtime(
    runtime: &Arc<Mutex<Runtime>>,
    on_event: &mut dyn FnMut(LibraryScanEvent),
) -> Result<Vec<LibraryIndex>, LibraryError> {
    let roots = runtime
        .lock()
        .map_err(|_| LibraryError::internal(Some("library runtime mutex poisoned")))?
        .config
        .roots
        .clone();
    if roots.is_empty() {
        let error = LibraryError::not_running();
        record_scan_failure(runtime, &error)?;
        return Err(error);
    }
    on_event(LibraryScanEvent::Started { root_count: roots.len() });
    let mut indexes = Vec::new();
    let mut indexed_files = 0;
    for (root_index, root) in roots.iter().enumerate() {
        let root = PathBuf::from(root);
        let index = scan_and_store_with_progress(&root, |files| {
            on_event(LibraryScanEvent::Progress {
                roots_completed: root_index,
                root_count: roots.len(),
                indexed_files: indexed_files + files,
            });
        });
        match index {
            Ok(index) => {
                indexed_files += index.files.len();
                indexes.push(index);
                on_event(LibraryScanEvent::Progress {
                    roots_completed: root_index + 1,
                    root_count: roots.len(),
                    indexed_files,
                });
            }
            Err(error) => {
                record_scan_failure(runtime, &error)?;
                on_event(LibraryScanEvent::Failed { code: error.code, message: error.message.clone() });
                return Err(error);
            }
        }
    }
    update_runtime(runtime, &indexes)?;
    let state = runtime
        .lock()
        .map_err(|_| LibraryError::internal(Some("library runtime mutex poisoned")))?;
    on_event(LibraryScanEvent::Finished {
        indexed_files: state.indexed_files,
        pending_groups: state.pending_groups,
    });
    Ok(indexes)
}

fn record_scan_failure(
    runtime: &Arc<Mutex<Runtime>>,
    error: &LibraryError,
) -> Result<(), LibraryError> {
    let mut state = runtime
        .lock()
        .map_err(|_| LibraryError::internal(Some("library runtime mutex poisoned")))?;
    state.last_scan_error = Some(LibraryScanIssue {
        code: error.code,
        message: error.message.clone(),
    });
    Ok(())
}

fn scan_and_store_with_progress<F>(root: &std::path::Path, on_file: F) -> Result<LibraryIndex, LibraryError>
where
    F: FnMut(usize),
{
    let previous = store::load(root)?;
    let index = store::preserve_resolutions(scanner::scan_root_with_progress(root, on_file)?, previous.as_ref());
    store::save_if_changed(root, &index)?;
    Ok(index)
}

fn update_runtime(
    runtime: &Arc<Mutex<Runtime>>,
    indexes: &[LibraryIndex],
) -> Result<(), LibraryError> {
    let mut state = runtime
        .lock()
        .map_err(|_| LibraryError::internal(Some("library runtime mutex poisoned")))?;
    state.last_scan_at_ms = indexes.iter().map(|index| index.updated_at_ms).max();
    state.last_scan_error = None;
    state.indexed_files = indexes.iter().map(|index| index.files.len()).sum();
    state.pending_groups = indexes
        .iter()
        .map(|index| {
            index
                .groups
                .iter()
                .filter(|group| matches!(group.resolution, GroupResolution::Pending))
                .count()
        })
        .sum();
    Ok(())
}

impl Default for MediaLibraryService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn test_root() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("lumina-library-{suffix}"))
    }

    #[test]
    fn first_scan_writes_and_reloads_local_index() {
        let root = test_root();
        fs::create_dir_all(&root).expect("create test root");
        fs::write(root.join("Example.Show.S01E01.mkv"), b"video").expect("write media");

        let service = MediaLibraryService::new();
        let status = service
            .start(LibraryWatchConfig {
                roots: vec![root.to_string_lossy().to_string()],
                poll_interval_secs: 5,
            })
            .expect("start watcher");
        assert!(status.running);
        assert_eq!(status.indexed_files, 1);
        assert_eq!(status.pending_groups, 1);
        let pending = service.pending_groups().expect("pending groups");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].group.key, "Example.Show");

        let titled = service
            .set_manual_title(
                root.to_string_lossy().to_string(),
                "Example.Show".into(),
                "示例剧集".into(),
            )
            .expect("manual title");
        assert_eq!(titled.group.manual_title.as_deref(), Some("示例剧集"));
        let reloaded = store::load(&root)
            .expect("reload index")
            .expect("index exists");
        assert_eq!(reloaded.groups[0].manual_title.as_deref(), Some("示例剧集"));

        let stored = store::load(&root)
            .expect("load index")
            .expect("index exists");
        assert_eq!(stored.files.len(), 1);
        assert_eq!(
            stored.groups[0].kind,
            crate::library::model::MediaGroupKind::Series
        );

        let mut resolved_index = stored;
        resolved_index.groups[0].resolution = GroupResolution::Ignored;
        update_runtime(&service.runtime, &[resolved_index]).expect("update runtime");
        assert_eq!(service.status().expect("status").pending_groups, 0);

        service.stop().expect("stop watcher");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_scan_is_exposed_as_safe_runtime_status() {
        let missing_root = test_root();
        let service = MediaLibraryService::new();
        {
            let mut runtime = service.runtime.lock().expect("lock runtime");
            runtime.config.roots = vec![missing_root.to_string_lossy().to_string()];
        }

        assert!(service.scan_now().is_err());
        let issue = service
            .status()
            .expect("status")
            .last_scan_error
            .expect("safe scan issue");
        assert!(issue.message.contains("媒体目录"));
        assert!(!issue.message.contains(&missing_root.to_string_lossy().to_string()));
    }
}
