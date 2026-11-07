//! User settings persisted as JSON in the app config dir. The shape mirrors
//! `src/lib/settings.ts`; loading never fails (missing or corrupt files fall
//! back to defaults) and saving is atomic (temp file + rename).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "settings.json";
pub const CHANGED_EVENT: &str = "settings-changed";

const DEFAULT_LIGHT_THEME: &str = "paper";
const DEFAULT_DARK_THEME: &str = "graphite";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppearanceSettings {
    pub mode: ThemeMode,
    pub light_theme: String,
    pub dark_theme: String,
    pub translucency: bool,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            mode: ThemeMode::System,
            light_theme: DEFAULT_LIGHT_THEME.into(),
            dark_theme: DEFAULT_DARK_THEME.into(),
            translucency: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub appearance: AppearanceSettings,
}

/// Loaded settings plus where they live, managed by Tauri.
pub struct SettingsState {
    pub path: PathBuf,
    pub settings: Mutex<Settings>,
}

impl SettingsState {
    pub fn load(path: PathBuf) -> Self {
        let settings = Mutex::new(load(&path));
        Self { path, settings }
    }
}

/// Trims theme ids and replaces empty ones with the defaults.
pub fn normalize(mut settings: Settings) -> Settings {
    let fix = |id: &mut String, default: &str| {
        let trimmed = id.trim();
        *id = if trimmed.is_empty() { default.to_string() } else { trimmed.to_string() };
    };
    fix(&mut settings.appearance.light_theme, DEFAULT_LIGHT_THEME);
    fix(&mut settings.appearance.dark_theme, DEFAULT_DARK_THEME);
    settings
}

/// Reads settings from `path`. Missing, unreadable or corrupt files yield defaults.
pub fn load(path: &Path) -> Settings {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
        .map(normalize)
        .unwrap_or_default()
}

/// Writes settings to a sibling temp file and renames it over `path`, so a
/// crash mid-write never leaves a truncated settings file.
pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(std::io::Error::other)?;
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&json)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Unique scratch dir under the system temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let dir = std::env::temp_dir().join(format!("ado-desk-settings-{name}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn defaults_match_the_frontend_contract() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "appearance": { "mode": "system", "lightTheme": "paper", "darkTheme": "graphite", "translucency": true }
            })
        );
    }

    #[test]
    fn missing_file_loads_defaults() {
        let scratch = Scratch::new("missing");
        assert_eq!(load(&scratch.0.join(FILE_NAME)), Settings::default());
    }

    #[test]
    fn corrupt_file_loads_defaults() {
        let scratch = Scratch::new("corrupt");
        let path = scratch.0.join(FILE_NAME);
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), Settings::default());
    }

    #[test]
    fn partial_file_keeps_known_fields_and_ignores_unknown_ones() {
        let scratch = Scratch::new("partial");
        let path = scratch.0.join(FILE_NAME);
        fs::write(&path, r#"{ "appearance": { "mode": "dark", "future": 1 }, "other": true }"#).unwrap();

        let settings = load(&path);

        assert_eq!(settings.appearance.mode, ThemeMode::Dark);
        assert_eq!(settings.appearance.light_theme, "paper");
        assert_eq!(settings.appearance.dark_theme, "graphite");
        assert!(settings.appearance.translucency);
    }

    #[test]
    fn normalize_replaces_empty_theme_ids_and_trims() {
        let mut settings = Settings::default();
        settings.appearance.light_theme = "   ".into();
        settings.appearance.dark_theme = " midnight ".into();

        let normalized = normalize(settings);

        assert_eq!(normalized.appearance.light_theme, "paper");
        assert_eq!(normalized.appearance.dark_theme, "midnight");
    }

    #[test]
    fn save_creates_parent_dirs_and_round_trips_without_leaving_temp_files() {
        let scratch = Scratch::new("save");
        let path = scratch.0.join("nested").join("dir").join(FILE_NAME);
        let mut settings = Settings::default();
        settings.appearance.mode = ThemeMode::Light;
        settings.appearance.translucency = false;

        save(&path, &settings).unwrap();
        settings.appearance.dark_theme = "midnight".into();
        save(&path, &settings).unwrap();

        assert_eq!(load(&path), settings);
        let entries: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from(FILE_NAME)]);
    }
}
