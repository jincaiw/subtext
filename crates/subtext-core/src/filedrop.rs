//! File-drop snippets: the text a dropped file turns into.
//!
//! CotEditor lets the user describe, per syntax and per file extension, what
//! dropping a file onto the editor should insert. A description is a format
//! string holding tokens such as `<<<RELATIVE-PATH>>>`, and expanding it is a
//! plain replacement in a fixed order.
//!
//! SubText keeps the same eleven tokens, the same replacement order and the
//! same matching rules, so a format written for CotEditor inserts the same text
//! here. What differs is storage. Upstream keeps each item as a dictionary of
//! strings in `UserDefaults` under `fileDropArray` — which is why `extensions`
//! is a comma-separated string there — while this crate stores the same four
//! fields as typed JSON. The settings pane still shows the extensions as a
//! comma-separated field, because that is how they are typed.
//!
//! # What the crate does not do
//!
//! Nothing here touches the filesystem. The two tokens that need more than the
//! path — `<<<FILECONTENT>>>` and `<<<IMAGEWIDTH>>>`/`<<<IMAGEHEIGHT>>>` — are
//! filled in by the caller, which reads them only when
//! [`FileDropItem::needs_content`] or [`FileDropItem::needs_image_size`] says
//! the format asks for them. That is upstream's own laziness, and it keeps the
//! expansion unit-testable without a disk.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// What a token starts with.
pub const TOKEN_PREFIX: &str = "<<<";
/// What a token ends with.
pub const TOKEN_SUFFIX: &str = ">>>";

/// One of the eleven things a file-drop format can insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variable {
    AbsolutePath,
    RelativePath,
    Filename,
    FilenameWithoutExtension,
    FileExtension,
    FileExtensionLowercase,
    FileExtensionUppercase,
    Directory,
    FileContent,
    ImageWidth,
    ImageHeight,
}

impl Variable {
    /// Every variable, in upstream's declaration order.
    pub const ALL: [Variable; 11] = [
        Variable::AbsolutePath,
        Variable::RelativePath,
        Variable::Filename,
        Variable::FilenameWithoutExtension,
        Variable::FileExtension,
        Variable::FileExtensionLowercase,
        Variable::FileExtensionUppercase,
        Variable::Directory,
        Variable::FileContent,
        Variable::ImageWidth,
        Variable::ImageHeight,
    ];

    /// The variables that describe where the file is. Upstream's
    /// `pathTokens`, and the group the *Insert Variable* menu lists first.
    pub const PATH: [Variable; 8] = [
        Variable::AbsolutePath,
        Variable::RelativePath,
        Variable::Filename,
        Variable::FilenameWithoutExtension,
        Variable::FileExtension,
        Variable::FileExtensionLowercase,
        Variable::FileExtensionUppercase,
        Variable::Directory,
    ];

    /// The variables that read the file, which upstream groups under
    /// *Text File*.
    pub const TEXT: [Variable; 1] = [Variable::FileContent];

    /// The variables that read the image, which upstream groups under
    /// *Image File*.
    pub const IMAGE: [Variable; 2] = [Variable::ImageWidth, Variable::ImageHeight];

    /// The name between the token markers, as upstream spells it.
    pub fn name(self) -> &'static str {
        match self {
            Variable::AbsolutePath => "ABSOLUTE-PATH",
            Variable::RelativePath => "RELATIVE-PATH",
            Variable::Filename => "FILENAME",
            Variable::FilenameWithoutExtension => "FILENAME-NOSUFFIX",
            Variable::FileExtension => "FILEEXTENSION",
            Variable::FileExtensionLowercase => "FILEEXTENSION-LOWER",
            Variable::FileExtensionUppercase => "FILEEXTENSION-UPPER",
            Variable::Directory => "DIRECTORY",
            Variable::FileContent => "FILECONTENT",
            Variable::ImageWidth => "IMAGEWIDTH",
            Variable::ImageHeight => "IMAGEHEIGHT",
        }
    }

    /// The whole token, markers included — what the user types in a format.
    pub fn token(self) -> String {
        format!("{TOKEN_PREFIX}{}{TOKEN_SUFFIX}", self.name())
    }

    /// What the variable inserts, for the *Insert Variable* menu.
    pub fn description(self) -> &'static str {
        match self {
            Variable::AbsolutePath => "The absolute path of the dropped file.",
            Variable::RelativePath => {
                "The relative path between the dropped file and the document."
            }
            Variable::Filename => "The dropped file's name including the extension, if any.",
            Variable::FilenameWithoutExtension => "The dropped file's name without extension.",
            Variable::FileExtension => "The dropped file's extension.",
            Variable::FileExtensionLowercase => {
                "The dropped file's extension (converted to lowercase)."
            }
            Variable::FileExtensionUppercase => {
                "The dropped file's extension (converted to uppercase)."
            }
            Variable::Directory => "The parent directory name of the dropped file.",
            Variable::FileContent => "The file content.",
            Variable::ImageWidth => "The image width.",
            Variable::ImageHeight => "The image height.",
        }
    }
}

/// One file-drop definition: when it applies, and what it inserts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileDropItem {
    /// The format string, with its tokens.
    pub format: String,
    /// The file extensions this item answers to, without dots and without
    /// regard to case. Empty means every extension.
    pub extensions: Vec<String>,
    /// The syntax this item is limited to, or `None` for all of them.
    pub scope: Option<String>,
    /// A note the user writes for themselves; never inserted.
    pub description: Option<String>,
}

impl FileDropItem {
    /// A blank item, which is what *Add* creates.
    pub fn new() -> Self {
        Self::default()
    }

    /// The items CotEditor ships, in its order.
    ///
    /// Six of them: two for Markdown, three for HTML and one for CSS. They are
    /// live in a fresh SubText for the same reason they are live in a fresh
    /// CotEditor — dropping a picture onto a Markdown document should insert a
    /// Markdown image, not a path.
    pub fn defaults() -> Vec<Self> {
        let image = ["jpg", "jpeg", "gif", "png"];
        vec![
            Self {
                format: "![<<<FILENAME-NOSUFFIX>>>](<<<RELATIVE-PATH>>>)".to_string(),
                extensions: image.iter().map(|it| (*it).to_string()).collect(),
                scope: Some("Markdown".to_string()),
                description: None,
            },
            Self {
                format: "[<<<FILENAME-NOSUFFIX>>>](<<<RELATIVE-PATH>>>)".to_string(),
                extensions: Vec::new(),
                scope: Some("Markdown".to_string()),
                description: None,
            },
            Self {
                format: "<img src=\"<<<RELATIVE-PATH>>>\" alt=\"<<<FILENAME-NOSUFFIX>>>\" \
                         title=\"<<<FILENAME-NOSUFFIX>>>\" width=\"<<<IMAGEWIDTH>>>\" \
                         height=\"<<<IMAGEHEIGHT>>>\" />"
                    .to_string(),
                extensions: image.iter().map(|it| (*it).to_string()).collect(),
                scope: Some("HTML".to_string()),
                description: None,
            },
            Self {
                format: "<script type=\"text/javascript\" src=\"<<<RELATIVE-PATH>>>\"></script>"
                    .to_string(),
                extensions: vec!["js".to_string()],
                scope: Some("HTML".to_string()),
                description: None,
            },
            Self {
                format: "<a href=\"<<<RELATIVE-PATH>>>\" \
                         title=\"<<<FILENAME-NOSUFFIX>>>\"></a>"
                    .to_string(),
                extensions: vec!["html".to_string(), "htm".to_string(), "php".to_string()],
                scope: Some("HTML".to_string()),
                description: None,
            },
            Self {
                format: "url(\"<<<RELATIVE-PATH>>>\")".to_string(),
                extensions: Vec::new(),
                scope: Some("CSS".to_string()),
                description: None,
            },
        ]
    }

    /// Whether this item answers to a file with `path_extension` dropped into a
    /// document whose syntax is `scope`.
    ///
    /// Upstream's rule, in full: a scope is either absent or has to match
    /// exactly, and an empty extension list means every extension. A file with
    /// no extension therefore never matches a list, which is what makes a
    /// `jpg, jpeg, png` item leave `README` alone.
    pub fn supports(&self, path_extension: Option<&str>, scope: Option<&str>) -> bool {
        if self.scope.is_some() && self.scope.as_deref() != scope {
            return false;
        }
        if self.extensions.is_empty() {
            return true;
        }
        path_extension.is_some_and(|extension| {
            self.extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
    }

    /// Whether the format names the file's text, and so wants it read.
    pub fn needs_content(&self) -> bool {
        self.format.contains(&Variable::FileContent.token())
    }

    /// Whether the format names the image's size, and so wants it read.
    pub fn needs_image_size(&self) -> bool {
        self.format.contains(&Variable::ImageWidth.token())
            || self.format.contains(&Variable::ImageHeight.token())
    }

    /// The text to insert for `file`.
    ///
    /// The order is upstream's, and two parts of it are load-bearing:
    ///
    /// * The file's text goes in **last**, because what it holds can look like
    ///   another token and must survive as written.
    /// * The image's size goes in only when both dimensions were read. A format
    ///   naming the width of something that is not an image keeps its token
    ///   visible rather than inserting a number that is not a width — again
    ///   upstream's behaviour, and the honest one.
    pub fn drop_text(&self, file: &DroppedFile) -> String {
        let mut text = self.format.clone();
        for (variable, value) in [
            (Variable::AbsolutePath, file.absolute_path.as_str()),
            (Variable::RelativePath, file.relative_path.as_str()),
            (Variable::Filename, file.filename.as_str()),
            (
                Variable::FilenameWithoutExtension,
                file.filename_without_extension.as_str(),
            ),
            (Variable::FileExtension, file.file_extension.as_str()),
            (Variable::Directory, file.directory.as_str()),
        ] {
            text = text.replace(&variable.token(), value);
        }

        // Derived from the extension rather than stored, so the two cannot
        // disagree.
        text = text.replace(
            &Variable::FileExtensionLowercase.token(),
            &file.file_extension.to_lowercase(),
        );
        text = text.replace(
            &Variable::FileExtensionUppercase.token(),
            &file.file_extension.to_uppercase(),
        );

        if let Some((width, height)) = file.image_size {
            text = text.replace(&Variable::ImageWidth.token(), &width.to_string());
            text = text.replace(&Variable::ImageHeight.token(), &height.to_string());
        }

        if let Some(content) = &file.content {
            text = text.replace(&Variable::FileContent.token(), content);
        }

        text
    }
}

/// The dropped file, as a format needs to see it.
///
/// Every field is a string because that is what goes into the text: the
/// alternative — keeping a `PathBuf` and formatting it at each token — would
/// have to repeat the lossy conversion and the extensionless case five times.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DroppedFile {
    /// The absolute path, or the address itself when a web URL was dropped.
    pub absolute_path: String,
    /// The path relative to the document's own directory, or the absolute path
    /// when there is no document or the two do not share a directory.
    pub relative_path: String,
    /// The last component, extension included.
    pub filename: String,
    /// The last component with its extension removed.
    pub filename_without_extension: String,
    /// The extension, without the dot. Empty when there is none.
    pub file_extension: String,
    /// The name of the directory holding the file. Empty at a root.
    pub directory: String,
    /// The file's text, for `<<<FILECONTENT>>>`.
    pub content: Option<String>,
    /// The image's pixel size, for `<<<IMAGEWIDTH>>>`/`<<<IMAGEHEIGHT>>>`.
    pub image_size: Option<(u32, u32)>,
}

impl DroppedFile {
    /// Describe a file on disk, relative to `document` when there is one.
    ///
    /// The comparison is lexical: `strip_prefix` compares components, so a
    /// symlink or a `..` in either path is taken at face value rather than
    /// resolved. CotEditor's `URL.path(relativeTo:)` does the same.
    pub fn new(path: &Path, document: Option<&Path>) -> Self {
        let absolute_path = path.to_string_lossy().into_owned();
        let relative_path = document
            .and_then(Path::parent)
            .and_then(|base| path.strip_prefix(base).ok())
            .map(|relative| relative.to_string_lossy().into_owned())
            .unwrap_or_else(|| absolute_path.clone());

        Self {
            absolute_path,
            relative_path,
            filename: component(path.file_name()),
            filename_without_extension: component(path.file_stem()),
            file_extension: component(path.extension()),
            directory: component(path.parent().and_then(Path::file_name)),
            content: None,
            image_size: None,
        }
    }

    /// Describe a web address, which has no directory to be relative to.
    pub fn from_url(url: &str) -> Self {
        // A query or a fragment is not part of the name, and neither is the
        // scheme's authority: `URL.lastPathComponent` reads past both.
        let path = url_path(url.split(['?', '#']).next().unwrap_or(url));
        let filename = path.rsplit('/').next().unwrap_or_default().to_string();
        let directory = path.rsplit('/').nth(1).unwrap_or_default().to_string();
        let (stem, extension) = split_extension(&filename);

        Self {
            absolute_path: url.to_string(),
            relative_path: url.to_string(),
            filename,
            filename_without_extension: stem,
            file_extension: extension,
            directory,
            content: None,
            image_size: None,
        }
    }

    /// Attach the file's text, for `<<<FILECONTENT>>>`.
    pub fn with_content(mut self, content: Option<String>) -> Self {
        self.content = content;
        self
    }

    /// Attach the image's pixel size, for `<<<IMAGEWIDTH>>>` and
    /// `<<<IMAGEHEIGHT>>>`.
    pub fn with_image_size(mut self, size: Option<(u32, u32)>) -> Self {
        self.image_size = size;
        self
    }
}

/// A path component as a string, or empty when there is none.
fn component(part: Option<&std::ffi::OsStr>) -> String {
    part.map(|part| part.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The path part of a URL: everything from the first `/` after the authority.
///
/// A string with no scheme is its own path, which is what makes
/// `example.com/a.png` behave like a bare path.
fn url_path(url: &str) -> &str {
    let Some(scheme_end) = url.find("://") else {
        return url;
    };
    let after_authority = &url[scheme_end + 3..];
    match after_authority.find('/') {
        Some(slash) => &after_authority[slash..],
        None => "",
    }
}

/// Split a filename into its stem and extension, the way
/// `URL.deletingPathExtension` does.
///
/// A leading dot is part of the name, not the start of an extension: `.bashrc`
/// has no extension.
fn split_extension(filename: &str) -> (String, String) {
    match filename.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem.to_string(), extension.to_string()),
        _ => (filename.to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(format: &str, extensions: &[&str], scope: Option<&str>) -> FileDropItem {
        FileDropItem {
            format: format.to_string(),
            extensions: extensions.iter().map(|it| (*it).to_string()).collect(),
            scope: scope.map(str::to_string),
            description: None,
        }
    }

    // ---- matching --------------------------------------------------------

    #[test]
    fn an_empty_item_answers_to_every_file_in_every_syntax() {
        let item = FileDropItem::default();
        assert!(item.supports(Some("JPG"), Some("foo")));
        assert!(item.supports(Some("jpg"), None));
        assert!(item.supports(None, Some("")));
        assert!(item.supports(None, None));
    }

    #[test]
    fn an_extension_list_answers_to_those_extensions_only() {
        let item = item("", &["jpg", "JPEG"], None);
        assert!(item.supports(Some("JPG"), Some("foo")));
        assert!(item.supports(Some("JPG"), None));
        assert!(!item.supports(Some("gif"), Some("foo")));
        // A file with no extension is not in the list, so it does not match.
        assert!(!item.supports(None, Some("foo")));
        assert!(!item.supports(None, None));
    }

    #[test]
    fn a_scope_answers_to_that_syntax_only() {
        let item = item("", &[], Some("foo"));
        assert!(item.supports(Some("JPG"), Some("foo")));
        assert!(item.supports(Some("gif"), Some("foo")));
        assert!(item.supports(None, Some("foo")));
        assert!(!item.supports(None, Some("bar")));
        assert!(!item.supports(Some("JPG"), None));
        assert!(!item.supports(None, None));
    }

    #[test]
    fn a_scope_and_an_extension_list_both_have_to_match() {
        let item = item("", &["jpg", "JPEG"], Some("foo"));
        assert!(item.supports(Some("JPG"), Some("foo")));
        assert!(item.supports(Some("jpeg"), Some("foo")));
        assert!(!item.supports(Some("gif"), Some("foo")));
        assert!(!item.supports(None, Some("foo")));
        assert!(!item.supports(None, Some("bar")));
        assert!(!item.supports(Some("JPG"), None));
        assert!(!item.supports(None, None));
    }

    // ---- describing the dropped file -------------------------------------

    #[test]
    fn a_file_is_described_by_its_path() {
        let file = DroppedFile::new(Path::new("/Documents/notes/img.png"), None);
        assert_eq!(file.absolute_path, "/Documents/notes/img.png");
        assert_eq!(file.relative_path, "/Documents/notes/img.png");
        assert_eq!(file.filename, "img.png");
        assert_eq!(file.filename_without_extension, "img");
        assert_eq!(file.file_extension, "png");
        assert_eq!(file.directory, "notes");
    }

    #[test]
    fn the_relative_path_is_measured_from_the_documents_own_directory() {
        let document = Path::new("/Documents/index.md");
        let file = DroppedFile::new(Path::new("/Documents/assets/img.png"), Some(document));
        assert_eq!(file.relative_path, "assets/img.png");
    }

    /// A file outside the document's directory has no relative path, so the
    /// absolute one stands in — as upstream's `path(relativeTo:)` does.
    #[test]
    fn a_file_elsewhere_falls_back_to_its_absolute_path() {
        let document = Path::new("/Documents/index.md");
        let file = DroppedFile::new(Path::new("/Pictures/img.png"), Some(document));
        assert_eq!(file.relative_path, "/Pictures/img.png");
    }

    #[test]
    fn a_file_at_the_root_has_no_directory_name() {
        let file = DroppedFile::new(Path::new("/img.png"), None);
        assert_eq!(file.directory, "");
        assert_eq!(file.filename, "img.png");
    }

    #[test]
    fn a_file_without_an_extension_has_an_empty_one() {
        let file = DroppedFile::new(Path::new("/Documents/README"), None);
        assert_eq!(file.filename, "README");
        assert_eq!(file.filename_without_extension, "README");
        assert_eq!(file.file_extension, "");
    }

    #[test]
    fn a_web_address_is_described_without_being_a_path() {
        let file = DroppedFile::from_url("https://example.com/assets/image.png");
        assert_eq!(file.absolute_path, "https://example.com/assets/image.png");
        assert_eq!(file.relative_path, "https://example.com/assets/image.png");
        assert_eq!(file.filename, "image.png");
        assert_eq!(file.filename_without_extension, "image");
        assert_eq!(file.file_extension, "png");
        assert_eq!(file.directory, "assets");
    }

    #[test]
    fn a_web_address_ignores_its_query_and_fragment() {
        let file = DroppedFile::from_url("https://example.com/a/image.png?w=100#top");
        assert_eq!(file.filename, "image.png");
        assert_eq!(file.file_extension, "png");
    }

    #[test]
    fn a_web_address_at_the_root_has_no_directory_name() {
        let file = DroppedFile::from_url("https://example.com/image.png");
        assert_eq!(file.filename, "image.png");
        assert_eq!(file.directory, "");
    }

    #[test]
    fn a_hidden_file_has_no_extension() {
        let file = DroppedFile::from_url("https://example.com/.bashrc");
        assert_eq!(file.filename, ".bashrc");
        assert_eq!(file.filename_without_extension, ".bashrc");
        assert_eq!(file.file_extension, "");
    }

    // ---- expanding a format ----------------------------------------------

    /// Upstream's own worked example: a web address, whose path cannot be
    /// relative to the document.
    #[test]
    fn a_web_address_expands_to_the_address_twice() {
        let item = item(
            "[<<<FILENAME-NOSUFFIX>>>](<<<RELATIVE-PATH>>>) <<<ABSOLUTE-PATH>>>",
            &[],
            None,
        );
        let file = DroppedFile::from_url("https://example.com/assets/image.png");
        assert_eq!(
            item.drop_text(&file),
            "[image](https://example.com/assets/image.png) https://example.com/assets/image.png"
        );
    }

    #[test]
    fn every_path_token_is_replaced() {
        let item = item(
            "<<<ABSOLUTE-PATH>>>|<<<RELATIVE-PATH>>>|<<<FILENAME>>>|\
             <<<FILENAME-NOSUFFIX>>>|<<<FILEEXTENSION>>>|<<<FILEEXTENSION-LOWER>>>|\
             <<<FILEEXTENSION-UPPER>>>|<<<DIRECTORY>>>",
            &[],
            None,
        );
        let file = DroppedFile::new(
            Path::new("/Documents/notes/Photo.PNG"),
            Some(Path::new("/Documents/index.md")),
        );
        assert_eq!(
            item.drop_text(&file),
            "/Documents/notes/Photo.PNG|notes/Photo.PNG|Photo.PNG|Photo|PNG|png|PNG|notes"
        );
    }

    #[test]
    fn a_format_without_tokens_is_inserted_as_written() {
        let item = item("nothing to expand", &[], None);
        let file = DroppedFile::new(Path::new("/a/b.png"), None);
        assert_eq!(item.drop_text(&file), "nothing to expand");
    }

    /// A token the format does not name is left alone, and a token it does name
    /// twice is replaced twice.
    #[test]
    fn only_the_named_tokens_are_replaced() {
        let item = item("<<<FILENAME>>> <<<FILENAME>>> <<<IMAGEWIDTH>>>", &[], None);
        let file = DroppedFile::new(Path::new("/a/b.png"), None);
        assert_eq!(item.drop_text(&file), "b.png b.png <<<IMAGEWIDTH>>>");
    }

    #[test]
    fn an_image_size_is_inserted_when_it_was_read() {
        let item = item("width=<<<IMAGEWIDTH>>> height=<<<IMAGEHEIGHT>>>", &[], None);
        let file = DroppedFile::new(Path::new("/a/b.png"), None).with_image_size(Some((640, 480)));
        assert_eq!(item.drop_text(&file), "width=640 height=480");
    }

    #[test]
    fn a_file_content_is_inserted_when_it_was_read() {
        let item = item("<script>\n<<<FILECONTENT>>>\n</script>", &[], None);
        let file =
            DroppedFile::new(Path::new("/a/b.js"), None).with_content(Some("let x = 1;".into()));
        assert_eq!(item.drop_text(&file), "<script>\nlet x = 1;\n</script>");
    }

    /// The file's text goes in last, so a token inside it survives as written
    /// rather than being expanded against the file it came from.
    #[test]
    fn a_token_inside_a_files_text_is_not_expanded() {
        let item = item("<<<FILENAME>>>: <<<FILECONTENT>>>", &[], None);
        let file = DroppedFile::new(Path::new("/a/b.txt"), None)
            .with_content(Some("<<<ABSOLUTE-PATH>>>".into()));
        assert_eq!(item.drop_text(&file), "b.txt: <<<ABSOLUTE-PATH>>>");
    }

    // ---- what the format asks for ----------------------------------------

    #[test]
    fn a_format_says_whether_it_wants_the_file_read() {
        assert!(item("<<<FILECONTENT>>>", &[], None).needs_content());
        assert!(!item("<<<FILENAME>>>", &[], None).needs_content());
    }

    #[test]
    fn a_format_says_whether_it_wants_the_image_measured() {
        assert!(item("<<<IMAGEWIDTH>>>", &[], None).needs_image_size());
        assert!(item("<<<IMAGEHEIGHT>>>", &[], None).needs_image_size());
        assert!(!item("<<<FILENAME>>>", &[], None).needs_image_size());
    }

    // ---- storage ---------------------------------------------------------

    #[test]
    fn an_item_round_trips_through_json() {
        let original = item(
            "![<<<FILENAME-NOSUFFIX>>>](<<<RELATIVE-PATH>>>)",
            &["jpg", "jpeg"],
            Some("Markdown"),
        );
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(
            serde_json::from_str::<FileDropItem>(&json).unwrap(),
            original
        );
    }

    /// A settings file written before this feature existed has no `fileDrop`
    /// key, and every field of an item may be missing.
    #[test]
    fn a_missing_field_falls_back_to_its_default() {
        let item: FileDropItem = serde_json::from_str("{}").unwrap();
        assert_eq!(item, FileDropItem::default());
        let item: FileDropItem =
            serde_json::from_str(r#"{"format":"x","extensions":["png"]}"#).unwrap();
        assert_eq!(item.scope, None);
        assert_eq!(item.description, None);
        assert_eq!(item.extensions, ["png"]);
    }

    /// Every token is spelled the way upstream spells it, and the three groups
    /// cover the whole set exactly once.
    #[test]
    fn the_variables_are_the_ones_upstream_names() {
        assert_eq!(Variable::ALL.len(), 11);
        assert_eq!(
            Variable::PATH.len() + Variable::TEXT.len() + Variable::IMAGE.len(),
            11
        );
        assert_eq!(Variable::AbsolutePath.token(), "<<<ABSOLUTE-PATH>>>");
        assert_eq!(
            Variable::FilenameWithoutExtension.token(),
            "<<<FILENAME-NOSUFFIX>>>"
        );
        assert_eq!(Variable::ImageHeight.token(), "<<<IMAGEHEIGHT>>>");

        for variable in Variable::ALL {
            assert!(
                Variable::PATH.contains(&variable)
                    || Variable::TEXT.contains(&variable)
                    || Variable::IMAGE.contains(&variable),
                "{variable:?} is in no group"
            );
            assert!(
                !variable.description().is_empty(),
                "{variable:?} has no description"
            );
        }
    }

    /// The six defaults are the ones CotEditor ships, and each one applies to
    /// the document it was written for.
    #[test]
    fn the_defaults_are_the_ones_coteditor_ships() {
        let defaults = FileDropItem::defaults();
        assert_eq!(defaults.len(), 6);
        assert_eq!(defaults[0].scope.as_deref(), Some("Markdown"));
        assert_eq!(defaults[0].extensions, ["jpg", "jpeg", "gif", "png"]);
        assert_eq!(defaults[5].scope.as_deref(), Some("CSS"));
        assert!(defaults[5].format.starts_with("url("));

        // The second Markdown item takes anything that the first did not.
        assert!(defaults[1].supports(Some("txt"), Some("Markdown")));
        assert!(!defaults[1].supports(Some("png"), Some("HTML")));
        // The first answers a picture in Markdown and nothing else.
        assert!(defaults[0].supports(Some("PNG"), Some("Markdown")));
        assert!(!defaults[0].supports(Some("png"), Some("HTML")));
        assert!(!defaults[0].supports(None, Some("Markdown")));
    }

    /// The HTML default names both dimensions, so it is the one that makes the
    /// caller read the image.
    #[test]
    fn the_html_default_asks_for_the_image_size() {
        let defaults = FileDropItem::defaults();
        assert!(defaults[2].needs_image_size());
        assert!(!defaults[0].needs_image_size());
    }
}
