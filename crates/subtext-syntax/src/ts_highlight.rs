//! Tree-sitter highlighting.
//!
//! Twenty-one of CotEditor's seventy syntaxes have no `Highlights.json` at all:
//! their colours come from a tree-sitter query beside the bundle,
//! `queries/<Name>/highlights.scm`. SubText used to fall back to the comment and
//! string delimiters from `Edit.json` for those languages, so a Rust or Python
//! buffer showed keywords, types, numbers and functions in the plain text
//! colour — everything but comments and literals.
//!
//! The capture names in those queries are CotEditor's own token names —
//! `@keywords`, `@types`, `@commands.method` — which is why this module needs no
//! translation table: the first component is the token, and anything that is
//! not one of the ten (a `@name` used inside a `#eq?` predicate, say) is not a
//! colour.
//!
//! Two of CotEditor's steps are reproduced here, in its order:
//!
//! * **conflict resolution** — two patterns matching the same node give the
//!   same range, and the one declared later in the query wins;
//! * **painting** — the surviving captures are applied in document order, so a
//!   string inside a call keeps the string colour over the call's.

use std::ops::Range;

use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use crate::highlight::{HighlightToken, TokenSpan};
use crate::ts_outline::{self, Error};

/// Run a highlight query over `text`.
pub fn highlights(syntax: &str, text: &str, query_source: &str) -> Result<Vec<TokenSpan>, Error> {
    let Some(language) = ts_outline::language(syntax) else {
        return Ok(Vec::new());
    };
    let mut parser = Parser::new();
    parser.set_language(&language).map_err(Error::Language)?;
    let Some(tree) = parser.parse(text, None) else {
        return Ok(Vec::new());
    };
    let query = Query::new(&language, query_source).map_err(Error::Query)?;

    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut found: Vec<Capture> = Vec::new();
    let mut matches = cursor.matches(&query, tree.root_node(), text.as_bytes());
    while let Some(matched) = matches.next() {
        for capture in matched.captures {
            let Some(name) = names.get(capture.index as usize) else {
                continue;
            };
            let Some(token) = token_for(name) else {
                continue;
            };
            found.push(Capture {
                range: capture.node.start_byte()..capture.node.end_byte(),
                token,
                pattern: matched.pattern_index,
            });
        }
    }

    // CotEditor sorts captures by location and then by length before painting
    // them, and resolves two captures of the same node by keeping the one from
    // the later pattern.
    found.sort_by_key(|capture| {
        (
            capture.range.start,
            capture.range.end - capture.range.start,
            capture.pattern,
        )
    });
    let mut resolved: Vec<Capture> = Vec::with_capacity(found.len());
    for capture in found {
        match resolved.last_mut() {
            Some(last) if last.range == capture.range => {
                if capture.pattern > last.pattern {
                    *last = capture;
                }
            }
            _ => resolved.push(capture),
        }
    }

    Ok(paint(text.len(), &resolved))
}

/// One capture that names a token.
struct Capture {
    range: Range<usize>,
    token: HighlightToken,
    /// Which pattern in the query produced it. A later pattern is the more
    /// specific one, which is what makes `(#eq? …)`-guarded rules win over the
    /// general rule above them.
    pattern: usize,
}

/// The token a capture name stands for.
///
/// `@commands.method` is a command: the suffix narrows the *pattern*, not the
/// colour, and CotEditor reads only the first component. A name that is not one
/// of the ten token names — `@name`, `@pkg`, `@_env` — is a helper for a
/// predicate and has no colour of its own.
fn token_for(name: &str) -> Option<HighlightToken> {
    HighlightToken::from_key(name.split('.').next()?)
}

/// Paint the captures over the buffer and return the resulting runs.
///
/// Painting rather than merging is what makes nesting come out right: captures
/// arrive in document order, so a later one overwrites the earlier one's bytes,
/// and a capture that starts later is a node inside the earlier one.
fn paint(length: usize, captures: &[Capture]) -> Vec<TokenSpan> {
    let mut painted: Vec<Option<HighlightToken>> = vec![None; length];
    for capture in captures {
        let end = capture.range.end.min(length);
        if capture.range.start >= end {
            continue;
        }
        painted[capture.range.start..end].fill(Some(capture.token));
    }

    let mut spans = Vec::new();
    let mut index = 0;
    while index < painted.len() {
        let Some(token) = painted[index] else {
            index += 1;
            continue;
        };
        let start = index;
        while index < painted.len() && painted[index] == Some(token) {
            index += 1;
        }
        spans.push(TokenSpan {
            range: start..index,
            token,
        });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(syntax: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../resources/queries")
            .join(syntax)
            .join("highlights.scm");
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path:?}: {error}"))
    }

    /// The text of every span with a given token, in order.
    fn text_with(text: &str, spans: &[TokenSpan], token: HighlightToken) -> Vec<String> {
        spans
            .iter()
            .filter(|span| span.token == token)
            .map(|span| text[span.range.clone()].to_string())
            .collect()
    }

    fn spans(syntax: &str, text: &str) -> Vec<TokenSpan> {
        highlights(syntax, text, &query(syntax)).unwrap_or_else(|error| panic!("{syntax}: {error}"))
    }

    #[test]
    fn a_rust_file_gets_its_keywords_types_and_strings() {
        let text = "fn main() {\n    let x: u32 = 1;\n    let s = \"hi\";\n    // note\n}\n";
        let spans = spans("Rust", text);

        assert_eq!(
            text_with(text, &spans, HighlightToken::Keywords),
            ["fn", "let", "let"]
        );
        assert_eq!(text_with(text, &spans, HighlightToken::Types), ["u32"]);
        assert_eq!(text_with(text, &spans, HighlightToken::Strings), ["\"hi\""]);
        assert_eq!(
            text_with(text, &spans, HighlightToken::Comments),
            ["// note"]
        );
        assert_eq!(text_with(text, &spans, HighlightToken::Numbers), ["1"]);
    }

    /// The spans tile without overlapping, which is what the editor's renderer
    /// needs: every byte is either in exactly one span or in none.
    #[test]
    fn the_spans_do_not_overlap() {
        let text = "fn main() {\n    let s = \"a // not a comment\";\n}\n";
        let spans = spans("Rust", text);
        for pair in spans.windows(2) {
            assert!(
                pair[0].range.end <= pair[1].range.start,
                "overlapping spans: {:?} and {:?}",
                pair[0].range,
                pair[1].range
            );
        }
        // The `//` inside the string stays a string.
        assert_eq!(
            text_with(text, &spans, HighlightToken::Comments),
            Vec::<String>::new()
        );
        assert_eq!(
            text_with(text, &spans, HighlightToken::Strings),
            ["\"a // not a comment\""]
        );
    }

    #[test]
    fn a_dotted_capture_name_is_its_first_component() {
        assert_eq!(token_for("commands.method"), Some(HighlightToken::Commands));
        assert_eq!(
            token_for("variables.builtin"),
            Some(HighlightToken::Variables)
        );
        // A helper capture for a predicate has no colour.
        assert_eq!(token_for("name"), None);
        assert_eq!(token_for("_env"), None);
    }

    #[test]
    fn a_later_pattern_wins_on_the_same_node() {
        // Two patterns capture the same identifier; the second one is the
        // specific rule, and CotEditor keeps it.
        let text = "abc\n";
        let source = "(identifier) @variables\n(identifier) @types\n";
        let language = ts_outline::language("Rust").expect("grammar");
        let mut parser = Parser::new();
        parser.set_language(&language).expect("language");
        let tree = parser.parse(text, None).expect("parse");
        let query = Query::new(&language, source).expect("query");
        let names = query.capture_names();
        let mut cursor = QueryCursor::new();
        let mut found = Vec::new();
        let mut matches = cursor.matches(&query, tree.root_node(), text.as_bytes());
        while let Some(matched) = matches.next() {
            for capture in matched.captures {
                let name = names[capture.index as usize];
                if let Some(token) = token_for(name) {
                    found.push(Capture {
                        range: capture.node.start_byte()..capture.node.end_byte(),
                        token,
                        pattern: matched.pattern_index,
                    });
                }
            }
        }
        found.sort_by_key(|capture| {
            (
                capture.range.start,
                capture.range.end - capture.range.start,
                capture.pattern,
            )
        });
        let mut resolved: Vec<Capture> = Vec::new();
        for capture in found {
            match resolved.last_mut() {
                Some(last) if last.range == capture.range => {
                    if capture.pattern > last.pattern {
                        *last = capture;
                    }
                }
                _ => resolved.push(capture),
            }
        }
        let spans = paint(text.len(), &resolved);
        assert_eq!(
            spans,
            [TokenSpan {
                range: 0..3,
                token: HighlightToken::Types
            }]
        );
    }

    /// Every bundled highlight query parses against its grammar.
    ///
    /// The same gate the outline has, for the same reason: a node type the
    /// grammar no longer has makes `Query::new` fail, and a failed query is an
    /// unhighlighted buffer with no message.
    #[test]
    fn every_bundled_highlight_query_parses() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/queries");
        let mut checked = 0;
        for entry in std::fs::read_dir(&root).expect("queries/") {
            let directory = entry.expect("entry").path();
            let name = directory
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            if !directory.join("highlights.scm").is_file() {
                continue;
            }
            let source = std::fs::read_to_string(directory.join("highlights.scm")).expect("query");
            // Two queries cannot run against the grammars this build carries:
            //
            // * SQL's needs node types the published grammar lacks — the same
            //   exception the outline gate names;
            // * Kotlin's was written for `fwcd/tree-sitter-kotlin`, which is
            //   only published for the tree-sitter 0.20 API; the alternatives
            //   (`-sg`, `-ng`) rename the nodes the query asks for
            //   (`type_identifier`, `break`).
            //
            // Both are listed rather than skipped, so the day a grammar catches
            // up the test fails and says so.
            const NOT_YET: [&str; 1] = ["SQL"];
            let result = highlights(&name, "", &source);
            if NOT_YET.contains(&name.as_str()) {
                assert!(result.is_err(), "{name} parses now — wire it up");
                continue;
            }
            result.unwrap_or_else(|error| panic!("{name}: {error}"));
            checked += 1;
        }
        assert_eq!(checked, 20, "expected twenty parsable highlight queries");
    }
}
