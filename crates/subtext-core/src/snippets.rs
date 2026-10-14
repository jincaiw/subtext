//! Snippets: the text a command or a key equivalent inserts.
//!
//! CotEditor's *Text > Insert Snippet* submenu is filled at runtime from a list
//! of snippets the user edits in Settings. Each one is a name, an optional
//! syntax scope, an optional key equivalent and a *format* — the text to insert,
//! with two tokens standing for things only the document knows:
//!
//! - `<<<SELECTION>>>` is replaced by the selected text, and
//! - `<<<CURSOR>>>` marks where the caret should end up.
//!
//! A format can carry several cursor tokens, which is what makes a snippet able
//! to produce more than one line with the caret left in a chosen place — see
//! [`Snippet::insertion`].
//!
//! # The two subtleties
//!
//! *The format is indented before it is split.* Every line break in the format
//! gains the indentation of the line the snippet is inserted into, so a snippet
//! inserted inside a block lines up with it. The indentation is taken from the
//! document, not from the snippet: `indent_at` reads the leading run of spaces
//! and tabs of the line the insertion starts on.
//!
//! *The cursor tokens are split on first, then the selection is substituted.*
//! Doing it the other way round would turn a `<<<CURSOR>>>` that is part of the
//! selected text into a marker, moving the caret into the middle of what the
//! user had selected. Upstream splits first for exactly that reason, and the
//! test `a_cursor_token_in_the_selection_stays_literal` holds the behaviour
//! down.
//!
//! # What is not here
//!
//! Upstream's `Snippet.insertions(for:ranges:)` expands a snippet once per
//! selected range, because `NSTextView` can hold several. SubText's editor has
//! one selection, so only the single-range form — [`Snippet::insertion`], which
//! is the same algorithm without the offset bookkeeping — is ported.

use std::ops::Range;

use serde::de::{Deserialize, Deserializer, Error as _};
use serde::ser::{Serialize, Serializer};

/// What a token starts with.
pub const TOKEN_PREFIX: &str = "<<<";

/// What a token ends with.
pub const TOKEN_SUFFIX: &str = ">>>";

/// The token a format uses to stand for the selected text.
pub const SELECTION_TOKEN: &str = "<<<SELECTION>>>";

/// The token a format uses to mark where the caret lands.
pub const CURSOR_TOKEN: &str = "<<<CURSOR>>>";

/// The tokens a format understands, in the order the *Insert Variable* menu
/// lists them.
pub const TOKENS: [&str; 2] = [SELECTION_TOKEN, CURSOR_TOKEN];

/// One of the two things a snippet format can insert.
///
/// Upstream's `Snippet.Variable`, whose `rawValue` is the name inside the
/// token's brackets — `SELECTION`, not `<<<SELECTION>>>`. The settings pane
/// shows the name and inserts the whole token, which is why both are here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variable {
    Selection,
    Cursor,
}

impl Variable {
    /// Every variable, in upstream's declaration order — which is the order
    /// the *Insert Variable* menu lists them.
    pub const ALL: [Variable; 2] = [Variable::Selection, Variable::Cursor];

    /// The name the token wraps.
    pub fn name(self) -> &'static str {
        match self {
            Variable::Selection => "SELECTION",
            Variable::Cursor => "CURSOR",
        }
    }

    /// The whole token, brackets and all: what the menu inserts.
    pub fn token(self) -> String {
        format!("{TOKEN_PREFIX}{}{TOKEN_SUFFIX}", self.name())
    }

    /// What the variable stands for, in CotEditor's wording.
    pub fn description(self) -> &'static str {
        match self {
            Variable::Selection => "The selected text.",
            Variable::Cursor => "The insertion point after inserting the snippet.",
        }
    }
}

/// Why a key equivalent cannot be used as a shortcut.
///
/// Upstream's `Shortcut.CustomizationError`, which the settings pane shows
/// beside the field. Its fourth case — a combination another menu command
/// already has — is not here, because answering it means knowing the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// Tab, Backtab or Shift-Return, which a text view needs for itself.
    Unsupported,
    /// Shift and a letter, which types a capital letter instead.
    ShiftOnly,
    /// One key with no Command, Control or Option.
    SingleKey,
}

impl Rejection {
    /// The sentence to show a person, in CotEditor's wording.
    ///
    /// `shortcut` is the combination the user typed, which two of the three
    /// sentences quote back.
    pub fn message(self, shortcut: Shortcut) -> String {
        match self {
            Self::Unsupported => format!(
                "The combination “{}” is not supported for the shortcut customization.",
                shortcut.symbol()
            ),
            Self::ShiftOnly => {
                "The Shift key can be used only with another modifier key.".to_string()
            }
            Self::SingleKey => "Single type is invalid for a shortcut.".to_string(),
        }
    }
}

// ---- shortcuts -----------------------------------------------------------

/// A key equivalent, in the two forms Cocoa stores and shows one in.
///
/// The stored form is the *key spec* — one character per modifier followed by
/// the key, so ⌘⇧K is `"@$k"`. That is `NSMenuItem.keyEquivalent`'s own
/// notation, which is why it is the form CotEditor writes to disk; keeping it
/// means a snippet list copied out of CotEditor's defaults reads here unchanged.
///
/// The displayed form is the symbols macOS shows in a menu, so ⌘⇧K is
/// `"⌘ ⇧ K"`.
///
/// A key is stored lowercased with an explicit `shift` flag rather than as an
/// uppercase character, which is upstream's `Shortcut.normalized`: AppKit
/// considers ⌘⇧c and ⌘C to be the same shortcut, so the two spellings have to
/// collapse to one value before they can be compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    /// The key, lowercased. One character: a letter, a digit or punctuation.
    pub key: char,
    pub control: bool,
    pub option: bool,
    pub shift: bool,
    pub command: bool,
}

impl Shortcut {
    /// Whether this is a shortcut at all, rather than ordinary typing.
    ///
    /// Upstream's `isValid`: one character, no Fn, and at least one modifier
    /// from the set AppKit lets a menu item carry. The one case it does *not*
    /// cover is Shift on its own with a letter — AppKit reports that as a
    /// capital letter rather than as a command, which is
    /// [`Rejection::ShiftOnly`]'s business, not this one's.
    ///
    /// Upstream also lets a handful of special keys — Home, End, the page keys
    /// and the function keys — stand alone, because they insert nothing. Those
    /// have no character to store in [`Shortcut::key`], so they are not
    /// modelled here.
    pub fn is_valid(self) -> bool {
        self.control || self.option || self.shift || self.command
    }

    /// Why this shortcut cannot be used, or `None` when it can.
    ///
    /// Upstream's `Shortcut.checkCustomizationAvailability`, minus the check
    /// against the menu — that one needs the menu, and belongs to the caller.
    /// The three it can answer on its own are here, in the order it asks them.
    pub fn rejection(self) -> Option<Rejection> {
        // Tab, Backtab and Shift-Return move the caret in a text view; a
        // shortcut on them would break typing rather than add a command.
        if self.key == '\t' || self.key == '\u{19}' || (self.shift && matches!(self.key, '\r' | '\n'))
        {
            return Some(Rejection::Unsupported);
        }
        // Shift and a letter inserts a capital letter instead of running a
        // command, so AppKit never delivers it.
        if self.shift
            && !(self.control || self.option || self.command)
            && self.key.is_alphabetic()
        {
            return Some(Rejection::ShiftOnly);
        }
        if !self.is_valid() {
            return Some(Rejection::SingleKey);
        }
        None
    }

    /// Read a shortcut from any of the three spellings a person might type.
    ///
    /// - the key spec: `"@$k"`, which is what is stored;
    /// - the symbols macOS displays: `"⌘⇧K"` or `"⌘ ⇧ K"`;
    /// - the words a keyboard has: `"cmd-shift-k"`, `"secondary-k"`.
    ///
    /// Returns `None` for anything else, including an empty string, so a
    /// settings field can show a parse failure rather than storing nonsense.
    ///
    /// The text is *not* trimmed: a Tab is a key, and trimming would turn
    /// `"@\t"` into Command-`@`. Spaces between the symbols of a chord are
    /// skipped where they are expected, which is what makes `"⌘ ⇧ K"` work;
    /// callers with a text field should trim before asking.
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() {
            return None;
        }

        // The three spellings are tried in order and the first one that leaves
        // exactly one character standing wins. Trying them in sequence rather
        // than committing to the first that recognises *something* is what makes
        // `^$a` work: Cocoa spells Control `^` in both the stored and the
        // displayed form, so the symbol pass claims the `^` and then trips over
        // the `$`, and only the key-spec pass reads the whole thing.
        let spellings = [
            strip_modifier_words(text),
            strip_modifier_symbols(text),
            Some(strip_modifier_specs(text)),
        ];
        let (modifiers, key) = spellings.into_iter().flatten().find_map(|(modifiers, rest)| {
            let mut chars = rest.chars();
            let key = chars.next()?;
            // A special key has no character to store, so the key is exactly one
            // character and anything left over is not a shortcut. `-` is a key
            // of its own rather than a separator, which is why it survives.
            chars.next().is_none().then_some((modifiers, key))
        })?;

        if key.is_ascii_uppercase() {
            return Some(Self { shift: true, ..modifiers.with_key(key.to_ascii_lowercase()) });
        }
        Some(modifiers.with_key(key))
    }

    /// The stored form: modifier characters followed by the key.
    pub fn spec(self) -> String {
        let mut spec = String::with_capacity(5);
        if self.control {
            spec.push('^');
        }
        if self.option {
            spec.push('~');
        }
        if self.shift {
            spec.push('$');
        }
        if self.command {
            spec.push('@');
        }
        spec.push(self.key);
        spec
    }

    /// The displayed form, the way macOS writes it in a menu.
    ///
    /// Modifiers are joined to each other and to the key by a thin space, which
    /// is what AppKit uses; a plain space would read as a gap rather than as one
    /// chord.
    pub fn symbol(self) -> String {
        /// The thin space AppKit puts between the symbols of one chord.
        const THIN_SPACE: char = '\u{2009}';

        let mut parts: Vec<char> = Vec::with_capacity(5);
        if self.control {
            parts.push('^');
        }
        if self.option {
            parts.push('⌥');
        }
        if self.shift {
            parts.push('⇧');
        }
        if self.command {
            parts.push('⌘');
        }
        parts.push(self.key.to_ascii_uppercase());

        let mut symbol = String::with_capacity(parts.len() * 2);
        for (index, part) in parts.into_iter().enumerate() {
            if index > 0 {
                symbol.push(THIN_SPACE);
            }
            symbol.push(part);
        }
        symbol
    }

    /// The keystroke GPUI's keymap understands, for [`Snippet::shortcut`].
    ///
    /// Command becomes `secondary`, which is Command on macOS and Control
    /// everywhere else — the same translation every other shortcut in the app
    /// gets, so a snippet CotEditor stored as ⌘⇧K is Ctrl+Shift+K on Windows and
    /// Linux rather than a key nobody can press.
    ///
    /// The components are written in the order GPUI documents
    /// (`secondary-ctrl-alt-shift-key`); it parses them in any order, but one
    /// order is easier to read in a diff.
    pub fn keystroke(self) -> String {
        let mut parts: Vec<String> = Vec::with_capacity(5);
        if self.command {
            parts.push("secondary".into());
        }
        if self.control {
            parts.push("ctrl".into());
        }
        if self.option {
            parts.push("alt".into());
        }
        if self.shift {
            parts.push("shift".into());
        }
        parts.push(self.key.to_string());
        parts.join("-")
    }
}

/// The modifiers collected so far, waiting for their key.
struct Modifiers {
    control: bool,
    option: bool,
    shift: bool,
    command: bool,
}

impl Modifiers {
    fn none() -> Self {
        Self { control: false, option: false, shift: false, command: false }
    }

    fn with_key(self, key: char) -> Shortcut {
        Shortcut {
            key,
            control: self.control,
            option: self.option,
            shift: self.shift,
            command: self.command,
        }
    }
}

/// Read the `cmd-shift-k` spelling, or `None` if no modifier word is there.
fn strip_modifier_words(text: &str) -> Option<(Modifiers, &str)> {
    let mut modifiers = Modifiers::none();
    let mut rest = text;
    let mut found = false;
    // The last component is the key, and `-` on its own is the minus key, so a
    // component that is not a modifier word ends the run rather than the parse.
    while let Some((word, tail)) = rest.split_once('-') {
        match word.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.control = true,
            "alt" | "opt" | "option" => modifiers.option = true,
            "shift" => modifiers.shift = true,
            // `secondary` is GPUI's spelling of "Command here, Control
            // elsewhere"; the others are the platform names for it.
            "cmd" | "command" | "secondary" | "super" | "win" | "meta" => {
                modifiers.command = true;
            }
            _ => break,
        }
        found = true;
        rest = tail;
    }
    found.then_some((modifiers, rest))
}

/// Read the `⌘⇧K` spelling, with or without spaces between the symbols.
fn strip_modifier_symbols(text: &str) -> Option<(Modifiers, &str)> {
    let mut modifiers = Modifiers::none();
    let mut rest = text.trim_start();
    let mut found = false;
    loop {
        let mut chars = rest.chars();
        match chars.next() {
            Some('^') => modifiers.control = true,
            Some('⌥') => modifiers.option = true,
            Some('⇧') => modifiers.shift = true,
            Some('⌘') => modifiers.command = true,
            _ => break,
        }
        found = true;
        rest = chars.as_str().trim_start();
    }
    found.then_some((modifiers, rest))
}

/// Read the `@$k` spelling, which is what is stored.
fn strip_modifier_specs(text: &str) -> (Modifiers, &str) {
    let mut modifiers = Modifiers::none();
    let mut rest = text;
    loop {
        let mut chars = rest.chars();
        match chars.next() {
            Some('^') => modifiers.control = true,
            Some('~') => modifiers.option = true,
            Some('$') => modifiers.shift = true,
            Some('@') => modifiers.command = true,
            _ => break,
        }
        rest = chars.as_str();
    }
    (modifiers, rest)
}

impl Serialize for Shortcut {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.spec())
    }
}

impl<'de> Deserialize<'de> for Shortcut {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let spec = String::deserialize(deserializer)?;
        Self::parse(&spec).ok_or_else(|| D::Error::custom(format!("`{spec}` is not a shortcut")))
    }
}

// ---- snippets ------------------------------------------------------------

/// One snippet.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Snippet {
    /// Identity, stable while the snippet exists.
    ///
    /// The menu item that runs a snippet and the settings row that edits it both
    /// name it by this, so neither has to name it by position — a row inserted
    /// above must not change what an existing menu item does.
    pub id: u64,
    pub name: String,
    /// The syntax this snippet is offered in. `None` means every syntax.
    pub scope: Option<String>,
    pub shortcut: Option<Shortcut>,
    /// The text to insert, with [`SELECTION_TOKEN`] and [`CURSOR_TOKEN`] in it.
    pub format: String,
}

impl Snippet {
    /// A snippet with nothing in it, for the settings table's *Add* command.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self { id, name: name.into(), scope: None, shortcut: None, format: String::new() }
    }

    /// Whether this snippet is offered in every syntax.
    ///
    /// Upstream splits its menu into the general snippets and the ones for the
    /// current syntax, so the two questions are asked separately rather than as
    /// one "does this apply" — a general snippet is listed under no heading, and
    /// a scoped one under a heading naming its syntax.
    pub fn is_general(&self) -> bool {
        self.scope.is_none()
    }

    /// Whether this snippet belongs to `scope`.
    ///
    /// The scoped group is only built when the document has a syntax at all —
    /// upstream asks `self.scope.map { ... }` — so there is no `None` case here:
    /// a general snippet is listed by [`Snippet::is_general`], not by matching
    /// an absent scope.
    pub fn matches_scope(&self, scope: &str) -> bool {
        self.scope.as_deref() == Some(scope)
    }

    /// Expand this snippet for one insertion.
    ///
    /// `selected` is the text the snippet replaces, and `indent` is the
    /// indentation of the line it is inserted on — see [`indent_at`].
    pub fn insertion(&self, selected: &str, indent: &str) -> Insertion {
        // Indent first: the cursor offsets are counted over the final text, so
        // everything that lengthens a line has to happen before they are.
        let indented = indent_lines(&self.format, indent);

        // Split on the cursor token *before* the selection goes in, so a cursor
        // token that is part of the selected text stays literal.
        let mut text = String::with_capacity(indented.len() + selected.len());
        let mut cursors = Vec::new();
        for (index, part) in indented.split(CURSOR_TOKEN).enumerate() {
            if index > 0 {
                cursors.push(text.len());
            }
            text.push_str(&part.replace(SELECTION_TOKEN, selected));
        }

        Insertion { text, cursors }
    }
}

/// The text one insertion produces, and where the caret lands in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insertion {
    /// The text that replaces the selection.
    pub text: String,
    /// Where the caret should land, as byte offsets into `text`.
    ///
    /// One entry per [`CURSOR_TOKEN`], in the order the tokens appear. Empty
    /// when the format has no cursor token, in which case the caret belongs
    /// after the inserted text.
    pub cursors: Vec<usize>,
}

impl Insertion {
    /// The caret's offset in `text`, with the end of the text as the fallback.
    pub fn caret(&self) -> usize {
        self.cursors.first().copied().unwrap_or(self.text.len())
    }
}

// ---- the menu ------------------------------------------------------------

/// The two groups the *Insert Snippet* submenu is built from.
///
/// Upstream's `SnippetManager.updateMenu`, with the menu objects taken out: the
/// general snippets are listed first, then a heading naming the document's
/// syntax with that syntax's snippets under it, then the *Edit Snippets…* item.
///
/// A document with no syntax has no scoped group at all — not an empty one — so
/// the heading is not shown over nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sections<'a> {
    /// The snippets with no scope, in the order they were configured.
    pub general: Vec<&'a Snippet>,
    /// The syntax the scoped group is for, when there is one.
    pub scope: Option<&'a str>,
    /// The snippets belonging to that syntax.
    pub scoped: Vec<&'a Snippet>,
}

impl<'a> Sections<'a> {
    /// Split `snippets` for a document whose syntax is `scope`.
    pub fn of(snippets: &'a [Snippet], scope: Option<&'a str>) -> Self {
        let general = snippets.iter().filter(|snippet| snippet.is_general()).collect();
        let scoped = match scope {
            Some(scope) => snippets
                .iter()
                .filter(|snippet| snippet.matches_scope(scope))
                .collect(),
            None => Vec::new(),
        };
        Self { general, scope, scoped }
    }

    /// Whether the shortcut of a general snippet is worth showing.
    ///
    /// A scoped snippet with the same shortcut wins, so advertising that
    /// shortcut beside the general one would be a lie: pressing it runs the
    /// scoped snippet, which is listed further down under its own heading.
    pub fn shows_shortcut(&self, snippet: &Snippet) -> bool {
        !self
            .scoped
            .iter()
            .any(|scoped| scoped.shortcut == snippet.shortcut)
    }

    /// Whether the submenu has anything to offer beyond *Edit Snippets…*.
    pub fn is_empty(&self) -> bool {
        self.general.is_empty() && self.scoped.is_empty()
    }

    /// The snippets that carry a usable shortcut, in the order the submenu
    /// lists them, each paired with the keystroke it stands for.
    ///
    /// Three rules decide who keeps a shortcut:
    ///
    /// - a scoped snippet shadows a general one with the same shortcut, because
    ///   the scoped one is what runs in that syntax — upstream clears the
    ///   general item's key equivalent for exactly this reason;
    /// - of two snippets that share a shortcut, the first listed keeps it, which
    ///   is how macOS settles a duplicate, by menu order;
    /// - a combination [`Shortcut::rejection`] refuses is dropped, so a snippet
    ///   cannot take a key the text view needs.
    ///
    /// The caller still has to check the shortcut against the application's own
    /// menu — upstream does that in the settings pane, and refuses to store one
    /// that a built-in command already has.
    pub fn bindings(&self) -> Vec<(u64, String)> {
        let mut bound: Vec<(u64, String)> = Vec::new();
        let general = self
            .general
            .iter()
            .copied()
            .filter(|snippet| self.shows_shortcut(snippet));

        for snippet in general.chain(self.scoped.iter().copied()) {
            let Some(shortcut) = snippet.shortcut else {
                continue;
            };
            if shortcut.rejection().is_some() {
                continue;
            }
            let keystroke = shortcut.keystroke();
            if bound.iter().any(|(_, taken)| *taken == keystroke) {
                continue;
            }
            bound.push((snippet.id, keystroke));
        }
        bound
    }
}

/// Follow every line break in `format` with `indent`.
///
/// Upstream's `.replacing(/\R/) { $0.output + indent }`. The breaks it knows are
/// the ones a document can contain: CR, LF, CRLF as one break rather than two,
/// and the three Unicode breaks a file may carry instead — NEL, LINE SEPARATOR
/// and PARAGRAPH SEPARATOR.
fn indent_lines(format: &str, indent: &str) -> String {
    if indent.is_empty() {
        return format.to_string();
    }

    let mut out = String::with_capacity(format.len() + indent.len());
    let mut chars = format.chars().peekable();
    while let Some(ch) = chars.next() {
        out.push(ch);
        if !is_line_break(ch) {
            continue;
        }
        if ch == '\r' && chars.peek() == Some(&'\n') {
            chars.next();
            out.push('\n');
        }
        out.push_str(indent);
    }
    out
}

fn is_line_break(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// The indentation of the line `offset` falls on.
///
/// The leading run of spaces and tabs, which is upstream's
/// `String.rangeOfIndent(at:)`. Empty when the line starts with anything else,
/// including a line that is empty.
pub fn indent_at(text: &str, offset: usize) -> &str {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line = &text[line_start..];
    let end = line
        .find(|ch| ch != ' ' && ch != '\t')
        .unwrap_or(line.len());
    &line[..end]
}

/// The range of a line's indentation, or `None` when the line starts flush.
///
/// The settings pane uses this to show which part of a line a snippet would
/// align with; `indent_at` is what the insertion itself uses.
pub fn indent_range_at(text: &str, offset: usize) -> Option<Range<usize>> {
    let indent = indent_at(text, offset);
    if indent.is_empty() {
        return None;
    }
    let start = offset.min(text.len()) - indent.len();
    Some(start..start + indent.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(format: &str) -> Snippet {
        Snippet { id: 1, name: String::new(), scope: None, shortcut: None, format: format.into() }
    }

    /// Join symbols the way AppKit does, with a thin space rather than a gap.
    fn shown(parts: &[&str]) -> String {
        parts.join("\u{2009}")
    }

    // ---- shortcuts -------------------------------------------------------

    /// Upstream's `createKeySpecChars` and `modifierKeyBasics`, which pin the
    /// stored spelling: control `^`, option `~`, shift `$`, command `@`, in that
    /// order however the modifiers were typed.
    #[test]
    fn a_spec_round_trips() {
        for spec in ["a", "k", "^a", "^$a", "~@b", "$@k", "^~$@k", "~,", "@-"] {
            let shortcut = Shortcut::parse(spec).expect(spec);
            assert_eq!(shortcut.spec(), spec, "{spec} did not round trip");
        }
    }

    #[test]
    fn the_spec_orders_its_modifiers_the_way_cocoa_does() {
        // Control, option, shift, command — which is not the order they are
        // listed in on a keyboard.
        let shortcut = Shortcut::parse("@$~^k").unwrap();
        assert_eq!(shortcut.spec(), "^~$@k");
    }

    /// Upstream's `shortcutSymbols` and `menuItemShortcut`.
    #[test]
    fn a_shortcut_reads_back_from_its_displayed_symbols() {
        assert_eq!(Shortcut::parse("^$a").unwrap().symbol(), shown(&["^", "⇧", "A"]));
        assert_eq!(Shortcut::parse("~@b").unwrap().symbol(), shown(&["⌥", "⌘", "B"]));
        // The Command-Shift-C that AppKit hands back for a `C` key equivalent.
        assert_eq!(Shortcut::parse("$@k").unwrap().symbol(), shown(&["⇧", "⌘", "K"]));

        // The thin spaces macOS writes are optional, and so are plain ones.
        assert_eq!(Shortcut::parse(&shown(&["⇧", "⌘", "K"])).unwrap().spec(), "$@k");
        assert_eq!(Shortcut::parse("⇧ ⌘ K").unwrap().spec(), "$@k");
        assert_eq!(Shortcut::parse("⌘⇧K").unwrap().spec(), "$@k");
    }

    #[test]
    fn a_shortcut_reads_back_from_keyboard_words() {
        let shortcut = Shortcut::parse("$@k").unwrap();
        assert_eq!(Shortcut::parse("cmd-shift-k").unwrap(), shortcut);
        assert_eq!(Shortcut::parse("command-shift-k").unwrap(), shortcut);
        assert_eq!(Shortcut::parse("secondary-shift-k").unwrap(), shortcut);
        assert_eq!(Shortcut::parse("ctrl-k").unwrap(), Shortcut::parse("^k").unwrap());
        assert_eq!(Shortcut::parse("alt-k").unwrap(), Shortcut::parse("~k").unwrap());
        assert_eq!(Shortcut::parse("option-k").unwrap(), Shortcut::parse("~k").unwrap());
    }

    /// Upstream's `equivalent`.
    #[test]
    fn an_uppercase_key_becomes_shift() {
        // AppKit treats ⌘⇧c and ⌘C as one shortcut, so the two spellings have to
        // collapse into one value.
        assert_eq!(Shortcut::parse("^A").unwrap(), Shortcut::parse("^$a").unwrap());
        assert_eq!(Shortcut::parse("cmd-K").unwrap(), Shortcut::parse("$@k").unwrap());
        assert_eq!(Shortcut::parse("⌘K").unwrap().spec(), "$@k");
    }

    #[test]
    fn a_shortcut_needs_a_modifier() {
        assert!(!Shortcut::parse("k").unwrap().is_valid());
        assert!(!Shortcut::parse(",").unwrap().is_valid());
        assert!(Shortcut::parse("^k").unwrap().is_valid());
        assert!(Shortcut::parse("~k").unwrap().is_valid());
        assert!(Shortcut::parse("@k").unwrap().is_valid());
        // Shift counts as a modifier here — it is Shift on a *letter* that
        // AppKit will not deliver, and that is `rejection`'s business.
        assert!(Shortcut::parse("$k").unwrap().is_valid());
        assert!(Shortcut::parse("$,").unwrap().is_valid());
    }

    #[test]
    fn nonsense_is_not_a_shortcut() {
        assert_eq!(Shortcut::parse(""), None);
        assert_eq!(Shortcut::parse("   "), None);
        assert_eq!(Shortcut::parse("shift"), None);
        assert_eq!(Shortcut::parse("⌘⇧"), None);
        assert_eq!(Shortcut::parse("@escape"), None);
        // A key is one character, so two of them are not a shortcut.
        assert_eq!(Shortcut::parse("ab"), None);
        assert_eq!(Shortcut::parse("@ab"), None);
    }

    #[test]
    fn the_minus_key_is_a_key_rather_than_a_separator() {
        let shortcut = Shortcut::parse("@-").unwrap();
        assert_eq!(shortcut.key, '-');
        assert_eq!(shortcut.spec(), "@-");
        // Which is also how the keymap has to hear it.
        assert_eq!(shortcut.keystroke(), "secondary--");
    }

    #[test]
    fn a_shortcut_becomes_a_keystroke_the_keymap_knows() {
        assert_eq!(Shortcut::parse("$@k").unwrap().keystroke(), "secondary-shift-k");
        assert_eq!(Shortcut::parse("^~$@k").unwrap().keystroke(), "secondary-ctrl-alt-shift-k");
        // Command moves to Control off macOS, the way every other shortcut does.
        assert_eq!(Shortcut::parse("@,").unwrap().keystroke(), "secondary-,");
    }

    /// Upstream's `checkCustomizationAvailability`, minus the menu check.
    #[test]
    fn a_key_the_text_view_needs_is_not_a_shortcut() {
        // Tab, Backtab and Shift-Return move the caret.
        assert_eq!(Shortcut::parse("@\t").unwrap().rejection(), Some(Rejection::Unsupported));
        assert_eq!(
            Shortcut::parse("$\r").unwrap().rejection(),
            Some(Rejection::Unsupported)
        );
        // Shift and a letter types a capital letter instead of running a
        // command.
        assert_eq!(Shortcut::parse("$k").unwrap().rejection(), Some(Rejection::ShiftOnly));
        // One key with nothing to modify it.
        assert_eq!(Shortcut::parse("k").unwrap().rejection(), Some(Rejection::SingleKey));
        // Shift and punctuation is fine: it is not something a person types.
        assert_eq!(Shortcut::parse("$,").unwrap().rejection(), None);
        assert_eq!(Shortcut::parse("^$a").unwrap().rejection(), None);
    }

    #[test]
    fn a_shortcut_round_trips_through_json_as_its_spec() {        let json = serde_json::to_string(&Shortcut::parse("$@k").unwrap()).unwrap();
        assert_eq!(json, "\"$@k\"");
        let back: Shortcut = serde_json::from_str(&json).unwrap();
        assert_eq!(back.spec(), "$@k");
        assert!(serde_json::from_str::<Shortcut>("\"nonsense\"").is_err());
    }

    // ---- scope -----------------------------------------------------------

    #[test]
    fn a_snippet_without_a_scope_belongs_to_every_syntax() {
        let mut item = snippet("");
        assert!(item.is_general());
        assert!(!item.matches_scope("Rust"));

        item.scope = Some("Rust".into());
        assert!(!item.is_general());
        assert!(item.matches_scope("Rust"));
        assert!(!item.matches_scope("Swift"));
    }

    /// Upstream's `SnippetManager.updateMenu`, whose two groups are not
    /// complements: a document with no syntax has no scoped group at all.
    #[test]
    fn the_menu_groups_general_snippets_before_the_syntax_heading() {
        let mut general = snippet("a");
        general.scope = None;
        let mut scoped = snippet("b");
        scoped.scope = Some("Rust".into());
        let mut other = snippet("c");
        other.scope = Some("Swift".into());
        let snippets = [general, scoped, other];

        let sections = Sections::of(&snippets, Some("Rust"));
        assert_eq!(sections.general, [&snippets[0]]);
        assert_eq!(sections.scope, Some("Rust"));
        assert_eq!(sections.scoped, [&snippets[1]]);
        assert!(!sections.is_empty());

        // A document with no syntax gets the general group and nothing else.
        let sections = Sections::of(&snippets, None);
        assert_eq!(sections.general, [&snippets[0]]);
        assert_eq!(sections.scope, None);
        assert!(sections.scoped.is_empty());
    }

    #[test]
    fn a_scoped_snippet_hides_the_shortcut_of_a_general_one() {
        let shortcut = Shortcut::parse("$@k").unwrap();

        let mut general = snippet("a");
        general.shortcut = Some(shortcut);
        let mut scoped = snippet("b");
        scoped.scope = Some("Rust".into());
        scoped.shortcut = Some(shortcut);
        let snippets = [general, scoped];

        // In Rust the shortcut runs the scoped snippet, so the general one does
        // not advertise it.
        let sections = Sections::of(&snippets, Some("Rust"));
        assert!(!sections.shows_shortcut(&snippets[0]));

        // Anywhere else there is nothing to shadow it.
        let sections = Sections::of(&snippets, Some("Swift"));
        assert!(sections.shows_shortcut(&snippets[0]));
        let sections = Sections::of(&snippets, None);
        assert!(sections.shows_shortcut(&snippets[0]));
    }

    /// A snippet, named and scoped, with a shortcut in the stored spelling.
    fn with_shortcut(id: u64, name: &str, scope: Option<&str>, spec: &str) -> Snippet {
        Snippet {
            id,
            name: name.into(),
            scope: scope.map(str::to_string),
            shortcut: Shortcut::parse(spec),
            format: String::new(),
        }
    }

    #[test]
    fn a_bound_shortcut_is_the_one_the_submenu_shows() {
        let snippets = [
            with_shortcut(1, "general", None, "@k"),
            with_shortcut(2, "rust", Some("Rust"), "@r"),
            with_shortcut(3, "swift", Some("Swift"), "@s"),
        ];
        let sections = Sections::of(&snippets, Some("Rust"));

        // The submenu lists the general group first, then the scoped one, and
        // the shortcuts follow that order.
        assert_eq!(
            sections.bindings(),
            [(1, "secondary-k".to_string()), (2, "secondary-r".to_string())]
        );
        assert_eq!(
            Sections::of(&snippets, None).bindings(),
            [(1, "secondary-k".to_string())]
        );
    }

    #[test]
    fn the_scoped_snippet_keeps_the_shortcut_it_shares_with_a_general_one() {
        let snippets = [
            with_shortcut(1, "general", None, "@k"),
            with_shortcut(2, "rust", Some("Rust"), "@k"),
        ];
        // In Rust the scoped snippet is what runs, so it is the one bound.
        assert_eq!(
            Sections::of(&snippets, Some("Rust")).bindings(),
            [(2, "secondary-k".to_string())]
        );
        // Everywhere else the general one is.
        assert_eq!(
            Sections::of(&snippets, Some("Swift")).bindings(),
            [(1, "secondary-k".to_string())]
        );
    }

    #[test]
    fn the_first_of_two_snippets_on_one_shortcut_keeps_it() {
        let snippets = [
            with_shortcut(1, "first", None, "@k"),
            with_shortcut(2, "second", None, "@k"),
        ];
        assert_eq!(
            Sections::of(&snippets, None).bindings(),
            [(1, "secondary-k".to_string())]
        );
    }

    #[test]
    fn a_shortcut_the_text_view_needs_is_not_bound() {
        let snippets = [
            with_shortcut(1, "tab", None, "@\t"),
            with_shortcut(2, "bare", None, "k"),
            with_shortcut(3, "fine", None, "@k"),
        ];
        assert_eq!(
            Sections::of(&snippets, None).bindings(),
            [(3, "secondary-k".to_string())]
        );
    }

    // ---- insertion -------------------------------------------------------

    /// Upstream's `Snippet.Variable`, whose tokens are what the format holds
    /// and whose names are what the *Insert Variable* menu shows.
    #[test]
    fn the_variables_are_the_tokens_a_format_holds() {
        let tokens: Vec<String> = Variable::ALL.iter().map(|variable| variable.token()).collect();
        assert_eq!(tokens, TOKENS);
        assert_eq!(Variable::Selection.name(), "SELECTION");
        assert_eq!(Variable::Cursor.name(), "CURSOR");
        assert_eq!(Variable::Selection.description(), "The selected text.");
        assert_eq!(
            Variable::Cursor.description(),
            "The insertion point after inserting the snippet."
        );
    }

    /// Upstream's `simpleSnippet`.
    #[test]
    fn a_selection_and_a_cursor_fill_one_line() {
        let insertion = snippet("<h1><<<SELECTION>>><<<CURSOR>>></h1>").insertion("abc", "");
        assert_eq!(insertion.text, "<h1>abc</h1>");
        assert_eq!(insertion.cursors, [7]);
        assert_eq!(insertion.caret(), 7);
    }

    /// Upstream's `cursorTokenInSelection`, both halves.
    #[test]
    fn a_cursor_token_in_the_selection_stays_literal() {
        // The split happens before the selection is substituted, so a token that
        // is part of the selected text is not treated as a marker.
        let insertion = snippet("<q><<<SELECTION>>></q>").insertion("a<<<CURSOR>>>b", "");
        assert_eq!(insertion.text, "<q>a<<<CURSOR>>>b</q>");
        assert!(insertion.cursors.is_empty());
        assert_eq!(insertion.caret(), insertion.text.len());

        // The format's own cursor token still works beside it.
        let insertion =
            snippet("<q><<<SELECTION>>><<<CURSOR>>></q>").insertion("x<<<CURSOR>>>y", "");
        assert_eq!(insertion.text, "<q>x<<<CURSOR>>>y</q>");
        assert_eq!(insertion.cursors, [17]);
    }

    /// Upstream's `multipleLines`.
    #[test]
    fn every_line_of_a_format_is_indented() {
        let format = "<ul>\n    <li><<<CURSOR>>></li>\n    <li><<<CURSOR>>></li>\n</ul>";

        // Without indentation the format goes in as written, minus the cursor
        // tokens — those are markers, not text.
        let insertion = snippet(format).insertion("", "");
        assert_eq!(insertion.text, "<ul>\n    <li></li>\n    <li></li>\n</ul>");
        assert_eq!(insertion.cursors, [13, 27]);

        // With it, every line break gains the document's indentation — the
        // lines the format already indented included, which is what makes a
        // snippet land inside a block.
        let insertion = snippet(format).insertion("", "    ");
        assert_eq!(
            insertion.text,
            "<ul>\n        <li></li>\n        <li></li>\n    </ul>"
        );
        assert_eq!(insertion.cursors, [17, 35]);
    }

    #[test]
    fn a_carriage_return_pair_is_one_line_break() {
        let insertion = snippet("a\r\nb\rc\u{85}d\u{2028}e").insertion("", "  ");
        assert_eq!(insertion.text, "a\r\n  b\r  c\u{85}  d\u{2028}  e");
    }

    #[test]
    fn a_format_without_a_cursor_leaves_the_caret_after_the_text() {
        let insertion = snippet("<b><<<SELECTION>>></b>").insertion("x", "");
        assert_eq!(insertion.text, "<b>x</b>");
        assert_eq!(insertion.caret(), 8);
    }

    #[test]
    fn a_selection_holding_a_selection_token_is_not_expanded() {
        let insertion = snippet("[<<<SELECTION>>>]").insertion("<<<SELECTION>>>", "");
        assert_eq!(insertion.text, "[<<<SELECTION>>>]");
    }

    // ---- indentation -----------------------------------------------------

    #[test]
    fn the_indent_is_the_leading_run_of_the_line() {
        let text = "fn main() {\n    let x = 1;\n\tlet y = 2;\n}";
        assert_eq!(indent_at(text, 0), "");
        assert_eq!(indent_at(text, 15), "    ");
        assert_eq!(indent_at(text, 31), "\t");
        assert_eq!(indent_at(text, text.len()), "");
    }

    #[test]
    fn the_indent_is_empty_on_a_line_that_starts_flush() {
        let text = "aaa\n  bbb\nccc";
        assert_eq!(indent_at(text, 7), "  ");
        assert_eq!(indent_at(text, 10), "");
        // The newline itself ends the line it belongs to.
        assert_eq!(indent_at(text, 3), "");
    }

    #[test]
    fn an_offset_inside_a_character_is_clamped() {
        let text = "  é";
        assert_eq!(indent_at(text, 3), "  ");
        assert_eq!(indent_at(text, 99), "  ");
    }

    #[test]
    fn an_indent_range_is_none_when_the_line_starts_flush() {
        let text = "  aaa\nbbb";
        assert_eq!(indent_range_at(text, 2), Some(0..2));
        assert_eq!(indent_range_at(text, 6), None);
    }
}
