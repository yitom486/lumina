//! MediaInspector — domain API over ffprobe. No libmpv dependency.

use std::path::Path;

use crate::media::error::MediaError;
use crate::media::ffprobe;
use crate::media::model::MediaInfo;

pub struct MediaInspector;

impl MediaInspector {
    pub fn inspect(path: impl AsRef<Path>) -> Result<MediaInfo, MediaError> {
        let path = path.as_ref();
        let info = ffprobe::probe_file(path)?;
        tracing::info!(
            path = %info.path,
            format = ?info.format_name,
            streams = info.streams.len(),
            duration_ms = ?info.duration_ms,
            "media inspected"
        );
        Ok(info)
    }
}
