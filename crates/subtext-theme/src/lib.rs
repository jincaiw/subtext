//! # subtext-theme
//!
//! CotEditor themes are `.cottheme` files: a flat map of token name to colour,
//! plus a metadata block. This crate parses them, works out whether a theme is
//! light or dark, and answers the one question the editor asks — *what colour
//! is this token?*

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use subtext_syntax::HighlightToken;

/// A straight (non-premultiplied) RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse `#RGB`, `#RRGGBB` or `#RRGGBBAA`.
    pub fn parse(value: &str) -> Option<Self> {
        let hex = value.trim().trim_start_matches('#');
        let byte = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).ok();
        match hex.len() {
            3 => {
                let nibble = |index: usize| {
                    u8::from_str_radix(&hex[index..index + 1], 16).ok().map(|v| v * 17)
                };
                Some(Self::new(nibble(0)?, nibble(1)?, nibble(2)?, 255))
            }
            6 => Some(Self::new(byte(0)?, byte(2)?, byte(4)?, 255)),
            8 => Some(Self::new(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    /// Relative luminance, used to decide whether the theme is dark.
    pub fn luminance(self) -> f32 {
        let channel = |value: u8| {
            let v = value as f32 / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
    }
}

/// One colour entry, with the flag CotEditor uses to defer to the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColorEntry {
    pub color: Option<Rgba>,
    pub uses_system_setting: bool,
}

impl ColorEntry {
    /// The colour, or `fallback` when the theme defers to the system.
    pub fn resolve(self, fallback: Rgba) -> Rgba {
        if self.uses_system_setting { fallback } else { self.color.unwrap_or(fallback) }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeMetadata {
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(rename = "distributionURL", default)]
    pub distribution_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEntry {
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    uses_system_setting: bool,
}

impl RawEntry {
    fn into_entry(self) -> ColorEntry {
        ColorEntry {
            color: self.color.as_deref().and_then(Rgba::parse),
            uses_system_setting: self.uses_system_setting,
        }
    }
}

/// A parsed `.cottheme`.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub source: Option<PathBuf>,
    pub metadata: ThemeMetadata,

    pub text: ColorEntry,
    pub background: ColorEntry,
    pub selection: ColorEntry,
    pub insertion_point: ColorEntry,
    pub invisibles: ColorEntry,
    pub line_highlight: ColorEntry,
    pub highlight: ColorEntry,

    pub keywords: ColorEntry,
    pub commands: ColorEntry,
    pub types: ColorEntry,
    pub attributes: ColorEntry,
    pub variables: ColorEntry,
    pub values: ColorEntry,
    pub numbers: ColorEntry,
    pub strings: ColorEntry,
    pub characters: ColorEntry,
    pub comments: ColorEntry,

    /// Any keys we did not recognise, kept so a round-trip does not lose them.
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTheme {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    metadata: ThemeMetadata,
    #[serde(default)]
    text: Option<RawEntry>,
    #[serde(default)]
    background: Option<RawEntry>,
    #[serde(default)]
    selection: Option<RawEntry>,
    #[serde(default)]
    insertion_point: Option<RawEntry>,
    #[serde(default)]
    invisibles: Option<RawEntry>,
    #[serde(default)]
    line_highlight: Option<RawEntry>,
    #[serde(default)]
    highlight: Option<RawEntry>,
    #[serde(default)]
    keywords: Option<RawEntry>,
    #[serde(default)]
    commands: Option<RawEntry>,
    #[serde(default)]
    types: Option<RawEntry>,
    #[serde(default)]
    attributes: Option<RawEntry>,
    #[serde(default)]
    variables: Option<RawEntry>,
    #[serde(default)]
    values: Option<RawEntry>,
    #[serde(default)]
    numbers: Option<RawEntry>,
    #[serde(default)]
    strings: Option<RawEntry>,
    #[serde(default)]
    characters: Option<RawEntry>,
    #[serde(default)]
    comments: Option<RawEntry>,
}

impl Theme {
    /// Read a `.cottheme` file.
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string());
        let mut theme = Self::parse(&bytes, &name)?;
        theme.source = Some(path.to_path_buf());
        Ok(theme)
    }

    /// Parse a theme from JSON.
    pub fn parse(bytes: &[u8], fallback_name: &str) -> std::io::Result<Self> {
        let raw: RawTheme = serde_json::from_slice(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let entry = |value: Option<RawEntry>| value.map(RawEntry::into_entry).unwrap_or_default();

        Ok(Self {
            name: raw.name.unwrap_or_else(|| fallback_name.to_string()),
            source: None,
            metadata: raw.metadata,
            text: entry(raw.text),
            background: entry(raw.background),
            selection: entry(raw.selection),
            insertion_point: entry(raw.insertion_point),
            invisibles: entry(raw.invisibles),
            line_highlight: entry(raw.line_highlight),
            highlight: entry(raw.highlight),
            keywords: entry(raw.keywords),
            commands: entry(raw.commands),
            types: entry(raw.types),
            attributes: entry(raw.attributes),
            variables: entry(raw.variables),
            values: entry(raw.values),
            numbers: entry(raw.numbers),
            strings: entry(raw.strings),
            characters: entry(raw.characters),
            comments: entry(raw.comments),
            extra: BTreeMap::new(),
        })
    }

    /// The background colour, defaulting to white.
    pub fn background_color(&self) -> Rgba {
        self.background.color.unwrap_or(Rgba::new(255, 255, 255, 255))
    }

    /// The body text colour, defaulting to near-black.
    pub fn text_color(&self) -> Rgba {
        self.text.resolve(Rgba::new(0, 0, 0, 255))
    }

    /// Whether the theme reads as dark, from its background luminance.
    pub fn is_dark(&self) -> bool {
        self.background_color().luminance() < 0.5
    }

    /// The colour a highlighting token is painted in.
    pub fn token_color(&self, token: HighlightToken) -> ColorEntry {
        match token {
            HighlightToken::Attributes => self.attributes,
            HighlightToken::Characters => self.characters,
            HighlightToken::Commands => self.commands,
            HighlightToken::Comments => self.comments,
            HighlightToken::Keywords => self.keywords,
            HighlightToken::Numbers => self.numbers,
            HighlightToken::Strings => self.strings,
            HighlightToken::Types => self.types,
            HighlightToken::Values => self.values,
            HighlightToken::Variables => self.variables,
        }
    }

    /// A theme that is always available, used when no resource directory is
    /// readable — a build running from a bare binary, for instance.
    pub fn builtin_light() -> Self {
        let entry = |hex: &str| ColorEntry { color: Rgba::parse(hex), uses_system_setting: false };
        Self {
            name: "SubText Light".into(),
            source: None,
            metadata: ThemeMetadata {
                author: Some("SubText".into()),
                description: Some("Built-in fallback theme.".into()),
                license: None,
                distribution_url: None,
            },
            text: entry("#000000"),
            background: entry("#FFFFFF"),
            selection: entry("#B5D6FD"),
            insertion_point: entry("#000000"),
            invisibles: entry("#C0C0C0"),
            line_highlight: entry("#F2F2F2"),
            highlight: entry("#FFE08A"),
            keywords: entry("#0F5FA6"),
            commands: entry("#7A3E9D"),
            types: entry("#B3541E"),
            attributes: entry("#8A2F5F"),
            variables: entry("#1F7A6B"),
            values: entry("#A32D4A"),
            numbers: entry("#6A3AA0"),
            strings: entry("#4D6629"),
            characters: entry("#6B8E23"),
            comments: entry("#6B7A82"),
            extra: BTreeMap::new(),
        }
    }

    /// The dark counterpart of [`Self::builtin_light`].
    pub fn builtin_dark() -> Self {
        let entry = |hex: &str| ColorEntry { color: Rgba::parse(hex), uses_system_setting: false };
        Self {
            name: "SubText Dark".into(),
            source: None,
            metadata: ThemeMetadata {
                author: Some("SubText".into()),
                description: Some("Built-in fallback theme.".into()),
                license: None,
                distribution_url: None,
            },
            text: entry("#DCDCDC"),
            background: entry("#1E1E1E"),
            selection: entry("#3A5C8C"),
            insertion_point: entry("#E6E6E6"),
            invisibles: entry("#4A4A4A"),
            line_highlight: entry("#2A2A2A"),
            highlight: entry("#5A4A1E"),
            keywords: entry("#7DB3E8"),
            commands: entry("#C79BE8"),
            types: entry("#E8A87C"),
            attributes: entry("#E88FB8"),
            variables: entry("#7FD1C4"),
            values: entry("#E88FA0"),
            numbers: entry("#B79BE8"),
            strings: entry("#A3C48F"),
            characters: entry("#B7D18F"),
            comments: entry("#7F8C93"),
            extra: BTreeMap::new(),
        }
    }
}

/// Every theme SubText can offer.
#[derive(Debug, Clone, Default)]
pub struct ThemeManager {
    themes: Vec<Theme>,
}

impl ThemeManager {
    /// Load every `.cottheme` in `directory`, sorted by name.
    pub fn load_from_directory(directory: impl AsRef<Path>) -> std::io::Result<Self> {
        let mut themes = Vec::new();
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "cottheme") {
                continue;
            }
            if let Ok(theme) = Theme::load(&path) {
                themes.push(theme);
            }
        }
        themes.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self { themes })
    }

    /// A manager holding only the two built-in themes.
    pub fn builtin() -> Self {
        Self { themes: vec![Theme::builtin_light(), Theme::builtin_dark()] }
    }

    /// A manager over themes that were already collected.
    ///
    /// Sorted by name so the Format ▸ Theme menu reads the same however the
    /// themes arrived — shipped, from the user's directory, or both.
    pub fn from_themes(mut themes: Vec<Theme>) -> Self {
        themes.sort_by(|a, b| a.name.cmp(&b.name));
        themes.dedup_by(|a, b| a.name == b.name);
        Self { themes }
    }

    pub fn all(&self) -> &[Theme] {
        &self.themes
    }

    pub fn by_name(&self, name: &str) -> Option<&Theme> {
        self.themes
            .iter()
            .find(|t| t.name == name)
            .or_else(|| self.themes.iter().find(|t| t.name.eq_ignore_ascii_case(name)))
    }

    /// The default theme: a light one if there is one, otherwise the first.
    pub fn default_theme(&self) -> &Theme {
        self.themes
            .iter()
            .find(|t| t.name == "Anura")
            .or_else(|| self.themes.iter().find(|t| !t.is_dark()))
            .or_else(|| self.themes.first())
            .unwrap_or_else(|| {
                // Only reachable when constructed by hand with no themes.
                static FALLBACK: std::sync::OnceLock<Theme> = std::sync::OnceLock::new();
                FALLBACK.get_or_init(Theme::builtin_light)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resources() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/themes")
    }

    #[test]
    fn parses_six_and_eight_digit_hex() {
        assert_eq!(Rgba::parse("#FFFFFF"), Some(Rgba::new(255, 255, 255, 255)));
        assert_eq!(Rgba::parse("ff0000"), Some(Rgba::new(255, 0, 0, 255)));
        assert_eq!(Rgba::parse("#00000080"), Some(Rgba::new(0, 0, 0, 0x80)));
        assert_eq!(Rgba::parse("#abc"), Some(Rgba::new(0xAA, 0xBB, 0xCC, 255)));
        assert_eq!(Rgba::parse("nope"), None);
    }

    #[test]
    fn loads_every_bundled_theme() {
        let manager = ThemeManager::load_from_directory(resources()).unwrap();
        assert_eq!(manager.all().len(), 14, "expected the full CotEditor theme set");
        assert!(manager.by_name("Anura").is_some());
    }

    #[test]
    fn dark_themes_are_recognised() {
        let manager = ThemeManager::load_from_directory(resources()).unwrap();
        assert!(!manager.by_name("Anura").unwrap().is_dark());
        assert!(manager.by_name("Anura (Dark)").unwrap().is_dark());
    }

    #[test]
    fn token_colors_come_from_the_right_key() {
        let manager = ThemeManager::load_from_directory(resources()).unwrap();
        let theme = manager.by_name("Anura").unwrap();
        assert_eq!(theme.token_color(HighlightToken::Keywords).color, Rgba::parse("#1b556b"));
        assert_eq!(theme.token_color(HighlightToken::Comments).color, Rgba::parse("#66747a"));
        assert_eq!(theme.token_color(HighlightToken::Strings).color, Rgba::parse("#4d6629"));
    }

    #[test]
    fn system_setting_entries_fall_back() {
        let manager = ThemeManager::load_from_directory(resources()).unwrap();
        let theme = manager.by_name("Anura").unwrap();
        // `selection` sets usesSystemSetting, so the fallback wins.
        let fallback = Rgba::new(1, 2, 3, 255);
        assert_eq!(theme.selection.resolve(fallback), fallback);
        assert!(theme.selection.color.is_some());
    }

    #[test]
    fn builtin_themes_are_usable_without_resources() {
        let light = Theme::builtin_light();
        let dark = Theme::builtin_dark();
        assert!(!light.is_dark());
        assert!(dark.is_dark());
        assert_ne!(light.text_color(), dark.text_color());
    }

    #[test]
    fn malformed_theme_is_an_error_not_a_panic() {
        assert!(Theme::parse(b"{ not json", "broken").is_err());
    }
}
