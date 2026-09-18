//! MCP 反向播放控制桥（`lumina_seek_playback` 的宿主侧）。
//!
//! `lumina-app.exe --lumina-mcp` 子进程不持有播放器：seek 请求经
//! `<workspace>/.lumina/mcp-control.json` 下发，本 watcher 消费（nonce
//! 去重 + 陈旧请求丢弃 + 时长钳制），执行后写回执供 MCP 工具限时轮询。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::Manager;

use crate::state::AppState;

const CONTROL_FILE: &str = "mcp-control.json";
const RESULT_FILE: &str = "mcp-control-result.json";
/// 请求新鲜度：超过此时长的请求视为陈旧（进程重启/迟到的孤儿文件）。
const REQUEST_MAX_AGE_MS: u128 = 30_000;
const POLL_INTERVAL_MS: u64 = 400;
/// 每个工作区保留的已处理 nonce 上限（防无限增长）。
const PROCESSED_NONCE_CAP: usize = 32;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ParsedSeekRequest {
    pub position_ms: u64,
}

/// 纯函数：校验并解析控制文件内容（无 IO，便于单测）。
pub(crate) fn parse_control_request(
    value: &Value,
    now_ms: u128,
) -> Result<ParsedSeekRequest, String> {
    if value.get("kind").and_then(Value::as_str) != Some("seek") {
        return Err("未知控制请求类型".into());
    }
    let position_ms = value
        .get("positionMs")
        .and_then(Value::as_u64)
        .ok_or_else(|| "跳转请求缺少 positionMs".to_string())?;
    let requested_at_ms = value
        .get("requestedAtMs")
        .and_then(Value::as_u64)
        .map(|value| value as u128)
        .ok_or_else(|| "跳转请求缺少 requestedAtMs".to_string())?;
    let age = now_ms.saturating_sub(requested_at_ms);
    if age > REQUEST_MAX_AGE_MS {
        return Err("跳转请求已过期".into());
    }
    Ok(ParsedSeekRequest { position_ms })
}

/// 纯函数：把请求位置钳制进媒体时长（无时长信息则原样返回）。
pub(crate) fn clamp_position(position_ms: u64, duration_ms: u64) -> u64 {
    if duration_ms > 0 {
        position_ms.min(duration_ms)
    } else {
        position_ms
    }
}

static WORKSPACES: Mutex<Option<HashMap<PathBuf, HashSet<String>>>> = Mutex::new(None);

fn with_workspaces<R>(f: impl FnOnce(&mut HashMap<PathBuf, HashSet<String>>) -> R) -> Option<R> {
    let Ok(mut guard) = WORKSPACES.lock() else {
        return None;
    };
    Some(f(guard.get_or_insert_with(HashMap::new)))
}

/// MCP snapshot 同步时登记工作区（`AppSessionEnvironment` 每次都会调用）。
pub(crate) fn register_workspace(workspace: &Path) {
    let _ = with_workspaces(|map| {
        map.entry(workspace.to_path_buf()).or_default();
    });
}

fn workspace_control_path(workspace: &Path) -> PathBuf {
    workspace.join(".lumina").join(CONTROL_FILE)
}

fn workspace_result_path(workspace: &Path) -> PathBuf {
    workspace.join(".lumina").join(RESULT_FILE)
}

fn process_one_workspace(app: &tauri::AppHandle, workspace: &Path) {
    let text = match std::fs::read_to_string(workspace_control_path(workspace)) {
        Ok(text) => text,
        Err(_) => return,
    };
    let value = match serde_json::from_str::<Value>(&text) {
        Ok(value) => value,
        Err(_) => return,
    };
    let Some(nonce) = value
        .get("nonce")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    let already = with_workspaces(|map| {
        map.get(workspace)
            .map(|seen| seen.contains(&nonce))
            .unwrap_or(false)
    });
    if already == Some(true) {
        return;
    }
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let request = match parse_control_request(&value, now_ms) {
        Ok(request) => request,
        Err(reason) => {
            write_result(
                workspace,
                &nonce,
                json!({ "status": "error", "message": reason }),
            );
            let _ = with_workspaces(|map| {
                remember_nonce(map, workspace, nonce.clone());
            });
            return;
        }
    };

    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let seek = state.with_player(|player| player.seek(request.position_ms));
    match seek {
        Ok((snapshot, events)) => {
            state.emit_all(events);
            let landed = clamp_position(request.position_ms, snapshot.duration_ms);
            write_result(
                workspace,
                &nonce,
                json!({ "status": "ok", "landedPositionMs": landed }),
            );
            tracing::info!(
                workspace = %workspace.display(),
                position_ms = request.position_ms,
                landed_position_ms = landed,
                "MCP seek request executed"
            );
        }
        Err(error) => {
            write_result(
                workspace,
                &nonce,
                json!({ "status": "error", "message": "播放器未能完成跳转" }),
            );
            tracing::warn!(%error, "MCP seek request failed");
        }
    }
    let _ = with_workspaces(|map| {
        remember_nonce(map, workspace, nonce);
    });
}

fn write_result(workspace: &Path, nonce: &str, payload: Value) {
    let mut payload = payload;
    if let Some(object) = payload.as_object_mut() {
        object.insert("nonce".into(), json!(nonce));
    }
    let path = workspace_result_path(workspace);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(&payload) {
        let _ = std::fs::write(&path, text);
    }
}

/// 启动轮询线程（app 启动时调用一次；无工作区时空转）。
pub fn start_control_watcher(app: tauri::AppHandle) {
    let _ = std::thread::Builder::new()
        .name("mcp-control".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            let workspaces: Vec<PathBuf> =
                with_workspaces(|map| map.keys().cloned().collect()).unwrap_or_default();
            for workspace in workspaces {
                process_one_workspace(&app, &workspace);
            }
        });
}

fn remember_nonce(map: &mut HashMap<PathBuf, HashSet<String>>, workspace: &Path, nonce: String) {
    let seen = map.entry(workspace.to_path_buf()).or_default();
    seen.insert(nonce);
    if seen.len() > PROCESSED_NONCE_CAP {
        seen.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_request_rejects_unknown_kind_and_missing_fields() {
        let now = 1_000_000_u128;
        assert!(parse_control_request(&json!({ "kind": "other" }), now).is_err());
        assert!(
            parse_control_request(&json!({ "kind": "seek", "requestedAtMs": now }), now).is_err()
        );
        let ok = parse_control_request(
            &json!({ "kind": "seek", "positionMs": 192_000, "requestedAtMs": now }),
            now,
        )
        .expect("valid seek");
        assert_eq!(ok.position_ms, 192_000);
    }

    #[test]
    fn stale_requests_are_rejected() {
        let now = 1_000_000_u128;
        let stale = json!({ "kind": "seek", "positionMs": 1_000, "requestedAtMs": now - 31_000 });
        let error = parse_control_request(&stale, now).expect_err("stale rejected");
        assert!(error.contains("过期"));
    }

    #[test]
    fn position_is_clamped_to_duration() {
        assert_eq!(clamp_position(10_000, 60_000), 10_000);
        assert_eq!(clamp_position(70_000, 60_000), 60_000);
        assert_eq!(clamp_position(70_000, 0), 70_000);
    }
}
