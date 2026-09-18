//! Google Antigravity ACP authentication runner.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::agent::discover::find_antigravity;
use crate::error::AcpError;

pub fn login_antigravity(proxy_port: Option<u16>) -> Result<String, AcpError> {
    let program = find_antigravity().ok_or_else(|| {
        AcpError::not_configured(Some(
            "未找到 Google Antigravity ACP 服务端程序（agy_acp_server.exe），请确认已安装或指定路径",
        ))
    })?;

    let port = proxy_port.unwrap_or(7897);
    let http_proxy = format!("http://127.0.0.1:{port}");
    let socks_proxy = format!("socks5://127.0.0.1:{port}");

    let mut cmd = Command::new(&program);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Clean PyInstaller vars to avoid security validation exit
    for (k, _) in std::env::vars() {
        if k.starts_with("_PYI") || k.starts_with("_MEI") {
            cmd.env_remove(&k);
        }
    }

    cmd.env("HTTP_PROXY", &http_proxy)
        .env("HTTPS_PROXY", &http_proxy)
        .env("http_proxy", &http_proxy)
        .env("https_proxy", &http_proxy)
        .env("ALL_PROXY", &socks_proxy)
        .env("all_proxy", &socks_proxy)
        .env("NO_PROXY", "localhost,127.0.0.1,::1")
        .env("no_proxy", "localhost,127.0.0.1,::1");

    let mut child = cmd
        .spawn()
        .map_err(|e| AcpError::spawn_failed(Some(&format!("启动 agy_acp_server 失败: {e}"))))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AcpError::spawn_failed(Some("stdout missing")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AcpError::spawn_failed(Some("stdin missing")))?;
    let stderr = child.stderr.take();

    let oauth_url_found = Arc::new(AtomicBool::new(false));
    let oauth_url_found_clone = oauth_url_found.clone();

    // Listen on stderr for OAuth link and trigger browser
    if let Some(err) = stderr {
        thread::spawn(move || {
            let mut reader = BufReader::new(err);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line) {
                if n == 0 {
                    break;
                }
                let trimmed = line.trim();
                if trimmed.contains("https://accounts.google.com/o/oauth2/") {
                    if let Some(start) = trimmed.find("https://accounts.google.com/") {
                        let url = trimmed[start..]
                            .split_whitespace()
                            .next()
                            .unwrap_or(&trimmed[start..]);
                        oauth_url_found_clone.store(true, Ordering::SeqCst);
                        tracing::info!(oauth_url = url, "Opening Google OAuth authorization page");
                        #[cfg(windows)]
                        {
                            let _ = Command::new("rundll32")
                                .args(["url.dll,FileProtocolHandler", url])
                                .spawn();
                        }
                        #[cfg(target_os = "macos")]
                        {
                            let _ = Command::new("open").arg(url).spawn();
                        }
                        #[cfg(target_os = "linux")]
                        {
                            let _ = Command::new("xdg-open").arg(url).spawn();
                        }
                    }
                }
                line.clear();
            }
        });
    }

    // Step 1: Send initialize
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": {
                    "readTextFile": true,
                    "writeTextFile": true
                }
            },
            "clientInfo": {
                "name": "lumina",
                "title": "Lumina",
                "version": "0.1.0"
            }
        }
    });
    writeln!(stdin, "{}", init_req)
        .map_err(|e| AcpError::protocol(Some(&format!("发送 initialize 失败: {e}"))))?;
    stdin.flush().ok();

    // Read initialize response
    let (tx, rx) = std::sync::mpsc::channel();
    let stdout_reader = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while let Ok(n) = reader.read_line(&mut line) {
            if n == 0 {
                break;
            }
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if tx.send(val).is_err() {
                        break;
                    }
                }
            }
            line.clear();
        }
    });

    let init_resp = rx.recv_timeout(Duration::from_secs(15)).map_err(|_| {
        let _ = child.kill();
        AcpError::protocol(Some("agy_acp_server 初始化响应超时"))
    })?;

    if let Some(err) = init_resp.get("error") {
        let _ = child.kill();
        return Err(AcpError::protocol(Some(&format!("initialize 错误: {err}"))));
    }

    // Step 2: Send authenticate with oauth-personal
    let auth_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "authenticate",
        "params": {
            "methodId": "oauth-personal"
        }
    });
    writeln!(stdin, "{}", auth_req)
        .map_err(|e| AcpError::protocol(Some(&format!("发送 authenticate 失败: {e}"))))?;
    stdin.flush().ok();

    // Step 3: Wait for authenticate response (give up to 180s for browser auth if needed)
    let auth_resp = rx.recv_timeout(Duration::from_secs(180)).map_err(|_| {
        let _ = child.kill();
        AcpError::protocol(Some("Google 账号登录授权等待超时"))
    })?;

    let _ = child.kill();
    let _ = stdout_reader.join();

    if let Some(err) = auth_resp.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("未知错误");
        return Err(AcpError::protocol(Some(&format!(
            "Google 授权失败：{msg}（请确保代理端口 {port} 连接正常）"
        ))));
    }

    Ok("Google 账号授权成功！凭据已就绪。".to_string())
}
