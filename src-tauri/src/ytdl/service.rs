//! YtdlService — busy lock around install / resolve.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::ytdl::download;
use crate::ytdl::error::YtdlError;
use crate::ytdl::model::{YtdlInstallEvent, YtdlResolveResult, YtdlStatus};
use crate::ytdl::paths;
use crate::ytdl::resolve;

pub struct YtdlService {
    busy: AtomicBool,
}

impl YtdlService {
    pub fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
        }
    }

    pub fn status(&self) -> YtdlStatus {
        paths::status()
    }

    pub fn install<F>(&self, on_event: F) -> Result<YtdlStatus, YtdlError>
    where
        F: FnMut(YtdlInstallEvent),
    {
        self.with_busy(|| download::install_cli(on_event))
    }

    pub fn resolve(&self, url: &str) -> Result<YtdlResolveResult, YtdlError> {
        self.with_busy(|| resolve::resolve_url(url))
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

impl Default for YtdlService {
    fn default() -> Self {
        Self::new()
    }
}
