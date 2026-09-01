//! Secure persistence for optional media-metadata credentials.
//!
//! Secret values are kept out of `.lumina`, WebView storage, logs, and normal
//! application settings. On Windows they live in the current user's Windows
//! Credential Manager. Environment variables remain a non-persistent fallback
//! for development and CI.

use serde::{Deserialize, Serialize};

use crate::library::error::LibraryError;

const SERVICE_NAME: &str = "com.lumina.desktop.media-metadata";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CredentialKind {
    ModelApiKey,
    TmdbAccessToken,
}

impl CredentialKind {
    fn account_name(self) -> &'static str {
        match self {
            Self::ModelApiKey => "model-api-key",
            Self::TmdbAccessToken => "tmdb-access-token",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub model_api_key_saved: bool,
    pub tmdb_access_token_saved: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialSaveInput {
    pub model_api_key: Option<String>,
    pub tmdb_access_token: Option<String>,
}

pub fn status() -> Result<CredentialStatus, LibraryError> {
    Ok(CredentialStatus {
        model_api_key_saved: read(CredentialKind::ModelApiKey)?.is_some(),
        tmdb_access_token_saved: read(CredentialKind::TmdbAccessToken)?.is_some(),
    })
}

pub fn save(input: CredentialSaveInput) -> Result<CredentialStatus, LibraryError> {
    let mut saved_any = false;
    if let Some(value) = input.model_api_key {
        save_one(CredentialKind::ModelApiKey, value, "模型密钥不能为空")?;
        saved_any = true;
    }
    if let Some(value) = input.tmdb_access_token {
        save_one(
            CredentialKind::TmdbAccessToken,
            value,
            "TMDb Token 不能为空",
        )?;
        saved_any = true;
    }
    if !saved_any {
        return Err(LibraryError::invalid_input("请至少输入一项密钥"));
    }
    status()
}

pub fn delete(kind: CredentialKind) -> Result<CredentialStatus, LibraryError> {
    delete_one(kind)?;
    status()
}

/// Return a secret only to backend code that is about to make the configured
/// request. It must never cross the Tauri command boundary back to the UI.
pub(crate) fn read(kind: CredentialKind) -> Result<Option<String>, LibraryError> {
    platform::read(kind)
}

fn save_one(
    kind: CredentialKind,
    value: String,
    empty_message: &'static str,
) -> Result<(), LibraryError> {
    if value.trim().is_empty() {
        return Err(LibraryError::invalid_input(empty_message));
    }
    platform::save(kind, &value)
}

fn delete_one(kind: CredentialKind) -> Result<(), LibraryError> {
    platform::delete(kind)
}

#[cfg(windows)]
mod platform {
    use keyring::{Entry, Error as KeyringError};

    use super::{CredentialKind, LibraryError, SERVICE_NAME};

    fn entry(kind: CredentialKind) -> Result<Entry, LibraryError> {
        Entry::new(SERVICE_NAME, kind.account_name()).map_err(|error| {
            LibraryError::credential_access_failed(Some(&format!(
                "credential entry initialization: {error}"
            )))
        })
    }

    pub(super) fn read(kind: CredentialKind) -> Result<Option<String>, LibraryError> {
        match entry(kind)?.get_password() {
            Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
            Ok(_) | Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(LibraryError::credential_access_failed(Some(&format!(
                "credential read: {error}"
            )))),
        }
    }

    pub(super) fn save(kind: CredentialKind, value: &str) -> Result<(), LibraryError> {
        entry(kind)?.set_password(value).map_err(|error| {
            LibraryError::credential_access_failed(Some(&format!("credential write: {error}")))
        })
    }

    pub(super) fn delete(kind: CredentialKind) -> Result<(), LibraryError> {
        match entry(kind)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(LibraryError::credential_access_failed(Some(&format!(
                "credential delete: {error}"
            )))),
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{CredentialKind, LibraryError};

    pub(super) fn read(_kind: CredentialKind) -> Result<Option<String>, LibraryError> {
        Ok(None)
    }

    pub(super) fn save(_kind: CredentialKind, _value: &str) -> Result<(), LibraryError> {
        Err(LibraryError::credential_access_failed(None))
    }

    pub(super) fn delete(_kind: CredentialKind) -> Result<(), LibraryError> {
        Err(LibraryError::credential_access_failed(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_account_names_are_stable() {
        assert_eq!(CredentialKind::ModelApiKey.account_name(), "model-api-key");
        assert_eq!(
            CredentialKind::TmdbAccessToken.account_name(),
            "tmdb-access-token"
        );
    }

    #[test]
    fn saving_no_value_is_rejected_before_touching_the_store() {
        let error = save(CredentialSaveInput {
            model_api_key: None,
            tmdb_access_token: None,
        })
        .expect_err("missing input must be rejected");
        assert_eq!(error.message, "请至少输入一项密钥");
    }
}
