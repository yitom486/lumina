//! Resolve optional on-demand whisper-cli + model (never loaded at startup).

use std::path::{Path, PathBuf};

use crate::asr::error::AsrError;
use crate::asr::model::{AsrModelInfo, AsrStatus};

pub struct AsrPaths {
    pub cli: PathBuf,
    pub model: PathBuf,
}

pub fn resolve_asr_paths(model_id: Option<&str>) -> Result<AsrPaths, AsrError> {
    let root = whisper_root();
    let cli = find_cli(&root).ok_or_else(|| {
        AsrError::not_configured(Some(
            "missing whisper-cli.exe under src-tauri/native/whisper/",
        ))
    })?;
    let models = list_models();
    if models.is_empty() {
        return Err(AsrError::not_configured(Some(
            "missing ggml-*.bin model under native/whisper/ or native/whisper/models/",
        )));
    }
    let model = pick_model(&models, model_id)?;
    Ok(AsrPaths { cli, model })
}

pub fn status() -> AsrStatus {
    let root = whisper_root();
    let cli = find_cli(&root);
    let models = list_models();
    match (cli.as_ref(), models.is_empty()) {
        (Some(cli), false) => {
            let default = pick_model(&models, None).ok();
            AsrStatus {
                available: true,
                cli_path: Some(cli.to_string_lossy().to_string()),
                model_path: default
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                models,
                message: "语音转写已就绪（仅在你开始任务时加载）".into(),
            }
        }
        _ => AsrStatus {
            available: false,
            cli_path: cli.map(|p| p.to_string_lossy().to_string()),
            model_path: None,
            models,
            message: AsrError::not_configured(None).message,
        },
    }
}

pub fn list_models() -> Vec<AsrModelInfo> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in model_dirs() {
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_ggml_model(&path) {
                continue;
            }
            let id = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("model.bin")
                .to_string();
            if !seen.insert(id.clone()) {
                continue;
            }
            let size_bytes = std::fs::metadata(&path).ok().map(|m| m.len());
            out.push(AsrModelInfo {
                id,
                path: path.to_string_lossy().to_string(),
                size_bytes,
            });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn pick_model(models: &[AsrModelInfo], model_id: Option<&str>) -> Result<PathBuf, AsrError> {
    if let Some(id) = model_id.map(str::trim).filter(|s| !s.is_empty()) {
        return models
            .iter()
            .find(|m| m.id.eq_ignore_ascii_case(id) || m.path == id)
            .map(|m| PathBuf::from(&m.path))
            .ok_or_else(|| AsrError::invalid("找不到所选转写模型"));
    }
    preferred_model(models)
        .or_else(|| models.first().map(|m| PathBuf::from(&m.path)))
        .ok_or_else(|| {
            AsrError::not_configured(Some(
                "missing ggml-*.bin model under native/whisper/ or native/whisper/models/",
            ))
        })
}

fn preferred_model(models: &[AsrModelInfo]) -> Option<PathBuf> {
    models
        .iter()
        .find(|m| {
            let name = m.id.to_ascii_lowercase();
            name.contains("base") || name.contains("tiny") || name.contains("small")
        })
        .map(|m| PathBuf::from(&m.path))
}

fn whisper_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("native")
        .join("whisper")
}

fn model_dirs() -> Vec<PathBuf> {
    let root = whisper_root();
    let mut dirs = vec![root.clone(), root.join("models")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.join("whisper"));
            dirs.push(dir.join("whisper").join("models"));
        }
    }
    dirs
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

fn is_ggml_model(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.to_ascii_lowercase().contains("ggml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_model_rejects_unknown_id() {
        let models = vec![AsrModelInfo {
            id: "ggml-base.bin".into(),
            path: "/tmp/ggml-base.bin".into(),
            size_bytes: Some(1),
        }];
        let err = pick_model(&models, Some("missing.bin")).expect_err("unknown");
        assert_eq!(err.code, crate::asr::AsrErrorCode::InvalidRequest);
        assert!(err.message.contains("模型"));
    }

    #[test]
    fn pick_model_prefers_base() {
        let models = vec![
            AsrModelInfo {
                id: "ggml-large.bin".into(),
                path: "/tmp/ggml-large.bin".into(),
                size_bytes: None,
            },
            AsrModelInfo {
                id: "ggml-base.bin".into(),
                path: "/tmp/ggml-base.bin".into(),
                size_bytes: None,
            },
        ];
        let path = pick_model(&models, None).expect("pick");
        assert!(path.ends_with("ggml-base.bin"));
    }
}
