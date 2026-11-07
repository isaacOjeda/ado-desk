//! Persists the Azure DevOps credentials in the OS credential store
//! (macOS Keychain, Windows Credential Manager, Secret Service on Linux).
//!
//! Layout (service `com.isaacojeda.adodesk`):
//! - `org:{name}`: the PAT of one organization, nothing else.
//! - `organizations`: a non-secret index `{ active, organizations }` with lowercase names.
//! - `azure-devops`: the legacy single-organization entry, migrated on first read.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const SERVICE: &str = "com.isaacojeda.adodesk";
const LEGACY_ACCOUNT: &str = "azure-devops";
const INDEX_ACCOUNT: &str = "organizations";

fn pat_account(organization: &str) -> String {
    format!("org:{organization}")
}

/// Canonical form of an organization name: Azure DevOps names are case-insensitive, so
/// the index, the keychain account and the session all use the lowercase form.
pub fn canonical(organization: &str) -> String {
    organization.to_ascii_lowercase()
}

/// Port over the OS credential store so the organization rules can be tested in memory.
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> AppResult<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> AppResult<()>;
    /// Deleting a missing entry succeeds.
    fn delete(&self, account: &str) -> AppResult<()>;
}

/// Production store backed by the `keyring` crate.
pub struct KeyringStore;

impl KeyringStore {
    fn entry(account: &str) -> AppResult<keyring::Entry> {
        Ok(keyring::Entry::new(SERVICE, account)?)
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, account: &str) -> AppResult<Option<String>> {
        match Self::entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn set(&self, account: &str, secret: &str) -> AppResult<()> {
        Self::entry(account)?.set_password(secret)?;
        Ok(())
    }

    fn delete(&self, account: &str) -> AppResult<()> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}

/// Shape of the legacy single-organization entry (`azure-devops`).
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

/// Non-secret list of configured organizations and the active one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrganizationIndex {
    pub active: Option<String>,
    pub organizations: Vec<String>,
}

impl OrganizationIndex {
    /// Lowercases and deduplicates names (keeping the first occurrence) and keeps `active`
    /// pointing at a listed organization.
    fn normalized(self) -> Self {
        let mut organizations: Vec<String> = Vec::with_capacity(self.organizations.len());
        for name in self.organizations.iter().map(|o| canonical(o)) {
            if !name.is_empty() && !organizations.contains(&name) {
                organizations.push(name);
            }
        }
        let active = self
            .active
            .map(|a| canonical(&a))
            .filter(|a| organizations.contains(a))
            .or_else(|| organizations.first().cloned());
        Self { active, organizations }
    }
}

/// Organization rules over a `SecretStore`. Read-modify-write sequences are serialized so
/// two windows changing organizations at once cannot drop each other's update.
pub struct OrganizationStore {
    secrets: Box<dyn SecretStore>,
    lock: Mutex<()>,
}

// Manual impl: the secret store may hold tokens, so it is never formatted.
impl std::fmt::Debug for OrganizationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrganizationStore").finish_non_exhaustive()
    }
}

impl Default for OrganizationStore {
    fn default() -> Self {
        Self::new(Box::new(KeyringStore))
    }
}

impl OrganizationStore {
    pub fn new(secrets: Box<dyn SecretStore>) -> Self {
        Self { secrets, lock: Mutex::new(()) }
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The current index, migrating the legacy entry first when there is no index yet.
    pub fn index(&self) -> AppResult<OrganizationIndex> {
        let _guard = self.guard();
        self.load_index()
    }

    /// PAT stored for `organization`, if any. Never sent to the webview.
    pub fn pat(&self, organization: &str) -> AppResult<Option<String>> {
        self.secrets.get(&pat_account(&canonical(organization)))
    }

    /// Stores (or replaces) the PAT of `organization` and makes it the active one.
    /// The caller validates the token before calling this.
    pub fn add(&self, organization: &str, pat: &str) -> AppResult<OrganizationIndex> {
        let _guard = self.guard();
        let organization = canonical(organization);
        let mut index = self.load_index()?;
        self.secrets.set(&pat_account(&organization), pat)?;
        if !index.organizations.contains(&organization) {
            index.organizations.push(organization.clone());
        }
        index.active = Some(organization);
        self.save_index(&index)?;
        Ok(index)
    }

    /// Marks an already configured organization as active.
    pub fn set_active(&self, organization: &str) -> AppResult<OrganizationIndex> {
        let _guard = self.guard();
        let organization = canonical(organization);
        let mut index = self.load_index()?;
        if !index.organizations.contains(&organization) {
            return Err(not_configured());
        }
        if index.active.as_deref() != Some(organization.as_str()) {
            index.active = Some(organization);
            self.save_index(&index)?;
        }
        Ok(index)
    }

    /// Deletes the PAT and index entry of `organization`. When it was active, the next
    /// remaining organization (in list order) becomes active, or none when it was the last.
    pub fn remove(&self, organization: &str) -> AppResult<OrganizationIndex> {
        let _guard = self.guard();
        let organization = canonical(organization);
        let mut index = self.load_index()?;
        self.secrets.delete(&pat_account(&organization))?;
        self.delete_legacy_of(&organization);

        if let Some(position) = index.organizations.iter().position(|o| *o == organization) {
            index.organizations.remove(position);
            if index.active.as_deref() == Some(organization.as_str()) {
                let next = position.min(index.organizations.len().saturating_sub(1));
                index.active = index.organizations.get(next).cloned();
            }
        }
        self.save_index(&index)?;
        Ok(index)
    }

    fn load_index(&self) -> AppResult<OrganizationIndex> {
        match self.secrets.get(INDEX_ACCOUNT)? {
            // A corrupt index reads as empty; the secrets are left alone so nothing is lost.
            Some(raw) => Ok(serde_json::from_str::<OrganizationIndex>(&raw)
                .map(OrganizationIndex::normalized)
                .unwrap_or_default()),
            None => self.migrate_legacy(),
        }
    }

    fn save_index(&self, index: &OrganizationIndex) -> AppResult<()> {
        let payload = serde_json::to_string(index).map_err(|e| AppError::Credentials(e.to_string()))?;
        self.secrets.set(INDEX_ACCOUNT, &payload)
    }

    /// Moves the legacy single entry to the per-organization layout. The new entries are
    /// written and read back before the legacy one is deleted; on any failure the legacy
    /// entry stays untouched (and a half-written index is rolled back) so the next launch
    /// retries and the user never loses their sign-in.
    fn migrate_legacy(&self) -> AppResult<OrganizationIndex> {
        let Some(raw) = self.secrets.get(LEGACY_ACCOUNT)? else {
            return Ok(OrganizationIndex::default());
        };
        // An unreadable legacy entry cannot be migrated; it is left in place and the user
        // simply signs in again (which writes the new layout).
        let Ok(legacy) = serde_json::from_str::<Credentials>(&raw) else {
            return Ok(OrganizationIndex::default());
        };
        let organization = canonical(legacy.organization.trim());
        if organization.is_empty() {
            return Ok(OrganizationIndex::default());
        }
        let index = OrganizationIndex {
            active: Some(organization.clone()),
            organizations: vec![organization.clone()],
        };

        let account = pat_account(&organization);
        let written = self
            .secrets
            .set(&account, &legacy.pat)
            .and_then(|()| self.save_index(&index))
            .and_then(|()| self.verify_migration(&account, &legacy.pat, &index));
        if let Err(err) = written {
            // Best effort: without an index the migration runs again next time.
            let _ = self.secrets.delete(INDEX_ACCOUNT);
            return Err(err);
        }

        // The data is safe in the new layout; a failed delete only leaves a stale copy that
        // `remove` cleans up later.
        let _ = self.secrets.delete(LEGACY_ACCOUNT);
        Ok(index)
    }

    fn verify_migration(&self, account: &str, pat: &str, index: &OrganizationIndex) -> AppResult<()> {
        let stored_pat = self.secrets.get(account)?;
        let stored_index = self
            .secrets
            .get(INDEX_ACCOUNT)?
            .and_then(|raw| serde_json::from_str::<OrganizationIndex>(&raw).ok());
        if stored_pat.as_deref() == Some(pat) && stored_index.as_ref() == Some(index) {
            Ok(())
        } else {
            Err(AppError::Credentials("could not migrate the saved sign-in".into()))
        }
    }

    /// Deletes a leftover legacy entry belonging to `organization` (best effort).
    fn delete_legacy_of(&self, organization: &str) {
        let belongs = self
            .secrets
            .get(LEGACY_ACCOUNT)
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str::<Credentials>(&raw).ok())
            .is_some_and(|legacy| canonical(legacy.organization.trim()) == organization);
        if belongs {
            let _ = self.secrets.delete(LEGACY_ACCOUNT);
        }
    }
}

fn not_configured() -> AppError {
    AppError::Credentials("this organization is not configured".into())
}

#[cfg(test)]
pub(crate) mod memory {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use super::SecretStore;
    use crate::error::{AppError, AppResult};

    /// In-memory `SecretStore` for tests. Clones share the same entries.
    #[derive(Clone, Default)]
    pub struct MemoryStore {
        pub entries: Arc<Mutex<HashMap<String, String>>>,
        /// Accounts whose `set` fails.
        pub failing_sets: Arc<Mutex<Vec<String>>>,
        /// Every operation in order, e.g. `set:organizations`.
        pub log: Arc<Mutex<Vec<String>>>,
    }

    impl MemoryStore {
        pub fn with(entries: &[(&str, &str)]) -> Self {
            let store = Self::default();
            for (account, secret) in entries {
                store.put(account, secret);
            }
            store
        }

        pub fn put(&self, account: &str, secret: &str) {
            self.entries.lock().unwrap().insert(account.into(), secret.into());
        }

        pub fn read(&self, account: &str) -> Option<String> {
            self.entries.lock().unwrap().get(account).cloned()
        }

        pub fn fail_set(&self, account: &str) {
            self.failing_sets.lock().unwrap().push(account.into());
        }

        pub fn operations(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    impl SecretStore for MemoryStore {
        fn get(&self, account: &str) -> AppResult<Option<String>> {
            Ok(self.read(account))
        }

        fn set(&self, account: &str, secret: &str) -> AppResult<()> {
            self.log.lock().unwrap().push(format!("set:{account}"));
            if self.failing_sets.lock().unwrap().iter().any(|a| a == account) {
                return Err(AppError::Credentials("keychain unavailable".into()));
            }
            self.put(account, secret);
            Ok(())
        }

        fn delete(&self, account: &str) -> AppResult<()> {
            self.log.lock().unwrap().push(format!("delete:{account}"));
            self.entries.lock().unwrap().remove(account);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::memory::MemoryStore;
    use super::*;

    const LEGACY: &str = r#"{"organization":"Contoso","pat":"legacy-token"}"#;

    fn store(memory: &MemoryStore) -> OrganizationStore {
        OrganizationStore::new(Box::new(memory.clone()))
    }

    fn index_of(memory: &MemoryStore) -> OrganizationIndex {
        serde_json::from_str(&memory.read(INDEX_ACCOUNT).expect("index entry")).unwrap()
    }

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

    #[test]
    fn store_debug_output_never_prints_tokens() {
        let memory = MemoryStore::with(&[("org:contoso", "super-secret-token")]);

        let rendered = format!("{:?}", store(&memory));

        assert!(!rendered.contains("super-secret-token"));
    }

    #[test]
    fn migrates_the_legacy_entry_and_deletes_it_last() {
        let memory = MemoryStore::with(&[(LEGACY_ACCOUNT, LEGACY)]);

        let index = store(&memory).index().unwrap();

        let expected = OrganizationIndex {
            active: Some("contoso".into()),
            organizations: vec!["contoso".into()],
        };
        assert_eq!(index, expected);
        assert_eq!(index_of(&memory), expected);
        assert_eq!(memory.read("org:contoso").as_deref(), Some("legacy-token"));
        assert_eq!(memory.read(LEGACY_ACCOUNT), None);
        assert_eq!(
            memory.operations(),
            vec!["set:org:contoso", "set:organizations", "delete:azure-devops"]
        );
    }

    #[test]
    fn failed_migration_keeps_the_legacy_entry() {
        let memory = MemoryStore::with(&[(LEGACY_ACCOUNT, LEGACY)]);
        memory.fail_set(INDEX_ACCOUNT);

        let result = store(&memory).index();

        assert!(matches!(result, Err(AppError::Credentials(_))));
        assert_eq!(memory.read(LEGACY_ACCOUNT).as_deref(), Some(LEGACY));
        assert_eq!(memory.read(INDEX_ACCOUNT), None);
    }

    #[test]
    fn failed_pat_write_keeps_the_legacy_entry_and_retries_next_time() {
        let memory = MemoryStore::with(&[(LEGACY_ACCOUNT, LEGACY)]);
        memory.fail_set("org:contoso");

        assert!(store(&memory).index().is_err());
        assert_eq!(memory.read(LEGACY_ACCOUNT).as_deref(), Some(LEGACY));

        memory.failing_sets.lock().unwrap().clear();
        let index = store(&memory).index().unwrap();
        assert_eq!(index.active.as_deref(), Some("contoso"));
        assert_eq!(memory.read(LEGACY_ACCOUNT), None);
    }

    #[test]
    fn corrupt_legacy_entry_reads_as_empty_and_is_kept() {
        let memory = MemoryStore::with(&[(LEGACY_ACCOUNT, "garbage")]);

        assert_eq!(store(&memory).index().unwrap(), OrganizationIndex::default());
        assert_eq!(memory.read(LEGACY_ACCOUNT).as_deref(), Some("garbage"));
    }

    #[test]
    fn no_entries_means_no_organizations() {
        let memory = MemoryStore::default();

        assert_eq!(store(&memory).index().unwrap(), OrganizationIndex::default());
    }

    #[test]
    fn corrupt_index_reads_as_empty_without_deleting_secrets() {
        let memory = MemoryStore::with(&[
            (INDEX_ACCOUNT, "{not json"),
            ("org:contoso", "token"),
            (LEGACY_ACCOUNT, LEGACY),
        ]);

        let index = store(&memory).index().unwrap();

        assert_eq!(index, OrganizationIndex::default());
        assert_eq!(memory.read("org:contoso").as_deref(), Some("token"));
        assert_eq!(memory.read(LEGACY_ACCOUNT).as_deref(), Some(LEGACY));
        assert!(memory.operations().iter().all(|op| !op.starts_with("delete:")));
    }

    #[test]
    fn index_is_normalized_on_read() {
        let memory = MemoryStore::with(&[(
            INDEX_ACCOUNT,
            r#"{"active":"Gone","organizations":["Contoso","contoso","fabrikam"]}"#,
        )]);

        let index = store(&memory).index().unwrap();

        assert_eq!(index.organizations, vec!["contoso", "fabrikam"]);
        assert_eq!(index.active.as_deref(), Some("contoso"));
    }

    #[test]
    fn add_stores_the_pat_and_makes_it_active() {
        let memory = MemoryStore::default();
        let store = store(&memory);

        store.add("contoso", "one").unwrap();
        let index = store.add("Fabrikam", "two").unwrap();

        assert_eq!(index.organizations, vec!["contoso", "fabrikam"]);
        assert_eq!(index.active.as_deref(), Some("fabrikam"));
        assert_eq!(index_of(&memory), index);
        assert_eq!(store.pat("fabrikam").unwrap().as_deref(), Some("two"));
    }

    #[test]
    fn adding_an_existing_organization_replaces_its_pat() {
        let memory = MemoryStore::default();
        let store = store(&memory);
        store.add("contoso", "old").unwrap();
        store.add("fabrikam", "other").unwrap();

        let index = store.add("CONTOSO", "new").unwrap();

        assert_eq!(index.organizations, vec!["contoso", "fabrikam"]);
        assert_eq!(index.active.as_deref(), Some("contoso"));
        assert_eq!(memory.read("org:contoso").as_deref(), Some("new"));
    }

    #[test]
    fn set_active_rejects_unknown_organizations() {
        let memory = MemoryStore::default();
        let store = store(&memory);
        store.add("contoso", "one").unwrap();

        assert!(matches!(store.set_active("fabrikam"), Err(AppError::Credentials(_))));
        assert_eq!(store.index().unwrap().active.as_deref(), Some("contoso"));
    }

    #[test]
    fn removing_the_active_organization_activates_the_next_one() {
        let memory = MemoryStore::default();
        let store = store(&memory);
        store.add("alpha", "a").unwrap();
        store.add("beta", "b").unwrap();
        store.add("gamma", "c").unwrap();
        store.set_active("beta").unwrap();

        let index = store.remove("beta").unwrap();

        assert_eq!(index.organizations, vec!["alpha", "gamma"]);
        assert_eq!(index.active.as_deref(), Some("gamma"));
        assert_eq!(memory.read("org:beta"), None);
        assert_eq!(index_of(&memory), index);
    }

    #[test]
    fn removing_the_last_listed_active_organization_falls_back_to_the_previous() {
        let memory = MemoryStore::default();
        let store = store(&memory);
        store.add("alpha", "a").unwrap();
        store.add("beta", "b").unwrap();

        let index = store.remove("beta").unwrap();

        assert_eq!(index.active.as_deref(), Some("alpha"));
    }

    #[test]
    fn removing_the_only_organization_signs_out() {
        let memory = MemoryStore::default();
        let store = store(&memory);
        store.add("contoso", "a").unwrap();

        let index = store.remove("contoso").unwrap();

        assert_eq!(index, OrganizationIndex::default());
        assert_eq!(memory.read("org:contoso"), None);
    }

    #[test]
    fn removing_an_inactive_organization_keeps_the_active_one() {
        let memory = MemoryStore::default();
        let store = store(&memory);
        store.add("alpha", "a").unwrap();
        store.add("beta", "b").unwrap();

        let index = store.remove("alpha").unwrap();

        assert_eq!(index.organizations, vec!["beta"]);
        assert_eq!(index.active.as_deref(), Some("beta"));
    }

    #[test]
    fn removing_also_deletes_a_leftover_legacy_entry_of_that_organization() {
        let memory = MemoryStore::with(&[
            (INDEX_ACCOUNT, r#"{"active":"contoso","organizations":["contoso"]}"#),
            ("org:contoso", "token"),
            (LEGACY_ACCOUNT, LEGACY),
        ]);

        store(&memory).remove("contoso").unwrap();

        assert_eq!(memory.read(LEGACY_ACCOUNT), None);
        assert_eq!(memory.read("org:contoso"), None);
    }
}
