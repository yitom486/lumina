//! Workshop Codex rollout cleanup: only tracked session_ids.
//!
//! Pure move from `jobs/pool.rs` (no behavior change). We never infer targets
//! by date, filename, cwd, or prefix, so historical and interactive Codex
//! conversations are untouched.

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::agent::discover::codex_home_dir;

/// Delete only rollout files whose persisted `session_meta.session_id` was
/// created by this pool. We never infer targets by date, filename, cwd, or
/// prefix, so historical and interactive Codex conversations are untouched.
pub(crate) fn cleanup_codex_rollouts(session_ids: &BTreeSet<String>, job_id: &str) {
    if session_ids.is_empty() {
        return;
    }
    let Some(home) = codex_home_dir() else {
        return;
    };
    let root = home.join("sessions");
    cleanup_rollouts_in_root(session_ids, job_id, &root);
}

fn cleanup_rollouts_in_root(session_ids: &BTreeSet<String>, job_id: &str, root: &Path) {
    let mut candidates = Vec::new();
    collect_rollout_files(root, &mut candidates);
    for path in candidates {
        let Some(session_id) = rollout_session_id(&path) else {
            continue;
        };
        if !session_ids.contains(&session_id) {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => {
                tracing::info!(job_id, session_id, path = %path.display(), "removed workshop Codex rollout")
            }
            Err(error) => {
                tracing::warn!(job_id, session_id, path = %path.display(), %error, "failed to remove workshop Codex rollout")
            }
        }
    }
}

fn collect_rollout_files(root: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rollout_files(&path, out);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
        {
            out.push(path);
        }
    }
}

fn rollout_session_id(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let first_line = BufReader::new(file).lines().next()?.ok()?;
    let value: serde_json::Value = serde_json::from_str(&first_line).ok()?;
    value
        .get("payload")?
        .get("session_id")?
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn cleanup_removes_only_tracked_rollouts() {
        let root =
            std::env::temp_dir().join(format!("lumina-workshop-cleanup-{}", std::process::id()));
        let nested = root.join("2026").join("09").join("15");
        fs::create_dir_all(&nested).expect("create root");
        let owned = nested.join("rollout-owned.jsonl");
        let historical = nested.join("rollout-history.jsonl");
        fs::write(
            &owned,
            r#"{"type":"session_meta","payload":{"session_id":"owned"}}"#,
        )
        .expect("owned rollout");
        fs::write(
            &historical,
            r#"{"type":"session_meta","payload":{"session_id":"history"}}"#,
        )
        .expect("history rollout");
        let session_ids = BTreeSet::from(["owned".to_string()]);

        cleanup_rollouts_in_root(&session_ids, "job", &root);

        assert!(!owned.exists());
        assert!(historical.exists());
        fs::remove_file(historical).expect("cleanup history fixture");
        fs::remove_dir_all(root).expect("cleanup root");
    }
}
