//! Rich text for the clipboard — CotEditor's `Edit ▸ Copy as Rich Text`.
//!
//! Upstream's `copyWithStyle(_:)` builds an `NSAttributedString` out of the
//! selection: the plain text, plus the foreground colour the syntax highlighter
//! put on each run. Handing that to `NSPasteboard.writeObjects` makes AppKit
//! publish it as RTF, so pasting into Mail or Pages keeps the colours.
//!
//! SubText has no `NSAttributedString` and GPUI's clipboard carries text or an
//! image, never styled text. The RTF is therefore written here, by hand, and
//! put on the pasteboard through `osascript` — see `rich_text` in the
//! application crate for that half.
//!
//! Only what the highlighter can say is carried: a colour per run, one font,
//! one size. Upstream carries the typing attributes too, which in CotEditor are
//! exactly those two.

use std::fmt::Write as _;

/// A colour, eight bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// One run of text that shares a colour.
#[derive(Debug, Clone)]
pub struct Run<'a> {
    pub text: &'a str,
    /// `None` means "the document's own text colour", which RTF spells as the
    /// automatic colour (no `\cf` at all) rather than a table entry.
    pub color: Option<Rgb>,
}

/// The RTF for `runs`, in `font` at `size` points.
///
/// The colour table is built from the runs in the order their colours first
/// appear, so the same input always produces the same bytes — which is what
/// makes this testable.
pub fn rtf(runs: &[Run<'_>], font: &str, size: f32) -> String {
    let mut palette: Vec<Rgb> = Vec::new();
    for run in runs {
        if let Some(color) = run.color
            && !palette.contains(&color)
        {
            palette.push(color);
        }
    }

    let mut text = String::from("{\\rtf1\\ansi\\ansicpg1252\\deff0\\uc1\n");
    let _ = writeln!(
        text,
        "{{\\fonttbl{{\\f0\\fnil\\fcharset0 {};}}}}",
        escape_font(font)
    );
    text.push_str("{\\colortbl;");
    for color in &palette {
        let _ = write!(
            text,
            "\\red{}\\green{}\\blue{};",
            color.red, color.green, color.blue
        );
    }
    text.push_str("}\n");
    // RTF measures type in half-points, and a fractional point is not a thing
    // any font on either platform can render.
    let _ = writeln!(text, "\\f0\\fs{}", (size * 2.0).round() as i32);

    for run in runs {
        match run.color {
            // `+ 1` because entry 0 of a colour table is the automatic colour,
            // which is what a run with no colour gets by writing nothing.
            Some(color) => {
                let index = palette
                    .iter()
                    .position(|known| *known == color)
                    .unwrap_or(0)
                    + 1;
                let _ = write!(text, "\\cf{index} ");
            }
            None => text.push_str("\\cf0 "),
        }
        escape_into(&mut text, run.text);
    }

    text.push_str("}\n");
    text
}

/// Escape a run of text for an RTF body.
///
/// Everything outside ASCII becomes `\uN?` with N the **signed** 16-bit code
/// unit, which is how RTF spells anything above 0x7F; a character outside the
/// Basic Multilingual Plane becomes two of them, as UTF-16 has it.
fn escape_into(out: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            // A hard line break is a new paragraph in RTF. `\r` is dropped
            // because `\r\n` has already been handled by the `\n`.
            '\n' => out.push_str("\\par\n"),
            '\r' => {}
            '\t' => out.push_str("\\tab "),
            control if (control as u32) < 0x20 => {}
            ascii if ascii.is_ascii() => out.push(ascii),
            other => {
                let mut units = [0u16; 2];
                for unit in other.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{}?", *unit as i16);
                }
            }
        }
    }
}

/// A font name goes inside `{\fonttbl …}`, where only the delimiters and the
/// semicolon that ends the entry need care — `\u` escapes are not read there on
/// every RTF reader, so non-ASCII characters are dropped rather than mangled.
fn escape_font(font: &str) -> String {
    let mut escaped = String::with_capacity(font.len());
    for character in font.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '{' | '}' | ';' => {
                escaped.push(' ');
            }
            ascii if ascii.is_ascii() => escaped.push(ascii),
            _ => escaped.push('?'),
        }
    }
    if escaped.is_empty() {
        escaped.push_str("Courier");
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: Rgb = Rgb {
        red: 0,
        green: 0,
        blue: 255,
    };
    const RED: Rgb = Rgb {
        red: 255,
        green: 0,
        blue: 0,
    };

    #[test]
    fn plain_text_gets_the_document_colour_and_no_palette() {
        let text = rtf(
            &[Run {
                text: "hello",
                color: None,
            }],
            "Menlo",
            13.0,
        );
        assert!(text.starts_with("{\\rtf1\\ansi"), "{text}");
        assert!(
            text.contains("{\\fonttbl{\\f0\\fnil\\fcharset0 Menlo;}}"),
            "{text}"
        );
        assert!(text.contains("{\\colortbl;}"), "{text}");
        // 13 points is 26 half-points.
        assert!(text.contains("\\f0\\fs26"), "{text}");
        assert!(text.contains("\\cf0 hello"), "{text}");
        assert!(text.ends_with("}\n"), "{text}");
    }

    #[test]
    fn a_coloured_run_gets_a_table_entry_and_an_index() {
        let text = rtf(
            &[
                Run {
                    text: "fn",
                    color: Some(BLUE),
                },
                Run {
                    text: " main",
                    color: None,
                },
                Run {
                    text: "()",
                    color: Some(RED),
                },
            ],
            "Menlo",
            13.0,
        );
        // Entry 0 is the automatic colour, so the first real colour is 1.
        assert!(
            text.contains("{\\colortbl;\\red0\\green0\\blue255;\\red255\\green0\\blue0;}"),
            "{text}"
        );
        assert!(text.contains("\\cf1 fn\\cf0  main\\cf2 ()"), "{text}");
    }

    #[test]
    fn the_same_colour_twice_gets_one_table_entry() {
        let text = rtf(
            &[
                Run {
                    text: "a",
                    color: Some(BLUE),
                },
                Run {
                    text: "b",
                    color: Some(BLUE),
                },
            ],
            "Menlo",
            13.0,
        );
        assert_eq!(text.matches("\\red0\\green0\\blue255").count(), 1, "{text}");
        assert!(text.contains("\\cf1 a\\cf1 b"), "{text}");
    }

    #[test]
    fn rtf_delimiters_in_the_text_are_escaped() {
        let text = rtf(
            &[Run {
                text: "a{b}c\\d",
                color: None,
            }],
            "Menlo",
            13.0,
        );
        assert!(text.contains("a\\{b\\}c\\\\d"), "{text}");
    }

    #[test]
    fn line_breaks_become_paragraphs_and_tabs_become_tabs() {
        let text = rtf(
            &[Run {
                text: "one\ntwo\tend",
                color: None,
            }],
            "Menlo",
            13.0,
        );
        assert!(text.contains("one\\par\ntwo\\tab end"), "{text}");
    }

    #[test]
    fn text_outside_ascii_becomes_a_signed_code_unit() {
        // 中 is U+4E2D = 20013, which fits a signed 16-bit value.
        let text = rtf(
            &[Run {
                text: "中",
                color: None,
            }],
            "Menlo",
            13.0,
        );
        assert!(text.contains("\\u20013?"), "{text}");

        // U+9F99 is 40857, which is past the signed range: RTF writes it as
        // 40857 - 65536 = -24679.
        let text = rtf(
            &[Run {
                text: "龙",
                color: None,
            }],
            "Menlo",
            13.0,
        );
        assert!(text.contains("\\u-24679?"), "{text}");
    }

    #[test]
    fn an_emoji_becomes_two_code_units() {
        // U+1F600, a surrogate pair in UTF-16.
        let text = rtf(
            &[Run {
                text: "😀",
                color: None,
            }],
            "Menlo",
            13.0,
        );
        // A surrogate pair is two `\uN?` escapes, both negative because the
        // pair is at the top of the range.
        assert_eq!(text.matches("\\u-").count(), 2, "{text}");
    }

    #[test]
    fn a_missing_font_name_falls_back_to_one_rtf_readers_know() {
        assert_eq!(escape_font(""), "Courier");
        assert_eq!(escape_font("SF Mono"), "SF Mono");
        // A semicolon would end the font-table entry early.
        assert_eq!(escape_font("Bad;Name"), "Bad Name");
    }
}
