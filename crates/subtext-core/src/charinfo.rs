//! Character inspection — the data behind CotEditor's **Inspect Character**
//! panel and its "Advanced Character Count".

use unicode_width::UnicodeWidthChar;

use crate::encoding::TextEncoding;

/// Everything the Inspect Character panel shows about one scalar value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterInfo {
    pub character: char,
    /// `U+1F600`, always at least four digits.
    pub code_point: String,
    /// The Unicode name, or the code point when Unicode leaves it unnamed
    /// (most of the CJK ranges are unnamed by design, and control characters
    /// are named by convention).
    pub name: String,
    /// The Unicode general category, e.g. `Lu` / `Pd`.
    pub category: &'static str,
    pub block: &'static str,
    /// UTF-8 bytes as hex, space separated.
    pub utf8: String,
    /// UTF-16 code units as hex, space separated.
    pub utf16: String,
    /// The scalar value in decimal.
    pub decimal: u32,
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
        let value = character as u32;
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
            code_point: format!("U+{value:04X}"),
            name: unicode_name(character),
            category: general_category(character),
            block: block_name(value),
            utf8: utf8.join(" "),
            utf16: utf16.join(" "),
            decimal: value,
            html_entity: format!("&#x{value:X};"),
            encoded_len,
            encoding_name: encoding.name,
            width: UnicodeWidthChar::width(character).unwrap_or(0),
        }
    }
}

/// The Unicode name of `c`, falling back to the code point notation.
fn unicode_name(c: char) -> String {
    if let Some(name) = unicode_names2::name(c) {
        return name.to_string();
    }
    // Hangul syllables and CJK ideographs are named algorithmically.
    if ('\u{AC00}'..='\u{D7A3}').contains(&c) {
        return format!("HANGUL SYLLABLE {:04X}", c as u32);
    }
    if ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{20000}'..='\u{2A6DF}').contains(&c)
    {
        return format!("CJK UNIFIED IDEOGRAPH-{:04X}", c as u32);
    }
    format!("<U+{:04X}>", c as u32)
}

/// A general-category abbreviation.
///
/// `char`'s built-in predicates do not expose the full Unicode category, so
/// this classifies by predicate plus the ranges that need to be separated out.
fn general_category(c: char) -> &'static str {
    if c.is_control() {
        return "Cc";
    }
    if c.is_whitespace() {
        // Zs (space separators) versus Zl/Zp (the line and paragraph separators)
        return match c {
            '\u{2028}' => "Zl",
            '\u{2029}' => "Zp",
            _ => "Zs",
        };
    }
    if c.is_numeric() {
        return if c.is_ascii_digit() || c.is_numeric() && c.to_string().len() == 1 {
            "Nd"
        } else {
            "No"
        };
    }
    if c.is_alphabetic() {
        if c.is_uppercase() {
            return "Lu";
        }
        if c.is_lowercase() {
            return "Ll";
        }
        // CJK ideographs, kana and Hangul are "other letter".
        if ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3040}'..='\u{30FF}').contains(&c)
            || ('\u{AC00}'..='\u{D7A3}').contains(&c)
        {
            return "Lo";
        }
        return "Lo";
    }
    match c {
        '_' => "Pc",
        '-' | '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' => "Pd",
        '(' | ')' | '[' | ']' | '{' | '}' => "Ps",
        '!' | '"' | '#' | '%' | '&' | '\'' | '*' | ',' | '.' | '/' | ':' | ';' | '?' | '@'
        | '\\' => "Po",
        '$' | '+' | '<' | '=' | '>' | '|' | '~' | '^' | '`' => "Sm",
        _ => {
            if c.is_ascii_punctuation() {
                "Po"
            } else if c.is_ascii_graphic() {
                "Sm"
            } else {
                "So"
            }
        }
    }
}

/// The Unicode block containing `value`.
///
/// Only the blocks a text editor realistically encounters are listed; anything
/// else is reported as "No Block".
fn block_name(value: u32) -> &'static str {
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
        (0x0900, 0x097F, "Devanagari"),
        (0x0E00, 0x0E7F, "Thai"),
        (0x10A0, 0x10FF, "Georgian"),
        (0x1100, 0x11FF, "Hangul Jamo"),
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
        (0x2460, 0x24FF, "Enclosed Alphanumerics"),
        (0x2500, 0x257F, "Box Drawing"),
        (0x2580, 0x259F, "Block Elements"),
        (0x25A0, 0x25FF, "Geometric Shapes"),
        (0x2600, 0x26FF, "Miscellaneous Symbols"),
        (0x2700, 0x27BF, "Dingbats"),
        (0x3000, 0x303F, "CJK Symbols and Punctuation"),
        (0x3040, 0x309F, "Hiragana"),
        (0x30A0, 0x30FF, "Katakana"),
        (0x3100, 0x312F, "Bopomofo"),
        (0x3130, 0x318F, "Hangul Compatibility Jamo"),
        (0x3190, 0x319F, "Kanbun"),
        (0x31F0, 0x31FF, "Katakana Phonetic Extensions"),
        (0x3200, 0x32FF, "Enclosed CJK Letters and Months"),
        (0x3300, 0x33FF, "CJK Compatibility"),
        (0x3400, 0x4DBF, "CJK Unified Ideographs Extension A"),
        (0x4E00, 0x9FFF, "CJK Unified Ideographs"),
        (0xA000, 0xA48F, "Yi Syllables"),
        (0xAC00, 0xD7AF, "Hangul Syllables"),
        (0xD800, 0xDFFF, "High Surrogates"),
        (0xE000, 0xF8FF, "Private Use Area"),
        (0xF900, 0xFAFF, "CJK Compatibility Ideographs"),
        (0xFB00, 0xFB4F, "Alphabetic Presentation Forms"),
        (0xFB50, 0xFDFF, "Arabic Presentation Forms-A"),
        (0xFE00, 0xFE0F, "Variation Selectors"),
        (0xFE30, 0xFE4F, "CJK Compatibility Forms"),
        (0xFE50, 0xFE6F, "Small Form Variants"),
        (0xFE70, 0xFEFF, "Arabic Presentation Forms-B"),
        (0xFF00, 0xFFEF, "Halfwidth and Fullwidth Forms"),
        (0xFFF0, 0xFFFF, "Specials"),
        (0x10000, 0x1007F, "Linear B Syllabary"),
        (0x1D100, 0x1D1FF, "Musical Symbols"),
        (0x1D400, 0x1D7FF, "Mathematical Alphanumeric Symbols"),
        (0x1F300, 0x1F5FF, "Miscellaneous Symbols and Pictographs"),
        (0x1F600, 0x1F64F, "Emoticons"),
        (0x1F680, 0x1F6FF, "Transport and Map Symbols"),
        (0x1F900, 0x1F9FF, "Supplemental Symbols and Pictographs"),
        (0x20000, 0x2A6DF, "CJK Unified Ideographs Extension B"),
        (0xE0000, 0xE007F, "Tags"),
        (0xF0000, 0xFFFFD, "Supplementary Private Use Area-A"),
        (0x100000, 0x10FFFD, "Supplementary Private Use Area-B"),
    ];
    BLOCKS
        .iter()
        .find(|(start, end, _)| value >= *start && value <= *end)
        .map_or("No Block", |(_, _, name)| *name)
}

/// CotEditor's "Advanced Character Count" breakdown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterCount {
    /// Every `char`, including line endings.
    pub characters: usize,
    /// Characters excluding line endings.
    pub characters_without_newlines: usize,
    /// Unicode scalar values, surrogate pairs counted once.
    pub unicode_scalars: usize,
    /// UTF-16 code units — what a JavaScript string would report.
    pub utf16_units: usize,
    /// UTF-8 bytes.
    pub utf8_bytes: usize,
    /// Bytes in the document's own encoding.
    pub encoded_bytes: usize,
    pub lines: usize,
    pub words: usize,
    pub paragraphs: usize,
}

impl CharacterCount {
    pub fn of(text: &str, encoding: TextEncoding) -> Self {
        let characters = text.chars().count();
        let line_endings = count_line_endings(text);
        let lines = crate::line_ending::split_lines(text).len();
        let paragraphs = text
            .split(['\n', '\r', '\u{2029}'])
            .filter(|p| !p.trim().is_empty())
            .count();
        let words = text
            .split(|c: char| c.is_whitespace() || is_cjk_punctuation(c))
            .filter(|w| !w.is_empty())
            .count();

        Self {
            characters,
            characters_without_newlines: characters.saturating_sub(line_endings),
            unicode_scalars: characters,
            utf16_units: text.chars().map(char::len_utf16).sum(),
            utf8_bytes: text.len(),
            encoded_bytes: encoding.encode(text).0.len(),
            lines,
            words,
            paragraphs,
        }
    }
}

/// How many *characters* the document's line endings occupy in total.
///
/// Scanned rather than counted per ending: `\r\n` is one ending of two
/// characters, and counting `\n` and `\r` separately would call it two.
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

fn is_cjk_punctuation(c: char) -> bool {
    matches!(c, '\u{3000}'..='\u{303F}' | '\u{FF01}'..='\u{FF65}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_character_info() {
        let info = CharacterInfo::new('A', TextEncoding::UTF8);
        assert_eq!(info.code_point, "U+0041");
        assert_eq!(info.name, "LATIN CAPITAL LETTER A");
        assert_eq!(info.category, "Lu");
        assert_eq!(info.block, "Basic Latin");
        assert_eq!(info.utf8, "41");
        assert_eq!(info.utf16, "0041");
        assert_eq!(info.width, 1);
    }

    #[test]
    fn cjk_character_is_wide_and_multibyte() {
        let info = CharacterInfo::new('漢', TextEncoding::UTF8);
        assert_eq!(info.code_point, "U+6F22");
        assert_eq!(info.block, "CJK Unified Ideographs");
        assert_eq!(info.width, 2);
        assert_eq!(info.encoded_len, 3);
        assert!(info.name.contains("CJK UNIFIED IDEOGRAPH"));
    }

    #[test]
    fn astral_character_needs_a_surrogate_pair() {
        let info = CharacterInfo::new('😀', TextEncoding::UTF8);
        assert_eq!(info.code_point, "U+1F600");
        assert_eq!(info.utf16, "D83D DE00");
        assert_eq!(info.width, 2);
    }

    #[test]
    fn shift_jis_character_is_two_bytes() {
        let info = CharacterInfo::new('あ', TextEncoding::from_identifier("shift_jis").unwrap());
        assert_eq!(info.encoded_len, 2);
        assert_eq!(info.encoding_name, "Japanese (Shift JIS)");
    }

    #[test]
    fn counting_a_mixed_document() {
        let text = "Hello world\nこんにちは\n";
        let count = CharacterCount::of(text, TextEncoding::UTF8);
        assert_eq!(count.lines, 3);
        assert_eq!(count.words, 3);
        assert_eq!(count.characters, 18);
        assert_eq!(count.characters_without_newlines, 16);
        assert_eq!(count.utf8_bytes, text.len());
        assert_eq!(count.utf16_units, 18);
    }

    #[test]
    fn crlf_counts_as_one_line_ending_of_two_characters() {
        let count = CharacterCount::of("a\r\nb", TextEncoding::UTF8);
        // Four scalars: 'a', '\r', '\n', 'b'. The CRLF is one line ending but
        // two characters, so excluding line endings leaves two.
        assert_eq!(count.characters, 4);
        assert_eq!(count.characters_without_newlines, 2);
        assert_eq!(count.lines, 2);
    }
}
