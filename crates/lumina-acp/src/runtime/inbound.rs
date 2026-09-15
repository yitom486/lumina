//! Agent→Client inbound handling: permission, session/update, host tools.
//!
//! Pure move from `runtime/service.rs` (no behavior change). Includes the
//! `tool_access` gate for isolated tasks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

use crate::domain::model::{AcpEvent, PermissionOption};
use crate::domain::settings::PermissionMode;
use crate::error::AcpError;
use crate::runtime::host::AcpHost;
use crate::runtime::io::write_raw;
use crate::runtime::lifecycle::LiveSession;
use crate::runtime::service::AcpService;
use crate::wire::codec::{error_response, success_response, Inbound};
use crate::wire::permission::{
    extract_permission_options, permission_auto_result, permission_cancelled_result,
    permission_selected_result,
};
use crate::wire::updates::{
    extract_agent_text, extract_plan_summary, extract_thought_text, extract_tool_call,
    extract_tool_call_content_chunk,
};

pub(crate) fn handle_permission_request(
    service: &AcpService,
    id: Value,
    params: &Value,
    canceling: bool,
    on_event: &mut dyn FnMut(AcpEvent),
) -> Value {
    let tool_id = params
        .pointer("/toolCall/toolCallId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let title = params
        .pointer("/toolCall/title")
        .and_then(Value::as_str)
        .map(str::to_string);

    if canceling {
        on_event(AcpEvent::PermissionResolved {
            tool_call_id: tool_id.clone(),
            decision: "cancelled".into(),
        });
        return success_response(id, permission_cancelled_result());
    }

    let permission_mode = service
        .permission_mode
        .lock()
        .map(|guard| *guard)
        .unwrap_or(PermissionMode::Auto);
    if permission_mode == PermissionMode::Auto {
        on_event(AcpEvent::PermissionResolved {
            tool_call_id: tool_id,
            decision: "auto".into(),
        });
        return success_response(id, permission_auto_result(params, false));
    }

    let request_id = format!(
        "perm-{}",
        service.permission_seq.fetch_add(1, Ordering::SeqCst)
    );
    let options: Vec<PermissionOption> = extract_permission_options(params)
        .into_iter()
        .map(|(option_id, name, kind)| PermissionOption {
            option_id,
            name,
            kind,
        })
        .collect();

    on_event(AcpEvent::PermissionRequest {
        request_id: request_id.clone(),
        tool_call_id: tool_id.clone(),
        title,
        options: options.clone(),
    });

    let (tx, rx) = mpsc::channel();
    if let Ok(mut guard) = service.permission_replies.lock() {
        *guard = Some(tx);
    }

    let selected = rx.recv_timeout(Duration::from_secs(120)).unwrap_or(None);
    let _ = service.permission_replies.lock().map(|mut g| {
        g.take();
    });

    let result = match selected {
        Some(option_id) if !option_id.is_empty() => {
            on_event(AcpEvent::PermissionResolved {
                tool_call_id: tool_id,
                decision: "approved".into(),
            });
            permission_selected_result(&option_id)
        }
        _ => {
            on_event(AcpEvent::PermissionResolved {
                tool_call_id: tool_id,
                decision: "denied".into(),
            });
            permission_cancelled_result()
        }
    };
    success_response(id, result)
}

pub(crate) fn emit_session_update(value: &Value, on_event: &mut dyn FnMut(AcpEvent)) {
    if let Some(text) = extract_agent_text(value) {
        if !text.is_empty() {
            on_event(AcpEvent::AgentMessage { text });
        }
    }
    if let Some(text) = extract_thought_text(value) {
        if !text.is_empty() {
            on_event(AcpEvent::AgentThought { text });
        }
    }
    if let Some(tool) = extract_tool_call(value) {
        if tool.update_kind == "tool_call" {
            on_event(AcpEvent::ToolCall {
                tool_call_id: tool.tool_call_id,
                title: tool.title,
                kind: tool.kind,
                status: tool.status,
                detail: tool.detail,
            });
        } else {
            on_event(AcpEvent::ToolCallUpdate {
                tool_call_id: tool.tool_call_id,
                status: tool.status,
                title: tool.title,
                detail: tool.detail,
                append_detail: tool.append_detail,
            });
        }
    } else if let Some((tool_call_id, detail)) = extract_tool_call_content_chunk(value) {
        if !detail.is_empty() {
            on_event(AcpEvent::ToolCallUpdate {
                tool_call_id,
                status: None,
                title: None,
                detail: Some(detail),
                append_detail: true,
            });
        }
    }
    if let Some(text) = extract_plan_summary(value) {
        on_event(AcpEvent::Plan { text });
    }
}

pub(crate) fn handle_inbound_side_effects(
    service: &AcpService,
    session: &mut LiveSession,
    host: &AcpHost,
    inbound: Inbound,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(AcpEvent),
) -> Result<Option<(u64, Value)>, AcpError> {
    match inbound {
        Inbound::Response { id, value } => Ok(Some((id, value))),
        Inbound::Notification { method, params } => {
            if method == "session/update" {
                let wrapped = serde_json::json!({
                    "method": "session/update",
                    "params": params,
                });
                emit_session_update(&wrapped, on_event);
            } else {
                tracing::debug!(%method, "ignored ACP notification");
            }
            Ok(None)
        }
        Inbound::AgentRequest { id, method, params } => {
            if !service.tool_access_enabled.load(Ordering::SeqCst) {
                let response = error_response(id, -32_001, "Tool access is disabled");
                write_raw(&mut session.stdin, &response)?;
                return Ok(None);
            }
            let canceling = cancel.load(Ordering::SeqCst);
            let response = if method == "session/request_permission" {
                handle_permission_request(service, id, &params, canceling, on_event)
            } else {
                if method.starts_with("fs/") {
                    on_event(AcpEvent::Progress {
                        message: format!("文件系统：{method}"),
                    });
                } else if method.starts_with("terminal/") {
                    on_event(AcpEvent::Progress {
                        message: format!("终端：{method}"),
                    });
                }
                host.handle_request(&method, id, &params, canceling)
            };
            write_raw(&mut session.stdin, &response)?;
            Ok(None)
        }
        Inbound::Other(value) => {
            tracing::debug!(%value, "ignored ACP message");
            Ok(None)
        }
    }
}
