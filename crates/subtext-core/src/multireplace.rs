//! CotEditor's Multiple Replace: an ordered list of find/replace rules, applied
//! one after another, plus the switches that steer them.
//!
//! Ported from `Packages/EditorCore/Sources/TextFind/MultipleReplace.swift` and
//! `MultipleReplace+TSV.swift`. This is the model behind the *Multiple Replace*
//! sheet; the find bar's single query lives in [`crate::finder`].
//!
//! # Rules see each other
//!
//! [`MultipleReplace::replace`] runs the enabled rules in order and each one
//! searches the *previous rule's output*. [`MultipleReplace::find`] does not:
//! Highlight All searches the original text with every rule and concatenates
//! the hits, so a rule can never see an earlier rule's work. That asymmetry is
//! upstream's, and it is why both methods exist.
//!
//! # Invalid rules are skipped
//!
//! A rule whose find string is empty, or whose pattern does not compile, drops
//! out of the batch rather than failing it — the sheet says as much under the
//! table: *"Invalid rules will be skipped."* [`MultipleReplace::validate`] is
//! the separate question the sheet asks before reporting a result, and it fails
//! on any rule at all, enabled or not.
//!
//! # What SubText cannot reproduce
//!
//! Four of the sheet's eight advanced switches ask Foundation for behaviour
//! Rust's `regex` and `str` do not offer: byte-exact comparison is what
//! `str::contains` already does, diacritic- and width-insensitive matching need
//! ICU collation, and word boundaries are Unicode-aware in `regex` already.
//! [`Settings`] keeps all eight so the sheet can show them, and the four that
//! cannot move are rendered disabled with the reason rather than pretending.

use std::ops::Range;

use regex::{Regex, RegexBuilder};

use crate::line_ending::{self, LineEnding};

/// The switches the *Advanced Find Options* popover holds.
///
/// Defaults follow upstream's `MultipleReplace.Settings.init()` except where
/// SubText's engine is already committed to one answer; those are noted on the
/// field, because a switch that reads `true` and cannot be turned off is a
/// statement about the engine, not a preference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Match only whole words. Honoured.
    pub matches_full_word: bool,
    /// Compare characters strictly.
    ///
    /// Always on: Rust's `str` comparison is byte-exact, which is what
    /// Foundation's `.literal` asks for, so upstream's default of "off" would
    /// describe a canonical-equivalence search SubText does not perform.
    pub is_literal_search: bool,
    /// Ignore diacritical marks (`ö` = `o`). Not honoured; needs ICU.
    pub ignores_diacritic_marks: bool,
    /// Ignore width differences (`ａ` = `a`). Not honoured; needs ICU.
    pub ignores_width: bool,
    /// Let `.` match line separators. Honoured.
    pub is_single_line: bool,
    /// Let `^` and `$` match the start and end of lines. Honoured.
    pub is_multiline: bool,
    /// Use Unicode TR#29 word boundaries.
    ///
    /// Always on: `regex`'s `\b` is Unicode-aware and has no ASCII-only mode to
    /// switch into, so this reports the engine rather than a preference.
    pub uses_unicode_boundaries: bool,
    /// Decode `\n`, `\t` and friends in a regular expression rule's
    /// replacement. Honoured; see [`unescaped_template`].
    pub unescapes_replacement_string: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            matches_full_word: false,
            is_literal_search: true,
            ignores_diacritic_marks: false,
            ignores_width: false,
            is_single_line: false,
            // Upstream's `regexOptions` default is `[.anchorsMatchLines]`.
            is_multiline: true,
            uses_unicode_boundaries: true,
            unescapes_replacement_string: true,
        }
    }
}

/// Why a rule cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternError {
    /// The find string is empty, so the rule would match everywhere.
    EmptyFindString,
    /// The pattern does not compile.
    InvalidRegularExpression(String),
}

impl PatternError {
    /// The one-line explanation the sheet shows in a warning's tooltip.
    pub fn message(&self) -> String {
        match self {
            PatternError::EmptyFindString => "The find string is empty.".to_string(),
            PatternError::InvalidRegularExpression(reason) => {
                format!("[Invalid regular expression] {reason}")
            }
        }
    }
}

/// One row of the Multiple Replace sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    pub find_string: String,
    pub replacement_string: String,
    pub uses_regular_expression: bool,
    pub ignores_case: bool,
    /// The sheet's last column. `None` rather than `""`, because upstream
    /// treats "no description" and "an empty description" as different states
    /// when it decides whether a rule says anything at all.
    pub description: Option<String>,
    pub is_enabled: bool,
}

impl Default for Replacement {
    /// A blank rule that is switched on, which is what the sheet's *Add*
    /// button inserts.
    fn default() -> Self {
        Self {
            find_string: String::new(),
            replacement_string: String::new(),
            uses_regular_expression: false,
            ignores_case: false,
            description: None,
            is_enabled: true,
        }
    }
}

impl Replacement {
    /// Whether the rule carries no information at all.
    ///
    /// A rule that only replaces something with nothing still counts as
    /// content — deleting every match is a thing to ask for.
    pub fn is_empty(&self) -> bool {
        self.find_string.is_empty()
            && self.replacement_string.is_empty()
            && self.description.is_none()
    }

    /// Whether the rule can be compiled under `settings`.
    pub fn validate(&self, settings: &Settings) -> Result<(), PatternError> {
        self.compile(settings).map(|_| ())
    }

    /// Compile the find string.
    ///
    /// `None` means "skip this rule", which is how both of the invalid cases
    /// are reported: an empty find string and a pattern that does not compile.
    fn compile(&self, settings: &Settings) -> Result<Regex, PatternError> {
        if self.find_string.is_empty() {
            return Err(PatternError::EmptyFindString);
        }
        // A textual rule is the escaped find string, optionally wrapped in word
        // boundaries; a regular expression rule is the pattern as written. Both
        // end up as a `regex`, which keeps one enumeration path for the two and
        // is the same thing `finder::Searcher` does for the find bar.
        let pattern = if self.uses_regular_expression {
            self.find_string.clone()
        } else {
            let literal = regex::escape(&self.find_string);
            if settings.matches_full_word {
                format!(r"\b{literal}\b")
            } else {
                literal
            }
        };
        RegexBuilder::new(&pattern)
            .case_insensitive(self.ignores_case)
            // Only meaningful for a pattern with anchors or a dot in it, so
            // setting them for a textual rule changes nothing.
            .multi_line(settings.is_multiline)
            .dot_matches_new_line(settings.is_single_line)
            .build()
            .map_err(|error| PatternError::InvalidRegularExpression(error.to_string()))
    }

    /// The replacement text a match is expanded against.
    ///
    /// Only a regular expression rule unescapes: a textual rule's replacement is
    /// inserted verbatim, which is upstream's `replacementString(from:)`.
    fn template(&self, settings: &Settings) -> String {
        if self.uses_regular_expression && settings.unescapes_replacement_string {
            unescaped_template(&self.replacement_string)
        } else {
            self.replacement_string.clone()
        }
    }
}

/// An ordered list of rules and the switches they share.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MultipleReplace {
    pub replacements: Vec<Replacement>,
    pub settings: Settings,
}

/// What a batch of rules did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub string: String,
    /// Where the selection should sit afterwards.
    ///
    /// `None` means "leave it alone", which is what upstream reports when the
    /// batch ran over the whole document. Upstream calls this `selectedRanges`
    /// and can hold several; SubText's editor has one selection, so it holds
    /// one range.
    pub selected_range: Option<Range<usize>>,
    /// How many matches were replaced, across every rule.
    pub count: usize,
}

impl MultipleReplace {
    /// Whether no rule says anything.
    pub fn is_empty(&self) -> bool {
        !self.replacements.iter().any(|rule| !rule.is_empty())
    }

    /// Whether every rule compiles — including the disabled ones, which is what
    /// upstream asks before it reports a result.
    pub fn validate(&self) -> bool {
        self.replacements
            .iter()
            .all(|rule| rule.validate(&self.settings).is_ok())
    }

    /// Every match every enabled rule finds in `text`.
    ///
    /// Returned in rule order, not document order, because that is what
    /// upstream returns: `"abc"` then `"def"` over `"abc def abc"` gives
    /// `0..3`, `8..11`, `4..7`. A caller that paints them sorts them first.
    ///
    /// Every rule searches the *original* text, so unlike [`Self::replace`] no
    /// rule here sees an earlier rule's work.
    pub fn find(&self, text: &str, scope: Range<usize>, in_selection: bool) -> Vec<Range<usize>> {
        let scope = if in_selection { clamp(&scope, text) } else { 0..text.len() };
        if scope.is_empty() {
            return Vec::new();
        }
        let mut found = Vec::new();
        for rule in self.replacements.iter().filter(|rule| rule.is_enabled) {
            let Ok(regex) = rule.compile(&self.settings) else {
                continue;
            };
            found.extend(matches_in(&regex, text, scope.clone()));
        }
        found
    }

    /// Apply the enabled rules, in order, each to the previous rule's output.
    ///
    /// `scope` is the selection the batch is confined to; it is only read when
    /// `in_selection` is set, matching upstream's `findRanges`/`inSelection`
    /// pair. `line_ending` rewrites the line endings of *inserted* text so a
    /// replacement pasted in from elsewhere adopts the document's convention;
    /// `None` leaves them as written.
    pub fn replace(
        &self,
        text: &str,
        scope: Range<usize>,
        in_selection: bool,
        line_ending: Option<LineEnding>,
    ) -> Outcome {
        let mut string = text.to_string();
        let mut selection = in_selection.then(|| clamp(&scope, text));
        let mut count = 0;

        for rule in self.replacements.iter().filter(|rule| rule.is_enabled) {
            let Ok(regex) = rule.compile(&self.settings) else {
                continue;
            };
            let scope = selection.clone().unwrap_or(0..string.len());
            let scope = clamp(&scope, &string);
            if scope.is_empty() {
                continue;
            }

            let template = rule.template(&self.settings);
            let (rebuilt, produced) =
                replace_in(&regex, &string, &scope, &template, line_ending, &mut count);
            string = rebuilt;
            if selection.is_some() {
                // The scope keeps its start; only its length moves, because
                // every rule stays inside it.
                selection = Some(scope.start..scope.start + produced);
            }
        }

        Outcome { string, selected_range: selection, count }
    }
}

// ---- the replacement itself ----------------------------------------------

/// Rewrite one scope, returning the whole string and how many bytes the scope
/// now occupies.
fn replace_in(
    regex: &Regex,
    text: &str,
    scope: &Range<usize>,
    template: &str,
    line_ending: Option<LineEnding>,
    count: &mut usize,
) -> (String, usize) {
    let matches = regex.captures_iter(&text[scope.clone()]).collect::<Vec<_>>();
    if matches.is_empty() {
        return (text.to_string(), scope.len());
    }

    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..scope.start]);

    let mut cursor = scope.start;
    // `consumed` counts the scope's original bytes walked so far and `produced`
    // the bytes that replaced them, so the new scope is `start..start + produced`
    // once the tail has been copied too.
    let mut produced = 0;
    for captures in &matches {
        let whole = captures.get(0).expect("group 0 is the whole match");
        let (start, end) = (scope.start + whole.start(), scope.start + whole.end());

        let gap = start - cursor;
        out.push_str(&text[cursor..start]);
        produced += gap;

        let mut expanded = String::new();
        captures.expand(template, &mut expanded);
        let expanded = normalize_line_endings(&expanded, line_ending);
        produced += expanded.len();
        out.push_str(&expanded);

        cursor = end;
        *count += 1;
    }

    let tail = scope.end - cursor;
    out.push_str(&text[cursor..scope.end]);
    produced += tail;
    out.push_str(&text[scope.end..]);

    (out, produced)
}

/// The byte ranges `regex` matches inside `scope`.
fn matches_in(regex: &Regex, text: &str, scope: Range<usize>) -> Vec<Range<usize>> {
    regex
        .find_iter(&text[scope.clone()])
        .map(|found| scope.start + found.start()..scope.start + found.end())
        .collect()
}

/// Rewrite the line endings of inserted text.
///
/// Upstream's `normalizeLineEndings(in:)`: only text that contains a line ending
/// is touched, and only when the caller named one, so a replacement without a
/// newline in it — the overwhelming majority — is inserted as written.
fn normalize_line_endings(text: &str, ending: Option<LineEnding>) -> String {
    match ending {
        Some(ending) if text.chars().any(line_ending::is_line_ending_char) => {
            line_ending::convert(text, ending)
        }
        _ => text.to_string(),
    }
}

/// `text` with the backslash escapes upstream's `unescapedTemplate` decodes.
///
/// The set is the seven combinations Swift's string literals define, and
/// upstream's own comment points at that list as the reason. One entry differs
/// in representation: `\\` becomes a single backslash here, because `regex`'s
/// template engine — unlike ICU's — does not consume backslashes, so keeping
/// the pair would print two where upstream prints one.
pub fn unescaped_template(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match chars.next() {
            Some('0') => out.push('\0'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('\\') => out.push('\\'),
            // A backslash in front of anything else is not an escape upstream
            // knows either, so it stays as it was typed.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ---- tab-separated import -------------------------------------------------

impl MultipleReplace {
    /// Read rules from tab-separated values.
    ///
    /// This is the second of the two formats upstream's *Import* accepts, the
    /// first being its own `.cotreplacement` files, and it is the one a user can
    /// produce from a spreadsheet. One rule per line; the first two tab-separated
    /// fields are the find string and the replacement, and any further fields are
    /// ignored. Blank lines, lines with fewer than two fields and lines whose
    /// first field is empty are dropped rather than reported, which is what
    /// upstream does when `failsOnInvalidValue` is not set.
    ///
    /// The imported rules take the default [`Settings`]: the format has nowhere
    /// to put them.
    pub fn from_tsv(source: &str) -> Self {
        let replacements = source
            .split(['\n', '\r'])
            .filter(|line| !line.is_empty())
            .filter_map(parse_tsv_line)
            .collect();
        Self { replacements, settings: Settings::default() }
    }
}

/// One line of TSV, or `None` when the line does not describe a rule.
fn parse_tsv_line(line: &str) -> Option<Replacement> {
    let mut fields = line.split('\t');
    let find_string = fields.next()?;
    let replacement_string = fields.next()?;
    if find_string.is_empty() {
        return None;
    }
    Some(Replacement {
        find_string: find_string.to_string(),
        replacement_string: replacement_string.to_string(),
        ..Replacement::default()
    })
}

// ---- ranges ---------------------------------------------------------------

/// `range` clamped to `text`, and to character boundaries.
///
/// A selection reported in the middle of a multi-byte character would panic a
/// slice; snapping it to the nearest boundary is a better failure than a crash.
fn clamp(range: &Range<usize>, text: &str) -> Range<usize> {
    let start = snap_down(text, range.start);
    let end = snap_up(text, range.end).max(start);
    start..end
}

fn snap_down(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn snap_up(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset < text.len() && !text.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A textual rule with `ignores_case` set, which is how a SubText rule asks
    /// for the case-insensitive comparison upstream spells as a textual option.
    fn rule(find: &str, replacement: &str) -> Replacement {
        Replacement {
            find_string: find.to_string(),
            replacement_string: replacement.to_string(),
            ..Replacement::default()
        }
    }

    fn regex_rule(find: &str, replacement: &str) -> Replacement {
        Replacement {
            uses_regular_expression: true,
            ..rule(find, replacement)
        }
    }

    #[test]
    fn an_empty_definition_holds_nothing() {
        assert!(MultipleReplace::default().is_empty());
        assert!(MultipleReplace { replacements: vec![Replacement::default()], ..Default::default() }
            .is_empty());
        assert!(
            MultipleReplace {
                replacements: vec![Replacement::default(), Replacement::default()],
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !MultipleReplace { replacements: vec![rule("a", "")], ..Default::default() }.is_empty()
        );
    }

    #[test]
    fn an_empty_rule_holds_nothing_but_a_description_counts() {
        assert!(Replacement::default().is_empty());
        assert!(!rule("a", "").is_empty());
        assert!(
            !Replacement { description: Some("a".into()), ..Default::default() }.is_empty()
        );
    }

    #[test]
    fn equality_is_literal() {
        // Two scalars that Unicode canonicalises together are still two
        // different find strings, because a rule that said one and now says the
        // other has been edited.
        let left = MultipleReplace { replacements: vec![rule("\u{00B7}", "")], ..Default::default() };
        let right = MultipleReplace { replacements: vec![rule("\u{0387}", "")], ..Default::default() };
        assert_ne!(left, right);

        let left = MultipleReplace { replacements: vec![rule("x", "\u{00B7}")], ..Default::default() };
        let right = MultipleReplace { replacements: vec![rule("x", "\u{0387}")], ..Default::default() };
        assert_ne!(left, right);
    }

    #[test]
    fn a_rule_reports_why_it_cannot_be_used() {
        assert_eq!(
            Replacement::default().validate(&Settings::default()),
            Err(PatternError::EmptyFindString)
        );
        assert!(matches!(
            regex_rule("(", "x").validate(&Settings::default()),
            Err(PatternError::InvalidRegularExpression(_))
        ));
    }

    #[test]
    fn one_bad_rule_invalidates_the_set() {
        let valid = MultipleReplace { replacements: vec![rule("a", "")], ..Default::default() };
        assert!(valid.validate());

        let empty = MultipleReplace {
            replacements: vec![Replacement::default()],
            ..Default::default()
        };
        assert!(!empty.validate());

        let bad = MultipleReplace {
            replacements: vec![regex_rule("(", "x")],
            ..Default::default()
        };
        assert!(!bad.validate());
    }

    #[test]
    fn a_textual_rule_can_be_restricted_to_whole_words() {
        let settings = Settings { matches_full_word: true, ..Default::default() };
        let definition = MultipleReplace {
            replacements: vec![Replacement { ignores_case: true, ..rule("cat", "dog") }],
            settings,
        };
        let outcome = definition.replace("cats cat Cat", 0..0, false, None);
        assert_eq!(outcome.string, "cats dog dog");
        assert_eq!(outcome.count, 2);
    }

    #[test]
    fn a_textual_rule_replaces_canonically_equivalent_characters() {
        // The find string is one scalar and the replacement another; a search
        // that folded them together would find nothing to replace.
        let definition = MultipleReplace {
            replacements: vec![rule("\u{00B7}", "\u{0387}")],
            ..Default::default()
        };
        let outcome = definition.replace("\u{00B7}", 0..0, false, None);
        assert_eq!(outcome.string.chars().map(u32::from).collect::<Vec<_>>(), vec![0x0387]);
    }

    #[test]
    fn a_regex_rule_ignores_case_and_unescapes_its_replacement() {
        // Upstream's pattern is `(?!=a)b(c)(?=d)`, which needs lookaround. The
        // test is about case folding and `\t`, so it is spelled without one.
        let definition = MultipleReplace {
            replacements: vec![Replacement { ignores_case: true, ..regex_rule("b(c)", "$1\\t") }],
            ..Default::default()
        };
        let outcome = definition.replace("abcdefg ABCDEFG", 0..0, false, None);
        assert_eq!(outcome.string, "ac\tdefg AC\tDEFG");
        assert_eq!(outcome.count, 2);
    }

    #[test]
    fn rules_are_confined_to_the_selection_and_chain() {
        let definition = MultipleReplace {
            replacements: vec![rule("abc", "x"), rule("x", "y")],
            ..Default::default()
        };
        let text = "abc abc abc";
        let outcome = definition.replace(text, 4..11, true, None);
        assert_eq!(outcome.string, "abc y y");
        // The scope's start does not move and its length is unchanged, so the
        // selection stays where the user put it.
        assert_eq!(outcome.selected_range, Some(4..7));
    }

    #[test]
    fn a_selection_grows_when_the_replacement_does() {
        let definition = MultipleReplace {
            replacements: vec![rule("x", "yyyy")],
            ..Default::default()
        };
        let outcome = definition.replace("x x", 0..3, true, None);
        assert_eq!(outcome.string, "yyyy yyyy");
        assert_eq!(outcome.selected_range, Some(0..9));
    }

    #[test]
    fn a_batch_outside_a_selection_reports_no_selection() {
        let definition = MultipleReplace {
            replacements: vec![rule("a", "b")],
            ..Default::default()
        };
        let outcome = definition.replace("aaa", 2..3, false, None);
        assert_eq!(outcome.string, "bbb");
        assert_eq!(outcome.selected_range, None);
    }

    #[test]
    fn inserted_line_endings_adopt_the_document_convention() {
        // `\R` is ICU's "any line break"; Rust's `regex` spells it as an
        // alternation, which is what the first rule uses here. The point of the
        // test is that the *expanded* replacement — `$0`, the match itself — is
        // normalised before the second rule reads it.
        let definition = MultipleReplace {
            replacements: vec![
                regex_rule(r"dog(?:\r\n|[\r\n])cow", "$0"),
                regex_rule(r"dog\ncow", "cat\r"),
            ],
            ..Default::default()
        };
        let outcome = definition.replace("before\r|dog\r\ncow|after\r", 0..0, false, Some(LineEnding::LF));
        assert_eq!(outcome.string, "before\r|cat\n|after\r");
    }

    #[test]
    fn a_rule_without_a_line_ending_keeps_the_one_it_was_given() {
        let definition = MultipleReplace {
            replacements: vec![regex_rule("a", "b")],
            ..Default::default()
        };
        let outcome = definition.replace("a", 0..0, false, Some(LineEnding::CRLF));
        assert_eq!(outcome.string, "b");
    }

    #[test]
    fn disabled_and_invalid_rules_are_skipped() {
        let definition = MultipleReplace {
            replacements: vec![
                Replacement { is_enabled: false, ..rule("hello", "hi") },
                regex_rule("(", "X"),
                rule("world", "earth"),
            ],
            ..Default::default()
        };
        let outcome = definition.replace("hello world", 0..0, false, None);
        assert_eq!(outcome.string, "hello earth");
        assert_eq!(outcome.count, 1);
    }

    #[test]
    fn find_returns_every_rule_s_hits_in_rule_order() {
        let definition = MultipleReplace {
            replacements: vec![rule("abc", ""), rule("def", "")],
            ..Default::default()
        };
        let found = definition.find("abc def abc", 0..0, false);
        // Rule order, not document order: the second rule's hit comes last.
        assert_eq!(found, vec![0..3, 8..11, 4..7]);
    }

    #[test]
    fn find_ignores_disabled_rules() {
        let definition = MultipleReplace {
            replacements: vec![
                Replacement { is_enabled: false, ..rule("abc", "") },
                rule("def", ""),
            ],
            ..Default::default()
        };
        assert_eq!(definition.find("abc def abc", 0..0, false), vec![4..7]);
    }

    #[test]
    fn find_in_an_empty_selection_finds_nothing() {
        let definition = MultipleReplace {
            replacements: vec![rule("abc", "")],
            ..Default::default()
        };
        assert!(definition.find("abc abc", 3..3, true).is_empty());
    }

    #[test]
    fn find_sees_the_original_text_for_every_rule() {
        // Unlike `replace`, a rule cannot match text an earlier rule inserted:
        // the first rule would rewrite `abc` to `x`, but the second rule is
        // still searching the string as it was.
        let definition = MultipleReplace {
            replacements: vec![rule("abc", "x"), rule("x", "")],
            ..Default::default()
        };
        assert_eq!(definition.find("abc", 0..0, false), vec![0..3]);
    }

    #[test]
    fn tsv_import_reads_the_first_two_fields_of_each_line() {
        let definition = MultipleReplace::from_tsv("cat\tdog\nb\tB\nc\tC\n");
        assert_eq!(
            definition.replacements,
            vec![rule("cat", "dog"), rule("b", "B"), rule("c", "C")]
        );
        assert_eq!(definition.settings, Settings::default());
    }

    #[test]
    fn tsv_import_skips_what_it_cannot_read() {
        // Blank lines, a line with one field, and a line whose first field is
        // empty all drop out; the rest of the file still loads.
        let definition = MultipleReplace::from_tsv("cow\n\n\tvalue\ncat\tdog\n\n\n");
        assert_eq!(definition.replacements, vec![rule("cat", "dog")]);
    }

    #[test]
    fn tsv_import_reads_an_empty_replacement() {
        let definition = MultipleReplace::from_tsv("dog\t");
        assert_eq!(definition.replacements, vec![rule("dog", "")]);
    }

    #[test]
    fn tsv_import_of_nothing_is_empty() {
        assert!(MultipleReplace::from_tsv("").is_empty());
    }

    #[test]
    fn tsv_import_splits_on_both_line_endings() {
        assert_eq!(
            MultipleReplace::from_tsv("a\tb\r\nc\td\re\tf").replacements.len(),
            3
        );
    }

    #[test]
    fn unescaping_covers_the_escapes_swift_defines() {
        assert_eq!(unescaped_template(r"a\0b"), "a\0b");
        assert_eq!(unescaped_template(r"a\tb"), "a\tb");
        assert_eq!(unescaped_template(r"a\nb"), "a\nb");
        assert_eq!(unescaped_template(r"a\rb"), "a\rb");
        assert_eq!(unescaped_template(r#"a\"b"#), "a\"b");
        assert_eq!(unescaped_template(r"a\'b"), "a'b");
        // `\\` collapses, because `regex` does not treat a backslash as an
        // escape in a template the way ICU does.
        assert_eq!(unescaped_template(r"a\\b"), r"a\b");
        // Anything else keeps its backslash.
        assert_eq!(unescaped_template(r"a\qb"), r"a\qb");
        assert_eq!(unescaped_template("a\\"), "a\\");
    }

    #[test]
    fn unescaping_is_only_for_regular_expression_rules() {
        let settings = Settings { unescapes_replacement_string: true, ..Default::default() };
        assert_eq!(rule("a", r"\n").template(&settings), r"\n");
        assert_eq!(regex_rule("a", r"\n").template(&settings), "\n");
    }

    #[test]
    fn unescaping_can_be_switched_off() {
        let settings = Settings { unescapes_replacement_string: false, ..Default::default() };
        assert_eq!(regex_rule("a", r"\n").template(&settings), r"\n");
    }

    #[test]
    fn a_selection_in_the_middle_of_a_character_is_snapped_to_a_boundary() {
        // `é` as `e` plus a combining acute: three bytes, and offset 2 is not a
        // boundary. Clamping must not panic.
        let text = "e\u{0301}x";
        let definition = MultipleReplace {
            replacements: vec![rule("x", "y")],
            ..Default::default()
        };
        let outcome = definition.replace(text, 2..4, true, None);
        assert_eq!(outcome.string, "e\u{0301}y");
    }

    #[test]
    fn a_scope_past_the_end_of_the_text_is_clamped() {
        let definition = MultipleReplace {
            replacements: vec![rule("a", "b")],
            ..Default::default()
        };
        let outcome = definition.replace("aaa", 0..99, false, None);
        assert_eq!(outcome.string, "bbb");
        assert_eq!(outcome.count, 3);
    }
}
