//! Abbreviated matching: the search behind a filter field.
//!
//! CotEditor's `String+Match.swift` answers one question — *do these characters
//! appear in this string, in this order?* — and both of its users ask it for
//! different reasons. The outline's filter uses it to decide which headings to
//! keep; the Quick Actions bar uses it to rank menu commands against what the
//! user has typed.
//!
//! The answer is a *subsequence*, not a substring: `gt` finds "Guten Tag", and
//! `lz tnk` finds "lazy tanuki". Each character is searched for on its own,
//! case-insensitively, starting where the previous one was found, so the
//! matches come back in order and never overlap.
//!
//! # Two shapes of the same search
//!
//! [`abbreviated_matched_ranges`] is the search on its own, and is what a
//! filter that only needs "does it match" asks for.
//! [`abbreviated_match`] wraps it with the two things a *ranked* list needs:
//!
//! - `score`, the number of characters between the first and last match — a
//!   tighter match is a better one, so it sorts first; and
//! - `remaining`, the part of the query that was not consumed, which is what
//!   lets a caller walk a *path* of strings: match the first segment, then
//!   hand the rest of the query to the next one.
//!
//! That second one is why the ranges are returned **one per matched
//! character** rather than merged into runs: `remaining` is the query after
//! its first `ranges.len()` characters, so merging two adjacent matches into
//! one range would silently swallow a character of the query. A caller that
//! wants runs (the outline does, to draw one highlight) merges them itself.

use std::ops::Range;

/// What an abbreviated match found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbbreviatedMatch {
    /// One range per matched character, in the order they were found.
    pub ranges: Vec<Range<usize>>,
    /// The part of the query after the characters that matched.
    pub remaining: String,
    /// How far apart the first and last match are, in characters.
    ///
    /// Upstream calls this the score and sorts by it ascending, so the
    /// candidates whose matches sit closest together come first.
    pub score: usize,
}

/// The ranges of the characters of `filter` found in `text`, in order.
///
/// One range per matched character, in byte offsets into `text`. Returns
/// `None` when either side is empty — there is nothing to match, and an empty
/// filter must not be reported as a match — or when a character cannot be
/// found after the previous one.
///
/// `incomplete` decides what happens on that last case: `true` keeps the
/// ranges found so far and stops, `false` gives up and returns `None`.
/// Upstream's `abbreviatedMatchedRanges(with:incomplete:)` has the same switch,
/// and the ranked search is the caller that wants the partial answer.
///
/// # Examples
///
/// ```
/// use subtext_core::matcher::abbreviated_matched_ranges;
///
/// // `q` and `a` are found in order, and each is reported where it is.
/// let ranges = abbreviated_matched_ranges("Quick Actions", "qa", false).unwrap();
/// assert_eq!(ranges, [0..1, 6..7]);
///
/// // The search only moves forward, so the `t` here is the one in "Guten":
/// // it is the first one after the `G` that matched, not the first in the
/// // string.
/// assert_eq!(
///     abbreviated_matched_ranges("Guten Tag", "gt", false).unwrap(),
///     [0..1, 2..3]
/// );
///
/// // `q` is nowhere in "Guten Tag", so a complete search fails and a partial
/// // one stops early.
/// assert_eq!(abbreviated_matched_ranges("Guten Tag", "gq", false), None);
/// assert_eq!(
///     abbreviated_matched_ranges("Guten Tag", "gq", true).unwrap().len(),
///     1
/// );
/// ```
pub fn abbreviated_matched_ranges(
    text: &str,
    filter: &str,
    incomplete: bool,
) -> Option<Vec<Range<usize>>> {
    if filter.is_empty() || text.is_empty() {
        return None;
    }

    let mut ranges: Vec<Range<usize>> = Vec::new();

    for needle in filter.chars() {
        let from = ranges.last().map_or(0, |last| last.end);
        let found = text[from..]
            .char_indices()
            .find(|(_, candidate)| eq_ignore_case(*candidate, needle))
            .map(|(offset, candidate)| (from + offset)..(from + offset + candidate.len_utf8()));

        match found {
            Some(range) => ranges.push(range),
            None if incomplete => break,
            None => return None,
        }
    }

    if ranges.is_empty() {
        return None;
    }
    Some(ranges)
}

/// The same search, plus the two things a ranked list needs.
///
/// Returns `None` under exactly the conditions
/// [`abbreviated_matched_ranges`] does, with `incomplete` set: a character
/// that is not found ends the search rather than failing it, and the caller
/// decides what an unfinished match means by looking at `remaining`.
pub fn abbreviated_match(text: &str, filter: &str) -> Option<AbbreviatedMatch> {
    let ranges = abbreviated_matched_ranges(text, filter, true)?;

    // Upstream: `searchString.suffix(searchString.count - ranges.count)`.
    // The ranges are one per matched character, so the count is the number of
    // query characters consumed.
    let remaining: String = filter.chars().skip(ranges.len()).collect();

    // Upstream measures this with `String.distance`, which counts *characters*,
    // not bytes: a score has to mean the same thing whatever the text is
    // written in. `char` counts scalar values where Swift counts grapheme
    // clusters, so a base letter and its combining mark count as two here and
    // as one there — a difference no menu title or outline heading has.
    let score = text[ranges[0].start..ranges[ranges.len() - 1].end]
        .chars()
        .count();

    Some(AbbreviatedMatch {
        ranges,
        remaining,
        score,
    })
}

/// Compare two characters the way a case-insensitive search does.
///
/// `to_lowercase` rather than `eq_ignore_ascii_case`: labels are arbitrary
/// text, and `Å` has to match `å`.
pub fn eq_ignore_case(candidate: char, needle: char) -> bool {
    candidate == needle || candidate.to_lowercase().eq(needle.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `StringMatchTests.abbreviatedMatch`, case for case.
    #[test]
    fn an_abbreviated_match_scores_the_span_it_covers() {
        let string = "The fox jumps over the lazy dogcow.";

        assert_eq!(abbreviated_match(string, "quick"), None);

        let dogcow = abbreviated_match(string, "dogcow").unwrap();
        assert_eq!(dogcow.score, 6);
        assert_eq!(dogcow.ranges.len(), 6);
        assert!(dogcow.remaining.is_empty());

        let ow = abbreviated_match(string, "ow").unwrap();
        assert_eq!(ow.score, 29);
        assert_eq!(ow.ranges.len(), 2);
        assert!(ow.remaining.is_empty());

        let lazy_tanuki = abbreviated_match(string, "lazy tanuki").unwrap();
        assert_eq!(lazy_tanuki.score, 5);
        assert_eq!(lazy_tanuki.ranges.len(), 5);
        assert_eq!(lazy_tanuki.remaining, "tanuki");
    }

    /// The same test's last four assertions, which are about the switch.
    #[test]
    fn an_incomplete_search_stops_at_the_character_it_cannot_find() {
        let string = "The fox jumps over the lazy dogcow.";

        assert_eq!(abbreviated_matched_ranges(string, "lazy tanuki", false), None);
        assert_eq!(
            abbreviated_matched_ranges(string, "lazy tanuki", true).unwrap().len(),
            5
        );

        // Every character is present here, so both forms find all six.
        assert_eq!(abbreviated_matched_ranges(string, "lazy w", false).unwrap().len(), 6);
        assert_eq!(abbreviated_matched_ranges(string, "lazy w", true).unwrap().len(), 6);
    }

    /// `StringMatchTests.abbreviatedMatchedRanges`, case for case.
    #[test]
    fn a_filter_of_nothing_matches_nothing() {
        assert_eq!(abbreviated_matched_ranges("AbC", "", false), None);
        assert_eq!(abbreviated_matched_ranges("", "a", false), None);
        assert_eq!(abbreviated_matched_ranges("AbC", "ac", false).unwrap().len(), 2);
        assert_eq!(abbreviated_matched_ranges("AbC", "ac", true).unwrap().len(), 2);
        assert_eq!(abbreviated_matched_ranges("AbC", "ad", false), None);
        assert_eq!(abbreviated_matched_ranges("AbC", "ad", true).unwrap().len(), 1);
    }

    /// The search is case-insensitive in both directions.
    #[test]
    fn case_is_ignored_on_both_sides() {
        assert!(abbreviated_match("Guten Tag", "gt").is_some());
        assert!(abbreviated_match("Guten Tag", "GT").is_some());
        assert!(abbreviated_match("guten tag", "GT").is_some());
        // Not ASCII folding: an accented letter matches its own accent only.
        assert!(abbreviated_match("Ångström", "ång").is_some());
        assert_eq!(abbreviated_match("Angstrom", "ång"), None);
    }

    /// Each character is matched after the one before it, never before it.
    #[test]
    fn the_search_moves_forward_only() {
        // `aa` cannot be found in `ab`, because there is no second `a` after
        // the first — the search does not go back.
        assert_eq!(abbreviated_matched_ranges("ab", "aa", false), None);
        assert_eq!(
            abbreviated_matched_ranges("aba", "aa", false).unwrap(),
            [0..1, 2..3]
        );
    }

    /// The ranges are one per matched character, and adjacent ones stay apart.
    ///
    /// This is what `remaining` depends on: the whole of "tanuki" matches
    /// here, so six query characters are consumed and only the `x` is left.
    /// Had the six adjacent ranges been merged into one, `remaining` would
    /// have been `anukix` — the rest of a query that had already been used up.
    #[test]
    fn adjacent_matches_are_not_merged() {
        let found = abbreviated_match("tanuki", "tanukix").unwrap();
        assert_eq!(found.ranges, [0..1, 1..2, 2..3, 3..4, 4..5, 5..6]);
        assert_eq!(found.remaining, "x");
    }

    /// `score` counts characters, not bytes.
    #[test]
    fn the_score_is_a_character_count() {
        // Six characters, twelve bytes.
        let found = abbreviated_match("日本語ですよ", "日よ").unwrap();
        assert_eq!(found.score, 6);
        assert_eq!(found.ranges, [0..3, 15..18]);
    }

    /// A query the string cannot finish still reports what it did find.
    #[test]
    fn an_unfinished_match_keeps_its_remaining_query() {
        let found = abbreviated_match("Save", "save as").unwrap();
        assert_eq!(found.ranges.len(), 4);
        assert_eq!(found.remaining, " as");
        assert_eq!(found.score, 4);
    }
}
