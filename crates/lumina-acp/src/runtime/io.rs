//! Line-delimited stdio IO for ACP: writes, reads, timeouts.
//!
//! Pure move from `runtime/service.rs` (no behavior change).

use std::io::{BufRead, Write};
use std::process::ChildStdin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::domain::model::AcpEvent;
use crate::error::AcpError;
use crate::runtime::host::AcpHost;
use crate::runtime::inbound::handle_inbound_side_effects;
use crate::runtime::lifecycle::LiveSession;
use crate::runtime::service::AcpService;
use crate::wire::codec::{classify_inbound, encode_line, is_error_response, notification, request};

/// Prompt-loop bounds, locked by unit test (silent timeout removal must fail loudly).
pub(crate) const PROMPT_DEADLINE_SECS: u64 = 600;
/// Grace period for an agent to honor `session/cancel` before the process is killed.
pub(crate) const CANCEL_KILL_SECS: u64 = 8;

pub(crate) fn write_request(
    stdin: &mut ChildStdin,
    id: u64,
    method: &str,
    params: Value,
) -> Result<(), AcpError> {
    let line = encode_line(&request(id, method, params))?;
    writeln!(stdin, "{line}").map_err(|error| {
        tracing::warn!(%error, method, "ACP write failed");
        AcpError::protocol(Some(&format!("write ACP request: {error}")))
    })?;
    stdin.flush().map_err(|error| {
        tracing::warn!(%error, "ACP flush failed");
        AcpError::protocol(Some(&format!("flush ACP stdin: {error}")))
    })?;
    Ok(())
}

pub(crate) fn write_notification(
    stdin: &mut ChildStdin,
    method: &str,
    params: Value,
) -> Result<(), AcpError> {
    let line = encode_line(&notification(method, params))?;
    writeln!(stdin, "{line}").map_err(|error| {
        tracing::warn!(%error, method, "ACP notification write failed");
        AcpError::protocol(Some(&format!("write ACP notification: {error}")))
    })?;
    stdin
        .flush()
        .map_err(|error| AcpError::protocol(Some(&format!("flush ACP stdin: {error}"))))?;
    Ok(())
}

pub(crate) fn write_raw(stdin: &mut ChildStdin, value: &Value) -> Result<(), AcpError> {
    let line = encode_line(value)?;
    writeln!(stdin, "{line}")
        .map_err(|error| AcpError::protocol(Some(&format!("write ACP response: {error}"))))?;
    stdin
        .flush()
        .map_err(|error| AcpError::protocol(Some(&format!("flush ACP stdin: {error}"))))?;
    Ok(())
}

pub(crate) fn read_until_id_raw(
    service: &AcpService,
    session: &mut LiveSession,
    target_id: u64,
    timeout: Duration,
    cancel: &AtomicBool,
    host: &AcpHost,
    on_event: &mut dyn FnMut(AcpEvent),
) -> Result<Value, AcpError> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(AcpError::cancelled());
        }
        if Instant::now() > deadline {
            return Err(AcpError::protocol(Some("ACP read timed out")));
        }
        match read_one(
            service,
            session,
            Duration::from_millis(200),
            cancel,
            host,
            on_event,
        )? {
            ReadOne::Eof => return Err(AcpError::protocol(Some("EOF on stdout"))),
            ReadOne::Response { id, value } if id == target_id => {
                if let Some(msg) = is_error_response(&value) {
                    return Err(AcpError::protocol(Some(&msg)));
                }
                return Ok(value);
            }
            ReadOne::Response { .. } => continue,
        }
    }
}

pub(crate) fn read_one(
    service: &AcpService,
    session: &mut LiveSession,
    _wait: Duration,
    cancel: &AtomicBool,
    host: &AcpHost,
    on_event: &mut dyn FnMut(AcpEvent),
) -> Result<ReadOne, AcpError> {
    let mut line_buf = String::new();
    loop {
        line_buf.clear();
        let bytes = session.reader.read_line(&mut line_buf).map_err(|error| {
            tracing::warn!(%error, "ACP read failed");
            AcpError::protocol(Some(&format!("read ACP stdout: {error}")))
        })?;
        if bytes == 0 {
            return Ok(ReadOne::Eof);
        }
        let trimmed = line_buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(error) => {
                // Truncated sample only: stdout may carry model text.
                let sample: String = trimmed.chars().take(200).collect();
                tracing::debug!(line = %sample, %error, "ACP skipping non-JSON stdout line");
                continue;
            }
        };

        match handle_inbound_side_effects(
            service,
            session,
            host,
            classify_inbound(value),
            cancel,
            on_event,
        )? {
            Some((id, value)) => return Ok(ReadOne::Response { id, value }),
            None => continue,
        }
    }
}

pub(crate) enum ReadOne {
    Eof,
    Response { id: u64, value: Value },
}

pub(crate) fn initialize_timeout() -> Duration {
    if cfg!(windows) {
        Duration::from_secs(180)
    } else {
        Duration::from_secs(60)
    }
}
