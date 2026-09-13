//! YtdlService — busy lock around install / resolve; caches last resolve for play.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::download;
use crate::error::YtdlError;
use crate::model::{YtdlInstallEvent, YtdlResolveResult, YtdlStatus};
use crate::paths;
use crate::playback::{self, PlaybackFormatsResponse, YtdlPlayTarget};
use crate::resolve;
use lumina_subtitle::{SubtitleChoice, SubtitleError, Transcript};

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

    pub fn cookie_status(&self) -> crate::YtdlCookieStatus {
        crate::cookies::status()
    }

    pub fn set_cookies(
        &self,
        input: crate::YtdlCookieConfigInput,
    ) -> Result<crate::YtdlCookieStatus, YtdlError> {
        // Changing cookies invalidates prior resolve cache (signed URLs / auth).
        let status = crate::cookies::save(input)?;
        self.clear_cache()?;
        Ok(status)
    }

    pub fn list_browser_profiles(
        &self,
        browser: crate::CookieBrowser,
    ) -> Result<Vec<crate::BrowserProfileOption>, YtdlError> {
        crate::cookies::list_browser_profiles(browser)
    }

    pub fn test_cookies(&self) -> Result<crate::YtdlCookieTestResult, YtdlError> {
        let settings = crate::cookies::load();
        self.with_busy(|| crate::cookies::test_cookies(&settings))
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

    /// Return the sanitized in-memory resolve for the currently opened page.
    pub fn cached_resolve(&self, page_url: &str) -> Result<Option<YtdlResolveResult>, YtdlError> {
        Ok(self
            .cached_full_resolve(page_url)?
            .map(|resolved| resolved.sanitized_for_ipc()))
    }

    fn cached_full_resolve(&self, page_url: &str) -> Result<Option<YtdlResolveResult>, YtdlError> {
        let guard = self
            .cache
            .lock()
            .map_err(|_| YtdlError::internal(Some("ytdl cache mutex poisoned")))?;
        Ok(guard
            .as_ref()
            .filter(|cached| cache_matches(cached, page_url))
            .map(|cached| cached.resolved.clone()))
    }

    pub fn list_subtitle_choices(
        &self,
        page_url: &str,
    ) -> Result<Vec<SubtitleChoice>, SubtitleError> {
        let resolved = self
            .cached_full_resolve(page_url)
            .map_err(|error| SubtitleError::extract_failed(error.details.as_deref()))?;
        let resolved = resolved.ok_or_else(|| {
            SubtitleError::extract_failed(Some("online resolve cache unavailable"))
        })?;
        Ok(crate::subtitle::list_choices(&resolved))
    }

    pub fn load_subtitle_choice(
        &self,
        page_url: &str,
        choice_id: &str,
    ) -> Result<Transcript, SubtitleError> {
        // Reuse the full in-process resolve (with signed subtitle URLs);
        // `cached_resolve` is sanitized for IPC and would drop the fast path.
        let resolved = self
            .cached_full_resolve(page_url)
            .map_err(|error| SubtitleError::extract_failed(error.details.as_deref()))?;
        let resolved = resolved.ok_or_else(|| {
            SubtitleError::extract_failed(Some("online resolve cache unavailable"))
        })?;
        crate::subtitle::load_choice(page_url, &resolved, choice_id)
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
    lumina_core::MediaSource::parse(page_url)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_list_without_cache_is_business_error_without_network() {
        let service = YtdlService::new();
        let err = service
            .list_subtitle_choices("https://www.youtube.com/watch?v=uncached")
            .expect_err("cache miss must not trigger resolve");
        assert_eq!(err.message, "无法提取字幕");
    }

    #[test]
    fn subtitle_load_without_cache_is_business_error_without_network() {
        let service = YtdlService::new();
        let err = service
            .load_subtitle_choice("https://www.youtube.com/watch?v=uncached", "online:en")
            .expect_err("cache miss must not trigger download");
        assert_eq!(err.message, "无法提取字幕");
    }

    #[test]
    fn subtitle_load_with_unknown_choice_is_business_error() {
        use crate::model::{YtdlResolveResult, YtdlSubtitleTrack};

        let service = YtdlService::new();
        let resolved = YtdlResolveResult {
            media_id: "youtube:abc".into(),
            title: None,
            duration_ms: None,
            webpage_url: Some("https://www.youtube.com/watch?v=abc".into()),
            extractor: None,
            chapters: vec![],
            formats: vec![],
            subtitles: vec![YtdlSubtitleTrack {
                language: "en".into(),
                ext: Some("vtt".into()),
                name: None,
                url: None,
            }],
            recommended_url: None,
            recommended_format_id: None,
        };
        service
            .store_cache("https://www.youtube.com/watch?v=abc", resolved, None)
            .expect("store cache");
        // Cached resolve exists, but the requested language is absent:
        // must fail before any download attempt.
        let err = service
            .load_subtitle_choice("https://www.youtube.com/watch?v=abc", "online:xx")
            .expect_err("unknown language");
        assert_eq!(err.message, "无法提取字幕");
        // List path reuses the same cache and keeps the online:<lang> id.
        let choices = service
            .list_subtitle_choices("https://www.youtube.com/watch?v=abc")
            .expect("list from cache");
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].id, "online:en");
        assert!(choices[0].external_path.is_none());
    }
}
