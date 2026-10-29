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
//! All twenty-one formatters are ported: Go's receivers and type parameters,
//! Ruby's receivers, Scala's parameter names, Swift's `name(label:)` signatures
//! and `MARK` rows, C#'s destructors and explicit interfaces, TypeScript's
//! namespaces. What is missing is not a formatter but a grammar — SQL's query
//! needs node types the published grammar does not have, and LaTeX's grammar
//! does not link — see the gap table in `docs/PLAN.md`.

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

/// Why a tree-sitter query could not be run.
///
/// Shared by the outline and the highlighter: both hand a query to a grammar,
/// and both can be handed a query that grammar no longer understands.
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
            Self::Query(error) => write!(formatter, "query: {error}"),
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

        if primary.kind == "separator" {
            items.push(TsItem {
                kind: Some("separator"),
                level: depth(primary.node),
                range: primary.range,
                title: String::new(),
            });
            continue;
        }

        let item = Item {
            syntax,
            kind: primary.kind,
            text,
            node: primary.node,
            capture: primary.range,
            whole: match_range(matched),
            parameters: captures_named(matched, names, "outline.signature.parameters")
                .into_iter()
                .zip(nodes_named(matched, names, "outline.signature.parameters"))
                .collect(),

            parameter_nodes: nodes_named(matched, names, "outline.signature.parameter"),
            name: captures_named(matched, names, "outline.name")
                .first()
                .cloned(),
        };
        let Some((title, range)) = format(&item) else {
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

/// Everything a formatter may need about one match.
///
/// CotEditor passes a `QueryMatch` and the source string around and each
/// formatter pulls what it wants out of them; this is the same set of facts,
/// collected once. `whole` is CotEditor's `QueryMatch.range` — the union of
/// every capture — which Scala and Swift use as the row's range, and the
/// `name` / `parameter_nodes` captures are Swift's, whose query spells a
/// signature as `begin`/`parameter`/`end` rather than as one parameter list.
struct Item<'a, 'tree> {
    syntax: &'a str,
    kind: &'static str,
    text: &'a str,
    node: Node<'tree>,
    capture: Range<usize>,
    whole: Range<usize>,
    /// Every `outline.signature.parameters` capture, as a range and as a node:
    /// the range is what most formatters paste in, the node is what Scala needs
    /// to read the parameter *names* out of.
    parameters: Vec<(Range<usize>, Node<'tree>)>,
    parameter_nodes: Vec<Node<'tree>>,
    name: Option<Range<usize>>,
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

/// The union of every capture's range, which is what CotEditor's
/// `QueryMatch.range` is.
fn match_range(matched: &tree_sitter::QueryMatch<'_, '_>) -> Range<usize> {
    matched
        .captures
        .iter()
        .map(|capture| capture.node.start_byte()..capture.node.end_byte())
        .reduce(|left, right| left.start.min(right.start)..left.end.max(right.end))
        .unwrap_or(0..0)
}

/// Every capture in a match with a given name, in query order.
fn captures_named(
    matched: &tree_sitter::QueryMatch<'_, '_>,
    names: &[&str],
    wanted: &str,
) -> Vec<Range<usize>> {
    matched
        .captures
        .iter()
        .filter(|capture| names.get(capture.index as usize) == Some(&wanted))
        .map(|capture| capture.node.start_byte()..capture.node.end_byte())
        .collect()
}

/// Every capture in a match with a given name, as nodes.
fn nodes_named<'tree>(
    matched: &tree_sitter::QueryMatch<'_, 'tree>,
    names: &[&str],
    wanted: &str,
) -> Vec<Node<'tree>> {
    matched
        .captures
        .iter()
        .filter(|capture| names.get(capture.index as usize) == Some(&wanted))
        .map(|capture| capture.node)
        .collect()
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
///
/// One branch per formatter CotEditor ships. The ones that only tidy their
/// capture — Markdown, CSS, HTML — are at the top; the ones that rebuild a
/// signature out of neighbouring nodes are below, in the order the language's
/// formatter file reads.
fn format(item: &Item<'_, '_>) -> Option<(String, Range<usize>)> {
    let text = item.text;
    let raw = text.get(item.capture.clone())?.to_string();
    let function = item.kind == "function";

    let (title, range) = match item.syntax {
        // `MarkdownOutlineFormatter.formatTitle`: the capture is the whole
        // heading line, `#` marks and all.
        "Markdown" => (strip_atx(&raw)?, item.capture.clone()),
        // `CSSOutlineFormatter.formatTitle`: a rule's capture runs to its
        // closing brace, so the title is the header above it.
        "CSS" => (css_header(&raw)?, item.capture.clone()),
        // `HTMLOutlineFormatter`: the item is an element, and its title is the
        // text inside it, not its markup.
        "HTML" => html_title(item.node, text)?,
        // `BashOutlineFormatter`: a shell function's name carries no
        // parentheses in the source, so the row adds them.
        "Bash" if function => {
            let range = bash_signature_range(text, item.capture.clone());
            (format!("{}()", &text[item.capture.clone()]), range)
        }
        // `COutlineFormatter` / `CPPOutlineFormatter`: the capture is the
        // declarator, so the name and parameter list have to be dug out of it.
        "C" | "C++" if function => {
            c_signature(item.node, text).unwrap_or((raw, item.capture.clone()))
        }
        // `PythonOutlineFormatter`: only a class-body assignment is an item.
        "Python" if item.kind == "value" => {
            if !python_is_class_variable(item.node) {
                return None;
            }
            (raw, item.capture.clone())
        }
        // `GoOutlineFormatter`: a method's row starts with its receiver —
        // `(*T).M(a int)` — and a generic one carries its type parameters.
        "Go" if function => go_signature(item),
        // `RubyOutlineFormatter`: `self.m(a)` keeps the receiver, and a
        // parameter list without parentheses gains them.
        "Ruby" if function => ruby_signature(item),
        // `ScalaOutlineFormatter`: the row is the parameter *names*, one clause
        // per capture — `f(a, b)` rather than `f(a: Int, b: String)`.
        "Scala" if function => scala_signature(item),
        // `CSharpOutlineFormatter`: a destructor keeps its `~`, an explicit
        // interface implementation keeps its prefix, and a generic method keeps
        // its type parameters.
        "C#" if function || item.kind == "value" => csharp_signature(item),
        // `SwiftOutlineFormatter`: the row is `name(label:label:)`, built from
        // the query's `signature` captures rather than from a parameter list.
        "Swift" if function => swift_signature(item),
        "Swift" if item.kind == "value" => match item.name.clone() {
            Some(name) => (text[name.clone()].to_string(), name),
            None => (raw, item.capture.clone()),
        },
        "Swift" if item.kind == "mark" => (swift_mark(&raw)?, item.capture.clone()),
        // `JavaScriptOutlineFormatter` / `TypeScriptOutlineFormatter`: a
        // function assigned to a variable is shown by the variable's name, not
        // by the arrow function's parameter list.
        "JavaScript" | "TypeScript" if function => {
            if item.node.parent().map(|node| node.kind()) == Some("variable_declarator") {
                return Some((raw, item.capture.clone()));
            }
            if item.syntax == "TypeScript" {
                typescript_signature(item)
            } else {
                with_parameters(item)
            }
        }
        // `TypeScriptOutlineFormatter.moduleTitle`: a `module`/`namespace` is
        // shown by its name, not by its whole body.
        "TypeScript" if item.kind == "container" => match module_title(item) {
            Some(title) => title,
            None => (raw, item.capture.clone()),
        },
        // Only a function row carries a parameter list. A container or a value
        // that happened to sit in a pattern with a `parameters` capture must
        // not grow one.
        _ if function => with_parameters(item),
        _ => (raw, item.capture.clone()),
    };

    // Each branch has already normalized its own title the way its formatter
    // does — Markdown only trims, CSS and HTML collapse. Collapsing again here
    // would quietly rewrite a heading's own spacing, which upstream keeps.
    (!title.is_empty()).then_some((title, range))
}

/// A name followed by its parameter list.
///
/// CotEditor's `functionTitle` for Java, Kotlin, Lua, PHP and the rest: the
/// name, then the captured parameter list with its whitespace tidied, or `()`
/// when the match captured none — a function without parentheses reads as a
/// variable otherwise.
fn with_parameters(item: &Item<'_, '_>) -> (String, Range<usize>) {
    let text = item.text;
    let name = &text[item.capture.clone()];
    let Some((parameters, _)) = item.parameters.first().cloned() else {
        return (format!("{name}()"), item.capture.clone());
    };
    // A capture that already contains the parameter list — C's declarator, for
    // one — must not have it appended twice.
    if parameters.start >= item.capture.start && parameters.end <= item.capture.end {
        return (name.to_string(), item.capture.clone());
    }
    let range = item.capture.start..item.capture.end.max(parameters.end);
    (format!("{name}{}", tidy(&text[parameters])), range)
}

/// `m<T>(a: T)` — TypeScript normalizes the whole signature — name, type
/// parameters and parameters — rather than appending a clause to the name.
fn typescript_signature(item: &Item<'_, '_>) -> (String, Range<usize>) {
    let text = item.text;
    let type_parameters = item
        .node
        .parent()
        .and_then(|declaration| declaration.child_by_field_name("type_parameters"))
        .map(|node| node.start_byte()..node.end_byte());
    let parameters = item.parameters.first().map(|(range, _)| range.clone());

    let mut start = item.capture.start;
    let mut end = item.capture.end;
    for range in [type_parameters, parameters].into_iter().flatten() {
        start = start.min(range.start);
        end = end.max(range.end);
    }
    (tidy_signature(&text[start..end]), start..end)
}

/// `(*T).M(a int)` — the receiver, the name, the type parameters, the
/// parameters, which is CotEditor's Go formatter in one line.
fn go_signature(item: &Item<'_, '_>) -> (String, Range<usize>) {
    let text = item.text;
    let name = &text[item.capture.clone()];
    let declaration = item.node.parent();
    let type_parameters = declaration
        .and_then(|declaration| declaration.child_by_field_name("type_parameters"))
        .map(|node| node.start_byte()..node.end_byte());
    let parameters = item.parameters.first().map(|(range, _)| range.clone());
    let receiver = go_receiver(declaration, text);

    let mut title = String::new();
    if let Some(receiver) = receiver {
        // A pointer receiver keeps its parentheses: `(*T).M` reads as a type,
        // `*T.M` reads as a dereference.
        if receiver.starts_with('*') {
            title.push('(');
            title.push_str(&receiver);
            title.push_str(").");
        } else {
            title.push_str(&receiver);
            title.push('.');
        }
    }
    title.push_str(name);
    if let Some(range) = type_parameters.clone() {
        title.push_str(&tidy(&text[range]));
    }
    match parameters.clone() {
        Some(range) => title.push_str(&tidy(&text[range])),
        None => title.push_str("()"),
    }

    let mut start = item.capture.start;
    let mut end = item.capture.end;
    for range in [type_parameters, parameters].into_iter().flatten() {
        start = start.min(range.start);
        end = end.max(range.end);
    }
    (title, start..end)
}

/// The type a Go method is declared on, from `func (t *T) M()`.
fn go_receiver(declaration: Option<Node<'_>>, text: &str) -> Option<String> {
    let declaration = declaration.filter(|node| node.kind() == "method_declaration")?;
    let receiver = declaration.child_by_field_name("receiver")?;
    let parameter = receiver.named_child(0)?;
    let ty = parameter.child_by_field_name("type")?;
    Some(collapse(&text[ty.start_byte()..ty.end_byte()]))
}

/// `self.m(a)` — Ruby keeps the receiver when the method has one, and wraps a
/// bare parameter list in the parentheses Ruby lets you leave out.
fn ruby_signature(item: &Item<'_, '_>) -> (String, Range<usize>) {
    let text = item.text;
    let name = &text[item.capture.clone()];
    let declaration = item.node.parent();
    let receiver = declaration
        .and_then(|node| node.child_by_field_name("object"))
        .map(|node| node.start_byte()..node.end_byte());
    let parameters = declaration
        .and_then(|node| node.child_by_field_name("parameters"))
        .map(|node| node.start_byte()..node.end_byte());

    let mut title = String::new();
    if let Some(range) = receiver.clone() {
        title.push_str(&text[range]);
        title.push('.');
    }
    title.push_str(name);
    title.push_str(&ruby_parameters(
        parameters.clone().map(|range| &text[range]),
    ));

    let mut start = item.capture.start;
    let mut end = item.capture.end;
    for range in [receiver, parameters].into_iter().flatten() {
        start = start.min(range.start);
        end = end.max(range.end);
    }
    (title, start..end)
}

/// A Ruby parameter list, with the parentheses it may have been written
/// without.
fn ruby_parameters(parameters: Option<&str>) -> String {
    let Some(parameters) = parameters else {
        return "()".to_string();
    };
    let normalized = tidy(parameters);
    if normalized.is_empty() {
        return "()".to_string();
    }
    if normalized.starts_with('(') {
        return normalized;
    }
    format!("({normalized})")
}

/// `f(a, b)` — Scala shows parameter *names*, one clause per capture, and keeps
/// `using` / `implicit` when the clause had one.
fn scala_signature(item: &Item<'_, '_>) -> (String, Range<usize>) {
    let text = item.text;
    let mut title = text[item.capture.clone()].to_string();
    for (range, node) in &item.parameters {
        title.push_str(&scala_clause(&text[range.clone()], *node, text));
    }
    // Scala takes the whole match as the row's range, so clicking it lands on
    // the signature rather than on the name alone.
    (title, item.whole.clone())
}

fn scala_clause(parameters: &str, node: Node<'_>, text: &str) -> String {
    let clause = tidy(parameters);
    let names = parameter_names(node, text);
    if names.is_empty() {
        return clause;
    }
    let joined = names.join(", ");
    if clause.starts_with("(using ") {
        return format!("(using {joined})");
    }
    if clause.starts_with("(implicit ") {
        return format!("(implicit {joined})");
    }
    format!("({joined})")
}

/// The names of the parameters in a parameter clause.
///
/// Walked rather than parsed: `(using c: C)` has one name and a modifier, and
/// splitting the text on `:` would hand back `using c`.
fn parameter_names<'tree>(clause: Node<'tree>, text: &str) -> Vec<String> {
    (0..clause.named_child_count() as u32)
        .filter_map(|index| clause.named_child(index))
        .filter(|child| child.kind() == "parameter")
        .filter_map(|parameter| parameter.child_by_field_name("name"))
        .map(|name| text[name.start_byte()..name.end_byte()].to_string())
        .collect()
}

/// `~A()` / `IFoo.M<T>(int a)` — the C# signature, which is the *text* between
/// the parts rather than a concatenation of them.
fn csharp_signature(item: &Item<'_, '_>) -> (String, Range<usize>) {
    let text = item.text;
    let declaration = item.node.parent();
    let explicit_interface = declaration.and_then(|node| {
        (0..node.named_child_count() as u32)
            .filter_map(|index| node.named_child(index))
            .find(|child| child.kind() == "explicit_interface_specifier")
            .map(|child| child.start_byte()..child.end_byte())
    });
    let type_parameters = declaration
        .and_then(|node| node.child_by_field_name("type_parameters"))
        .map(|node| node.start_byte()..node.end_byte());

    // A destructor's `~` sits before the name, and the capture starts at the
    // name.
    let mut name = item.capture.clone();
    if declaration.map(|node| node.kind()) == Some("destructor_declaration")
        && name.start > 0
        && text.get(name.start - 1..name.start) == Some("~")
    {
        name.start -= 1;
    }

    let parameters = if item.kind == "value" {
        None
    } else {
        item.parameters.first().map(|(range, _)| range.clone())
    };

    let mut start = name.start;
    let mut end = name.end;
    for range in [explicit_interface, type_parameters, parameters]
        .into_iter()
        .flatten()
    {
        start = start.min(range.start);
        end = end.max(range.end);
    }
    (tidy_signature(&text[start..end]), start..end)
}

/// `m(a:)` — Swift's signature is the parameter *labels*, not the parameter
/// list, and `init` keeps its `!`/`?`.
fn swift_signature(item: &Item<'_, '_>) -> (String, Range<usize>) {
    let text = item.text;
    let name = &text[item.capture.clone()];
    let range = item.whole.clone();

    if name == "deinit" {
        return (name.to_string(), range);
    }

    let signature = &text[range.clone()];
    let base = if name == "init" {
        initializer_name(signature).unwrap_or_else(|| name.to_string())
    } else {
        name.to_string()
    };

    let labels: Vec<String> = item
        .parameter_nodes
        .iter()
        .filter_map(|parameter| {
            parameter
                .child_by_field_name("external_name")
                .or_else(|| parameter.child_by_field_name("name"))
                .map(|label| text[label.start_byte()..label.end_byte()].to_string())
        })
        .collect();

    if labels.is_empty() {
        return (format!("{base}()"), range);
    }
    let labels: Vec<String> = labels.iter().map(|label| format!("{label}:")).collect();
    (format!("{base}({})", labels.join("")), range)
}

/// The `init`/`init!`/`init?` a Swift initializer starts with.
fn initializer_name(signature: &str) -> Option<String> {
    let trimmed = signature.trim_start();
    let rest = trimmed.strip_prefix("init")?;
    let suffix = rest.chars().next().filter(|c| *c == '!' || *c == '?');
    Some(match suffix {
        Some(suffix) => format!("init{suffix}"),
        None => "init".to_string(),
    })
}

/// `MARK: - Section` becomes `Section`; a comment that says nothing else is not
/// a row.
fn swift_mark(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let inner = if let Some(rest) = trimmed.strip_prefix("//") {
        rest.trim()
    } else if let Some(rest) = trimmed.strip_prefix("/*") {
        rest.trim().trim_end_matches("*/").trim()
    } else {
        trimmed
    };
    let inner = inner.strip_prefix("MARK:").map(str::trim).unwrap_or(inner);
    let inner = inner.trim_start_matches('-').trim();
    (!inner.is_empty()).then(|| inner.to_string())
}

/// A TypeScript `module`/`namespace` by its name.
fn module_title(item: &Item<'_, '_>) -> Option<(String, Range<usize>)> {
    if !matches!(item.node.kind(), "internal_module" | "module") {
        return None;
    }
    let name = item.node.child_by_field_name("name")?;
    Some((
        item.text[name.start_byte()..name.end_byte()].to_string(),
        name.start_byte()..name.end_byte(),
    ))
}

/// Collapse runs of whitespace and tidy the punctuation around it.
///
/// CotEditor's `normalizedClause`, which every function formatter ends with:
/// `( a ,  b )` is `(a, b)`.
fn tidy(clause: &str) -> String {
    let collapsed = collapse(clause);
    let mut out = String::with_capacity(collapsed.len());
    let mut characters = collapsed.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            // Nothing sits just inside an opening bracket.
            '(' | '[' => {
                out.push(character);
                while characters.peek() == Some(&' ') {
                    characters.next();
                }
            }
            // …or just before a closing one.
            ')' | ']' => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push(character);
            }
            // A comma takes exactly one space after it.
            ',' => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push(',');
                while characters.peek() == Some(&' ') {
                    characters.next();
                }
                out.push(' ');
            }
            _ => out.push(character),
        }
    }
    out.trim().to_string()
}

/// [`tidy`], plus the angle brackets and dots a C# or TypeScript signature
/// puts next to each other.
fn tidy_signature(signature: &str) -> String {
    let tidied = tidy(signature);
    let mut out = String::with_capacity(tidied.len());
    let mut characters = tidied.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            // A signature has no space between a name and its argument list.
            '(' => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push('(');
                while characters.peek() == Some(&' ') {
                    characters.next();
                }
            }
            '<' => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push('<');
                while characters.peek() == Some(&' ') {
                    characters.next();
                }
            }
            '>' => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push('>');
            }
            '.' => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push('.');
                while characters.peek() == Some(&' ') {
                    characters.next();
                }
            }
            _ => out.push(character),
        }
    }
    out.trim().to_string()
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

    #[test]
    fn a_go_method_keeps_its_receiver_and_a_generic_function_its_type_parameters() {
        assert_eq!(
            items(
                "Go",
                "package p\n\nfunc (t *T) M(a int) {}\n\nfunc F[T any](v T) {}\n"
            ),
            [
                ("function", 3, "(*T).M(a int)".to_string()),
                ("function", 3, "F[T any](v T)".to_string()),
            ]
        );
    }

    #[test]
    fn a_ruby_method_keeps_its_receiver_and_gains_its_parentheses() {
        assert_eq!(
            items(
                "Ruby",
                "class A\n  def self.m(a, b)\n  end\n\n  def n\n  end\nend\n"
            ),
            [
                ("container", 3, "A".to_string()),
                ("function", 5, "self.m(a, b)".to_string()),
                // `def n` has no parentheses in the source; the row adds them,
                // or the entry reads like a variable.
                ("function", 5, "n()".to_string()),
            ]
        );
    }

    #[test]
    fn a_scala_method_shows_parameter_names_and_keeps_using() {
        assert_eq!(
            items(
                "Scala",
                "object O {\n  def f(a: Int, b: String) = a\n  def g(using c: C) = 1\n}\n"
            ),
            [
                ("container", 3, "O".to_string()),
                ("function", 5, "f(a, b)".to_string()),
                // The `using` modifier is not a parameter, so it is not a name.
                ("function", 5, "g(using c)".to_string()),
            ]
        );
    }

    #[test]
    fn a_swift_method_shows_its_argument_labels() {
        assert_eq!(
            items(
                "Swift",
                "struct S {\n  func m(a: Int, b: String) {}\n  init?(x: Int) {}\n  deinit {}\n  var p: Int\n}\n"
            ),
            [
                ("container", 3, "S".to_string()),
                ("function", 5, "m(a:b:)".to_string()),
                // An initializer keeps its failable marker.
                ("function", 5, "init?(x:)".to_string()),
                ("function", 5, "deinit".to_string()),
                // A property is its name, from the query's `outline.name`.
                ("value", 5, "p".to_string()),
            ]
        );
    }

    #[test]
    fn a_swift_mark_becomes_its_text_and_also_a_separator() {
        // Two patterns match a `MARK:` comment — one calls it a separator, one
        // calls it a mark — and CotEditor keeps both, so the outline shows a
        // hairline above the label. That is the shipped query's doing, not this
        // module's.
        assert_eq!(
            items("Swift", "// MARK: - Section\n\nfunc f() {}\n"),
            [
                ("separator", 2, String::new()),
                ("mark", 2, "Section".to_string()),
                ("function", 3, "f()".to_string()),
            ]
        );
        // A comment that says nothing after the colon is not a row.
        assert_eq!(
            items("Swift", "// MARK: -\n"),
            [("separator", 2, String::new())]
        );
    }

    #[test]
    fn a_csharp_signature_keeps_its_tilde_interface_and_type_parameters() {
        assert_eq!(
            items(
                "C#",
                "class A {\n  void M(int a) {}\n  ~A() {}\n  void IFoo.M() {}\n  void G<T>(T a) {}\n}\n"
            ),
            [
                ("container", 3, "A".to_string()),
                ("function", 5, "M(int a)".to_string()),
                // The `~` sits before the name, and the query captures the name.
                ("function", 5, "~A()".to_string()),
                ("function", 5, "IFoo.M()".to_string()),
                ("function", 5, "G<T>(T a)".to_string()),
            ]
        );
    }

    #[test]
    fn a_typescript_signature_includes_its_type_parameters() {
        assert_eq!(
            items(
                "TypeScript",
                "namespace N {\n}\nclass A {\n  m<T>(a: T): void {}\n}\nconst g = (x) => x;\n"
            ),
            [
                // A namespace is its name, not its body.
                ("container", 3, "N".to_string()),
                ("container", 3, "A".to_string()),
                ("function", 5, "m<T>(a: T)".to_string()),
                // A function assigned to a variable is the variable's row.
                ("function", 4, "g".to_string()),
            ]
        );
    }

    #[test]
    fn the_plain_function_languages_append_the_parameter_list() {
        let cases: [(&str, &str, &[&str]); 4] = [
            (
                "Java",
                "class A {\n  void m(int a) {}\n  void n() {}\n}\n",
                &["m(int a)", "n()"],
            ),
            (
                "Kotlin",
                "class A {\n  fun m(a: Int) {}\n}\n",
                &["m(a: Int)"],
            ),
            ("Lua", "function f(a)\nend\n", &["f(a)"]),
            (
                "PHP",
                "<?php\nclass A {\n  function m($a) {}\n}\n",
                &["m($a)"],
            ),
        ];
        for (syntax, text, expected) in cases {
            let titles: Vec<String> = items(syntax, text)
                .into_iter()
                .filter(|(kind, _, _)| *kind == "function")
                .map(|(_, _, title)| title)
                .collect();
            assert_eq!(titles, expected, "{syntax}");
        }
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
