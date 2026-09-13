//! App compat: subtitle implementation lives in `lumina-subtitle` (M3).
//!
//! `translate` stays here as a temporary bridge (it depends on ACP) until M5 `lumina-ai`.
//! Existing `crate::subtitle::…` paths keep working through this re-export.

pub mod translate;

pub use lumina_subtitle::*;
