//! ACP permission wire helpers (official SDK types; tolerant fallback kept).
//!
//! Primary path uses `agent_client_protocol::schema::v1` so the wire shapes
//! track the spec instead of hand-written JSON. The SDK is stricter than
//! observed agents in two places, so option-list extraction keeps the
//! historical manual walk as fallback:
//! - unknown `kind` strings fail the whole SDK request parse (we used to skip
//!   just that option);
//! - `name` is required by the SDK (we used to default it to `optionId`).

use agent_client_protocol::schema::v1::{
    PermissionOptionId, RequestPermissionOutcome, RequestPermissionResponse,
    SelectedPermissionOutcome,
};
use serde_json::{json, Value};

fn response_value(response: RequestPermissionResponse) -> Value {
    serde_json::to_value(&response)
        .unwrap_or_else(|_| json!({ "outcome": { "outcome": "cancelled" } }))
}

/// Build permission response from user-selected option id.
pub fn permission_selected_result(option_id: &str) -> Value {
    response_value(RequestPermissionResponse::new(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
            PermissionOptionId::new(option_id),
        )),
    ))
}

pub fn permission_cancelled_result() -> Value {
    response_value(RequestPermissionResponse::new(
        RequestPermissionOutcome::Cancelled,
    ))
}

pub fn extract_permission_options(params: &Value) -> Vec<(String, String, Option<String>)> {
    if let Ok(request) = serde_json::from_value::<
        agent_client_protocol::schema::v1::RequestPermissionRequest,
    >(params.clone())
    {
        let mut out = Vec::new();
        for option in &request.options {
            let option_id = option.option_id.0.to_string();
            if option_id.is_empty() {
                continue;
            }
            let kind = serde_json::to_value(option.kind)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string));
            out.push((option_id, option.name.clone(), kind));
        }
        return out;
    }
    extract_permission_options_manual(params)
}

fn extract_permission_options_manual(params: &Value) -> Vec<(String, String, Option<String>)> {
    let mut out = Vec::new();
    if let Some(arr) = params.get("options").and_then(Value::as_array) {
        for opt in arr {
            let option_id = opt
                .get("optionId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if option_id.is_empty() {
                continue;
            }
            let name = opt
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(option_id.as_str())
                .to_string();
            let kind = opt.get("kind").and_then(Value::as_str).map(str::to_string);
            out.push((option_id, name, kind));
        }
    }
    out
}

/// Auto-resolve permission: prefer allow_* option, else cancelled.
pub fn permission_auto_result(params: &Value, canceling: bool) -> Value {
    if canceling {
        return permission_cancelled_result();
    }
    for (option_id, _, kind) in extract_permission_options(params) {
        if kind.as_deref().unwrap_or("").starts_with("allow") {
            return permission_selected_result(&option_id);
        }
    }
    permission_cancelled_result()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn permission_prefers_allow_option() {
        let params = json!({
            "options": [
                { "optionId": "deny", "kind": "reject_once", "name": "Deny" },
                { "optionId": "allow", "kind": "allow_once", "name": "Allow" }
            ]
        });
        let result = permission_auto_result(&params, false);
        assert_eq!(
            result.pointer("/outcome/optionId").and_then(Value::as_str),
            Some("allow")
        );
    }

    #[test]
    fn permission_selected_result_shape_matches_spec() {
        let result = permission_selected_result("allow-once");
        assert_eq!(
            result.pointer("/outcome/outcome").and_then(Value::as_str),
            Some("selected")
        );
        assert_eq!(
            result.pointer("/outcome/optionId").and_then(Value::as_str),
            Some("allow-once")
        );
    }

    #[test]
    fn permission_tolerates_unknown_option_kinds() {
        // SDK 会整单拒绝未知 kind；手工兜底只跳过坏项，不断整单。
        let params = json!({
            "options": [
                { "optionId": "weird", "kind": "allow_eventually", "name": "Weird" },
                { "optionId": "", "kind": "allow_once", "name": "Empty" }
            ]
        });
        let options = extract_permission_options(&params);
        assert_eq!(
            options,
            vec![(
                "weird".to_string(),
                "Weird".to_string(),
                Some("allow_eventually".to_string())
            )]
        );
        let result = permission_auto_result(&params, false);
        assert_eq!(
            result.pointer("/outcome/optionId").and_then(Value::as_str),
            Some("weird")
        );
    }
}
