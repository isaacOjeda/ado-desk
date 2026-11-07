//! Persists the Azure DevOps credentials in the OS credential store
//! (macOS Keychain, Windows Credential Manager, Secret Service on Linux).

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const SERVICE: &str = "com.isaacojeda.adodesk";
const ACCOUNT: &str = "azure-devops";

#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub organization: String,
    pub pat: String,
}

// Manual impl so the token can never leak through `{:?}` formatting.
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("organization", &self.organization)
            .field("pat", &"<redacted>")
            .finish()
    }
}

fn entry() -> AppResult<keyring::Entry> {
    Ok(keyring::Entry::new(SERVICE, ACCOUNT)?)
}

pub fn save(credentials: &Credentials) -> AppResult<()> {
    let payload =
        serde_json::to_string(credentials).map_err(|e| AppError::Credentials(e.to_string()))?;
    entry()?.set_password(&payload)?;
    Ok(())
}

pub fn load() -> AppResult<Option<Credentials>> {
    match entry()?.get_password() {
        Ok(payload) => serde_json::from_str(&payload)
            .map(Some)
            .map_err(|_| AppError::Credentials("stored credentials are corrupted".into())),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

pub fn delete() -> AppResult<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_redacts_the_token() {
        let credentials = Credentials {
            organization: "contoso".into(),
            pat: "super-secret-token".into(),
        };

        let rendered = format!("{credentials:?}");

        assert!(rendered.contains("contoso"));
        assert!(!rendered.contains("super-secret-token"));
    }
}
