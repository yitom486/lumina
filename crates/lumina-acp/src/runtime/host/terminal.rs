//! Terminal half of `AcpHost`: managed child processes + output cache.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use crate::error::AcpError;
use crate::runtime::process::command;

use super::fs::require_str;
use super::AcpHost;

static TERMINAL_SEQ: AtomicU64 = AtomicU64::new(1);

pub(super) struct ManagedTerminal {
    pub(super) child: Child,
    pub(super) output: Arc<Mutex<String>>,
    pub(super) truncated: Arc<AtomicBool>,
    pub(super) exited: Arc<AtomicBool>,
    pub(super) exit_code: Arc<Mutex<Option<i32>>>,
    pub(super) signal: Arc<Mutex<Option<String>>>,
}

impl AcpHost {
    pub(super) fn terminal_create(&self, params: &Value) -> Result<Value, AcpError> {
        let program = params
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| AcpError::bad_request("终端命令缺失"))?;
        let args: Vec<String> = params
            .get("args")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let cwd = match params.get("cwd").and_then(Value::as_str) {
            Some(raw) => {
                let path = PathBuf::from(raw);
                if !path.is_absolute() {
                    return Err(AcpError::bad_request("终端工作目录必须是绝对路径"));
                }
                path
            }
            None => self.workspace_cwd()?,
        };

        let byte_limit = params
            .get("outputByteLimit")
            .and_then(Value::as_u64)
            .unwrap_or(1024 * 1024) as usize;

        let mut cmd = command(program);
        cmd.args(&args)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(env_arr) = params.get("env").and_then(Value::as_array) {
            for item in env_arr {
                let name = item.get("name").and_then(Value::as_str);
                let value = item.get("value").and_then(Value::as_str);
                if let (Some(name), Some(value)) = (name, value) {
                    cmd.env(name, value);
                }
            }
        }

        let mut child = cmd.spawn().map_err(|error| {
            tracing::warn!(%program, cwd = %cwd.display(), %error, "ACP terminal spawn failed");
            AcpError::spawn_failed(Some(&error.to_string()))
        })?;

        let output = Arc::new(Mutex::new(String::new()));
        let truncated = Arc::new(AtomicBool::new(false));
        let exited = Arc::new(AtomicBool::new(false));
        let exit_code = Arc::new(Mutex::new(None));
        let signal = Arc::new(Mutex::new(None));

        if let Some(stdout) = child.stdout.take() {
            spawn_pipe_reader(stdout, output.clone(), truncated.clone(), byte_limit);
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_pipe_reader(stderr, output.clone(), truncated.clone(), byte_limit);
        }

        let terminal_id = format!("term-{}", TERMINAL_SEQ.fetch_add(1, Ordering::SeqCst));
        tracing::info!(%terminal_id, %program, cwd = %cwd.display(), "ACP terminal/create");

        let mut map = self
            .terminals
            .lock()
            .map_err(|_| AcpError::internal(Some("terminal map poisoned")))?;
        map.insert(
            terminal_id.clone(),
            ManagedTerminal {
                child,
                output,
                truncated,
                exited,
                exit_code,
                signal,
            },
        );

        Ok(json!({ "terminalId": terminal_id }))
    }

    pub(super) fn terminal_output(&self, params: &Value) -> Result<Value, AcpError> {
        let id = require_str(params, "terminalId")?;
        let mut map = self
            .terminals
            .lock()
            .map_err(|_| AcpError::internal(Some("terminal map poisoned")))?;
        let term = map
            .get_mut(id)
            .ok_or_else(|| AcpError::protocol(Some(&format!("unknown terminalId: {id}"))))?;

        refresh_exit_status(term);

        let output = term
            .output
            .lock()
            .map_err(|_| AcpError::internal(Some("terminal output lock poisoned")))?
            .clone();
        let truncated = term.truncated.load(Ordering::SeqCst);
        let exit_status = if term.exited.load(Ordering::SeqCst) {
            let code = *term
                .exit_code
                .lock()
                .map_err(|_| AcpError::internal(Some("exit code lock poisoned")))?;
            let signal = term
                .signal
                .lock()
                .map_err(|_| AcpError::internal(Some("signal lock poisoned")))?
                .clone();
            Some(json!({
                "exitCode": code,
                "signal": signal,
            }))
        } else {
            None
        };

        Ok(json!({
            "output": output,
            "truncated": truncated,
            "exitStatus": exit_status,
        }))
    }

    pub(super) fn terminal_wait_for_exit(&self, params: &Value) -> Result<Value, AcpError> {
        let id = require_str(params, "terminalId")?;
        let deadline = std::time::Instant::now() + Duration::from_secs(600);
        loop {
            {
                let mut map = self
                    .terminals
                    .lock()
                    .map_err(|_| AcpError::internal(Some("terminal map poisoned")))?;
                let term = map.get_mut(id).ok_or_else(|| {
                    AcpError::protocol(Some(&format!("unknown terminalId: {id}")))
                })?;
                refresh_exit_status(term);
                if term.exited.load(Ordering::SeqCst) {
                    let code = *term
                        .exit_code
                        .lock()
                        .map_err(|_| AcpError::internal(Some("exit code lock poisoned")))?;
                    let signal = term
                        .signal
                        .lock()
                        .map_err(|_| AcpError::internal(Some("signal lock poisoned")))?
                        .clone();
                    return Ok(json!({
                        "exitCode": code,
                        "signal": signal,
                    }));
                }
            }
            if std::time::Instant::now() > deadline {
                return Err(AcpError::protocol(Some("terminal wait timed out")));
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    pub(super) fn terminal_kill(&self, params: &Value) -> Result<Value, AcpError> {
        let id = require_str(params, "terminalId")?;
        let mut map = self
            .terminals
            .lock()
            .map_err(|_| AcpError::internal(Some("terminal map poisoned")))?;
        let term = map
            .get_mut(id)
            .ok_or_else(|| AcpError::protocol(Some(&format!("unknown terminalId: {id}"))))?;
        let _ = term.child.kill();
        refresh_exit_status(term);
        tracing::info!(terminal_id = id, "ACP terminal/kill");
        Ok(json!({}))
    }

    pub(super) fn terminal_release(&self, params: &Value) -> Result<Value, AcpError> {
        let id = require_str(params, "terminalId")?;
        let mut map = self
            .terminals
            .lock()
            .map_err(|_| AcpError::internal(Some("terminal map poisoned")))?;
        if let Some(mut term) = map.remove(id) {
            let _ = term.child.kill();
            let _ = term.child.wait();
            tracing::info!(terminal_id = id, "ACP terminal/release");
        }
        Ok(json!({}))
    }
}

fn spawn_pipe_reader<R: Read + Send + 'static>(
    reader: R,
    output: Arc<Mutex<String>>,
    truncated: Arc<AtomicBool>,
    byte_limit: usize,
) {
    thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]);
                    if let Ok(mut out) = output.lock() {
                        out.push_str(&chunk);
                        if out.len() > byte_limit {
                            let overflow = out.len() - byte_limit;
                            let mut cut = overflow;
                            while cut < out.len() && !out.is_char_boundary(cut) {
                                cut += 1;
                            }
                            *out = out[cut..].to_string();
                            truncated.store(true, Ordering::SeqCst);
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });
}

fn refresh_exit_status(term: &mut ManagedTerminal) {
    if term.exited.load(Ordering::SeqCst) {
        return;
    }
    match term.child.try_wait() {
        Ok(Some(status)) => {
            term.exited.store(true, Ordering::SeqCst);
            if let Ok(mut code) = term.exit_code.lock() {
                *code = status.code();
            }
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(sig) = status.signal() {
                    if let Ok(mut signal) = term.signal.lock() {
                        *signal = Some(sig.to_string());
                    }
                }
            }
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(%error, "try_wait terminal failed");
            term.exited.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn terminal_echo_and_wait() {
        let host = AcpHost::new();
        let cwd = std::env::temp_dir();
        host.set_workspace(cwd.clone());

        #[cfg(windows)]
        let create = host
            .terminal_create(&json!({
                "command": "cmd",
                "args": ["/C", "echo", "lumina-term"],
                "cwd": cwd.to_string_lossy(),
            }))
            .expect("create");
        #[cfg(not(windows))]
        let create = host
            .terminal_create(&json!({
                "command": "sh",
                "args": ["-c", "echo lumina-term"],
                "cwd": cwd.to_string_lossy(),
            }))
            .expect("create");

        let tid = create
            .get("terminalId")
            .and_then(Value::as_str)
            .expect("tid")
            .to_string();
        let wait = host
            .terminal_wait_for_exit(&json!({ "terminalId": tid }))
            .expect("wait");
        assert!(wait.get("exitCode").and_then(Value::as_i64).is_some());
        let mut text = String::new();
        for _ in 0..20 {
            let out = host
                .terminal_output(&json!({ "terminalId": tid }))
                .expect("output");
            text = out
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if text.contains("lumina-term") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(text.contains("lumina-term"), "got: {text}");
        host.terminal_release(&json!({ "terminalId": tid }))
            .expect("release");
    }

    #[test]
    fn terminal_defaults_to_workspace_cwd() {
        let host = AcpHost::new();
        let cwd = std::env::temp_dir();
        host.set_workspace(cwd);

        #[cfg(windows)]
        let create = host
            .terminal_create(&json!({
                "command": "cmd",
                "args": ["/C", "cd"]
            }))
            .expect("create without cwd");
        #[cfg(not(windows))]
        let create = host
            .terminal_create(&json!({
                "command": "pwd",
                "args": []
            }))
            .expect("create without cwd");

        let tid = create
            .get("terminalId")
            .and_then(Value::as_str)
            .expect("tid")
            .to_string();
        let _ = host.terminal_wait_for_exit(&json!({ "terminalId": tid }));
        let _ = host.terminal_release(&json!({ "terminalId": tid }));
    }
}
