//! Local filesystem + terminal host for ACP Agent→Client requests.

mod fs;
mod terminal;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::wire::codec::{error_response, success_response};
use crate::wire::permission::permission_auto_result;

#[derive(Default)]
pub struct AcpHost {
    terminals: Mutex<HashMap<String, terminal::ManagedTerminal>>,
    /// Absolute session workspace from `session/new` cwd.
    workspace: Mutex<Option<PathBuf>>,
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
            "session/request_permission" => {
                success_response(id, permission_auto_result(params, canceling))
            }
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
}
