//! Local filesystem + terminal host for ACP Agent→Client requests.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use crate::acp::error::AcpError;
use crate::acp::protocol::{error_response, success_response};
use crate::process_util::command;

static TERMINAL_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub struct AcpHost {
    terminals: Mutex<HashMap<String, ManagedTerminal>>,
    /// Absolute session workspace from `session/new` cwd.
    workspace: Mutex<Option<PathBuf>>,
}

struct ManagedTerminal {
    child: Child,
    output: Arc<Mutex<String>>,
    truncated: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
    exit_code: Arc<Mutex<Option<i32>>>,
    signal: Arc<Mutex<Option<String>>>,
}

impl AcpHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_workspace(&self, cwd: PathBuf) {
        if let Ok(mut guard) = self.workspace.lock() {
            *guard = Some(cwd);
        }
    }

    pub fn clear_workspace(&self) {
        if let Ok(mut guard) = self.workspace.lock() {
            *guard = None;
        }
    }

    pub fn handle_request(
        &self,
        method: &str,
        id: Value,
        params: &Value,
        canceling: bool,
    ) -> Value {
        if canceling
            && matches!(
                method,
                "fs/read_text_file"
                    | "fs/write_text_file"
                    | "terminal/create"
                    | "terminal/output"
                    | "terminal/wait_for_exit"
            )
        {
            return error_response(id, -32800, "request cancelled");
        }

        match method {
            "session/request_permission" => success_response(
                id,
                crate::acp::protocol::permission_auto_result(params, canceling),
            ),
            "fs/read_text_file" => match self.read_text_file(params) {
                Ok(result) => success_response(id, result),
                Err(error) => error_response(id, -32000, &error.message),
            },
            "fs/write_text_file" => match self.write_text_file(params) {
                Ok(()) => success_response(id, Value::Null),
                Err(error) => error_response(id, -32000, &error.message),
            },
            "terminal/create" => match self.terminal_create(params) {
                Ok(result) => success_response(id, result),
                Err(error) => error_response(id, -32000, &error.message),
            },
            "terminal/output" => match self.terminal_output(params) {
                Ok(result) => success_response(id, result),
                Err(error) => error_response(id, -32000, &error.message),
            },
            "terminal/wait_for_exit" => match self.terminal_wait_for_exit(params) {
                Ok(result) => success_response(id, result),
                Err(error) => error_response(id, -32000, &error.message),
            },
            "terminal/kill" => match self.terminal_kill(params) {
                Ok(result) => success_response(id, result),
                Err(error) => error_response(id, -32000, &error.message),
            },
            "terminal/release" => match self.terminal_release(params) {
                Ok(result) => success_response(id, result),
                Err(error) => error_response(id, -32000, &error.message),
            },
            "elicitation/create" => {
                success_response(id, json!({ "outcome": { "outcome": "cancelled" } }))
            }
            other => {
                tracing::warn!(method = other, "unsupported Agent→Client ACP method");
                error_response(id, -32601, &format!("Method not found: {other}"))
            }
        }
    }

    fn read_text_file(&self, params: &Value) -> Result<Value, AcpError> {
        let path = self.resolve_path(params, "path")?;
        let line = params
            .get("line")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1) as usize;
        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);

        let raw = fs::read_to_string(&path).map_err(|error| {
            tracing::warn!(path = %path.display(), %error, "ACP fs read failed");
            AcpError::new(
                crate::acp::AcpErrorCode::ProtocolError,
                "无法读取该文件",
                Some(error.to_string()),
            )
        })?;

        let content = if line <= 1 && limit.is_none() {
            raw
        } else {
            let lines: Vec<&str> = raw.lines().collect();
            let start = line.saturating_sub(1).min(lines.len());
            let end = match limit {
                Some(n) => (start + n).min(lines.len()),
                None => lines.len(),
            };
            lines[start..end].join("\n")
        };

        tracing::info!(path = %path.display(), bytes = content.len(), "ACP fs/read_text_file");
        Ok(json!({ "content": content }))
    }

    fn write_text_file(&self, params: &Value) -> Result<(), AcpError> {
        let path = self.resolve_path(params, "path")?;
        let content = params
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| AcpError::bad_request("写入内容缺失"))?;

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|error| {
                    tracing::warn!(path = %parent.display(), %error, "ACP fs mkdir failed");
                    AcpError::new(
                        crate::acp::AcpErrorCode::ProtocolError,
                        "无法创建文件目录",
                        Some(error.to_string()),
                    )
                })?;
            }
        }

        fs::write(&path, content).map_err(|error| {
            tracing::warn!(path = %path.display(), %error, "ACP fs write failed");
            AcpError::new(
                crate::acp::AcpErrorCode::ProtocolError,
                "无法写入该文件",
                Some(error.to_string()),
            )
        })?;

        tracing::info!(path = %path.display(), bytes = content.len(), "ACP fs/write_text_file");
        Ok(())
    }

    fn terminal_create(&self, params: &Value) -> Result<Value, AcpError> {
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

    fn terminal_output(&self, params: &Value) -> Result<Value, AcpError> {
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

    fn terminal_wait_for_exit(&self, params: &Value) -> Result<Value, AcpError> {
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

    fn terminal_kill(&self, params: &Value) -> Result<Value, AcpError> {
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

    fn terminal_release(&self, params: &Value) -> Result<Value, AcpError> {
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

    pub fn release_all_for_shutdown(&self) {
        if let Ok(mut map) = self.terminals.lock() {
            for (id, mut term) in map.drain() {
                let _ = term.child.kill();
                tracing::debug!(terminal_id = %id, "killed ACP terminal on app shutdown");
            }
        }
        self.clear_workspace();
    }

    pub fn release_all(&self) {
        if let Ok(mut map) = self.terminals.lock() {
            for (id, mut term) in map.drain() {
                let _ = term.child.kill();
                let _ = term.child.wait();
                tracing::debug!(terminal_id = %id, "released ACP terminal on session end");
            }
        }
        self.clear_workspace();
    }

    fn workspace_cwd(&self) -> Result<PathBuf, AcpError> {
        self.workspace
            .lock()
            .map_err(|_| AcpError::internal(Some("workspace lock poisoned")))?
            .clone()
            .ok_or_else(|| AcpError::protocol(Some("session workspace cwd missing")))
    }

    /// Absolute paths preferred; relative paths resolve against session workspace.
    fn resolve_path(&self, params: &Value, key: &str) -> Result<PathBuf, AcpError> {
        let raw = require_str(params, key)?;
        let path = PathBuf::from(raw);
        if path.is_absolute() {
            return Ok(path);
        }
        let base = self.workspace_cwd()?;
        Ok(base.join(path))
    }
}

fn require_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, AcpError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| AcpError::bad_request(format!("缺少参数 {key}")))
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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("lumina-acp-{nanos}-{name}"))
    }

    #[test]
    fn read_write_roundtrip() {
        let host = AcpHost::new();
        let path = temp_path("sample.txt");
        fs::write(&path, "a\nb\nc").expect("seed");

        let read = host
            .read_text_file(&json!({
                "path": path.to_string_lossy(),
                "line": 2,
                "limit": 1
            }))
            .expect("read");
        assert_eq!(read.get("content").and_then(Value::as_str), Some("b"));

        let path2 = temp_path("out.txt");
        host.write_text_file(&json!({
            "path": path2.to_string_lossy(),
            "content": "hello"
        }))
        .expect("write");
        assert_eq!(fs::read_to_string(&path2).expect("reread"), "hello");
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&path2);
    }

    #[test]
    fn relative_path_resolves_against_workspace() {
        let host = AcpHost::new();
        let dir = temp_path("ws");
        fs::create_dir_all(&dir).expect("mkdir");
        let file = dir.join("note.txt");
        fs::write(&file, "workspace-rel").expect("seed");
        host.set_workspace(dir.clone());

        let read = host
            .read_text_file(&json!({ "path": "note.txt" }))
            .expect("read relative");
        assert_eq!(
            read.get("content").and_then(Value::as_str),
            Some("workspace-rel")
        );
        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_relative_without_workspace() {
        let host = AcpHost::new();
        let err = host
            .read_text_file(&json!({ "path": "relative.txt" }))
            .expect_err("relative");
        assert_eq!(err.message, "与 Agent 通信失败");
    }

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
        let out = host
            .terminal_output(&json!({ "terminalId": tid }))
            .expect("output");
        let text = out.get("output").and_then(Value::as_str).unwrap_or("");
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
