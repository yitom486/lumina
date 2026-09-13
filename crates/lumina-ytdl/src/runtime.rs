//! Detect available JavaScript runtimes for yt-dlp EJS challenge solving.
//!
//! yt-dlp requires an external JS runtime (Deno, Node, Bun, QuickJS) to solve
//! YouTube's dynamic n-sig/JS challenges, especially for authenticated/members-only requests.
//! yt-dlp only enables Deno by default; Node, Bun, and QuickJS must be explicitly enabled
//! via `--js-runtimes <runtime>[:<path>]`.

use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsRuntime {
    pub name: &'static str,
    pub path: Option<PathBuf>,
}

/// Discovered JS runtimes in order of yt-dlp preference: Deno -> Node -> QuickJS -> Bun.
pub fn available_js_runtimes() -> Vec<JsRuntime> {
    let mut runtimes = Vec::new();

    // 1. Deno (yt-dlp default preference)
    if let Some(path) = find_executable("deno", &[]) {
        runtimes.push(JsRuntime {
            name: "deno",
            path: Some(path),
        });
    }

    // 2. Node.js (widely installed, rock-solid)
    let node_candidates = [
        #[cfg(windows)]
        r"C:\Program Files\nodejs\node.exe",
        #[cfg(windows)]
        r"C:\Program Files (x86)\nodejs\node.exe",
    ];
    if let Some(path) = find_executable("node", &node_candidates) {
        runtimes.push(JsRuntime {
            name: "node",
            path: Some(path),
        });
    }

    // 3. QuickJS
    if let Some(path) = find_executable("quickjs", &[]) {
        runtimes.push(JsRuntime {
            name: "quickjs",
            path: Some(path),
        });
    }

    // 4. Bun (often used in dev)
    let bun_candidates = [
        #[cfg(windows)]
        r"%USERPROFILE%\.bun\bin\bun.exe",
    ];
    if let Some(path) = find_executable("bun", &bun_candidates) {
        runtimes.push(JsRuntime {
            name: "bun",
            path: Some(path),
        });
    }

    runtimes
}

/// Primary JS runtime name (if any), suitable for mpv `ytdl-raw-options`.
pub fn primary_js_runtime_name() -> Option<&'static str> {
    available_js_runtimes().first().map(|r| r.name)
}

/// Append `--js-runtimes` flags and ensure the runtime directory is in PATH.
pub fn apply_to_command(cmd: &mut Command) {
    let runtimes = available_js_runtimes();
    if runtimes.is_empty() {
        return;
    }

    for runtime in &runtimes {
        cmd.arg("--js-runtimes");
        cmd.arg(runtime.name);
    }

    // Ensure the runtime directory is present in PATH for child processes
    if let Some(first_path) = runtimes.iter().find_map(|r| r.path.as_ref()) {
        if let Some(parent) = first_path.parent() {
            let current_path = std::env::var_os("PATH").unwrap_or_default();
            let mut paths = std::env::split_paths(&current_path).collect::<Vec<_>>();
            if !paths.iter().any(|p| p == parent) {
                paths.insert(0, parent.to_path_buf());
                if let Ok(new_path) = std::env::join_paths(paths) {
                    cmd.env("PATH", new_path);
                }
            }
        }
    }
}

fn find_executable(name: &str, known_paths: &[&str]) -> Option<PathBuf> {
    if let Ok(p) = which::which(name) {
        if p.is_file() {
            return Some(p);
        }
    }
    for candidate in known_paths {
        let expanded = expand_env_path(candidate);
        let p = PathBuf::from(expanded);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn expand_env_path(path: &str) -> String {
    #[cfg(windows)]
    {
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            return path.replace("%USERPROFILE%", &userprofile);
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtimes_have_supported_names() {
        let supported = ["deno", "node", "quickjs", "bun"];
        for r in available_js_runtimes() {
            assert!(
                supported.contains(&r.name),
                "unsupported runtime: {}",
                r.name
            );
        }
    }

    #[test]
    fn primary_runtime_matches_first() {
        let all = available_js_runtimes();
        assert_eq!(primary_js_runtime_name(), all.first().map(|r| r.name));
    }
}
