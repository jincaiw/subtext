//! Reading `.cotsyntax` bundles and choosing one for a file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::highlight::{HighlightRule, HighlightToken, RawRule};

/// The `Info.json` file map.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMap {
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub interpreters: Vec<String>,
    #[serde(default)]
    pub filenames: Vec<String>,
    #[serde(default)]
    pub file_name_globs: Vec<String>,
}

/// How CotEditor categorises a syntax.
///
/// `Syntax.Kind` upstream, and it has exactly these two cases. An earlier
/// revision spelled them `code` / `markup` / `none`, which meant the seven
/// bundles that say `"kind": "general"` — Markdown, LaTeX, BibTeX, BBCode and
/// Plain Text among them — failed to deserialize *at all*. `read_json` treats a
/// file it cannot parse as absent, so those syntaxes loaded with no file map,
/// and a `.md` file opened as Plain Text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyntaxKind {
    #[default]
    General,
    Code,
}

/// Read leniently: anything that is not `code` is general.
///
/// The alternative is what caused the bug above — one unknown value taking the
/// whole `Info.json` with it, and with it the file map, the metadata and the
/// syntax's claim on its extensions. A category is a label for grouping, and a
/// label this build has not heard of is no reason to lose the rest.
impl<'de> Deserialize<'de> for SyntaxKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "code" => Self::Code,
            _ => Self::General,
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SyntaxMetadata {
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(rename = "lastModified", default)]
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoFile {
    #[serde(default)]
    pub file_map: FileMap,
    #[serde(default)]
    pub kind: SyntaxKind,
    #[serde(default)]
    pub metadata: SyntaxMetadata,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelimiterPair {
    pub begin: String,
    #[serde(default)]
    pub end: Option<String>,
    /// A block comment whose markers may nest.
    #[serde(default)]
    pub is_nestable: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CommentRules {
    #[serde(default)]
    pub blocks: Vec<DelimiterPair>,
    #[serde(default)]
    pub inlines: Vec<DelimiterPair>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Indentation {
    #[serde(default)]
    pub block_delimiters: Vec<DelimiterPair>,
}

/// A string delimiter, with the optional prefixes that make it a string.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StringDelimiter {
    pub begin: String,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub escape_character: Option<String>,
    #[serde(default)]
    pub is_multiline: bool,
    #[serde(default)]
    pub prefixes: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditFile {
    #[serde(default)]
    pub comment: CommentRules,
    #[serde(default)]
    pub indentation: Indentation,
    #[serde(default)]
    pub string_delimiters: Vec<StringDelimiter>,
    /// Single-character literals, e.g. Rust's `'a'`. CotEditor keeps these
    /// apart from strings because they cannot span lines.
    #[serde(default)]
    pub character_delimiters: Vec<StringDelimiter>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompletionItem {
    pub text: String,
    #[serde(rename = "type", default)]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutlineItem {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub kind: String,
    pub pattern: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub ignore_case: bool,
}

/// One syntax definition, as read from disk.
#[derive(Debug, Clone)]
pub struct Syntax {
    pub name: String,
    pub directory: PathBuf,
    pub info: InfoFile,
    pub edit: EditFile,
    /// Highlighting rules, in the order CotEditor applies them.
    ///
    /// Always populated with the comment, string and character delimiters
    /// from `Edit.json`. For the regex-highlighted languages it also carries
    /// the keyword/number/type rules from `Highlights.json`.
    pub highlights: BTreeMap<HighlightToken, Vec<HighlightRule>>,
    /// Whether the bundle ships a `Highlights.json`.
    ///
    /// `false` means CotEditor highlights this language with tree-sitter, so
    /// the regex rules here only cover comments and literals.
    pub has_highlight_file: bool,
    pub outlines: Vec<OutlineItem>,
    pub completions: Vec<CompletionItem>,
}

impl Syntax {
    /// Read a `.cotsyntax` bundle.
    pub fn load(directory: impl AsRef<Path>) -> std::io::Result<Self> {
        let directory = directory.as_ref().to_path_buf();
        let name = directory
            .file_name()
            .map(|n| {
                n.to_string_lossy()
                    .trim_end_matches(".cotsyntax")
                    .to_string()
            })
            .unwrap_or_else(|| "Plain Text".to_string());

        let info: InfoFile = read_json(directory.join("Info.json"))?.unwrap_or_default();
        let edit: EditFile = read_json(directory.join("Edit.json"))?.unwrap_or_default();

        // CotEditor stores regex-based rules under `Regex/`; the plain files
        // sit at the bundle root. Prefer whichever exists.
        let highlights: BTreeMap<String, Vec<RawRule>> =
            read_json(directory.join("Regex/Highlights.json"))?
                .or(read_json(directory.join("Highlights.json"))?)
                .unwrap_or_default();
        let has_highlight_file = !highlights.is_empty();
        let outlines: Vec<OutlineItem> = read_json(directory.join("Regex/Outlines.json"))?
            .or(read_json(directory.join("Outlines.json"))?)
            .unwrap_or_default();
        let completions: Vec<CompletionItem> =
            read_json(directory.join("Completion.json"))?.unwrap_or_default();

        // Compile every rule once; a rule with a bad pattern is dropped rather
        // than taking the whole syntax down with it.
        let mut compiled: BTreeMap<HighlightToken, Vec<HighlightRule>> = BTreeMap::new();
        for (key, rules) in highlights {
            let Some(token) = HighlightToken::from_key(&key) else {
                continue;
            };
            let usable = rules
                .into_iter()
                .filter_map(|rule| HighlightRule::compile(rule).ok());
            compiled.entry(token).or_default().extend(usable);
        }

        // `Edit.json` supplies comments, strings and character literals. They
        // are editing structures first, but CotEditor feeds them to the
        // highlighter too — which is why `comments` appears in only one of the
        // seventy highlight files, and `strings` in a handful.
        for pair in &edit.comment.inlines {
            if let Ok(rule) = HighlightRule::inline_comment(&pair.begin) {
                compiled
                    .entry(HighlightToken::Comments)
                    .or_default()
                    .push(rule);
            }
        }
        for pair in &edit.comment.blocks {
            if let Some(end) = &pair.end
                && let Ok(rule) = HighlightRule::delimited(
                    &pair.begin,
                    end,
                    None,
                    true,
                    pair.is_nestable,
                    Vec::new(),
                )
            {
                compiled
                    .entry(HighlightToken::Comments)
                    .or_default()
                    .push(rule);
            }
        }
        for delimiter in &edit.string_delimiters {
            if let Some(end) = &delimiter.end
                && let Ok(rule) = HighlightRule::delimited(
                    &delimiter.begin,
                    end,
                    delimiter
                        .escape_character
                        .as_deref()
                        .and_then(|e| e.chars().next()),
                    delimiter.is_multiline,
                    false,
                    delimiter.prefixes.clone(),
                )
            {
                compiled
                    .entry(HighlightToken::Strings)
                    .or_default()
                    .push(rule);
            }
        }
        for delimiter in &edit.character_delimiters {
            if let Some(end) = &delimiter.end
                && let Ok(rule) = HighlightRule::delimited(
                    &delimiter.begin,
                    end,
                    delimiter
                        .escape_character
                        .as_deref()
                        .and_then(|e| e.chars().next()),
                    false,
                    false,
                    delimiter.prefixes.clone(),
                )
            {
                compiled
                    .entry(HighlightToken::Characters)
                    .or_default()
                    .push(rule);
            }
        }

        Ok(Self {
            name,
            directory,
            info,
            edit,
            highlights: compiled,
            has_highlight_file,
            outlines,
            completions,
        })
    }

    /// The tree-sitter outline query for this syntax, when it has one.
    ///
    /// CotEditor keeps the queries beside the syntax bundles rather than inside
    /// them — `resources/queries/<Name>/outline.scm` — so this reads one level
    /// up from the `syntaxes` directory. A syntax the user installed in their
    /// own folder has no `queries` directory next to it and gets no outline,
    /// which is also what CotEditor does: the queries are not part of a
    /// `.cotsyntax` bundle.
    pub fn outline_query(&self) -> Option<String> {
        let queries = self.directory.parent()?.parent()?.join("queries");
        std::fs::read_to_string(queries.join(&self.name).join("outline.scm")).ok()
    }

    /// A bare Plain Text syntax, with no rules at all.
    ///
    /// Used when the resource directory is missing or empty. Without it the
    /// application would have nothing to fall back on, and an editor that
    /// cannot open its own window because a data directory is absent is worse
    /// than one that opens with plain, unhighlighted text.
    pub fn plain_text() -> Self {
        Self {
            name: "Plain Text".to_string(),
            directory: PathBuf::new(),
            info: InfoFile::default(),
            edit: EditFile::default(),
            highlights: BTreeMap::new(),
            has_highlight_file: false,
            outlines: Vec::new(),
            completions: Vec::new(),
        }
    }

    /// The inline and block comment markers for this syntax.
    pub fn comment_delimiters(&self) -> subtext_core::CommentDelimiters {
        let inline = self.edit.comment.inlines.first().map(|p| p.begin.clone());
        let block = self
            .edit
            .comment
            .blocks
            .first()
            .and_then(|p| p.end.clone().map(|end| (p.begin.clone(), end)));
        subtext_core::CommentDelimiters { inline, block }
    }

    /// Whether `path` looks like a file this syntax handles.
    pub fn matches_path(&self, path: &Path) -> bool {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if self
            .info
            .file_map
            .filenames
            .iter()
            .any(|f| f.to_lowercase() == name)
        {
            return true;
        }
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        !extension.is_empty()
            && self
                .info
                .file_map
                .extensions
                .iter()
                .any(|e| e.trim_start_matches('.').to_lowercase() == extension)
    }

    /// Whether this syntax handles files run by `interpreter`.
    pub fn matches_interpreter(&self, interpreter: &str) -> bool {
        let interpreter = interpreter.to_lowercase();
        self.info
            .file_map
            .interpreters
            .iter()
            .any(|i| i.to_lowercase() == interpreter)
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: PathBuf) -> std::io::Result<Option<T>> {
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => Ok(Some(value)),
            // A malformed optional file is treated as absent; the syntax still
            // loads with whatever else it has.
            Err(_) => Ok(None),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Every syntax definition SubText can use.
#[derive(Debug, Clone, Default)]
pub struct SyntaxManager {
    syntaxes: Vec<Syntax>,
}

/// The fallback used when no `.cotsyntax` bundle could be read.
static PLAIN_TEXT: std::sync::LazyLock<Syntax> = std::sync::LazyLock::new(Syntax::plain_text);

impl SyntaxManager {
    /// Load every `.cotsyntax` bundle in `directory`.
    pub fn load_from_directory(directory: impl AsRef<Path>) -> std::io::Result<Self> {
        let mut syntaxes = Vec::new();
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() || path.extension().is_none_or(|e| e != "cotsyntax") {
                continue;
            }
            if let Ok(syntax) = Syntax::load(&path) {
                syntaxes.push(syntax);
            }
        }
        syntaxes.sort_by_key(|a| a.name.to_lowercase());
        Ok(Self { syntaxes })
    }

    /// Build a manager from syntaxes that were assembled somewhere else.
    ///
    /// The application merges two directories — the bundled set and the user's
    /// — before it has a manager, so it needs a way to hand the result over
    /// without going back through the filesystem. A later entry with the same
    /// name replaces an earlier one, which is what makes the user's directory
    /// override the shipped one; the result is then sorted the way
    /// [`load_from_directory`](Self::load_from_directory) sorts, so the two
    /// paths produce the same ordering.
    pub fn from_syntaxes(syntaxes: Vec<Syntax>) -> Self {
        let mut merged: Vec<Syntax> = Vec::with_capacity(syntaxes.len());
        for syntax in syntaxes {
            if let Some(existing) = merged.iter_mut().find(|s| s.name == syntax.name) {
                *existing = syntax;
            } else {
                merged.push(syntax);
            }
        }
        merged.sort_by_key(|a| a.name.to_lowercase());
        Self { syntaxes: merged }
    }

    pub fn all(&self) -> &[Syntax] {
        &self.syntaxes
    }

    pub fn is_empty(&self) -> bool {
        self.syntaxes.is_empty()
    }

    pub fn by_name(&self, name: &str) -> Option<&Syntax> {
        self.syntaxes.iter().find(|s| s.name == name).or_else(|| {
            self.syntaxes
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(name))
        })
    }

    /// The syntax to use for a file, falling back to Plain Text.
    ///
    /// A more specific match beats a less specific one: a full filename beats
    /// an extension, and an extension beats nothing.
    pub fn syntax_for_path(&self, path: &Path) -> &Syntax {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if let Some(found) = self.syntaxes.iter().find(|s| {
            s.info
                .file_map
                .filenames
                .iter()
                .any(|f| f.to_lowercase() == name)
        }) {
            return found;
        }
        if let Some(found) = self.syntaxes.iter().find(|s| s.matches_path(path)) {
            return found;
        }
        self.plain_text()
    }

    pub fn syntax_for_interpreter(&self, interpreter: &str) -> &Syntax {
        self.syntaxes
            .iter()
            .find(|s| s.matches_interpreter(interpreter))
            .unwrap_or_else(|| self.plain_text())
    }

    /// The Plain Text syntax, or any syntax at all if it is missing.
    ///
    /// Never panics: an empty manager is a legitimate state (no resource
    /// directory, or every bundle unreadable), and the caller still needs
    /// something to highlight with.
    pub fn plain_text(&self) -> &Syntax {
        self.by_name("Plain Text")
            .or_else(|| self.syntaxes.first())
            .unwrap_or(&PLAIN_TEXT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resources() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/syntaxes")
    }

    fn manager() -> SyntaxManager {
        SyntaxManager::load_from_directory(resources()).expect("resources/syntaxes must exist")
    }

    /// The seven bundles that say `"kind": "general"` keep their file maps.
    ///
    /// They did not, once: `SyntaxKind` was spelled `code`/`markup`/`none`, so
    /// `general` failed to deserialize, `read_json` treated the whole
    /// `Info.json` as absent, and a `.md` file opened as Plain Text.
    #[test]
    fn a_general_syntax_keeps_its_extensions() {
        let manager = manager();
        let markdown = manager.by_name("Markdown").expect("Markdown must load");
        assert_eq!(markdown.info.kind, SyntaxKind::General);
        assert!(
            markdown.info.file_map.extensions.iter().any(|e| e == "md"),
            "Markdown lost its extensions: {:?}",
            markdown.info.file_map
        );
        assert_eq!(
            manager.syntax_for_path(Path::new("notes.md")).name,
            "Markdown"
        );
        assert_eq!(
            manager.syntax_for_path(Path::new("paper.tex")).name,
            "LaTeX"
        );
        assert_eq!(manager.syntax_for_path(Path::new("a.rs")).name, "Rust");
    }

    #[test]
    fn loads_the_bundled_syntax_set() {
        let manager = manager();
        assert!(
            manager.all().len() >= 60,
            "expected the full CotEditor set, got {}",
            manager.all().len()
        );
        assert!(manager.by_name("Plain Text").is_some());
        assert!(manager.by_name("Python").is_some());
    }

    #[test]
    fn picks_syntax_by_extension() {
        let manager = manager();
        assert_eq!(manager.syntax_for_path(Path::new("main.rs")).name, "Rust");
        assert_eq!(manager.syntax_for_path(Path::new("app.py")).name, "Python");
        assert_eq!(
            manager.syntax_for_path(Path::new("index.html")).name,
            "HTML"
        );
        assert_eq!(
            manager.syntax_for_path(Path::new("notes.unknownext")).name,
            "Plain Text"
        );
    }

    #[test]
    fn full_filename_beats_extension() {
        let manager = manager();
        assert_eq!(
            manager.syntax_for_path(Path::new("Makefile")).name,
            "Makefile"
        );
        assert_eq!(
            manager.syntax_for_path(Path::new("Dockerfile")).name,
            "Dockerfile"
        );
    }

    #[test]
    fn interpreter_lookup_works() {
        let manager = manager();
        assert_eq!(manager.syntax_for_interpreter("python3").name, "Python");
    }

    #[test]
    fn python_comment_delimiters_come_from_edit_json() {
        let manager = manager();
        let python = manager.by_name("Python").unwrap();
        let delimiters = python.comment_delimiters();
        assert_eq!(delimiters.inline.as_deref(), Some("#"));
    }

    #[test]
    fn markdown_has_a_block_comment() {
        let manager = manager();
        let markdown = manager.by_name("Markdown").unwrap();
        let delimiters = markdown.comment_delimiters();
        assert_eq!(delimiters.block, Some(("<!--".into(), "-->".into())));
    }

    #[test]
    fn regex_highlighted_syntaxes_carry_their_rules() {
        let manager = manager();
        // YAML is one of the languages CotEditor highlights with regex rules.
        let yaml = manager.by_name("YAML").unwrap();
        assert!(yaml.has_highlight_file);
        assert!(yaml.highlights.contains_key(&HighlightToken::Keywords));
    }

    #[test]
    fn every_syntax_gets_comment_and_string_rules_from_edit_json() {
        let manager = manager();
        // The delimiters in `Edit.json` drive highlighting for *every*
        // language, including the tree-sitter ones — that is the whole point.
        for name in ["Rust", "Python", "Swift", "Go", "YAML", "JSON"] {
            let syntax = manager.by_name(name).unwrap();
            assert!(
                syntax.highlights.contains_key(&HighlightToken::Comments),
                "{name} should have comment rules from Edit.json"
            );
        }
        let rust = manager.by_name("Rust").unwrap();
        assert!(rust.highlights.contains_key(&HighlightToken::Strings));
        assert!(rust.highlights.contains_key(&HighlightToken::Characters));
    }

    #[test]
    fn tree_sitter_syntaxes_have_no_highlight_file() {
        let manager = manager();
        // CotEditor splits its syntax set in two: the major languages are
        // highlighted by tree-sitter (queries under `resources/queries`), and
        // their `.cotsyntax` bundles deliberately carry no `Highlights.json`.
        // SubText must respect that split rather than inventing rules.
        for name in ["Rust", "Python", "Swift", "JavaScript", "Go"] {
            let syntax = manager.by_name(name).unwrap();
            assert!(
                !syntax.has_highlight_file,
                "{name} is tree-sitter highlighted and should have no Highlights.json"
            );
        }
    }

    #[test]
    fn the_two_syntax_populations_are_both_large() {
        let manager = manager();
        let with_rules = manager
            .all()
            .iter()
            .filter(|s| s.has_highlight_file)
            .count();
        let without = manager.all().len() - with_rules;
        assert!(
            with_rules >= 40,
            "expected the regex population, got {with_rules}"
        );
        assert!(
            without >= 20,
            "expected the tree-sitter population, got {without}"
        );
    }

    #[test]
    fn rust_block_comments_are_nestable() {
        let manager = manager();
        let rust = manager.by_name("Rust").unwrap();
        assert!(
            rust.edit.comment.blocks.iter().any(|b| b.is_nestable),
            "Rust's /* */ comments nest"
        );
    }

    #[test]
    fn rust_outlines_are_loaded() {
        let manager = manager();
        let rust = manager.by_name("Rust").unwrap();
        assert!(
            !rust.outlines.is_empty(),
            "Rust should define outline rules"
        );
    }

    #[test]
    fn completions_are_loaded_for_languages_that_have_them() {
        let manager = manager();
        let python = manager.by_name("Python").unwrap();
        assert!(
            !python.completions.is_empty(),
            "Python ships a completion list"
        );
    }
}
