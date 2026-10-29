//! Tree-sitter outlines.
//!
//! Twenty-two of CotEditor's seventy syntaxes take their outline from a
//! tree-sitter query — `queries/<Name>/outline.scm` — and their `.cotsyntax`
//! bundles ship no `Regex/Outlines.json` at all. Markdown, Python, JavaScript,
//! TypeScript, Rust, HTML, CSS, C, C++, Java, Go, Ruby, PHP, Swift, Kotlin,
//! Scala, Lua, Bash, SQL, Makefile and C# are that set, which is why a Markdown
//! document's outline panel was empty until this module existed.
//!
//! The work is split the way CotEditor splits it: the *query* decides which
//! nodes are items and how deep they sit, and a per-language *formatter*
//! decides what the row says. The second half is not cosmetic — `main` and
//! `main(int argc, char **argv)` are the same item — so the formatters here are
//! ports of CotEditor's, not approximations.
//!
//! What is *not* ported yet is listed in `docs/PLAN.md` §20: Go and Ruby
//! receivers, Go/Java/C# type parameters, C#'s destructor and explicit-interface
//! prefixes, SQL's statement headers, and LaTeX (whose grammar this build does
//! not carry).

use std::fmt;
use std::ops::Range;

use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator};

/// An outline item a query produced, before it becomes an outline entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsItem {
    /// The kind token from the capture name: `container`, `value`, `function`,
    /// `title`, `heading`, `mark` or `separator`.
    pub kind: Option<&'static str>,
    /// The raw depth. A `heading` carries the level its capture names;
    /// everything else carries its depth in the tree, which is what makes a
    /// method nest under its class.
    pub level: usize,
    pub range: Range<usize>,
    pub title: String,
}

/// Why a query could not be run.
#[derive(Debug)]
pub enum Error {
    /// The grammar and the parser disagree about a language version.
    Language(tree_sitter::LanguageError),
    /// The query does not parse against the grammar — which is what a grammar
    /// upgraded past its query looks like.
    Query(tree_sitter::QueryError),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Language(error) => write!(formatter, "tree-sitter language: {error}"),
            Self::Query(error) => write!(formatter, "outline query: {error}"),
        }
    }
}

impl std::error::Error for Error {}

/// The grammar for a syntax, by the name CotEditor gives it.
///
/// `None` for the syntaxes whose outline is a regex rule — and for LaTeX, whose
/// outline is a tree-sitter query too but whose grammar this build does not
/// carry.
pub fn language(syntax: &str) -> Option<Language> {
    Some(match syntax {
        "Bash" => tree_sitter_bash::LANGUAGE.into(),
        "C" => tree_sitter_c::LANGUAGE.into(),
        "C#" => tree_sitter_c_sharp::LANGUAGE.into(),
        "C++" => tree_sitter_cpp::LANGUAGE.into(),
        "CSS" => tree_sitter_css::LANGUAGE.into(),
        "Go" => tree_sitter_go::LANGUAGE.into(),
        "HTML" => tree_sitter_html::LANGUAGE.into(),
        "Java" => tree_sitter_java::LANGUAGE.into(),
        "JavaScript" => tree_sitter_javascript::LANGUAGE.into(),
        "Kotlin" => tree_sitter_kotlin_sg::LANGUAGE.into(),
        "Lua" => tree_sitter_lua::LANGUAGE.into(),
        "Makefile" => tree_sitter_make::LANGUAGE.into(),
        "Markdown" => tree_sitter_md::LANGUAGE.into(),
        "PHP" => tree_sitter_php::LANGUAGE_PHP.into(),
        "Python" => tree_sitter_python::LANGUAGE.into(),
        "Ruby" => tree_sitter_ruby::LANGUAGE.into(),
        "Rust" => tree_sitter_rust::LANGUAGE.into(),
        "Scala" => tree_sitter_scala::LANGUAGE.into(),
        "SQL" => tree_sitter_sequel::LANGUAGE.into(),
        "Swift" => tree_sitter_swift::LANGUAGE.into(),
        "TypeScript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        _ => return None,
    })
}

/// Whether this build can give `syntax` a tree-sitter outline at all.
pub fn supports(syntax: &str) -> bool {
    language(syntax).is_some()
}

/// Run `query_source` over `text` and return the items it describes.
///
/// The whole buffer is parsed on every call. CotEditor parses incrementally
/// because it also highlights from the same tree; SubText only needs the
/// outline, so there is no tree to keep between edits. The cost is real but
/// bounded: a 234 KB Markdown document with 4,000 headings takes about 75 ms in
/// a release build (31 ms of it the parse, 5 ms the query scan, the rest this
/// module's own per-item work), and a document a tenth that size is a tenth of
/// that. An incremental layer would be state to keep correct for a gain nobody
/// would see on the files this editor is for.
pub fn extract(syntax: &str, text: &str, query_source: &str) -> Result<Vec<TsItem>, Error> {
    let Some(language) = language(syntax) else {
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
    let mut items = Vec::new();
    let mut matches = cursor.matches(&query, tree.root_node(), text.as_bytes());

    while let Some(matched) = matches.next() {
        let Some(primary) = primary_capture(matched, names) else {
            continue;
        };
        let parameters = parameters_range(matched, names);

        if primary.kind == "separator" {
            items.push(TsItem {
                kind: Some("separator"),
                level: depth(primary.node),
                range: primary.range,
                title: String::new(),
            });
            continue;
        }

        let Some((title, range)) = format(
            syntax,
            primary.kind,
            primary.node,
            text,
            primary.range,
            parameters,
        ) else {
            continue;
        };
        if title.is_empty() {
            continue;
        }
        items.push(TsItem {
            kind: Some(primary.kind),
            level: match primary.level {
                Some(level) => level,
                None => depth(primary.node),
            },
            range,
            title,
        });
    }

    // A query's matches come out in the order the cursor walks the tree, which
    // is document order for a single pattern but not for a file's worth of
    // them. Sorting by position is what makes the outline read top to bottom;
    // the length is the tie-break the regex path uses, for two items that start
    // in the same place.
    items.sort_by_key(|item| (item.range.start, item.range.end - item.range.start));
    Ok(items)
}

/// The capture an item is built from.
struct Primary<'tree> {
    kind: &'static str,
    /// The heading level its capture name spelled, if it spelled one.
    level: Option<usize>,
    node: Node<'tree>,
    range: Range<usize>,
}

/// The first capture in a match whose name is an outline capture.
///
/// "First" is CotEditor's rule, and it matters: a match's other captures are
/// the pieces a formatter may use — `outline.signature.parameters` and friends
/// — and the item is the node the first *kind* capture named.
fn primary_capture<'tree>(
    matched: &tree_sitter::QueryMatch<'_, 'tree>,
    names: &[&str],
) -> Option<Primary<'tree>> {
    matched.captures.iter().find_map(|capture| {
        let name = names.get(capture.index as usize)?;
        let (kind, level) = kind_of(name)?;
        Some(Primary {
            kind,
            level,
            node: capture.node,
            range: capture.node.start_byte()..capture.node.end_byte(),
        })
    })
}

/// The first `outline.signature.parameters` capture in a match.
fn parameters_range(
    matched: &tree_sitter::QueryMatch<'_, '_>,
    names: &[&str],
) -> Option<Range<usize>> {
    matched.captures.iter().find_map(|capture| {
        (names.get(capture.index as usize)? == &"outline.signature.parameters")
            .then(|| capture.node.start_byte()..capture.node.end_byte())
    })
}

/// Read a capture name into a kind and, for a heading, its level.
///
/// The name is `outline.<kind>[.<level>]`. Anything else — `outline.signature.
/// parameters`, or a name a future query invents — is not an item, which is
/// what keeps the extra captures out of the outline.
fn kind_of(name: &str) -> Option<(&'static str, Option<usize>)> {
    let mut parts = name.split('.');
    if parts.next() != Some("outline") {
        return None;
    }
    let kind = match parts.next()? {
        "container" => "container",
        "value" => "value",
        "function" => "function",
        "title" => "title",
        "heading" => "heading",
        "mark" => "mark",
        "separator" => "separator",
        _ => return None,
    };
    // CotEditor's `Kind.levelRange` is 1...7, and a level outside it is not a
    // level — `heading.9` is not an outline kind.
    let level = parts
        .next()
        .and_then(|part| part.parse::<usize>().ok())
        .filter(|level| (1..=7).contains(level));
    Some((kind, level))
}

/// How deep a node sits, counting itself.
///
/// This is CotEditor's `OutlinePolicy.depth` with the default policy, whose
/// `ignoredDepthNodeTypes` is empty: every node in the chain counts. A
/// top-level function in Python therefore sits at 3 — module, definition, name
/// — and the level normalization that follows turns that into 0.
fn depth(node: Node<'_>) -> usize {
    let mut count = 1;
    let mut current = node.parent();
    while let Some(parent) = current {
        count += 1;
        current = parent.parent();
    }
    count
}

/// The display title and range for an item, per language.
fn format(
    syntax: &str,
    kind: &'static str,
    node: Node<'_>,
    text: &str,
    capture: Range<usize>,
    parameters: Option<Range<usize>>,
) -> Option<(String, Range<usize>)> {
    let raw = text.get(capture.clone())?.to_string();

    let (title, range) = match syntax {
        // CotEditor's `MarkdownOutlineFormatter.formatTitle`: the capture is
        // the whole heading line, `#` marks and all.
        "Markdown" => (strip_atx(&raw)?, capture),
        // `CSSOutlineFormatter.formatTitle`: a rule's capture runs to its
        // closing brace, so the title is the header above it.
        "CSS" => (css_header(&raw)?, capture),
        // `HTMLOutlineFormatter`: the item is an element, and its title is the
        // text inside it, not its markup.
        "HTML" => html_title(node, text)?,
        // `BashOutlineFormatter`: a shell function's name carries no
        // parentheses in the source, so the row adds them.
        "Bash" if kind == "function" => {
            let range = bash_signature_range(text, capture.clone());
            (format!("{}()", &text[capture]), range)
        }
        // `COutlineFormatter` / `CPPOutlineFormatter`: the capture is the
        // declarator, so the name and parameter list have to be dug out of it.
        "C" | "C++" if kind == "function" => match c_signature(node, text) {
            Some(signature) => signature,
            None => (raw, capture),
        },
        // `PythonOutlineFormatter`: only a class-body assignment is an item,
        // and a function's row carries its parameter list.
        "Python" if kind == "value" => {
            if !python_is_class_variable(node) {
                return None;
            }
            (raw, capture)
        }
        _ => with_parameters(text, capture.clone(), parameters),
    };

    // Each branch has already normalized its own title the way its formatter
    // does — Markdown only trims, CSS and HTML collapse. Collapsing again here
    // would quietly rewrite a heading's own spacing, which upstream keeps.
    (!title.is_empty()).then_some((title, range))
}

/// A name followed by its parameter list, when the match captured one.
///
/// This is the shape of most of CotEditor's formatters — Java, Kotlin, Scala,
/// Rust, JavaScript, TypeScript, PHP, Lua, Ruby, Go and Swift all do exactly
/// this for `.function`, and several of them add a receiver or a type parameter
/// clause on top. The additions are listed as gaps; the parameters are not a
/// gap, and leaving them out would make every function row look like a
/// variable.
fn with_parameters(
    text: &str,
    capture: Range<usize>,
    parameters: Option<Range<usize>>,
) -> (String, Range<usize>) {
    let Some(parameters) = parameters else {
        return (text[capture.clone()].to_string(), capture);
    };
    // A capture that already contains the parameter list — C's declarator, for
    // one — must not have it appended twice.
    if parameters.start >= capture.start && parameters.end <= capture.end {
        return (text[capture.clone()].to_string(), capture);
    }
    let title = format!(
        "{}{}",
        &text[capture.clone()],
        collapse(&text[parameters.clone()])
    );
    let range = capture.start..capture.end.max(parameters.end);
    (title, range)
}

/// Strip a Markdown heading down to its text.
///
/// `# One`, `## Two ##` and a setext heading all arrive as whole lines; the
/// first line is the title, without the ATX marks at either end. A heading with
/// nothing but marks is not an item — `formatTitle` returns nil for it.
fn strip_atx(raw: &str) -> Option<String> {
    let first = raw.split('\n').next().unwrap_or(raw);
    let trimmed = first.trim_start_matches('#');
    let trimmed = trimmed.trim_end_matches(['#', ' ']);
    Some(trimmed.trim().to_string())
}

/// The header of a CSS rule: everything before its block or its semicolon.
fn css_header(raw: &str) -> Option<String> {
    let end = raw.find('{').or_else(|| raw.find(';')).unwrap_or(raw.len());
    Some(collapse(&raw[..end]))
}

/// The text inside an HTML element, with its tags taken out.
fn html_title(node: Node<'_>, text: &str) -> Option<(String, Range<usize>)> {
    let ranges = text_ranges(node);
    let first = ranges.first()?;
    let last = ranges.last()?;
    let range = first.start..last.end;

    let mut title = String::new();
    let mut previous_end: Option<usize> = None;
    for piece in &ranges {
        if let Some(end) = previous_end {
            let gap = end..piece.start;
            // A line break between two text nodes is a space in the rendered
            // document; a tag between them is not. `contains_visible_whitespace`
            // is what tells the two apart — it looks *outside* the tags.
            if !gap.is_empty()
                && !title.ends_with(char::is_whitespace)
                && contains_visible_whitespace(&text[gap])
            {
                title.push(' ');
            }
        }
        title.push_str(&text[piece.clone()]);
        previous_end = Some(piece.end);
    }
    Some((collapse(&title), range))
}

/// The ranges of the text nodes inside an element, in document order.
fn text_ranges(node: Node<'_>) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    collect_text_ranges(node, &mut ranges);
    ranges
}

fn collect_text_ranges(node: Node<'_>, ranges: &mut Vec<Range<usize>>) {
    if matches!(node.kind(), "text" | "entity" | "raw_text") {
        ranges.push(node.start_byte()..node.end_byte());
        return;
    }
    for index in 0..node.child_count() as u32 {
        if let Some(child) = node.child(index) {
            collect_text_ranges(child, ranges);
        }
    }
}

/// Whether a run of markup contains whitespace a reader would see.
///
/// Everything inside a tag — attribute values included — is skipped, so the
/// gap between `</b>` and `<i>` is not a space, while a newline between two
/// tags is.
fn contains_visible_whitespace(markup: &str) -> bool {
    let mut inside_tag = false;
    let mut quote: Option<char> = None;
    for character in markup.chars() {
        if let Some(quoted) = quote {
            if character == quoted {
                quote = None;
            }
        } else if inside_tag {
            match character {
                '"' | '\'' => quote = Some(character),
                '>' => inside_tag = false,
                _ => {}
            }
        } else if character == '<' {
            inside_tag = true;
        } else if character.is_whitespace() {
            return true;
        }
    }
    false
}

/// A shell function's name plus the `()` that follows it, if one does.
///
/// The query captures the name alone, and the parentheses live in the source
/// a few characters later — at most 32, which is CotEditor's search window.
fn bash_signature_range(text: &str, name: Range<usize>) -> Range<usize> {
    let search_end = (name.end + 32).min(text.len());
    let tail = &text[name.end..search_end];
    let trimmed = tail.trim_start();
    if !trimmed.starts_with("()") {
        return name;
    }
    name.start..name.end + (tail.len() - trimmed.len()) + 2
}

/// The name and parameter list of a C or C++ function declarator.
fn c_signature(node: Node<'_>, text: &str) -> Option<(String, Range<usize>)> {
    let declarator = find_declarator(node, "function_declarator")?;
    let name = declarator
        .child_by_field_name("declarator")
        .and_then(|child| c_name(child))?;
    let parameters = declarator.child_by_field_name("parameters")?;
    let title = format!(
        "{}{}",
        &text[name.start_byte()..name.end_byte()],
        collapse(&text[parameters.start_byte()..parameters.end_byte()])
    );
    Some((title, name.start_byte()..parameters.end_byte()))
}

/// The nearest `function_declarator` at or below a node, following the
/// `declarator` field — which is how a pointer return type wraps one.
fn find_declarator<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    if node.kind() == kind {
        return Some(node);
    }
    node.child_by_field_name("declarator")
        .and_then(|child| find_declarator(child, kind))
}

/// The identifier a C declarator names.
fn c_name(node: Node<'_>) -> Option<Node<'_>> {
    if matches!(
        node.kind(),
        "identifier"
            | "field_identifier"
            | "qualified_identifier"
            | "operator_name"
            | "destructor_name"
            | "type_identifier"
            | "primitive_type"
    ) {
        return Some(node);
    }
    if let Some(child) = node.child_by_field_name("declarator")
        && let Some(found) = c_name(child)
    {
        return Some(found);
    }
    for index in 0..node.named_child_count() as u32 {
        if let Some(child) = node.named_child(index)
            && let Some(found) = c_name(child)
        {
            return Some(found);
        }
    }
    None
}

/// Whether an assignment sits directly in a class body.
///
/// CotEditor shows a Python class's attributes and not a module's variables,
/// and this is how it tells them apart: the assignment's enclosing block must
/// belong to a `class_definition`.
fn python_is_class_variable(node: Node<'_>) -> bool {
    let Some(assignment) = ancestor(node, "assignment") else {
        return false;
    };
    let Some(block) = ancestor(assignment, "block") else {
        return false;
    };
    block.parent().map(|parent| parent.kind()) == Some("class_definition")
}

/// The nearest node of a kind, at or above `node`.
fn ancestor<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut current = Some(node);
    while let Some(node) = current {
        if node.kind() == kind {
            return Some(node);
        }
        current = node.parent();
    }
    None
}

/// Collapse runs of whitespace, which is what every one of CotEditor's
/// formatters ends with.
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            in_space = true;
        } else {
            if in_space && !out.is_empty() {
                out.push(' ');
            }
            in_space = false;
            out.push(character);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bundled query, read from the resource tree.
    fn query(syntax: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../resources/queries")
            .join(syntax)
            .join("outline.scm");
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path:?}: {error}"))
    }

    /// The titles and kinds a syntax produces, in order.
    fn items(syntax: &str, text: &str) -> Vec<(&'static str, usize, String)> {
        extract(syntax, text, &query(syntax))
            .unwrap_or_else(|error| panic!("{syntax}: {error}"))
            .into_iter()
            .map(|item| (item.kind.unwrap_or("-"), item.level, item.title))
            .collect()
    }

    #[test]
    fn markdown_headings_come_out_without_their_marks() {
        let text = "# One\n\nbody\n\n## Two ##\n\n---\n\n### Three\n\nSetext\n======\n";
        assert_eq!(
            items("Markdown", text),
            [
                ("heading", 1, "One".to_string()),
                ("heading", 2, "Two".to_string()),
                // A separator carries no title — it is a hairline, not a row.
                ("separator", 4, String::new()),
                ("heading", 3, "Three".to_string()),
                // A setext heading arrives as two lines; the title is the first.
                ("heading", 1, "Setext".to_string()),
            ]
        );
    }

    #[test]
    fn a_python_class_shows_its_attributes_and_a_module_does_not() {
        let text = "class A:\n    x = 1\n\n    def m(self, a):\n        pass\n\ny = 2\n\ndef f(a, b=1):\n    pass\n";
        assert_eq!(
            items("Python", text),
            [
                ("container", 3, "A".to_string()),
                ("value", 6, "x".to_string()),
                ("function", 5, "m(self, a)".to_string()),
                // `y = 2` is a module variable and is not an item; `f` is.
                ("function", 3, "f(a, b=1)".to_string()),
            ]
        );
    }

    #[test]
    fn a_function_row_carries_its_parameter_list() {
        assert_eq!(
            items(
                "Rust",
                "impl S {\n    fn m(&self, a: u8) {}\n}\n\nfn top() {}\n"
            ),
            [
                ("container", 3, "S".to_string()),
                ("function", 5, "m(&self, a: u8)".to_string()),
                ("function", 3, "top()".to_string()),
            ]
        );
        assert_eq!(
            items(
                "TypeScript",
                "class A { m(a: number): void {} }\nfunction f(a: string) {}\n"
            ),
            [
                ("container", 3, "A".to_string()),
                ("function", 5, "m(a: number)".to_string()),
                ("function", 3, "f(a: string)".to_string()),
            ]
        );
    }

    #[test]
    fn a_c_function_shows_the_declarator_it_was_captured_as() {
        // The query captures the `function_declarator`, so the row is the name
        // and parameter list — not the return type, and not the body.
        assert_eq!(
            items(
                "C",
                "struct S { int a; };\n\nint main(int argc, char **argv) {\n  return 0;\n}\n"
            ),
            [
                ("container", 3, "S".to_string()),
                ("value", 5, "a".to_string()),
                ("function", 3, "main(int argc, char **argv)".to_string()),
            ]
        );
    }

    #[test]
    fn an_html_title_is_its_text_and_not_its_markup() {
        assert_eq!(
            items(
                "HTML",
                "<html><body><h1>Title <b>bold</b></h1><title>Doc</title></body></html>"
            ),
            [
                ("heading", 1, "Title bold".to_string()),
                ("title", 4, "Doc".to_string()),
            ]
        );
        // A title with nothing in it is not a row, and neither is one whose
        // text is only whitespace between tags.
        assert_eq!(
            items("HTML", "<h1><br></h1>"),
            Vec::<(&'static str, usize, String)>::new()
        );
    }

    #[test]
    fn css_outlines_are_at_rules_because_that_is_what_the_query_asks_for() {
        // CotEditor's CSS query captures `at_rule`, `media_statement` and
        // friends — a plain rule set is not an outline item there either.
        let text = "body {\n  color: red;\n}\n\n@media screen {\n  a { b: c }\n}\n";
        assert_eq!(
            items("CSS", text),
            [("container", 2, "@media screen".to_string()),]
        );
    }

    #[test]
    fn a_shell_function_gains_the_parentheses_its_name_lacks() {
        assert_eq!(
            items("Bash", "foo() {\n  echo hi\n}\n"),
            [("function", 3, "foo()".to_string())]
        );
    }

    /// Every bundled query has to parse against the grammar this build carries.
    ///
    /// This is the gate that keeps a grammar upgrade from silently emptying an
    /// outline: a node type that disappears makes `Query::new` fail, and a
    /// failing query is an empty panel with no message. The one exception is
    /// named rather than skipped, so the day a grammar catches up the test
    /// starts failing and the row goes away.
    #[test]
    fn every_bundled_query_parses_against_its_grammar() {
        // CotEditor builds SQL from `DerekStride/tree-sitter-sql`'s `gh-pages`
        // branch, which has node types (`drop_procedure`) that the published
        // `tree-sitter-sequel` 0.3.11 does not.
        const NOT_YET: [&str; 1] = ["SQL"];

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/queries");
        let mut checked = 0;
        for entry in std::fs::read_dir(&root).expect("queries/") {
            let directory = entry.expect("entry").path();
            let name = directory
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            if !directory.join("outline.scm").is_file() {
                continue;
            }
            let source = std::fs::read_to_string(directory.join("outline.scm")).expect("query");
            let result = extract(&name, "", &source);

            if NOT_YET.contains(&name.as_str()) {
                assert!(
                    result.is_err(),
                    "{name} parses now — drop it from NOT_YET and wire it up"
                );
                continue;
            }
            result.unwrap_or_else(|error| panic!("{name}: {error}"));
            checked += 1;
        }
        assert_eq!(checked, 21, "expected twenty-one parsable outline queries");
    }

    /// A syntax with no query and no regex rules is simply empty, and asking for
    /// one must not be an error.
    #[test]
    fn a_syntax_without_a_grammar_has_no_outline_and_no_complaint() {
        assert_eq!(extract("Plain Text", "text", "").expect("no grammar"), []);
        assert_eq!(
            extract("LaTeX", "\\section{One}", "").expect("no grammar"),
            []
        );
    }
}
