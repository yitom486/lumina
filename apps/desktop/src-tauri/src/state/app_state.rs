//! Tauri State for Player Runtime + native video surface + event Channel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::acp::AcpService;
use crate::asr::AsrService;
use crate::library::MediaLibraryService;
use crate::mcp::PromptSnapshotState;
use crate::notes::NoteService;
use crate::player::error::PlayerError;
use crate::player::model::PlayerEvent;
use crate::player::mpv::window::VideoSurface;
use crate::player::PlayerService;
use crate::ytdl::provider::ProviderService;
use crate::ytdl::YtdlService;

const POSITION_TICK_MS: u64 = 200;

/// Cap for workshop job snapshots (per spec: 32, evict oldest terminal).
pub const WORKSHOP_JOB_CAP: usize = 32;

/// Translation/proofread workshop job phase for cross-panel resume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkshopJobPhase {
    Running,
    Finished,
    Failed,
}

/// Ledger snapshot for one workshop job (additive: never changes old DTOs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopJobSnapshot {
    pub job_id: String,
    pub media_path: String,
    pub choice_id: String,
    pub target_lang: String,
    pub phase: WorkshopJobPhase,
    pub done: Option<usize>,
    pub total: Option<usize>,
    pub message: String,
    pub updated_at_ms: u64,
}

/// Wall-clock millis for ledger ordering (poison-safe: falls back to 0).
pub fn workshop_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn evict_workshop_if_needed(map: &mut HashMap<String, WorkshopJobSnapshot>, incoming_job_id: &str) {
    if map.contains_key(incoming_job_id) {
        return;
    }
    if map.len() < WORKSHOP_JOB_CAP {
        return;
    }
    let mut oldest_terminal: Option<(String, u64)> = None;
    for (id, snapshot) in map.iter() {
        if !matches!(
            snapshot.phase,
            WorkshopJobPhase::Finished | WorkshopJobPhase::Failed
        ) {
            continue;
        }
        let is_older = match &oldest_terminal {
            None => true,
            Some((_, oldest_ms)) => snapshot.updated_at_ms < *oldest_ms,
        };
        if is_older {
            oldest_terminal = Some((id.clone(), snapshot.updated_at_ms));
        }
    }
    if let Some((evict_id, _)) = oldest_terminal {
        map.remove(&evict_id);
        return;
    }
    let mut oldest: Option<(String, u64)> = None;
    for (id, snapshot) in map.iter() {
        let is_older = match &oldest {
            None => true,
            Some((_, oldest_ms)) => snapshot.updated_at_ms < *oldest_ms,
        };
        if is_older {
            oldest = Some((id.clone(), snapshot.updated_at_ms));
        }
    }
    if let Some((evict_id, _)) = oldest {
        map.remove(&evict_id);
    }
}

pub struct AppState {
    player: Mutex<PlayerService>,
    surface: Mutex<Option<VideoSurface>>,
    events: Mutex<Option<Channel<PlayerEvent>>>,
    workshop_jobs: Mutex<HashMap<String, WorkshopJobSnapshot>>,
    pub asr: Arc<AsrService>,
    pub acp: Arc<AcpService>,
    /// Warm chat-snapshot state (M5: owned by app, used by the ACP adapter).
    pub prompt_snapshots: Arc<Mutex<PromptSnapshotState>>,
    pub library: Arc<MediaLibraryService>,
    pub notes: Arc<NoteService>,
    pub ytdl: Arc<YtdlService>,
    pub provider: Arc<ProviderService>,
    ticker_started: AtomicBool,
    shutdown: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        crate::acp::adapter::install_session_environment();
        Self {
            player: Mutex::new(PlayerService::new()),
            surface: Mutex::new(None),
            events: Mutex::new(None),
            workshop_jobs: Mutex::new(HashMap::new()),
            asr: Arc::new(AsrService::new()),
            acp: Arc::new(AcpService::new()),
            prompt_snapshots: Arc::new(Mutex::new(PromptSnapshotState::default())),
            library: Arc::new(MediaLibraryService::new()),
            notes: Arc::new(NoteService::new()),
            ytdl: Arc::new(YtdlService::new()),
            provider: Arc::new(ProviderService::new()),
            ticker_started: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
        }
    }

    /// Insert a workshop snapshot (job start). Evicts oldest terminal at cap.
    pub fn record_workshop_snapshot(&self, snapshot: WorkshopJobSnapshot) {
        let Ok(mut jobs) = self.workshop_jobs.lock() else {
            tracing::warn!("workshop ledger lock failed on insert");
            return;
        };
        evict_workshop_if_needed(&mut jobs, &snapshot.job_id);
        jobs.insert(snapshot.job_id.clone(), snapshot);
    }

    /// Update progress for a known job; returns the fresh snapshot for emit.
    pub fn update_workshop_progress(
        &self,
        job_id: &str,
        done: Option<usize>,
        total: Option<usize>,
        message: String,
    ) -> Option<WorkshopJobSnapshot> {
        let Ok(mut jobs) = self.workshop_jobs.lock() else {
            tracing::warn!("workshop ledger lock failed on progress");
            return None;
        };
        let entry = jobs.get_mut(job_id)?;
        entry.done = done;
        entry.total = total;
        entry.message = message;
        entry.updated_at_ms = workshop_now_ms();
        Some(entry.clone())
    }

    /// Mark a job terminal; returns the fresh snapshot for emit.
    pub fn finish_workshop_job(
        &self,
        job_id: &str,
        phase: WorkshopJobPhase,
        message: String,
    ) -> Option<WorkshopJobSnapshot> {
        let Ok(mut jobs) = self.workshop_jobs.lock() else {
            tracing::warn!("workshop ledger lock failed on finish");
            return None;
        };
        let entry = jobs.get_mut(job_id)?;
        entry.phase = phase;
        entry.message = message;
        entry.updated_at_ms = workshop_now_ms();
        Some(entry.clone())
    }

    /// Latest snapshot for one media path (for panel remount resume).
    pub fn workshop_status_for_media(&self, media_path: &str) -> Option<WorkshopJobSnapshot> {
        let Ok(jobs) = self.workshop_jobs.lock() else {
            tracing::warn!("workshop ledger lock failed on status");
            return None;
        };
        jobs.values()
            .filter(|snapshot| snapshot.media_path == media_path)
            .max_by_key(|snapshot| snapshot.updated_at_ms)
            .cloned()
    }

    pub fn ytdl(&self) -> &YtdlService {
        self.ytdl.as_ref()
    }

    pub fn provider(&self) -> &ProviderService {
        self.provider.as_ref()
    }

    pub fn with_player<T>(
        &self,
        f: impl FnOnce(&mut PlayerService) -> Result<T, PlayerError>,
    ) -> Result<T, PlayerError> {
        let mut player = self
            .player
            .lock()
            .map_err(|_| PlayerError::internal(Some("player mutex poisoned")))?;
        f(&mut player)
    }

    pub fn set_surface(&self, surface: VideoSurface) -> Result<(), PlayerError> {
        let mut slot = self
            .surface
            .lock()
            .map_err(|_| PlayerError::internal(Some("surface mutex poisoned")))?;
        *slot = Some(surface);
        Ok(())
    }

    pub fn with_surface<T>(
        &self,
        f: impl FnOnce(&VideoSurface) -> Result<T, PlayerError>,
    ) -> Result<T, PlayerError> {
        let slot = self
            .surface
            .lock()
            .map_err(|_| PlayerError::internal(Some("surface mutex poisoned")))?;
        let surface = slot
            .as_ref()
            .ok_or_else(|| PlayerError::internal(Some("video surface not ready")))?;
        f(surface)
    }

    pub fn set_event_channel(&self, channel: Channel<PlayerEvent>) -> Result<(), PlayerError> {
        let mut slot = self
            .events
            .lock()
            .map_err(|_| PlayerError::internal(Some("event channel mutex poisoned")))?;
        *slot = Some(channel);
        tracing::info!("player event channel subscribed");
        Ok(())
    }

    pub fn emit(&self, event: PlayerEvent) {
        let Ok(slot) = self.events.lock() else {
            return;
        };
        let Some(channel) = slot.as_ref() else {
            return;
        };
        if let Err(error) = channel.send(event) {
            tracing::warn!(%error, "failed to send player event");
        }
    }

    pub fn emit_all(&self, events: Vec<PlayerEvent>) {
        for event in events {
            self.emit(event);
        }
    }

    pub fn ensure_event_ticker(&self, app: AppHandle) {
        if self.ticker_started.swap(true, Ordering::SeqCst) {
            return;
        }

        let result = std::thread::Builder::new()
            .name("player-events".into())
            .spawn(move || {
                tracing::info!(
                    interval_ms = POSITION_TICK_MS,
                    "player event ticker started"
                );
                loop {
                    std::thread::sleep(Duration::from_millis(POSITION_TICK_MS));
                    let Some(state) = app.try_state::<AppState>() else {
                        break;
                    };
                    if state.shutdown.load(Ordering::SeqCst) {
                        break;
                    }

                    let events = match state.with_player(|player| {
                        if player.is_shutdown() {
                            return Ok(Vec::new());
                        }
                        Ok(player.poll_tick())
                    }) {
                        Ok(events) => events,
                        Err(_) => continue,
                    };

                    if !events.is_empty() {
                        state.emit_all(events);
                    }
                }
                tracing::info!("player event ticker stopped");
            });

        if let Err(error) = result {
            self.ticker_started.store(false, Ordering::SeqCst);
            tracing::error!(%error, "failed to spawn player event ticker");
        }
    }

    pub fn mark_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.events.lock() {
            *slot = None;
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(
        job_id: &str,
        media_path: &str,
        phase: WorkshopJobPhase,
        updated_at_ms: u64,
    ) -> WorkshopJobSnapshot {
        WorkshopJobSnapshot {
            job_id: job_id.to_string(),
            media_path: media_path.to_string(),
            choice_id: "c1".to_string(),
            target_lang: "zh".to_string(),
            phase,
            done: None,
            total: None,
            message: "progress".to_string(),
            updated_at_ms,
        }
    }

    #[test]
    fn ledger_inserts_and_returns_latest_for_media() {
        let state = AppState::new();
        state.record_workshop_snapshot(snapshot("j1", "/a.mp4", WorkshopJobPhase::Running, 10));
        state.record_workshop_snapshot(snapshot("j2", "/a.mp4", WorkshopJobPhase::Running, 20));
        state.record_workshop_snapshot(snapshot("j3", "/b.mp4", WorkshopJobPhase::Running, 30));
        let latest = state.workshop_status_for_media("/a.mp4");
        assert!(latest.is_some());
        assert_eq!(latest.map(|item| item.job_id), Some("j2".to_string()));
        assert!(state.workshop_status_for_media("/missing.mp4").is_none());
    }

    #[test]
    fn ledger_updates_progress_and_finish() {
        let state = AppState::new();
        state.record_workshop_snapshot(snapshot("j1", "/a.mp4", WorkshopJobPhase::Running, 1));
        let Some(updated) =
            state.update_workshop_progress("j1", Some(2), Some(5), "half".to_string())
        else {
            panic!("progress should exist");
        };
        assert_eq!(updated.done, Some(2));
        assert_eq!(updated.total, Some(5));
        assert_eq!(updated.message, "half");
        let Some(finished) =
            state.finish_workshop_job("j1", WorkshopJobPhase::Finished, "done".to_string())
        else {
            panic!("finish should exist");
        };
        assert_eq!(finished.phase, WorkshopJobPhase::Finished);
        assert!(state
            .update_workshop_progress("nope", None, None, "x".to_string())
            .is_none());
        assert!(state
            .finish_workshop_job("nope", WorkshopJobPhase::Failed, "x".to_string())
            .is_none());
    }

    #[test]
    fn ledger_evicts_oldest_terminal_at_cap() {
        let mut map: HashMap<String, WorkshopJobSnapshot> = HashMap::new();
        for index in 0..WORKSHOP_JOB_CAP {
            let phase = if index == 0 {
                WorkshopJobPhase::Running
            } else {
                WorkshopJobPhase::Finished
            };
            map.insert(
                format!("j{index}"),
                snapshot(&format!("j{index}"), "/a.mp4", phase, index as u64),
            );
        }
        // Oldest terminal is j1 (updated_at 1); j0 is Running and must survive.
        evict_workshop_if_needed(&mut map, "j-new");
        map.insert(
            "j-new".to_string(),
            snapshot("j-new", "/a.mp4", WorkshopJobPhase::Running, 999),
        );
        assert_eq!(map.len(), WORKSHOP_JOB_CAP);
        assert!(map.contains_key("j0"));
        assert!(!map.contains_key("j1"));
        assert!(map.contains_key("j-new"));
    }

    #[test]
    fn ledger_evicts_oldest_when_all_terminal() {
        let mut map: HashMap<String, WorkshopJobSnapshot> = HashMap::new();
        for index in 0..WORKSHOP_JOB_CAP {
            map.insert(
                format!("f{index}"),
                snapshot(
                    &format!("f{index}"),
                    "/a.mp4",
                    WorkshopJobPhase::Finished,
                    index as u64,
                ),
            );
        }
        evict_workshop_if_needed(&mut map, "f-new");
        map.insert(
            "f-new".to_string(),
            snapshot("f-new", "/a.mp4", WorkshopJobPhase::Finished, 999),
        );
        assert_eq!(map.len(), WORKSHOP_JOB_CAP);
        assert!(!map.contains_key("f0"));
        assert!(map.contains_key("f-new"));
    }

    #[test]
    fn ledger_reinsert_same_job_does_not_evict() {
        let mut map: HashMap<String, WorkshopJobSnapshot> = HashMap::new();
        for index in 0..WORKSHOP_JOB_CAP {
            map.insert(
                format!("j{index}"),
                snapshot(
                    &format!("j{index}"),
                    "/a.mp4",
                    WorkshopJobPhase::Finished,
                    index as u64,
                ),
            );
        }
        evict_workshop_if_needed(&mut map, "j0");
        assert_eq!(map.len(), WORKSHOP_JOB_CAP);
        assert!(map.contains_key("j0"));
    }
}
