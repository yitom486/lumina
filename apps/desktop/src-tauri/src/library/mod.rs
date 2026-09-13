//! App compat: library implementation lives in `lumina-library` (M7).
//!
//! Agent-backed resolver/discovery orchestration that needs ACP session
//! access lives in `adapter`; the crate only sees the core invoker port.

pub mod adapter;

pub use lumina_library::*;
