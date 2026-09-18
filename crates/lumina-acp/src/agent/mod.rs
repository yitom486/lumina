//! ACP agent building blocks: profiles, launch, status, workspace, discovery.

pub mod discover;
pub mod launch;
pub mod login;
pub mod profile;
pub mod status;
pub mod workspace;

pub use launch::{pick_auth_method, resolve_launch, LaunchSpec, CODEX_ACP_PACKAGE};
pub use login::login_antigravity;
pub use profile::{
    default_profiles_hint, list_status, prepare_profiles, resolve_active_profile, AgentKind,
    AgentProfile, AgentProfileStatus, PreparedProfiles,
};
pub use status::{install_hint, status_from_profiles, RESPONSES_ONLY_NOTE};
pub use workspace::resolve_session_cwd;
