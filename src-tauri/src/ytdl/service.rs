//! YtdlService — busy lock around install / resolve; caches last resolve for play.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::ytdl::download;
use crate::ytdl::error::YtdlError;
use crate::ytdl::model::{YtdlInstallEvent, YtdlResolveResult, YtdlStatus};
use crate::ytdl::paths;
use crate::ytdl::playback::{self, PlaybackFormatsResponse, YtdlPlayTarget};
use crate::ytdl::resolve;

struct CachedResolve {
    page_url: String,
    resolved: YtdlResolveResult,
    current_format_id: Option<String>,
}

pub struct YtdlService {
    busy: AtomicBool,
    cache: Mutex<Option<CachedResolve>>,
}

impl YtdlService {
    pub fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
            cache: Mutex::new(None),
        }
    }

    pub fn status(&self) -> YtdlStatus {
        paths::status()
    }

    pub fn cookie_status(&self) -> crate::ytdl::YtdlCookieStatus {
        crate::ytdl::cookies::status()
    }

    pub fn set_cookies(
        &self,
        input: crate::ytdl::YtdlCookieConfigInput,
    ) -> Result<crate::ytdl::YtdlCookieStatus, YtdlError> {
        // Changing cookies invalidates prior resolve cache (signed URLs / auth).
        let status = crate::ytdl::cookies::save(input)?;
        self.clear_cache()?;
        Ok(status)
    }

    pub fn install<F>(&self, on_event: F) -> Result<YtdlStatus, YtdlError>
    where
        F: FnMut(YtdlInstallEvent),
    {
        self.with_busy(|| download::install_cli(on_event))
    }

    /// Resolve metadata; caches full result (with stream URLs) in-process.
    pub fn resolve(&self, url: &str) -> Result<YtdlResolveResult, YtdlError> {
        self.with_busy(|| {
            let resolved = resolve::resolve_url(url)?;
            self.store_cache(
                url,
                resolved.clone(),
                resolved.recommended_format_id.clone(),
            )?;
            Ok(resolved.sanitized_for_ipc())
        })
    }

    /// Build a playable stream target from cache or a fresh resolve.
    pub fn play_target(
        &self,
        url: &str,
        format_id: Option<&str>,
    ) -> Result<YtdlPlayTarget, YtdlError> {
        let page = url.trim();
        if page.is_empty() {
            return Err(YtdlError::invalid("请提供在线视频链接"));
        }

        if let Some(target) = self.try_play_from_cache(page, format_id)? {
            self.set_current_format(&target.format_id)?;
            return Ok(target);
        }

        self.with_busy(|| {
            let resolved = resolve::resolve_url(page)?;
            let target = playback::play_target(&resolved, page, format_id)?;
            self.store_cache(page, resolved, Some(target.format_id.clone()))?;
            Ok(target)
        })
    }

    /// Switch quality using the cached resolve for the given page URL.
    pub fn play_target_cached(
        &self,
        page_url: &str,
        format_id: &str,
    ) -> Result<YtdlPlayTarget, YtdlError> {
        let page = page_url.trim();
        if page.is_empty() {
            return Err(YtdlError::invalid("当前没有可切换清晰度的在线视频"));
        }
        match self.try_play_from_cache(page, Some(format_id))? {
            Some(target) => {
                self.set_current_format(&target.format_id)?;
                Ok(target)
            }
            None => Err(YtdlError::invalid("请先解析该在线视频后再切换清晰度")),
        }
    }

    pub fn list_formats(&self) -> Result<PlaybackFormatsResponse, YtdlError> {
        let guard = self
            .cache
            .lock()
            .map_err(|_| YtdlError::internal(Some("ytdl cache mutex poisoned")))?;
        let Some(cached) = guard.as_ref() else {
            return Ok(PlaybackFormatsResponse {
                formats: Vec::new(),
                current_format_id: None,
            });
        };
        Ok(PlaybackFormatsResponse {
            formats: playback::progressive_options(&cached.resolved.formats),
            current_format_id: cached.current_format_id.clone(),
        })
    }

    fn try_play_from_cache(
        &self,
        page_url: &str,
        format_id: Option<&str>,
    ) -> Result<Option<YtdlPlayTarget>, YtdlError> {
        let guard = self
            .cache
            .lock()
            .map_err(|_| YtdlError::internal(Some("ytdl cache mutex poisoned")))?;
        let Some(cached) = guard.as_ref() else {
            return Ok(None);
        };
        if !cache_matches(cached, page_url) {
            return Ok(None);
        }
        Ok(Some(playback::play_target(
            &cached.resolved,
            &cached.page_url,
            format_id,
        )?))
    }

    fn store_cache(
        &self,
        page_url: &str,
        resolved: YtdlResolveResult,
        current_format_id: Option<String>,
    ) -> Result<(), YtdlError> {
        let mut guard = self
            .cache
            .lock()
            .map_err(|_| YtdlError::internal(Some("ytdl cache mutex poisoned")))?;
        *guard = Some(CachedResolve {
            page_url: page_url.to_string(),
            resolved,
            current_format_id,
        });
        Ok(())
    }

    fn set_current_format(&self, format_id: &str) -> Result<(), YtdlError> {
        let mut guard = self
            .cache
            .lock()
            .map_err(|_| YtdlError::internal(Some("ytdl cache mutex poisoned")))?;
        if let Some(cached) = guard.as_mut() {
            cached.current_format_id = Some(format_id.to_string());
        }
        Ok(())
    }

    fn clear_cache(&self) -> Result<(), YtdlError> {
        let mut guard = self
            .cache
            .lock()
            .map_err(|_| YtdlError::internal(Some("ytdl cache mutex poisoned")))?;
        *guard = None;
        Ok(())
    }

    fn with_busy<R, F>(&self, work: F) -> Result<R, YtdlError>
    where
        F: FnOnce() -> Result<R, YtdlError>,
    {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(YtdlError::busy());
        }
        let result = work();
        self.busy.store(false, Ordering::SeqCst);
        result
    }
}

fn cache_matches(cached: &CachedResolve, page_url: &str) -> bool {
    if urls_match(&cached.page_url, page_url) {
        return true;
    }
    if cached
        .resolved
        .webpage_url
        .as_deref()
        .is_some_and(|w| urls_match(w, page_url))
    {
        return true;
    }
    crate::player::source::MediaSource::parse(page_url)
        .map(|s| s.media_id() == cached.resolved.media_id)
        .unwrap_or(false)
}

fn urls_match(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl Default for YtdlService {
    fn default() -> Self {
        Self::new()
    }
}
