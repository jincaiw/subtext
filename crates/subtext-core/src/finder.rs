//! Find and replace for the find bar: one query, one replacement, in the
//! flavour CotEditor's **Find** menu uses.
//!
//! The *Multiple Replace* sheet runs a whole table of rules instead, and its
//! model lives in [`crate::multireplace`].

use std::ops::Range;

use regex::{Regex, RegexBuilder};

use crate::textops::CommentDelimiters;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindOptions {
    /// Treat the query as a regular expression.
    pub regex: bool,
    pub case_sensitive: bool,
    /// Match only at word boundaries. Ignored for regex queries, where the
    /// pattern is expected to spell out its own boundaries.
    pub whole_word: bool,
    /// Continue from the other end of the document when no match remains.
    pub wrap_around: bool,
    /// Interpret `\n`, `\t`, `\r` and `\0` in a non-regex query.
    pub escape_sequences: bool,
    /// Restrict matches to the current selection.
    pub in_selection: bool,
    /// Let `.` match line separators — upstream's *Dot matches line separators*
    /// (`findRegexIsSingleline`).
    pub singleline: bool,
    /// Let `^` and `$` match the start and end of lines — upstream's
    /// *Anchors match lines* (`findRegexIsMultiline`).
    pub multiline: bool,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            regex: false,
            case_sensitive: false,
            whole_word: false,
            wrap_around: true,
            escape_sequences: false,
            in_selection: false,
            // Upstream's defaults: `findRegexIsSingleline: false`,
            // `findRegexIsMultiline: true`.
            singleline: false,
            multiline: true,
        }
    }
}

/// A compiled search that can be reused across keystrokes.
#[derive(Debug)]
pub struct Searcher {
    regex: Regex,
    /// The pattern was empty, so nothing can ever match.
    empty: bool,
}

impl Searcher {
    /// Compile `query` under `options`. A malformed regex is reported as an
    /// error so the find bar can show it instead of silently matching nothing.
    pub fn new(query: &str, options: &FindOptions) -> Result<Self, regex::Error> {
        if query.is_empty() {
            return Ok(Self {
                regex: Regex::new("").expect("empty pattern is valid"),
                empty: true,
            });
        }
        let pattern = if options.regex {
            query.to_string()
        } else {
            let mut literal = if options.escape_sequences {
                unescape(query)
            } else {
                query.to_string()
            };
            literal = regex::escape(&literal);
            if options.whole_word {
                literal = format!(r"\b{literal}\b");
            }
            literal
        };
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!options.case_sensitive)
            .multi_line(options.multiline)
            .dot_matches_new_line(options.singleline)
            .build()?;
        Ok(Self {
            regex,
            empty: false,
        })
    }

    /// Every match in `text`, in document order and non-overlapping.
    pub fn find_all(&self, text: &str, scope: &Range<usize>) -> Vec<Range<usize>> {
        self.find_all_with_captures(text, scope)
            .iter()
            .map(|captures| captures[0].clone().expect("group 0 always participates"))
            .collect()
    }

    /// Every match with its capture groups, in document order.
    ///
    /// Group 0 is the whole match; the remaining entries follow the pattern's
    /// capture groups, `None` where a group did not participate in that match.
    /// A literal search has exactly one group, so every entry is
    /// `[Some(range)]` — the result table's highlighter keys off the group
    /// count either way, the way upstream's `numberOfCaptureGroups + 1`
    /// colours do.
    pub fn find_all_with_captures(
        &self,
        text: &str,
        scope: &Range<usize>,
    ) -> Vec<Vec<Option<Range<usize>>>> {
        if self.empty {
            return Vec::new();
        }
        let start = scope.start.min(text.len());
        let end = scope.end.min(text.len());
        if start >= end {
            return Vec::new();
        }
        let haystack = &text[start..end];
        self.regex
            .captures_iter(haystack)
            .map(|captures| {
                captures
                    .iter()
                    .map(|group| {
                        group.map(|matched| start + matched.start()..start + matched.end())
                    })
                    .collect()
            })
            .collect()
    }

    /// The first match at or after `from`, wrapping when enabled.
    pub fn find_next(
        &self,
        text: &str,
        from: usize,
        scope: &Range<usize>,
        wrap_around: bool,
    ) -> Option<Range<usize>> {
        if self.empty {
            return None;
        }
        let start = scope.start.min(text.len());
        let end = scope.end.min(text.len());
        let from = from.clamp(start, end);

        if let Some(found) = self.regex.find_at(text, from)
            && found.start() >= from
            && found.end() <= end
        {
            return Some(found.start()..found.end());
        }
        if wrap_around {
            let found = self.regex.find_at(text, start)?;
            if found.end() <= end {
                return Some(found.start()..found.end());
            }
        }
        None
    }

    /// Expand a replacement pattern against the capture groups of `range`.
    ///
    /// `range` must be a match this searcher produced. `$1`, `${name}` and
    /// `$$` follow `regex`'s syntax. Returns `None` when the range is not a
    /// match after all, which can only happen if the text changed underneath.
    pub fn expand(&self, text: &str, range: &Range<usize>, replacement: &str) -> Option<String> {
        let slice = text.get(range.clone())?;
        let captures = self.regex.captures(slice)?;
        let mut out = String::with_capacity(replacement.len());
        captures.expand(replacement, &mut out);
        Some(out)
    }

    /// The last match that ends at or before `from`, wrapping when enabled.
    pub fn find_previous(
        &self,
        text: &str,
        from: usize,
        scope: &Range<usize>,
        wrap_around: bool,
    ) -> Option<Range<usize>> {
        if self.empty {
            return None;
        }
        let start = scope.start.min(text.len());
        let end = scope.end.min(text.len());
        let limit = from.clamp(start, end);

        let mut best: Option<Range<usize>> = None;
        for found in self.regex.find_iter(&text[start..limit]) {
            best = Some(start + found.start()..start + found.end());
        }
        if best.is_some() {
            return best;
        }
        if wrap_around {
            let mut last = None;
            for found in self.regex.find_iter(&text[start..end]) {
                last = Some(start + found.start()..start + found.end());
            }
            return last;
        }
        None
    }
}

/// Expand `\n`, `\t`, `\r`, `\0` and `\\` in a literal query.
pub fn unescape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// How a replacement pattern is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplacementSyntax {
    /// `$1`, `${name}` and `$$` as `regex` defines them.
    #[default]
    Regex,
    /// Insert the text verbatim; `$` has no meaning.
    Literal,
}

/// Replace every match in `text`.
pub fn replace_all(
    text: &str,
    searcher: &Searcher,
    replacement: &str,
    syntax: ReplacementSyntax,
    scope: &Range<usize>,
) -> (String, usize) {
    let matches = searcher.find_all(text, scope);
    if matches.is_empty() {
        return (text.to_string(), 0);
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for m in &matches {
        out.push_str(&text[cursor..m.start]);
        match syntax {
            // `expand` is what turns `$1` into the captured text.
            ReplacementSyntax::Regex => {
                if let Some(expanded) = searcher.expand(text, m, replacement) {
                    out.push_str(&expanded);
                }
            }
            ReplacementSyntax::Literal => out.push_str(replacement),
        }
        cursor = m.end;
    }
    out.push_str(&text[cursor..]);
    (out, matches.len())
}

/// Replace one match, returning the new text and where the caret should sit.
pub fn replace_one(
    text: &str,
    searcher: &Searcher,
    replacement: &str,
    syntax: ReplacementSyntax,
    range: &Range<usize>,
) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..range.start]);
    let inserted = match syntax {
        ReplacementSyntax::Regex => searcher
            .expand(text, range, replacement)
            .unwrap_or_default(),
        ReplacementSyntax::Literal => replacement.to_string(),
    };
    let caret = range.start + inserted.len();
    out.push_str(&inserted);
    out.push_str(&text[range.end..]);
    (out, caret)
}

/// Whether `delimiters` can be used to comment out a document.
pub fn supports_comments(delimiters: &CommentDelimiters) -> bool {
    delimiters.inline.is_some() || delimiters.block.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_bounded_scope_still_reports_offsets_into_the_whole_text() {
        // What *In Selection* rests on: the search is narrowed by the scope,
        // and the matches come back as offsets into the whole buffer — so the
        // editor can select one without translating it back.
        let text = "cat dog cat";
        let searcher = Searcher::new("cat", &FindOptions::default()).expect("a literal query");

        assert_eq!(searcher.find_all(text, &(0..text.len())), vec![0..3, 8..11]);
        assert_eq!(searcher.find_all(text, &(4..11)), vec![8..11]);
        // A scope with nothing in it finds nothing.
        assert!(searcher.find_all(text, &(4..7)).is_empty());
    }

    #[test]
    fn literal_search_is_case_insensitive_by_default() {
        let searcher = Searcher::new("hello", &FindOptions::default()).unwrap();
        let text = "Hello world, hello there";
        assert_eq!(searcher.find_all(text, &(0..text.len())).len(), 2);
    }

    #[test]
    fn case_sensitive_search_narrows_results() {
        let options = FindOptions {
            case_sensitive: true,
            ..Default::default()
        };
        let searcher = Searcher::new("hello", &options).unwrap();
        let text = "Hello world, hello there";
        assert_eq!(searcher.find_all(text, &(0..text.len())).len(), 1);
    }

    #[test]
    fn find_all_with_captures_returns_group_zero_and_optional_groups_in_order() {
        let text = "a=1 b=two";
        let options = FindOptions {
            regex: true,
            ..Default::default()
        };
        let searcher = Searcher::new(r"([a-z])=(1|(two))", &options).unwrap();
        let found = searcher.find_all_with_captures(text, &(0..text.len()));
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], vec![Some(0..3), Some(0..1), Some(2..3), None]);
        assert_eq!(
            found[1],
            vec![Some(4..9), Some(4..5), Some(6..9), Some(6..9)]
        );
    }

    #[test]
    fn find_all_with_captures_preserves_document_offsets_for_a_scoped_search() {
        let text = "prefix alpha suffix";
        let options = FindOptions {
            regex: true,
            ..Default::default()
        };
        let searcher = Searcher::new(r"(alpha)", &options).unwrap();
        assert_eq!(
            searcher.find_all_with_captures(text, &(7..12)),
            vec![vec![Some(7..12), Some(7..12)]]
        );
    }

    #[test]
    fn singleline_lets_dot_match_line_separators() {
        let text = "one\ntwo";
        let options = FindOptions {
            regex: true,
            singleline: true,
            ..Default::default()
        };
        let searcher = Searcher::new("one.two", &options).unwrap();
        assert_eq!(searcher.find_all(text, &(0..text.len())).len(), 1);

        let options = FindOptions {
            regex: true,
            singleline: false,
            ..Default::default()
        };
        let searcher = Searcher::new("one.two", &options).unwrap();
        assert!(searcher.find_all(text, &(0..text.len())).is_empty());
    }

    #[test]
    fn multiline_controls_whether_anchors_match_lines() {
        let text = "x\ny";
        // Upstream's default: anchors match lines.
        let options = FindOptions {
            regex: true,
            multiline: true,
            ..Default::default()
        };
        let searcher = Searcher::new(r"^y", &options).unwrap();
        assert_eq!(searcher.find_all(text, &(0..text.len())).len(), 1);

        let options = FindOptions {
            regex: true,
            multiline: false,
            ..Default::default()
        };
        let searcher = Searcher::new(r"^y", &options).unwrap();
        assert!(searcher.find_all(text, &(0..text.len())).is_empty());
    }

    #[test]
    fn regex_search_uses_capture_groups() {
        let options = FindOptions {
            regex: true,
            ..Default::default()
        };
        let searcher = Searcher::new(r"\d{3}", &options).unwrap();
        let text = "abc 123 def 456";
        assert_eq!(searcher.find_all(text, &(0..text.len())).len(), 2);

        let (replaced, count) = replace_all(
            text,
            &searcher,
            "[$0]",
            ReplacementSyntax::Regex,
            &(0..text.len()),
        );
        assert_eq!(count, 2);
        assert_eq!(replaced, "abc [123] def [456]");
    }

    #[test]
    fn whole_word_does_not_match_substrings() {
        let options = FindOptions {
            whole_word: true,
            ..Default::default()
        };
        let searcher = Searcher::new("cat", &options).unwrap();
        let text = "cat catalog cats";
        assert_eq!(searcher.find_all(text, &(0..text.len())).len(), 1);
    }

    #[test]
    fn find_next_wraps_to_the_top() {
        let searcher = Searcher::new("a", &FindOptions::default()).unwrap();
        let text = "a b a";
        assert_eq!(
            searcher.find_next(text, 1, &(0..text.len()), true),
            Some(4..5)
        );
        assert_eq!(
            searcher.find_next(text, 5, &(0..text.len()), true),
            Some(0..1)
        );
        assert_eq!(searcher.find_next(text, 5, &(0..text.len()), false), None);
    }

    #[test]
    fn find_previous_scans_backwards() {
        let searcher = Searcher::new("a", &FindOptions::default()).unwrap();
        let text = "a b a";
        assert_eq!(
            searcher.find_previous(text, 4, &(0..text.len()), true),
            Some(0..1)
        );
        assert_eq!(
            searcher.find_previous(text, 0, &(0..text.len()), false),
            None
        );
    }

    #[test]
    fn empty_query_matches_nothing() {
        let searcher = Searcher::new("", &FindOptions::default()).unwrap();
        let text = "anything";
        assert!(searcher.find_all(text, &(0..text.len())).is_empty());
    }

    #[test]
    fn escape_sequences_decode_newlines() {
        assert_eq!(unescape(r"a\nb\tc"), "a\nb\tc");
        let options = FindOptions {
            escape_sequences: true,
            ..Default::default()
        };
        let searcher = Searcher::new(r"a\nb", &options).unwrap();
        let text = "a\nb";
        assert_eq!(searcher.find_all(text, &(0..text.len())).len(), 1);
    }

    #[test]
    fn literal_replacement_keeps_dollar_signs() {
        let searcher = Searcher::new("x", &FindOptions::default()).unwrap();
        let (replaced, _) = replace_all("x", &searcher, "$1", ReplacementSyntax::Literal, &(0..1));
        assert_eq!(replaced, "$1");
    }

    #[test]
    fn invalid_regex_is_reported() {
        let options = FindOptions {
            regex: true,
            ..Default::default()
        };
        assert!(Searcher::new("([", &options).is_err());
    }
}
