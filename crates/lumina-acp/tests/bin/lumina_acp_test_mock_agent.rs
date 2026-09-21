use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;

use serde_json::{json, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state_path = std::env::var_os("LUMINA_ACP_MOCK_STATE")
        .map(PathBuf::from)
        .ok_or("LUMINA_ACP_MOCK_STATE is required")?;
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
            "session/prompt" if spawn_number == 1 => {
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
