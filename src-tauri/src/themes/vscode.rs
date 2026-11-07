//! Maps a VS Code color theme (`colors` map + base kind) onto ADO Desk's theme tokens.
//! The key lists mirror the `vscode` column documented in src/themes/types.ts. Every token
//! is always produced: missing sources are derived from the theme's own colors or, as a
//! last resort, taken from the built-in default theme of the same kind.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::color::Rgba;

/// Every key of `ThemeColors` in src/themes/types.ts.
pub const TOKENS: [&str; 27] = [
    "bg",
    "sidebar",
    "contentBg",
    "surface",
    "surfaceHover",
    "border",
    "borderStrong",
    "text",
    "textMuted",
    "textFaint",
    "accent",
    "accentStrong",
    "accentContrast",
    "glassSidebar",
    "glassChrome",
    "shadow",
    "brandStart",
    "brandEnd",
    "diffInserted",
    "diffRemoved",
    "toneNeutral",
    "toneBlue",
    "toneGreen",
    "toneAmber",
    "toneRed",
    "toneViolet",
    "toneOrange",
];

/// Lowest contrast against the reading surface for a border to count as visible.
const MIN_BORDER_CONTRAST: f64 = 1.12;
/// Lowest contrast for `button.foreground` to be kept as the label on the accent color.
const MIN_LABEL_CONTRAST: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
    Light,
    Dark,
}

/// Base kind of a theme: the package.json `uiTheme` wins, then the theme file `type`, then
/// the luminance of `editor.background`. High-contrast variants map to their base kind.
pub fn kind_of(ui_theme: Option<&str>, file_type: Option<&str>, colors: &HashMap<String, String>) -> ThemeKind {
    let from_ui = match ui_theme.map(str::trim) {
        Some("vs") | Some("hc-light") => Some(ThemeKind::Light),
        Some("vs-dark") | Some("hc-black") => Some(ThemeKind::Dark),
        _ => None,
    };
    let from_type = || match file_type.map(|t| t.trim().to_ascii_lowercase()).as_deref() {
        Some("light") | Some("hclight") => Some(ThemeKind::Light),
        Some("dark") | Some("hc") | Some("hcdark") => Some(ThemeKind::Dark),
        _ => None,
    };
    let from_background = || {
        let bg = colors.get("editor.background").and_then(|c| Rgba::parse(c))?;
        Some(if bg.luminance() > 0.5 { ThemeKind::Light } else { ThemeKind::Dark })
    };
    from_ui.or_else(from_type).or_else(from_background).unwrap_or(ThemeKind::Dark)
}

const BUILTIN: &str = include_str!("../../../src/themes/builtin.json");

#[derive(Deserialize)]
struct BuiltinTheme {
    id: String,
    colors: BTreeMap<String, String>,
}

/// Colors of the default built-in theme of `kind` (paper / graphite), used as last resort.
fn builtin_default(kind: ThemeKind) -> &'static BTreeMap<String, String> {
    static DEFAULTS: OnceLock<(BTreeMap<String, String>, BTreeMap<String, String>)> = OnceLock::new();
    let (light, dark) = DEFAULTS.get_or_init(|| {
        let themes: Vec<BuiltinTheme> = serde_json::from_str(BUILTIN).expect("builtin.json is valid");
        let find = |id: &str| themes.iter().find(|t| t.id == id).map(|t| t.colors.clone()).expect("default theme exists");
        (find("paper"), find("graphite"))
    });
    match kind {
        ThemeKind::Light => light,
        ThemeKind::Dark => dark,
    }
}

/// Builds a complete token map (`#rrggbb` / `#rrggbbaa`) from a VS Code `colors` map.
pub fn map_colors(source: &HashMap<String, String>, kind: ThemeKind) -> BTreeMap<String, String> {
    let defaults = builtin_default(kind);
    let fallback = |token: &str| Rgba::parse(&defaults[token]).expect("builtin colors are valid hex");
    let pick = |keys: &[&str]| keys.iter().find_map(|key| source.get(*key).and_then(|value| Rgba::parse(value)));

    let content_bg = pick(&["editor.background"])
        .map(|c| c.over(fallback("contentBg")))
        .unwrap_or_else(|| fallback("contentBg"));
    let solid = |keys: &[&str], under: Rgba| pick(keys).map(|c| c.over(under));
    let visible = |c: Rgba| c.contrast(content_bg) >= MIN_BORDER_CONTRAST;

    let bg = solid(&["editorGroupHeader.tabsBackground", "titleBar.activeBackground"], content_bg).unwrap_or(content_bg);
    let sidebar = solid(&["sideBar.background", "activityBar.background"], bg).unwrap_or(bg);
    let text = solid(&["editor.foreground", "foreground"], content_bg).unwrap_or_else(|| fallback("text"));
    let surface = solid(&["input.background", "editorWidget.background"], content_bg)
        .unwrap_or_else(|| content_bg.mix(text, 0.05));
    let surface_hover = solid(&["list.hoverBackground", "list.inactiveSelectionBackground"], content_bg)
        .filter(|c| *c != content_bg)
        .unwrap_or_else(|| content_bg.mix(text, 0.09));
    let border = solid(&["panel.border", "editorGroup.border", "sideBar.border"], bg)
        .filter(|c| visible(*c))
        .unwrap_or_else(|| content_bg.mix(text, 0.15));
    let border_strong = solid(&["input.border", "contrastBorder"], bg)
        .filter(|c| visible(*c))
        .unwrap_or_else(|| border.mix(text, 0.15));
    let text_muted = solid(&["descriptionForeground"], content_bg).unwrap_or_else(|| text.mix(content_bg, 0.3));
    let text_faint = solid(&["editorLineNumber.foreground", "disabledForeground"], content_bg)
        .unwrap_or_else(|| text.mix(content_bg, 0.5));
    let accent = solid(&["textLink.foreground", "focusBorder", "button.background"], content_bg)
        .unwrap_or_else(|| fallback("accent"));
    let accent_strong = solid(&["textLink.activeForeground", "button.hoverBackground"], content_bg)
        .unwrap_or_else(|| accent.mix(text, 0.25));
    let best_label = if content_bg.contrast(accent) >= text.contrast(accent) { content_bg } else { text };
    let accent_contrast = solid(&["button.foreground"], accent)
        .filter(|c| c.contrast(accent) >= MIN_LABEL_CONTRAST)
        .unwrap_or(best_label);
    let tone = |keys: &[&str], token: &str| solid(keys, content_bg).unwrap_or_else(|| fallback(token));
    let tone_green = tone(&["terminal.ansiGreen", "gitDecoration.addedResourceForeground"], "toneGreen");
    let tone_red = tone(&["terminal.ansiRed", "errorForeground"], "toneRed");
    let tone_violet = tone(&["terminal.ansiMagenta"], "toneViolet");
    let diff = |keys: &[&str], tint: Rgba| pick(keys).unwrap_or_else(|| tint.with_alpha(0x21));

    let colors: [(&str, Rgba); 27] = [
        ("bg", bg),
        ("sidebar", sidebar),
        ("contentBg", content_bg),
        ("surface", surface),
        ("surfaceHover", surface_hover),
        ("border", border),
        ("borderStrong", border_strong),
        ("text", text),
        ("textMuted", text_muted),
        ("textFaint", text_faint),
        ("accent", accent),
        ("accentStrong", accent_strong),
        ("accentContrast", accent_contrast),
        ("glassSidebar", sidebar.with_alpha(0xb3)),
        ("glassChrome", bg.with_alpha(0xc2)),
        ("shadow", pick(&["widget.shadow"]).unwrap_or_else(|| fallback("shadow"))),
        ("brandStart", accent),
        ("brandEnd", tone_violet),
        ("diffInserted", diff(&["diffEditor.insertedLineBackground", "diffEditor.insertedTextBackground"], tone_green)),
        ("diffRemoved", diff(&["diffEditor.removedLineBackground", "diffEditor.removedTextBackground"], tone_red)),
        ("toneNeutral", text_muted),
        ("toneBlue", tone(&["terminal.ansiBlue"], "toneBlue")),
        ("toneGreen", tone_green),
        ("toneAmber", tone(&["terminal.ansiYellow", "editorWarning.foreground"], "toneAmber")),
        ("toneRed", tone_red),
        ("toneViolet", tone_violet),
        ("toneOrange", tone(&["gitDecoration.modifiedResourceForeground", "terminal.ansiBrightYellow"], "toneOrange")),
    ];
    colors.into_iter().map(|(token, color)| (token.to_string(), color.to_hex())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::color::is_strict_hex;

    fn colors(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn every_token_is_produced_as_strict_hex_even_from_an_empty_theme() {
        for kind in [ThemeKind::Light, ThemeKind::Dark] {
            let mapped = map_colors(&HashMap::new(), kind);
            assert_eq!(mapped.len(), TOKENS.len());
            for token in TOKENS {
                assert!(is_strict_hex(&mapped[token]), "{token}: {}", mapped[token]);
            }
        }
    }

    #[test]
    fn tokens_match_the_contrast_test_list() {
        let builtin = builtin_default(ThemeKind::Dark);
        let mut keys: Vec<&str> = builtin.keys().map(String::as_str).collect();
        let mut tokens = TOKENS.to_vec();
        keys.sort_unstable();
        tokens.sort_unstable();
        assert_eq!(keys, tokens);
    }

    #[test]
    fn first_matching_vscode_key_wins_and_short_hex_is_expanded() {
        let mapped = map_colors(
            &colors(&[
                ("editor.background", "#1E1E2E"),
                ("editor.foreground", "#cdd6f4"),
                ("foreground", "#ffffff"),
                ("textLink.foreground", "#89b4fa"),
                ("focusBorder", "#f00"),
            ]),
            ThemeKind::Dark,
        );
        assert_eq!(mapped["contentBg"], "#1e1e2e");
        assert_eq!(mapped["text"], "#cdd6f4");
        assert_eq!(mapped["accent"], "#89b4fa");
        assert_eq!(mapped["brandStart"], "#89b4fa");
    }

    #[test]
    fn opaque_tokens_flatten_translucent_sources_over_the_reading_surface() {
        let mapped = map_colors(
            &colors(&[("editor.background", "#000000"), ("list.hoverBackground", "#ffffff80")]),
            ThemeKind::Dark,
        );
        assert_eq!(mapped["surfaceHover"], "#808080");
    }

    #[test]
    fn invisible_borders_are_derived_from_text_and_background() {
        let mapped = map_colors(
            &colors(&[("editor.background", "#000000"), ("editor.foreground", "#ffffff"), ("panel.border", "#00000000")]),
            ThemeKind::Dark,
        );
        assert_ne!(mapped["border"], "#000000");
        assert_eq!(mapped["border"], "#262626");
    }

    #[test]
    fn glass_layers_and_diffs_stay_translucent() {
        let mapped = map_colors(
            &colors(&[("editor.background", "#101010"), ("diffEditor.insertedTextBackground", "#00ff0033")]),
            ThemeKind::Dark,
        );
        assert_eq!(mapped["glassSidebar"], "#101010b3");
        assert_eq!(mapped["glassChrome"], "#101010c2");
        assert_eq!(mapped["diffInserted"], "#00ff0033");
        assert!(mapped["diffRemoved"].len() == 9, "derived diff keeps an alpha channel");
    }

    #[test]
    fn unreadable_button_label_falls_back_to_the_best_contrast() {
        let mapped = map_colors(
            &colors(&[
                ("editor.background", "#ffffff"),
                ("editor.foreground", "#111111"),
                ("textLink.foreground", "#0050c0"),
                ("button.foreground", "#0050c0"),
            ]),
            ThemeKind::Light,
        );
        assert_eq!(mapped["accentContrast"], "#ffffff");
    }

    #[test]
    fn missing_sources_fall_back_to_the_builtin_default_of_the_kind() {
        let light = map_colors(&HashMap::new(), ThemeKind::Light);
        assert_eq!(light["contentBg"], builtin_default(ThemeKind::Light)["contentBg"]);
        assert_eq!(light["toneBlue"], builtin_default(ThemeKind::Light)["toneBlue"]);
    }

    #[test]
    fn kind_prefers_ui_theme_then_type_then_background() {
        let empty = HashMap::new();
        assert_eq!(kind_of(Some("vs"), Some("dark"), &empty), ThemeKind::Light);
        assert_eq!(kind_of(Some("hc-black"), None, &empty), ThemeKind::Dark);
        assert_eq!(kind_of(Some("hc-light"), None, &empty), ThemeKind::Light);
        assert_eq!(kind_of(None, Some("hcLight"), &empty), ThemeKind::Light);
        assert_eq!(kind_of(None, None, &colors(&[("editor.background", "#fafafa")])), ThemeKind::Light);
        assert_eq!(kind_of(None, None, &empty), ThemeKind::Dark);
    }
}
