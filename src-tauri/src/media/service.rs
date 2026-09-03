//! MediaInspector — domain API over ffprobe. No libmpv dependency.

use std::path::{Path, PathBuf};

use crate::media::error::MediaError;
use crate::media::ffprobe;
use crate::media::model::MediaInfo;

pub struct MediaInspector;

impl MediaInspector {
    /// 打包后版本：传入 Tauri `resource_dir` 以正确定位 ffprobe/ffmpeg。
    pub fn inspect_with(
        path: impl AsRef<Path>,
        resource_dir: Option<&PathBuf>,
    ) -> Result<MediaInfo, MediaError> {
        let path = path.as_ref();
        let info = match ffprobe::probe_file_with(path, resource_dir) {
            Ok(info) => info,
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    code = ?error.code,
                    details = ?error.details,
                    "media inspect failed"
                );
                return Err(error);
            }
        };
        tracing::info!(
            path = %info.path,
            format = ?info.format_name,
            streams = info.streams.len(),
            duration_ms = ?info.duration_ms,
            "media inspected"
        );
        Ok(info)
    }

    /// 开发期便利方法（无 resource_dir）。
    pub fn inspect(path: impl AsRef<Path>) -> Result<MediaInfo, MediaError> {
        Self::inspect_with(path, None)
    }
}
