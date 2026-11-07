//! Color themes imported from Open VSX (VS Code color theme extensions). Search and download
//! run here, never in the webview; the frontend only receives validated token maps.

mod color;
mod jsonc;
pub mod openvsx;
pub mod store;
mod vscode;
mod vsix;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, State};

use crate::error::{AppError, AppResult};
use openvsx::{OpenVsx, ThemeExtension};
use store::{ImportedTheme, ThemeSource};

/// Emitted to every window with the full imported-theme list after it changes.
pub const CHANGED_EVENT: &str = "themes-changed";

pub struct ThemesState {
    path: PathBuf,
    themes: Mutex<Vec<ImportedTheme>>,
    client: Option<OpenVsx>,
}

impl ThemesState {
    pub fn load(path: PathBuf) -> Self {
        let themes = Mutex::new(store::load(&path));
        // Building the HTTP client only fails if the TLS backend cannot start; imports then report it.
        Self { path, themes, client: OpenVsx::new().ok() }
    }

    fn client(&self) -> AppResult<&OpenVsx> {
        self.client.as_ref().ok_or_else(|| AppError::Network("HTTP client unavailable".into()))
    }

    fn current(&self) -> Vec<ImportedTheme> {
        self.themes.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Applies `change` to the stored list under the lock, persists it, then broadcasts it.
    fn update<T>(
        &self,
        app: &AppHandle,
        change: impl FnOnce(&[ImportedTheme]) -> AppResult<(Vec<ImportedTheme>, T)>,
    ) -> AppResult<T> {
        let (themes, result) = {
            // Holding the lock across the write serializes imports and removals from any window.
            let mut current = self.themes.lock().unwrap_or_else(|e| e.into_inner());
            let (next, result) = change(&current)?;
            store::save(&self.path, &next).map_err(|_| AppError::ThemeImport("Could not save imported themes"))?;
            *current = next.clone();
            (next, result)
        };
        let _ = app.emit(CHANGED_EVENT, &themes);
        Ok(result)
    }
}

#[tauri::command]
pub async fn search_theme_extensions(state: State<'_, ThemesState>, query: String) -> AppResult<Vec<ThemeExtension>> {
    let query = openvsx::normalize_query(&query)?;
    state.client()?.search(&query).await
}

/// Downloads an extension and imports every color theme it contributes; returns the themes just imported.
#[tauri::command]
pub async fn import_theme_extension(
    app: AppHandle,
    state: State<'_, ThemesState>,
    namespace: String,
    name: String,
) -> AppResult<Vec<ImportedTheme>> {
    let (extension, bytes) = state.client()?.download(namespace.trim(), name.trim()).await?;
    let fallback_name = extension.display_name.clone();
    let files = tauri::async_runtime::spawn_blocking(move || vsix::read_themes(bytes, &fallback_name))
        .await
        .map_err(|_| AppError::ThemeImport("The extension package could not be read"))?
        .map_err(|e| AppError::ThemeImport(e.message()))?;
    let source = ThemeSource::from_extension(&extension);
    state.update(&app, |current| {
        store::merge(current, &source, files)
            .ok_or(AppError::ThemeImport("Too many imported themes; remove some before importing more"))
    })
}

#[tauri::command]
pub fn list_imported_themes(state: State<'_, ThemesState>) -> Vec<ImportedTheme> {
    state.current()
}

/// Removes one imported theme; returns the remaining list.
#[tauri::command]
pub fn remove_imported_theme(app: AppHandle, state: State<'_, ThemesState>, id: String) -> AppResult<Vec<ImportedTheme>> {
    state.update(&app, |current| {
        let next = store::remove(current, id.trim()).ok_or(AppError::NotFound)?;
        Ok((next.clone(), next))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live check against open-vsx.org (network): `cargo test live_open_vsx -- --ignored`.
    #[test]
    #[ignore = "needs network access to open-vsx.org"]
    fn live_open_vsx_search_and_import_of_real_theme_packages() {
        tauri::async_runtime::block_on(async {
            let client = OpenVsx::new().unwrap();
            let popular = client.search("").await.unwrap();
            assert!(!popular.is_empty());
            let found = client.search("tokyo night").await.unwrap();
            assert!(found.iter().any(|e| e.namespace == "enkia" && e.name == "tokyo-night"));

            for (namespace, name, min_themes) in [
                ("enkia", "tokyo-night", 3),                          // JSONC
                ("arcticicestudio", "nord-visual-studio-code", 1),   // JSONC
                ("GitHub", "github-vscode-theme", 6),                // no `type`, hc variants
                ("zhuangtongfa", "material-theme", 5),               // short hex
                ("Catppuccin", "catppuccin-vsc", 4),
            ] {
                let (extension, bytes) = client.download(namespace, name).await.unwrap();
                let themes = vsix::read_themes(bytes, &extension.display_name).unwrap();
                assert!(themes.len() >= min_themes, "{namespace}.{name}: {}", themes.len());
                let (all, added) = store::merge(&[], &ThemeSource::from_extension(&extension), themes).unwrap();
                assert!(added.iter().all(store::is_valid), "{namespace}.{name}");
                for t in &all {
                    println!("{:<48} {:?} bg={} text={} accent={}", t.id, t.kind, t.colors["contentBg"], t.colors["text"], t.colors["accent"]);
                }
            }
        });
    }
}
