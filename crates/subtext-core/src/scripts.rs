//! The user's scripts, and the menu they become.
//!
//! CotEditor keeps its scripts in a folder and turns that folder into a menu:
//! every file it recognises as a script becomes an item, every folder becomes a
//! submenu, and a file called `-` becomes a separator. Nothing here is
//! CotEditor-specific — a script is a file the operating system knows how to run
//! — but the *shape* of the menu is, so it is reproduced item for item.
//!
//! # Where the platform boundary is
//!
//! Two of the things upstream supports are macOS only, and this module says so
//! rather than pretending otherwise:
//!
//! *AppleScript* — `.applescript`, `.scpt`, `.scptd` — needs the Apple event
//! runtime. [`ScriptKind::is_available`] is `false` for it everywhere else, so
//! [`scan`] leaves those files out of the menu rather than offering an item that
//! cannot run.
//!
//! *The shebang* — a `.sh` file runs because its first line names an
//! interpreter, which Windows does not read. [`Script::launch`] hands the file
//! to an interpreter there instead.
//!
//! # The script's own source decides what it does
//!
//! A script may ask for the document's text and say where its answer goes, in
//! the two `%%%{CotEditorX…}%%%` tokens upstream defines. [`Invocation::scan`]
//! reads them. Applying the answer is the application's business, because it
//! needs a window; the arithmetic is here, where it can be tested.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::snippets::Shortcut;

/// What kind of thing a script file is.
///
/// Upstream decides by asking the file's uniform type identifier whether it
/// conforms to one of two lists, which also picks up anything a user has
/// declared to conform. There is no type database to ask off macOS, so the
/// extension is the whole answer — the same eight and three upstream names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    /// An AppleScript, run through the Apple event runtime: `.applescript`,
    /// `.scpt`, `.scptd`.
    AppleScript,
    /// A script whose first line names its interpreter: `.sh`, `.py` and the
    /// rest of upstream's list.
    Unix,
}

impl ScriptKind {
    /// The kind of script `path` is, or `None` when it is not one.
    ///
    /// Case is folded: `.SH` is a shell script on a case-insensitive filesystem
    /// and there is no reason for it to stop being one here.
    pub fn of_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "applescript" | "scpt" | "scptd" => Some(Self::AppleScript),
            "sh" | "pl" | "php" | "rb" | "py" | "js" | "awk" | "swift" => Some(Self::Unix),
            _ => None,
        }
    }

    /// Whether this build can run a script of this kind at all.
    ///
    /// AppleScript is the Apple event runtime and nothing else; a `.scpt` is
    /// compiled bytecode for a system that is not there on the other two
    /// platforms, so the file is left out of the menu rather than offered as an
    /// item that would fail when chosen.
    pub fn is_available(self) -> bool {
        match self {
            Self::Unix => true,
            Self::AppleScript => cfg!(target_os = "macos"),
        }
    }
}

/// How the operating system starts a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    /// Run the file itself. Its first line names the interpreter, which is what
    /// macOS and Linux both honour.
    File,
    /// Hand the file to a program.
    Program(&'static str),
}

/// The program macOS runs an AppleScript with.
const OSASCRIPT: &str = "osascript";

/// One script file, as the menu needs to see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    /// Where the file is. This is also the script's identity: it is what the
    /// menu item carries and what the command is handed back.
    pub path: PathBuf,
    /// What the menu calls it.
    pub name: String,
    pub kind: ScriptKind,
    /// The key the script answers to, read out of its file name.
    pub shortcut: Option<Shortcut>,
}

impl Script {
    /// The script at `path`, called `name`, or `None` when the file is not one.
    ///
    /// `name` is what [`scan`] worked out for the file — its stem, less any
    /// ordering prefix. The shortcut is read out of the extension that is left
    /// once the script's own is taken off, so `Save.@s.sh` is Command-S and
    /// `Save.sh` is just a name. A combination that could never be pressed is
    /// not a shortcut, which is what keeps `backup.old.sh` from becoming a menu
    /// item bound to nothing.
    pub fn of_path(path: &Path, name: &str) -> Option<Self> {
        let kind = ScriptKind::of_path(path)?;
        if !kind.is_available() {
            return None;
        }

        let mut name = name.to_string();
        let shortcut = path
            .file_stem()
            .and_then(|stem| Path::new(stem).extension())
            .and_then(|spec| spec.to_str())
            .and_then(Shortcut::parse)
            .filter(|shortcut| shortcut.rejection().is_none());
        if shortcut.is_some() {
            // The shortcut is part of the name the file was *written* with, not
            // part of what the user reads: `Save.@s.sh` is the item "Save".
            name.truncate(name.find('.').unwrap_or(name.len()));
        }

        Some(Self {
            path: path.to_path_buf(),
            name,
            kind,
            shortcut,
        })
    }

    /// How this script is started here.
    pub fn launch(&self) -> Launch {
        match self.kind {
            ScriptKind::AppleScript => Launch::Program(OSASCRIPT),
            ScriptKind::Unix => self.interpreter().map_or(Launch::File, Launch::Program),
        }
    }

    /// The interpreter Windows has to be told about, or `None` elsewhere.
    ///
    /// A shebang is a Unix convention. Windows decides what runs a file from the
    /// registry, which is not something a file dropped in this folder can rely
    /// on, so the interpreter is named here instead — each under the name its
    /// own installer puts on the path.
    fn interpreter(&self) -> Option<&'static str> {
        if !cfg!(windows) {
            return None;
        }
        let extension = self.path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "sh" => Some("sh"),
            "pl" => Some("perl"),
            "php" => Some("php"),
            "rb" => Some("ruby"),
            "py" => Some("python"),
            "js" => Some("node"),
            "awk" => Some("awk"),
            "swift" => Some("swift"),
            _ => None,
        }
    }
}

/// One entry of the Script menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Script(Script),
    Folder { name: String, items: Vec<Item> },
    /// A file called `-`, which is how upstream lets a folder of scripts be
    /// divided into groups without a submenu.
    Separator,
}

impl Item {
    /// Every script at or below this item, in menu order.
    pub fn scripts(&self) -> Vec<&Script> {
        match self {
            Self::Script(script) => vec![script],
            Self::Folder { items, .. } => items.iter().flat_map(Self::scripts).collect(),
            Self::Separator => Vec::new(),
        }
    }

    /// The script at `path`, if this item holds it.
    pub fn find(&self, path: &Path) -> Option<&Script> {
        self.scripts().into_iter().find(|script| script.path == path)
    }
}

/// The entries the Script menu holds for the scripts in `directory`.
///
/// Upstream's `scriptMenuItems(at:)`, with the menu objects taken out. The rules
/// are all its own:
///
/// * hidden files are skipped, and so is anything whose name starts with `_` —
///   how a script that is only meant to be sourced is filed next to the ones
///   meant to be run;
/// * the entries are sorted by file name, extension and all;
/// * a leading `12)` is dropped from the name, which is how a folder of scripts
///   is put in an order the alphabet does not give it;
/// * a file called `-` is a separator;
/// * a script is asked about before a folder is, because a script *can* be a
///   folder — `.scptd` is one;
/// * a folder becomes a submenu, scanned by the same rules.
pub fn scan(directory: &Path) -> Vec<Item> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        // A folder that is not there yet is an empty menu rather than an error:
        // it is the state every user starts in, and the Script menu still has
        // its way to the folder.
        return Vec::new();
    };

    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| !is_hidden(path) && !is_ignored(path))
        .collect();
    files.sort_by_key(|path| file_name(path));

    files.iter().filter_map(|path| entry(path)).collect()
}

/// One entry of a folder, or `None` when the file is neither a script nor a
/// folder.
fn entry(path: &Path) -> Option<Item> {
    let name = menu_name(path);
    if name == "-" {
        return Some(Item::Separator);
    }
    if let Some(script) = Script::of_path(path, &name) {
        return Some(Item::Script(script));
    }
    if path.is_dir() {
        return Some(Item::Folder {
            name,
            items: scan(path),
        });
    }
    None
}

/// Whether the entry is a dotfile, which upstream's `skipsHiddenFiles` leaves
/// out.
fn is_hidden(path: &Path) -> bool {
    file_name(path).starts_with('.')
}

/// Whether the user has asked for the entry to be kept out of the menu.
fn is_ignored(path: &Path) -> bool {
    file_name(path).starts_with('_')
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The name the menu shows for the entry at `path`.
///
/// The script's own extension goes first, then a leading `12)` — upstream's
/// ordering prefix.
fn menu_name(path: &Path) -> String {
    static ORDER_PREFIX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\d+\)").expect("the ordering prefix is a valid pattern"));

    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    ORDER_PREFIX.replace(&stem, "").into_owned()
}

// ---- what a script asks for ----------------------------------------------

/// The document text a script wants on its standard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    Selection,
    CurrentLine,
    AllText,
}

/// Where a script's standard output goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputType {
    ReplaceSelection,
    ReplaceCurrentLine,
    ReplaceAllText,
    InsertAfterSelection,
    AppendToAllText,
    NewDocument,
    Pasteboard,
}

impl OutputType {
    /// The range of `text` this output replaces, or `None` when it replaces
    /// nothing.
    ///
    /// Upstream's `range(in:selectedRange:)`. The two that replace nothing — a
    /// new document and the clipboard — have no range at all, which is what the
    /// `None` says.
    pub fn range(self, text: &str, selection: &Range<usize>) -> Option<Range<usize>> {
        Some(match self {
            Self::ReplaceSelection => selection.clone(),
            Self::ReplaceCurrentLine => line_range(text, selection.clone())?,
            Self::ReplaceAllText => 0..text.len(),
            Self::InsertAfterSelection => selection.start..selection.start,
            Self::AppendToAllText => text.len()..text.len(),
            Self::NewDocument | Self::Pasteboard => return None,
        })
    }
}

/// The range of the line `range` sits on, its newline included.
///
/// `NSString.lineRange(for:)`, which is what upstream asks for the current line:
/// the text from the start of that line to the start of the next one. A caret
/// on a newline belongs to the line the newline ends, because that is the line
/// the caret is drawn at the end of.
pub fn line_range(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let start = text
        .get(..range.start)?
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let end = text
        .get(range.end..)?
        .find('\n')
        .map_or(text.len(), |index| range.end + index + 1);
    Some(start..end)
}

/// What a script's own source asks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Invocation {
    /// The document text to hand it, or `None` for a script that reads nothing.
    pub input: Option<InputType>,
    /// Where to put its answer, or `None` for one that is run for its effect.
    pub output: Option<OutputType>,
}

impl Invocation {
    /// Read the two tokens out of `source`.
    ///
    /// The tokens are upstream's, down to the delimiters:
    /// `%%%{CotEditorXInput=Selection}%%%` and
    /// `%%%{CotEditorXOutput=ReplaceSelection}%%%`. Only the first of each is
    /// read, which is what upstream's `firstMatch` does — a token quoted in a
    /// comment above the one that counts is still the one that counts.
    pub fn scan(source: &str) -> Self {
        Self {
            input: token(&INPUT_TOKEN, source).and_then(|word| match word {
                "Selection" => Some(InputType::Selection),
                "CurrentLine" => Some(InputType::CurrentLine),
                "AllText" => Some(InputType::AllText),
                _ => None,
            }),
            output: token(&OUTPUT_TOKEN, source).and_then(|word| match word {
                "ReplaceSelection" => Some(OutputType::ReplaceSelection),
                "ReplaceCurrentLine" => Some(OutputType::ReplaceCurrentLine),
                "ReplaceAllText" => Some(OutputType::ReplaceAllText),
                "InsertAfterSelection" => Some(OutputType::InsertAfterSelection),
                "AppendToAllText" => Some(OutputType::AppendToAllText),
                "NewDocument" => Some(OutputType::NewDocument),
                "Pasteboard" => Some(OutputType::Pasteboard),
                _ => None,
            }),
        }
    }
}

static INPUT_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"%%%\{CotEditorXInput=(\w+)\}%%%").expect("the input token is a valid pattern")
});

static OUTPUT_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"%%%\{CotEditorXOutput=(\w+)\}%%%").expect("the output token is a valid pattern")
});

/// The word inside the first `pattern` match, if there is one.
fn token<'a>(pattern: &Regex, source: &'a str) -> Option<&'a str> {
    pattern.captures(source).map(|captures| {
        captures
            .get(1)
            .expect("the pattern captures one word")
            .as_str()
    })
}

// ---- shortcuts ------------------------------------------------------------

/// The scripts that carry a usable shortcut, each with the key it stands for.
///
/// Upstream's `applyShortcuts()`: the submenu named after the document's syntax
/// goes first, so a script filed under that language keeps a key another script
/// would otherwise take, and then the rest are walked in menu order. Of two
/// scripts that want the same key, the one listed first keeps it — which is how
/// macOS settles a duplicate too, by menu order.
///
/// The caller still has to check each key against the application's own menu, so
/// that a script cannot take a key ⌘S already answers to.
pub fn bindings(items: &[Item], scope: Option<&str>) -> Vec<(PathBuf, String)> {
    let mut bound: Vec<(PathBuf, String)> = Vec::new();

    // Upstream applies the shortcuts of the submenu named after the document's
    // syntax first, so a script filed under that language wins a key another
    // script would otherwise take. With no such submenu it applies the top
    // level only — upstream's `recursively: false` — because a folder's name is
    // not what the document is highlighted as and so gets no say.
    match scope.and_then(|scope| folder(items, scope)) {
        Some(scoped) => take(scoped, true, &mut bound),
        None => take(items, false, &mut bound),
    }
    take(items, true, &mut bound);
    bound
}

/// The items of the submenu called `name`, if the bar has one.
fn folder<'a>(items: &'a [Item], name: &str) -> Option<&'a [Item]> {
    items.iter().find_map(|item| match item {
        Item::Folder { name: folder, items } if folder == name => Some(items.as_slice()),
        _ => None,
    })
}

/// Take the shortcuts of `items`, in menu order, into `bound`.
fn take(items: &[Item], recursively: bool, bound: &mut Vec<(PathBuf, String)>) {
    for item in items {
        match item {
            Item::Script(script) => bind(script, bound),
            Item::Folder { items, .. } if recursively => take(items, true, bound),
            _ => {}
        }
    }
}

/// Give `script` its key, unless something already took it.
fn bind(script: &Script, bound: &mut Vec<(PathBuf, String)>) {
    let Some(shortcut) = script.shortcut else {
        return;
    };
    if shortcut.rejection().is_some() {
        return;
    }
    let keystroke = shortcut.keystroke();
    if bound.iter().any(|(_, taken)| *taken == keystroke) {
        return;
    }
    bound.push((script.path.clone(), keystroke));
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    /// A folder of our own, emptied when it goes out of scope.
    struct Fixture(PathBuf);

    impl Fixture {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "subtext-scripts-{}-{name}",
                std::process::id()
            ));
            fs::remove_dir_all(&path).ok();
            fs::create_dir_all(&path).expect("the fixture folder is creatable");
            Self(path)
        }

        fn file(&self, name: &str) -> &Self {
            fs::write(self.0.join(name), "#!/bin/sh\n").expect("the fixture file is writable");
            self
        }

        fn folder(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(&path).expect("the fixture folder is creatable");
            path
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    /// The names the menu would show, with separators marked and folders in
    /// parentheses, so the shape of the list is visible.
    fn shape(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                Item::Script(script) => script.name.clone(),
                Item::Folder { name, items } => format!("({name}: {})", shape(items).join(", ")),
                Item::Separator => "―".to_string(),
            })
            .collect()
    }

    /// The one script in a fixture of one.
    fn only(items: &[Item]) -> Script {
        let mut scripts = items.iter().flat_map(Item::scripts);
        let script = scripts.next().expect("the fixture holds a script").clone();
        assert!(scripts.next().is_none(), "the fixture holds more than one");
        script
    }

    #[test]
    fn the_extension_decides_what_a_script_is() {
        assert_eq!(
            ScriptKind::of_path(Path::new("a.sh")),
            Some(ScriptKind::Unix)
        );
        assert_eq!(
            ScriptKind::of_path(Path::new("a.PY")),
            Some(ScriptKind::Unix),
            "the extension is case-folded"
        );
        assert_eq!(
            ScriptKind::of_path(Path::new("a.scptd")),
            Some(ScriptKind::AppleScript)
        );
        assert_eq!(ScriptKind::of_path(Path::new("a.txt")), None);
        assert_eq!(ScriptKind::of_path(Path::new("a")), None);
    }

    /// The eight upstream calls unix scripts, and the three it calls AppleScript.
    #[test]
    fn upstream_s_extensions_are_the_ones_recognised() {
        for extension in ["sh", "pl", "php", "rb", "py", "js", "awk", "swift"] {
            assert_eq!(
                ScriptKind::of_path(&PathBuf::from(format!("a.{extension}"))),
                Some(ScriptKind::Unix),
                "{extension}"
            );
        }
        for extension in ["applescript", "scpt", "scptd"] {
            assert_eq!(
                ScriptKind::of_path(&PathBuf::from(format!("a.{extension}"))),
                Some(ScriptKind::AppleScript),
                "{extension}"
            );
        }
    }

    /// A `.scpt` is bytecode for a system that is not there, so it is left out
    /// rather than offered as an item that would fail when chosen.
    #[test]
    fn a_script_the_platform_cannot_run_is_not_a_script() {
        let apple_script = Script::of_path(Path::new("a.scpt"), "a");
        assert_eq!(
            apple_script.is_some(),
            cfg!(target_os = "macos"),
            "AppleScript is offered exactly where it can run"
        );
        assert!(Script::of_path(Path::new("a.sh"), "a").is_some());
    }

    /// The key is the extension left once the script's own is taken off.
    #[test]
    fn the_shortcut_comes_out_of_the_file_name() {
        let script = Script::of_path(Path::new("Save.@s.sh"), "Save.@s").unwrap();
        assert_eq!(script.name, "Save");
        assert_eq!(script.shortcut.map(Shortcut::keystroke).as_deref(), Some("secondary-s"));
    }

    /// A word spelling and the symbols macOS shows are read too, because the
    /// shortcut is a file name and nobody agrees on how to write one.
    #[test]
    fn the_shortcut_can_be_written_the_way_a_person_writes_it() {
        for (file, name) in [
            ("Save.@$s.sh", "Save.@$s"),
            ("Save.⌘⇧S.sh", "Save.⌘⇧S"),
            ("Save.cmd-shift-s.sh", "Save.cmd-shift-s"),
        ] {
            let script = Script::of_path(Path::new(file), name).unwrap();
            assert_eq!(script.name, "Save", "{file}");
            assert_eq!(
                script.shortcut.map(Shortcut::keystroke).as_deref(),
                Some("secondary-shift-s"),
                "{file}"
            );
        }
    }

    /// A combination that could never be pressed is not a shortcut, so a file
    /// called `backup.old.sh` keeps its whole name.
    #[test]
    fn something_that_is_not_a_shortcut_is_part_of_the_name() {
        let script = Script::of_path(Path::new("backup.old.sh"), "backup.old").unwrap();
        assert_eq!(script.name, "backup.old");
        assert_eq!(script.shortcut, None);
    }

    /// The list is the folder, filtered and sorted the way upstream filters and
    /// sorts it.
    #[test]
    fn the_folder_becomes_the_menu() {
        let fixture = Fixture::new("folder");
        fixture
            .file("2)Second.sh")
            .file("1)First.sh")
            .file("-.sh")
            .file("_helper.sh")
            .file(".hidden.sh")
            .file("notes.txt");
        let nested = fixture.folder("Tools");
        fs::write(nested.join("Clean.py"), "#!/usr/bin/env python\n").unwrap();

        assert_eq!(
            shape(&scan(fixture.path())),
            ["―", "First", "Second", "(Tools: Clean)"],
            "sorted by file name, with `-` a separator and the hidden and underscored gone"
        );
    }

    /// A script can be a folder — `.scptd` is one — so it is asked about before
    /// the directory question.
    #[test]
    fn a_script_that_is_a_folder_is_a_script() {
        let fixture = Fixture::new("bundle");
        fixture.folder("Action.scptd");
        let items = scan(fixture.path());

        if cfg!(target_os = "macos") {
            assert_eq!(shape(&items), ["Action"], "the bundle is an item, not a submenu");
            assert_eq!(
                only(&items).kind,
                ScriptKind::AppleScript,
                "and it is an AppleScript"
            );
        } else {
            assert_eq!(
                shape(&items),
                ["(Action: )"],
                "without the Apple event runtime the bundle is only a folder"
            );
        }
    }

    /// A folder with nothing to offer is still a submenu, because the user put
    /// it there.
    #[test]
    fn an_empty_folder_is_still_a_submenu() {
        let fixture = Fixture::new("empty-folder");
        fixture.folder("Empty");
        assert_eq!(shape(&scan(fixture.path())), ["(Empty: )"]);
    }

    /// A folder that is not there is an empty menu, not a failure.
    #[test]
    fn a_missing_folder_is_empty() {
        assert!(scan(Path::new("/nonexistent/subtext/scripts")).is_empty());
    }

    #[test]
    fn the_ordering_prefix_is_dropped() {
        assert_eq!(menu_name(Path::new("12)Sort.sh")), "Sort");
        assert_eq!(
            menu_name(Path::new("12) Sort.sh")),
            " Sort",
            "only the prefix goes: upstream leaves the space the user typed"
        );
        assert_eq!(menu_name(Path::new("7- Eleven.sh")), "7- Eleven");
        assert_eq!(menu_name(Path::new("Plain.sh")), "Plain");
    }

    #[test]
    fn the_two_tokens_are_read_out_of_the_source() {
        let source = "#!/bin/sh\n%%%{CotEditorXInput=Selection}%%%\n%%%{CotEditorXOutput=ReplaceSelection}%%%\ncat\n";
        assert_eq!(
            Invocation::scan(source),
            Invocation {
                input: Some(InputType::Selection),
                output: Some(OutputType::ReplaceSelection),
            }
        );
    }

    /// Only the first of each is read, which is what upstream's `firstMatch`
    /// does.
    #[test]
    fn the_first_token_of_each_kind_is_the_one_that_counts() {
        let source = "%%%{CotEditorXInput=AllText}%%%\n%%%{CotEditorXInput=Selection}%%%\n";
        assert_eq!(Invocation::scan(source).input, Some(InputType::AllText));
    }

    #[test]
    fn a_script_that_asks_for_nothing_asks_for_nothing() {
        assert_eq!(Invocation::scan("#!/bin/sh\necho hi\n"), Invocation::default());
        assert_eq!(
            Invocation::scan("%%%{CotEditorXInput=Clipboard}%%%").input,
            None,
            "a word upstream does not define is not an input"
        );
    }

    /// Every output type upstream names is one this reads.
    #[test]
    fn every_output_type_upstream_names_is_read() {
        for (word, expected) in [
            ("ReplaceSelection", OutputType::ReplaceSelection),
            ("ReplaceCurrentLine", OutputType::ReplaceCurrentLine),
            ("ReplaceAllText", OutputType::ReplaceAllText),
            ("InsertAfterSelection", OutputType::InsertAfterSelection),
            ("AppendToAllText", OutputType::AppendToAllText),
            ("NewDocument", OutputType::NewDocument),
            ("Pasteboard", OutputType::Pasteboard),
        ] {
            let source = format!("%%%{{CotEditorXOutput={word}}}%%%");
            assert_eq!(Invocation::scan(&source).output, Some(expected), "{word}");
        }
    }

    #[test]
    fn each_output_type_replaces_the_range_it_names() {
        let text = "first\nsecond\nthird";
        let caret = 8..8;

        assert_eq!(OutputType::ReplaceSelection.range(text, &(2..5)), Some(2..5));
        assert_eq!(
            OutputType::ReplaceCurrentLine.range(text, &caret),
            Some(6..13)
        );
        assert_eq!(OutputType::ReplaceAllText.range(text, &caret), Some(0..18));
        assert_eq!(
            OutputType::InsertAfterSelection.range(text, &caret),
            Some(8..8)
        );
        assert_eq!(
            OutputType::AppendToAllText.range(text, &caret),
            Some(18..18)
        );
        assert_eq!(OutputType::NewDocument.range(text, &caret), None);
        assert_eq!(OutputType::Pasteboard.range(text, &caret), None);
    }

    /// A caret on a newline belongs to the line the newline ends, because that
    /// is the line it is drawn at the end of.
    #[test]
    fn a_caret_on_a_newline_belongs_to_the_line_before_it() {
        let text = "first\nsecond";
        assert_eq!(line_range(text, 5..5), Some(0..6));
        assert_eq!(line_range(text, 6..6), Some(6..12));
        assert_eq!(line_range(text, 0..0), Some(0..6));
    }

    #[test]
    fn the_shortcuts_are_the_scripts_that_have_one() {
        let fixture = Fixture::new("bindings");
        fixture
            .file("Save.@s.sh")
            .file("Plain.sh")
            .file("Sort.@$o.sh");
        let items = scan(fixture.path());

        assert_eq!(
            bindings(&items, None)
                .into_iter()
                .map(|(path, keystroke)| (
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    keystroke
                ))
                .collect::<Vec<_>>(),
            [
                ("Save.@s.sh".to_string(), "secondary-s".to_string()),
                ("Sort.@$o.sh".to_string(), "secondary-shift-o".to_string()),
            ]
        );
    }

    /// A script filed under the document's syntax keeps its key, and the one
    /// that wanted it too loses it.
    #[test]
    fn the_folder_named_after_the_syntax_gets_the_keys_first() {
        let fixture = Fixture::new("scope");
        fixture.file("Zebra.@s.sh");
        let scoped = fixture.folder("Python");
        fs::write(scoped.join("Nested.@s.sh"), "#!/bin/sh\n").unwrap();

        let items = scan(fixture.path());
        let names = |scope: Option<&str>| {
            bindings(&items, scope)
                .into_iter()
                .map(|(path, keystroke)| {
                    (
                        path.file_name().unwrap().to_string_lossy().into_owned(),
                        keystroke,
                    )
                })
                .collect::<Vec<_>>()
        };

        assert_eq!(
            names(Some("Python")),
            [("Nested.@s.sh".to_string(), "secondary-s".to_string())],
            "the script under the document's syntax is the one that gets the key"
        );
        assert_eq!(
            names(None),
            [("Zebra.@s.sh".to_string(), "secondary-s".to_string())],
            "with no syntax to prefer, only the top level is asked first"
        );
    }

    /// Of two scripts that want the same key, the one listed first keeps it.
    #[test]
    fn the_first_script_listed_keeps_a_contested_key() {
        let fixture = Fixture::new("contested");
        fixture.file("Alpha.@s.sh").file("Beta.@s.sh");
        let items = scan(fixture.path());

        assert_eq!(
            bindings(&items, None)
                .into_iter()
                .map(|(path, keystroke)| (
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    keystroke
                ))
                .collect::<Vec<_>>(),
            [("Alpha.@s.sh".to_string(), "secondary-s".to_string())],
            "Beta wants the same key and does not get it"
        );
    }
}
