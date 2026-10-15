//! Character inspection — the data behind CotEditor's **Inspect Character**
//! panel, the **Document Inspector**'s Character section, and the character
//! counts that both of those and the status bar are built from.

use unicode_properties::UnicodeGeneralCategory;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

use std::sync::LazyLock;

use regex::Regex;

use crate::encoding::TextEncoding;
use crate::textops::{self, Normalization};

/// The Unicode **General_Category** of a scalar value.
///
/// The variant names are CotEditor's: they are Swift's
/// `Unicode.GeneralCategory` cases, and [`Self::long_name`] / [`Self::short_name`]
/// are its two `switch`es transcribed entry for entry.
///
/// The value comes from `unicode-properties`, which carries the real
/// `General_Category` table. `char`'s own predicates cannot answer this
/// question: `char::is_numeric` is true for `Ⅻ` (`Nl`, a letter number) and for
/// `½` (`No`, an other number), and `char::is_alphabetic` is true for both
/// letters and the letter-like marks Unicode files elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeneralCategory {
    UppercaseLetter,
    LowercaseLetter,
    TitlecaseLetter,
    ModifierLetter,
    OtherLetter,
    NonspacingMark,
    SpacingMark,
    EnclosingMark,
    DecimalNumber,
    LetterNumber,
    OtherNumber,
    ConnectorPunctuation,
    DashPunctuation,
    OpenPunctuation,
    ClosePunctuation,
    InitialPunctuation,
    FinalPunctuation,
    OtherPunctuation,
    MathSymbol,
    CurrencySymbol,
    ModifierSymbol,
    OtherSymbol,
    SpaceSeparator,
    LineSeparator,
    ParagraphSeparator,
    Control,
    Format,
    Surrogate,
    PrivateUse,
    Unassigned,
}

impl GeneralCategory {
    /// The category of `c`.
    pub fn of(c: char) -> Self {
        match c.general_category() {
            unicode_properties::GeneralCategory::UppercaseLetter => Self::UppercaseLetter,
            unicode_properties::GeneralCategory::LowercaseLetter => Self::LowercaseLetter,
            unicode_properties::GeneralCategory::TitlecaseLetter => Self::TitlecaseLetter,
            unicode_properties::GeneralCategory::ModifierLetter => Self::ModifierLetter,
            unicode_properties::GeneralCategory::OtherLetter => Self::OtherLetter,
            unicode_properties::GeneralCategory::NonspacingMark => Self::NonspacingMark,
            unicode_properties::GeneralCategory::SpacingMark => Self::SpacingMark,
            unicode_properties::GeneralCategory::EnclosingMark => Self::EnclosingMark,
            unicode_properties::GeneralCategory::DecimalNumber => Self::DecimalNumber,
            unicode_properties::GeneralCategory::LetterNumber => Self::LetterNumber,
            unicode_properties::GeneralCategory::OtherNumber => Self::OtherNumber,
            unicode_properties::GeneralCategory::ConnectorPunctuation => Self::ConnectorPunctuation,
            unicode_properties::GeneralCategory::DashPunctuation => Self::DashPunctuation,
            unicode_properties::GeneralCategory::OpenPunctuation => Self::OpenPunctuation,
            unicode_properties::GeneralCategory::ClosePunctuation => Self::ClosePunctuation,
            unicode_properties::GeneralCategory::InitialPunctuation => Self::InitialPunctuation,
            unicode_properties::GeneralCategory::FinalPunctuation => Self::FinalPunctuation,
            unicode_properties::GeneralCategory::OtherPunctuation => Self::OtherPunctuation,
            unicode_properties::GeneralCategory::MathSymbol => Self::MathSymbol,
            unicode_properties::GeneralCategory::CurrencySymbol => Self::CurrencySymbol,
            unicode_properties::GeneralCategory::ModifierSymbol => Self::ModifierSymbol,
            unicode_properties::GeneralCategory::OtherSymbol => Self::OtherSymbol,
            unicode_properties::GeneralCategory::SpaceSeparator => Self::SpaceSeparator,
            unicode_properties::GeneralCategory::LineSeparator => Self::LineSeparator,
            unicode_properties::GeneralCategory::ParagraphSeparator => Self::ParagraphSeparator,
            unicode_properties::GeneralCategory::Control => Self::Control,
            unicode_properties::GeneralCategory::Format => Self::Format,
            unicode_properties::GeneralCategory::Surrogate => Self::Surrogate,
            unicode_properties::GeneralCategory::PrivateUse => Self::PrivateUse,
            unicode_properties::GeneralCategory::Unassigned => Self::Unassigned,
        }
    }

    /// The long value alias, e.g. `Uppercase Letter`.
    pub fn long_name(self) -> &'static str {
        match self {
            Self::UppercaseLetter => "Uppercase Letter",
            Self::LowercaseLetter => "Lowercase Letter",
            Self::TitlecaseLetter => "Titlecase Letter",
            Self::ModifierLetter => "Modifier Letter",
            Self::OtherLetter => "Other Letter",
            Self::NonspacingMark => "Nonspacing Mark",
            Self::SpacingMark => "Spacing Mark",
            Self::EnclosingMark => "Enclosing Mark",
            Self::DecimalNumber => "Decimal Number",
            Self::LetterNumber => "Letter Number",
            Self::OtherNumber => "Other Number",
            Self::ConnectorPunctuation => "Connector Punctuation",
            Self::DashPunctuation => "Dash Punctuation",
            Self::OpenPunctuation => "Open Punctuation",
            Self::ClosePunctuation => "Close Punctuation",
            Self::InitialPunctuation => "Initial Punctuation",
            Self::FinalPunctuation => "Final Punctuation",
            Self::OtherPunctuation => "Other Punctuation",
            Self::MathSymbol => "Math Symbol",
            Self::CurrencySymbol => "Currency Symbol",
            Self::ModifierSymbol => "Modifier Symbol",
            Self::OtherSymbol => "Other Symbol",
            Self::SpaceSeparator => "Space Separator",
            Self::LineSeparator => "Line Separator",
            Self::ParagraphSeparator => "Paragraph Separator",
            Self::Control => "Control",
            Self::Format => "Format",
            Self::Surrogate => "Surrogate",
            Self::PrivateUse => "Private Use",
            Self::Unassigned => "Unassigned",
        }
    }

    /// The short value alias, e.g. `Lu`.
    pub fn short_name(self) -> &'static str {
        match self {
            Self::UppercaseLetter => "Lu",
            Self::LowercaseLetter => "Ll",
            Self::TitlecaseLetter => "Lt",
            Self::ModifierLetter => "Lm",
            Self::OtherLetter => "Lo",
            Self::NonspacingMark => "Mn",
            Self::SpacingMark => "Mc",
            Self::EnclosingMark => "Me",
            Self::DecimalNumber => "Nd",
            Self::LetterNumber => "Nl",
            Self::OtherNumber => "No",
            Self::ConnectorPunctuation => "Pc",
            Self::DashPunctuation => "Pd",
            Self::OpenPunctuation => "Ps",
            Self::ClosePunctuation => "Pe",
            Self::InitialPunctuation => "Pi",
            Self::FinalPunctuation => "Pf",
            Self::OtherPunctuation => "Po",
            Self::MathSymbol => "Sm",
            Self::CurrencySymbol => "Sc",
            Self::ModifierSymbol => "Sk",
            Self::OtherSymbol => "So",
            Self::SpaceSeparator => "Zs",
            Self::LineSeparator => "Zl",
            Self::ParagraphSeparator => "Zp",
            Self::Control => "Cc",
            Self::Format => "Cf",
            Self::Surrogate => "Cs",
            Self::PrivateUse => "Co",
            Self::Unassigned => "Cn",
        }
    }
}

impl std::fmt::Display for GeneralCategory {
    /// `Uppercase Letter (Lu)` — the shape both CotEditor panels use, written
    /// once here so the two of them cannot drift.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.long_name(), self.short_name())
    }
}

/// Everything both CotEditor panels show about one **scalar value**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarInfo {
    /// The scalar's value.
    pub value: u32,
    /// `U+1F600` — always at least four digits, uppercase.
    pub code_point: String,
    /// The Unicode name; the conventional name of a control character where
    /// Unicode leaves those unnamed; and `None` for the scalars that have
    /// neither — the private-use areas and the unassigned planes. Upstream's
    /// `Unicode.Scalar.name` is optional too, and both panels render its `nil`
    /// as an empty value rather than a stand-in.
    pub name: Option<String>,
    /// The Unicode block, or `None` where Unicode defines none. See
    /// [`block_name`].
    pub block: Option<&'static str>,
    pub category: GeneralCategory,
    /// The glyph to draw in place of an invisible control character, from the
    /// Unicode Control Pictures block — space is `␠`, DELETE is `␡`. Upstream's
    /// `Unicode.Scalar.pictureRepresentation`, and the reason its big glyph is
    /// drawn in the secondary colour for a control character.
    pub picture: Option<char>,
    /// `Unicode.Scalar.Properties.isDeprecated`.
    pub is_deprecated: bool,
    /// The Unicode version that assigned the scalar, as `(major, minor)`.
    /// `None` for a scalar Unicode has not assigned.
    pub age: Option<(u8, u8)>,
    /// What the scalar varies, when it is a variation selector or a skin-tone
    /// modifier: `Text Style`, `Emoji Style`, `Skin Tone III`, `Variant`.
    /// Upstream's `Unicode.Scalar.variantDescription`.
    pub variant_description: Option<&'static str>,
    /// The UTF-16 surrogate pair as two code points, for a scalar outside the
    /// Basic Multilingual Plane.
    pub surrogate_code_points: Option<(String, String)>,
}

impl ScalarInfo {
    pub fn of(c: char) -> Self {
        let value = c as u32;
        Self {
            value,
            code_point: format!("U+{value:04X}"),
            name: unicode_names2::name(c)
                .map(|name| name.to_string())
                .or_else(|| control_character_name(value)),
            block: block_name(value),
            category: GeneralCategory::of(c),
            picture: picture_representation(value),
            is_deprecated: crate::unicode_tables::is_deprecated(value),
            age: crate::unicode_tables::age(value),
            variant_description: variant_description(value),
            surrogate_code_points: surrogate_code_points(c),
        }
    }

    /// The name to display, which is the official name or nothing.
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or_default()
    }

    /// The block to display, which is the localized name or "No Block".
    pub fn display_block(&self) -> &str {
        self.block.map_or("No Block", display_block_name)
    }

    /// The Code Point row: the code point, with the surrogate pair beside it
    /// when the scalar needs one — upstream writes `U+1F600 (D83D DE00)`.
    pub fn code_point_with_surrogates(&self) -> String {
        match &self.surrogate_code_points {
            Some((lead, trail)) => format!("{} ({lead} {trail})", self.code_point),
            None => self.code_point.clone(),
        }
    }

    /// The Version row: `Unicode 1.1`, or nothing where Unicode has not
    /// assigned the scalar.
    pub fn version(&self) -> Option<String> {
        self.age.map(|(major, minor)| format!("Unicode {major}.{minor}"))
    }
}

/// The control-picture glyph for an invisible control character.
///
/// Upstream's `Unicode.Scalar.pictureRepresentation`: the C0 range plus space
/// map into the Control Pictures block at a fixed offset, and DELETE has a
/// picture of its own.
fn picture_representation(value: u32) -> Option<char> {
    match value {
        0x0000..=0x0020 => char::from_u32(value + 0x2400),
        0x007F => char::from_u32(0x2421),
        _ => None,
    }
}

/// What a variation selector or a skin-tone modifier varies.
fn variant_description(value: u32) -> Option<&'static str> {
    match value {
        0xFE0E => Some("Text Style"),
        0xFE0F => Some("Emoji Style"),
        0x1F3FB => Some("Skin Tone I-II"),
        0x1F3FC => Some("Skin Tone III"),
        0x1F3FD => Some("Skin Tone IV"),
        0x1F3FE => Some("Skin Tone V"),
        0x1F3FF => Some("Skin Tone VI"),
        _ if crate::unicode_tables::is_variation_selector(value) => Some("Variant"),
        _ => None,
    }
}

/// The surrogate pair of a scalar outside the Basic Multilingual Plane.
///
/// `None` for a scalar that fits in one UTF-16 code unit — upstream's
/// `isSurrogatePair`, which is what decides whether the Code Point row shows a
/// pair at all.
fn surrogate_code_points(c: char) -> Option<(String, String)> {
    let mut units = [0u16; 2];
    match *c.encode_utf16(&mut units) {
        [lead, trail] => Some((format!("{lead:04X}"), format!("{trail:04X}"))),
        _ => None,
    }
}

/// The **Character** section of the document inspector, and the whole of the
/// Inspect Character panel.
///
/// Upstream keeps `EditorCounter.Result.character`: the first selected
/// *character* — one extended grapheme cluster — and only while the selection
/// is exactly one of them. Both surfaces are drawn from that character's
/// scalars, and both show nothing at all when there are none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterReport {
    /// The character itself, as text. Empty when the selection is not one
    /// character.
    pub character: String,
    /// One entry per scalar value, in order. Empty means "not selected", which
    /// is what an empty selection, a multi-character selection and a selection
    /// that does not sit on character boundaries all give.
    pub scalars: Vec<ScalarInfo>,
}

impl CharacterReport {
    /// The report for the selected text, or an empty one when that text is not
    /// exactly one character.
    pub fn of(selected: &str) -> Self {
        let mut characters = selected.graphemes(true);
        match (characters.next(), characters.next()) {
            (Some(character), None) => Self {
                character: character.to_string(),
                scalars: character.chars().map(ScalarInfo::of).collect(),
            },
            _ => Self::default(),
        }
    }

    /// Whether the section has anything to show.
    pub fn is_empty(&self) -> bool {
        self.scalars.is_empty()
    }

    /// Whether the section's Name / Block / Category rows apply.
    ///
    /// Upstream asks `scalars.count == 1`: for a character built from several
    /// scalars those three rows have no single answer, so it draws only the
    /// code points.
    pub fn is_single_scalar(&self) -> bool {
        self.scalars.len() == 1
    }

    /// The code points, in order, as the section's Code Point(s) row lists them.
    pub fn code_points(&self) -> Vec<&str> {
        self.scalars.iter().map(|scalar| scalar.code_point.as_str()).collect()
    }

    /// Whether the character is one scalar wearing a variant selector.
    ///
    /// Upstream's `Character.isVariant`: exactly two scalars, the second a
    /// variation selector or a skin-tone modifier. Such a character is *not*
    /// "complex" — it has a name of its own, with the variant in parentheses.
    pub fn is_variant(&self) -> bool {
        self.scalars.len() == 2 && self.scalars[1].variant_description.is_some()
    }

    /// Whether the character is several scalars and not a variant.
    ///
    /// Upstream's `Character.isComplex`. Such a character has no name, and the
    /// panel says how many scalars it is made of instead.
    pub fn is_complex(&self) -> bool {
        self.scalars.len() > 1 && !self.is_variant()
    }

    /// The headline: the character's name, with its variant in parentheses, or
    /// a note that it is several characters, or nothing when Unicode names none
    /// of them. Upstream's `Character.localizedDescription`.
    pub fn description(&self) -> Option<String> {
        if self.is_complex() {
            return Some(format!("<a letter consisting of {} characters>", self.scalars.len()));
        }
        let name = self.scalars.first()?.name.as_deref()?;
        Some(match self.scalars.last().and_then(|scalar| scalar.variant_description) {
            Some(variant) => format!("{name} ({variant})"),
            None => name.to_string(),
        })
    }

    /// The big glyph the panel draws beside the detail.
    ///
    /// Upstream's `Character.pictureCharacter`: a control character is drawn as
    /// its Control Picture rather than as an invisible glyph — and only a
    /// single-scalar character has one, which is the `ignore CRLF` note in its
    /// source. `None` means "draw the character itself".
    pub fn picture(&self) -> Option<char> {
        if self.scalars.len() == 1 { self.scalars[0].picture } else { None }
    }
}

/// Everything the Inspect Character panel shows about one character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterInfo {
    pub character: char,
    /// The Unicode data, shared with the document inspector's Character
    /// section — one implementation of the name, block and category rules.
    pub scalar: ScalarInfo,
    /// UTF-8 bytes as hex, space separated.
    pub utf8: String,
    /// UTF-16 code units as hex, space separated.
    pub utf16: String,
    /// A numeric HTML entity, e.g. `&#x1F600;`.
    pub html_entity: String,
    /// How many bytes this character occupies in a given encoding.
    pub encoded_len: usize,
    pub encoding_name: &'static str,
    /// Display width: 0 for combining marks, 2 for wide CJK.
    pub width: usize,
}

impl CharacterInfo {
    pub fn new(character: char, encoding: TextEncoding) -> Self {
        let encoded_len = encoding.encode(&character.to_string()).0.len();
        let utf16: Vec<String> = character
            .encode_utf16(&mut [0u16; 2])
            .iter()
            .map(|u| format!("{u:04X}"))
            .collect();
        let utf8: Vec<String> =
            character.to_string().as_bytes().iter().map(|b| format!("{b:02X}")).collect();

        Self {
            character,
            scalar: ScalarInfo::of(character),
            utf8: utf8.join(" "),
            utf16: utf16.join(" "),
            html_entity: format!("&#x{:X};", character as u32),
            encoded_len,
            encoding_name: encoding.name,
            width: UnicodeWidthChar::width(character).unwrap_or(0),
        }
    }

    /// The value in decimal.
    pub fn decimal(&self) -> u32 {
        self.scalar.value
    }

    pub fn code_point(&self) -> &str {
        &self.scalar.code_point
    }
}

/// The Unicode block ranges, in ascending order and non-overlapping.
///
/// Based on Unicode 17.0.0 (2025-08-01), cf. `Blocks.txt` — the same revision
/// CotEditor's table is built from, transcribed range for range. There are gaps
/// between the blocks; those are the values Unicode has not assigned a block.
const BLOCKS: &[(u32, u32, &str)] = &[
    (0x0000, 0x007F, "Basic Latin"),
    (0x0080, 0x00FF, "Latin-1 Supplement"),
    (0x0100, 0x017F, "Latin Extended-A"),
    (0x0180, 0x024F, "Latin Extended-B"),
    (0x0250, 0x02AF, "IPA Extensions"),
    (0x02B0, 0x02FF, "Spacing Modifier Letters"),
    (0x0300, 0x036F, "Combining Diacritical Marks"),
    (0x0370, 0x03FF, "Greek and Coptic"),
    (0x0400, 0x04FF, "Cyrillic"),
    (0x0500, 0x052F, "Cyrillic Supplement"),
    (0x0530, 0x058F, "Armenian"),
    (0x0590, 0x05FF, "Hebrew"),
    (0x0600, 0x06FF, "Arabic"),
    (0x0700, 0x074F, "Syriac"),
    (0x0750, 0x077F, "Arabic Supplement"),
    (0x0780, 0x07BF, "Thaana"),
    (0x07C0, 0x07FF, "NKo"),
    (0x0800, 0x083F, "Samaritan"),
    (0x0840, 0x085F, "Mandaic"),
    (0x0860, 0x086F, "Syriac Supplement"),
    (0x0870, 0x089F, "Arabic Extended-B"),
    (0x08A0, 0x08FF, "Arabic Extended-A"),
    (0x0900, 0x097F, "Devanagari"),
    (0x0980, 0x09FF, "Bengali"),
    (0x0A00, 0x0A7F, "Gurmukhi"),
    (0x0A80, 0x0AFF, "Gujarati"),
    (0x0B00, 0x0B7F, "Oriya"),
    (0x0B80, 0x0BFF, "Tamil"),
    (0x0C00, 0x0C7F, "Telugu"),
    (0x0C80, 0x0CFF, "Kannada"),
    (0x0D00, 0x0D7F, "Malayalam"),
    (0x0D80, 0x0DFF, "Sinhala"),
    (0x0E00, 0x0E7F, "Thai"),
    (0x0E80, 0x0EFF, "Lao"),
    (0x0F00, 0x0FFF, "Tibetan"),
    (0x1000, 0x109F, "Myanmar"),
    (0x10A0, 0x10FF, "Georgian"),
    (0x1100, 0x11FF, "Hangul Jamo"),
    (0x1200, 0x137F, "Ethiopic"),
    (0x1380, 0x139F, "Ethiopic Supplement"),
    (0x13A0, 0x13FF, "Cherokee"),
    (0x1400, 0x167F, "Unified Canadian Aboriginal Syllabics"),
    (0x1680, 0x169F, "Ogham"),
    (0x16A0, 0x16FF, "Runic"),
    (0x1700, 0x171F, "Tagalog"),
    (0x1720, 0x173F, "Hanunoo"),
    (0x1740, 0x175F, "Buhid"),
    (0x1760, 0x177F, "Tagbanwa"),
    (0x1780, 0x17FF, "Khmer"),
    (0x1800, 0x18AF, "Mongolian"),
    (0x18B0, 0x18FF, "Unified Canadian Aboriginal Syllabics Extended"),
    (0x1900, 0x194F, "Limbu"),
    (0x1950, 0x197F, "Tai Le"),
    (0x1980, 0x19DF, "New Tai Lue"),
    (0x19E0, 0x19FF, "Khmer Symbols"),
    (0x1A00, 0x1A1F, "Buginese"),
    (0x1A20, 0x1AAF, "Tai Tham"),
    (0x1AB0, 0x1AFF, "Combining Diacritical Marks Extended"),
    (0x1B00, 0x1B7F, "Balinese"),
    (0x1B80, 0x1BBF, "Sundanese"),
    (0x1BC0, 0x1BFF, "Batak"),
    (0x1C00, 0x1C4F, "Lepcha"),
    (0x1C50, 0x1C7F, "Ol Chiki"),
    (0x1C80, 0x1C8F, "Cyrillic Extended-C"),
    (0x1C90, 0x1CBF, "Georgian Extended"),
    (0x1CC0, 0x1CCF, "Sundanese Supplement"),
    (0x1CD0, 0x1CFF, "Vedic Extensions"),
    (0x1D00, 0x1D7F, "Phonetic Extensions"),
    (0x1D80, 0x1DBF, "Phonetic Extensions Supplement"),
    (0x1DC0, 0x1DFF, "Combining Diacritical Marks Supplement"),
    (0x1E00, 0x1EFF, "Latin Extended Additional"),
    (0x1F00, 0x1FFF, "Greek Extended"),
    (0x2000, 0x206F, "General Punctuation"),
    (0x2070, 0x209F, "Superscripts and Subscripts"),
    (0x20A0, 0x20CF, "Currency Symbols"),
    (0x20D0, 0x20FF, "Combining Diacritical Marks for Symbols"),
    (0x2100, 0x214F, "Letterlike Symbols"),
    (0x2150, 0x218F, "Number Forms"),
    (0x2190, 0x21FF, "Arrows"),
    (0x2200, 0x22FF, "Mathematical Operators"),
    (0x2300, 0x23FF, "Miscellaneous Technical"),
    (0x2400, 0x243F, "Control Pictures"),
    (0x2440, 0x245F, "Optical Character Recognition"),
    (0x2460, 0x24FF, "Enclosed Alphanumerics"),
    (0x2500, 0x257F, "Box Drawing"),
    (0x2580, 0x259F, "Block Elements"),
    (0x25A0, 0x25FF, "Geometric Shapes"),
    (0x2600, 0x26FF, "Miscellaneous Symbols"),
    (0x2700, 0x27BF, "Dingbats"),
    (0x27C0, 0x27EF, "Miscellaneous Mathematical Symbols-A"),
    (0x27F0, 0x27FF, "Supplemental Arrows-A"),
    (0x2800, 0x28FF, "Braille Patterns"),
    (0x2900, 0x297F, "Supplemental Arrows-B"),
    (0x2980, 0x29FF, "Miscellaneous Mathematical Symbols-B"),
    (0x2A00, 0x2AFF, "Supplemental Mathematical Operators"),
    (0x2B00, 0x2BFF, "Miscellaneous Symbols and Arrows"),
    (0x2C00, 0x2C5F, "Glagolitic"),
    (0x2C60, 0x2C7F, "Latin Extended-C"),
    (0x2C80, 0x2CFF, "Coptic"),
    (0x2D00, 0x2D2F, "Georgian Supplement"),
    (0x2D30, 0x2D7F, "Tifinagh"),
    (0x2D80, 0x2DDF, "Ethiopic Extended"),
    (0x2DE0, 0x2DFF, "Cyrillic Extended-A"),
    (0x2E00, 0x2E7F, "Supplemental Punctuation"),
    (0x2E80, 0x2EFF, "CJK Radicals Supplement"),
    (0x2F00, 0x2FDF, "Kangxi Radicals"),
    (0x2FF0, 0x2FFF, "Ideographic Description Characters"),
    (0x3000, 0x303F, "CJK Symbols and Punctuation"),
    (0x3040, 0x309F, "Hiragana"),
    (0x30A0, 0x30FF, "Katakana"),
    (0x3100, 0x312F, "Bopomofo"),
    (0x3130, 0x318F, "Hangul Compatibility Jamo"),
    (0x3190, 0x319F, "Kanbun"),
    (0x31A0, 0x31BF, "Bopomofo Extended"),
    (0x31C0, 0x31EF, "CJK Strokes"),
    (0x31F0, 0x31FF, "Katakana Phonetic Extensions"),
    (0x3200, 0x32FF, "Enclosed CJK Letters and Months"),
    (0x3300, 0x33FF, "CJK Compatibility"),
    (0x3400, 0x4DBF, "CJK Unified Ideographs Extension A"),
    (0x4DC0, 0x4DFF, "Yijing Hexagram Symbols"),
    (0x4E00, 0x9FFF, "CJK Unified Ideographs"),
    (0xA000, 0xA48F, "Yi Syllables"),
    (0xA490, 0xA4CF, "Yi Radicals"),
    (0xA4D0, 0xA4FF, "Lisu"),
    (0xA500, 0xA63F, "Vai"),
    (0xA640, 0xA69F, "Cyrillic Extended-B"),
    (0xA6A0, 0xA6FF, "Bamum"),
    (0xA700, 0xA71F, "Modifier Tone Letters"),
    (0xA720, 0xA7FF, "Latin Extended-D"),
    (0xA800, 0xA82F, "Syloti Nagri"),
    (0xA830, 0xA83F, "Common Indic Number Forms"),
    (0xA840, 0xA87F, "Phags-pa"),
    (0xA880, 0xA8DF, "Saurashtra"),
    (0xA8E0, 0xA8FF, "Devanagari Extended"),
    (0xA900, 0xA92F, "Kayah Li"),
    (0xA930, 0xA95F, "Rejang"),
    (0xA960, 0xA97F, "Hangul Jamo Extended-A"),
    (0xA980, 0xA9DF, "Javanese"),
    (0xA9E0, 0xA9FF, "Myanmar Extended-B"),
    (0xAA00, 0xAA5F, "Cham"),
    (0xAA60, 0xAA7F, "Myanmar Extended-A"),
    (0xAA80, 0xAADF, "Tai Viet"),
    (0xAAE0, 0xAAFF, "Meetei Mayek Extensions"),
    (0xAB00, 0xAB2F, "Ethiopic Extended-A"),
    (0xAB30, 0xAB6F, "Latin Extended-E"),
    (0xAB70, 0xABBF, "Cherokee Supplement"),
    (0xABC0, 0xABFF, "Meetei Mayek"),
    (0xAC00, 0xD7AF, "Hangul Syllables"),
    (0xD7B0, 0xD7FF, "Hangul Jamo Extended-B"),
    (0xD800, 0xDB7F, "High Surrogates"),
    (0xDB80, 0xDBFF, "High Private Use Surrogates"),
    (0xDC00, 0xDFFF, "Low Surrogates"),
    (0xE000, 0xF8FF, "Private Use Area"),
    (0xF900, 0xFAFF, "CJK Compatibility Ideographs"),
    (0xFB00, 0xFB4F, "Alphabetic Presentation Forms"),
    (0xFB50, 0xFDFF, "Arabic Presentation Forms-A"),
    (0xFE00, 0xFE0F, "Variation Selectors"),
    (0xFE10, 0xFE1F, "Vertical Forms"),
    (0xFE20, 0xFE2F, "Combining Half Marks"),
    (0xFE30, 0xFE4F, "CJK Compatibility Forms"),
    (0xFE50, 0xFE6F, "Small Form Variants"),
    (0xFE70, 0xFEFF, "Arabic Presentation Forms-B"),
    (0xFF00, 0xFFEF, "Halfwidth and Fullwidth Forms"),
    (0xFFF0, 0xFFFF, "Specials"),
    (0x10000, 0x1007F, "Linear B Syllabary"),
    (0x10080, 0x100FF, "Linear B Ideograms"),
    (0x10100, 0x1013F, "Aegean Numbers"),
    (0x10140, 0x1018F, "Ancient Greek Numbers"),
    (0x10190, 0x101CF, "Ancient Symbols"),
    (0x101D0, 0x101FF, "Phaistos Disc"),
    (0x10280, 0x1029F, "Lycian"),
    (0x102A0, 0x102DF, "Carian"),
    (0x102E0, 0x102FF, "Coptic Epact Numbers"),
    (0x10300, 0x1032F, "Old Italic"),
    (0x10330, 0x1034F, "Gothic"),
    (0x10350, 0x1037F, "Old Permic"),
    (0x10380, 0x1039F, "Ugaritic"),
    (0x103A0, 0x103DF, "Old Persian"),
    (0x10400, 0x1044F, "Deseret"),
    (0x10450, 0x1047F, "Shavian"),
    (0x10480, 0x104AF, "Osmanya"),
    (0x104B0, 0x104FF, "Osage"),
    (0x10500, 0x1052F, "Elbasan"),
    (0x10530, 0x1056F, "Caucasian Albanian"),
    (0x10570, 0x105BF, "Vithkuqi"),
    (0x105C0, 0x105FF, "Todhri"),
    (0x10600, 0x1077F, "Linear A"),
    (0x10780, 0x107BF, "Latin Extended-F"),
    (0x10800, 0x1083F, "Cypriot Syllabary"),
    (0x10840, 0x1085F, "Imperial Aramaic"),
    (0x10860, 0x1087F, "Palmyrene"),
    (0x10880, 0x108AF, "Nabataean"),
    (0x108E0, 0x108FF, "Hatran"),
    (0x10900, 0x1091F, "Phoenician"),
    (0x10920, 0x1093F, "Lydian"),
    (0x10940, 0x1095F, "Sidetic"),
    (0x10980, 0x1099F, "Meroitic Hieroglyphs"),
    (0x109A0, 0x109FF, "Meroitic Cursive"),
    (0x10A00, 0x10A5F, "Kharoshthi"),
    (0x10A60, 0x10A7F, "Old South Arabian"),
    (0x10A80, 0x10A9F, "Old North Arabian"),
    (0x10AC0, 0x10AFF, "Manichaean"),
    (0x10B00, 0x10B3F, "Avestan"),
    (0x10B40, 0x10B5F, "Inscriptional Parthian"),
    (0x10B60, 0x10B7F, "Inscriptional Pahlavi"),
    (0x10B80, 0x10BAF, "Psalter Pahlavi"),
    (0x10C00, 0x10C4F, "Old Turkic"),
    (0x10C80, 0x10CFF, "Old Hungarian"),
    (0x10D00, 0x10D3F, "Hanifi Rohingya"),
    (0x10D40, 0x10D8F, "Garay"),
    (0x10E60, 0x10E7F, "Rumi Numeral Symbols"),
    (0x10E80, 0x10EBF, "Yezidi"),
    (0x10EC0, 0x10EFF, "Arabic Extended-C"),
    (0x10F00, 0x10F2F, "Old Sogdian"),
    (0x10F30, 0x10F6F, "Sogdian"),
    (0x10F70, 0x10FAF, "Old Uyghur"),
    (0x10FB0, 0x10FDF, "Chorasmian"),
    (0x10FE0, 0x10FFF, "Elymaic"),
    (0x11000, 0x1107F, "Brahmi"),
    (0x11080, 0x110CF, "Kaithi"),
    (0x110D0, 0x110FF, "Sora Sompeng"),
    (0x11100, 0x1114F, "Chakma"),
    (0x11150, 0x1117F, "Mahajani"),
    (0x11180, 0x111DF, "Sharada"),
    (0x111E0, 0x111FF, "Sinhala Archaic Numbers"),
    (0x11200, 0x1124F, "Khojki"),
    (0x11280, 0x112AF, "Multani"),
    (0x112B0, 0x112FF, "Khudawadi"),
    (0x11300, 0x1137F, "Grantha"),
    (0x11380, 0x113FF, "Tulu-Tigalari"),
    (0x11400, 0x1147F, "Newa"),
    (0x11480, 0x114DF, "Tirhuta"),
    (0x11580, 0x115FF, "Siddham"),
    (0x11600, 0x1165F, "Modi"),
    (0x11660, 0x1167F, "Mongolian Supplement"),
    (0x11680, 0x116CF, "Takri"),
    (0x116D0, 0x116FF, "Myanmar Extended-C"),
    (0x11700, 0x1174F, "Ahom"),
    (0x11800, 0x1184F, "Dogra"),
    (0x118A0, 0x118FF, "Warang Citi"),
    (0x11900, 0x1195F, "Dives Akuru"),
    (0x119A0, 0x119FF, "Nandinagari"),
    (0x11A00, 0x11A4F, "Zanabazar Square"),
    (0x11A50, 0x11AAF, "Soyombo"),
    (0x11AB0, 0x11ABF, "Unified Canadian Aboriginal Syllabics Extended-A"),
    (0x11AC0, 0x11AFF, "Pau Cin Hau"),
    (0x11B00, 0x11B5F, "Devanagari Extended-A"),
    (0x11B60, 0x11B7F, "Sharada Supplement"),
    (0x11BC0, 0x11BFF, "Sunuwar"),
    (0x11C00, 0x11C6F, "Bhaiksuki"),
    (0x11C70, 0x11CBF, "Marchen"),
    (0x11D00, 0x11D5F, "Masaram Gondi"),
    (0x11D60, 0x11DAF, "Gunjala Gondi"),
    (0x11DB0, 0x11DEF, "Tolong Siki"),
    (0x11EE0, 0x11EFF, "Makasar"),
    (0x11F00, 0x11F5F, "Kawi"),
    (0x11FB0, 0x11FBF, "Lisu Supplement"),
    (0x11FC0, 0x11FFF, "Tamil Supplement"),
    (0x12000, 0x123FF, "Cuneiform"),
    (0x12400, 0x1247F, "Cuneiform Numbers and Punctuation"),
    (0x12480, 0x1254F, "Early Dynastic Cuneiform"),
    (0x12F90, 0x12FFF, "Cypro-Minoan"),
    (0x13000, 0x1342F, "Egyptian Hieroglyphs"),
    (0x13430, 0x1345F, "Egyptian Hieroglyph Format Controls"),
    (0x13460, 0x143FF, "Egyptian Hieroglyphs Extended-A"),
    (0x14400, 0x1467F, "Anatolian Hieroglyphs"),
    (0x16100, 0x1613F, "Gurung Khema"),
    (0x16800, 0x16A3F, "Bamum Supplement"),
    (0x16A40, 0x16A6F, "Mro"),
    (0x16A70, 0x16ACF, "Tangsa"),
    (0x16AD0, 0x16AFF, "Bassa Vah"),
    (0x16B00, 0x16B8F, "Pahawh Hmong"),
    (0x16D40, 0x16D7F, "Kirat Rai"),
    (0x16E40, 0x16E9F, "Medefaidrin"),
    (0x16EA0, 0x16EDF, "Beria Erfe"),
    (0x16F00, 0x16F9F, "Miao"),
    (0x16FE0, 0x16FFF, "Ideographic Symbols and Punctuation"),
    (0x17000, 0x187FF, "Tangut"),
    (0x18800, 0x18AFF, "Tangut Components"),
    (0x18B00, 0x18CFF, "Khitan Small Script"),
    (0x18D00, 0x18D7F, "Tangut Supplement"),
    (0x18D80, 0x18DFF, "Tangut Components Supplement"),
    (0x1AFF0, 0x1AFFF, "Kana Extended-B"),
    (0x1B000, 0x1B0FF, "Kana Supplement"),
    (0x1B100, 0x1B12F, "Kana Extended-A"),
    (0x1B130, 0x1B16F, "Small Kana Extension"),
    (0x1B170, 0x1B2FF, "Nushu"),
    (0x1BC00, 0x1BC9F, "Duployan"),
    (0x1BCA0, 0x1BCAF, "Shorthand Format Controls"),
    (0x1CC00, 0x1CEBF, "Symbols for Legacy Computing Supplement"),
    (0x1CEC0, 0x1CEFF, "Miscellaneous Symbols Supplement"),
    (0x1CF00, 0x1CFCF, "Znamenny Musical Notation"),
    (0x1D000, 0x1D0FF, "Byzantine Musical Symbols"),
    (0x1D100, 0x1D1FF, "Musical Symbols"),
    (0x1D200, 0x1D24F, "Ancient Greek Musical Notation"),
    (0x1D2C0, 0x1D2DF, "Kaktovik Numerals"),
    (0x1D2E0, 0x1D2FF, "Mayan Numerals"),
    (0x1D300, 0x1D35F, "Tai Xuan Jing Symbols"),
    (0x1D360, 0x1D37F, "Counting Rod Numerals"),
    (0x1D400, 0x1D7FF, "Mathematical Alphanumeric Symbols"),
    (0x1D800, 0x1DAAF, "Sutton SignWriting"),
    (0x1DF00, 0x1DFFF, "Latin Extended-G"),
    (0x1E000, 0x1E02F, "Glagolitic Supplement"),
    (0x1E030, 0x1E08F, "Cyrillic Extended-D"),
    (0x1E100, 0x1E14F, "Nyiakeng Puachue Hmong"),
    (0x1E290, 0x1E2BF, "Toto"),
    (0x1E2C0, 0x1E2FF, "Wancho"),
    (0x1E4D0, 0x1E4FF, "Nag Mundari"),
    (0x1E5D0, 0x1E5FF, "Ol Onal"),
    (0x1E6C0, 0x1E6FF, "Tai Yo"),
    (0x1E7E0, 0x1E7FF, "Ethiopic Extended-B"),
    (0x1E800, 0x1E8DF, "Mende Kikakui"),
    (0x1E900, 0x1E95F, "Adlam"),
    (0x1EC70, 0x1ECBF, "Indic Siyaq Numbers"),
    (0x1ED00, 0x1ED4F, "Ottoman Siyaq Numbers"),
    (0x1EE00, 0x1EEFF, "Arabic Mathematical Alphabetic Symbols"),
    (0x1F000, 0x1F02F, "Mahjong Tiles"),
    (0x1F030, 0x1F09F, "Domino Tiles"),
    (0x1F0A0, 0x1F0FF, "Playing Cards"),
    (0x1F100, 0x1F1FF, "Enclosed Alphanumeric Supplement"),
    (0x1F200, 0x1F2FF, "Enclosed Ideographic Supplement"),
    (0x1F300, 0x1F5FF, "Miscellaneous Symbols and Pictographs"),
    (0x1F600, 0x1F64F, "Emoticons"),
    (0x1F650, 0x1F67F, "Ornamental Dingbats"),
    (0x1F680, 0x1F6FF, "Transport and Map Symbols"),
    (0x1F700, 0x1F77F, "Alchemical Symbols"),
    (0x1F780, 0x1F7FF, "Geometric Shapes Extended"),
    (0x1F800, 0x1F8FF, "Supplemental Arrows-C"),
    (0x1F900, 0x1F9FF, "Supplemental Symbols and Pictographs"),
    (0x1FA00, 0x1FA6F, "Chess Symbols"),
    (0x1FA70, 0x1FAFF, "Symbols and Pictographs Extended-A"),
    (0x1FB00, 0x1FBFF, "Symbols for Legacy Computing"),
    (0x20000, 0x2A6DF, "CJK Unified Ideographs Extension B"),
    (0x2A700, 0x2B73F, "CJK Unified Ideographs Extension C"),
    (0x2B740, 0x2B81F, "CJK Unified Ideographs Extension D"),
    (0x2B820, 0x2CEAF, "CJK Unified Ideographs Extension E"),
    (0x2CEB0, 0x2EBEF, "CJK Unified Ideographs Extension F"),
    (0x2EBF0, 0x2EE5F, "CJK Unified Ideographs Extension I"),
    (0x2F800, 0x2FA1F, "CJK Compatibility Ideographs Supplement"),
    (0x30000, 0x3134F, "CJK Unified Ideographs Extension G"),
    (0x31350, 0x323AF, "CJK Unified Ideographs Extension H"),
    (0x323B0, 0x3347F, "CJK Unified Ideographs Extension J"),
    (0xE0000, 0xE007F, "Tags"),
    (0xE0100, 0xE01EF, "Variation Selectors Supplement"),
    (0xF0000, 0xFFFFF, "Supplementary Private Use Area-A"),
    (0x100000, 0x10FFFF, "Supplementary Private Use Area-B"),
];

/// The Unicode block containing `value`, or `None` where Unicode defines none.
///
/// `None` rather than a placeholder: upstream's `blockName` returns `nil` and
/// each panel draws its own "No Block" text, so that string belongs to the
/// panel — see [`ScalarInfo::display_block`].
pub fn block_name(value: u32) -> Option<&'static str> {
    BLOCKS
        .iter()
        .find(|(start, end, _)| value >= *start && value <= *end)
        .map(|(_, _, name)| *name)
}

/// The name CotEditor *displays* for a block.
///
/// Its `localizeBlockName(_:)` looks the `Blocks.txt` name up in a string
/// catalog, so the interface shows the catalog's value rather than the block's
/// official name. In English twenty of the 346 entries are abbreviated
/// (`… Extension A` → `… Ext. A`) and the rest are the identity; this is that
/// table reduced to its differences. The catalog's other twelve languages are
/// not ported — SubText's interface is English.
pub fn display_block_name(block: &str) -> &str {
    match block {
        "Arabic Mathematical Alphabetic Symbols" => "Arabic Math Alphabetic Symbols",
        "CJK Compatibility Ideographs Supplement" => "CJK Compatibility Ideographs Supp.",
        "CJK Unified Ideographs Extension A" => "CJK Unified Ideographs Ext. A",
        "CJK Unified Ideographs Extension B" => "CJK Unified Ideographs Ext. B",
        "CJK Unified Ideographs Extension C" => "CJK Unified Ideographs Ext. C",
        "CJK Unified Ideographs Extension D" => "CJK Unified Ideographs Ext. D",
        "CJK Unified Ideographs Extension E" => "CJK Unified Ideographs Ext. E",
        "CJK Unified Ideographs Extension F" => "CJK Unified Ideographs Ext. F",
        "CJK Unified Ideographs Extension G" => "CJK Unified Ideographs Ext. G",
        "CJK Unified Ideographs Extension H" => "CJK Unified Ideographs Ext. H",
        "CJK Unified Ideographs Extension I" => "CJK Unified Ideographs Ext. I",
        "CJK Unified Ideographs Extension J" => "CJK Unified Ideographs Ext. J",
        "Ideographic Description Characters" => "Ideographic Desc. Characters",
        "Mathematical Alphanumeric Symbols" => "Math Alphanumeric Symbols",
        "Miscellaneous Mathematical Symbols-A" => "Miscellaneous Math Symbols-A",
        "Miscellaneous Mathematical Symbols-B" => "Miscellaneous Math Symbols-B",
        "Miscellaneous Symbols and Pictographs" => "Miscellaneous Symbols And Pictographs",
        "NKo" => "N'Ko",
        "Supplemental Mathematical Operators" => "Supplemental Math Operators",
        "Transport and Map Symbols" => "Transport And Map Symbols",
        other => other,
    }
}

/// The conventional name of a control character.
///
/// Unicode leaves the C0 and C1 ranges unnamed — `UnicodeData.txt` says
/// `<control>` — and CotEditor supplies the ASCII / ISO 6429 names itself
/// rather than showing the code point. Transcribed from its
/// `Unicode.Scalar.controlCharacterName`.
fn control_character_name(value: u32) -> Option<String> {
    let name = match value {
        0x0000 => "NULL",
        0x0001 => "START OF HEADING",
        0x0002 => "START OF TEXT",
        0x0003 => "END OF TEXT",
        0x0004 => "END OF TRANSMISSION",
        0x0005 => "ENQUIRY",
        0x0006 => "ACKNOWLEDGE",
        0x0007 => "BELL",
        0x0008 => "BACKSPACE",
        0x0009 => "HORIZONTAL TABULATION",
        0x000A => "LINE FEED",
        0x000B => "VERTICAL TABULATION",
        0x000C => "FORM FEED",
        0x000D => "CARRIAGE RETURN",
        0x000E => "SHIFT OUT",
        0x000F => "SHIFT IN",
        0x0010 => "DATA LINK ESCAPE",
        0x0011 => "DEVICE CONTROL ONE",
        0x0012 => "DEVICE CONTROL TWO",
        0x0013 => "DEVICE CONTROL THREE",
        0x0014 => "DEVICE CONTROL FOUR",
        0x0015 => "NEGATIVE ACKNOWLEDGE",
        0x0016 => "SYNCHRONOUS IDLE",
        0x0017 => "END OF TRANSMISSION BLOCK",
        0x0018 => "CANCEL",
        0x0019 => "END OF MEDIUM",
        0x001A => "SUBSTITUTE",
        0x001B => "ESCAPE",
        0x001C => "FILE SEPARATOR",
        0x001D => "GROUP SEPARATOR",
        0x001E => "RECORD SEPARATOR",
        0x001F => "UNIT SEPARATOR",
        0x007F => "DELETE",
        0x0080 => "PADDING CHARACTER",
        0x0081 => "HIGH OCTET PRESET",
        0x0082 => "BREAK PERMITTED HERE",
        0x0083 => "NO BREAK HERE",
        0x0084 => "INDEX",
        0x0085 => "NEXT LINE",
        0x0086 => "START OF SELECTED AREA",
        0x0087 => "END OF SELECTED AREA",
        0x0088 => "CHARACTER TABULATION SET",
        0x0089 => "CHARACTER TABULATION WITH JUSTIFICATION",
        0x008A => "LINE TABULATION SET",
        0x008B => "PARTIAL LINE FORWARD",
        0x008C => "PARTIAL LINE BACKWARD",
        0x008D => "REVERSE LINE FEED",
        0x008E => "SINGLE SHIFT TWO",
        0x008F => "SINGLE SHIFT THREE",
        0x0090 => "DEVICE CONTROL STRING",
        0x0091 => "PRIVATE USE ONE",
        0x0092 => "PRIVATE USE TWO",
        0x0093 => "SET TRANSMIT STATE",
        0x0094 => "CANCEL CHARACTER",
        0x0095 => "MESSAGE WAITING",
        0x0096 => "START OF PROTECTED AREA",
        0x0097 => "END OF PROTECTED AREA",
        0x0098 => "START OF STRING",
        0x0099 => "SINGLE GRAPHIC CHARACTER INTRODUCER",
        0x009A => "SINGLE CHARACTER INTRODUCER",
        0x009B => "CONTROL SEQUENCE INTRODUCER",
        0x009C => "STRING TERMINATOR",
        0x009D => "OPERATING SYSTEM COMMAND",
        0x009E => "PRIVACY MESSAGE",
        0x009F => "APPLICATION PROGRAM COMMAND",
        _ => return None,
    };
    Some(name.to_string())
}

/// One of the three counts, whole and selected.
///
/// Upstream's `EditorCounter.Count`. The pair is what makes a count worth a
/// glance mid-edit, so the formatting rule lives next to the pair rather than
/// at each of the two places that draw one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Count {
    pub entire: usize,
    /// How much of `entire` is selected. Zero means no selection, and the
    /// parenthesised half is then left off.
    pub selected: usize,
}

impl Count {
    /// `entire (selected)` — upstream's `Count.formatted`.
    pub fn formatted(self) -> String {
        if self.selected > 0 {
            format!("{} ({})", self.entire, self.selected)
        } else {
            self.entire.to_string()
        }
    }
}

/// CotEditor's "Advanced Character Count" breakdown, and the three counts the
/// status bar and the document inspector's Count section share.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterCount {
    /// Extended grapheme clusters — Swift's `String.count`, which is what
    /// "Characters" means everywhere in CotEditor. A CRLF is one of them, a
    /// flag is one, `👍🏻` is one.
    pub characters: usize,
    /// Characters excluding line endings.
    pub characters_without_newlines: usize,
    /// Unicode scalar values, so a surrogate pair counts once.
    pub unicode_scalars: usize,
    /// UTF-16 code units — what a JavaScript string would report.
    pub utf16_units: usize,
    /// UTF-8 bytes.
    pub utf8_bytes: usize,
    /// Bytes in the document's own encoding.
    pub encoded_bytes: usize,
    /// Lines, excluding a last blank one. See [`count_lines`].
    pub lines: usize,
    /// Words, by UAX #29 word boundaries. See [`count_words`].
    pub words: usize,
    /// Paragraphs — SubText's own; CotEditor has no such count.
    pub paragraphs: usize,
}

impl CharacterCount {
    pub fn of(text: &str, encoding: TextEncoding) -> Self {
        let characters = text.graphemes(true).count();
        let paragraphs =
            text.split(['\n', '\r', '\u{2029}']).filter(|p| !p.trim().is_empty()).count();

        Self {
            characters,
            characters_without_newlines: characters.saturating_sub(count_line_endings(text)),
            unicode_scalars: text.chars().count(),
            utf16_units: text.chars().map(char::len_utf16).sum(),
            utf8_bytes: text.len(),
            encoded_bytes: encoding.encode(text).0.len(),
            lines: count_lines(text),
            words: count_words(text),
            paragraphs,
        }
    }

    /// The three counts the status bar and the inspector's Count section show,
    /// whole and selected, one value each.
    ///
    /// `selected` is the count of the selected slice; `None` means nothing is
    /// selected, which is not the same as an empty selection being counted.
    pub fn counts(&self, selected: Option<&Self>) -> [Count; 3] {
        let pair = |entire: usize, pick: fn(&Self) -> usize| Count {
            entire,
            selected: selected.map_or(0, pick),
        };
        [
            pair(self.lines, |count| count.lines),
            pair(self.characters, |count| count.characters),
            pair(self.words, |count| count.words),
        ]
    }
}

/// How many *characters* the document's line endings occupy in total.
///
/// Scanned rather than counted per ending: `\r\n` is one ending of two
/// characters, and counting `\n` and `\r` separately would call it two. Each
/// ending is exactly one grapheme cluster, so the answer is also the number of
/// characters to subtract from a grapheme count.
fn count_line_endings(text: &str) -> usize {
    let mut total = 0;
    let mut offset = 0;
    while offset < text.len() {
        match crate::line_ending::line_ending_at(text, offset) {
            Some((ending, len)) => {
                total += ending.as_str().chars().count();
                offset += len;
            }
            None => offset += text[offset..].chars().next().map_or(1, char::len_utf8),
        }
    }
    total
}

/// The number of lines, excluding a last blank one.
///
/// Upstream's `String.numberOfLines` is `enumerateSubstrings(.byLines)`, which
/// counts a trailing line ending as *ending* the line it closes rather than as
/// opening an empty one: `""` is 0 lines, `"\n"` is 1, `"a\nb c\n\n"` is 3. So
/// it is one per line ending, plus one when the text does not end with one.
fn count_lines(text: &str) -> usize {
    let mut endings = 0;
    let mut offset = 0;
    let mut at_ending = false;
    while offset < text.len() {
        match crate::line_ending::line_ending_at(text, offset) {
            Some((_, len)) => {
                endings += 1;
                offset += len;
                at_ending = true;
            }
            None => {
                offset += text[offset..].chars().next().map_or(1, char::len_utf8);
                at_ending = false;
            }
        }
    }
    if text.is_empty() { 0 } else { endings + usize::from(!at_ending) }
}

/// The number of words.
///
/// Upstream's `String.numberOfWords` is `enumerateSubstrings(.byWords, .localized)`,
/// which is ICU's word segmentation. UAX #29 — what `split_word_bounds`
/// implements — agrees with it on every case upstream tests: `"Clarus says
/// moof!"` is 3, `"plain-text"` is 2, `"!"` is 0. A segment counts when it
/// holds a letter or a digit, which drops the whitespace and the punctuation
/// word segmentation hands back between the words.
fn count_words(text: &str) -> usize {
    text.split_word_bounds().filter(|word| word.chars().any(char::is_alphanumeric)).count()
}

// ---- the advanced character count ----------------------------------------

/// What the *Advanced Character Count* counts.
///
/// CotEditor's `CharacterCountOptions.CharacterUnit`, and the four entries of
/// its *Unit* menu in the same order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CharacterUnit {
    GraphemeCluster,
    UnicodeScalar,
    Utf16,
    Byte,
}

impl CharacterUnit {
    /// Every unit, in upstream's `CaseIterable` order.
    pub const ALL: [CharacterUnit; 4] = [
        CharacterUnit::GraphemeCluster,
        CharacterUnit::UnicodeScalar,
        CharacterUnit::Utf16,
        CharacterUnit::Byte,
    ];

    /// The name its menu shows.
    pub fn label(self) -> &'static str {
        match self {
            CharacterUnit::GraphemeCluster => "Grapheme cluster",
            CharacterUnit::UnicodeScalar => "Unicode scalar",
            CharacterUnit::Utf16 => "UTF-16",
            CharacterUnit::Byte => "Byte",
        }
    }

    /// The sentence under the menu that says what the unit means.
    pub fn description(self) -> &'static str {
        match self {
            CharacterUnit::GraphemeCluster => {
                "Count in the intuitive way defined in Unicode. A character consisting of \
                 multiple Unicode code points, such as emojis, is counted as one character."
            }
            CharacterUnit::UnicodeScalar => {
                "Count Unicode code points. Same as counting UTF-32."
            }
            CharacterUnit::Utf16 => {
                "Count Unicode code points but a surrogate pair as two characters."
            }
            CharacterUnit::Byte => {
                "Count bytes of the text encoded with the specified encoding."
            }
        }
    }
}

/// How to count, and what to count.
///
/// CotEditor's `CharacterCountOptions`. Each field is one control in its
/// options panel, and the order of the transformations in [`Self::count`] is
/// the order the checkboxes are stacked in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterCountOptions {
    pub unit: CharacterUnit,
    /// `None` is the *Normalization* checkbox being off. The form the user
    /// picked is remembered separately, so turning the checkbox back on
    /// restores it — which is why this is not just a `Normalization`.
    pub normalization_form: Option<Normalization>,
    pub ignores_newlines: bool,
    pub ignores_whitespaces: bool,
    pub treats_consecutive_whitespace_as_single: bool,
    /// Only the byte unit reads this.
    pub encoding: TextEncoding,
}

impl Default for CharacterCountOptions {
    fn default() -> Self {
        Self {
            unit: CharacterUnit::GraphemeCluster,
            normalization_form: None,
            ignores_newlines: false,
            ignores_whitespaces: false,
            treats_consecutive_whitespace_as_single: false,
            encoding: TextEncoding::UTF8,
        }
    }
}

/// `\R` — the line endings *Ignore line endings* takes out.
///
/// ICU's `\R`, which is what upstream's regex means by it: CRLF as one, then
/// each of the others on its own. Note that this is a wider set than the six
/// line endings a document can be *written* with — vertical tab and form feed
/// are in it, and they are not line endings CotEditor's editor recognizes.
static LINE_ENDINGS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\r\n|[\n\x0B\x0C\r\u{85}\u{2028}\u{2029}]").expect("valid pattern")
});

/// `[\t\p{Zs}]` — *Ignore whitespace*.
///
/// Space *separators* and the tab, and deliberately not the line endings: that
/// is what makes the two checkboxes independent, and why collapsing runs is
/// still meaningful with one of them on.
static WHITESPACES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\t\p{Zs}]").expect("valid pattern"));

/// `\s{2,}` — the runs *Treat consecutive whitespace as one space* collapses.
///
/// ICU's `\s` is `[\t\n\f\r\p{Z}]`: it is *not* Rust's `\p{White_Space}`, which
/// would also take the vertical tab and NEL. Written out so the two agree.
static WHITESPACE_RUNS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\t\n\x0C\r\p{Z}]{2,}").expect("valid pattern"));

impl CharacterCountOptions {
    /// Count `text` the way these options describe.
    ///
    /// Upstream's `String.count(options:)`. `None` is the one failure it can
    /// report: the text is not representable in [`Self::encoding`], which only
    /// the byte unit can care about.
    pub fn count(&self, text: &str) -> Option<usize> {
        // Before any transformation, so an empty document counts as zero bytes
        // rather than as a failure.
        if text.is_empty() {
            return Some(0);
        }

        let mut text = text.to_string();
        if self.ignores_newlines {
            text = LINE_ENDINGS.replace_all(&text, "").into_owned();
        }
        if self.ignores_whitespaces {
            text = WHITESPACES.replace_all(&text, "").into_owned();
        }
        // The checkbox is disabled while both of the others are on, because
        // there would be nothing left to collapse; the guard is repeated here
        // so the rule holds whatever the panel does.
        if self.treats_consecutive_whitespace_as_single
            && !(self.ignores_newlines && self.ignores_whitespaces)
        {
            text = WHITESPACE_RUNS.replace_all(&text, " ").into_owned();
        }
        if let Some(form) = self.normalization_form {
            text = textops::normalize(&text, &(0..text.len()), form).text;
        }

        match self.unit {
            CharacterUnit::GraphemeCluster => Some(text.graphemes(true).count()),
            CharacterUnit::UnicodeScalar => Some(text.chars().count()),
            CharacterUnit::Utf16 => Some(text.chars().map(char::len_utf16).sum()),
            CharacterUnit::Byte => {
                let (bytes, had_errors) = self.encoding.encode(&text);
                (!had_errors).then_some(bytes.len())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_character_info() {
        let info = CharacterInfo::new('A', TextEncoding::UTF8);
        assert_eq!(info.code_point(), "U+0041");
        assert_eq!(info.scalar.name.as_deref(), Some("LATIN CAPITAL LETTER A"));
        assert_eq!(info.scalar.category, GeneralCategory::UppercaseLetter);
        assert_eq!(info.scalar.category.to_string(), "Uppercase Letter (Lu)");
        assert_eq!(info.scalar.block, Some("Basic Latin"));
        assert_eq!(info.utf8, "41");
        assert_eq!(info.utf16, "0041");
        assert_eq!(info.width, 1);
    }

    #[test]
    fn cjk_character_is_wide_and_multibyte() {
        let info = CharacterInfo::new('漢', TextEncoding::UTF8);
        assert_eq!(info.code_point(), "U+6F22");
        assert_eq!(info.scalar.block, Some("CJK Unified Ideographs"));
        assert_eq!(info.width, 2);
        assert_eq!(info.encoded_len, 3);
        // Unicode names the CJK ranges algorithmically, and both Swift and
        // `unicode_names2` spell that out rather than leaving it empty.
        assert_eq!(info.scalar.name.as_deref(), Some("CJK UNIFIED IDEOGRAPH-6F22"));
    }

    #[test]
    fn astral_character_needs_a_surrogate_pair() {
        let info = CharacterInfo::new('😀', TextEncoding::UTF8);
        assert_eq!(info.code_point(), "U+1F600");
        assert_eq!(info.utf16, "D83D DE00");
        assert_eq!(info.width, 2);
        assert_eq!(info.scalar.block, Some("Emoticons"));
    }

    #[test]
    fn shift_jis_character_is_two_bytes() {
        let info = CharacterInfo::new('あ', TextEncoding::from_identifier("shift_jis").unwrap());
        assert_eq!(info.encoded_len, 2);
        assert_eq!(info.encoding_name, "Japanese (Shift JIS)");
    }

    #[test]
    fn a_control_character_is_named_by_convention() {
        // Unicode calls these `<control>`; CotEditor names them.
        let null = CharacterInfo::new('\u{0}', TextEncoding::UTF8);
        assert_eq!(null.scalar.name.as_deref(), Some("NULL"));
        assert_eq!(null.scalar.category, GeneralCategory::Control);
        let delete = CharacterInfo::new('\u{7F}', TextEncoding::UTF8);
        assert_eq!(delete.scalar.name.as_deref(), Some("DELETE"));
        let next_line = CharacterInfo::new('\u{85}', TextEncoding::UTF8);
        assert_eq!(next_line.scalar.name.as_deref(), Some("NEXT LINE"));
        // The C1 range is where the two tables could most easily disagree, so
        // its last entry is checked too.
        let apc = CharacterInfo::new('\u{9F}', TextEncoding::UTF8);
        assert_eq!(apc.scalar.name.as_deref(), Some("APPLICATION PROGRAM COMMAND"));
    }

    #[test]
    fn an_unnamed_scalar_has_no_name_rather_than_a_stand_in() {
        // The private-use area: Unicode gives it no name, and neither does
        // CotEditor, which draws an empty value.
        let info = CharacterInfo::new('\u{E000}', TextEncoding::UTF8);
        assert_eq!(info.scalar.name, None);
        assert_eq!(info.scalar.category, GeneralCategory::PrivateUse);
        assert_eq!(info.scalar.block, Some("Private Use Area"));
        assert_eq!(info.scalar.display_name(), "");
    }

    #[test]
    fn a_category_the_predicates_would_get_wrong() {
        // `char::is_numeric` is true for all three of these, and they are three
        // different categories.
        assert_eq!(GeneralCategory::of('7'), GeneralCategory::DecimalNumber);
        assert_eq!(GeneralCategory::of('Ⅻ'), GeneralCategory::LetterNumber);
        assert_eq!(GeneralCategory::of('½'), GeneralCategory::OtherNumber);
        // `char::is_alphabetic` is true for these two, which differ as well.
        assert_eq!(GeneralCategory::of('漢'), GeneralCategory::OtherLetter);
        assert_eq!(GeneralCategory::of('\u{2B0}'), GeneralCategory::ModifierLetter);
        assert_eq!(GeneralCategory::of('\u{301}'), GeneralCategory::NonspacingMark);
        assert_eq!(GeneralCategory::of('\u{20}'), GeneralCategory::SpaceSeparator);
        assert_eq!(GeneralCategory::of('\u{2028}'), GeneralCategory::LineSeparator);
        assert_eq!(GeneralCategory::of('\u{200B}'), GeneralCategory::Format);
        assert_eq!(GeneralCategory::of('\u{1F600}'), GeneralCategory::OtherSymbol);
        assert_eq!(GeneralCategory::of('\u{0}'), GeneralCategory::Control);
    }

    #[test]
    fn the_block_table_is_ascending_and_does_not_overlap() {
        // The table is transcribed, so this is the check that the transcription
        // did not drop a range, overlap two, or run one backwards. Unicode's
        // blocks are ascending and disjoint; they are *not* contiguous, so a
        // gap between two neighbours is expected and only an overlap is not.
        let mut previous: Option<(u32, u32, &str)> = None;
        for &(start, end, name) in BLOCKS {
            assert!(start <= end, "{name} runs backwards");
            if let Some((_, previous_end, previous_name)) = previous {
                assert!(start > previous_end, "{name} overlaps {previous_name}");
            }
            previous = Some((start, end, name));
        }
        assert_eq!(BLOCKS.len(), 346, "Unicode 17.0 defines 346 blocks");
        assert_eq!(BLOCKS[0], (0x0000, 0x007F, "Basic Latin"));
        assert_eq!(BLOCKS[345], (0x100000, 0x10FFFF, "Supplementary Private Use Area-B"));

        // Spot checks across the table, including the gap Unicode really has
        // between the last CJK extension and the Tags block.
        assert_eq!(block_name(0x0000), Some("Basic Latin"));
        assert_eq!(block_name(0x007F), Some("Basic Latin"));
        assert_eq!(block_name(0x0080), Some("Latin-1 Supplement"));
        assert_eq!(block_name(0x4E00), Some("CJK Unified Ideographs"));
        assert_eq!(block_name(0x1F600), Some("Emoticons"));
        assert_eq!(block_name(0xE0000), Some("Tags"));
        assert_eq!(block_name(0x10FFFF), Some("Supplementary Private Use Area-B"));
        // A hole Unicode really has: "CJK Unified Ideographs Extension J" ends
        // at 0x3347F and the next block does not start until 0xE0000.
        assert_eq!(block_name(0x3347F), Some("CJK Unified Ideographs Extension J"));
        assert_eq!(block_name(0x34000), None);
    }

    #[test]
    fn the_display_name_is_the_catalogs_english_value() {
        assert_eq!(display_block_name("Basic Latin"), "Basic Latin");
        assert_eq!(
            display_block_name("CJK Unified Ideographs Extension A"),
            "CJK Unified Ideographs Ext. A"
        );
        assert_eq!(display_block_name("NKo"), "N'Ko");
        // The abbreviated entries have to be reachable through the panel's
        // helper too, or a row would show the long name where upstream shows
        // the short one.
        let info = CharacterInfo::new('漢', TextEncoding::UTF8);
        assert_eq!(info.scalar.display_block(), "CJK Unified Ideographs");
        assert_eq!(ScalarInfo::of('\u{E000}').display_block(), "Private Use Area");
        // A block with no display name of its own is drawn as-is.
        assert_eq!(display_block_name("Transport and Map Symbols"), "Transport And Map Symbols");
    }

    #[test]
    fn a_character_report_needs_exactly_one_character() {
        let report = CharacterReport::of("😀");
        assert_eq!(report.code_points(), vec!["U+1F600"]);
        assert!(report.is_single_scalar());
        assert_eq!(report.character, "😀");

        // A flag is one character of two scalars: the code points are listed
        // and the three single-scalar rows are not.
        let flag = CharacterReport::of("🇯🇵");
        assert_eq!(flag.code_points(), vec!["U+1F1EF", "U+1F1F5"]);
        assert!(!flag.is_single_scalar());

        // `👍🏻` is one character of two scalars as well.
        assert_eq!(CharacterReport::of("👍🏻").scalars.len(), 2);

        assert!(CharacterReport::of("").is_empty());
        assert!(CharacterReport::of("ab").is_empty());
        // A CRLF is one character, but of two scalars, so it lists two code
        // points and none of the single-scalar rows.
        assert_eq!(CharacterReport::of("\r\n").code_points(), vec!["U+000D", "U+000A"]);
        assert!(!CharacterReport::of("\r\n").is_single_scalar());
    }

    #[test]
    fn an_invisible_control_character_is_drawn_as_its_picture() {
        // Upstream's `pictureRepresentation`, which is why the big glyph is
        // greyed for a control character: the picture is a stand-in, the
        // character itself would be invisible.
        assert_eq!(ScalarInfo::of('\u{0}').picture, Some('␀'));
        assert_eq!(ScalarInfo::of('\n').picture, Some('␊'));
        assert_eq!(ScalarInfo::of(' ').picture, Some('␠'));
        assert_eq!(ScalarInfo::of('\u{7F}').picture, Some('␡'));
        assert_eq!(ScalarInfo::of('A').picture, None);

        // Only a single-scalar character has one.
        assert_eq!(CharacterReport::of("\r\n").picture(), None);
        assert_eq!(CharacterReport::of("A").picture(), None);
        assert_eq!(CharacterReport::of("\t").picture(), Some('␉'));
    }

    #[test]
    fn a_variant_is_not_a_complex_character() {
        // Two scalars, the second a variation selector: the character keeps its
        // name and gains the variant in parentheses.
        let check = CharacterReport::of("✔\u{FE0F}");
        assert!(check.is_variant());
        assert!(!check.is_complex());
        assert_eq!(check.description().as_deref(), Some("HEAVY CHECK MARK (Emoji Style)"));

        // The text-style selector is the other half of that pair.
        let text = CharacterReport::of("✔\u{FE0E}");
        assert_eq!(text.description().as_deref(), Some("HEAVY CHECK MARK (Text Style)"));

        // A skin-tone modifier is a variant too.
        let thumb = CharacterReport::of("👍🏻");
        assert!(thumb.is_variant());
        assert_eq!(thumb.description().as_deref(), Some("THUMBS UP SIGN (Skin Tone I-II)"));

        // A general variation selector has no name of its own.
        assert_eq!(CharacterReport::of("A\u{FE00}").description().as_deref(), Some("LATIN CAPITAL LETTER A (Variant)"));
    }

    #[test]
    fn a_complex_character_says_how_many_scalars_it_is() {
        // A pirate flag: four scalars, none of them a trailing variant.
        let pirate = CharacterReport::of("🏴\u{200D}☠\u{FE0F}");
        assert_eq!(pirate.scalars.len(), 4);
        assert!(pirate.is_complex());
        assert!(!pirate.is_variant());
        assert_eq!(pirate.description().as_deref(), Some("<a letter consisting of 4 characters>"));

        // Two regional indicators are two scalars, and neither is a variant.
        let flags = CharacterReport::of("🇦🇦");
        assert!(flags.is_complex());
        assert_eq!(flags.description().as_deref(), Some("<a letter consisting of 2 characters>"));
    }

    #[test]
    fn an_unnamed_character_has_no_headline() {
        // A private-use scalar has no name, so the panel says "Unknown" — the
        // `description` is what tells it to.
        assert_eq!(CharacterReport::of("\u{E000}").description(), None);
        // A control character does have one, by convention.
        assert_eq!(CharacterReport::of("\n").description().as_deref(), Some("LINE FEED"));
    }

    #[test]
    fn the_code_point_row_shows_the_surrogate_pair() {
        let grinning = ScalarInfo::of('😀');
        assert_eq!(grinning.surrogate_code_points, Some(("D83D".into(), "DE00".into())));
        assert_eq!(grinning.code_point_with_surrogates(), "U+1F600 (D83D DE00)");

        // A scalar that fits in one UTF-16 code unit has no pair to show.
        let ascii = ScalarInfo::of('A');
        assert_eq!(ascii.surrogate_code_points, None);
        assert_eq!(ascii.code_point_with_surrogates(), "U+0041");
    }

    #[test]
    fn the_version_row_is_the_unicode_version_that_assigned_the_scalar() {
        assert_eq!(ScalarInfo::of('A').version().as_deref(), Some("Unicode 1.1"));
        assert_eq!(ScalarInfo::of('€').version().as_deref(), Some("Unicode 2.1"));
        assert_eq!(ScalarInfo::of('😀').version().as_deref(), Some("Unicode 6.1"));
        // An unassigned scalar has no version, which is the empty row. The
        // private-use planes are *assigned* — they are 2.0 — so the example has
        // to be a real gap in Unicode's coverage.
        let unassigned = ScalarInfo::of('\u{34000}');
        assert_eq!(unassigned.age, None);
        assert_eq!(unassigned.version(), None);
    }

    #[test]
    fn the_deprecated_flag_is_unicode_s_own() {
        assert!(ScalarInfo::of('\u{149}').is_deprecated);
        assert!(!ScalarInfo::of('A').is_deprecated);
    }

    #[test]
    fn the_count_formats_the_selection_the_way_upstream_does() {
        assert_eq!(Count { entire: 12, selected: 0 }.formatted(), "12");
        assert_eq!(Count { entire: 12, selected: 4 }.formatted(), "12 (4)");
    }

    #[test]
    fn the_three_counts_pair_up_with_their_selected_halves() {
        let entire = CharacterCount::of("a\nb c\n\n", TextEncoding::UTF8);
        let selected = CharacterCount::of("b c", TextEncoding::UTF8);
        let counts = entire.counts(Some(&selected));
        assert_eq!(counts[0].formatted(), "3 (1)");
        assert_eq!(counts[1].formatted(), "7 (3)");
        assert_eq!(counts[2].formatted(), "3 (2)");
        // Nothing selected is not the same as an empty selection.
        let counts = entire.counts(None);
        assert_eq!(counts[0].formatted(), "3");
        assert_eq!(counts[1].formatted(), "7");
        assert_eq!(counts[2].formatted(), "3");
    }

    // ---- the vectors from upstream `StringCountingTests.swift` -------------

    #[test]
    fn counting_a_mixed_document() {
        let text = "Hello world\nこんにちは\n";
        let count = CharacterCount::of(text, TextEncoding::UTF8);
        assert_eq!(count.characters, 18);
        assert_eq!(count.characters_without_newlines, 16);
        assert_eq!(count.unicode_scalars, 18);
        assert_eq!(count.utf8_bytes, text.len());
        assert_eq!(count.utf16_units, 18);
        // Seven, not three: see `words_are_uax_29_word_boundaries` for why the
        // five kana are five words here and one word in CotEditor.
        assert_eq!(count.words, 7);
        // Two lines: the trailing line ending closes the second rather than
        // opening a third.
        assert_eq!(count.lines, 2);
    }

    #[test]
    fn characters_are_grapheme_clusters() {
        // Upstream `countComposedCharacters`.
        let characters = |text: &str| CharacterCount::of(text, TextEncoding::UTF8).characters;
        assert_eq!(characters("foo"), 3);
        assert_eq!(characters("\r\n"), 1);
        assert_eq!(characters("😀🇯🇵a"), 3);
        assert_eq!(characters("😀🏻"), 1);
        assert_eq!(characters("👍🏻"), 1);
        assert_eq!(characters("🇦 "), 2);

        // The scalar count is a different question with a different answer:
        // a grinning face is one, each regional indicator is one, and the
        // letter is one.
        assert_eq!(CharacterCount::of("😀🇯🇵a", TextEncoding::UTF8).unicode_scalars, 4);
    }

    #[test]
    fn crlf_counts_as_one_line_ending_of_two_characters() {
        let count = CharacterCount::of("a\r\nb", TextEncoding::UTF8);
        // Four scalars: 'a', '\r', '\n', 'b'. The CRLF is one line ending but
        // two characters, so excluding line endings leaves one character.
        assert_eq!(count.unicode_scalars, 4);
        assert_eq!(count.characters, 3);
        assert_eq!(count.characters_without_newlines, 1);
        assert_eq!(count.lines, 2);
    }

    #[test]
    fn words_are_uax_29_word_boundaries() {
        // Upstream `countWords`, which every one of these matches.
        let words = |text: &str| CharacterCount::of(text, TextEncoding::UTF8).words;
        assert_eq!(words("Clarus says moof!"), 3);
        assert_eq!(words("plain-text"), 2);
        assert_eq!(words("!"), 0);
        assert_eq!(words(""), 0);
        assert_eq!(words("3.14"), 1);
        assert_eq!(words("don't"), 1);
        assert_eq!(words("a_b"), 1);
        assert_eq!(words("😀 😀"), 0);

        // **These are where the two rules part company, and they are
        // measured.** Upstream asks ICU for *localized* word boundaries, which
        // is UAX #29 plus a dictionary for the scripts that do not put spaces
        // between their words. A probe against Foundation
        // (`enumerateSubstrings(.byWords, .localized)`, locale `zh-Hans_US`)
        // gives `こんにちは` = 1 and `漢字テスト` = 2; the rules alone give 5
        // and 3. ICU's answer is itself locale-dependent — with `en` its CJK
        // dictionary is not loaded — so no single number is "the one CotEditor
        // shows". Everything above this line is a case both rules agree on.
        // See the gap table in `docs/PLAN.md`.
        assert_eq!(words("こんにちは"), 5);
        assert_eq!(words("漢字テスト"), 3);
    }

    #[test]
    fn lines_do_not_count_a_last_blank_one() {
        // Upstream `numberOfLines`.
        let lines = |text: &str| CharacterCount::of(text, TextEncoding::UTF8).lines;
        assert_eq!(lines(""), 0);
        assert_eq!(lines("a"), 1);
        assert_eq!(lines("\n"), 1);
        assert_eq!(lines("\n\n"), 2);
        assert_eq!(lines("\u{feff}"), 1);
        assert_eq!(lines("ab\r\ncd"), 2);
        assert_eq!(lines("a\u{FEFF}\nb"), 2);
        assert_eq!(lines("\u{FEFF}\nb"), 2);
        assert_eq!(lines("a\nb c\n\n"), 3);
        // The other four line endings move the count the same way.
        assert_eq!(lines("a\rb"), 2);
        assert_eq!(lines("a\u{85}b"), 2);
        assert_eq!(lines("a\u{2028}b"), 2);
        assert_eq!(lines("a\u{2029}b"), 2);
    }

    // ---- the vectors from upstream `AdvancedCountingTests.swift` ----------

    #[test]
    fn the_whitespace_options_narrow_the_count() {
        let text = "aaa \t \u{1F431}\n\r\n c";
        let count = |options: CharacterCountOptions| options.count(text).expect("countable");

        // Eleven grapheme clusters to begin with: three letters, three
        // whitespace characters, the cat, the two line endings and the space
        // before the last letter.
        assert_eq!(count(CharacterCountOptions::default()), 11);

        let ignores_newlines =
            CharacterCountOptions { ignores_newlines: true, ..Default::default() };
        assert_eq!(count(ignores_newlines), 9);

        let ignores_both = CharacterCountOptions {
            ignores_newlines: true,
            ignores_whitespaces: true,
            ..Default::default()
        };
        assert_eq!(count(ignores_both), 5);

        // Whitespace without newlines leaves the line endings in.
        let ignores_whitespaces =
            CharacterCountOptions { ignores_whitespaces: true, ..Default::default() };
        assert_eq!(count(ignores_whitespaces), 7);

        // Collapsing runs takes the three characters before the cat down to
        // one *and* the four after it — the newline run and the space run are
        // one run, because a newline is whitespace here.
        let collapses = CharacterCountOptions {
            treats_consecutive_whitespace_as_single: true,
            ..Default::default()
        };
        assert_eq!(count(collapses), 7);

        // Collapsing and dropping newlines agree on this text, because the run
        // the newlines were part of collapses to the same single space.
        let collapses_and_drops = CharacterCountOptions {
            treats_consecutive_whitespace_as_single: true,
            ignores_newlines: true,
            ..Default::default()
        };
        assert_eq!(count(collapses_and_drops), 7);

        // The two drops together leave nothing to collapse.
        assert_eq!(count(ignores_both), 5);
    }

    #[test]
    fn the_unit_changes_what_is_counted() {
        let text = "aaa \t \u{1F431}\n\r\n c";
        let count = |unit: CharacterUnit| {
            CharacterCountOptions { unit, ..Default::default() }.count(text).expect("countable")
        };
        assert_eq!(count(CharacterUnit::GraphemeCluster), 11);
        // The CRLF is two scalars where it was one cluster.
        assert_eq!(count(CharacterUnit::UnicodeScalar), 12);
        // …and the cat is two UTF-16 units where it was one scalar.
        assert_eq!(count(CharacterUnit::Utf16), 13);
    }

    #[test]
    fn normalization_is_applied_before_the_unit_is_read() {
        // A decomposed "Ä": one grapheme cluster, two scalars.
        let text = "A\u{308}";
        let scalars = |form| {
            CharacterCountOptions {
                unit: CharacterUnit::UnicodeScalar,
                normalization_form: form,
                ..Default::default()
            }
            .count(text)
            .expect("countable")
        };
        assert_eq!(scalars(None), 2);
        assert_eq!(scalars(Some(Normalization::Nfc)), 1);
        // The grapheme count does not move either way.
        assert_eq!(
            CharacterCountOptions::default().count(text).expect("countable"),
            1
        );
    }

    #[test]
    fn the_byte_unit_can_fail() {
        let text = "abc\u{72AC}\u{725B}";
        let bytes = |encoding| {
            CharacterCountOptions { unit: CharacterUnit::Byte, encoding, ..Default::default() }
                .count(text)
        };
        assert_eq!(bytes(TextEncoding::UTF8), Some(9));
        assert_eq!(
            bytes(TextEncoding::from_identifier("shift_jis").unwrap()),
            Some(7)
        );
        // The one failure the count can report: ASCII cannot spell these.
        assert_eq!(bytes(TextEncoding::from_identifier("us-ascii").unwrap()), None);

        // An empty document is zero bytes, not a failure.
        assert_eq!(bytes(TextEncoding::from_identifier("us-ascii").unwrap()), None);
        let empty = CharacterCountOptions {
            unit: CharacterUnit::Byte,
            encoding: TextEncoding::from_identifier("us-ascii").unwrap(),
            ..Default::default()
        };
        assert_eq!(empty.count(""), Some(0));
    }

    #[test]
    fn the_line_endings_the_count_drops_are_icurs_r_not_the_editors_six() {
        // `\R` also covers the vertical tab and the form feed, which no
        // document can be *written* with but which a buffer can certainly hold.
        let options = CharacterCountOptions { ignores_newlines: true, ..Default::default() };
        for ending in ["\u{B}", "\u{C}", "\u{85}", "\u{2028}", "\u{2029}", "\r\n"] {
            let text = format!("a{ending}b");
            assert_eq!(options.count(&text), Some(2), "{ending:?} was not dropped");
            // Without the option it is three, so the difference is the ending.
            assert_eq!(
                CharacterCountOptions::default().count(&text).expect("countable"),
                3,
                "{ending:?}"
            );
        }

        // …while *Ignore whitespace* takes the tab and the space separators
        // and leaves every line ending alone.
        let spaces = CharacterCountOptions { ignores_whitespaces: true, ..Default::default() };
        assert_eq!(spaces.count("a\tb"), Some(2));
        assert_eq!(spaces.count("a\u{3000}b"), Some(2));
        assert_eq!(spaces.count("a\u{A0}b"), Some(2));
        // A newline is not a space separator, so it stays.
        assert_eq!(spaces.count("a\nb"), Some(3));
        assert_eq!(spaces.count("a\u{2028}b"), Some(3));
    }
}
