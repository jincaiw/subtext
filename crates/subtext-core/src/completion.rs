//! Word completion — CotEditor's `EditorTextView.completions(forPartialWordRange:)`.
//!
//! Three sources feed the candidate list, and the Mode pane's three checkboxes
//! pick which of them are consulted. Two of them are reachable from Rust; the
//! third is AppKit's system word list, which SubText has no access to and the
//! pane disables with a reason.
//!
//! # The partial word
//!
//! Upstream asks AppKit for `rangeForUserCompletion` and then *widens* it
//! backwards while the characters are ones a completion could continue. Both
//! halves of that are measured rather than assumed — see
//! [`partial_word_range`] — and the widening can also *narrow* AppKit's answer,
//! because the two use different character sets.

use std::ops::Range;

use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

/// Which sources the candidate list is drawn from.
///
/// CotEditor's `CompletionWordTypes`, whose three flags are the three
/// checkboxes in the Mode pane.
///
/// `standard` is AppKit's system word list, reached through
/// `NSTextView.completions`. SubText has no such source: the list lives in the
/// system text engine, and the pane disables that switch rather than pretending
/// it does something.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompletionSources {
    pub standard: bool,
    pub document: bool,
    pub syntax: bool,
}

impl CompletionSources {
    /// Whether any source at all is switched on.
    ///
    /// Upstream asks this before doing any work: with every flag off the
    /// command does nothing rather than showing an empty list.
    pub fn any(self) -> bool {
        self.standard || self.document || self.syntax
    }
}

/// The range of the word being completed, ending at `caret`.
///
/// Two ranges, in upstream's order:
///
/// 1. **AppKit's `rangeForUserCompletion`**, which is the run of letters,
///    digits, `_` and `.` ending at the caret. Probed against a real
///    `NSTextView`: `a.b` gives the whole string, `@inter` gives `inter`, and
///    a combining mark stays with the letter it attaches to.
/// 2. **CotEditor's widening**, which moves the start back while the characters
///    are ones a completion could continue — alphanumerics, `_`, and the first
///    character of any word the syntax defines. That last part is what lets a
///    language whose completion list holds `@interface` complete from `@`.
///
/// The widening set does **not** hold `.`, so it can also narrow AppKit's
/// answer: `a.b` completes `b`, not `a.b`. The start is AppKit's unless the
/// widening found a character it cannot continue from, in which case it is the
/// position after that character.
pub fn partial_word_range(text: &str, caret: usize, syntax_first_letters: &[char]) -> Range<usize> {
    let caret = floor_char_boundary(text, caret);
    let appkit = run_start(text, caret, &|c| {
        c.is_alphanumeric() || c == '_' || c == '.'
    });
    let widened = run_start(text, caret, &|c| {
        c.is_alphanumeric() || c == '_' || syntax_first_letters.contains(&c)
    });
    // `widened == 0` means every character before the caret could be continued,
    // which is upstream's "no invalid character found" branch: AppKit's start
    // stands.
    let start = if widened == 0 { appkit } else { widened };
    start..caret
}

/// Where the run of `keep` characters ending at `end` begins.
///
/// Walked by grapheme cluster rather than by scalar, because a combining mark
/// belongs to the letter it attaches to: `a\u{300}` is one cluster whose first
/// scalar decides, which is what AppKit's answer does too.
fn run_start(text: &str, end: usize, keep: &dyn Fn(char) -> bool) -> usize {
    let mut start = end;
    for (offset, grapheme) in text[..end].grapheme_indices(true).rev() {
        match grapheme.chars().next() {
            Some(first) if keep(first) => start = offset,
            _ => break,
        }
    }
    start
}

/// The candidates for the word at `partial`.
///
/// Upstream's `completions(forPartialWordRange:indexOfSelectedItem:)`, minus
/// the `standard` source. The order is upstream's: every word in the document
/// that begins with `partial`, then every syntax word that does — so the
/// document's own spelling wins when the same word appears in both.
///
/// An empty list means "offer nothing", which is also what upstream returns
/// when the only candidate is the word already there.
pub fn candidates(
    text: &str,
    partial: &Range<usize>,
    sources: CompletionSources,
    syntax_words: &[String],
) -> Vec<String> {
    if partial.is_empty() || partial.end > text.len() || !sources.any() {
        return Vec::new();
    }
    let word = &text[partial.clone()];

    let mut found: Vec<String> = Vec::new();

    // Words already in the document. Upstream skips this for a one-character
    // partial that is not a letter, so that completing `_` does not offer every
    // identifier in the file.
    let graphemes = word.graphemes(true).count();
    let letter = word.chars().next().is_some_and(char::is_alphabetic);
    if sources.document && (graphemes > 1 || letter) {
        // `\b` + the partial + `\w+` + `\b`: the whole word the partial begins,
        // not the partial itself. Both `\b` and `\w` follow UTS #18 in ICU and
        // in Rust's regex, so the two engines agree on what a word is.
        let pattern = format!(r"\b{}\w+\b", regex::escape(word));
        if let Ok(pattern) = Regex::new(&pattern) {
            found.extend(
                pattern
                    .find_iter(text)
                    .map(|found| found.as_str().to_string()),
            );
        }
    }

    // Words the syntax defines, matched as a *case-insensitive prefix* — the
    // document's words above are matched case-sensitively, which is upstream's
    // asymmetry rather than an oversight here.
    if sources.syntax {
        let needle = word.to_lowercase();
        found.extend(
            syntax_words
                .iter()
                .filter(|candidate| candidate.to_lowercase().starts_with(&needle))
                .cloned(),
        );
    }

    dedupe(&mut found);

    // One candidate that is the word already there is not a completion.
    if found.len() == 1 && found[0].eq_ignore_ascii_case(word) {
        return Vec::new();
    }
    found
}

/// Drop repeats, keeping the first of each — upstream's `unique()`.
///
/// The comparison is exact, as upstream's is: two spellings that differ only in
/// case are two candidates.
fn dedupe(words: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    words.retain(|word| seen.insert(word.clone()));
}

/// Where the selection goes after `word` replaces `partial`.
///
/// Upstream's `insertCompletion`: a word with a parenthesised part — `foo()`,
/// `println!(…)`, `@interface …` — leaves the caret *inside* it, so the next
/// keystroke is the argument rather than a jump over the closing bracket. The
/// match is the first `(` to the last `)`, which is what its greedy `.*` does.
pub fn selection_after_insert(word: &str, partial: &Range<usize>) -> Range<usize> {
    let start = partial.start;
    let end = start + word.len();
    match (word.find('('), word.rfind(')')) {
        (Some(open), Some(close)) if open < close => start + open + 1..start + close,
        _ => start..end,
    }
}

/// Whether the automatic popup should appear — upstream's `performCompletion`.
///
/// The guards are all about *not* interrupting: the caret must be at the end of
/// a word that is long enough to be worth completing, the character before it
/// must not be whitespace, and the character after it must not be a letter —
/// completing in the middle of a word would replace its tail. `partial` is the
/// word being completed, as [`partial_word_range`] finds it, and its length is
/// the one upstream compares against `minimumAutomaticCompletionLength`.
///
/// Upstream also checks `hasMarkedText()`, which is an input method part-way
/// through composing. GPUI's editor does not expose that state, so SubText
/// cannot ask; see the gap table.
pub fn should_complete_automatically(text: &str, partial: &Range<usize>, minimum: usize) -> bool {
    let caret = floor_char_boundary(text, partial.end);
    let before = text[..caret].chars().next_back();
    let after = text[caret..].chars().next();
    if before.is_some_and(char::is_whitespace) {
        return false;
    }
    if after.is_some_and(char::is_alphanumeric) {
        return false;
    }
    // The *word* is what upstream measures — `rangeForUserCompletion.length`,
    // not how much text precedes the caret. Counting the whole prefix would
    // suggest a completion for `bc` in `a bc`, which upstream never does.
    //
    // The length is counted in grapheme clusters, as upstream's `length` is
    // not: `NSRange.length` is UTF-16 units, so `é` counts two there and one
    // here.
    let start = floor_char_boundary(text, partial.start.min(caret));
    text[start..caret].graphemes(true).count() >= minimum
}

/// Round `index` down to a character boundary.
fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|word| (*word).to_string()).collect()
    }

    #[test]
    fn the_partial_word_is_the_run_of_word_characters_at_the_caret() {
        let range = |text: &str, caret: usize| partial_word_range(text, caret, &[]);
        // AppKit's own answers, measured against a real `NSTextView`.
        assert_eq!(range("foo bar", 7), 4..7);
        assert_eq!(range("foo bar", 3), 0..3);
        assert_eq!(range("_ab", 3), 0..3);
        assert_eq!(range("ab_cd", 5), 0..5);
        assert_eq!(range("héllo", 5), 0..5);
        assert_eq!(range("foo", 0), 0..0);
        assert_eq!(range("", 0), 0..0);
        // AppKit counts the period as a word character — but CotEditor's
        // widening does not, so the override *shortens* this to `b`.
        assert_eq!(range("a.b", 3), 2..3);
        // `@` is a word character to neither, so AppKit's own answer stands.
        assert_eq!(range("@inter", 6), 1..6);
        // A combining mark stays with the letter it attaches to, and the
        // separators that are not ASCII are still separators. The caret is the
        // end of the string in each case — these are byte offsets, and the
        // characters here are not one byte each.
        for (text, expected) in [
            ("a\u{300}b", 0..4),
            ("a\u{2028}b", 4..5),
            ("a\u{A0}b", 3..4),
        ] {
            assert_eq!(range(text, text.len()), expected, "{text:?}");
        }
    }

    #[test]
    fn the_syntax_letters_decide_where_the_widening_stops() {
        // The widening only ever moves the start *backwards to the last
        // character it cannot continue from*, so its set decides where that
        // stop is. Measured, and it is not what the name suggests: it can
        // shorten AppKit's answer but never lengthen it.
        //
        // `a.@b` with `@` in the set: the last character the widening cannot
        // continue from is the period, so the partial is `@b`.
        assert_eq!(partial_word_range("a.@b", 4, &['@']), 2..4);
        // Without it, the stop is the `@` itself, so the partial is `b`.
        assert_eq!(partial_word_range("a.@b", 4, &[]), 3..4);
        // A syntax word starting with `.` moves the stop all the way out.
        assert_eq!(partial_word_range("a.b", 3, &['.']), 0..3);
        // With no character to stop at, AppKit's own range stands — which is
        // why `@inter` completes `inter` even when the syntax defines
        // `@interface`.
        assert_eq!(partial_word_range("@inter", 6, &['@']), 1..6);
    }

    #[test]
    fn the_document_supplies_the_words_that_begin_with_the_partial() {
        let text = "let foo = 1\nfooBar()\nlet food = 2\n";
        let sources = CompletionSources {
            document: true,
            ..Default::default()
        };
        let found = candidates(text, &(4..6), sources, &[]);
        assert_eq!(found, words(&["foo", "fooBar", "food"]));

        // The match is case sensitive, as upstream's pattern is.
        assert!(candidates(text, &(0..3), sources, &[]).is_empty());
    }

    #[test]
    fn a_one_character_symbol_does_not_offer_the_whole_document() {
        let text = "_alpha _beta";
        let sources = CompletionSources {
            document: true,
            ..Default::default()
        };
        // `_` on its own: the guard skips the document search.
        assert!(candidates(text, &(0..1), sources, &[]).is_empty());
        // A one-character *letter* is searched.
        assert_eq!(
            candidates("a b alpha", &(0..1), sources, &[]),
            words(&["alpha"])
        );
    }

    #[test]
    fn the_syntax_supplies_case_insensitive_prefixes() {
        let syntax = words(&["@interface", "int", "Integer"]);
        let sources = CompletionSources {
            syntax: true,
            ..Default::default()
        };
        assert_eq!(
            candidates("in", &(0..2), sources, &syntax),
            words(&["int", "Integer"])
        );
        // Case-insensitive, unlike the document search.
        assert_eq!(
            candidates("IN", &(0..2), sources, &syntax),
            words(&["int", "Integer"])
        );
        // An anchored prefix, not a substring.
        assert!(candidates("ter", &(0..3), sources, &syntax).is_empty());
    }

    #[test]
    fn the_document_comes_before_the_syntax_and_repeats_are_dropped() {
        let syntax = words(&["int", "Integer"]);
        let sources = CompletionSources {
            document: true,
            syntax: true,
            ..Default::default()
        };
        let found = candidates("int x\ninteger y", &(0..3), sources, &syntax);
        // The document's search needs at least one more word character after
        // the partial, so it offers `integer` and never the word itself.
        assert_eq!(found, words(&["integer", "int", "Integer"]));
    }

    #[test]
    fn one_candidate_that_is_the_word_itself_is_not_a_completion() {
        // The document search never returns the word itself — its pattern needs
        // one more character — so this guard is about the syntax list.
        let syntax = words(&["int"]);
        let sources = CompletionSources {
            syntax: true,
            ..Default::default()
        };
        assert!(candidates("int", &(0..3), sources, &syntax).is_empty());
        // A second candidate makes the list worth showing.
        let syntax = words(&["int", "interface"]);
        assert_eq!(
            candidates("int", &(0..3), sources, &syntax),
            words(&["int", "interface"])
        );
    }

    #[test]
    fn no_sources_means_no_candidates() {
        assert!(
            candidates(
                "foo",
                &(0..3),
                CompletionSources::default(),
                &words(&["foobar"])
            )
            .is_empty()
        );
        // The standard source is AppKit's, which SubText cannot reach: with
        // only that switch on there is nothing to offer.
        let standard = CompletionSources {
            standard: true,
            ..Default::default()
        };
        assert!(candidates("foo", &(0..3), standard, &words(&["foobar"])).is_empty());
    }

    #[test]
    fn a_word_with_brackets_leaves_the_caret_inside_them() {
        // `foo()` is completed so that the argument can be typed straight away.
        assert_eq!(selection_after_insert("foo()", &(0..3)), 4..4);
        assert_eq!(selection_after_insert("println!(x)", &(0..2)), 9..10);
        // A word with no bracket pair selects the word itself.
        assert_eq!(selection_after_insert("foobar", &(0..3)), 0..6);
        // An unbalanced bracket is not a pair.
        assert_eq!(selection_after_insert("foo(", &(0..3)), 0..4);
        assert_eq!(selection_after_insert("foo)", &(0..3)), 0..4);
    }

    #[test]
    fn the_automatic_popup_waits_for_a_word_worth_completing() {
        // Three characters, a letter before and a space after.
        assert!(should_complete_automatically("foo ", &(0..3), 3));
        assert!(should_complete_automatically("let foo", &(4..7), 3));
        // Too short.
        assert!(!should_complete_automatically("fo ", &(0..2), 3));
        // Nothing typed yet.
        assert!(!should_complete_automatically("", &(0..0), 3));
        // The caret follows whitespace, so there is no word to continue.
        assert!(!should_complete_automatically("foo bar", &(4..4), 3));
        // …or sits in the middle of a word, where completing would eat its tail.
        assert!(!should_complete_automatically("foobar", &(0..3), 3));
        // The word is what counts, not the text before it: `bc` is two
        // characters even though four precede the caret.
        assert!(!should_complete_automatically("a bc", &(2..4), 3));
        assert!(should_complete_automatically("a bcd", &(2..5), 3));
        // A combining mark is one character, as upstream's UTF-16 length makes
        // it two — the one place the two counts disagree.
        assert!(!should_complete_automatically("e\u{300}e", &(0..3), 3));
    }
}
