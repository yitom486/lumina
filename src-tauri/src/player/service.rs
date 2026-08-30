//! PlayerService — domain API. Does not depend on libmpv FFI types.

use crate::player::error::PlayerError;
use crate::player::model::{PlayerSnapshot, PlayerState};
use crate::player::mpv::LibMpvPlayer;

const VOLUME_MIN: f64 = 0.0;
const VOLUME_MAX: f64 = 100.0;
const RATE_MIN: f64 = 0.25;
const RATE_MAX: f64 = 4.0;

pub struct PlayerService {
    snapshot: PlayerSnapshot,
    backend: Option<LibMpvPlayer>,
    shutdown: bool,
}

impl PlayerService {
    pub fn new() -> Self {
        Self {
            snapshot: PlayerSnapshot::idle(),
            backend: None,
            shutdown: false,
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
                if matches!(
                    self.snapshot.status,
                    PlayerState::Error | PlayerState::Idle
                ) {
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
        if self.backend.take().is_some() {
            tracing::info!("player backend dropped");
        }
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

    pub fn open(&mut self, path: String) -> Result<PlayerSnapshot, PlayerError> {
        if self.snapshot.status == PlayerState::Loading {
            return Err(PlayerError::invalid_state("open", self.snapshot.status));
        }

        if self.backend.is_none() {
            self.snapshot.current_file = Some(path);
            let err = PlayerError::backend_missing();
            self.fail(err.clone());
            return Err(err);
        }

        self.snapshot.status = PlayerState::Loading;
        self.snapshot.current_file = Some(path.clone());
        self.snapshot.current_time_ms = 0;
        self.snapshot.duration_ms = 0;
        self.snapshot.error = None;

        let open_result = self
            .backend
            .as_ref()
            .map(|backend| backend.open(&path))
            .unwrap_or_else(|| Err(PlayerError::backend_missing()));

        if let Err(error) = open_result {
            self.fail(error.clone());
            return Err(error);
        }

        if let Some(backend) = self.backend.as_ref() {
            if let Ok(duration) = backend.duration_ms() {
                self.snapshot.duration_ms = duration;
            }
            let _ = backend.set_volume(self.snapshot.volume);
            let _ = backend.set_rate(self.snapshot.rate);
        }

        self.snapshot.status = PlayerState::Playing;
        tracing::info!(path = %path, "file opened");
        Ok(self.snapshot())
    }

    pub fn play(&mut self) -> Result<PlayerSnapshot, PlayerError> {
        self.require(
            &[PlayerState::Ready, PlayerState::Paused, PlayerState::Ended],
            "play",
        )?;
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
        Ok(self.snapshot())
    }

    pub fn pause(&mut self) -> Result<PlayerSnapshot, PlayerError> {
        self.require(&[PlayerState::Playing], "pause")?;
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
        Ok(self.snapshot())
    }

    pub fn stop(&mut self) -> Result<PlayerSnapshot, PlayerError> {
        self.require(
            &[
                PlayerState::Ready,
                PlayerState::Playing,
                PlayerState::Paused,
                PlayerState::Ended,
            ],
            "stop",
        )?;
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
        Ok(self.snapshot())
    }

    pub fn seek(&mut self, position_ms: u64) -> Result<PlayerSnapshot, PlayerError> {
        self.require(
            &[PlayerState::Ready, PlayerState::Playing, PlayerState::Paused],
            "seek",
        )?;
        let result = self
            .backend
            .as_ref()
            .ok_or_else(PlayerError::backend_missing)
            .and_then(|backend| backend.seek_ms(position_ms));
        if let Err(error) = result {
            self.fail(error.clone());
            return Err(error);
        }
        self.snapshot.current_time_ms = position_ms;
        Ok(self.snapshot())
    }

    pub fn set_volume(&mut self, volume: f64) -> Result<PlayerSnapshot, PlayerError> {
        if !(VOLUME_MIN..=VOLUME_MAX).contains(&volume) {
            return Err(PlayerError::playback(format!(
                "volume must be {VOLUME_MIN}..={VOLUME_MAX}"
            )));
        }
        self.snapshot.volume = volume;
        if let Some(backend) = self.backend.as_ref() {
            backend.set_volume(volume)?;
        }
        Ok(self.snapshot())
    }

    pub fn set_rate(&mut self, rate: f64) -> Result<PlayerSnapshot, PlayerError> {
        if !(RATE_MIN..=RATE_MAX).contains(&rate) {
            return Err(PlayerError::playback(format!(
                "rate must be {RATE_MIN}..={RATE_MAX}"
            )));
        }
        self.snapshot.rate = rate;
        if let Some(backend) = self.backend.as_ref() {
            backend.set_rate(rate)?;
        }
        Ok(self.snapshot())
    }

    /// Reserved for later phases. Not implemented in Phase 1.
    pub fn set_audio_track(&mut self, _id: i64) -> Result<PlayerSnapshot, PlayerError> {
        Err(PlayerError::internal(
            "set_audio_track is not implemented in Phase 1",
            None,
        ))
    }

    /// Reserved for later phases. Not implemented in Phase 1.
    pub fn set_subtitle_track(&mut self, _id: i64) -> Result<PlayerSnapshot, PlayerError> {
        Err(PlayerError::internal(
            "set_subtitle_track is not implemented in Phase 1",
            None,
        ))
    }

    fn require(&self, allowed: &[PlayerState], operation: &str) -> Result<(), PlayerError> {
        if allowed.contains(&self.snapshot.status) {
            Ok(())
        } else {
            Err(PlayerError::invalid_state(operation, self.snapshot.status))
        }
    }

    fn fail(&mut self, error: PlayerError) {
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
    }

    #[test]
    fn open_without_backend_goes_error() {
        let mut player = PlayerService::new();
        let err = player
            .open(r"C:\video.mp4".into())
            .expect_err("backend missing");
        assert_eq!(err.code, PlayerErrorCode::InternalError);
        assert_eq!(player.get_state(), PlayerState::Error);
        assert_eq!(
            player.snapshot().current_file.as_deref(),
            Some(r"C:\video.mp4")
        );
    }

    #[test]
    fn volume_and_rate_range() {
        let mut player = PlayerService::new();
        assert!(player.set_volume(101.0).is_err());
        assert!(player.set_volume(40.0).is_ok());
        assert!(player.set_rate(0.1).is_err());
        assert!(player.set_rate(1.25).is_ok());
        assert_eq!(player.snapshot().volume, 40.0);
        assert_eq!(player.snapshot().rate, 1.25);
    }

    #[test]
    fn reserved_api_and_getters() {
        let mut player = PlayerService::new();
        assert_eq!(player.get_position(), 0);
        assert_eq!(player.get_duration(), 0);
        assert_eq!(
            player.set_audio_track(0).unwrap_err().code,
            PlayerErrorCode::InternalError
        );
        assert_eq!(
            player.set_subtitle_track(0).unwrap_err().code,
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
