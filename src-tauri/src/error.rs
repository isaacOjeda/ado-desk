use serde::{Serialize, Serializer};

/// Errors surfaced to the frontend. Messages never contain credentials.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    // The raw input is not echoed back: users sometimes paste the PAT into the wrong field.
    #[error("Invalid organization. Use its name (contoso) or URL (https://dev.azure.com/contoso)")]
    InvalidOrganization,
    #[error("The personal access token is invalid or expired")]
    Unauthorized,
    #[error("The token lacks the required scopes (Work Items, Code, Project and Team: Read; Build: Read for build status)")]
    Forbidden,
    #[error("Resource not found")]
    NotFound,
    #[error("Not signed in")]
    NotSignedIn,
    #[error("Azure DevOps returned HTTP {status}: {message}")]
    Api { status: u16, message: String },
    #[error("Network error: {0}")]
    Network(String),
    #[error("Unexpected response from Azure DevOps: {0}")]
    Decode(String),
    #[error("Credential store error: {0}")]
    Credentials(String),
    // Theme import (Open VSX). Messages are fixed text: the search or id typed is never echoed.
    #[error("Search text must be at most 100 characters, without line breaks")]
    InvalidThemeSearch,
    #[error("Invalid Open VSX extension identifier")]
    InvalidExtension,
    #[error("Open VSX returned HTTP {status}")]
    OpenVsx { status: u16 },
    #[error("{0}")]
    ThemeImport(&'static str),
}

impl AppError {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            AppError::InvalidOrganization => "invalidOrganization",
            AppError::Unauthorized => "unauthorized",
            AppError::Forbidden => "forbidden",
            AppError::NotFound => "notFound",
            AppError::NotSignedIn => "notSignedIn",
            AppError::Api { .. } => "api",
            AppError::Network(_) => "network",
            AppError::Decode(_) => "decode",
            AppError::Credentials(_) => "credentials",
            AppError::InvalidThemeSearch => "invalidThemeSearch",
            AppError::InvalidExtension => "invalidExtension",
            AppError::OpenVsx { .. } => "openVsx",
            AppError::ThemeImport(_) => "themeImport",
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        // `without_url` keeps query strings out of user-facing messages.
        let err = err.without_url();
        if err.is_decode() {
            AppError::Decode(err.to_string())
        } else {
            AppError::Network(err.to_string())
        }
    }
}

impl From<keyring::Error> for AppError {
    fn from(err: keyring::Error) -> Self {
        AppError::Credentials(err.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
