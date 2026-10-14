//! Line endings, including the Unicode separators CotEditor supports.
//!
//! CotEditor does not stop at LF/CRLF/CR: it also recognises NEL, LINE
//! SEPARATOR and PARAGRAPH SEPARATOR, and lets you convert between all six.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, serde::Serialize, serde::Deserialize)]
pub enum LineEnding {
    #[default]
    LF,
    CRLF,
    CR,
    /// U+0085 NEXT LINE
    NEL,
    /// U+2028 LINE SEPARATOR
    LS,
    /// U+2029 PARAGRAPH SEPARATOR
    PS,
}

/// Every line ending, in the order CotEditor lists them in its menu.
pub const ALL: [LineEnding; 6] =
    [LineEnding::LF, LineEnding::CRLF, LineEnding::CR, LineEnding::NEL, LineEnding::LS, LineEnding::PS];

impl LineEnding {
    /// The character sequence this ending is written as.
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::LF => "\n",
            LineEnding::CRLF => "\r\n",
            LineEnding::CR => "\r",
            LineEnding::NEL => "\u{85}",
            LineEnding::LS => "\u{2028}",
            LineEnding::PS => "\u{2029}",
        }
    }

    /// The abbreviation CotEditor shows in the status bar.
    pub fn abbreviation(self) -> &'static str {
        match self {
            LineEnding::LF => "LF",
            LineEnding::CRLF => "CRLF",
            LineEnding::CR => "CR",
            LineEnding::NEL => "NEL",
            LineEnding::LS => "LS",
            LineEnding::PS => "PS",
        }
    }

    /// The human-readable name, used in menus and settings.
    pub fn description(self) -> &'static str {
        match self {
            LineEnding::LF => "Line Feed",
            LineEnding::CRLF => "Carriage Return + Line Feed",
            LineEnding::CR => "Carriage Return",
            LineEnding::NEL => "Next Line",
            LineEnding::LS => "Line Separator",
            LineEnding::PS => "Paragraph Separator",
        }
    }

    /// The system a line ending is characteristic of.
    ///
    /// This is the parenthesised second half of CotEditor's Format ▸ Default
    /// line endings picker, kept verbatim so the two pickers read alike.
    pub fn platform_description(self) -> &'static str {
        match self {
            LineEnding::LF => "macOS / Unix",
            LineEnding::CR => "Classic Mac OS",
            LineEnding::CRLF => "Windows",
            LineEnding::NEL => "Unix Next Line",
            LineEnding::LS => "Unix Line Separator",
            LineEnding::PS => "Unix Paragraph Separator",
        }
    }

    /// Whether this is one of the three endings the Format pane offers.
    ///
    /// CotEditor hides NEL, LS and PS from that picker: they are readable and
    /// convertible but nobody creates a document with them.
    pub fn is_basic(self) -> bool {
        matches!(self, LineEnding::LF | LineEnding::CR | LineEnding::CRLF)
    }

    /// The three endings the Format pane's picker lists, in its order.
    pub const BASIC: [LineEnding; 3] = [LineEnding::LF, LineEnding::CR, LineEnding::CRLF];

    /// The Unicode scalar value of a single-character ending.
    pub fn as_char(self) -> Option<char> {
        match self {
            LineEnding::LF => Some('\n'),
            LineEnding::CR => Some('\r'),
            LineEnding::NEL => Some('\u{85}'),
            LineEnding::LS => Some('\u{2028}'),
            LineEnding::PS => Some('\u{2029}'),
            LineEnding::CRLF => None,
        }
    }

    pub fn from_abbreviation(value: &str) -> Option<Self> {
        ALL.into_iter().find(|e| e.abbreviation() == value)
    }
}

impl fmt::Display for LineEnding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.abbreviation())
    }
}

/// Whether `c` begins a line ending, and which one.
///
/// CRLF is deliberately *not* returned here: a lone `\r` may be followed by a
/// `\n`, so callers must look ahead. See [`line_ending_at`].
pub fn is_line_ending_char(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// The line ending starting at `offset` in `text`, if any.
pub fn line_ending_at(text: &str, offset: usize) -> Option<(LineEnding, usize)> {
    let rest = text.get(offset..)?;
    let mut chars = rest.chars();
    let first = chars.next()?;
    match first {
        '\n' => Some((LineEnding::LF, 1)),
        '\r' if chars.next() == Some('\n') => Some((LineEnding::CRLF, 2)),
        '\r' => Some((LineEnding::CR, 1)),
        '\u{85}' => Some((LineEnding::NEL, '\u{85}'.len_utf8())),
        '\u{2028}' => Some((LineEnding::LS, '\u{2028}'.len_utf8())),
        '\u{2029}' => Some((LineEnding::PS, '\u{2029}'.len_utf8())),
        _ => None,
    }
}

/// The line ending a document predominantly uses.
///
/// CotEditor counts occurrences and picks the winner; ties go to the ending
/// that appears first, which keeps a mixed document stable rather than
/// flip-flopping as the user edits. An empty or single-line document is LF.
pub fn detect(text: &str) -> LineEnding {
    let mut counts = [0usize; ALL.len()];
    let mut first_seen: [Option<usize>; ALL.len()] = [None; ALL.len()];

    let mut offset = 0;
    while offset < text.len() {
        match line_ending_at(text, offset) {
            Some((ending, len)) => {
                let index = index_of(ending);
                counts[index] += 1;
                if first_seen[index].is_none() {
                    first_seen[index] = Some(offset);
                }
                offset += len;
            }
            None => {
                let step = text[offset..].chars().next().map_or(1, char::len_utf8);
                offset += step;
            }
        }
    }

    let mut best = LineEnding::LF;
    let mut best_count = 0;
    let mut best_position = usize::MAX;
    for (index, ending) in ALL.into_iter().enumerate() {
        let count = counts[index];
        if count == 0 {
            continue;
        }
        let position = first_seen[index].unwrap_or(usize::MAX);
        if count > best_count || (count == best_count && position < best_position) {
            best = ending;
            best_count = count;
            best_position = position;
        }
    }
    best
}

fn index_of(ending: LineEnding) -> usize {
    ALL.iter().position(|e| *e == ending).unwrap_or(0)
}

/// Rewrite every line ending in `text` as `ending`.
pub fn convert(text: &str, ending: LineEnding) -> String {
    let mut out = String::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        match line_ending_at(text, offset) {
            Some((_, len)) => {
                out.push_str(ending.as_str());
                offset += len;
            }
            None => {
                let c = text[offset..].chars().next().expect("offset is on a char boundary");
                out.push(c);
                offset += c.len_utf8();
            }
        }
    }
    out
}

/// Split `text` into lines, discarding the line endings.
///
/// Unlike [`str::lines`], this understands all six endings and keeps a
/// trailing empty line, so `"a\n"` is two lines — which is what a text editor
/// must show.
pub fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut offset = 0;
    while offset < text.len() {
        match line_ending_at(text, offset) {
            Some((_, len)) => {
                lines.push(&text[start..offset]);
                offset += len;
                start = offset;
            }
            None => {
                offset += text[offset..].chars().next().map_or(1, char::len_utf8);
            }
        }
    }
    lines.push(&text[start..]);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_crlf_and_counts_lines() {
        assert_eq!(detect("a\r\nb\r\nc"), LineEnding::CRLF);
        assert_eq!(split_lines("a\r\nb\r\nc"), vec!["a", "b", "c"]);
    }

    #[test]
    fn trailing_newline_yields_a_final_empty_line() {
        assert_eq!(split_lines("a\n"), vec!["a", ""]);
        assert_eq!(split_lines(""), vec![""]);
    }

    #[test]
    fn crlf_is_not_split_into_two_endings() {
        assert_eq!(detect("\r\n\r\n\r\n"), LineEnding::CRLF);
        assert_eq!(convert("\r\n", LineEnding::LF), "\n");
    }

    #[test]
    fn unicode_separators_are_line_endings() {
        assert_eq!(detect("a\u{2028}b\u{2028}c"), LineEnding::LS);
        assert_eq!(split_lines("a\u{2028}b"), vec!["a", "b"]);
        assert_eq!(convert("a\u{2028}b", LineEnding::LF), "a\nb");
    }

    #[test]
    fn majority_wins_on_mixed_documents() {
        assert_eq!(detect("a\r\nb\r\nc\n"), LineEnding::CRLF);
    }

    #[test]
    fn conversion_is_idempotent() {
        let converted = convert("a\r\nb\rc\u{85}d", LineEnding::LF);
        assert_eq!(converted, "a\nb\nc\nd");
        assert_eq!(convert(&converted, LineEnding::LF), converted);
    }

    #[test]
    fn the_basic_set_is_what_the_format_pane_offers() {
        assert_eq!(LineEnding::BASIC, [LineEnding::LF, LineEnding::CR, LineEnding::CRLF]);
        for ending in LineEnding::BASIC {
            assert!(ending.is_basic(), "{ending:?} should be basic");
        }
        for ending in [LineEnding::NEL, LineEnding::LS, LineEnding::PS] {
            assert!(!ending.is_basic(), "{ending:?} should not be basic");
        }
    }

    #[test]
    fn every_ending_has_a_platform_description() {
        for ending in ALL {
            assert!(
                !ending.platform_description().is_empty(),
                "{ending:?} has no platform description"
            );
        }
        assert_eq!(LineEnding::LF.platform_description(), "macOS / Unix");
        assert_eq!(LineEnding::CR.platform_description(), "Classic Mac OS");
        assert_eq!(LineEnding::CRLF.platform_description(), "Windows");
    }
}
