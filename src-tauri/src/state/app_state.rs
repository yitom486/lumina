//! Tauri State for Player Runtime + native video surface + event Channel.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::player::error::PlayerError;
use crate::player::model::PlayerEvent;
use crate::player::mpv::window::VideoSurface;
use crate::player::PlayerService;

const POSITION_TICK_MS: u64 = 200;

pub struct AppState {
    player: Mutex<PlayerService>,
    surface: Mutex<Option<VideoSurface>>,
    events: Mutex<Option<Channel<PlayerEvent>>>,
    ticker_started: AtomicBool,
    shutdown: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            player: Mutex::new(PlayerService::new()),
            surface: Mutex::new(None),
            events: Mutex::new(None),
            ticker_started: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
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

    pub fn set_event_channel(&self, channel: Channel<PlayerEvent>) -> Result<(), PlayerError> {
        let mut slot = self
            .events
            .lock()
            .map_err(|_| PlayerError::internal("events mutex poisoned", None))?;
        *slot = Some(channel);
        tracing::info!("player event channel subscribed");
        Ok(())
    }

    pub fn emit(&self, event: PlayerEvent) {
        let Ok(slot) = self.events.lock() else {
            return;
        };
        let Some(channel) = slot.as_ref() else {
            return;
        };
        if let Err(error) = channel.send(event) {
            tracing::warn!(%error, "failed to send player event");
        }
    }

    pub fn emit_all(&self, events: Vec<PlayerEvent>) {
        for event in events {
            self.emit(event);
        }
    }

    pub fn ensure_event_ticker(&self, app: AppHandle) {
        if self.ticker_started.swap(true, Ordering::SeqCst) {
            return;
        }

        let result = std::thread::Builder::new()
            .name("player-events".into())
            .spawn(move || {
                tracing::info!(
                    interval_ms = POSITION_TICK_MS,
                    "player event ticker started"
                );
                loop {
                    std::thread::sleep(Duration::from_millis(POSITION_TICK_MS));
                    let Some(state) = app.try_state::<AppState>() else {
                        break;
                    };
                    if state.shutdown.load(Ordering::SeqCst) {
                        break;
                    }

                    let events = match state.with_player(|player| {
                        if player.is_shutdown() {
                            return Ok(Vec::new());
                        }
                        Ok(player.poll_tick())
                    }) {
                        Ok(events) => events,
                        Err(_) => continue,
                    };

                    if !events.is_empty() {
                        state.emit_all(events);
                    }
                }
                tracing::info!("player event ticker stopped");
            });

        if let Err(error) = result {
            self.ticker_started.store(false, Ordering::SeqCst);
            tracing::error!(%error, "failed to spawn player event ticker");
        }
    }

    pub fn mark_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.events.lock() {
            *slot = None;
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
