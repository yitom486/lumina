//! ACP wire helpers (split from `wire/protocol.rs`, no behavior change).
//!
//! `wire::protocol::*` remains available via the `protocol` compatibility
//! shim below so old paths and tests keep resolving.

pub mod codec;
pub mod permission;
pub mod sanitize;
pub mod session;
pub mod updates;

pub use codec::*;
pub use permission::*;
pub use sanitize::*;
pub use session::*;
pub use updates::*;

pub mod protocol {
    pub use super::*;
}
