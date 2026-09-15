//! ACP availability text + status aggregation (profiles + discovery).

use crate::agent::discover::{
    codex_config_present, find_acp_adapter, find_bunx, find_codex, native_acp_dir,
};
use crate::agent::profile::{list_status, prepare_profiles, AgentKind};
use crate::domain::model::{AcpStatus, AgentProfilesHint};
use crate::error::AcpError;

pub fn status_from_profiles(hint: &AgentProfilesHint) -> AcpStatus {
    let adapter_found = find_acp_adapter().is_some();
    let bunx_found = find_bunx().is_some();
    let codex_found = find_codex().is_some();
    let codex_config_found = codex_config_present();
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
        let is_codex = active.map(|p| p.kind == AgentKind::Codex).unwrap_or(false);
        let codex_home = codex_home_label();
        if is_codex && codex_found && codex_config_found {
            format!("{name} 已检测到本机配置，发起提问时将验证连接")
        } else if is_codex && codex_found {
            format!("{name} 已找到，但未检测到 {codex_home} 登录配置")
        } else if is_codex && bunx_found {
            format!("{name} 启动器已找到，首次提问将下载并验证 Agent")
        } else {
            format!("{name} 已就绪（仅在你发起会话时启动）")
        }
    } else if let Some(p) = active {
        if p.kind == AgentKind::Custom && p.command.is_empty() {
            "自定义 Agent 尚未填写启动命令".into()
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
    if bunx_found {
        if codex_found && codex_config_found {
            return "已找到本机 Codex 与 ~/.codex 配置；首次提问可能仍需下载 ACP 适配器。".into();
        }
        return if codex_found {
            "已找到本机 Codex；若提问失败，请在终端运行 codex login 完成登录。".into()
        } else {
            "将通过 Bun 按需获取并启动 Codex ACP；发布包自带兼容的 Codex。首次使用可能需要下载适配器。".into()
        };
    }
    if adapter_found {
        if codex_found {
            return "Codex ACP 适配器与 Codex 均已找到。模型侧请使用 Responses API（非 Chat Completions）。".into();
        }
        return "已找到 ACP 适配器；未找到 Codex 本体时，适配器可能仍能使用自带/环境中的 Codex。可选安装官方 Codex，或设置 CODEX_PATH。".into();
    }
    format!(
        "未找到可用 ACP Agent。请安装 Bun，或将 codex-acp 单文件放到 {}。播放功能不依赖这些可选组件。",
        native_acp_dir().display()
    )
}

pub const RESPONSES_ONLY_NOTE: &str =
    "Codex 自定义模型须使用 Responses API（wire_api=responses）。仅支持 Chat Completions 的地址需经 LiteLLM/OpenRouter 等网关，或改用其它 ACP Agent（如 Claude）。";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::profile::default_profiles_hint;

    #[test]
    fn status_message_is_chinese_when_missing() {
        let hint = default_profiles_hint();
        let status = status_from_profiles(&hint);
        assert!(
            status
                .message
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "{}",
            status.message
        );
        assert!(
            status
                .hint
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "{}",
            status.hint
        );
        assert!(status.responses_only_note.contains("Responses"));
    }

    #[test]
    fn install_hint_is_chinese() {
        let hint = install_hint(false, false, false, false);
        assert!(hint.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
        assert!(!hint.to_ascii_lowercase().contains("must install bun"));
    }

    #[test]
    fn bunx_hint_describes_on_demand_launch() {
        let hint = install_hint(false, true, true, false);
        assert!(hint.contains("Bun") || hint.contains("Codex"));
        assert!(!hint.contains("未找到可用"));
    }

    #[test]
    fn config_found_hint_mentions_codex_home() {
        let hint = install_hint(false, true, true, true);
        assert!(hint.contains("配置") || hint.contains("Codex"));
    }
}
