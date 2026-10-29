//! Running a syntax's highlighting rules over a buffer.
//!
//! CotEditor's rules are begin/end pairs of regular expressions (or literal
//! strings) that may span lines. They are applied in a fixed priority order,
//! and a rule never paints over text a higher-priority rule already claimed —
//! which is why a `//` inside a string literal stays a string.

use std::collections::BTreeMap;
use std::ops::Range;

use regex::{Regex, RegexBuilder};
use serde::Deserialize;

/// The token categories a `.cotsyntax` file can define.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HighlightToken {
    Attributes,
    Characters,
    Commands,
    Comments,
    Keywords,
    Numbers,
    Strings,
    Types,
    Values,
    Variables,
}

/// The order rules are applied in, highest priority first.
///
/// This is the exact reverse of CotEditor's `SyntaxType.allCases`: its
/// `highlights(dictionary:)` walks `allCases.reversed()` and lets each type
/// claim only the bytes no later type already took, which makes the *first*
/// case the weakest. Comments therefore win over everything, and keywords
/// lose to everything — so a keyword spelled inside a comment or a literal
/// stays part of that comment or literal.
pub const PRIORITY: [HighlightToken; 10] = [
    HighlightToken::Comments,
    HighlightToken::Characters,
    HighlightToken::Strings,
    HighlightToken::Numbers,
    HighlightToken::Values,
    HighlightToken::Variables,
    HighlightToken::Attributes,
    HighlightToken::Types,
    HighlightToken::Commands,
    HighlightToken::Keywords,
];

impl HighlightToken {
    /// The key this token uses inside `Highlights.json`.
    pub fn key(self) -> &'static str {
        match self {
            HighlightToken::Attributes => "attributes",
            HighlightToken::Characters => "characters",
            HighlightToken::Commands => "commands",
            HighlightToken::Comments => "comments",
            HighlightToken::Keywords => "keywords",
            HighlightToken::Numbers => "numbers",
            HighlightToken::Strings => "strings",
            HighlightToken::Types => "types",
            HighlightToken::Values => "values",
            HighlightToken::Variables => "variables",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        PRIORITY.into_iter().find(|token| token.key() == key)
    }

    /// The name the *painter's* theme resolver knows this token by.
    ///
    /// CotEditor's own key (`keywords`) is the one `Highlights.json` and the
    /// tree-sitter queries spell, and [`key`](Self::key) keeps it. The editor,
    /// though, paints through `gpui-component`'s syntax palette, which speaks
    /// Zed's singular vocabulary — `keyword`, `string.special`, `type` — so the
    /// ten tokens map onto those names here, once.
    pub fn style_name(self) -> &'static str {
        match self {
            HighlightToken::Attributes => "attribute",
            HighlightToken::Characters => "string.special",
            HighlightToken::Commands => "function",
            HighlightToken::Comments => "comment",
            HighlightToken::Keywords => "keyword",
            HighlightToken::Numbers => "number",
            HighlightToken::Strings => "string",
            HighlightToken::Types => "type",
            HighlightToken::Values => "constant",
            HighlightToken::Variables => "variable",
        }
    }

    pub fn priority(self) -> usize {
        PRIORITY
            .iter()
            .position(|t| *t == self)
            .unwrap_or(usize::MAX)
    }
}

/// A rule exactly as it appears in `Highlights.json`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRule {
    pub begin: String,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub regular_expression: bool,
    #[serde(default)]
    pub ignore_case: bool,
    #[serde(default)]
    pub is_multiline: bool,
    #[serde(default)]
    pub prefixes: Vec<String>,
}

/// How a rule decides where its span stops.
#[derive(Debug, Clone)]
enum Terminator {
    /// Just the `begin` match — a rule like `//.*` that spells out its own end.
    None,
    /// An explicit closing pattern.
    Pattern(Regex),
    /// An inline comment: everything to the end of the line.
    EndOfLine,
}

/// A rule with its patterns compiled.
///
/// The extra fields beyond `Highlights.json` come from `Edit.json`: comment
/// delimiters, string delimiters and character delimiters are *editing*
/// structures that CotEditor also feeds to the highlighter, which is why
/// `comments` and `strings` are almost absent from the highlight files.
#[derive(Debug, Clone)]
pub struct HighlightRule {
    pub description: Option<String>,
    pub is_multiline: bool,
    begin: Regex,
    terminator: Terminator,
    /// Text that must sit immediately before `begin`, e.g. Rust's `r` in
    /// `r"..."`.
    prefixes: Vec<String>,
    /// A delimiter preceded by this character is not a delimiter.
    escape: Option<char>,
    /// A block comment whose markers may nest, like Rust's `/* /* */ */`.
    nestable: bool,
}

impl HighlightRule {
    /// Compile a raw rule from `Highlights.json`.
    pub fn compile(raw: RawRule) -> Result<Self, regex::Error> {
        let begin = compile_pattern(&raw.begin, raw.regular_expression, raw.ignore_case)?;
        let terminator = match &raw.end {
            Some(pattern) => Terminator::Pattern(compile_pattern(
                pattern,
                raw.regular_expression,
                raw.ignore_case,
            )?),
            None => Terminator::None,
        };
        Ok(Self {
            description: raw.description,
            is_multiline: raw.is_multiline,
            begin,
            terminator,
            prefixes: raw.prefixes,
            escape: None,
            nestable: false,
        })
    }

    /// An inline comment: `begin` through the end of the line.
    pub fn inline_comment(begin: &str) -> Result<Self, regex::Error> {
        Ok(Self {
            description: Some(format!("{begin} comment")),
            is_multiline: false,
            begin: compile_pattern(begin, false, false)?,
            terminator: Terminator::EndOfLine,
            prefixes: Vec::new(),
            escape: None,
            nestable: false,
        })
    }

    /// A block comment or string: `begin` through the next unescaped `end`.
    pub fn delimited(
        begin: &str,
        end: &str,
        escape: Option<char>,
        is_multiline: bool,
        nestable: bool,
        prefixes: Vec<String>,
    ) -> Result<Self, regex::Error> {
        Ok(Self {
            description: Some(format!("{begin} … {end}")),
            is_multiline,
            begin: compile_pattern(begin, false, false)?,
            terminator: Terminator::Pattern(compile_pattern(end, false, false)?),
            prefixes,
            escape,
            nestable,
        })
    }

    /// Every span this rule claims in `text`.
    fn scan(&self, text: &str) -> Vec<Range<usize>> {
        let mut spans = Vec::new();
        let mut search_from = 0;

        while search_from <= text.len() {
            let Some(found) = self.begin.find_at(text, search_from) else {
                break;
            };
            // A zero-width match would loop forever; step past it.
            let advance = if found.end() > found.start() {
                found.end()
            } else {
                found.end() + 1
            };

            // A prefix such as Rust's `r` belongs to the literal, so the span
            // starts at the prefix, not at the delimiter.
            let Some(span_start) = self.prefix_start(text, found.start()) else {
                search_from = advance;
                continue;
            };

            let limit = if self.is_multiline {
                text.len()
            } else {
                line_end(text, found.start())
            };
            let span_end = self.end_of(text, found.end(), limit);

            if span_end > span_start {
                spans.push(span_start..span_end);
            }
            search_from = span_end.max(advance);
            if search_from > text.len() {
                break;
            }
        }

        spans
    }

    /// Where the span begins, including any required prefix.
    ///
    /// Returns `None` when a prefix is required and none sits before `start`.
    fn prefix_start(&self, text: &str, start: usize) -> Option<usize> {
        if self.prefixes.is_empty() {
            return Some(start);
        }
        self.prefixes.iter().find_map(|prefix| {
            let before = text[..start].strip_suffix(prefix.as_str())?;
            // The prefix must be a standalone token, not the tail of an
            // identifier: `br"` is a prefix, `foo"` is not.
            let standalone = !before
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
            standalone.then_some(start - prefix.len())
        })
    }

    /// Where the span stops, given the position just after `begin`.
    fn end_of(&self, text: &str, from: usize, limit: usize) -> usize {
        match &self.terminator {
            Terminator::None => from.min(limit.max(from)),
            Terminator::EndOfLine => limit,
            Terminator::Pattern(end) => {
                let mut cursor = from;
                loop {
                    if cursor > limit {
                        // Unterminated: run to the end of the line (or of the
                        // document for a multiline construct), so the reader
                        // can see that the literal was never closed.
                        return limit;
                    }
                    let Some(found) = end.find_at(text, cursor) else {
                        return limit;
                    };
                    if found.end() > limit {
                        return limit;
                    }
                    if self.is_escaped(text, found.start()) {
                        cursor = found.end().max(cursor + 1);
                        continue;
                    }
                    if self.nestable {
                        // Count how many more `begin` markers open before this
                        // `end`, and skip that many closers.
                        let between = &text[from..found.start()];
                        let opens = self.begin.find_iter(between).count();
                        if opens > 0 {
                            let mut remaining = opens;
                            let mut scan = found.end();
                            while remaining > 0 {
                                let Some(next) = end.find_at(text, scan) else {
                                    return limit;
                                };
                                remaining -= 1;
                                scan = next.end().max(scan + 1);
                            }
                            return scan;
                        }
                    }
                    return found.end();
                }
            }
        }
    }

    /// Whether the character at `offset` is escaped by a preceding backslash.
    fn is_escaped(&self, text: &str, offset: usize) -> bool {
        let Some(escape) = self.escape else {
            return false;
        };
        let mut backslashes = 0;
        for c in text[..offset].chars().rev() {
            if c == escape {
                backslashes += 1;
            } else {
                break;
            }
        }
        backslashes % 2 == 1
    }
}

fn compile_pattern(
    pattern: &str,
    is_regex: bool,
    ignore_case: bool,
) -> Result<Regex, regex::Error> {
    let source = if is_regex {
        pattern.to_string()
    } else {
        regex::escape(pattern)
    };
    RegexBuilder::new(&source)
        .case_insensitive(ignore_case)
        .multi_line(true)
        .build()
}

/// End of the line containing `offset`, excluding the newline.
fn line_end(text: &str, offset: usize) -> usize {
    text[offset..].find('\n').map_or(text.len(), |i| offset + i)
}

/// A styled span produced by highlighting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSpan {
    pub range: Range<usize>,
    pub token: HighlightToken,
}

/// Where a highlighter gets its colours from.
#[derive(Debug, Clone)]
enum Source {
    /// A `.cotsyntax` bundle's `Highlights.json`, already compiled.
    Rules(Vec<(HighlightToken, Vec<HighlightRule>)>),
    /// A tree-sitter query, for the syntaxes that have no rules at all.
    Query { syntax: String, query: String },
}

/// A syntax's rules bound to a buffer.
#[derive(Debug, Clone)]
pub struct Highlighter {
    /// Rules grouped by token, ready to apply in [`PRIORITY`] order — or the
    /// query that replaces them.
    source: Source,
    spans: Vec<TokenSpan>,
}

impl Highlighter {
    /// Build a highlighter from a syntax's rule map.
    pub fn new(rules: &BTreeMap<HighlightToken, Vec<HighlightRule>>) -> Self {
        let ordered = PRIORITY
            .into_iter()
            .filter_map(|token| rules.get(&token).map(|rules| (token, rules.clone())))
            .collect();
        Self {
            source: Source::Rules(ordered),
            spans: Vec::new(),
        }
    }

    /// Build a highlighter from a tree-sitter highlight query.
    ///
    /// Twenty-one of CotEditor's syntaxes have no rules to compile: their
    /// colours are a query, and a regex engine cannot read one. See
    /// [`ts_highlight`](crate::ts_highlight).
    pub fn with_query(syntax: &str, query: &str) -> Self {
        Self {
            source: Source::Query {
                syntax: syntax.to_string(),
                query: query.to_string(),
            },
            spans: Vec::new(),
        }
    }

    /// Recompute every span for `text`.
    ///
    /// The two sources differ in how a conflict is settled, because the two
    /// formats do: a rule never overwrites an earlier one, so the first claim on
    /// a byte wins, while a query's captures are painted in document order, so
    /// the last one does. Both end up non-overlapping, which is what the
    /// renderer needs.
    pub fn rehighlight(&mut self, text: &str) {
        match &self.source {
            Source::Rules(rules) => {
                let mut claimed = vec![false; text.len()];
                let mut spans = Vec::new();

                for (token, rules) in rules {
                    for rule in rules {
                        for range in rule.scan(text) {
                            if range.end > text.len() || range.start >= range.end {
                                continue;
                            }
                            if claimed[range.clone()].iter().any(|taken| *taken) {
                                continue;
                            }
                            claimed[range.clone()].fill(true);
                            spans.push(TokenSpan {
                                range,
                                token: *token,
                            });
                        }
                    }
                }

                spans.sort_by_key(|span| span.range.start);
                self.spans = spans;
            }
            Source::Query { syntax, query } => {
                // A query that does not parse against its grammar leaves the
                // buffer uncoloured rather than half-coloured; the gate in
                // `ts_highlight` is what keeps one from shipping.
                self.spans =
                    crate::ts_highlight::highlights(syntax, text, query).unwrap_or_default();
            }
        }
    }

    pub fn spans(&self) -> &[TokenSpan] {
        &self.spans
    }

    /// The token covering `offset`, if any.
    pub fn token_at(&self, offset: usize) -> Option<HighlightToken> {
        self.spans
            .iter()
            .find(|span| span.range.start <= offset && offset < span.range.end)
            .map(|span| span.token)
    }

    /// The token covering `offset`, used for comment and string awareness.
    pub fn is_in_comment_or_string(&self, offset: usize) -> bool {
        matches!(
            self.token_at(offset),
            Some(HighlightToken::Comments | HighlightToken::Strings | HighlightToken::Characters)
        )
    }
}

#[cfg(test)]
mod tests {

    /// Two vocabularies, one token: CotEditor's plural key is what the files
    /// spell, and the painter's theme knows the singular one.
    #[test]
    fn a_token_has_a_file_name_and_a_theme_name() {
        assert_eq!(HighlightToken::Keywords.key(), "keywords");
        assert_eq!(HighlightToken::Keywords.style_name(), "keyword");
        assert_eq!(HighlightToken::Characters.key(), "characters");
        assert_eq!(HighlightToken::Characters.style_name(), "string.special");
        // The file vocabulary still round-trips, which is what `Highlights.json`
        // and the tree-sitter queries rely on.
        for token in PRIORITY {
            assert_eq!(HighlightToken::from_key(token.key()), Some(token));
        }
    }

    use super::*;

    fn rules(entries: &[(&str, RawRule)]) -> BTreeMap<HighlightToken, Vec<HighlightRule>> {
        let mut map: BTreeMap<HighlightToken, Vec<HighlightRule>> = BTreeMap::new();
        for (key, raw) in entries {
            let token = HighlightToken::from_key(key).unwrap();
            map.entry(token)
                .or_default()
                .push(HighlightRule::compile(raw.clone()).unwrap());
        }
        map
    }

    fn raw(begin: &str, end: Option<&str>, regex: bool, multiline: bool) -> RawRule {
        RawRule {
            begin: begin.into(),
            end: end.map(Into::into),
            description: None,
            regular_expression: regex,
            ignore_case: false,
            is_multiline: multiline,
            prefixes: Vec::new(),
        }
    }

    #[test]
    fn token_keys_round_trip() {
        for token in PRIORITY {
            assert_eq!(HighlightToken::from_key(token.key()), Some(token));
        }
        assert_eq!(HighlightToken::from_key("nonsense"), None);
    }

    #[test]
    fn literal_keyword_is_highlighted() {
        let map = rules(&[("keywords", raw("if", None, false, false))]);
        let mut highlighter = Highlighter::new(&map);
        highlighter.rehighlight("if a then if");
        assert_eq!(highlighter.spans().len(), 2);
        assert_eq!(highlighter.token_at(0), Some(HighlightToken::Keywords));
        assert_eq!(highlighter.token_at(3), None);
    }

    #[test]
    fn string_wins_over_keyword_inside_it() {
        let map = rules(&[
            ("keywords", raw("if", None, false, false)),
            ("strings", raw("\"", Some("\""), false, false)),
        ]);
        let mut highlighter = Highlighter::new(&map);
        highlighter.rehighlight("x = \"if if if\"");
        // Everything inside the quotes is a string, including the keyword.
        assert_eq!(highlighter.token_at(5), Some(HighlightToken::Strings));
        assert_eq!(highlighter.token_at(9), Some(HighlightToken::Strings));
    }

    #[test]
    fn comment_wins_over_string() {
        // CotEditor spells a "to end of line" comment as a regex anchored at
        // the marker, not as an `end` delimiter.
        let map = rules(&[
            ("strings", raw("\"", Some("\""), false, false)),
            ("comments", raw("//.*", None, true, false)),
        ]);
        let mut highlighter = Highlighter::new(&map);
        let text = "let s = \"a\"; // a comment";
        highlighter.rehighlight(text);
        let comment_start = text.find("//").unwrap();
        assert_eq!(
            highlighter.token_at(comment_start),
            Some(HighlightToken::Comments)
        );
        assert_eq!(
            highlighter.token_at(text.len() - 1),
            Some(HighlightToken::Comments)
        );
        // The string before the comment is untouched by the comment rule.
        assert_eq!(highlighter.token_at(8), Some(HighlightToken::Strings));
    }

    #[test]
    fn a_rule_without_an_end_claims_only_its_own_match() {
        let map = rules(&[("comments", raw("//", None, false, false))]);
        let mut highlighter = Highlighter::new(&map);
        highlighter.rehighlight("a // b");
        assert_eq!(highlighter.spans()[0].range, 2..4);
    }

    #[test]
    fn multiline_rule_spans_lines() {
        let map = rules(&[("comments", raw("/\\*", Some("\\*/"), true, true))]);
        let mut highlighter = Highlighter::new(&map);
        let text = "/* one\ntwo */ after";
        highlighter.rehighlight(text);
        let closing = text.find("*/").unwrap() + 2;
        assert_eq!(highlighter.spans()[0].range, 0..closing);
        assert_eq!(highlighter.token_at(6), Some(HighlightToken::Comments));
        assert_eq!(highlighter.token_at(closing + 1), None);
    }

    #[test]
    fn single_line_rule_stops_at_the_newline() {
        let map = rules(&[("comments", raw("/\\*", Some("\\*/"), true, false))]);
        let mut highlighter = Highlighter::new(&map);
        let text = "/* one\ntwo */ after";
        highlighter.rehighlight(text);
        // The `*/` sits on the next line, which a single-line rule may not
        // cross, so the span runs to the end of the first line.
        assert_eq!(&text[highlighter.spans()[0].range.clone()], "/* one");
        assert_eq!(highlighter.token_at(8), None);
    }

    #[test]
    fn unterminated_string_runs_to_the_end_of_the_line() {
        let map = rules(&[("strings", raw("\"", Some("\""), false, false))]);
        let mut highlighter = Highlighter::new(&map);
        highlighter.rehighlight("let s = \"unterminated");
        assert_eq!(highlighter.spans().len(), 1);
        // From the opening quote to the end of the line: the reader can see
        // that the literal was never closed.
        assert_eq!(highlighter.spans()[0].range, 8..21);
    }

    #[test]
    fn escape_characters_protect_the_closing_delimiter() {
        let rule =
            HighlightRule::delimited("\"", "\"", Some('\\'), false, false, Vec::new()).unwrap();
        let mut map: BTreeMap<HighlightToken, Vec<HighlightRule>> = BTreeMap::new();
        map.insert(HighlightToken::Strings, vec![rule]);
        let mut highlighter = Highlighter::new(&map);

        let text = r#"let s = "a\"b";"#;
        highlighter.rehighlight(text);
        assert_eq!(&text[highlighter.spans()[0].range.clone()], r#""a\"b""#);
    }

    #[test]
    fn prefixes_are_required_before_a_delimited_literal() {
        let rule =
            HighlightRule::delimited("\"", "\"", None, true, false, vec!["r".into()]).unwrap();
        let mut map: BTreeMap<HighlightToken, Vec<HighlightRule>> = BTreeMap::new();
        map.insert(HighlightToken::Strings, vec![rule]);
        let mut highlighter = Highlighter::new(&map);

        // Only the raw literal prefixed with `r` is claimed.
        let text = "let a = \"plain\"; let b = r\"raw\";";
        highlighter.rehighlight(text);
        assert_eq!(highlighter.spans().len(), 1);
        assert_eq!(&text[highlighter.spans()[0].range.clone()], "r\"raw\"");
    }

    #[test]
    fn a_prefix_that_is_an_identifier_tail_does_not_count() {
        let rule =
            HighlightRule::delimited("\"", "\"", None, false, false, vec!["r".into()]).unwrap();
        let mut map: BTreeMap<HighlightToken, Vec<HighlightRule>> = BTreeMap::new();
        map.insert(HighlightToken::Strings, vec![rule]);
        let mut highlighter = Highlighter::new(&map);
        highlighter.rehighlight("foo\"bar\"");
        assert!(
            highlighter.spans().is_empty(),
            "`foo` is an identifier, not a prefix"
        );
    }

    #[test]
    fn nestable_block_comments_skip_inner_closers() {
        let rule = HighlightRule::delimited("/*", "*/", None, true, true, Vec::new()).unwrap();
        let mut map: BTreeMap<HighlightToken, Vec<HighlightRule>> = BTreeMap::new();
        map.insert(HighlightToken::Comments, vec![rule]);
        let mut highlighter = Highlighter::new(&map);

        let text = "/* outer /* inner */ still */ after";
        highlighter.rehighlight(text);
        assert_eq!(highlighter.spans().len(), 1);
        assert_eq!(
            &text[highlighter.spans()[0].range.clone()],
            "/* outer /* inner */ still */"
        );
    }

    #[test]
    fn non_nestable_block_comments_stop_at_the_first_closer() {
        let rule = HighlightRule::delimited("/*", "*/", None, true, false, Vec::new()).unwrap();
        let mut map: BTreeMap<HighlightToken, Vec<HighlightRule>> = BTreeMap::new();
        map.insert(HighlightToken::Comments, vec![rule]);
        let mut highlighter = Highlighter::new(&map);

        let text = "/* outer /* inner */ still */ after";
        highlighter.rehighlight(text);
        assert_eq!(
            &text[highlighter.spans()[0].range.clone()],
            "/* outer /* inner */"
        );
    }

    #[test]
    fn regex_alternation_matches_whole_keywords() {
        let map = rules(&[("keywords", raw(r"\b(?:let|fn|return)\b", None, true, false))]);
        let mut highlighter = Highlighter::new(&map);
        let text = "let letter = 1; fn f() { return }";
        highlighter.rehighlight(text);

        let spans: Vec<_> = highlighter
            .spans()
            .iter()
            .map(|s| s.range.clone())
            .collect();
        assert_eq!(spans.len(), 3);
        assert_eq!(&text[spans[0].clone()], "let");
        assert_eq!(&text[spans[1].clone()], "fn");
        assert_eq!(&text[spans[2].clone()], "return");
        // "letter" must not be claimed by the `let` alternative.
        assert_eq!(highlighter.token_at(4), None);
    }

    #[test]
    fn zero_width_patterns_terminate() {
        let map = rules(&[("values", raw("^", None, true, false))]);
        let mut highlighter = Highlighter::new(&map);
        highlighter.rehighlight("a\nb\nc");
        // Zero-width matches produce no span, and the scan still finishes.
        assert!(highlighter.spans().is_empty());
    }

    #[test]
    fn comment_and_string_detection_helpers() {
        let map = rules(&[("comments", raw("//", None, false, false))]);
        let mut highlighter = Highlighter::new(&map);
        highlighter.rehighlight("code // note");
        assert!(highlighter.is_in_comment_or_string(6));
        assert!(!highlighter.is_in_comment_or_string(1));
    }
}
