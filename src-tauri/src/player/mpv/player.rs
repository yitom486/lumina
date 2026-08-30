//! LibMpvPlayer — the only module that talks to libmpv.

use libmpv2::Mpv;

use crate::player::error::{PlayerError, PlayerErrorCode};

pub struct LibMpvPlayer {
    mpv: Mpv,
}

impl LibMpvPlayer {
    /// Headless init for unit tests (`vo=null`).
    pub fn initialize() -> Result<Self, PlayerError> {
        tracing::info!("libmpv initializing (null vo)");
        let mpv = Mpv::with_initializer(|init| {
            init.set_option("vo", "null")?;
            init.set_option("idle", "yes")?;
            init.set_option("terminal", "no")?;
            Ok(())
        })
        .map_err(map_init_error)?;
        log_version(&mpv);
        Ok(Self { mpv })
    }

    /// Embed into a native HWND via `wid` (Windows Phase 1 surface).
    pub fn initialize_with_wid(wid: i64) -> Result<Self, PlayerError> {
        tracing::info!(wid, "libmpv initializing with wid");
        let mpv = Mpv::with_initializer(|init| {
            init.set_option("wid", wid)?;
            init.set_option("idle", "yes")?;
            init.set_option("terminal", "no")?;
            init.set_option("keep-open", "yes")?;
            // Prefer hardware decode; mpv falls back to software if needed.
            init.set_option("hwdec", "auto")?;
            Ok(())
        })
        .map_err(map_init_error)?;
        log_version(&mpv);
        Ok(Self { mpv })
    }

    pub fn open(&self, path: &str) -> Result<(), PlayerError> {
        tracing::info!(path, "libmpv loadfile");
        self.mpv
            .command("loadfile", &[path, "replace"])
            .map_err(map_load_error)?;
        self.mpv
            .set_property("pause", false)
            .map_err(map_playback_error)?;
        Ok(())
    }

    pub fn play(&self) -> Result<(), PlayerError> {
        self.mpv
            .set_property("pause", false)
            .map_err(map_playback_error)
    }

    pub fn pause(&self) -> Result<(), PlayerError> {
        self.mpv
            .set_property("pause", true)
            .map_err(map_playback_error)
    }

    pub fn stop(&self) -> Result<(), PlayerError> {
        self.mpv.command("stop", &[]).map_err(map_playback_error)
    }

    pub fn seek_ms(&self, position_ms: u64) -> Result<(), PlayerError> {
        let seconds = position_ms as f64 / 1000.0;
        self.mpv
            .command("seek", &[&seconds.to_string(), "absolute"])
            .map_err(map_playback_error)
    }

    pub fn set_volume(&self, volume: f64) -> Result<(), PlayerError> {
        self.mpv
            .set_property("volume", volume)
            .map_err(map_playback_error)
    }

    pub fn set_rate(&self, rate: f64) -> Result<(), PlayerError> {
        self.mpv
            .set_property("speed", rate)
            .map_err(map_playback_error)
    }

    pub fn duration_ms(&self) -> Result<u64, PlayerError> {
        let duration: f64 = self
            .mpv
            .get_property("duration")
            .map_err(map_playback_error)?;
        Ok((duration.max(0.0) * 1000.0) as u64)
    }

    pub fn position_ms(&self) -> Result<u64, PlayerError> {
        let position: f64 = self
            .mpv
            .get_property("time-pos")
            .map_err(map_playback_error)?;
        Ok((position.max(0.0) * 1000.0) as u64)
    }

    pub fn eof_reached(&self) -> Result<bool, PlayerError> {
        self.mpv
            .get_property("eof-reached")
            .map_err(map_playback_error)
    }
}

impl Drop for LibMpvPlayer {
    fn drop(&mut self) {
        tracing::info!("libmpv shutting down");
    }
}

fn log_version(mpv: &Mpv) {
    match mpv.get_property::<String>("mpv-version") {
        Ok(version) => tracing::info!(%version, "libmpv initialized"),
        Err(error) => tracing::info!(%error, "libmpv initialized"),
    }
}

fn map_init_error(error: libmpv2::Error) -> PlayerError {
    PlayerError::new(
        PlayerErrorCode::InitializationError,
        "failed to initialize libmpv",
        Some(error.to_string()),
    )
}

fn map_load_error(error: libmpv2::Error) -> PlayerError {
    let details = error.to_string();
    let lower = details.to_ascii_lowercase();
    if lower.contains("unsupported")
        || lower.contains("codec")
        || lower.contains("no demuxer")
        || lower.contains("unrecognized")
    {
        return PlayerError::unsupported("unsupported or unreadable media", Some(&details));
    }
    PlayerError::load("failed to load media file", Some(&details))
}

fn map_playback_error(error: libmpv2::Error) -> PlayerError {
    PlayerError::new(
        PlayerErrorCode::PlaybackError,
        "libmpv playback command failed",
        Some(error.to_string()),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn initialize_and_shutdown() {
        let player = super::LibMpvPlayer::initialize().expect("libmpv should initialize");
        drop(player);
    }
}
