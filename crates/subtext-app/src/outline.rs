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
//!
//! # From a flat list to a tree
//!
//! The inspector shows the entries as a tree, and CotEditor's
//! `OutlineBuildNode.build(from:)` is what turns the flat list into one. Its
//! one subtlety is that an entry's indent has *two* forms — an explicit level,
//! or the whitespace the rule captured — and they behave differently. A
//! *level* entry nests under whatever is open above it; a *whitespace* entry
//! is pushed to the top level and **clears** the open levels, so the entries
//! after it start again from the root. [`build_tree`] is that algorithm and
//! [`Indent`] is the two-form type it needs; collapsing the two into one
//! number, as an earlier revision did, silently mis-nests every outline whose
//! rules capture whitespace — which is most of them.
//!
//! # What an entry knows
//!
//! An entry carries the byte range its rule matched, because that range is
//! what the entry *stands for* — the commands that step from one entry to the
//! next put the selection on it, and so does clicking a row. Nothing is kept
//! per entry that could be derived from it: there is no line, so which entry
//! the caret is in is decided by the range alone, and a rule that matches
//! mid-line puts the caret in a different entry than its line would.
//!
//! # What the filter does
//!
//! The filter field at the bottom of the pane is **not** a substring search.
//! CotEditor's `abbreviatedMatchedRanges(with:)` matches the filter as a
//! *subsequence* of the title, one character at a time and case-insensitively,
//! so `gt` finds "Guten Tag" and the matched characters are highlighted in
//! place. A node that does not match but has a descendant that does stays in
//! the tree, so the path down to a hit is visible.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::LazyLock;

use gpui_kit::assets::IconName;
use gpui_kit::{Hsla, hsla};
use regex::{Captures, Regex};
use subtext_syntax::OutlineItem;

/// What kind of thing an outline rule describes.
///
/// CotEditor's `Syntax.Outline.Kind`. The kind decides the row's symbol, its
/// colour and whether the label is drawn bold, and — for `title` and
/// `heading.N` — whether the entry carries a nesting level of its own.
///
/// `heading.N` is folded into [`OutlineKind::Heading`] because upstream does
/// the same: the level moves into [`Indent`] and the kind keeps only the fact
/// that it is a heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OutlineKind {
    Container,
    Value,
    Function,
    Title,
    Heading,
    Mark,
    Separator,
}

impl OutlineKind {
    /// The kind a rule's `kind` field names, or `None` when it names nothing
    /// SubText knows — three of the bundled rules have no kind at all.
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "container" => Self::Container,
            "value" => Self::Value,
            "function" => Self::Function,
            "title" => Self::Title,
            "mark" => Self::Mark,
            "separator" => Self::Separator,
            _ if name.starts_with("heading") => Self::Heading,
            _ => return None,
        })
    }

    /// The name the syntax files spell the kind with.
    pub fn name(self) -> &'static str {
        match self {
            Self::Container => "container",
            Self::Value => "value",
            Self::Function => "function",
            Self::Title => "title",
            Self::Heading => "heading",
            Self::Mark => "mark",
            Self::Separator => "separator",
        }
    }

    /// The accessible name of the kind, which is also its tooltip.
    pub fn label(self) -> &'static str {
        match self {
            Self::Container => "Container",
            Self::Value => "Value",
            Self::Function => "Function",
            Self::Title => "Title",
            Self::Heading => "Heading",
            Self::Mark => "Mark",
            Self::Separator => "Separator",
        }
    }

    /// Whether the label is drawn semibold. CotEditor gives `.container` and
    /// `.mark` a `Style` of `.bold` and leaves the rest plain.
    pub fn is_bold(self) -> bool {
        matches!(self, Self::Container | Self::Mark)
    }
}

/// How deeply an entry is nested.
///
/// CotEditor's `OutlineItem.Indent`, and the two cases are not
/// interchangeable — see the module docs. `title` rules and `heading.N` rules
/// produce [`Indent::Level`]; every other rule produces [`Indent::String`]
/// carrying the whitespace it captured.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Indent {
    /// An explicit nesting level. Also what a separator gets, which is what
    /// keeps it at the top level.
    Level(usize),
    /// The whitespace the rule captured. Drawn before the symbol, and it takes
    /// the entry out of the nesting.
    String(String),
}

impl Indent {
    /// The nesting level, or `None` when the indent is captured whitespace.
    pub fn level(&self) -> Option<usize> {
        match self {
            Self::Level(level) => Some(*level),
            Self::String(_) => None,
        }
    }
}

/// One entry in the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    /// Identity, stable across a re-parse that does not change the entry's
    /// kind or label. See [`assign_ids`].
    pub id: String,
    /// The label, with the rule's nesting whitespace removed. Empty for a
    /// separator.
    pub title: String,
    /// The byte range of the text the rule matched — what the entry *stands
    /// for*, as opposed to what it is called.
    ///
    /// CotEditor keeps the same thing (`OutlineItem.range`, an `NSRange` of
    /// the match) and it is what the commands that step from one entry to the
    /// next put the selection on, so a rule that matches mid-line selects
    /// mid-line. The offsets are bytes rather than UTF-16 units because that
    /// is what the editor's selections are; the two only differ past a
    /// non-BMP character, and both name the same place.
    pub range: Range<usize>,
    /// How the entry nests.
    pub indent: Indent,
    /// The rule's kind, or `None` for a rule that names none.
    pub kind: Option<OutlineKind>,
}

impl OutlineEntry {
    /// A `kind: separator` rule — a boundary, not a label.
    pub fn is_separator(&self) -> bool {
        self.kind == Some(OutlineKind::Separator)
    }
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
        let kind = OutlineKind::from_name(&rule.kind);

        for captures in regex.captures_iter(text) {
            let Some(matched) = captures.get(0) else {
                continue;
            };
            let range = matched.start()..matched.end();

            if kind == Some(OutlineKind::Separator) {
                entries.push(OutlineEntry {
                    id: String::new(),
                    title: String::new(),
                    range,
                    // `OutlineItem.separator` defaults to `.level(0)`, which is
                    // what makes a separator a root rather than a child.
                    indent: Indent::Level(0),
                    kind,
                });
                continue;
            }

            let label = collapse_whitespace(&expand(&captures, &rule.template));
            let (indent, title) = split_indent(&label);
            if title.is_empty() {
                continue;
            }

            entries.push(OutlineEntry {
                id: String::new(),
                title: title.to_string(),
                range,
                indent: indent_for(kind, &rule.kind, indent),
                kind,
            });
        }
    }

    entries.sort_by_key(sort_key);
    // Two rules landing on the same span with the same label are one entry.
    // Upstream has no dedup at all — `removingDuplicateIDs` only catches an
    // identity resolver that handed one id out twice, which it does not do —
    // so this is SubText's own rule, and keyed on the span rather than the
    // line so that it only ever merges what a reader would call one thing.
    entries.dedup_by(|a, b| a.range == b.range && a.title == b.title);
    assign_ids(&mut entries);
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

/// How an entry nests, from its kind and the whitespace its label carried.
///
/// CotEditor's `OutlineExtractor`:
///
/// ```swift
/// let indent: OutlineItem.Indent = switch self.kind {
///     case .title: .level(0)
///     case .heading(let level?): .level(level)
///     default: .string(String(match.indent))
/// }
/// ```
///
/// The `heading` case matters: a rule whose kind is a bare `heading`, with no
/// level, falls through to the whitespace case, and two of the bundled rules
/// are spelled that way.
fn indent_for(kind: Option<OutlineKind>, rule_kind: &str, indent: &str) -> Indent {
    match kind {
        Some(OutlineKind::Title) => Indent::Level(0),
        Some(OutlineKind::Heading) => rule_kind
            .strip_prefix("heading.")
            .and_then(|level| level.parse::<usize>().ok())
            .map_or_else(|| Indent::String(indent.to_string()), Indent::Level),
        _ => Indent::String(indent.to_string()),
    }
}

/// Where an entry sorts: by position, then by the match's length.
///
/// CotEditor sorts by `range.location` and then `range.length`, and now that
/// an entry carries its range, this is that key exactly. An earlier revision
/// sorted by line and then by nesting level, because a range was not kept —
/// the level stood in for the length. The two agree except for two rules that
/// match on the same line, where the length is the honest tie-break.
fn sort_key(entry: &OutlineEntry) -> (usize, usize) {
    (entry.range.start, entry.range.end - entry.range.start)
}

/// Give every entry a stable identity.
///
/// CotEditor keeps a `UUID` per item and reuses it across re-parses by matching
/// titles and kinds, so that the tree's expansion state survives the user
/// typing. SubText derives the identity from the same two things instead of
/// diffing UUIDs: an entry keeps its id for as long as its kind and label do,
/// and duplicates of the same kind and label are told apart by the order they
/// appear in. The separator between the parts is a character no label can
/// contain, so `("ab", "c")` and `("a", "bc")` cannot collide.
fn assign_ids(entries: &mut [OutlineEntry]) {
    const SEPARATOR: char = '\u{1}';
    let mut seen: HashMap<String, usize> = HashMap::new();
    for entry in entries.iter_mut() {
        let key = format!(
            "{}{SEPARATOR}{}",
            entry.kind.map_or("plain", OutlineKind::name),
            entry.title
        );
        let nth = seen.entry(key.clone()).or_insert(0);
        *nth += 1;
        entry.id = if *nth == 1 {
            key
        } else {
            format!("{key}{SEPARATOR}{nth}")
        };
    }
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

// ---- the tree -----------------------------------------------------------

/// One node of the outline tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineNode {
    pub entry: OutlineEntry,
    pub children: Vec<OutlineNode>,
}

impl OutlineNode {
    /// Whether the node has anything under it, and so can be collapsed.
    pub fn is_folder(&self) -> bool {
        !self.children.is_empty()
    }
}

/// Build the nesting tree from the flat entries.
///
/// A direct port of CotEditor's `OutlineBuildNode.build(from:)`. The rule that
/// is easy to miss is the whitespace indent: such an entry is not nested at
/// all — it becomes a root and *clears* the open levels, so the entries after
/// it start again from the top.
pub fn build_tree(entries: &[OutlineEntry]) -> Vec<OutlineNode> {
    let levels = nesting_levels(entries);
    let flat: Vec<(usize, &OutlineEntry)> = levels.into_iter().zip(entries).collect();
    let mut index = 0;
    descend(&flat, &mut index, 0)
}

/// How many open ancestors each entry has, in order.
///
/// Upstream builds the tree and the depths in one pass; splitting them lets
/// the tree itself be built by a plain recursive descent over a slice, which
/// is what keeps this free of the index bookkeeping that a single-pass port
/// would need.
fn nesting_levels(entries: &[OutlineEntry]) -> Vec<usize> {
    let mut open: Vec<usize> = Vec::new();
    let mut levels = Vec::with_capacity(entries.len());

    for entry in entries {
        let Some(level) = entry.indent.level() else {
            open.clear();
            levels.push(0);
            continue;
        };
        while open.last().is_some_and(|outer| *outer >= level) {
            open.pop();
        }
        levels.push(open.len());
        open.push(level);
    }

    levels
}

/// Collect the nodes at `depth`, recursing into anything deeper.
fn descend(flat: &[(usize, &OutlineEntry)], index: &mut usize, depth: usize) -> Vec<OutlineNode> {
    let mut nodes = Vec::new();
    while let Some((node_depth, entry)) = flat.get(*index) {
        if *node_depth != depth {
            break;
        }
        let entry = (*entry).clone();
        *index += 1;
        let children = match flat.get(*index) {
            Some((child_depth, _)) if *child_depth > depth => descend(flat, index, *child_depth),
            _ => Vec::new(),
        };
        nodes.push(OutlineNode { entry, children });
    }
    nodes
}

/// Every id in the tree, in document order.
///
/// Upstream calls this `allIDs`, and uses it to tell which entries are new
/// since the last parse — those are the ones that open by themselves.
pub fn collect_ids(nodes: &[OutlineNode]) -> Vec<String> {
    let mut ids = Vec::new();
    gather_ids(nodes, &mut ids);
    ids
}

fn gather_ids(nodes: &[OutlineNode], ids: &mut Vec<String>) {
    for node in nodes {
        ids.push(node.entry.id.clone());
        gather_ids(&node.children, ids);
    }
}

// ---- the filter ---------------------------------------------------------

/// One node of the tree as the filter leaves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredNode {
    pub entry: OutlineEntry,
    pub children: Vec<FilteredNode>,
    /// The byte ranges in `entry.title` the filter matched, merged where they
    /// run together. Empty when the node is shown only because a descendant
    /// matched — which is how the path down to a hit stays visible.
    pub matches: Vec<Range<usize>>,
}

impl FilteredNode {
    /// Whether the node has anything under it, and so can be collapsed.
    pub fn is_folder(&self) -> bool {
        !self.children.is_empty()
    }
}

/// Keep the nodes that match `filter`, and the nodes above them.
pub fn filtered_tree(nodes: &[OutlineNode], filter: &str) -> Vec<FilteredNode> {
    nodes
        .iter()
        .filter_map(|node| filtered_node(node, filter))
        .collect()
}

fn filtered_node(node: &OutlineNode, filter: &str) -> Option<FilteredNode> {
    let children: Vec<_> = node
        .children
        .iter()
        .filter_map(|child| filtered_node(child, filter))
        .collect();
    // An empty filter matches everything, which is upstream's `.noFilter`
    // state; only a real filter can reject a node.
    let matches = if filter.is_empty() {
        Some(Vec::new())
    } else {
        abbreviated_matched_ranges(&node.entry.title, filter)
    };

    if matches.is_none() && children.is_empty() {
        return None;
    }

    Some(FilteredNode {
        entry: node.entry.clone(),
        children,
        matches: matches.unwrap_or_default(),
    })
}

/// The ids of every node that has children.
///
/// Upstream calls this `expandableIDs`: when a filter is on, the whole tree
/// opens, because a collapsed match would be invisible.
pub fn expandable_ids(nodes: &[FilteredNode]) -> HashSet<String> {
    let mut ids = HashSet::new();
    gather_expandable(nodes, &mut ids);
    ids
}

fn gather_expandable(nodes: &[FilteredNode], ids: &mut HashSet<String>) {
    for node in nodes {
        if node.is_folder() {
            ids.insert(node.entry.id.clone());
            gather_expandable(&node.children, ids);
        }
    }
}

/// The ranges of the characters of `filter` found in `text`, in order.
///
/// The search itself is [`subtext_core::matcher::abbreviated_matched_ranges`],
/// shared with the Quick Actions bar. What this adds is the merging: upstream
/// reports one range per matched character, and the outline draws a run of
/// them as a single highlight, so adjacent ranges are joined here rather than
/// in the renderer.
///
/// Returns `None` when the filter is empty — there is nothing to highlight —
/// or when a character is missing.
pub fn abbreviated_matched_ranges(text: &str, filter: &str) -> Option<Vec<Range<usize>>> {
    let ranges = subtext_core::matcher::abbreviated_matched_ranges(text, filter, false)?;

    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if last.end == range.start => last.end = range.end,
            _ => merged.push(range),
        }
    }
    Some(merged)
}

/// The entry the caret at `offset` sits in.
///
/// CotEditor's `item(at:)`: the *last* entry that starts at or before the
/// caret. Separators are skipped — they mark a boundary rather than a place.
///
/// This asks about the entry's *range*, not its line. A rule that matches
/// mid-line, like SVG's `id="…"`, puts the caret at the start of that line
/// *before* the entry it will reach: the line would claim the caret for an
/// entry the stepping commands below do not consider it to be in, and the two
/// would disagree about which row ⌃⌘↑ starts from.
pub fn entry_at(entries: &[OutlineEntry], offset: usize) -> Option<&OutlineEntry> {
    index_of_item(entries, offset).map(|index| &entries[index])
}

/// The entry before the one `selection` sits in.
///
/// CotEditor's `previousItem(for:)`: find the entry the *start* of the
/// selection is in, then step one back. `None` when there is none — which
/// includes a selection that starts before the first entry, so the caret on
/// the first line reaches back to nothing rather than wrapping to the end.
pub fn previous_item<'a>(
    entries: &'a [OutlineEntry],
    selection: &Range<usize>,
) -> Option<&'a OutlineEntry> {
    let current = index_of_item(entries, selection.start)?;
    entries[..current]
        .iter()
        .rev()
        .find(|entry| !entry.is_separator())
}

/// The entry after the one `selection` sits in.
///
/// CotEditor's `nextItem(for:)`, including its first branch: a selection that
/// *ends* before the first entry gets the first entry. That is what makes
/// ⌃⌘↓ from the top of a file reach the outline at all — no entry contains the
/// caret yet, so without the branch there would be nothing to step off.
pub fn next_item<'a>(
    entries: &'a [OutlineEntry],
    selection: &Range<usize>,
) -> Option<&'a OutlineEntry> {
    if let Some(first) = entries.iter().find(|entry| !entry.is_separator())
        && selection.end < first.range.start
    {
        return Some(first);
    }
    let current = index_of_item(entries, selection.end)?;
    entries[current + 1..]
        .iter()
        .find(|entry| !entry.is_separator())
}

/// The index of the last non-separator entry that starts at or before `offset`.
///
/// CotEditor's `indexOfItem(at:)`. [`entry_at`] reads it directly and the two
/// stepping functions step off it, so all three agree on which entry the caret
/// is in — which is the whole reason they share it.
fn index_of_item(entries: &[OutlineEntry], offset: usize) -> Option<usize> {
    entries
        .iter()
        .rposition(|entry| !entry.is_separator() && entry.range.start <= offset)
}

// ---- what a row needs to be drawn ---------------------------------------

/// Everything the view needs to draw one row beyond the label it already has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    /// The rule's kind, or `None` for a rule that names none.
    pub kind: Option<OutlineKind>,
    /// The whitespace a whitespace indent captured, drawn before the symbol.
    ///
    /// `None` means the entry's indent is a level rather than captured
    /// whitespace — a `title` rule, or a `heading.N` one. Only a heading can
    /// be either, so this is what tells the two kinds of heading apart.
    pub indent: Option<String>,
    /// The byte ranges in the title the filter matched.
    pub matches: Vec<Range<usize>>,
}

impl OutlineRow {
    /// The symbol the row shows, or `None` for a rule that named no kind.
    ///
    /// These are the closest Lucide equivalents of the SF Symbols CotEditor
    /// uses; `heading.N` is the one case that cannot be matched, because Lucide
    /// has no numbered squares, so a levelled heading gets `Hash` where
    /// upstream gets `N.square.fill`.
    pub fn icon(&self) -> Option<IconName> {
        Some(match self.kind? {
            OutlineKind::Container => IconName::Box,
            OutlineKind::Value => IconName::SquareDot,
            OutlineKind::Function => IconName::SquareFunction,
            OutlineKind::Title => IconName::Tag,
            // `indent` is `None` exactly when the entry's indent is a level,
            // which for a heading means `heading.N` and for everything else
            // means a `title` rule. Only a heading can be either.
            OutlineKind::Heading if self.indent.is_none() => IconName::Hash,
            OutlineKind::Heading => IconName::SquareMenu,
            OutlineKind::Mark => IconName::Flag,
            OutlineKind::Separator => IconName::Minus,
        })
    }

    /// The colour of that symbol.
    ///
    /// CotEditor colours each kind with a macOS system colour. These are those
    /// colours in the light appearance, converted to HSLA, and they are
    /// deliberately *not* theme colours: the point of the colour is to say what
    /// kind of thing this is, which must not change when the editor's theme
    /// does.
    pub fn icon_color(&self) -> Hsla {
        match self.kind {
            Some(OutlineKind::Container) => hsla(0.587, 1.0, 0.50, 1.0),  // systemBlue
            Some(OutlineKind::Value) => hsla(0.377, 0.584, 0.492, 1.0),    // systemGreen
            Some(OutlineKind::Function) => hsla(0.097, 1.0, 0.50, 1.0),    // systemOrange
            Some(OutlineKind::Title) => hsla(0.526, 0.609, 0.484, 1.0),    // systemTeal
            Some(OutlineKind::Heading) => hsla(0.668, 0.606, 0.588, 1.0),  // systemIndigo
            Some(OutlineKind::Mark) => hsla(0.009, 1.0, 0.594, 1.0),       // systemRed
            Some(OutlineKind::Separator) | None => hsla(0.667, 0.022, 0.569, 1.0), // systemGray
        }
    }
}

/// Index every row of `nodes` by its entry id.
///
/// The tree hands its renderer only an id and a label, so the kind, the
/// captured indent and the filter matches have to be looked up by id.
pub fn row_map(nodes: &[FilteredNode]) -> HashMap<String, OutlineRow> {
    let mut rows = HashMap::new();
    collect_rows(nodes, &mut rows);
    rows
}

fn collect_rows(nodes: &[FilteredNode], rows: &mut HashMap<String, OutlineRow>) {
    for node in nodes {
        let indent = match &node.entry.indent {
            Indent::String(whitespace) => Some(whitespace.clone()),
            Indent::Level(_) => None,
        };
        rows.insert(
            node.entry.id.clone(),
            OutlineRow {
                kind: node.entry.kind,
                indent,
                matches: node.matches.clone(),
            },
        );
        collect_rows(&node.children, rows);
    }
}

/// Split `title` into runs, each flagged as matched or not.
///
/// The view draws the matched runs on the find-highlight colour. Ranges are
/// byte offsets into `title`; one that does not land on a character boundary,
/// or that runs past the end, is skipped rather than panicking, because a
/// label and a filter can be out of step for a frame while the tree catches up.
pub fn highlight_segments(title: &str, matches: &[Range<usize>]) -> Vec<(Range<usize>, bool)> {
    let mut segments = Vec::new();
    let mut cursor = 0;

    for range in matches {
        if range.start < cursor
            || range.end > title.len()
            || !title.is_char_boundary(range.start)
            || !title.is_char_boundary(range.end)
        {
            continue;
        }
        if cursor < range.start {
            segments.push((cursor..range.start, false));
        }
        segments.push((range.clone(), true));
        cursor = range.end;
    }

    if cursor < title.len() {
        segments.push((cursor..title.len(), false));
    }

    segments
}

// ---- the warnings inspector ---------------------------------------------

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
    let text = document.editor().read(cx).value().to_string();
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

    /// The ids of a tree's roots and their descendants, in document order.
    fn tree_titles(nodes: &[OutlineNode]) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        fn walk(nodes: &[OutlineNode], depth: usize, out: &mut Vec<(usize, String)>) {
            for node in nodes {
                out.push((depth, node.entry.title.clone()));
                walk(&node.children, depth + 1, out);
            }
        }
        walk(nodes, 0, &mut out);
        out
    }

    fn filtered_titles(nodes: &[FilteredNode]) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        fn walk(nodes: &[FilteredNode], depth: usize, out: &mut Vec<(usize, String)>) {
            for node in nodes {
                out.push((depth, node.entry.title.clone()));
                walk(&node.children, depth + 1, out);
            }
        }
        walk(nodes, 0, &mut out);
        out
    }

    // ---- extraction -----------------------------------------------------

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
    fn a_captured_whitespace_indent_is_kept_as_whitespace() {
        // The whitespace is *not* a level: it is displayed as-is and it takes
        // the entry out of the nesting. Collapsing it to a number is what an
        // earlier revision did, and it mis-nested every outline built from a
        // rule like this one.
        let rules = [rule("function", r"^(\s*)fn (\w+)", "$1$2")];
        let entries = extract("fn top() {}\n    fn nested() {}\n", &rules);
        assert_eq!(titles(&entries), ["top", "nested"]);
        assert_eq!(entries[0].indent, Indent::String(String::new()));
        assert_eq!(entries[1].indent, Indent::String("    ".into()));
        assert_eq!(entries[1].indent.level(), None);
    }

    #[test]
    fn a_title_kind_carries_level_zero() {
        let rules = [rule("title", r"^= (.+)$", "$1")];
        let entries = extract("= Summary\n", &rules);
        assert_eq!(entries[0].indent, Indent::Level(0));
        assert_eq!(entries[0].kind, Some(OutlineKind::Title));
    }

    #[test]
    fn a_heading_kind_carries_its_own_level() {
        let rules = [rule("heading.3", r"^### (.+)$", "$1")];
        let entries = extract("### Deep\n", &rules);
        assert_eq!(entries[0].indent, Indent::Level(3));
        assert_eq!(entries[0].title, "Deep");
        // The level lives in the indent, so the kind is just "a heading".
        assert_eq!(entries[0].kind, Some(OutlineKind::Heading));
    }

    #[test]
    fn a_heading_without_a_level_falls_back_to_its_whitespace() {
        // Two of the bundled rules are spelled `"kind": "heading"` with no
        // level, and upstream sends those down the whitespace path.
        let rules = [rule("heading", r"^(\s*)# (.+)$", "$1$2")];
        let entries = extract("  # Heading\n", &rules);
        assert_eq!(entries[0].indent, Indent::String("  ".into()));
    }

    #[test]
    fn a_rule_without_a_kind_gets_no_symbol() {
        let rules = [rule("", r"^# (.+)$", "$1")];
        let entries = extract("# Title\n", &rules);
        assert_eq!(entries[0].kind, None);
        assert_eq!(entries[0].indent, Indent::String(String::new()));
    }

    #[test]
    fn a_separator_is_recorded_rather_than_labelled() {
        let rules = [rule("separator", r"^---$", "")];
        let entries = extract("---\nkey: 1\n", &rules);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_separator());
        assert!(entries[0].title.is_empty());
        // Upstream's `OutlineItem.separator` defaults to `.level(0)`.
        assert_eq!(entries[0].indent, Indent::Level(0));
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
    fn entries_are_ordered_by_position() {
        let text = "fn b() {}\nstruct A;\n";
        let entries = extract(text, &syntax("Rust").outlines);
        let starts: Vec<_> = entries.iter().map(|entry| entry.range.start).collect();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        // `fn b` is the first line and `struct A` the second.
        assert_eq!(starts, [0, 10]);
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

    #[test]
    fn two_rules_on_the_same_span_with_the_same_label_are_one_entry() {
        // The one place `extract` drops something upstream would keep, and it
        // is keyed on the span: the same text, described twice.
        let rules = [
            rule("function", r"^fn (\w+)", "$1"),
            rule("value", r"^fn (\w+)", "$1"),
        ];
        let entries = extract("fn a() {}\n", &rules);
        assert_eq!(titles(&entries), ["a"]);
    }

    #[test]
    fn two_rules_on_the_same_line_are_two_entries() {
        // …but the same *line* is not the same thing, which is what an earlier
        // revision treated it as. A rule matching mid-line is its own entry.
        let rules = [
            rule("function", r"^fn (\w+)", "$1"),
            rule("value", r#"id="([^"]+)""#, "#$1"),
        ];
        let entries = extract("fn a() {}  id=\"b\"\n", &rules);
        assert_eq!(titles(&entries), ["a", "#b"]);
    }

    // ---- identity --------------------------------------------------------

    #[test]
    fn an_id_survives_a_reparse_that_does_not_change_the_entry() {
        let rules = [rule("function", r"^fn (\w+)", "$1")];
        let before = extract("fn a() {}\n", &rules);
        // The same entry, one line further down.
        let after = extract("\n\nfn a() {}\n", &rules);
        assert_eq!(before[0].id, after[0].id);
        assert_eq!(before[0].range, 0..4);
        assert_eq!(after[0].range, 2..6);
    }

    #[test]
    fn duplicates_of_the_same_kind_and_label_get_distinct_ids() {
        let rules = [rule("function", r"^fn (\w+)", "$1")];
        let entries = extract("fn a() {}\nfn a() {}\n", &rules);
        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].id, entries[1].id);
    }

    #[test]
    fn a_label_cannot_collide_with_its_kind() {
        // The id is `kind + separator + title`, and the separator is a
        // character no label can hold, so these two stay distinct.
        let rules = [
            rule("function", r"^fn (\w+)", "$1"),
            rule("value", r"^let (\w+)", "$1"),
        ];
        let entries = extract("fn a() {}\nlet a = 1;\n", &rules);
        assert_ne!(entries[0].id, entries[1].id);
    }

    // ---- the tree --------------------------------------------------------

    #[test]
    fn a_levelled_entry_nests_under_the_one_above_it() {
        let rules = [
            rule("heading.1", r"^# (.+)$", "$1"),
            rule("heading.2", r"^## (.+)$", "$1"),
        ];
        let text = "# One\n## Two\n## Three\n# Four\n";
        let entries = extract(text, &rules);
        let tree = build_tree(&entries);
        assert_eq!(
            tree_titles(&tree),
            [
                (0, "One".into()),
                (1, "Two".into()),
                (1, "Three".into()),
                (0, "Four".into()),
            ]
        );
    }

    #[test]
    fn a_whitespace_entry_is_a_root_and_clears_the_levels() {
        // This is the whole reason `Indent` has two cases. Upstream:
        // `guard case .level(let level) = item.indent else { roots.append(node);
        // stack.removeAll() }`.
        let mut entries = extract(
            "# Top\n",
            &[rule("heading.1", r"^# (.+)$", "$1")],
        );
        // A whitespace entry, as a `function` rule would produce.
        entries.push(OutlineEntry {
            id: String::new(),
            title: "loose".into(),
            range: 8..13,
            indent: Indent::String(String::new()),
            kind: Some(OutlineKind::Function),
        });
        let tree = build_tree(&entries);
        assert_eq!(
            tree_titles(&tree),
            [(0, "Top".into()), (0, "loose".into())]
        );
    }

    #[test]
    fn a_skipped_level_still_nests_one_deep() {
        // A `heading.3` under a `heading.1` is one level in, not two: the
        // depth is the number of open ancestors, not the level itself.
        let rules = [
            rule("heading.1", r"^# (.+)$", "$1"),
            rule("heading.3", r"^### (.+)$", "$1"),
        ];
        let entries = extract("# One\n### Three\n", &rules);
        let tree = build_tree(&entries);
        assert_eq!(
            tree_titles(&tree),
            [(0, "One".into()), (1, "Three".into())]
        );
    }

    #[test]
    fn a_separator_lands_at_the_top_level() {
        let rules = [
            rule("heading.1", r"^# (.+)$", "$1"),
            rule("separator", r"^---$", ""),
        ];
        let entries = extract("# One\n---\n", &rules);
        let tree = build_tree(&entries);
        assert_eq!(tree.len(), 2);
        assert!(tree[1].entry.is_separator());
        assert!(tree[1].children.is_empty());
    }

    #[test]
    fn collect_ids_walks_the_tree_in_document_order() {
        let rules = [
            rule("heading.1", r"^# (.+)$", "$1"),
            rule("heading.2", r"^## (.+)$", "$1"),
        ];
        let entries = extract("# One\n## Two\n# Three\n", &rules);
        let tree = build_tree(&entries);
        let ids = collect_ids(&tree);
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[0], entries[0].id);
        assert_eq!(ids[1], entries[1].id);
        assert_eq!(ids[2], entries[2].id);
    }

    // ---- the filter ------------------------------------------------------

    #[test]
    fn the_filter_matches_a_subsequence() {
        // `gt` finds "Guten Tag" — this is not a substring search. Each
        // character is looked for from where the last one ended, so the `t` is
        // the one in "Guten" rather than the one starting "Tag".
        let ranges = abbreviated_matched_ranges("Guten Tag", "gt").unwrap();
        assert_eq!(ranges, [0..1, 2..3]);
    }

    #[test]
    fn the_filter_ignores_case() {
        let ranges = abbreviated_matched_ranges("Guten Tag", "gT").unwrap();
        assert_eq!(ranges, [0..1, 2..3]);
    }

    #[test]
    fn the_filter_merges_a_run_of_matches() {
        let ranges = abbreviated_matched_ranges("Guten Tag", "guten").unwrap();
        assert_eq!(ranges, vec![0..5]);
    }

    #[test]
    fn the_filter_does_not_reuse_a_character() {
        // The search for each character starts where the last one ended, so a
        // second `a` cannot be found in the same place.
        assert_eq!(abbreviated_matched_ranges("ab", "aa"), None);
        assert_eq!(abbreviated_matched_ranges("aba", "aa"), Some(vec![0..1, 2..3]));
    }

    #[test]
    fn an_empty_filter_matches_nothing_to_highlight() {
        assert_eq!(abbreviated_matched_ranges("anything", ""), None);
        assert_eq!(abbreviated_matched_ranges("", "a"), None);
    }

    #[test]
    fn a_filtered_tree_keeps_the_path_to_a_hit() {
        let rules = [
            rule("heading.1", r"^# (.+)$", "$1"),
            rule("heading.2", r"^## (.+)$", "$1"),
        ];
        let entries = extract("# Alpha\n## Beta\n## Gamma\n# Delta\n", &rules);
        let tree = build_tree(&entries);
        let filtered = filtered_tree(&tree, "gam");
        // `Delta` is gone, `Alpha` stays only because `Gamma` is under it.
        assert_eq!(
            filtered_titles(&filtered),
            [(0, "Alpha".into()), (1, "Gamma".into())]
        );
        assert!(filtered[0].matches.is_empty(), "Alpha did not match itself");
        assert_eq!(filtered[0].children[0].matches, vec![0..3]);
    }

    #[test]
    fn an_empty_filter_keeps_every_node() {
        let rules = [rule("heading.1", r"^# (.+)$", "$1")];
        let entries = extract("# One\n# Two\n", &rules);
        let tree = build_tree(&entries);
        let filtered = filtered_tree(&tree, "");
        assert_eq!(filtered_titles(&filtered), [(0, "One".into()), (0, "Two".into())]);
        assert!(filtered.iter().all(|node| node.matches.is_empty()));
    }

    #[test]
    fn expandable_ids_names_the_folders() {
        let rules = [
            rule("heading.1", r"^# (.+)$", "$1"),
            rule("heading.2", r"^## (.+)$", "$1"),
        ];
        let entries = extract("# One\n## Two\n# Three\n", &rules);
        let tree = build_tree(&entries);
        let filtered = filtered_tree(&tree, "");
        let expandable = expandable_ids(&filtered);
        assert_eq!(expandable.len(), 1);
        assert!(expandable.contains(&entries[0].id));
    }

    // ---- the caret -------------------------------------------------------

    #[test]
    fn the_caret_picks_the_last_entry_at_or_before_it() {
        let rules = [rule("heading.1", r"^# (.+)$", "$1")];
        let text = "# One\n\n\n# Two\n";
        let entries = extract(text, &rules);
        // `# One` matches `0..5`, `# Two` matches `8..13`.
        assert_eq!(entries[0].range, 0..5);
        assert_eq!(entries[1].range, 8..13);
        assert_eq!(entry_at(&entries, 0).unwrap().title, "One");
        assert_eq!(entry_at(&entries, 4).unwrap().title, "One");
        assert_eq!(entry_at(&entries, 5).unwrap().title, "One");
        assert_eq!(entry_at(&entries, 7).unwrap().title, "One");
        assert_eq!(entry_at(&entries, 8).unwrap().title, "Two");
        assert_eq!(entry_at(&entries, 99).unwrap().title, "Two");
    }

    #[test]
    fn a_caret_before_every_entry_is_in_none_of_them() {
        // The difference between asking by offset and asking by line: the
        // caret on line 1 but before the match is not yet in the entry that
        // line 1 will produce.
        let rules = [rule("value", r#"id="([^"]+)""#, "#$1")];
        let entries = extract("  <g id=\"g1\">\n", &rules);
        // `id="g1"` starts after `  <g `, so at 5.
        assert_eq!(entries[0].range, 5..12);
        assert!(entry_at(&entries, 0).is_none());
        assert!(entry_at(&entries, 4).is_none());
        assert_eq!(entry_at(&entries, 5).unwrap().title, "#g1");
    }

    #[test]
    fn the_caret_skips_separators() {
        let rules = [
            rule("heading.1", r"^# (.+)$", "$1"),
            rule("separator", r"^---$", ""),
        ];
        let entries = extract("# One\n---\n", &rules);
        // The separator matches `6..9`, the heading `0..5`.
        assert_eq!(entries[1].range, 6..9);
        assert_eq!(entry_at(&entries, 7).unwrap().title, "One");
    }

    // ---- stepping through the entries ------------------------------------

    /// The fixture CotEditor's `OutlineCollectionTests` uses: three entries
    /// with separators between them, so every case there has a case here.
    ///
    /// It is built by hand for the same reason upstream builds it by hand —
    /// these cases are about the ranges, and a real document would have to be
    /// padded to put the matches on those exact offsets. The labels carry no
    /// kind, matching `OutlineItem(title:range:)`'s default of `nil`.
    fn upstream_fixture() -> Vec<OutlineEntry> {
        fn item(range: Range<usize>, title: &str, kind: Option<OutlineKind>) -> OutlineEntry {
            OutlineEntry {
                id: format!("{title}@{}", range.start),
                title: title.to_string(),
                range,
                indent: Indent::Level(0),
                kind,
            }
        }
        let separator = Some(OutlineKind::Separator);
        vec![
            item(10..15, "dog", None),        // 0
            item(20..25, "", separator),      // 1
            item(30..35, "", separator),      // 2
            item(40..45, "dogcow", None),     // 3
            item(50..55, "", separator),      // 4
            item(60..65, "cow", None),        // 5
            item(70..75, "", separator),      // 6
        ]
    }

    fn title_of(entry: Option<&OutlineEntry>) -> Option<&str> {
        entry.map(|entry| entry.title.as_str())
    }

    #[test]
    fn the_previous_item_is_the_one_before_the_caret_s_own() {
        // CotEditor's `previousItem()` test, case for case.
        let entries = upstream_fixture();
        assert!(title_of(previous_item(&[], &(10..20))).is_none());
        // The caret is in the first entry, so there is nothing before it.
        assert!(title_of(previous_item(&entries, &(10..20))).is_none());
        // …and a caret before the first entry is in none of them either.
        assert!(title_of(previous_item(&entries, &(19..19))).is_none());
        // A selection starting inside the last entry steps back to the one
        // before it, and a selection spanning two entries starts in the
        // earlier one, so it steps back one further.
        assert_eq!(title_of(previous_item(&entries, &(60..70))), Some("dogcow"));
        assert_eq!(title_of(previous_item(&entries, &(59..70))), Some("dog"));
    }

    #[test]
    fn the_previous_item_walks_back_one_entry_at_a_time() {
        // The cases the upstream fixture cannot show, because its first entry
        // has nothing before it.
        let entries = upstream_fixture();
        assert_eq!(title_of(previous_item(&entries, &(40..40))), Some("dog"));
        assert_eq!(title_of(previous_item(&entries, &(65..65))), Some("dogcow"));
        // Past the last entry the caret is still in the last entry.
        assert_eq!(title_of(previous_item(&entries, &(99..99))), Some("dogcow"));
    }

    #[test]
    fn the_next_item_is_the_one_after_the_caret_s_own() {
        // CotEditor's `nextItem()` test, case for case.
        let entries = upstream_fixture();
        assert!(title_of(next_item(&[], &(10..20))).is_none());
        // Before the first entry the *first* entry is next, which is the
        // branch that makes ⌃⌘↓ work from the top of a file.
        assert_eq!(title_of(next_item(&entries, &(0..0))), Some("dog"));
        // A selection that ends at the first entry steps past it, and the
        // separators in between are skipped.
        assert_eq!(title_of(next_item(&entries, &(0..10))), Some("dogcow"));
        assert_eq!(title_of(next_item(&entries, &(40..40))), Some("cow"));
        assert!(title_of(next_item(&entries, &(60..60))).is_none());
        assert!(title_of(next_item(&entries, &(40..61))).is_none());
    }

    #[test]
    fn a_separator_is_never_the_answer_in_either_direction() {
        // Separators sit at 20, 30, 50 and 70, and every answer above skipped
        // them. This pins the skipping itself: with nothing but separators
        // there is no answer at all, and no entry contains the caret.
        let separators = vec![
            OutlineEntry {
                id: "a".into(),
                title: String::new(),
                range: 0..3,
                indent: Indent::Level(0),
                kind: Some(OutlineKind::Separator),
            },
            OutlineEntry {
                id: "b".into(),
                title: String::new(),
                range: 10..13,
                indent: Indent::Level(0),
                kind: Some(OutlineKind::Separator),
            },
        ];
        assert!(title_of(entry_at(&separators, 0)).is_none());
        assert!(title_of(previous_item(&separators, &(10..13))).is_none());
        assert!(title_of(next_item(&separators, &(0..3))).is_none());
    }

    #[test]
    fn stepping_over_a_multibyte_entry_uses_byte_offsets() {
        // A rule matching past a wide character: the entry's range is a byte
        // range and the caret arrives as one, so they have to be counted the
        // same way. `日` is three bytes and `本` three more, so `# 語` starts
        // at 7 and the label runs to 12.
        let rules = [rule("heading.1", r"^# (.+)$", "$1")];
        let entries = extract("日本\n# 語\n", &rules);
        assert_eq!(entries[0].range, 7..12);
        assert!(title_of(entry_at(&entries, 6)).is_none());
        assert_eq!(title_of(entry_at(&entries, 7)), Some("語"));
        assert!(title_of(previous_item(&entries, &(7..12))).is_none());
        assert_eq!(title_of(next_item(&entries, &(0..6))), Some("語"));
    }

    // ---- drawing ---------------------------------------------------------

    #[test]
    fn highlights_split_the_title_into_runs() {
        let segments = highlight_segments("Guten Tag", &[0..1, 6..7]);
        assert_eq!(
            segments,
            [
                (0..1, true),
                (1..6, false),
                (6..7, true),
                (7..9, false),
            ]
        );
    }

    #[test]
    fn a_title_with_no_matches_is_one_plain_run() {
        assert_eq!(highlight_segments("abc", &[]), [(0..3, false)]);
        assert!(highlight_segments("", &[]).is_empty());
    }

    #[test]
    fn a_match_off_a_character_boundary_is_skipped() {
        // A multi-byte label and a range that lands inside its first
        // character: the row still draws, just without the highlight.
        let title = "é";
        let straddling = 1..2;
        assert_eq!(highlight_segments(title, &[straddling]), [(0..2, false)]);
    }

    #[test]
    fn a_row_map_carries_the_kind_and_the_indent() {
        let rules = [rule("function", r"^(\s*)fn (\w+)", "$1$2")];
        let entries = extract("    fn nested() {}\n", &rules);
        let tree = build_tree(&entries);
        let filtered = filtered_tree(&tree, "");
        let rows = row_map(&filtered);
        let row = &rows[&entries[0].id];
        assert_eq!(row.kind, Some(OutlineKind::Function));
        assert_eq!(row.indent.as_deref(), Some("    "));
        assert_eq!(row.icon(), Some(IconName::SquareFunction));
    }

    #[test]
    fn a_kind_picks_its_own_symbol() {
        let row = |kind| OutlineRow {
            kind: Some(kind),
            indent: None,
            matches: Vec::new(),
        };
        assert_eq!(row(OutlineKind::Container).icon(), Some(IconName::Box));
        assert_eq!(row(OutlineKind::Value).icon(), Some(IconName::SquareDot));
        assert_eq!(row(OutlineKind::Function).icon(), Some(IconName::SquareFunction));
        assert_eq!(row(OutlineKind::Title).icon(), Some(IconName::Tag));
        assert_eq!(row(OutlineKind::Mark).icon(), Some(IconName::Flag));
        assert_eq!(row(OutlineKind::Separator).icon(), Some(IconName::Minus));
        assert_eq!(row(OutlineKind::Function).icon_color().h, 0.097);
        assert_eq!(
            OutlineRow {
                kind: None,
                indent: None,
                matches: Vec::new(),
            }
            .icon(),
            None
        );
    }

    #[test]
    fn a_levelled_heading_gets_the_hash() {
        // `heading.1` … `heading.6` carry a level, so they draw no captured
        // whitespace and get the hash.
        let row = OutlineRow {
            kind: Some(OutlineKind::Heading),
            indent: None,
            matches: Vec::new(),
        };
        assert_eq!(row.icon(), Some(IconName::Hash));

        // A bare `heading` rule nests by its captured whitespace instead, and
        // gets the plain square. Two bundled rules are spelled that way.
        let row = OutlineRow {
            kind: Some(OutlineKind::Heading),
            indent: Some("  ".into()),
            matches: Vec::new(),
        };
        assert_eq!(row.icon(), Some(IconName::SquareMenu));
    }

    #[test]
    fn a_container_and_a_mark_are_bold() {
        assert!(OutlineKind::Container.is_bold());
        assert!(OutlineKind::Mark.is_bold());
        assert!(!OutlineKind::Function.is_bold());
        assert!(!OutlineKind::Heading.is_bold());
    }

    #[test]
    fn every_kind_round_trips_through_its_name() {
        for kind in [
            OutlineKind::Container,
            OutlineKind::Value,
            OutlineKind::Function,
            OutlineKind::Title,
            OutlineKind::Heading,
            OutlineKind::Mark,
            OutlineKind::Separator,
        ] {
            assert_eq!(OutlineKind::from_name(kind.name()), Some(kind));
            assert!(!kind.label().is_empty());
        }
        assert_eq!(OutlineKind::from_name("heading.4"), Some(OutlineKind::Heading));
        assert_eq!(OutlineKind::from_name("nonsense"), None);
        assert_eq!(OutlineKind::from_name(""), None);
    }
}
