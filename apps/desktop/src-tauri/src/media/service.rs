//! MediaInspector — domain API over ffprobe. No libmpv dependency.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use crate::media::error::MediaError;
use crate::media::ffprobe;
use crate::media::model::MediaInfo;

/// Process-wide probe cache: one ffprobe run per unchanged file.
/// Open flows hit the same file repeatedly (media panel, subtitle listing,
/// transcript load); without this a 37-stream file pays ~0.5 s three times.
static PROBE_CACHE: OnceLock<Mutex<ProbeCache>> = OnceLock::new();
const PROBE_CACHE_MAX_ENTRIES: usize = 64;

struct CachedProbe {
    modified: Option<SystemTime>,
    size: u64,
    info: MediaInfo,
}

#[derive(Default)]
struct ProbeCache {
    entries: HashMap<PathBuf, CachedProbe>,
}

impl ProbeCache {
    fn lookup(&self, key: &Path, modified: Option<SystemTime>, size: u64) -> Option<MediaInfo> {
        let entry = self.entries.get(key)?;
        if entry.modified == modified && entry.size == size {
            Some(entry.info.clone())
        } else {
            None
        }
    }

    fn store(&mut self, key: PathBuf, modified: Option<SystemTime>, size: u64, info: &MediaInfo) {
        if self.entries.len() >= PROBE_CACHE_MAX_ENTRIES {
            self.entries.clear();
        }
        self.entries.insert(
            key,
            CachedProbe {
                modified,
                size,
                info: info.clone(),
            },
        );
    }
}

fn cache_key(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

fn cache_get(path: &Path) -> Option<MediaInfo> {
    let key = cache_key(path)?;
    let meta = std::fs::metadata(&key).ok()?;
    let cache = PROBE_CACHE.get_or_init(|| Mutex::new(ProbeCache::default()));
    let guard = cache.lock().ok()?;
    guard.lookup(&key, meta.modified().ok(), meta.len())
}

fn cache_put(path: &Path, info: &MediaInfo) {
    let Some(key) = cache_key(path) else {
        return;
    };
    let Ok(meta) = std::fs::metadata(&key) else {
        return;
    };
    let cache = PROBE_CACHE.get_or_init(|| Mutex::new(ProbeCache::default()));
    let Ok(mut guard) = cache.lock() else {
        return;
    };
    guard.store(key, meta.modified().ok(), meta.len(), info);
}

pub struct MediaInspector;

impl MediaInspector {
    /// 打包后版本：传入 Tauri `resource_dir` 以正确定位 ffprobe/ffmpeg。
    pub fn inspect_with(
        path: impl AsRef<Path>,
        resource_dir: Option<&PathBuf>,
    ) -> Result<MediaInfo, MediaError> {
        let path = path.as_ref();
        if let Some(cached) = cache_get(path) {
            tracing::debug!(path = %path.display(), "media inspect cache hit");
            return Ok(cached);
        }
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
        cache_put(path, &info);
        Ok(info)
    }

    /// 开发期便利方法（无 resource_dir）。
    pub fn inspect(path: impl AsRef<Path>) -> Result<MediaInfo, MediaError> {
        Self::inspect_with(path, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fixture(dir: &std::path::Path) -> std::path::PathBuf {
        let ffmpeg = crate::media::tools::resolve_ffmpeg().expect("spike needs ffmpeg");
        let out = dir.join("cache.mp4");
        let status = crate::process_util::command(&ffmpeg)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=320x240:rate=10",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-pix_fmt",
                "yuv420p",
                "-an",
            ])
            .arg(&out)
            .output()
            .expect("spawn ffmpeg");
        assert!(status.status.success(), "cache fixture failed to encode");
        out
    }

    #[test]
    fn probe_cache_hits_misses_and_evicts() {
        use std::time::Duration;

        let key = PathBuf::from("/tmp/video.mp4");
        let info = MediaInfo {
            path: "/tmp/video.mp4".into(),
            format_name: Some("mp4".into()),
            format_long_name: None,
            duration_ms: Some(1000),
            size_bytes: Some(10),
            bit_rate: None,
            streams: Vec::new(),
            chapters: Vec::new(),
        };
        let stamp = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(7));
        let mut cache = ProbeCache::default();
        assert!(cache.lookup(&key, stamp, 10).is_none());
        cache.store(key.clone(), stamp, 10, &info);
        assert_eq!(
            cache.lookup(&key, stamp, 10).map(|info| info.duration_ms),
            Some(Some(1000))
        );
        // Size change invalidates.
        assert!(cache.lookup(&key, stamp, 11).is_none());
        // mtime change invalidates.
        assert!(cache
            .lookup(&key, Some(SystemTime::UNIX_EPOCH), 10)
            .is_none());
        // Cap evicts instead of growing unboundedly.
        for index in 0..PROBE_CACHE_MAX_ENTRIES + 5 {
            cache.store(PathBuf::from(format!("/tmp/{index}.mp4")), stamp, 1, &info);
        }
        assert!(cache.entries.len() <= PROBE_CACHE_MAX_ENTRIES);
    }

    #[test]
    fn repeated_inspects_agree() {
        if crate::media::tools::resolve_ffmpeg().is_err() {
            eprintln!("SKIP probe cache: ffmpeg not vendored on this machine");
            return;
        }
        let dir = std::env::temp_dir().join(format!("lumina-probe-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("cache temp dir");
        let media = make_fixture(&dir);

        let first = MediaInspector::inspect(&media).expect("first probe");
        let second = MediaInspector::inspect(&media).expect("second probe");
        assert_eq!(first.streams.len(), second.streams.len());
        assert_eq!(first.duration_ms, second.duration_ms);

        // Size change invalidates but stays a clean error, not a stale hit.
        std::fs::write(&media, b"x").ok();
        assert!(MediaInspector::inspect(&media).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
