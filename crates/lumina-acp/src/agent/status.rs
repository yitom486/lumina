//! ACP availability text + status aggregation (profiles + discovery).

use crate::agent::discover::{
    antigravity_credentials_present, codex_config_present, cursor_auth_present, find_acp_adapter,
    find_antigravity, find_bunx, find_codex, native_acp_dir,
};
use crate::agent::profile::{list_status, prepare_profiles, AgentKind};
use crate::domain::model::{AcpStatus, AgentProfilesHint};
use crate::error::AcpError;

pub fn status_from_profiles(hint: &AgentProfilesHint) -> AcpStatus {
    let adapter_found = find_acp_adapter().is_some();
    let bunx_found = find_bunx().is_some();
    let codex_found = find_codex().is_some();
    let codex_config_found = codex_config_present();
    let antigravity_found = find_antigravity().is_some();
    let antigravity_credentials_found = antigravity_credentials_present();
    let prepared = prepare_profiles(hint);
    let (active_id, profiles) = list_status(&prepared);

    let active = profiles.iter().find(|p| p.id == active_id);
    let available = active.map(|p| p.available).unwrap_or(false)
        && !(active.map(|p| p.command.is_empty()).unwrap_or(true));

    let cli_path = active
        .and_then(|p| p.resolved_command.clone())
        .or_else(|| find_acp_adapter().map(|p| p.to_string_lossy().to_string()));

    let message = if available {
        let name = active.map(|p| p.name.as_str()).unwrap_or("Agent");
        let active_profile = prepared.profiles.iter().find(|p| p.id == active_id);
        let codex_like = active_profile.is_some_and(|p| p.is_codex_like());
        let antigravity = active_profile.is_some_and(|p| p.is_antigravity());
        let codex_home = codex_home_label();
        if antigravity {
            let port = active
                .and_then(|p| p.env.get("ACP_PROXY_PORT"))
                .map(String::as_str)
                .unwrap_or("7897");
            if antigravity_credentials_found {
                format!("{name} 已就绪，已检测到 Google 账号授权凭据（代理端口：{port}）")
            } else {
                format!("{name} 已就绪，尚未登录 Google 账号，请在下方点击登录")
            }
        } else if codex_like && codex_found && codex_config_found {
            format!("{name} 已检测到本机 Codex 登录配置，将直接复用，无需重新认证")
        } else if codex_like && codex_found {
            format!("{name} 已找到，但未检测到 {codex_home} 登录配置")
        } else if codex_like && bunx_found {
            format!("{name} 启动器已找到，首次提问将下载并验证 Agent")
        } else if active_profile.is_some_and(|p| p.is_cursor_local_auth()) && cursor_auth_present()
        {
            format!("{name} 已检测到本机 Cursor 登录配置，将直接复用，无需重新认证")
        } else {
            format!("{name} 已就绪（仅在你发起会话时启动）")
        }
    } else if let Some(p) = active {
        if p.kind == AgentKind::Custom && p.command.is_empty() {
            "自定义 Agent 尚未填写启动命令".into()
        } else if p.kind == AgentKind::Cursor {
            "未找到 Cursor CLI（agent）：请先安装 Cursor CLI 并运行 agent login 后重试".into()
        } else if p.kind == AgentKind::Antigravity {
            "未找到 Google Antigravity ACP 程序（agy_acp_server.exe），请确认已安装或自定义程序路径"
                .into()
        } else {
            format!("当前 Agent「{}」不可用：找不到 {}", p.name, p.command)
        }
    } else {
        AcpError::not_configured(None).message
    };

    AcpStatus {
        available,
        adapter_found,
        codex_found,
        codex_config_found,
        antigravity_found,
        antigravity_credentials_found,
        active_profile_id: active_id,
        profiles,
        cli_path,
        codex_path: find_codex().map(|p| p.to_string_lossy().to_string()),
        message,
        hint: install_hint(adapter_found, codex_found, bunx_found, codex_config_found),
        responses_only_note: RESPONSES_ONLY_NOTE.into(),
        session_active: false,
        busy: false,
        session_model_options: None,
    }
}

fn codex_home_label() -> &'static str {
    if cfg!(windows) {
        "%USERPROFILE%\\.codex"
    } else {
        "~/.codex"
    }
}

/// Hint text: install guidance without requiring bun for end users.
pub fn install_hint(
    adapter_found: bool,
    codex_found: bool,
    bunx_found: bool,
    codex_config_found: bool,
) -> String {
    if adapter_found {
        String::new()
    } else if codex_found && !codex_config_found {
        "已找到 Codex 可执行文件；请在终端运行 `codex login` 或设置 OPENAI_API_KEY 后重试。".into()
    } else if codex_found {
        String::new()
    } else if bunx_found {
        "未找到本地独立适配器，将通过 bunx 运行官方 @agentclientprotocol/codex-acp。首次启动稍慢属正常现象。".into()
    } else {
        format!(
            "推荐：将预编译适配器放在 {}，或安装 bun / codex-cli。如使用其他 Agent，可在下方配置自定义启动命令。",
            native_acp_dir().display()
        )
    }
}

pub const RESPONSES_ONLY_NOTE: &str =
    "当前官方 Codex ACP 仅支持走 Responses API 的会话（ChatGPT 登录或已配 Responses 的网关）。纯 ChatCompletions 接口可能无法收到回复。";
