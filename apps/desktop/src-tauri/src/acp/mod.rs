//! App compat: ACP implementation lives in `lumina-acp` (M5).
//!
//! Chat snapshot lifecycle + MCP session environment live in `adapter`.

pub mod adapter;

pub use lumina_acp::*;
