//! Tauri State for Player Runtime + native video surface.

use std::sync::Mutex;

use crate::player::error::PlayerError;
use crate::player::mpv::window::VideoSurface;
use crate::player::PlayerService;

pub struct AppState {
    player: Mutex<PlayerService>,
    surface: Mutex<Option<VideoSurface>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            player: Mutex::new(PlayerService::new()),
            surface: Mutex::new(None),
        }
    }

    pub fn with_player<T>(
        &self,
        f: impl FnOnce(&mut PlayerService) -> Result<T, PlayerError>,
    ) -> Result<T, PlayerError> {
        let mut player = self
            .player
            .lock()
            .map_err(|_| PlayerError::internal("player mutex poisoned", None))?;
        f(&mut player)
    }

    pub fn set_surface(&self, surface: VideoSurface) -> Result<(), PlayerError> {
        let mut slot = self
            .surface
            .lock()
            .map_err(|_| PlayerError::internal("surface mutex poisoned", None))?;
        *slot = Some(surface);
        Ok(())
    }

    pub fn with_surface<T>(
        &self,
        f: impl FnOnce(&VideoSurface) -> Result<T, PlayerError>,
    ) -> Result<T, PlayerError> {
        let slot = self
            .surface
            .lock()
            .map_err(|_| PlayerError::internal("surface mutex poisoned", None))?;
        let surface = slot
            .as_ref()
            .ok_or_else(|| PlayerError::internal("video surface is not ready", None))?;
        f(surface)
    }

    pub fn take_surface(&self) -> Option<VideoSurface> {
        self.surface.lock().ok().and_then(|mut slot| slot.take())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
