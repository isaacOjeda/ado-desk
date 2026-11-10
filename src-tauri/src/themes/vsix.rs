//! Reads color themes out of a `.vsix` (a zip) held in memory. The archive is untrusted:
//! entries are read only by name, never extracted to disk, and every read is capped.

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};

use serde_json::Value;
use zip::ZipArchive;

use super::jsonc;
use super::vscode::{self, ThemeKind};

/// Largest single JSON entry read from the archive (theme files are ~75 KB in practice).
const MAX_ENTRY_BYTES: u64 = 4 * 1024 * 1024;
/// Total decompressed bytes read from one archive.
const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
/// Themes read from one extension; extra entries are ignored.
pub const MAX_THEMES_PER_EXTENSION: usize = 32;
/// `include` chains followed per theme file.
const MAX_INCLUDE_DEPTH: usize = 3;
/// Longest theme name kept, in characters.
pub const MAX_NAME_CHARS: usize = 80;

const ROOT: &str = "extension";

/// Failures reading an extension package. Messages are fixed strings: nothing from the archive is echoed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VsixError {
    NotAnArchive,
    MissingManifest,
    TooLarge,
    NoThemes,
}

impl VsixError {
    pub fn message(self) -> &'static str {
        match self {
            VsixError::NotAnArchive => "The download is not a valid extension package",
            VsixError::MissingManifest => "The extension package has no readable package.json",
            VsixError::TooLarge => "The extension package is too large to import",
            VsixError::NoThemes => "The extension has no color themes that could be read",
        }
    }
}

/// One color theme, already mapped onto ADO Desk tokens.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeFile {
    pub name: String,
    pub kind: ThemeKind,
    pub colors: BTreeMap<String, String>,
}

struct Archive {
    zip: ZipArchive<Cursor<Vec<u8>>>,
    budget: u64,
}

impl Archive {
    /// Reads one entry as UTF-8 text. `Ok(None)` when it does not exist or is not text.
    fn read(&mut self, name: &str) -> Result<Option<String>, VsixError> {
        let Ok(entry) = self.zip.by_name(name) else { return Ok(None) };
        let cap = MAX_ENTRY_BYTES.min(self.budget);
        let mut bytes = Vec::new();
        // `take` bounds the decompressed size whatever the entry header claims.
        entry.take(cap + 1).read_to_end(&mut bytes).map_err(|_| VsixError::NotAnArchive)?;
        let read = bytes.len() as u64;
        if read > cap {
            return Err(VsixError::TooLarge);
        }
        self.budget -= read;
        let text = String::from_utf8(bytes).ok().map(|t| t.trim_start_matches('\u{feff}').to_string());
        Ok(text)
    }

    fn read_json(&mut self, name: &str) -> Result<Option<Value>, VsixError> {
        Ok(self.read(name)?.and_then(|text| serde_json::from_str(&jsonc::to_json(&text)).ok()))
    }
}

/// Resolves `relative` against the directory `base` (both `/`-separated, inside the archive).
/// Returns `None` for absolute paths or paths escaping the `extension/` root.
pub fn resolve(base: &str, relative: &str) -> Option<String> {
    let relative = relative.replace('\\', "/");
    if relative.starts_with('/') || relative.contains(':') {
        return None;
    }
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    (parts.first() == Some(&ROOT) && parts.len() > 1).then(|| parts.join("/"))
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Bidirectional and zero-width format characters, which can make a name display as something else.
pub(crate) fn is_format_char(c: char) -> bool {
    matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2069}' | '\u{feff}')
}

/// Keeps printable characters (no control or bidi format characters), trims, and bounds the length.
pub fn clean_text(input: &str, max_chars: usize) -> String {
    let cleaned: String = input.chars().filter(|c| !c.is_control() && !is_format_char(*c)).collect();
    cleaned.trim().chars().take(max_chars).collect::<String>().trim().to_string()
}

/// The parts of a VS Code theme file ADO Desk uses.
struct RawTheme {
    colors: HashMap<String, String>,
    file_type: Option<String>,
    name: Option<String>,
}

/// Reads `colors` (string values only), `type` and `name` of a theme file, following `include`.
fn read_theme_file(archive: &mut Archive, path: &str, depth: usize) -> Result<Option<RawTheme>, VsixError> {
    let Some(json) = archive.read_json(path)? else { return Ok(None) };
    let include = json
        .get("include")
        .and_then(Value::as_str)
        .filter(|include| depth < MAX_INCLUDE_DEPTH && include.to_ascii_lowercase().ends_with(".json"))
        .and_then(|include| resolve(parent(path), include));
    let base = match include {
        Some(target) => read_theme_file(archive, &target, depth + 1)?,
        None => None,
    };
    let mut theme = base.unwrap_or(RawTheme { colors: HashMap::new(), file_type: None, name: None });
    if let Some(map) = json.get("colors").and_then(Value::as_object) {
        for (key, value) in map {
            if let Some(value) = value.as_str() {
                theme.colors.insert(key.clone(), value.to_string());
            }
        }
    }
    if let Some(t) = json.get("type").and_then(Value::as_str) {
        theme.file_type = Some(t.to_string());
    }
    theme.name = json.get("name").and_then(Value::as_str).map(str::to_string);
    Ok(Some(theme))
}

/// Reads every color theme listed in `contributes.themes`. `fallback_name` names themes without a usable label.
pub fn read_themes(bytes: Vec<u8>, fallback_name: &str) -> Result<Vec<ThemeFile>, VsixError> {
    let zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| VsixError::NotAnArchive)?;
    let mut archive = Archive { zip, budget: MAX_TOTAL_BYTES };
    let manifest = archive.read_json("extension/package.json")?.ok_or(VsixError::MissingManifest)?;
    let nls = archive.read_json("extension/package.nls.json")?;

    let entries = manifest
        .pointer("/contributes/themes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut themes = Vec::new();
    for entry in entries.iter().take(MAX_THEMES_PER_EXTENSION) {
        let Some(path) = entry.get("path").and_then(Value::as_str).and_then(|p| resolve(ROOT, p)) else { continue };
        let Some(RawTheme { colors, file_type, name: file_name }) = read_theme_file(&mut archive, &path, 0)? else {
            continue;
        };
        if colors.is_empty() {
            continue;
        }
        let label = entry.get("label").and_then(Value::as_str).map(|label| localize(label, nls.as_ref()));
        let name = [label, file_name, Some(fallback_name.to_string())]
            .into_iter()
            .flatten()
            .map(|n| clean_text(&n, MAX_NAME_CHARS))
            .find(|n| !n.is_empty() && !n.starts_with('%'))
            .unwrap_or_else(|| "Imported theme".to_string());
        let kind = vscode::kind_of(entry.get("uiTheme").and_then(Value::as_str), file_type.as_deref(), &colors);
        themes.push(ThemeFile { name, kind, colors: vscode::map_colors(&colors, kind) });
    }
    if themes.is_empty() {
        return Err(VsixError::NoThemes);
    }
    Ok(themes)
}

/// `%key%` labels are looked up in package.nls.json, as VS Code does.
fn localize(label: &str, nls: Option<&Value>) -> String {
    label
        .strip_prefix('%')
        .and_then(|rest| rest.strip_suffix('%'))
        .and_then(|key| nls?.get(key)?.as_str())
        .unwrap_or(label)
        .to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    use crate::themes::color::is_strict_hex;

    /// Builds an in-memory vsix from `(path, contents)` pairs.
    pub(crate) fn vsix(files: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (path, contents) in files {
            writer.start_file(*path, SimpleFileOptions::default()).unwrap();
            writer.write_all(contents.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    const MANIFEST: &str = r#"{
        "name": "demo",
        "contributes": { "themes": [
            { "label": "Demo Dark", "uiTheme": "vs-dark", "path": "./themes/dark.json" },
            { "label": "%light.label%", "uiTheme": "vs", "path": "themes/light.json" }
        ] }
    }"#;

    #[test]
    fn reads_every_contributed_theme_with_kind_name_and_full_tokens() {
        let bytes = vsix(&[
            ("extension/package.json", MANIFEST),
            ("extension/package.nls.json", r#"{ "light.label": "Demo Light" }"#),
            ("extension/themes/dark.json", r##"{ // JSONC
                "colors": { "editor.background": "#1e1e2e", "editor.foreground": "#ccc", }, }"##),
            ("extension/themes/light.json", r##"{ "colors": { "editor.background": "#ffffff", "x": ["#000"] } }"##),
        ]);
        let themes = read_themes(bytes, "Demo").unwrap();
        assert_eq!(themes.len(), 2);
        assert_eq!((themes[0].name.as_str(), themes[0].kind), ("Demo Dark", ThemeKind::Dark));
        assert_eq!(themes[0].colors["contentBg"], "#1e1e2e");
        assert_eq!(themes[0].colors["text"], "#cccccc");
        assert_eq!((themes[1].name.as_str(), themes[1].kind), ("Demo Light", ThemeKind::Light));
        for theme in &themes {
            assert_eq!(theme.colors.len(), vscode::TOKENS.len());
            assert!(theme.colors.values().all(|c| is_strict_hex(c)));
        }
    }

    #[test]
    fn follows_include_inside_the_archive_and_lets_the_file_override_it() {
        let bytes = vsix(&[
            (
                "extension/package.json",
                r#"{ "contributes": { "themes": [ { "label": "Child", "path": "./themes/child.json" } ] } }"#,
            ),
            ("extension/themes/base.json", r##"{ "type": "light", "colors": { "editor.background": "#fafafa", "editor.foreground": "#111111" } }"##),
            ("extension/themes/child.json", r##"{ "include": "./base.json", "colors": { "editor.foreground": "#222222" } }"##),
        ]);
        let theme = &read_themes(bytes, "x").unwrap()[0];
        assert_eq!(theme.kind, ThemeKind::Light);
        assert_eq!(theme.colors["contentBg"], "#fafafa");
        assert_eq!(theme.colors["text"], "#222222");
    }

    #[test]
    fn paths_cannot_escape_the_extension_folder() {
        assert_eq!(resolve("extension", "./themes/a.json").as_deref(), Some("extension/themes/a.json"));
        assert_eq!(resolve("extension/themes", "../b.json").as_deref(), Some("extension/b.json"));
        assert_eq!(resolve("extension", ".\\themes\\a.json").as_deref(), Some("extension/themes/a.json"));
        assert_eq!(resolve("extension", "../secret.json"), None);
        assert_eq!(resolve("extension", "/etc/passwd"), None);
        assert_eq!(resolve("extension", "C:/x.json"), None);
        assert_eq!(resolve("extension", "."), None);
    }

    #[test]
    fn rejects_non_archives_and_extensions_without_themes() {
        assert_eq!(read_themes(b"not a zip".to_vec(), "x"), Err(VsixError::NotAnArchive));
        assert_eq!(read_themes(vsix(&[("extension/readme.md", "hi")]), "x"), Err(VsixError::MissingManifest));
        let no_themes = vsix(&[("extension/package.json", r#"{ "contributes": { "iconThemes": [] } }"#)]);
        assert_eq!(read_themes(no_themes, "x"), Err(VsixError::NoThemes));
    }

    #[test]
    fn oversized_entries_are_rejected_without_reading_them_fully() {
        let huge = format!("{{ \"colors\": {{}}, \"pad\": \"{}\" }}", "a".repeat(MAX_ENTRY_BYTES as usize + 10));
        let bytes = vsix(&[
            ("extension/package.json", r#"{ "contributes": { "themes": [ { "path": "./t.json" } ] } }"#),
            ("extension/t.json", &huge),
        ]);
        assert_eq!(read_themes(bytes, "x"), Err(VsixError::TooLarge));
    }

    #[test]
    fn names_are_cleaned_and_fall_back_to_the_extension_name() {
        let bytes = vsix(&[
            ("extension/package.json", r#"{ "contributes": { "themes": [ { "label": "%missing%", "path": "./t.json" } ] } }"#),
            ("extension/t.json", r##"{ "colors": { "editor.background": "#000000" } }"##),
        ]);
        assert_eq!(read_themes(bytes, "Fallback\u{7}Name").unwrap()[0].name, "FallbackName");
        assert_eq!(clean_text(&format!("  {}  ", "x".repeat(200)), MAX_NAME_CHARS).len(), MAX_NAME_CHARS);
        assert_eq!(clean_text("\u{202e}evil\u{200b} name\u{2066}", MAX_NAME_CHARS), "evil name");
    }
}
