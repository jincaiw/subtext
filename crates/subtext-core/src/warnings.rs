//! The two scans behind CotEditor's **Warnings** inspector.
//!
//! Both are *scans of the buffer*, not of the file: they answer "what would go
//! wrong if this text were written out as it is", which is why they run on
//! every edit rather than when a file is opened.
//!
//! # Incompatible characters
//!
//! A character is incompatible when it does not survive a **lossy round trip**
//! through the document's encoding — encode it (substituting where it cannot be
//! represented), decode the bytes back, and compare. That catches two different
//! problems in one rule: characters the encoding has no room for, and
//! characters it can spell but spells as something else.
//!
//! The scan bisects: the whole buffer is tested first, and any chunk that
//! differs is halved on a character boundary until the offending character is
//! alone. Testing every character one at a time would be simpler and far
//! slower — the round trip is a codec call, and most documents are almost
//! entirely compatible.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::encoding::TextEncoding;
use crate::line_ending::{self, LineEnding};

/// A character that will not survive a round trip through the document's
/// encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompatibleCharacter {
    /// The character as it is in the buffer — one *character*, not one scalar
    /// value, which is what upstream's `Character` is too: an emoji built from
    /// several code points arrives here whole.
    pub character: String,
    /// What the encoding turns it into, or `None` when the text cannot be
    /// written at all. CotEditor shows `�` for the character in that case.
    pub converted: Option<String>,
    /// Where it sits, in bytes.
    pub range: Range<usize>,
}

/// Every character in `text` that `encoding` cannot round-trip.
///
/// Empty for an empty buffer and for a Unicode encoding, which is upstream's
/// own guard: nothing is incompatible with UTF-8, and scanning it would be work
/// for nothing.
pub fn incompatible_characters(text: &str, encoding: TextEncoding) -> Vec<IncompatibleCharacter> {
    if text.is_empty() || encoding.is_unicode() {
        return Vec::new();
    }
    let mut found = Vec::new();
    find_incompatible(text, 0..text.len(), encoding, &mut found);
    found
}

/// The bisecting search. See the module note.
fn find_incompatible(
    text: &str,
    range: Range<usize>,
    encoding: TextEncoding,
    found: &mut Vec<IncompatibleCharacter>,
) {
    let chunk = &text[range.clone()];
    if chunk.is_empty() {
        return;
    }

    let converted = round_trip(chunk, encoding);
    match &converted {
        // The chunk survives unchanged, so nothing inside it can be wrong.
        Some(converted) if converted == chunk => return,
        // A single character that changes, or that cannot be written: that is
        // the answer.
        _ if is_one_character(chunk) => {
            found.push(IncompatibleCharacter {
                character: chunk.to_string(),
                converted,
                range,
            });
            return;
        }
        // Otherwise the offender is somewhere inside: halve and recurse.
        _ => {}
    }

    let middle = split_point(chunk, range.start);
    if middle <= range.start || middle >= range.end {
        return;
    }
    find_incompatible(text, range.start..middle, encoding, found);
    find_incompatible(text, middle..range.end, encoding, found);
}

/// Encode `chunk` lossily and decode the bytes back.
///
/// `None` is upstream's "could not be produced", which it reaches when the
/// decoder rejects the bytes the encoder wrote.
fn round_trip(chunk: &str, encoding: TextEncoding) -> Option<String> {
    let (bytes, _) = encoding.encode(chunk);
    let (decoded, had_errors) = encoding.decode(&bytes);
    (!had_errors).then_some(decoded)
}

/// Whether `chunk` is exactly one extended grapheme cluster.
fn is_one_character(chunk: &str) -> bool {
    let mut characters = chunk.graphemes(true);
    characters.next().is_some() && characters.next().is_none()
}

/// Where to cut `chunk` in half, snapped to a character boundary.
///
/// Counted in grapheme clusters rather than bytes, so a chunk of two-character
/// clusters splits between them instead of inside one.
fn split_point(chunk: &str, start: usize) -> usize {
    let characters = chunk.graphemes(true).count();
    if characters < 2 {
        return start;
    }
    start
        + chunk
            .grapheme_indices(true)
            .nth(characters / 2)
            .map_or(chunk.len(), |(at, _)| at)
}

/// The line endings in `text` that are **not** `base`, with their ranges.
///
/// CotEditor's `LineEndingScanner.inconsistentLineEndings`: it keeps every line
/// ending the document's own ending does not match, in the order they appear,
/// so the inspector can list them and the editor can tint them.
pub fn inconsistent_line_endings(text: &str, base: LineEnding) -> Vec<(Range<usize>, LineEnding)> {
    let mut found = Vec::new();
    let mut offset = 0;
    while offset < text.len() {
        match line_ending::line_ending_at(text, offset) {
            Some((ending, length)) => {
                if ending != base {
                    found.push((offset..offset + length, ending));
                }
                offset += length;
            }
            None => offset += text[offset..].chars().next().map_or(1, char::len_utf8),
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shift_jis() -> TextEncoding {
        TextEncoding::from_identifier("shift_jis").expect("the registry has Shift JIS")
    }

    fn ascii() -> TextEncoding {
        TextEncoding::from_identifier("us-ascii").expect("the registry has ASCII")
    }

    #[test]
    fn a_unicode_document_has_nothing_incompatible() {
        assert!(incompatible_characters("", TextEncoding::UTF8).is_empty());
        assert!(incompatible_characters("abc😀", TextEncoding::UTF8).is_empty());
        // UTF-16 is a Unicode encoding too, so it is skipped as well.
        assert!(
            incompatible_characters("abc😀", TextEncoding::from_identifier("utf-16le").unwrap())
                .is_empty()
        );
    }

    #[test]
    fn text_the_encoding_can_spell_has_nothing_incompatible() {
        // Upstream's `scanOnlyCompatibleCharacters`.
        assert!(incompatible_characters("Just ASCII text 12345.", ascii()).is_empty());
    }

    #[test]
    fn characters_the_encoding_cannot_spell_are_reported_one_by_one() {
        // Upstream's `scanOnlyIncompatibleCharacters`: every character that
        // does not survive the round trip is its own entry, in order, and each
        // has something it converts to. `"👾🐱‍👓"` is two *characters* — the cat,
        // the zero-width joiner and the glasses are one cluster between them —
        // and both of them are incompatible.
        let text = "👾🐱\u{200D}👓";
        assert_eq!(text.graphemes(true).count(), 2);
        let found = incompatible_characters(text, shift_jis());
        assert_eq!(found.len(), 2);
        for item in &found {
            assert!(item.converted.is_some(), "{item:?}");
            assert_eq!(text[item.range.clone()], item.character);
        }
        assert_eq!(found[0].range.start, 0);
        assert_eq!(found[1].range.start, 4);
    }

    #[test]
    fn a_run_of_them_does_not_hide_the_ones_after_it() {
        // The bisection is the thing under test: two offenders side by side
        // have to come back as two entries, not one.
        let found = incompatible_characters("aa👾👾bb", shift_jis());
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].character, "👾");
        assert_eq!(found[1].character, "👾");
        assert_eq!(found[0].range.end, found[1].range.start);
        assert_eq!(found[1].range.end, "aa👾👾".len());
    }

    #[test]
    fn the_ranges_are_byte_offsets_into_the_buffer() {
        // Byte offsets, which is what Rust's `str` indexes by. CotEditor's are
        // UTF-16 offsets, so the two numbers differ for any character outside
        // the BMP — the *character* is what has to agree.
        let text = "hello👾world";
        let found = incompatible_characters(text, shift_jis());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].character, "👾");
        assert_eq!(&text[found[0].range.clone()], "👾");
        assert_eq!(found[0].range, 5..9);
    }

    #[test]
    fn the_line_endings_that_do_not_match_the_base_are_listed() {
        // Upstream's `LineEndingScannerTests.scan`: `"dog\u{85}cow\ncat\r\ncow"`
        // with LF as the document's ending gives the NEL and the CRLF. The
        // offsets here are *bytes* where upstream's are UTF-16 code units, so
        // the NEL occupies two of them.
        let text = "dog\u{85}cow\ncat\r\ncow";
        assert_eq!(
            inconsistent_line_endings(text, LineEnding::LF),
            vec![(3..5, LineEnding::NEL), (12..14, LineEnding::CRLF),]
        );
        // …and the character at the start of each range is the ending itself.
        for (range, ending) in inconsistent_line_endings(text, LineEnding::LF) {
            assert_eq!(&text[range], ending.as_str());
        }

        // Nothing but the base ending: nothing to report.
        assert!(inconsistent_line_endings("dog\ncat\n", LineEnding::LF).is_empty());
        // An empty document has no line endings at all.
        assert!(inconsistent_line_endings("", LineEnding::LF).is_empty());
    }

    #[test]
    fn the_crlf_is_one_line_ending_and_not_two() {
        // `"\r\n"` must not be reported as a stray CR followed by a stray LF.
        let text = "a\r\nb";
        assert!(inconsistent_line_endings(text, LineEnding::CRLF).is_empty());
        assert_eq!(
            inconsistent_line_endings(text, LineEnding::LF),
            vec![(1..3, LineEnding::CRLF)]
        );
    }

    #[test]
    fn every_line_ending_is_recognized() {
        // The six, each one inconsistent with LF.
        let text = "\n\r\r\n\u{85}\u{2028}\u{2029}";
        let found = inconsistent_line_endings(text, LineEnding::LF);
        let endings: Vec<LineEnding> = found.iter().map(|(_, ending)| *ending).collect();
        assert_eq!(
            endings,
            vec![
                LineEnding::CR,
                LineEnding::CRLF,
                LineEnding::NEL,
                LineEnding::LS,
                LineEnding::PS,
            ]
        );
    }
}

#[cfg(test)]
mod debug_probe {
    use super::*;

    #[test]
    fn print_converted() {
        for id in ["shift_jis", "windows-1252", "us-ascii", "euc-jp"] {
            let encoding = TextEncoding::from_identifier(id).unwrap();
            for text in ["👾", "あ", "é", "€"] {
                let found = incompatible_characters(text, encoding);
                println!(
                    "{id} {text:?} -> {:?}",
                    found
                        .iter()
                        .map(|f| f.converted.clone())
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

#[cfg(test)]
mod debug_list {
    use crate::encoding;

    #[test]
    fn print_available() {
        for e in encoding::AVAILABLE.iter() {
            println!("ENC\t{}\t{}", e.name, e.identifier());
        }
    }
}
