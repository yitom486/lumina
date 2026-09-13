//! PlayerService — domain API. Does not depend on libmpv FFI types.

use std::time::{Duration, Instant};

use crate::player::error::PlayerError;
use crate::player::model::{PlayerEvent, PlayerSnapshot, PlayerState};
use crate::player::mpv::LibMpvPlayer;
use crate::player::source::MediaSourceKind;

const VOLUME_MIN: f64 = 0.0;
const VOLUME_MAX: f64 = 100.0;
const RATE_MIN: f64 = 0.25;
const RATE_MAX: f64 = 4.0;
/// Remote yt-dlp hook can take several seconds before demux; after this, surface a soft error.
const REMOTE_DEMUX_TIMEOUT: Duration = Duration::from_secs(12);

pub struct PlayerService {
    snapshot: PlayerSnapshot,
    backend: Option<LibMpvPlayer>,
    shutdown: bool,
    /// When set, remote open is waiting for mpv demux (duration/position).
    remote_demux_deadline: Option<Instant>,
}

impl PlayerService {
    pub fn new() -> Self {
        Self {
            snapshot: PlayerSnapshot::idle(),
            backend: None,
            shutdown: false,
            remote_demux_deadline: None,
        }
    }

    pub fn attach_backend_with_wid(&mut self, wid: i64) -> Result<(), PlayerError> {
        if self.shutdown {
            tracing::info!("skip libmpv init; app is shutting down");
            return Ok(());
        }
        if self.backend.is_some() {
            return Ok(());
        }

        match LibMpvPlayer::initialize_with_wid(wid) {
            Ok(backend) => {
                self.backend = Some(backend);
                if matches!(self.snapshot.status, PlayerState::Error | PlayerState::Idle) {
                    self.snapshot.status = PlayerState::Idle;
                    self.snapshot.error = None;
                }
                Ok(())
            }
            Err(error) => {
                self.fail(error.clone());
                Err(error)
            }
        }
    }

    pub fn shutdown(&mut self) {
        self.shutdown = true;
        if let Some(backend) = self.backend.as_ref() {
            let _ = backend.stop();
            backend.quit();
        }
        if self.backend.take().is_some() {
            tracing::info!("player backend dropped");
        }
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        self.snapshot.clone()
    }

    pub fn get_state(&self) -> PlayerState {
        self.snapshot.status
    }

    pub fn get_position(&self) -> u64 {
        self.snapshot.current_time_ms
    }

    pub fn get_duration(&self) -> u64 {
        self.snapshot.duration_ms
    }

    pub fn open(
        &mut self,
        path: String,
    ) -> Result<(PlayerSnapshot, Vec<PlayerEvent>), PlayerError> {
        let source = match crate::player::source::MediaSource::parse(&path) {
            Ok(source) => source,
            Err(error) => {
                self.snapshot.current_file = Some(path);
                self.snapshot.media_id = None;
                self.snapshot.source_kind = None;
                self.fail(error.clone());
                return Err(error);
            }
        };
        self.open_source(source, None, None, None, None, false)
    }

    /// Open a validated source. `playback_override` is a resolved stream URL for
    /// remote pages; `current_file` / `media_id` stay tied to `source`.
    /// `audio_url` pairs a separate audio stream (DASH). Rate is always restored.
    pub fn open_source(
        &mut self,
        source: crate::player::source::MediaSource,
        playback_override: Option<String>,
        playback_format_id: Option<String>,
        audio_url: Option<String>,
        resume_ms: Option<u64>,
        resume_paused: bool,
    ) -> Result<(PlayerSnapshot, Vec<PlayerEvent>), PlayerError> {
        self.open_source_with_network(
            source,
            playback_override,
            playback_format_id,
            audio_url,
            resume_ms,
            resume_paused,
            None,
            crate::player::mpv::NetworkPlaybackOpts::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open_source_with_network(
        &mut self,
        source: crate::player::source::MediaSource,
        playback_override: Option<String>,
        playback_format_id: Option<String>,
        audio_url: Option<String>,
        resume_ms: Option<u64>,
        resume_paused: bool,
        duration_hint_ms: Option<u64>,
        network: crate::player::mpv::NetworkPlaybackOpts,
    ) -> Result<(PlayerSnapshot, Vec<PlayerEvent>), PlayerError> {
        if self.snapshot.status == PlayerState::Loading {
            return Err(PlayerError::invalid_state("open", self.snapshot.status));
        }

        let display_path = source.playback_target().to_string();
        let media_id = source.media_id();
        let source_kind = source.kind();
        let playback_target = playback_override
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| display_path.clone());

        if let Err(error) = source.validate() {
            self.snapshot.current_file = Some(display_path);
            self.snapshot.media_id = Some(media_id);
            self.snapshot.source_kind = Some(source_kind);
            self.snapshot.playback_format_id = None;
            self.fail(error.clone());
            return Err(error);
        }

        if self.backend.is_none() {
            self.snapshot.current_file = Some(display_path);
            self.snapshot.media_id = Some(media_id);
            self.snapshot.source_kind = Some(source_kind);
            self.snapshot.playback_format_id = None;
            let err = PlayerError::backend_missing();
            self.fail(err.clone());
            return Err(err);
        }

        // Replace previous file on the same mpv instance — no second backend.
        if matches!(
            self.snapshot.status,
            PlayerState::Ready | PlayerState::Playing | PlayerState::Paused | PlayerState::Ended
        ) {
            if let Some(backend) = self.backend.as_ref() {
                if let Err(error) = backend.stop() {
                    tracing::warn!(%error, "stop before open failed; continuing");
                }
            }
        }

        self.snapshot.status = PlayerState::Loading;
        self.snapshot.current_file = Some(display_path.clone());
        self.snapshot.media_id = Some(media_id.clone());
        self.snapshot.source_kind = Some(source_kind);
        self.snapshot.playback_format_id = playback_format_id.clone();
        self.snapshot.current_time_ms = 0;
        // Never treat yt-dlp hint as demux-ready duration (that triggers premature seek).
        self.snapshot.duration_ms = 0;
        self.snapshot.duration_hint_ms = duration_hint_ms.filter(|ms| *ms > 0);
        self.snapshot.error = None;
        self.remote_demux_deadline = if source_kind == MediaSourceKind::Remote {
            Some(Instant::now() + REMOTE_DEMUX_TIMEOUT)
        } else {
            None
        };
        tracing::info!(
            %playback_target,
            %display_path,
            %media_id,
            ?source_kind,
            ?playback_format_id,
            resume_ms,
            duration_hint_ms,
            "open"
        );

        let open_result = self
            .backend
            .as_ref()
            .map(|backend| backend.open_media(&playback_target, audio_url.as_deref(), network))
            .unwrap_or_else(|| Err(PlayerError::backend_missing()));

        if let Err(error) = open_result {
            self.fail(error.clone());
            return Err(error);
        }

        let mut duration_ms = 0;
        if let Some(backend) = self.backend.as_ref() {
            if let Ok(duration) = backend.duration_ms() {
                if duration > 0 {
                    duration_ms = duration;
                    self.snapshot.duration_ms = duration;
                }
            }
            let _ = backend.set_volume(self.snapshot.volume);
            let _ = backend.set_rate(self.snapshot.rate);
        }

        let mut events = vec![
            PlayerEvent::FileLoaded {
                path: display_path,
                duration_ms,
            },
            PlayerEvent::DurationChanged { duration_ms },
        ];

        // Seek requires a playable state — mark Playing before resume seek.
        self.snapshot.status = PlayerState::Playing;

        if let Some(position_ms) = resume_ms.filter(|ms| *ms > 0) {
            match self.seek(position_ms) {
                Ok((_, seek_events)) => events.extend(seek_events),
                Err(error) => tracing::warn!(%error, position_ms, "resume seek after open failed"),
            }
        }

        if resume_paused {
            match self.pause() {
                Ok((_, pause_events)) => events.extend(pause_events),
                Err(error) => {
                    tracing::warn!(%error, "resume pause after open failed");
                    events.push(PlayerEvent::StateChanged {
                        status: PlayerState::Playing,
                    });
                }
            }
        } else {
            events.push(PlayerEvent::StateChanged {
                status: PlayerState::Playing,
            });
        }

        tracing::info!(%playback_target, %media_id, duration_ms, "file opened");
        Ok((self.snapshot(), events))
    }

    pub fn play(&mut self) -> Result<(PlayerSnapshot, Vec<PlayerEvent>), PlayerError> {
        if self.snapshot.status == PlayerState::Playing {
            return Ok((self.snapshot(), Vec::new()));
        }
        self.require(
            &[PlayerState::Ready, PlayerState::Paused, PlayerState::Ended],
            "play",
        )?;
        tracing::info!("play");
        let result = self
            .backend
            .as_ref()
            .ok_or_else(PlayerError::backend_missing)
            .and_then(LibMpvPlayer::play);
        if let Err(error) = result {
            self.fail(error.clone());
            return Err(error);
        }
        self.snapshot.status = PlayerState::Playing;
        Ok((
            self.snapshot(),
            vec![PlayerEvent::StateChanged {
                status: PlayerState::Playing,
            }],
        ))
    }

    pub fn pause(&mut self) -> Result<(PlayerSnapshot, Vec<PlayerEvent>), PlayerError> {
        self.require(&[PlayerState::Playing], "pause")?;
        tracing::info!("pause");
        let result = self
            .backend
            .as_ref()
            .ok_or_else(PlayerError::backend_missing)
            .and_then(LibMpvPlayer::pause);
        if let Err(error) = result {
            self.fail(error.clone());
            return Err(error);
        }
        self.snapshot.status = PlayerState::Paused;
        Ok((
            self.snapshot(),
            vec![PlayerEvent::StateChanged {
                status: PlayerState::Paused,
            }],
        ))
    }

    pub fn stop(&mut self) -> Result<(PlayerSnapshot, Vec<PlayerEvent>), PlayerError> {
        self.require(
            &[
                PlayerState::Ready,
                PlayerState::Playing,
                PlayerState::Paused,
                PlayerState::Ended,
            ],
            "stop",
        )?;
        tracing::info!("stop");
        let result = self
            .backend
            .as_ref()
            .ok_or_else(PlayerError::backend_missing)
            .and_then(LibMpvPlayer::stop);
        if let Err(error) = result {
            self.fail(error.clone());
            return Err(error);
        }
        self.snapshot.status = PlayerState::Idle;
        self.snapshot.current_time_ms = 0;
        Ok((
            self.snapshot(),
            vec![PlayerEvent::StateChanged {
                status: PlayerState::Idle,
            }],
        ))
    }

    pub fn seek(
        &mut self,
        position_ms: u64,
    ) -> Result<(PlayerSnapshot, Vec<PlayerEvent>), PlayerError> {
        self.require(
            &[
                PlayerState::Ready,
                PlayerState::Playing,
                PlayerState::Paused,
                PlayerState::Ended,
            ],
            "seek",
        )?;
        tracing::info!(position_ms, "seek");
        let result = self
            .backend
            .as_ref()
            .ok_or_else(PlayerError::backend_missing)
            .and_then(|backend| backend.seek_ms(position_ms));
        if let Err(error) = result {
            // Seek can fail transiently (demux not ready after open/resume).
            // Do not hard-fault the whole session into Error.
            tracing::warn!(%error, position_ms, "seek failed");
            return Err(error);
        }
        self.snapshot.current_time_ms = position_ms;
        // Refresh duration — often still 0 right after open until demux catches up.
        if let Some(backend) = self.backend.as_ref() {
            if let Ok(duration_ms) = backend.duration_ms() {
                if duration_ms > 0 {
                    self.snapshot.duration_ms = duration_ms;
                }
            }
        }
        Ok((
            self.snapshot(),
            vec![PlayerEvent::PositionChanged { position_ms }],
        ))
    }

    pub fn set_volume(&mut self, volume: f64) -> Result<PlayerSnapshot, PlayerError> {
        if !(VOLUME_MIN..=VOLUME_MAX).contains(&volume) {
            let msg = format!("volume out of range {VOLUME_MIN}–{VOLUME_MAX}: {volume}");
            return Err(PlayerError::playback(Some(&msg)));
        }
        tracing::info!(volume, "set_volume");
        self.snapshot.volume = volume;
        if let Some(backend) = self.backend.as_ref() {
            backend.set_volume(volume)?;
        }
        Ok(self.snapshot())
    }

    pub fn set_rate(&mut self, rate: f64) -> Result<PlayerSnapshot, PlayerError> {
        if !(RATE_MIN..=RATE_MAX).contains(&rate) {
            let msg = format!("rate out of range {RATE_MIN}–{RATE_MAX}: {rate}");
            return Err(PlayerError::playback(Some(&msg)));
        }
        tracing::info!(rate, "set_rate");
        self.snapshot.rate = rate;
        if let Some(backend) = self.backend.as_ref() {
            backend.set_rate(rate)?;
        }
        Ok(self.snapshot())
    }

    /// Periodic poll from the event ticker (~100–250ms). Does not log position.
    pub fn poll_tick(&mut self) -> Vec<PlayerEvent> {
        let mut events = Vec::new();
        let Some(backend) = self.backend.as_ref() else {
            return events;
        };

        // Remote load/extractor failures are asynchronous. Drain before polling
        // properties so the diagnostic that caused a 0:00 black screen is kept.
        backend.drain_diagnostic_events();

        if matches!(
            self.snapshot.status,
            PlayerState::Loading | PlayerState::Ready | PlayerState::Playing | PlayerState::Paused
        ) {
            if let Ok(duration_ms) = backend.duration_ms() {
                if duration_ms > 0 && duration_ms != self.snapshot.duration_ms {
                    self.snapshot.duration_ms = duration_ms;
                    self.remote_demux_deadline = None;
                    events.push(PlayerEvent::DurationChanged { duration_ms });
                }
            }
        }

        if matches!(
            self.snapshot.status,
            PlayerState::Playing | PlayerState::Paused
        ) {
            if let Ok(position_ms) = backend.position_ms() {
                if position_ms != self.snapshot.current_time_ms {
                    self.snapshot.current_time_ms = position_ms;
                    if position_ms > 0 {
                        self.remote_demux_deadline = None;
                    }
                    events.push(PlayerEvent::PositionChanged { position_ms });
                }
            }
        }

        if self.snapshot.status == PlayerState::Playing {
            match backend.eof_reached() {
                Ok(true) => {
                    self.snapshot.status = PlayerState::Ended;
                    self.remote_demux_deadline = None;
                    tracing::info!("playback ended");
                    events.push(PlayerEvent::StateChanged {
                        status: PlayerState::Ended,
                    });
                    events.push(PlayerEvent::Ended);
                }
                Ok(false) => {}
                Err(_) => {}
            }
        }

        if let Some(deadline) = self.remote_demux_deadline {
            if Instant::now() >= deadline
                && self.snapshot.source_kind == Some(MediaSourceKind::Remote)
                && self.snapshot.duration_ms == 0
                && self.snapshot.current_time_ms == 0
                && matches!(
                    self.snapshot.status,
                    PlayerState::Playing | PlayerState::Paused | PlayerState::Loading
                )
            {
                self.remote_demux_deadline = None;
                let error = PlayerError::new(
                    crate::player::PlayerErrorCode::LoadError,
                    "无法拉取在线视频流，请更新登录态后重试",
                    Some("remote demux timeout (yt-dlp/mpv)".into()),
                );
                tracing::warn!(
                    details = error.details.as_deref().unwrap_or(""),
                    "remote stream stall"
                );
                self.fail(error.clone());
                events.push(PlayerEvent::Error { error });
                events.push(PlayerEvent::StateChanged {
                    status: PlayerState::Error,
                });
            }
        }

        events
    }

    /// Show subtitle on the native video surface (mpv overlay).
    /// `source`: "Embedded" | "Sidecar" | "None"
    pub fn set_subtitle(
        &mut self,
        source: &str,
        stream_index: Option<u32>,
        external_path: Option<&str>,
    ) -> Result<PlayerSnapshot, PlayerError> {
        let backend = self
            .backend
            .as_ref()
            .ok_or_else(PlayerError::backend_missing)?;

        match source {
            "None" => {
                backend.clear_subtitle()?;
            }
            "Embedded" => {
                let index = stream_index.ok_or_else(|| {
                    PlayerError::playback(Some("embedded subtitle missing streamIndex"))
                })?;
                backend.set_embedded_subtitle(i64::from(index))?;
            }
            "Sidecar" => {
                let path = external_path.ok_or_else(|| {
                    PlayerError::playback(Some("sidecar subtitle missing file path"))
                })?;
                backend.set_external_subtitle(path)?;
            }
            other => {
                let msg = format!("unknown subtitle source: {other}");
                return Err(PlayerError::playback(Some(&msg)));
            }
        }

        Ok(self.snapshot())
    }

    /// Select embedded audio by ffprobe stream index.
    pub fn set_audio_track(&mut self, stream_index: u32) -> Result<PlayerSnapshot, PlayerError> {
        let backend = self
            .backend
            .as_ref()
            .ok_or_else(PlayerError::backend_missing)?;
        backend.set_embedded_audio(i64::from(stream_index))?;
        Ok(self.snapshot())
    }

    pub fn set_subtitle_track(&mut self, id: i64) -> Result<PlayerSnapshot, PlayerError> {
        self.set_subtitle("Embedded", Some(id as u32), None)
    }

    fn require(&self, allowed: &[PlayerState], operation: &str) -> Result<(), PlayerError> {
        if allowed.contains(&self.snapshot.status) {
            Ok(())
        } else {
            Err(PlayerError::invalid_state(operation, self.snapshot.status))
        }
    }

    fn fail(&mut self, error: PlayerError) {
        tracing::error!(code = ?error.code, message = %error.message, "player error");
        self.snapshot.status = PlayerState::Error;
        self.snapshot.error = Some(error);
    }
}

impl Default for PlayerService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::error::PlayerErrorCode;
    use crate::player::model::{PlayerEvent, PlayerState};

    #[test]
    fn play_from_idle_is_invalid() {
        let mut player = PlayerService::new();
        let err = player.play().expect_err("idle cannot play");
        assert_eq!(err.code, PlayerErrorCode::InvalidState);
        assert!(err.message.contains("空闲"));
        assert!(err.message.contains("播放"));
    }

    #[test]
    fn open_without_backend_goes_error() {
        let mut player = PlayerService::new();
        let err = player
            .open(r"C:\video.mp4".into())
            .expect_err("missing path");
        assert_eq!(err.code, PlayerErrorCode::LoadError);
        assert!(err
            .message
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
        assert_eq!(player.get_state(), PlayerState::Error);
        assert_eq!(
            player.snapshot().current_file.as_deref(),
            Some(r"C:\video.mp4")
        );
    }

    #[test]
    fn open_existing_file_without_backend_is_internal() {
        let dir = std::env::temp_dir();
        let path = dir.join("lumina-m8-probe.bin");
        if std::fs::write(&path, b"not-a-video").is_err() {
            return;
        }
        let path_str = path.to_string_lossy().to_string();
        let mut player = PlayerService::new();
        let err = player.open(path_str.clone()).expect_err("backend missing");
        let _ = std::fs::remove_file(&path);
        assert_eq!(err.code, PlayerErrorCode::InternalError);
        assert_eq!(err.message, "内部错误，请重试");
        assert_eq!(player.get_state(), PlayerState::Error);
        assert_eq!(
            player.snapshot().current_file.as_deref(),
            Some(path_str.as_str())
        );
    }

    #[test]
    fn open_remote_url_without_backend_is_internal() {
        let mut player = PlayerService::new();
        let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string();
        let err = player.open(url.clone()).expect_err("backend missing");
        assert_eq!(err.code, PlayerErrorCode::InternalError);
        assert_eq!(player.get_state(), PlayerState::Error);
        assert_eq!(
            player.snapshot().current_file.as_deref(),
            Some(url.as_str())
        );
        assert_eq!(
            player.snapshot().media_id.as_deref(),
            Some("youtube:dQw4w9WgXcQ")
        );
        assert_eq!(
            player.snapshot().source_kind,
            Some(crate::player::source::MediaSourceKind::Remote)
        );
    }

    #[test]
    fn open_source_override_keeps_page_identity() {
        let mut player = PlayerService::new();
        let source = crate::player::source::MediaSource::parse(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        )
        .unwrap();
        let err = player
            .open_source(
                source,
                Some("https://cdn.example/stream.mp4".into()),
                Some("22".into()),
                None,
                None,
                false,
            )
            .expect_err("backend missing");
        assert_eq!(err.code, PlayerErrorCode::InternalError);
        let snap = player.snapshot();
        assert_eq!(
            snap.current_file.as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
        assert_eq!(snap.media_id.as_deref(), Some("youtube:dQw4w9WgXcQ"));
        assert_eq!(
            snap.source_kind,
            Some(crate::player::source::MediaSourceKind::Remote)
        );
        // Format id is cleared when open fails before backend.
        assert!(snap.playback_format_id.is_none());
    }

    #[test]
    fn open_empty_path_is_load_error() {
        let mut player = PlayerService::new();
        let err = player.open("   ".into()).expect_err("empty path");
        assert_eq!(err.code, PlayerErrorCode::LoadError);
        assert_eq!(err.message, "无法打开该媒体文件");
        assert_eq!(err.details.as_deref(), Some("empty media path"));
        assert_eq!(player.get_state(), PlayerState::Error);
    }

    #[test]
    fn volume_and_rate_range() {
        let mut player = PlayerService::new();
        let vol_err = player.set_volume(101.0).expect_err("over max");
        assert_eq!(vol_err.code, PlayerErrorCode::PlaybackError);
        assert!(vol_err
            .details
            .as_deref()
            .is_some_and(|d| d.contains("volume")));
        assert!(player.set_volume(40.0).is_ok());
        let rate_err = player.set_rate(0.1).expect_err("under min");
        assert_eq!(rate_err.code, PlayerErrorCode::PlaybackError);
        assert!(rate_err
            .details
            .as_deref()
            .is_some_and(|d| d.contains("rate")));
        assert!(player.set_rate(1.25).is_ok());
        assert_eq!(player.snapshot().volume, 40.0);
        assert_eq!(player.snapshot().rate, 1.25);
    }

    #[test]
    fn pause_stop_seek_from_idle_are_invalid() {
        let mut player = PlayerService::new();
        assert_eq!(
            player.pause().unwrap_err().code,
            PlayerErrorCode::InvalidState
        );
        assert_eq!(
            player.stop().unwrap_err().code,
            PlayerErrorCode::InvalidState
        );
        assert_eq!(
            player.seek(1000).unwrap_err().code,
            PlayerErrorCode::InvalidState
        );
    }

    #[test]
    fn reserved_api_and_getters() {
        let mut player = PlayerService::new();
        assert_eq!(player.get_position(), 0);
        assert_eq!(player.get_duration(), 0);
        // Without backend, audio/subtitle apply fail as InternalError (backend missing).
        assert_eq!(
            player.set_audio_track(0).unwrap_err().code,
            PlayerErrorCode::InternalError
        );
        assert_eq!(
            player
                .set_subtitle("Embedded", Some(0), None)
                .unwrap_err()
                .code,
            PlayerErrorCode::InternalError
        );
    }

    #[test]
    fn player_event_shapes_exist() {
        let _ = PlayerEvent::Ended;
        let _ = PlayerEvent::StateChanged {
            status: PlayerState::Idle,
        };
    }
}
