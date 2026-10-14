//! The Outline inspector, and the document warnings beside it.
//!
//! CotEditor builds its outline from the syntax's `Outlines.json`: each rule
//! is a regular expression plus a `$1`-style template for the label. The same
//! rules work here unchanged — but the *matching* has to match too, and
//! CotEditor's `OutlineExtractor` does four things that are easy to miss:
//!
//! 1. It compiles every pattern with `.anchorsMatchLines`, so `^` and `$`
//!    bind to line boundaries. Without that, a rule like `^([\t ]*)fn` would
//!    only ever match the first line of the file.
//! 2. It collapses runs of whitespace in the expanded title down to one
//!    space, so `$1struct $2` does not come out full of padding.
//! 3. It reads the *nesting* out of the expanded title's leading whitespace,
//!    which is how a rule says "this is inside that".
//! 4. A rule whose `kind` is `separator` produces a rule in the list rather
//!    than a label — YAML's `^---` is the example.
//!
//! All four are reproduced below, because an outline that finds the wrong
//! things is worse than one that finds nothing: the user has no way to tell
//! it is wrong.

use std::sync::LazyLock;

use regex::{Captures, Regex};
use subtext_syntax::OutlineItem;

/// One entry in the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    /// The label, with the rule's nesting whitespace removed. Empty for a
    /// separator.
    pub title: String,
    /// 1-based line the entry sits on.
    pub line: usize,
    /// How far to indent it: a heading's own level, or the width of the
    /// whitespace the rule captured.
    pub level: usize,
    /// The rule's `kind`, kept so the view can style entries differently.
    pub kind: String,
    /// A `kind: separator` rule — a boundary, not a label.
    pub separator: bool,
}

/// Extract outline entries from `text` using a syntax's outline rules.
///
/// Rules are applied in order and their results merged, then sorted by
/// position — which is what makes an outline list headings and functions in
/// the order they appear rather than grouped by rule.
pub fn extract(text: &str, rules: &[OutlineItem]) -> Vec<OutlineEntry> {
    let mut entries = Vec::new();

    for rule in rules {
        let Ok(regex) = compile(rule) else {
            // A syntax that ships a broken pattern should still give an
            // outline from its other rules, not none at all.
            continue;
        };

        for captures in regex.captures_iter(text) {
            let Some(matched) = captures.get(0) else {
                continue;
            };
            let line = text[..matched.start()].matches('\n').count() + 1;

            if rule.kind == "separator" {
                entries.push(OutlineEntry {
                    title: String::new(),
                    line,
                    level: 0,
                    kind: rule.kind.clone(),
                    separator: true,
                });
                continue;
            }

            let label = collapse_whitespace(&expand(&captures, &rule.template));
            let (indent, title) = split_indent(&label);
            if title.is_empty() {
                continue;
            }

            entries.push(OutlineEntry {
                title: title.to_string(),
                line,
                level: level_for(rule, indent),
                kind: rule.kind.clone(),
                separator: false,
            });
        }
    }

    entries.sort_by_key(|entry| (entry.line, entry.level));
    entries.dedup_by(|a, b| a.line == b.line && a.title == b.title);
    entries
}

/// Compile a rule the way CotEditor compiles it.
fn compile(rule: &OutlineItem) -> Result<Regex, regex::Error> {
    // `.anchorsMatchLines` is not optional: every bundled outline pattern
    // starts with `^`, and without it they would only see the first line.
    let mut pattern = String::from("(?m)");
    if rule.ignore_case {
        pattern.push_str("(?i)");
    }
    pattern.push_str(&rule.pattern);
    Regex::new(&pattern)
}

/// How deeply an entry is nested.
///
/// CotEditor's `OutlineItem.Indent` is either an explicit level or the
/// whitespace the rule captured; `title` and `heading.N` are the two kinds
/// that carry a level of their own.
fn level_for(rule: &OutlineItem, indent: &str) -> usize {
    match rule.kind.as_str() {
        "title" => 0,
        kind => kind
            .strip_prefix("heading.")
            .and_then(|level| level.parse::<usize>().ok())
            .unwrap_or_else(|| indent_width(indent)),
    }
}

/// The width of an indent in columns, counting a tab as four.
fn indent_width(indent: &str) -> usize {
    indent.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

/// The leading whitespace of `label`, and the label without it.
fn split_indent(label: &str) -> (&str, &str) {
    let trimmed = label.trim_start_matches([' ', '\t']);
    (label.split_at(label.len() - trimmed.len()).0, trimmed)
}

/// Squeeze every whitespace run that follows a non-space down to one space.
///
/// This is CotEditor's `.replacing(/(\S)\s+/) { "\($0.1) " }`, and it is what
/// turns a template like `$1struct $2` into a readable label. Leading
/// whitespace is left alone — it is the nesting.
fn collapse_whitespace(label: &str) -> String {
    static RUN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\S)\s+").expect("the whitespace pattern is valid"));
    RUN.replace_all(label, "$1 ").into_owned()
}

/// Expand a template's `$1` / `${1}` placeholders against the captures.
///
/// Cocoa and Rust disagree about where a group reference ends: Cocoa's
/// `replacementString` reads `$1struct` as group 1 followed by the literal
/// `struct`, while Rust's `Captures::expand` reads the name as `1struct`,
/// finds no such group, and drops it. Rewriting the template to the braced
/// form first makes Rust agree with the syntax files, which were written for
/// Cocoa — `resources/syntaxes/Rust.cotsyntax` relies on it.
fn expand(captures: &Captures<'_>, template: &str) -> String {
    if template.is_empty() {
        return captures.get(0).map(|m| m.as_str().to_string()).unwrap_or_default();
    }
    let mut out = String::new();
    captures.expand(&brace_group_references(template), &mut out);
    out
}

/// Rewrite `$1struct` as `${1}struct`.
///
/// A hand-written scan rather than a regex: the rewrite needs to look at the
/// character *after* the group number, and Rust's regex crate has no
/// look-ahead.
fn brace_group_references(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(dollar) = rest.find('$') {
        out.push_str(&rest[..dollar]);
        rest = &rest[dollar + 1..];

        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 {
            out.push('$');
            continue;
        }

        let (number, tail) = rest.split_at(digits);
        let ambiguous = tail.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
        if ambiguous {
            out.push_str("${");
            out.push_str(number);
            out.push('}');
        } else {
            out.push('$');
            out.push_str(number);
        }
        rest = tail;
    }

    out.push_str(rest);
    out
}

/// Problems CotEditor would flag in its Warnings inspector.
///
/// The set mirrors the checks that matter for a plain-text editor: inconsistent
/// line endings, a mixed encoding that produced replacement characters, and
/// trailing whitespace, which CotEditor treats as worth surfacing because it
/// is invisible.
pub fn document_warnings(
    document: &crate::workspace::OpenDocument,
    cx: &mut gpui_kit::App,
) -> Vec<String> {
    let text = document.editor.read(cx).value().to_string();
    let mut warnings = Vec::new();

    // Mixed line endings.
    let mut endings: Vec<&'static str> = Vec::new();
    for ending in subtext_core::line_ending::ALL {
        if text.matches(ending.as_str()).count() > 0 && ending != subtext_core::LineEnding::CRLF {
            endings.push(ending.abbreviation());
        }
    }
    let has_crlf = text.matches("\r\n").count() > 0;
    if has_crlf {
        endings.insert(0, "CRLF");
    }
    if endings.len() > 1 {
        warnings.push(format!("Inconsistent line endings: {}", endings.join(", ")));
    }

    // Decoding loss.
    if text.contains(char::REPLACEMENT_CHARACTER) {
        warnings.push(
            "The text contains U+FFFD, which usually means the file was decoded with the wrong encoding"
                .to_string(),
        );
    }

    // Trailing whitespace.
    let trailing = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.len() != line.trim_end().len())
        .map(|(index, _)| index + 1)
        .collect::<Vec<_>>();
    if !trailing.is_empty() {
        warnings.push(format!(
            "Trailing whitespace on {} line(s), starting at line {}",
            trailing.len(),
            trailing[0]
        ));
    }

    // A byte-order mark that the encoding cannot carry.
    if document.document.bom() != subtext_core::Bom::None
        && !document.document.encoding().supports_bom()
    {
        warnings.push(format!(
            "A byte-order mark is set but {} cannot carry one",
            document.document.encoding().name
        ));
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use subtext_syntax::SyntaxManager;

    fn syntax(name: &str) -> subtext_syntax::Syntax {
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/syntaxes");
        SyntaxManager::load_from_directory(resources)
            .unwrap()
            .by_name(name)
            .unwrap()
            .clone()
    }

    fn rule(kind: &str, pattern: &str, template: &str) -> OutlineItem {
        OutlineItem {
            description: None,
            kind: kind.into(),
            pattern: pattern.into(),
            template: template.into(),
            ignore_case: false,
        }
    }

    fn titles(entries: &[OutlineEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.title.as_str()).collect()
    }

    #[test]
    fn rust_outline_finds_functions_and_structs() {
        let text = "fn main() {\n}\n\nstruct Point {\n    x: f64,\n}\n";
        let entries = extract(text, &syntax("Rust").outlines);
        let found = titles(&entries);
        assert!(found.contains(&"main"), "got {found:?}");
        assert!(found.contains(&"struct Point"), "got {found:?}");
    }

    #[test]
    fn anchors_bind_to_line_starts() {
        // The bundled patterns all begin with `^`, so without `(?m)` only the
        // first line would ever be considered.
        let rules = [rule("function", r"^fn (\w+)", "$1")];
        let entries = extract("fn first() {}\nfn second() {}\n", &rules);
        assert_eq!(titles(&entries), ["first", "second"]);
    }

    #[test]
    fn a_group_reference_may_run_into_a_word() {
        // `$1struct $2` is Cocoa's spelling; Rust would read the name as
        // `1struct` and drop it, which is what `brace_group_references` is for.
        let rules = [rule("container", r"^(\s*)struct (\w+)", "$1struct $2")];
        let entries = extract("struct Point {\n", &rules);
        assert_eq!(titles(&entries), ["struct Point"]);
    }

    #[test]
    fn nesting_comes_from_the_captured_whitespace() {
        let rules = [rule("function", r"^(\s*)fn (\w+)", "$1$2")];
        let entries = extract("fn top() {}\n    fn nested() {}\n", &rules);
        assert_eq!(titles(&entries), ["top", "nested"]);
        assert_eq!(entries[0].level, 0);
        assert_eq!(entries[1].level, 4);
    }

    #[test]
    fn a_heading_kind_carries_its_own_level() {
        let rules = [rule("heading.3", r"^### (.+)$", "$1")];
        let entries = extract("### Deep\n", &rules);
        assert_eq!(entries[0].level, 3);
        assert_eq!(entries[0].title, "Deep");
    }

    #[test]
    fn a_separator_is_recorded_rather_than_labelled() {
        let rules = [rule("separator", r"^---$", "")];
        let entries = extract("---\nkey: 1\n", &rules);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].separator);
        assert!(entries[0].title.is_empty());
    }

    #[test]
    fn runs_of_whitespace_are_squeezed() {
        let rules = [rule("value", r"^(\w+)\s*=\s*(.+)$", "$1 = $2")];
        let entries = extract("name    =    value\n", &rules);
        assert_eq!(titles(&entries), ["name = value"]);
    }

    #[test]
    fn an_empty_template_uses_the_whole_match() {
        let rules = [rule("title", r"^SUMMARY:(.*)$", "")];
        let entries = extract("SUMMARY:Report\n", &rules);
        assert_eq!(titles(&entries), ["SUMMARY:Report"]);
    }

    #[test]
    fn entries_are_ordered_by_line() {
        let text = "fn b() {}\nstruct A;\n";
        let entries = extract(text, &syntax("Rust").outlines);
        let lines: Vec<_> = entries.iter().map(|entry| entry.line).collect();
        let mut sorted = lines.clone();
        sorted.sort_unstable();
        assert_eq!(lines, sorted);
        assert_eq!(lines, [1, 2]);
    }

    #[test]
    fn no_rules_yields_no_entries() {
        assert!(extract("anything", &[]).is_empty());
    }

    #[test]
    fn a_broken_pattern_is_skipped() {
        let rules = [rule("heading", "([", "$1"), rule("heading", r"^# (.+)$", "$1")];
        let entries = extract("# Title\n", &rules);
        assert_eq!(titles(&entries), ["Title"]);
    }
}
