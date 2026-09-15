//! ACP permission wire helpers.
//! Split from `wire/protocol.rs` without behavior change.

use serde_json::{json, Value};

/// Build permission response from user-selected option id.
pub fn permission_selected_result(option_id: &str) -> Value {
    json!({
        "outcome": { "outcome": "selected", "optionId": option_id }
    })
}

pub fn permission_cancelled_result() -> Value {
    json!({ "outcome": { "outcome": "cancelled" } })
}

pub fn extract_permission_options(params: &Value) -> Vec<(String, String, Option<String>)> {
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
        return json!({ "outcome": { "outcome": "cancelled" } });
    }
    let options = params
        .get("options")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for opt in &options {
        let kind = opt.get("kind").and_then(Value::as_str).unwrap_or("");
        if kind.starts_with("allow") {
            if let Some(option_id) = opt.get("optionId").and_then(Value::as_str) {
                return json!({
                    "outcome": { "outcome": "selected", "optionId": option_id }
                });
            }
        }
    }
    json!({ "outcome": { "outcome": "cancelled" } })
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
}
