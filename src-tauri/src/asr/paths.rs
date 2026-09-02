//! Resolve optional on-demand whisper-cli + model (never loaded at startup).

use std::path::{Path, PathBuf};

use crate::asr::error::AsrError;
use crate::asr::model::{AsrCatalogModel, AsrModelInfo, AsrStatus};

pub struct AsrPaths {
    pub cli: PathBuf,
    pub model: PathBuf,
}

pub fn resolve_asr_paths(model_id: Option<&str>) -> Result<AsrPaths, AsrError> {
    let cli = find_cli_any().ok_or_else(|| {
        AsrError::not_configured(Some(
            "missing whisper-cli under native/whisper/ or app data whisper/",
        ))
    })?;
    let models = list_models();
    if models.is_empty() {
        return Err(AsrError::not_configured(Some(
            "missing ggml-*.bin model under whisper roots",
        )));
    }
    let model = pick_model(&models, model_id)?;
    Ok(AsrPaths { cli, model })
}

pub fn status() -> AsrStatus {
    let cli = find_cli_any();
    let models = list_models();
    let catalog = catalog_models(&models);
    let cli_ready = cli.is_some();
    let install_supported = install_supported();
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
                catalog,
                cli_ready,
                install_supported,
                message: "语音转写已就绪（仅在你开始任务时加载）".into(),
            }
        }
        _ => {
            let message = if !install_supported {
                AsrError::not_configured(None).message
            } else if !cli_ready && models.is_empty() {
                "尚未安装语音转写组件，可一键下载".into()
            } else if !cli_ready {
                "已有模型，但仍需下载转写引擎".into()
            } else {
                "已有引擎，请再下载一个转写模型".into()
            };
            AsrStatus {
                available: false,
                cli_path: cli.map(|p| p.to_string_lossy().to_string()),
                model_path: None,
                models,
                catalog,
                cli_ready,
                install_supported,
                message,
            }
        }
    }
}

pub fn catalog_models(installed: &[AsrModelInfo]) -> Vec<AsrCatalogModel> {
    catalog_defs()
        .into_iter()
        .map(|def| {
            let installed = installed.iter().any(|m| {
                m.id.eq_ignore_ascii_case(def.file_name)
                    || m.id
                        .to_ascii_lowercase()
                        .contains(&format!("ggml-{}", def.id))
            });
            AsrCatalogModel {
                id: def.id.to_string(),
                file_name: def.file_name.to_string(),
                label: def.label.to_string(),
                approx_bytes: def.approx_bytes,
                installed,
            }
        })
        .collect()
}

pub struct CatalogDef {
    pub id: &'static str,
    pub file_name: &'static str,
    pub label: &'static str,
    pub approx_bytes: u64,
}

pub fn catalog_defs() -> Vec<CatalogDef> {
    vec![
        CatalogDef {
            id: "tiny",
            file_name: "ggml-tiny.bin",
            label: "Tiny（快 · 约 75 MB）",
            approx_bytes: 75_000_000,
        },
        CatalogDef {
            id: "base",
            file_name: "ggml-base.bin",
            label: "Base（推荐 · 约 142 MB）",
            approx_bytes: 142_000_000,
        },
        CatalogDef {
            id: "small",
            file_name: "ggml-small.bin",
            label: "Small（更准 · 约 466 MB）",
            approx_bytes: 466_000_000,
        },
    ]
}

pub fn resolve_catalog_model(model_id: &str) -> Result<&'static CatalogDef, AsrError> {
    let key = model_id.trim().to_ascii_lowercase();
    let key = key
        .strip_prefix("ggml-")
        .unwrap_or(&key)
        .strip_suffix(".bin")
        .unwrap_or(&key);
    // Leak-free: return from static table via matching index
    for def in CATALOG {
        if def.id == key || def.file_name.eq_ignore_ascii_case(model_id.trim()) {
            return Ok(def);
        }
    }
    Err(AsrError::invalid("不支持的转写模型，请选择 Tiny / Base / Small"))
}

const CATALOG: &[CatalogDef] = &[
    CatalogDef {
        id: "tiny",
        file_name: "ggml-tiny.bin",
        label: "Tiny（快 · 约 75 MB）",
        approx_bytes: 75_000_000,
    },
    CatalogDef {
        id: "base",
        file_name: "ggml-base.bin",
        label: "Base（推荐 · 约 142 MB）",
        approx_bytes: 142_000_000,
    },
    CatalogDef {
        id: "small",
        file_name: "ggml-small.bin",
        label: "Small（更准 · 约 466 MB）",
        approx_bytes: 466_000_000,
    },
];

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
        let key = id.to_ascii_lowercase();
        let key_stem = key
            .strip_prefix("ggml-")
            .unwrap_or(&key)
            .strip_suffix(".bin")
            .unwrap_or(&key);
        return models
            .iter()
            .find(|m| {
                m.id.eq_ignore_ascii_case(id)
                    || m.path == id
                    || m.id.to_ascii_lowercase().contains(&format!("ggml-{key_stem}"))
            })
            .map(|m| PathBuf::from(&m.path))
            .ok_or_else(|| AsrError::invalid("找不到所选转写模型"));
    }
    preferred_model(models)
        .or_else(|| models.first().map(|m| PathBuf::from(&m.path)))
        .ok_or_else(|| {
            AsrError::not_configured(Some("missing ggml-*.bin model under whisper roots"))
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

/// Writable install root: `{APPDATA|…}/lumina/whisper`.
pub fn install_root() -> PathBuf {
    if let Some(dir) = dirs_data() {
        return dir.join("lumina").join("whisper");
    }
    std::env::temp_dir().join("lumina-whisper")
}

pub fn models_install_dir() -> PathBuf {
    install_root().join("models")
}

pub fn install_supported() -> bool {
    cfg!(windows)
}

fn dirs_data() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
            })
    }
}

fn whisper_dev_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("native")
        .join("whisper")
}

fn search_roots() -> Vec<PathBuf> {
    let mut roots = vec![install_root(), whisper_dev_root()];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join("whisper"));
        }
    }
    roots
}

fn model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root in search_roots() {
        dirs.push(root.clone());
        dirs.push(root.join("models"));
    }
    dirs
}

pub fn find_cli_any() -> Option<PathBuf> {
    for root in search_roots() {
        if let Some(cli) = find_cli_in(&root) {
            return Some(cli);
        }
    }
    None
}

fn find_cli_in(root: &Path) -> Option<PathBuf> {
    let candidates = [
        root.join("whisper-cli.exe"),
        root.join("whisper-cli"),
        root.join("main.exe"),
        root.join("whisper.exe"),
        root.join("Release").join("whisper-cli.exe"),
        root.join("Release").join("main.exe"),
        root.join("bin").join("whisper-cli.exe"),
        root.join("bin").join("whisper-cli"),
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

    #[test]
    fn resolve_catalog_accepts_aliases() {
        assert_eq!(resolve_catalog_model("base").unwrap().file_name, "ggml-base.bin");
        assert_eq!(
            resolve_catalog_model("ggml-tiny.bin").unwrap().id,
            "tiny"
        );
    }
}
