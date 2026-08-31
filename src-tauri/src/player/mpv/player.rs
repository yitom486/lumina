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
            init.set_option("sub-visibility", "yes")?;
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
        // mpv accepts 0–100+ soft volume; prefer f64, fall back to int property.
        match self.mpv.set_property("volume", volume) {
            Ok(()) => Ok(()),
            Err(first) => {
                tracing::warn!(%first, volume, "set volume f64 failed; retry as i64");
                self.mpv
                    .set_property("volume", volume.round() as i64)
                    .map_err(map_playback_error)
            }
        }
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

    /// Select embedded subtitle by FFmpeg/ffprobe stream index.
    pub fn set_embedded_subtitle(&self, ff_stream_index: i64) -> Result<(), PlayerError> {
        tracing::info!(ff_stream_index, "set embedded subtitle");
        set_sub_visibility(&self.mpv, true)?;

        if let Some(sid) = find_sid_by_ff_index(&self.mpv, ff_stream_index)? {
            tracing::info!(sid, ff_stream_index, "matched mpv sid via track-list");
            return self
                .mpv
                .set_property("sid", sid)
                .map_err(map_playback_error);
        }

        // Fallback: Nth subtitle track (0-based among sub tracks → mpv sid).
        if let Some(sid) = find_sid_by_subtitle_ordinal(&self.mpv, ff_stream_index)? {
            tracing::info!(
                sid,
                ff_stream_index,
                "matched mpv sid via subtitle ordinal fallback"
            );
            return self
                .mpv
                .set_property("sid", sid)
                .map_err(map_playback_error);
        }

        Err(PlayerError::playback(Some(&format!(
            "no matching subtitle track (ff-index {ff_stream_index})"
        ))))
    }

    /// Load and select an external subtitle file.
    pub fn set_external_subtitle(&self, path: &str) -> Result<(), PlayerError> {
        tracing::info!(path, "set external subtitle (sub-add)");
        set_sub_visibility(&self.mpv, true)?;
        self.mpv
            .command("sub-add", &[path, "select"])
            .map_err(map_playback_error)?;
        Ok(())
    }

    pub fn clear_subtitle(&self) -> Result<(), PlayerError> {
        tracing::info!("clear subtitle");
        set_sub_visibility(&self.mpv, false)?;
        self.mpv
            .set_property("sid", "no")
            .map_err(map_playback_error)?;
        Ok(())
    }

    /// Select embedded audio by FFmpeg/ffprobe stream index.
    pub fn set_embedded_audio(&self, ff_stream_index: i64) -> Result<(), PlayerError> {
        tracing::info!(ff_stream_index, "set embedded audio");
        if let Some(aid) = find_aid_by_ff_index(&self.mpv, ff_stream_index)? {
            tracing::info!(aid, ff_stream_index, "matched mpv aid via track-list");
            return self
                .mpv
                .set_property("aid", aid)
                .map_err(map_playback_error);
        }
        if let Some(aid) = find_aid_by_audio_ordinal(&self.mpv, ff_stream_index)? {
            tracing::info!(
                aid,
                ff_stream_index,
                "matched mpv aid via audio ordinal fallback"
            );
            return self
                .mpv
                .set_property("aid", aid)
                .map_err(map_playback_error);
        }
        Err(PlayerError::playback(Some(&format!(
            "no matching audio track (ff-index {ff_stream_index})"
        ))))
    }
}

impl Drop for LibMpvPlayer {
    fn drop(&mut self) {
        tracing::info!("libmpv shutting down");
    }
}

fn set_sub_visibility(mpv: &Mpv, visible: bool) -> Result<(), PlayerError> {
    let value = if visible { "yes" } else { "no" };
    mpv.set_property("sub-visibility", value)
        .map_err(map_playback_error)
}

fn track_list_count(mpv: &Mpv) -> Result<i64, PlayerError> {
    mpv.get_property("track-list/count")
        .map_err(map_playback_error)
}

fn find_sid_by_ff_index(mpv: &Mpv, ff_stream_index: i64) -> Result<Option<i64>, PlayerError> {
    find_track_id_by_ff_index(mpv, "sub", ff_stream_index)
}

fn find_aid_by_ff_index(mpv: &Mpv, ff_stream_index: i64) -> Result<Option<i64>, PlayerError> {
    find_track_id_by_ff_index(mpv, "audio", ff_stream_index)
}

fn find_track_id_by_ff_index(
    mpv: &Mpv,
    track_type: &str,
    ff_stream_index: i64,
) -> Result<Option<i64>, PlayerError> {
    let count = track_list_count(mpv)?;
    for i in 0..count {
        let typ: String = mpv
            .get_property(&format!("track-list/{i}/type"))
            .map_err(map_playback_error)?;
        if typ != track_type {
            continue;
        }
        let ff_index: i64 = mpv
            .get_property(&format!("track-list/{i}/ff-index"))
            .map_err(map_playback_error)?;
        if ff_index == ff_stream_index {
            let id: i64 = mpv
                .get_property(&format!("track-list/{i}/id"))
                .map_err(map_playback_error)?;
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn find_aid_by_audio_ordinal(mpv: &Mpv, ff_stream_index: i64) -> Result<Option<i64>, PlayerError> {
    let count = track_list_count(mpv)?;
    let mut aids: Vec<i64> = Vec::new();
    for i in 0..count {
        let typ: String = match mpv.get_property(&format!("track-list/{i}/type")) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if typ != "audio" {
            continue;
        }
        let id: i64 = match mpv.get_property(&format!("track-list/{i}/id")) {
            Ok(value) => value,
            Err(_) => continue,
        };
        aids.push(id);
    }
    if aids.is_empty() {
        return Ok(None);
    }
    if ff_stream_index >= 1 {
        let idx = (ff_stream_index as usize).saturating_sub(1);
        if let Some(aid) = aids.get(idx) {
            return Ok(Some(*aid));
        }
        if aids.contains(&ff_stream_index) {
            return Ok(Some(ff_stream_index));
        }
    }
    Ok(aids.first().copied())
}

fn find_sid_by_subtitle_ordinal(
    mpv: &Mpv,
    ff_stream_index: i64,
) -> Result<Option<i64>, PlayerError> {
    let count = track_list_count(mpv)?;
    let mut sub_sids: Vec<i64> = Vec::new();
    for i in 0..count {
        let typ: String = match mpv.get_property(&format!("track-list/{i}/type")) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if typ != "sub" {
            continue;
        }
        let sid: i64 = match mpv.get_property(&format!("track-list/{i}/id")) {
            Ok(value) => value,
            Err(_) => continue,
        };
        sub_sids.push(sid);
    }
    if sub_sids.is_empty() {
        return Ok(None);
    }
    // If caller passed a 1-based subtitle ordinal (or mpv sid), try it.
    if ff_stream_index >= 1 {
        let idx = (ff_stream_index as usize).saturating_sub(1);
        if let Some(sid) = sub_sids.get(idx) {
            return Ok(Some(*sid));
        }
        if sub_sids.contains(&ff_stream_index) {
            return Ok(Some(ff_stream_index));
        }
    }
    // Last resort: first subtitle track.
    Ok(sub_sids.first().copied())
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
        "播放引擎初始化失败",
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
        return PlayerError::unsupported(Some(&details));
    }
    PlayerError::load(Some(&details))
}

fn map_playback_error(error: libmpv2::Error) -> PlayerError {
    let details = error.to_string();
    let lower = details.to_ascii_lowercase();
    let message = if lower.contains("seek") {
        "跳转失败，请稍后再试"
    } else if lower.contains("property") {
        "播放属性设置失败"
    } else {
        "播放操作失败，请重试"
    };
    PlayerError::new(PlayerErrorCode::PlaybackError, message, Some(details))
}

#[cfg(test)]
mod tests {
    #[test]
    fn initialize_and_shutdown() {
        let player = super::LibMpvPlayer::initialize().expect("libmpv should initialize");
        drop(player);
    }

    #[test]
    fn map_playback_error_is_chinese() {
        // Construct via Display-like string path used in production mapping.
        let err = super::map_playback_error(libmpv2::Error::Raw(1));
        assert!(
            err.message
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "message should be Chinese: {}",
            err.message
        );
        assert!(err.details.is_some());
    }
}
