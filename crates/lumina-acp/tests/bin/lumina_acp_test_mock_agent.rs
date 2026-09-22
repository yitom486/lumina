use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;

use serde_json::{json, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state_path = std::env::var_os("LUMINA_ACP_MOCK_STATE")
        .map(PathBuf::from)
        .ok_or("LUMINA_ACP_MOCK_STATE is required")?;
    // Resume blackbox scripts (tests/session_resume_blackbox.rs). Unset/empty
    // keeps the legacy chapter behavior byte-for-byte.
    let script = std::env::var("LUMINA_ACP_MOCK_SCRIPT").unwrap_or_default();
    let log_path = std::env::var_os("LUMINA_ACP_MOCK_LOG").map(PathBuf::from);
    let is_chapter_script = script.is_empty() || script == "chapter";
    let spawn_number = next_spawn_number(&state_path)?;
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());

    for line in BufReader::new(stdin.lock()).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line)?;
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        log_method(log_path.as_ref(), method);
        let Some(id) = request.get("id").cloned() else {
            continue;
        };

        match method {
            "initialize" => write_response(
                &mut stdout,
                id,
                json!({
                    "protocolVersion": 1,
                    "agentInfo": { "name": "lumina-test-mock-agent" },
                    "agentCapabilities": {
                        "sessionCapabilities": { "close": {} }
                    }
                }),
            )?,
            "session/new" => write_response(
                &mut stdout,
                id,
                json!({ "sessionId": format!("mock-session-{spawn_number}") }),
            )?,
            "session/resume" => match script.as_str() {
                "resume-fail-unavailable" => {
                    write_error(&mut stdout, id, -32000, "no rollout found for thread id")?
                }
                "resume-fail-occupied" => write_error(
                    &mut stdout,
                    id,
                    -32000,
                    "thread mock-session already has an active writer",
                )?,
                // Success (resume-ok and the legacy default): the harness
                // asserts on the client's SessionSaved event, not the payload.
                _ => write_response(&mut stdout, id, json!({}))?,
            },
            // The transport-recovery kill belongs to the chapter script only;
            // resume scripts answer prompts normally so they never interfere.
            "session/prompt" if spawn_number == 1 && is_chapter_script => {
                eprintln!("mock-agent-transport-detail-must-not-reach-message");
                return Ok(());
            }
            "session/prompt" => {
                let session_id = request
                    .pointer("/params/sessionId")
                    .and_then(Value::as_str)
                    .unwrap_or("mock-session-2");
                write_notification(
                    &mut stdout,
                    "session/update",
                    json!({
                        "sessionId": session_id,
                        "update": {
                            "sessionUpdate": "agent_message_chunk",
                            "content": {
                                "type": "text",
                                "text": "恢复成功：mock ACP"
                            }
                        }
                    }),
                )?;
                write_response(&mut stdout, id, json!({ "stopReason": "end_turn" }))?;
            }
            "session/close" => {
                write_response(&mut stdout, id, json!({}))?;
                return Ok(());
            }
            _ => write_response(&mut stdout, id, json!({}))?,
        }
    }

    Ok(())
}

fn next_spawn_number(path: &PathBuf) -> Result<u64, Box<dyn std::error::Error>> {
    let previous = match fs::read_to_string(path) {
        Ok(value) => value.trim().parse::<u64>()?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    let next = previous.saturating_add(1);
    fs::write(path, next.to_string())?;
    Ok(next)
}

fn write_response(
    stdout: &mut impl Write,
    id: Value,
    result: Value,
) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(
        &mut *stdout,
        &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
    )?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn write_notification(
    stdout: &mut impl Write,
    method: &str,
    params: Value,
) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(
        &mut *stdout,
        &json!({ "jsonrpc": "2.0", "method": method, "params": params }),
    )?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn write_error(
    stdout: &mut impl Write,
    id: Value,
    code: i64,
    message: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(
        &mut *stdout,
        &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
    )?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

/// Append one inbound request method per line so blackbox tests can assert
/// whether `session/resume` was attempted (and its order vs `session/new`).
/// Best effort: logging must never break the mocked protocol.
fn log_method(path: Option<&PathBuf>, method: &str) {
    if let Some(path) = path {
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{method}");
        }
    }
}
