//! Guards the built-in reading themes (src/themes/builtin.json): every theme defines every
//! token with a valid color, and its text/accent/tone colors meet WCAG contrast targets on
//! the reading surface.

use std::collections::HashSet;

use serde_json::Value;

const BUILTIN: &str = include_str!("../../src/themes/builtin.json");

/// Every key of `ThemeColors` in src/themes/types.ts.
const TOKENS: &[&str] = &[
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

/// Tokens painted as solid surfaces or text; they must be fully opaque.
const OPAQUE: &[&str] = &[
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
    "brandStart",
    "brandEnd",
    "toneNeutral",
    "toneBlue",
    "toneGreen",
    "toneAmber",
    "toneRed",
    "toneViolet",
    "toneOrange",
];

const TONES: &[&str] = &[
    "toneNeutral",
    "toneBlue",
    "toneGreen",
    "toneAmber",
    "toneRed",
    "toneViolet",
    "toneOrange",
];

/// (foreground, background, minimum ratio)
const PAIRS: &[(&str, &str, f64)] = &[
    ("text", "contentBg", 7.0),
    ("textMuted", "contentBg", 4.5),
    ("textFaint", "contentBg", 3.0),
    ("accent", "contentBg", 4.5),
    ("text", "sidebar", 7.0),
    // Primary button label on the accent fill.
    ("accentContrast", "accent", 3.0),
];

fn themes() -> Vec<Value> {
    let parsed: Value = serde_json::from_str(BUILTIN).expect("builtin.json is valid JSON");
    parsed.as_array().expect("builtin.json is an array of themes").clone()
}

fn str_field<'a>(theme: &'a Value, key: &str) -> &'a str {
    theme[key]
        .as_str()
        .unwrap_or_else(|| panic!("theme field `{key}` must be a string: {theme}"))
}

/// Parses `#rrggbb` or `#rrggbbaa` into (r, g, b, a) in 0..=255.
fn parse_hex(value: &str) -> Option<[u8; 4]> {
    let hex = value.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let alpha = if hex.len() == 8 { byte(6)? } else { 255 };
    Some([byte(0)?, byte(2)?, byte(4)?, alpha])
}

fn color(theme: &Value, token: &str) -> [u8; 4] {
    let id = str_field(theme, "id");
    let raw = theme["colors"][token]
        .as_str()
        .unwrap_or_else(|| panic!("theme `{id}` is missing color `{token}`"));
    parse_hex(raw).unwrap_or_else(|| panic!("theme `{id}` color `{token}` is not #rrggbb[aa]: {raw}"))
}

/// WCAG 2.x relative luminance.
fn luminance([r, g, b, _]: [u8; 4]) -> f64 {
    let channel = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn contrast(a: [u8; 4], b: [u8; 4]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[test]
fn ships_the_required_themes_with_unique_ids() {
    let themes = themes();
    let mut ids = HashSet::new();
    for theme in &themes {
        let id = str_field(theme, "id");
        assert!(ids.insert(id.to_owned()), "duplicate theme id `{id}`");
        assert!(!str_field(theme, "name").is_empty(), "theme `{id}` needs a name");
        let kind = str_field(theme, "kind");
        assert!(kind == "light" || kind == "dark", "theme `{id}` has invalid kind `{kind}`");
    }
    for (id, kind) in [("paper", "light"), ("sepia", "light"), ("graphite", "dark"), ("midnight", "dark")] {
        let theme = themes
            .iter()
            .find(|t| t["id"] == id)
            .unwrap_or_else(|| panic!("missing built-in theme `{id}`"));
        assert_eq!(str_field(theme, "kind"), kind, "theme `{id}` must be {kind}");
    }
}

#[test]
fn defaults_are_paper_for_light_and_graphite_for_dark() {
    // Mirrors DEFAULT_SETTINGS (src/lib/settings.ts) and DEFAULT_THEME_IDS (src/themes/types.ts).
    let themes = themes();
    for (kind, default_id) in [("light", "paper"), ("dark", "graphite")] {
        let defaults: Vec<&str> = themes
            .iter()
            .filter(|t| t["kind"] == kind && t["id"] == default_id)
            .map(|t| str_field(t, "id"))
            .collect();
        assert_eq!(defaults, [default_id], "exactly one default {kind} theme `{default_id}`");
    }
}

#[test]
fn every_theme_defines_every_token_as_a_valid_color() {
    for theme in themes() {
        let id = str_field(&theme, "id");
        let colors = theme["colors"].as_object().unwrap_or_else(|| panic!("theme `{id}` needs colors"));
        for token in TOKENS {
            color(&theme, token);
        }
        for key in colors.keys() {
            assert!(TOKENS.contains(&key.as_str()), "theme `{id}` has unknown token `{key}`");
        }
        for token in OPAQUE {
            assert_eq!(color(&theme, token)[3], 255, "theme `{id}` token `{token}` must be opaque");
        }
    }
}

#[test]
fn reading_colors_meet_wcag_contrast() {
    let mut failures = Vec::new();
    for theme in themes() {
        let id = str_field(&theme, "id");
        let tone_pairs = TONES.iter().map(|tone| (*tone, "contentBg", 3.0));
        for (fg, bg, min) in PAIRS.iter().copied().chain(tone_pairs) {
            let ratio = contrast(color(&theme, fg), color(&theme, bg));
            eprintln!("{id:>10}: {fg:>14} on {bg:<9} {ratio:5.2}:1 (min {min})");
            if ratio < min {
                failures.push(format!("{id}: {fg} on {bg} is {ratio:.2}:1, needs {min}:1"));
            }
        }
    }
    assert!(failures.is_empty(), "contrast failures:\n{}", failures.join("\n"));
}

#[test]
fn contrast_math_matches_wcag_reference_values() {
    let black = parse_hex("#000000").unwrap();
    let white = parse_hex("#ffffff").unwrap();
    assert!((contrast(black, white) - 21.0).abs() < 1e-9);
    assert!((contrast(white, white) - 1.0).abs() < 1e-9);
    // #767676 on white is the classic 4.54:1 AA boundary.
    let grey = parse_hex("#767676").unwrap();
    assert!((contrast(grey, white) - 4.54).abs() < 0.01);
    assert_eq!(parse_hex("#11223380"), Some([0x11, 0x22, 0x33, 0x80]));
    assert_eq!(parse_hex("#123"), None);
}
