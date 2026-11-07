//! Imported themes persisted as JSON in the app config dir, next to settings.json. Loading
//! never fails and drops entries that do not validate (the file may be edited by hand);
//! saving is atomic. Ids are `ovsx-` slugs so they never collide with built-in themes.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::color::is_strict_hex;
use super::openvsx::{is_valid_id, ThemeExtension};
use super::vscode::{ThemeKind, TOKENS};
use super::vsix::{clean_text, ThemeFile, MAX_NAME_CHARS};
use crate::settings::{is_valid_theme_id, write_atomic, MAX_THEME_ID_LEN};

pub const FILE_NAME: &str = "imported-themes.json";
pub const ID_PREFIX: &str = "ovsx-";
/// Upper bound on stored themes, so the catalog stays small.
pub const MAX_IMPORTED_THEMES: usize = 200;

/// The Open VSX extension a theme came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSource {
    pub namespace: String,
    pub name: String,
    pub display_name: String,
    pub version: String,
}

/// Same shape as the frontend `Theme`, plus where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedTheme {
    pub id: String,
    pub name: String,
    pub kind: ThemeKind,
    pub colors: BTreeMap<String, String>,
    pub source: ThemeSource,
}

#[derive(Default, Serialize, Deserialize)]
struct StoredThemes {
    #[serde(default)]
    themes: Vec<serde_json::Value>,
}

impl ThemeSource {
    pub fn from_extension(extension: &ThemeExtension) -> Self {
        Self {
            namespace: extension.namespace.clone(),
            name: extension.name.clone(),
            display_name: extension.display_name.clone(),
            version: extension.version.clone(),
        }
    }

    fn same_extension(&self, other: &ThemeSource) -> bool {
        self.namespace.eq_ignore_ascii_case(&other.namespace) && self.name.eq_ignore_ascii_case(&other.name)
    }
}

/// True when every field is safe to hand to the webview.
pub fn is_valid(theme: &ImportedTheme) -> bool {
    let colors_ok = theme.colors.len() == TOKENS.len()
        && TOKENS.iter().all(|token| theme.colors.get(*token).is_some_and(|c| is_strict_hex(c)));
    let source_ok = is_valid_id(&theme.source.namespace) && is_valid_id(&theme.source.name);
    is_valid_theme_id(&theme.id)
        && theme.id.starts_with(ID_PREFIX)
        && !theme.name.is_empty()
        && clean_text(&theme.name, MAX_NAME_CHARS) == theme.name
        && colors_ok
        && source_ok
}

/// Reads imported themes. Missing, unreadable or corrupt files yield an empty list; invalid entries are dropped.
pub fn load(path: &Path) -> Vec<ImportedTheme> {
    let stored = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<StoredThemes>(&text).ok())
        .unwrap_or_default();
    let mut seen = HashSet::new();
    stored
        .themes
        .into_iter()
        .filter_map(|value| serde_json::from_value::<ImportedTheme>(value).ok())
        .filter(|theme| is_valid(theme) && seen.insert(theme.id.clone()))
        .take(MAX_IMPORTED_THEMES)
        .collect()
}

pub fn save(path: &Path, themes: &[ImportedTheme]) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(&serde_json::json!({ "themes": themes })).map_err(std::io::Error::other)?;
    write_atomic(path, &json)
}

/// `Catppuccin Mocha` -> `catppuccin-mocha`.
fn slug(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn truncate_id(id: &str, max: usize) -> String {
    id.chars().take(max).collect::<String>().trim_end_matches('-').to_string()
}

/// `ovsx-{namespace}-{extension}-{theme}`, bounded, made unique against `taken`.
fn unique_id(source: &ThemeSource, theme_name: &str, taken: &HashSet<String>) -> String {
    let base = truncate_id(
        &format!("{ID_PREFIX}{}", slug(&format!("{}-{}-{}", source.namespace, source.name, theme_name))),
        MAX_THEME_ID_LEN,
    );
    if !taken.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| {
            let suffix = format!("-{n}");
            format!("{}{suffix}", truncate_id(&base, MAX_THEME_ID_LEN - suffix.len()))
        })
        .find(|candidate| !taken.contains(candidate))
        .expect("an unused suffix exists")
}

/// Replaces every theme previously imported from the same extension with `files`.
/// Returns the new full list and the themes just imported, or `None` when it would exceed the cap.
pub fn merge(
    existing: &[ImportedTheme],
    source: &ThemeSource,
    files: Vec<ThemeFile>,
) -> Option<(Vec<ImportedTheme>, Vec<ImportedTheme>)> {
    let mut themes: Vec<ImportedTheme> =
        existing.iter().filter(|t| !t.source.same_extension(source)).cloned().collect();
    if themes.len() + files.len() > MAX_IMPORTED_THEMES {
        return None;
    }
    let mut taken: HashSet<String> = themes.iter().map(|t| t.id.clone()).collect();
    let mut added = Vec::new();
    for file in files {
        let name = Some(clean_text(&file.name, MAX_NAME_CHARS)).filter(|n| !n.is_empty());
        let name = name.unwrap_or_else(|| "Imported theme".to_string());
        let id = unique_id(source, &name, &taken);
        taken.insert(id.clone());
        added.push(ImportedTheme { id, name, kind: file.kind, colors: file.colors, source: source.clone() });
    }
    themes.extend(added.iter().cloned());
    Some((themes, added))
}

/// Removes the theme with `id`; `None` when there is no such imported theme.
pub fn remove(existing: &[ImportedTheme], id: &str) -> Option<Vec<ImportedTheme>> {
    existing.iter().any(|t| t.id == id).then(|| existing.iter().filter(|t| t.id != id).cloned().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::vscode::map_colors;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let dir = std::env::temp_dir().join(format!("ado-desk-themes-{name}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn file(&self) -> PathBuf {
            self.0.join(FILE_NAME)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn source(namespace: &str, name: &str) -> ThemeSource {
        ThemeSource { namespace: namespace.into(), name: name.into(), display_name: name.into(), version: "1.0.0".into() }
    }

    fn file(name: &str) -> ThemeFile {
        ThemeFile { name: name.into(), kind: ThemeKind::Dark, colors: map_colors(&HashMap::new(), ThemeKind::Dark) }
    }

    #[test]
    fn imported_ids_are_prefixed_slugs_valid_as_settings_theme_ids() {
        let (all, added) = merge(&[], &source("Catppuccin", "catppuccin-vsc"), vec![file("Catppuccin Frappé")]).unwrap();
        assert_eq!(added[0].id, "ovsx-catppuccin-catppuccin-vsc-catppuccin-frapp");
        assert_eq!(all, added);
        assert!(is_valid(&added[0]));
    }

    #[test]
    fn long_or_duplicate_names_still_get_unique_bounded_ids() {
        let long = "x".repeat(200);
        let (_, added) = merge(&[], &source("ns", "ext"), vec![file(&long), file(&long), file("Same"), file("same")]).unwrap();
        let ids: HashSet<&str> = added.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids.len(), 4);
        assert!(added.iter().all(|t| t.id.len() <= MAX_THEME_ID_LEN && is_valid(t)), "{added:?}");
        assert_eq!(added[3].id, "ovsx-ns-ext-same-2");
    }

    #[test]
    fn reimporting_an_extension_replaces_its_themes_and_keeps_others() {
        let (first, _) = merge(&[], &source("a", "one"), vec![file("Old"), file("Gone")]).unwrap();
        let (first, _) = merge(&first, &source("b", "two"), vec![file("Other")]).unwrap();
        let (all, added) = merge(&first, &source("A", "ONE"), vec![file("New")]).unwrap();
        let names: Vec<&str> = all.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Other", "New"]);
        assert_eq!(added.len(), 1);
    }

    #[test]
    fn merge_refuses_to_exceed_the_cap() {
        let files: Vec<ThemeFile> = (0..MAX_IMPORTED_THEMES).map(|i| file(&format!("t{i}"))).collect();
        let (all, _) = merge(&[], &source("a", "big"), files).unwrap();
        assert!(merge(&all, &source("b", "more"), vec![file("one")]).is_none());
    }

    #[test]
    fn remove_drops_only_the_matching_theme() {
        let (all, added) = merge(&[], &source("a", "one"), vec![file("A"), file("B")]).unwrap();
        let left = remove(&all, &added[0].id).unwrap();
        assert_eq!(left, vec![added[1].clone()]);
        assert!(remove(&all, "paper").is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let scratch = Scratch::new("roundtrip");
        let (all, _) = merge(&[], &source("a", "one"), vec![file("A"), file("B")]).unwrap();
        save(&scratch.file(), &all).unwrap();
        assert_eq!(load(&scratch.file()), all);
    }

    #[test]
    fn load_drops_tampered_entries_and_survives_corrupt_files() {
        let scratch = Scratch::new("tampered");
        assert!(load(&scratch.file()).is_empty());
        fs::write(scratch.file(), "{ not json").unwrap();
        assert!(load(&scratch.file()).is_empty());

        let (all, _) = merge(&[], &source("a", "one"), vec![file("Good"), file("Bad color"), file("Bad id")]).unwrap();
        let mut stored = serde_json::to_value(serde_json::json!({ "themes": all })).unwrap();
        stored["themes"][1]["colors"]["text"] = "red;}body{display:none".into();
        stored["themes"][2]["id"] = "paper".into();
        fs::write(scratch.file(), stored.to_string()).unwrap();
        let loaded = load(&scratch.file());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Good");
    }
}
