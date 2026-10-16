//! The rows of the **Find All** result table.
//!
//! CotEditor's find panel is a vertical split of three parts, and the middle one
//! is a table of the matches: one row per match, two columns — the 1-based line
//! number and the matched line, cut short so the match itself is visible.
//!
//! The panel is collapsed until *Find All* finds something, and the table is
//! built from the same match list the editor highlights, so nothing here
//! searches. This module only turns `[Range<usize>]` into rows.
//!
//! # The three numbers upstream keeps per row
//!
//! Upstream's `FindAllMatch` carries `range`, `lineNumber`, `inlineLocation` and
//! `attributedLineString`, and they come from three different places:
//!
//! | field | source |
//! | --- | --- |
//! | `range` | the match, in whole-document offsets |
//! | `lineNumber` | `LineCounter.lineNumber(at:)`, 1-based |
//! | `inlineLocation` | `match.start - fragment.start`, an offset **into the row's own string** |
//! | `attributedLineString` | the line's content, clamped around the match |
//!
//! `inlineLocation` is the one worth naming: it is not a document offset, and
//! the cell renderer's head truncation is the only thing that reads it.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::textops::{LineIndex, floor_char_boundary};

/// The most bytes kept from one line before it is cut short.
///
/// Upstream's `clamped(around:maxLength: 1024)` — a guard against building a
/// megabyte-long cell out of a minified file's single line.
pub const MAX_LINE_FRAGMENT: usize = 1024;

/// How many characters are kept before the match when a line is cut.
///
/// Upstream's `headPadding`, defaulting to 64.
pub const HEAD_PADDING: usize = 64;

/// How many characters the **table cell** keeps before the match.
///
/// Upstream's `attributedLineString(offset: 16)`. Smaller than
/// [`HEAD_PADDING`] on purpose: the fragment keeps 64 characters so the cut is
/// not visible at typical widths, and the cell then trims that down to 16 so a
/// row reads as `…the words right before the match` rather than as the start of
/// the line.
pub const CELL_HEAD_OFFSET: usize = 16;

/// One row of the result table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchLine {
    /// The match, in whole-document byte offsets.
    pub range: Range<usize>,
    /// 1-based line number of the match.
    pub line_number: usize,
    /// Where the match starts inside [`Self::line`].
    pub inline_location: usize,
    /// The matched line, clamped around the match.
    pub line: String,
    /// Each capture group's range *inside* [`Self::line`], group 0 first —
    /// `None` where a group did not participate or fell outside the clamped
    /// fragment. Upstream paints these with the decomposed highlighter colours
    /// as background attributes on the cell's attributed string.
    pub capture_ranges: Vec<Option<Range<usize>>>,
}

/// Build one row per match.
pub fn match_lines(text: &str, matches: &[Range<usize>]) -> Vec<MatchLine> {
    match_lines_with_captures(
        text,
        &matches
            .iter()
            .map(|range| vec![Some(range.clone())])
            .collect::<Vec<_>>(),
    )
}

/// Build one row per match, carrying each match's capture-group ranges.
///
/// Every entry of `matches` is one match's capture array, group 0 first —
/// what [`crate::finder::Searcher::find_all_with_captures`] produces. Group 0
/// always participates, so it is also the row's [`MatchLine::range`].
pub fn match_lines_with_captures(
    text: &str,
    matches: &[Vec<Option<Range<usize>>>],
) -> Vec<MatchLine> {
    let index = LineIndex::new(text);
    matches
        .iter()
        .filter_map(|captures| {
            let range = captures.first()?.as_ref()?;
            let (line_number, content) = index.locate(range.start);
            let fragment = clamp_around(&content, range, MAX_LINE_FRAGMENT, HEAD_PADDING);
            let capture_ranges = captures
                .iter()
                .map(|capture| {
                    capture.as_ref().and_then(|capture| {
                        let start = capture.start.max(fragment.start);
                        let end = capture.end.min(fragment.end);
                        (start < end).then_some(start - fragment.start..end - fragment.start)
                    })
                })
                .collect();
            Some(MatchLine {
                range: range.clone(),
                line_number,
                inline_location: range.start.saturating_sub(fragment.start),
                capture_ranges,
                line: text[fragment].to_string(),
            })
        })
        .collect()
}

/// The cell text for one row: the line, with its head cut short when the match
/// sits further in than `offset` characters.
///
/// Upstream's `truncatedHead(until:offset:)` — a leading `…` replaces everything
/// more than `offset` composed characters before the match, so the match stays
/// visible in a narrow column instead of scrolling off the right edge.
pub fn truncate_head(line: &str, inline_location: usize, offset: usize) -> String {
    truncate_head_with_ranges(line, inline_location, offset, &[]).0
}

/// Truncate a line at the same grapheme boundary as [`truncate_head`] and
/// translate any character ranges into the returned string's byte offsets.
///
/// This is for annotated text such as capture-group highlights: the displayed
/// string begins with a three-byte ellipsis, so the source ranges need to move
/// with the visible tail rather than being applied to the pre-truncation line.
/// Ranges that the cut removed come back as `None`, the way upstream drops a
/// capture whose intersection with the visible fragment is empty.
pub fn truncate_head_with_ranges(
    line: &str,
    inline_location: usize,
    offset: usize,
    ranges: &[Option<Range<usize>>],
) -> (String, Vec<Option<Range<usize>>>) {
    let Some(boundary) = head_truncation_boundary(line, inline_location, offset) else {
        return (line.to_string(), ranges.to_vec());
    };

    let mut truncated = String::with_capacity(line.len() - boundary + 3);
    truncated.push('…');
    truncated.push_str(&line[boundary..]);
    let translated = ranges
        .iter()
        .map(|range| {
            let range = range.as_ref()?;
            let start = range.start.max(boundary);
            let end = range.end.min(line.len());
            (start < end).then_some(3 + start - boundary..3 + end - boundary)
        })
        .collect();
    (truncated, translated)
}

/// The source byte index to replace with the ellipsis, if this line must be
/// truncated. `None` means preserve the complete line.
fn head_truncation_boundary(line: &str, inline_location: usize, offset: usize) -> Option<usize> {
    if inline_location <= offset {
        return None;
    }

    let mut boundary = cluster_start(line, inline_location);
    for _ in 0..offset {
        if boundary == 0 {
            return None;
        }
        boundary = cluster_start(line, boundary - 1);
    }
    (boundary > 0).then_some(boundary)
}

/// Cut `content` down to `max_length` bytes around `target`.
///
/// Upstream's `clamped(around:maxLength:headPadding:)`: when the line is longer
/// than the limit, keep up to `head_padding` bytes before the match and fill the
/// rest of the budget after it. A line already within the limit is returned
/// untouched, so the common case costs nothing.
fn clamp_around(
    content: &Range<usize>,
    target: &Range<usize>,
    max_length: usize,
    head_padding: usize,
) -> Range<usize> {
    if content.len() <= max_length {
        return content.clone();
    }
    let effective_head = head_padding.min(target.start.saturating_sub(content.start));
    let start = content.start.max(target.start.saturating_sub(effective_head));
    let end = content.end.min(start + max_length);
    start..end
}

/// The byte index where the grapheme cluster containing `index` starts.
///
/// `index` is first moved down to a character boundary, so an index inside a
/// multi-byte scalar belongs to the cluster that scalar is in. An index at or
/// past the end returns the end itself, which makes "walk back from the end"
/// fall out of the same helper.
fn cluster_start(text: &str, index: usize) -> usize {
    let index = floor_char_boundary(text, index);
    if index == 0 || index >= text.len() {
        return index;
    }
    text.grapheme_indices(true)
        .take_while(|(start, _)| *start <= index)
        .last()
        .map_or(0, |(start, _)| start)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `match_lines` for a single match.
    ///
    /// A helper rather than `&[a..b]`: a one-element array of ranges reads to
    /// Clippy as an attempt to build a `Vec` out of a range, and
    /// `std::slice::from_ref` says what is meant without an `allow`.
    fn rows_for(text: &str, range: Range<usize>) -> Vec<MatchLine> {
        match_lines(text, std::slice::from_ref(&range))
    }

    #[test]
    fn a_row_carries_the_line_number_and_the_match_offset_inside_the_line() {
        let text = "alpha\nbeta alpha\ngamma\n";
        // "alpha" at 0..5, the one inside "beta alpha" at 11..16.
        let rows = match_lines(text, &[0..5, 11..16]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].line_number, 1);
        assert_eq!(rows[0].line, "alpha");
        assert_eq!(rows[0].inline_location, 0);
        assert_eq!(rows[1].line_number, 2);
        assert_eq!(rows[1].line, "beta alpha");
        // "beta " is five characters, so the match starts at 5 inside the line
        // even though it starts at 11 in the document.
        assert_eq!(rows[1].inline_location, 5);
    }

    #[test]
    fn line_numbers_count_the_unicode_terminators_too() {
        // The status bar and this table share Foundation's six-terminator view.
        // `\u{2028}` is LS; a three-terminator scanner would call the last line
        // line 2 here.
        let text = "one\u{2028}two\u{2028}three";
        let rows = rows_for(text, 14..19);
        assert_eq!(rows[0].line_number, 3);
        assert_eq!(rows[0].line, "three");
    }

    #[test]
    fn a_carriage_return_line_feed_counts_as_one_line() {
        let text = "one\r\ntwo\r\nthree";
        // "one\r\n" and "two\r\n" are five bytes each, so "three" starts at 10.
        let rows = rows_for(text, 10..15);
        assert_eq!(rows[0].line_number, 3);
        assert_eq!(rows[0].inline_location, 0);
        assert_eq!(rows[0].line, "three");
    }

    #[test]
    fn an_offset_inside_a_carriage_return_line_feed_stays_on_the_first_line() {
        // Upstream: "If the LF-side index of a CRLF pair is passed, it is
        // treated as part of the previous line."
        let text = "one\r\ntwo";
        let rows = rows_for(text, 4..4);
        assert_eq!(rows[0].line_number, 1);
        assert_eq!(rows[0].line, "one");
        assert_eq!(rows[0].inline_location, 4);
    }

    #[test]
    fn a_line_longer_than_the_limit_keeps_sixty_four_characters_before_the_match() {
        let long = "x".repeat(200) + "MATCH" + &"y".repeat(2000);
        let rows = rows_for(&long, 200..205);
        assert_eq!(rows[0].line.len(), MAX_LINE_FRAGMENT);
        assert_eq!(rows[0].inline_location, HEAD_PADDING);
        assert_eq!(rows[0].line_number, 1);
    }

    #[test]
    fn a_line_within_the_limit_is_left_whole() {
        let text = "short line with a match";
        let rows = rows_for(text, 15..20);
        assert_eq!(rows[0].line, text);
        assert_eq!(rows[0].inline_location, 15);
    }

    #[test]
    fn a_match_within_the_head_offset_is_not_truncated() {
        // 16 characters before the match is the boundary; at it, nothing is cut.
        let line = "abcdefghijklmnopMATCH";
        assert_eq!(truncate_head(line, 16, CELL_HEAD_OFFSET), line);
    }

    #[test]
    fn a_match_past_the_head_offset_loses_its_head_to_an_ellipsis() {
        let line = "abcdefghijklmnopqMATCH";
        // 17 characters before the match: keep 16 of them.
        assert_eq!(truncate_head(line, 17, CELL_HEAD_OFFSET), "…bcdefghijklmnopqMATCH");
    }

    #[test]
    fn truncating_a_highlighted_line_rebases_capture_ranges_after_the_ellipsis() {
        let line = "abcdefghijklmnopqMATCH";
        let groups = vec![Some(17..22), Some(17..22), None];
        let (shown, shifted) = truncate_head_with_ranges(line, 17, CELL_HEAD_OFFSET, &groups);
        assert_eq!(shown, "…bcdefghijklmnopqMATCH");
        // The cut removes one byte and the ellipsis adds three, so "MATCH"
        // moves to 19..24 in the displayed string.
        assert_eq!(shifted, vec![Some(19..24), Some(19..24), None]);
        assert_eq!(&shown[shifted[0].as_ref().unwrap().clone()], "MATCH");
    }

    #[test]
    fn truncating_a_highlighted_line_keeps_a_partial_capture_intersection() {
        let line = "abcdefghijklmnopqMATCH";
        // "ab" hangs over the cut; its intersection with the visible tail is
        // just "b", the way upstream paints the intersection that survives.
        let groups = vec![Some(0..2), Some(17..22)];
        let (shown, shifted) = truncate_head_with_ranges(line, 17, CELL_HEAD_OFFSET, &groups);
        assert_eq!(shown, "…bcdefghijklmnopqMATCH");
        assert_eq!(shifted, vec![Some(3..4), Some(19..24)]);
        assert_eq!(&shown[shifted[0].as_ref().unwrap().clone()], "b");
    }

    #[test]
    fn head_truncation_counts_grapheme_clusters_not_scalars() {
        // Twenty family emoji, then the match. Each emoji is one cluster but
        // five scalars, so a scalar-based cut would land mid-emoji.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let line = format!("{}MATCH", family.repeat(20));
        let inline_location = line.len() - "MATCH".len();
        let truncated = truncate_head(&line, inline_location, CELL_HEAD_OFFSET);
        // 16 clusters kept, so 4 of the 20 emoji are dropped.
        assert_eq!(truncated, format!("…{}MATCH", family.repeat(16)));
    }

    #[test]
    fn head_truncation_gives_up_when_the_line_has_fewer_clusters_than_the_offset() {
        // Upstream leaves the string whole rather than replacing all of it.
        let line = "abMATCH";
        assert_eq!(truncate_head(line, 2, CELL_HEAD_OFFSET), line);
    }
}
