//! Hex color parsing and mixing for imported themes. Only strict `#rrggbb` / `#rrggbbaa`
//! strings leave this module, because they are written straight into CSS variables.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    /// Parses `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` (any case, surrounding spaces ignored).
    pub fn parse(input: &str) -> Option<Self> {
        let hex = input.trim().strip_prefix('#')?;
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let digits: Vec<u8> = hex.chars().map(|c| c.to_digit(16).unwrap_or(0) as u8).collect();
        let (r, g, b, a) = match digits.len() {
            3 => (digits[0] * 17, digits[1] * 17, digits[2] * 17, 255),
            4 => (digits[0] * 17, digits[1] * 17, digits[2] * 17, digits[3] * 17),
            6 | 8 => {
                let pair = |i: usize| digits[i] * 16 + digits[i + 1];
                (pair(0), pair(2), pair(4), if digits.len() == 8 { pair(6) } else { 255 })
            }
            _ => return None,
        };
        Some(Self { r, g, b, a })
    }

    pub fn is_opaque(self) -> bool {
        self.a == 255
    }

    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// Composites this color over an opaque background, returning an opaque color.
    pub fn over(self, background: Rgba) -> Rgba {
        let alpha = f64::from(self.a) / 255.0;
        let blend = |fg: u8, bg: u8| (f64::from(fg) * alpha + f64::from(bg) * (1.0 - alpha)).round() as u8;
        Rgba { r: blend(self.r, background.r), g: blend(self.g, background.g), b: blend(self.b, background.b), a: 255 }
    }

    /// Linear mix toward `other` by `amount` (0 keeps self, 1 returns other), alpha of self.
    pub fn mix(self, other: Rgba, amount: f64) -> Rgba {
        let t = amount.clamp(0.0, 1.0);
        let blend = |a: u8, b: u8| (f64::from(a) * (1.0 - t) + f64::from(b) * t).round() as u8;
        Rgba { r: blend(self.r, other.r), g: blend(self.g, other.g), b: blend(self.b, other.b), a: self.a }
    }

    /// WCAG relative luminance.
    pub fn luminance(self) -> f64 {
        let channel = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// WCAG contrast ratio between two opaque colors.
    pub fn contrast(self, other: Rgba) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// `#rrggbb` when opaque, `#rrggbbaa` otherwise.
    pub fn to_hex(self) -> String {
        if self.is_opaque() {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

/// True for the only color syntax imported themes may carry: lowercase `#rrggbb` or `#rrggbbaa`.
pub fn is_strict_hex(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else { return false };
    matches!(hex.len(), 6 | 8) && hex.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_hex_form() {
        assert_eq!(Rgba::parse("#fff"), Some(Rgba { r: 255, g: 255, b: 255, a: 255 }));
        assert_eq!(Rgba::parse("#0000"), Some(Rgba { r: 0, g: 0, b: 0, a: 0 }));
        assert_eq!(Rgba::parse(" #1E1E2E "), Some(Rgba { r: 0x1e, g: 0x1e, b: 0x2e, a: 255 }));
        assert_eq!(Rgba::parse("#1e1e2e80"), Some(Rgba { r: 0x1e, g: 0x1e, b: 0x2e, a: 0x80 }));
    }

    #[test]
    fn rejects_anything_that_is_not_hex() {
        for bad in ["", "fff", "#ff", "#fffff", "#gggggg", "red", "rgb(0 0 0)", "#ffffff;}", "#ffffffff00"] {
            assert_eq!(Rgba::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn to_hex_is_lowercase_and_drops_full_alpha() {
        assert_eq!(Rgba::parse("#ABCDEF").unwrap().to_hex(), "#abcdef");
        assert_eq!(Rgba::parse("#ABCDEF80").unwrap().to_hex(), "#abcdef80");
        assert!(is_strict_hex(&Rgba::parse("#fff").unwrap().to_hex()));
    }

    #[test]
    fn over_composites_alpha_onto_the_background() {
        let half_white = Rgba::parse("#ffffff80").unwrap();
        let black = Rgba::parse("#000000").unwrap();
        assert_eq!(half_white.over(black).to_hex(), "#808080");
    }

    #[test]
    fn strict_hex_only_accepts_lowercase_long_forms() {
        assert!(is_strict_hex("#a1b2c3"));
        assert!(is_strict_hex("#a1b2c3d4"));
        assert!(!is_strict_hex("#A1B2C3"));
        assert!(!is_strict_hex("#fff"));
        assert!(!is_strict_hex("#a1b2c3;color:red"));
    }

    #[test]
    fn contrast_of_black_on_white_is_21() {
        let ratio = Rgba::parse("#000").unwrap().contrast(Rgba::parse("#fff").unwrap());
        assert!((ratio - 21.0).abs() < 0.01);
    }
}
