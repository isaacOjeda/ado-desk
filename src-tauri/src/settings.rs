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
    /// How much of the desktop shows through glass layers, 0 (opaque) to 100.
    pub glass_intensity: u8,
}

/// Matches the original fixed glass opacity (sidebar 70%, chrome 76%); reading panes sit at 85%.
pub const DEFAULT_GLASS_INTENSITY: u8 = 50;

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            mode: ThemeMode::System,
            light_theme: DEFAULT_LIGHT_THEME.into(),
            dark_theme: DEFAULT_DARK_THEME.into(),
            translucency: true,
            glass_intensity: DEFAULT_GLASS_INTENSITY,
        }
    }
}

/// The global search bar shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchSettings {
    pub global_shortcut_enabled: bool,
    /// Accelerator such as `CommandOrControl+Shift+Space`.
    pub global_shortcut: String,
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self { global_shortcut_enabled: true, global_shortcut: crate::search_bar::DEFAULT_SHORTCUT.into() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub appearance: AppearanceSettings,
    pub search: SearchSettings,
}

/// Loaded settings plus where they live, managed by Tauri.
pub struct SettingsState {
    pub path: PathBuf,
    /// Held only for short reads and writes; never across a call that waits on the main thread.
    pub settings: Mutex<Settings>,
    /// Serializes `update_settings` calls. Only update (worker threads) takes it, so it may be
    /// held while the shortcut plugin waits on the main thread.
    pub updates: Mutex<()>,
}

impl SettingsState {
    pub fn load(path: PathBuf) -> Self {
        let settings = Mutex::new(load(&path));
        Self { path, settings, updates: Mutex::new(()) }
    }

    pub fn current(&self) -> Settings {
        self.settings.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Longest theme id accepted from the webview.
pub const MAX_THEME_ID_LEN: usize = 64;

/// Theme ids are short slugs (`[a-z0-9-]`), which also fits ids derived from Open VSX themes.
pub fn is_valid_theme_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_THEME_ID_LEN
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Trims theme ids (empty, oversized or malformed ones become the defaults) and clamps glass intensity.
pub fn normalize(mut settings: Settings) -> Settings {
    let fix = |id: &mut String, default: &str| {
        let trimmed = id.trim();
        *id = if is_valid_theme_id(trimmed) { trimmed.to_string() } else { default.to_string() };
    };
    fix(&mut settings.appearance.light_theme, DEFAULT_LIGHT_THEME);
    fix(&mut settings.appearance.dark_theme, DEFAULT_DARK_THEME);
    settings.appearance.glass_intensity = settings.appearance.glass_intensity.min(100);
    let shortcut = settings.search.global_shortcut.trim();
    settings.search.global_shortcut = if crate::search_bar::is_valid_shortcut(shortcut) {
        shortcut.to_string()
    } else {
        SearchSettings::default().global_shortcut
    };
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
    let json = serde_json::to_vec_pretty(settings).map_err(std::io::Error::other)?;
    write_atomic(path, &json)
}

/// Writes `bytes` to a sibling temp file, syncs it and renames it over `path`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
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
    fn glass_intensity_defaults_to_today_look_for_older_files() {
        let settings: Settings = serde_json::from_str(
            r#"{ "appearance": { "mode": "dark", "lightTheme": "paper", "darkTheme": "graphite", "translucency": true } }"#,
        )
        .unwrap();

        assert_eq!(settings.appearance.glass_intensity, DEFAULT_GLASS_INTENSITY);
        assert_eq!(DEFAULT_GLASS_INTENSITY, 50);
    }

    #[test]
    fn older_files_get_the_default_global_shortcut() {
        let settings: Settings = serde_json::from_str(r#"{ "appearance": { "mode": "dark" } }"#).unwrap();
        assert_eq!(settings.search, SearchSettings::default());
        assert!(settings.search.global_shortcut_enabled);
    }

    #[test]
    fn normalize_replaces_an_invalid_global_shortcut_and_trims_a_valid_one() {
        let mut settings = Settings::default();
        settings.search.global_shortcut = "Shift+A".into();
        assert_eq!(normalize(settings.clone()).search.global_shortcut, "CommandOrControl+Shift+Space");

        settings.search.global_shortcut = " Alt+KeyK ".into();
        settings.search.global_shortcut_enabled = false;
        let fixed = normalize(settings);
        assert_eq!(fixed.search.global_shortcut, "Alt+KeyK");
        assert!(!fixed.search.global_shortcut_enabled);
    }

    #[test]
    fn normalize_clamps_glass_intensity_to_100() {
        let mut settings = Settings::default();
        settings.appearance.glass_intensity = 180;

        assert_eq!(normalize(settings).appearance.glass_intensity, 100);
    }

    #[test]
    fn normalize_replaces_malformed_or_oversized_theme_ids_with_defaults() {
        let mut settings = Settings::default();
        settings.appearance.light_theme = "</style><script>x</script>".into();
        settings.appearance.dark_theme = "a".repeat(65);

        let fixed = normalize(settings);

        assert_eq!(fixed.appearance.light_theme, DEFAULT_LIGHT_THEME);
        assert_eq!(fixed.appearance.dark_theme, DEFAULT_DARK_THEME);
    }

    #[test]
    fn normalize_keeps_valid_theme_ids() {
        let mut settings = Settings::default();
        settings.appearance.light_theme = " sepia ".into();
        settings.appearance.dark_theme = "open-vsx-theme-2".into();

        let fixed = normalize(settings);

        assert_eq!(fixed.appearance.light_theme, "sepia");
        assert_eq!(fixed.appearance.dark_theme, "open-vsx-theme-2");
    }

    #[test]
    fn defaults_match_the_frontend_contract() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "appearance": { "mode": "system", "lightTheme": "paper", "darkTheme": "graphite", "translucency": true, "glassIntensity": 50 },
                "search": { "globalShortcutEnabled": true, "globalShortcut": "CommandOrControl+Shift+Space" }
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
