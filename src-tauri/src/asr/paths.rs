//! Resolve optional on-demand whisper-cli + model (never loaded at startup).

use std::path::{Path, PathBuf};

use crate::asr::error::AsrError;
use crate::asr::model::AsrStatus;

pub struct AsrPaths {
    pub cli: PathBuf,
    pub model: PathBuf,
}

pub fn resolve_asr_paths() -> Result<AsrPaths, AsrError> {
    let root = whisper_root();
    let cli = find_cli(&root).ok_or_else(|| {
        AsrError::not_configured(Some(
            "missing whisper-cli.exe under src-tauri/native/whisper/",
        ))
    })?;
    let model = find_model(&root).ok_or_else(|| {
        AsrError::not_configured(Some(
            "missing ggml-*.bin model under native/whisper/ or native/whisper/models/",
        ))
    })?;
    Ok(AsrPaths { cli, model })
}

pub fn status() -> AsrStatus {
    match resolve_asr_paths() {
        Ok(paths) => AsrStatus {
            available: true,
            cli_path: Some(paths.cli.to_string_lossy().to_string()),
            model_path: Some(paths.model.to_string_lossy().to_string()),
            message: "ASR ready (loaded only when you start a job)".into(),
        },
        Err(error) => AsrStatus {
            available: false,
            cli_path: None,
            model_path: None,
            message: error.message,
        },
    }
}

fn whisper_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("native")
        .join("whisper")
}

fn find_cli(root: &Path) -> Option<PathBuf> {
    let candidates = [
        root.join("whisper-cli.exe"),
        root.join("whisper-cli"),
        root.join("main.exe"),
        root.join("whisper.exe"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn find_model(root: &Path) -> Option<PathBuf> {
    let mut dirs = vec![root.to_path_buf(), root.join("models")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.join("whisper"));
            dirs.push(dir.join("whisper").join("models"));
        }
    }

    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut models: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.to_ascii_lowercase().contains("ggml"))
            })
            .collect();
        models.sort();
        // Prefer smaller base/tiny if present.
        if let Some(preferred) = models.iter().find(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.contains("base") || name.contains("tiny") || name.contains("small")
        }) {
            return Some(preferred.clone());
        }
        if let Some(first) = models.into_iter().next() {
            return Some(first);
        }
    }
    None
}
