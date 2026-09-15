//! ACP agent building blocks: profiles, launch, status, workspace, discovery.

pub mod discover;
pub mod launch;
pub mod profile;
pub mod status;
pub mod workspace;

pub use launch::{resolve_launch, LaunchSpec, CODEX_ACP_PACKAGE};
pub use profile::{
    default_profiles_hint, list_status, prepare_profiles, resolve_active_profile, AgentKind,
    AgentProfile, AgentProfileStatus, PreparedProfiles,
};
pub use status::{install_hint, status_from_profiles, RESPONSES_ONLY_NOTE};
#[allow(deprecated)]
pub use workspace::resolve_acp_paths;
pub use workspace::{resolve_session_cwd, AcpPaths};
