//! The application's persisted preferences.
//!
//! CotEditor's settings window has nine panes down the left, in this order:
//! General, Appearance, Window, Edit, Mode, Format, Snippets, Shortcuts and
//! Integration. SubText keeps that shape — the same nine panes, the same
//! labels, the same grouping of controls inside each pane — because a
//! preference the user knows by position should not move.
//!
//! Everything here round-trips through a JSON file in the user's config
//! directory. A few of CotEditor's switches control behaviour that only
//! AppKit has (`Smart quotes`, `Check spelling while typing`). Those are
//! modelled anyway so the pane reads the same, and the ones SubText cannot
//! honour are rendered disabled with a note rather than quietly faked.
//!
//! Field names follow CotEditor's `@AppStorage` keys, lower-cased and
//! snake-cased, so that a value can be traced back to its upstream origin
//! without a lookup table.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use subtext_core::encoding::TextEncoding;
use subtext_core::filedrop::FileDropItem;
use subtext_core::folderfind::FileScope;
use subtext_core::line_ending::LineEnding;
use subtext_core::snippets::Snippet;

/// Which pane of the settings window is showing.
///
/// The order of `ALL` is the order of the rail, and it matches
/// `SettingsPane.swift` in the upstream project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SettingsPane {
    #[default]
    General,
    Appearance,
    Window,
    Edit,
    Mode,
    Format,
    Snippets,
    Shortcuts,
    Integration,
}

impl SettingsPane {
    pub const ALL: [SettingsPane; 9] = [
        SettingsPane::General,
        SettingsPane::Appearance,
        SettingsPane::Window,
        SettingsPane::Edit,
        SettingsPane::Mode,
        SettingsPane::Format,
        SettingsPane::Snippets,
        SettingsPane::Shortcuts,
        SettingsPane::Integration,
    ];

    /// The localized label CotEditor shows in the rail.
    pub fn title(self) -> &'static str {
        match self {
            SettingsPane::General => "General",
            SettingsPane::Appearance => "Appearance",
            SettingsPane::Window => "Window",
            SettingsPane::Edit => "Edit",
            SettingsPane::Mode => "Mode",
            SettingsPane::Format => "Format",
            SettingsPane::Snippets => "Snippets",
            SettingsPane::Shortcuts => "Shortcuts",
            SettingsPane::Integration => "Integration",
        }
    }

    /// The symbol name CotEditor uses for the pane's rail icon.
    ///
    /// Kept as the upstream SF Symbol name so the two rails can be compared
    /// item by item; `icon_for` maps each one to the closest icon the icon set
    /// actually ships.
    pub fn symbol(self) -> &'static str {
        match self {
            SettingsPane::General => "gearshape",
            SettingsPane::Appearance => "eyeglasses",
            SettingsPane::Window => "uiwindow.split.2x1",
            SettingsPane::Edit => "square.and.pencil",
            SettingsPane::Mode => "switch.2",
            SettingsPane::Format => "text.document",
            SettingsPane::Snippets => "text.viewfinder",
            SettingsPane::Shortcuts => "keyboard",
            SettingsPane::Integration => "apple.terminal.fill",
        }
    }

    /// A one-line explanation of what the pane is for.
    pub fn summary(self) -> &'static str {
        match self {
            SettingsPane::General => "Startup, saving and updates.",
            SettingsPane::Appearance => "Fonts, line height and editor opacity.",
            SettingsPane::Window => "Window size, tabs, and what the editor shows.",
            SettingsPane::Edit => "Indentation, whitespace and content parsing.",
            SettingsPane::Mode => "Per-syntax editing behaviour.",
            SettingsPane::Format => "Default line endings, encoding and syntax.",
            SettingsPane::Snippets => "Text inserted by commands and shortcuts.",
            SettingsPane::Shortcuts => "Every menu command's key equivalent.",
            SettingsPane::Integration => "The command-line tool and remote editing.",
        }
    }
}

/// What to do when the application launches with nothing else open.
///
/// CotEditor calls this `NoDocumentOnLaunchOption`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NoDocumentOnLaunchOption {
    /// Open a blank document, which is what CotEditor does out of the box.
    #[default]
    CreateNewDocument,
    ShowOpenDialog,
    DoNothing,
}

impl NoDocumentOnLaunchOption {
    pub const ALL: [NoDocumentOnLaunchOption; 3] = [
        NoDocumentOnLaunchOption::CreateNewDocument,
        NoDocumentOnLaunchOption::ShowOpenDialog,
        NoDocumentOnLaunchOption::DoNothing,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NoDocumentOnLaunchOption::CreateNewDocument => "Create New Document",
            NoDocumentOnLaunchOption::ShowOpenDialog => "Show Open Dialog",
            NoDocumentOnLaunchOption::DoNothing => "Do Nothing",
        }
    }
}

/// How to resolve a file that changed on disk while it was open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DocumentConflictOption {
    /// CotEditor's default: the open buffer wins.
    #[default]
    KeepOpenVersion,
    AskHowToResolve,
    UpdateToModifiedVersion,
}

impl DocumentConflictOption {
    pub const ALL: [DocumentConflictOption; 3] = [
        DocumentConflictOption::KeepOpenVersion,
        DocumentConflictOption::AskHowToResolve,
        DocumentConflictOption::UpdateToModifiedVersion,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DocumentConflictOption::KeepOpenVersion => "Keep CotEditor's version",
            DocumentConflictOption::AskHowToResolve => "Ask how to resolve",
            DocumentConflictOption::UpdateToModifiedVersion => "Update to modified version",
        }
    }
}

/// Which of the two fonts a document is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FontType {
    #[default]
    Standard,
    Monospaced,
}

/// The document appearance, mirroring `AppearanceMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DocumentAppearance {
    #[default]
    System,
    Light,
    Dark,
}

impl DocumentAppearance {
    pub const ALL: [DocumentAppearance; 3] = [
        DocumentAppearance::System,
        DocumentAppearance::Light,
        DocumentAppearance::Dark,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DocumentAppearance::System => "System",
            DocumentAppearance::Light => "Light",
            DocumentAppearance::Dark => "Dark",
        }
    }
}

/// How new windows decide whether to open documents in tabs.
///
/// Mirrors `NSWindow.UserTabbingPreference`; `-1` upstream means "respect the
/// system setting", which `RespectSystem` expresses directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WindowTabbing {
    #[default]
    RespectSystem,
    Manual,
    InFullScreen,
    Always,
}

impl WindowTabbing {
    pub const ALL: [WindowTabbing; 4] = [
        WindowTabbing::RespectSystem,
        WindowTabbing::Manual,
        WindowTabbing::InFullScreen,
        WindowTabbing::Always,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WindowTabbing::RespectSystem => "Respect System Setting",
            WindowTabbing::Manual => "Manually",
            WindowTabbing::InFullScreen => "In Full Screen Only",
            WindowTabbing::Always => "Always",
        }
    }
}

/// The writing direction CotEditor lays the text out in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WritingDirection {
    #[default]
    LeftToRight,
    RightToLeft,
}

/// What appears in the editor's gutter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorOptions {
    pub show_line_numbers: bool,
    pub show_invisibles: bool,
    /// Line endings, drawn as `¶`-like marks.
    pub show_invisible_new_line: bool,
    pub show_invisible_tab: bool,
    pub show_invisible_space: bool,
    /// Other whitespace: non-breaking space, ideographic space and friends.
    pub show_invisible_whitespaces: bool,
    /// Other control characters.
    pub show_invisible_control: bool,
    pub show_indent_guides: bool,
    pub show_page_guide: bool,
    /// The column the page guide is drawn at.
    pub page_guide_column: usize,
    pub wrap_lines: bool,
    pub highlight_current_line: bool,
    pub antialias: bool,
    pub ligatures: bool,
    pub editable: bool,
}

impl Default for EditorOptions {
    fn default() -> Self {
        Self::coteditor_defaults()
    }
}

impl EditorOptions {
    /// CotEditor's defaults for a new document.
    pub fn coteditor_defaults() -> Self {
        Self {
            show_line_numbers: true,
            show_invisibles: false,
            show_invisible_new_line: true,
            show_invisible_tab: true,
            show_invisible_space: true,
            show_invisible_whitespaces: false,
            show_invisible_control: false,
            show_indent_guides: false,
            show_page_guide: false,
            page_guide_column: 80,
            wrap_lines: false,
            highlight_current_line: true,
            antialias: true,
            ligatures: false,
            editable: true,
        }
    }
}

/// Which chrome is visible around the editor.
///
/// No `Eq`: the saved window size and the hanging indent are floats.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowOptions {
    pub show_tab_bar: bool,
    pub show_toolbar: bool,
    pub show_sidebar: bool,
    pub show_inspector: bool,
    pub show_navigation_bar: bool,
    pub show_status_bar: bool,
    pub keep_on_top: bool,

    /// Open documents in tabs rather than separate windows.
    pub window_tabbing: WindowTabbing,
    /// The saved window width in points. `None` means "fit the content".
    pub window_width: Option<f64>,
    pub window_height: Option<f64>,

    /// A tinted status bar follows the editor; an opaque one does not.
    pub prefers_opaque_bar_background: bool,
    /// Draw a hanging indent on wrapped lines.
    pub enables_hanging_indent: bool,
    pub hanging_indent_width: f32,
    pub writing_direction: WritingDirection,

    /// Which fields the status bar shows.
    pub show_status_bar_lines: bool,
    pub show_status_bar_chars: bool,
    pub show_status_bar_words: bool,
    pub show_status_bar_location: bool,
    pub show_status_bar_line: bool,
    pub show_status_bar_column: bool,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self::coteditor_defaults()
    }
}

impl WindowOptions {
    pub fn coteditor_defaults() -> Self {
        Self {
            show_tab_bar: true,
            show_toolbar: true,
            show_sidebar: false,
            show_inspector: false,
            show_navigation_bar: true,
            show_status_bar: true,
            keep_on_top: false,
            window_tabbing: WindowTabbing::default(),
            window_width: None,
            window_height: None,
            prefers_opaque_bar_background: false,
            enables_hanging_indent: false,
            hanging_indent_width: 2.0,
            writing_direction: WritingDirection::default(),
            show_status_bar_lines: true,
            show_status_bar_chars: true,
            show_status_bar_words: true,
            show_status_bar_location: true,
            show_status_bar_line: true,
            show_status_bar_column: true,
        }
    }
}

/// Editing behaviour: indentation, whitespace and content parsing.
///
/// The field names follow the Edit pane's `@AppStorage` keys.
///
/// No `Eq`: the delay is a float, and claiming exact equality on one would be
/// a promise the type cannot keep.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EditOptions {
    /// Prefer spaces over tabs when indenting.
    pub auto_expand_tab: bool,
    /// How wide one indentation step is, in columns.
    pub tab_width: usize,
    /// Read the indentation style off the document when it opens.
    pub detects_indent_style: bool,
    /// Keep the previous line's indentation when Return is pressed.
    pub auto_indent: bool,
    /// Tab indents the selection instead of replacing it.
    pub indent_with_tab_key: bool,

    pub auto_trims_trailing_whitespace: bool,
    pub trims_whitespace_only_lines: bool,

    pub inserts_comment_delimiters_after_indent: bool,
    pub appends_comment_spacer: bool,

    pub auto_link_detection: bool,
    pub highlight_braces: bool,
    pub highlight_selection_instance: bool,
    /// How long to wait before highlighting every instance, in seconds.
    pub selection_instance_highlight_delay: f32,

    /// Add a byte-order mark when saving as UTF-8.
    pub save_utf8_bom: bool,
    /// Whether the editor is read-only.
    pub editable: bool,
}

impl Default for EditOptions {
    fn default() -> Self {
        Self::coteditor_defaults()
    }
}

impl EditOptions {
    pub fn coteditor_defaults() -> Self {
        Self {
            auto_expand_tab: true,
            tab_width: 4,
            detects_indent_style: true,
            auto_indent: true,
            indent_with_tab_key: true,
            auto_trims_trailing_whitespace: false,
            trims_whitespace_only_lines: false,
            inserts_comment_delimiters_after_indent: true,
            appends_comment_spacer: false,
            auto_link_detection: true,
            highlight_braces: true,
            highlight_selection_instance: false,
            selection_instance_highlight_delay: 0.25,
            save_utf8_bom: false,
            editable: true,
        }
    }
}

/// Startup, saving and update behaviour — the General pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralOptions {
    /// Reopen the documents that were open when the app last quit.
    pub quit_always_keeps_windows: bool,
    pub no_document_on_launch_option: NoDocumentOnLaunchOption,
    /// Save in place with versions, rather than asking.
    pub enables_autosave_in_place: bool,
    pub document_conflict_option: DocumentConflictOption,
    /// Ask before opening a file whose line endings are mixed.
    pub suppresses_inconsistent_line_ending_alert: bool,
    pub enable_automatic_update_checks: bool,
    pub checks_updates_for_beta: bool,
}

impl Default for GeneralOptions {
    fn default() -> Self {
        Self::coteditor_defaults()
    }
}

impl GeneralOptions {
    pub fn coteditor_defaults() -> Self {
        Self {
            quit_always_keeps_windows: false,
            no_document_on_launch_option: NoDocumentOnLaunchOption::default(),
            enables_autosave_in_place: false,
            document_conflict_option: DocumentConflictOption::default(),
            suppresses_inconsistent_line_ending_alert: false,
            enable_automatic_update_checks: true,
            checks_updates_for_beta: false,
        }
    }
}

/// Fonts, line height and opacity — the Appearance pane.
///
/// No `Eq`: the sizes, the line height and the opacity are floats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppearanceOptions {
    /// The font used for prose documents.
    pub standard_font_family: Option<String>,
    pub standard_font_size: f32,
    pub standard_antialias: bool,
    pub standard_ligature: bool,

    /// The font used for code documents.
    pub monospaced_font_family: Option<String>,
    pub monospaced_font_size: f32,
    pub monospaced_antialias: bool,
    pub monospaced_ligature: bool,

    /// The line height as a multiple of the font's own.
    pub line_height: f32,
    pub document_appearance: DocumentAppearance,
    /// `1.0` is opaque; the slider bottoms out at `0.2`, as upstream does.
    pub window_alpha: f32,
}

impl Default for AppearanceOptions {
    fn default() -> Self {
        Self::coteditor_defaults()
    }
}

impl AppearanceOptions {
    /// The range CotEditor's opacity slider spans.
    pub const WINDOW_ALPHA_RANGE: std::ops::RangeInclusive<f32> = 0.2..=1.0;

    pub fn coteditor_defaults() -> Self {
        Self {
            standard_font_family: None,
            standard_font_size: 13.0,
            standard_antialias: true,
            standard_ligature: false,
            monospaced_font_family: None,
            monospaced_font_size: 13.0,
            monospaced_antialias: true,
            monospaced_ligature: false,
            line_height: 1.0,
            document_appearance: DocumentAppearance::default(),
            window_alpha: 1.0,
        }
    }
}

/// The per-syntax editing behaviour shown in the Mode pane.
///
/// CotEditor keeps one of these per syntax and one per syntax *kind*. SubText
/// models the kind-level defaults; per-syntax overrides arrive with the syntax
/// editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModeOptions {
    pub font_type: FontType,
    pub smart_insert_delete: bool,
    pub automatic_quote_substitution: bool,
    pub automatic_dash_substitution: bool,
    pub automatic_text_replacement: bool,
    pub automatic_period_substitution: bool,
    pub automatic_symbol_balancing: bool,
    pub continuous_spell_checking: bool,
    pub grammar_checking: bool,
    pub automatic_spelling_correction: bool,
    pub completion_standard_words: bool,
    pub completion_document_words: bool,
    pub completion_syntax_words: bool,
    pub automatic_completion: bool,
}

/// The sidebar's selected pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SidebarPane {
    #[default]
    FileBrowser,
    FolderFind,
}

/// The inspector's selected pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InspectorPane {
    #[default]
    Document,
    Outline,
    Warnings,
}

/// The Outline inspector's row font size before the user changes it.
///
/// CotEditor's `NSFont.smallSystemFontSize`, which is 11 points on macOS and is
/// also the floor its *Smaller* command stops at.
pub const DEFAULT_OUTLINE_FONT_SIZE: f32 = 11.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub general: GeneralOptions,
    pub appearance: AppearanceOptions,
    pub editor: EditorOptions,
    pub edit: EditOptions,
    pub mode: ModeOptions,
    pub window: WindowOptions,
    pub sidebar_pane: SidebarPane,
    pub inspector_pane: InspectorPane,

    /// The theme applied to the editor. `None` means "follow the system".
    pub theme: Option<String>,
    pub font_family: Option<String>,
    pub font_size: f32,

    /// Mirrors `edit.tab_width`; kept so callers that think in terms of the
    /// document's indentation need not know which pane owns the value.
    pub tab_width: usize,
    pub uses_spaces_for_indentation: bool,
    pub auto_expand_tabs: bool,

    pub default_encoding: String,
    pub default_line_ending: LineEnding,
    pub default_syntax: Option<String>,
    /// Honour the encoding declared in the file rather than guessing.
    pub refer_to_encoding_tag: bool,

    /// Continue a find from the other end of the document.
    pub find_wrap_around: bool,
    /// Interpret `\n` and friends in the find field.
    pub find_escape_sequences: bool,
    pub find_regex: bool,
    pub find_case_sensitive: bool,
    pub find_whole_word: bool,

    /// Accept `rmate` connections from a remote host.
    pub enables_remote_editing: bool,
    pub remote_editing_port: u16,

    /// The notation the *Edit Color Code* panel writes colour codes in.
    ///
    /// The number is CotEditor's `ColorCodeType` raw value, kept rather than
    /// translated so the two applications agree on what is stored; see
    /// [`subtext_core::colorcode::ColorCodeType::from_raw_value`].
    pub color_code_type: i32,

    /// The most recently opened documents, newest first.
    pub recent_documents: Vec<PathBuf>,

    /// Folder Find's *Text / Regular Expression* menu.
    ///
    /// CotEditor keeps the four switches below in `UserDefaults` and restores
    /// them between launches, so the pane reopens on the mode the user left it
    /// in. Unlike the find bar, folder find is **case sensitive** by default —
    /// hence the doubly negated key, which is upstream's own spelling.
    pub folder_find_uses_regular_expression: bool,
    /// Upstream's `folderFindIgnoresCase`.
    pub folder_find_ignores_case: bool,
    pub folder_find_includes_hidden_files: bool,
    pub folder_find_includes_other_file_types: bool,

    /// Whether the split editors sit side by side rather than one above the
    /// other.
    ///
    /// CotEditor's `splitViewVertical`, which names the *divider*: a vertical
    /// divider separates panes that sit left and right. Like upstream's, it is
    /// not in the Settings window — the View menu's *Stack Editors …* item and
    /// the navigation bar's button are the only way to change it, and the
    /// choice is remembered for the next launch.
    pub split_view_vertical: bool,

    /// Whether the Outline inspector wraps long labels.
    ///
    /// CotEditor's `outlineViewWrapsLines`. Kept, and shown in the pane's
    /// Options menu, but it has no effect here: the inspector is a virtualized
    /// list, and every row of one must be the same height. See the Outline
    /// section of `docs/PLAN.md` for the deviation.
    pub outline_wraps_lines: bool,
    /// The Outline inspector's row font size, in points.
    ///
    /// CotEditor's `outlineViewFontSize`, whose default is
    /// `NSFont.smallSystemFontSize` — 11 points on macOS.
    pub outline_font_size: f32,

    /// The unnamed file scope the *File Scope* menu last held.
    pub folder_find_scope: Option<FileScope>,
    /// Named scopes, keyed by name — upstream's `folderFindSavedScopes`.
    pub folder_find_saved_scopes: BTreeMap<String, FileScope>,

    /// The snippets behind *Text > Insert Snippet*, in menu order.
    ///
    /// CotEditor keeps these in `UserDefaults` under `snippets`, which is the
    /// same store as the rest of its preferences, so they belong in this file
    /// rather than in one of their own. The order is the user's: the submenu
    /// lists them in the order the settings table shows them, and a snippet's
    /// position in that table is the only thing that decides which of two
    /// snippets sharing a shortcut wins.
    pub snippets: Vec<Snippet>,

    /// The file-drop definitions behind the Snippets pane's second tab.
    ///
    /// CotEditor keeps these in `UserDefaults` under `fileDropArray`, as
    /// dictionaries of strings. SubText stores the same four fields as typed
    /// JSON, and — one deliberate difference — always writes the list out
    /// rather than deleting it when it happens to equal the shipped defaults.
    /// Upstream's trick of storing nothing in that case is what lets its
    /// *Restore Defaults* button know whether it has anything to restore; here
    /// the button simply puts [`FileDropItem::defaults`] back, which needs no
    /// bookkeeping and survives a hand-edited file.
    ///
    /// The order is the user's, and it is the order that decides which item
    /// answers when two of them match the same file.
    pub file_drop: Vec<FileDropItem>,
}

impl Default for Settings {
    fn default() -> Self {
        let edit = EditOptions::coteditor_defaults();
        Self {
            general: GeneralOptions::coteditor_defaults(),
            appearance: AppearanceOptions::coteditor_defaults(),
            editor: EditorOptions::coteditor_defaults(),
            edit,
            mode: ModeOptions::default(),
            window: WindowOptions::coteditor_defaults(),
            sidebar_pane: SidebarPane::default(),
            inspector_pane: InspectorPane::default(),
            theme: None,
            font_family: None,
            font_size: edit.tab_width as f32 + 9.0,
            tab_width: edit.tab_width,
            uses_spaces_for_indentation: edit.auto_expand_tab,
            auto_expand_tabs: edit.auto_expand_tab,
            default_encoding: TextEncoding::UTF8.identifier().to_string(),
            default_line_ending: LineEnding::LF,
            default_syntax: None,
            refer_to_encoding_tag: true,
            find_wrap_around: true,
            find_escape_sequences: false,
            find_regex: false,
            find_case_sensitive: false,
            find_whole_word: false,
            enables_remote_editing: false,
            remote_editing_port: 52698,
            // CotEditor's `colorCodeType` default is `1`, which is `Hex`.
            color_code_type: subtext_core::colorcode::ColorCodeType::default().raw_value(),
            recent_documents: Vec::new(),
            folder_find_uses_regular_expression: false,
            // CotEditor's `folderFindIgnoresCase` default is `false`, so the
            // folder pane starts case sensitive.
            folder_find_ignores_case: false,
            folder_find_includes_hidden_files: false,
            folder_find_includes_other_file_types: false,
            // CotEditor's `splitViewVertical` default is `false`: a fresh
            // window stacks its editors one above the other.
            split_view_vertical: false,
            // CotEditor's `outlineViewWrapsLines` is `false`, and its
            // `outlineViewFontSize` is `NSFont.smallSystemFontSize`.
            outline_wraps_lines: false,
            outline_font_size: DEFAULT_OUTLINE_FONT_SIZE,
            folder_find_scope: None,
            folder_find_saved_scopes: BTreeMap::new(),
            // CotEditor ships no snippets of its own, and neither does
            // SubText: the submenu holds only *Edit Snippets…* until one is
            // made.
            snippets: Vec::new(),
            // The file-drop list is the exception: CotEditor ships six, and a
            // fresh SubText drops a picture into Markdown as an image for the
            // same reason a fresh CotEditor does.
            file_drop: FileDropItem::defaults(),
        }
    }
}

impl Settings {
    /// The encoding new documents are created with.
    pub fn encoding(&self) -> TextEncoding {
        TextEncoding::from_identifier(&self.default_encoding).unwrap_or(TextEncoding::UTF8)
    }

    /// The whitespace one indentation step is made of.
    pub fn indent_unit(&self) -> String {
        if self.uses_spaces_for_indentation {
            " ".repeat(self.tab_width)
        } else {
            "\t".to_string()
        }
    }

    /// The options the find bar should start from.
    pub fn find_options(&self) -> subtext_core::FindOptions {
        subtext_core::FindOptions {
            regex: self.find_regex,
            case_sensitive: self.find_case_sensitive,
            whole_word: self.find_whole_word,
            wrap_around: self.find_wrap_around,
            escape_sequences: self.find_escape_sequences,
            in_selection: false,
        }
    }

    /// Bring the indentation fields back into agreement.
    ///
    /// Three fields describe one decision, because three panes and the Format
    /// menu each need a different name for it. They are written through this
    /// method so they cannot drift apart.
    pub fn set_indentation(&mut self, uses_spaces: bool, tab_width: usize) {
        self.edit.auto_expand_tab = uses_spaces;
        self.edit.tab_width = tab_width.clamp(1, 99);
        self.uses_spaces_for_indentation = self.edit.auto_expand_tab;
        self.auto_expand_tabs = self.edit.auto_expand_tab;
        self.tab_width = self.edit.tab_width;
    }

    /// Where the settings file lives.
    pub fn path() -> PathBuf {
        crate::resources::config_directory().join("Settings.json")
    }

    /// Read the settings, falling back to the defaults.
    ///
    /// A corrupt file is not fatal: the user gets a working editor and their
    /// broken file is left alone rather than silently overwritten.
    pub fn load() -> Self {
        let path = Self::path();
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Record a document in the recent list, newest first and de-duplicated.
    pub fn note_recent(&mut self, path: &std::path::Path) {
        self.recent_documents.retain(|existing| existing != path);
        self.recent_documents.insert(0, path.to_path_buf());
        self.recent_documents.truncate(20);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pane_has_a_title_and_a_symbol() {
        assert_eq!(SettingsPane::ALL.len(), 9);
        for pane in SettingsPane::ALL {
            assert!(!pane.title().is_empty(), "{pane:?} has no title");
            assert!(!pane.symbol().is_empty(), "{pane:?} has no symbol");
            assert!(!pane.summary().is_empty(), "{pane:?} has no summary");
        }
        assert_eq!(SettingsPane::Shortcuts.title(), "Shortcuts");
        assert_eq!(SettingsPane::ALL[3], SettingsPane::Edit);
        assert_eq!(SettingsPane::ALL[8], SettingsPane::Integration);
    }

    /// The default encoding is stored as a spelling, not as an index, so a
    /// settings file outlives the list it was written against. Aligning the
    /// registry with upstream's took six encodings out of the menu and merged
    /// four more away; every spelling a build before that could have written
    /// still has to name the encoding it named.
    #[test]
    fn a_default_encoding_the_menu_no_longer_lists_still_resolves() {
        let mut settings = Settings::default();

        // Six entries left the menu for the unlisted tier.
        for (spelling, name) in [
            ("koi8-r", "Cyrillic (KOI8-R)"),
            ("koi8-u", "Ukrainian (KOI8-U)"),
            ("windows-1254", "Turkish (Windows Latin 5)"),
            ("windows-1257", "Baltic (Windows)"),
            ("windows-1258", "Vietnamese (Windows)"),
            ("gbk", "Chinese (GBK)"),
        ] {
            settings.default_encoding = spelling.to_string();
            assert_eq!(settings.encoding().name, name, "{spelling} was lost");
        }

        // Four identifiers were merged into the entry that survived; one of them
        // is not a WHATWG label, so it goes through the alias table.
        for (spelling, name) in [
            ("windows-latin-2", "Central European (Windows Latin 2)"),
            ("iso-8859-2", "Central European (ISO Latin 2)"),
            ("iso-8859-4", "Central European (ISO Latin 4)"),
            ("iso-8859-9", "Turkish (ISO Latin 5)"),
        ] {
            settings.default_encoding = spelling.to_string();
            assert_eq!(settings.encoding().name, name, "{spelling} was lost");
        }

        // And the fallback still stands for a spelling that names nothing.
        settings.default_encoding = "no-such-encoding".to_string();
        assert_eq!(settings.encoding(), TextEncoding::UTF8);
    }

    #[test]
    fn defaults_match_coteditor() {
        let settings = Settings::default();
        assert_eq!(settings.tab_width, 4);
        assert!(settings.uses_spaces_for_indentation);
        assert_eq!(settings.default_line_ending, LineEnding::LF);
        assert!(settings.window.show_tab_bar);
        assert!(!settings.window.show_sidebar);
        assert!(settings.editor.show_line_numbers);
        assert!(!settings.editor.wrap_lines);
        assert_eq!(settings.editor.page_guide_column, 80);
        assert_eq!(settings.appearance.line_height, 1.0);
        assert_eq!(settings.appearance.window_alpha, 1.0);
        assert_eq!(settings.general.no_document_on_launch_option, NoDocumentOnLaunchOption::CreateNewDocument);
        assert_eq!(settings.window.window_tabbing, WindowTabbing::RespectSystem);
    }

    #[test]
    fn indentation_is_written_through_one_door() {
        let mut settings = Settings::default();
        settings.set_indentation(false, 8);

        assert!(!settings.edit.auto_expand_tab);
        assert_eq!(settings.edit.tab_width, 8);
        assert!(!settings.uses_spaces_for_indentation);
        assert!(!settings.auto_expand_tabs);
        assert_eq!(settings.tab_width, 8);
        assert_eq!(settings.indent_unit(), "\t");

        // Out-of-range widths are clamped rather than stored.
        settings.set_indentation(true, 0);
        assert_eq!(settings.edit.tab_width, 1);
        settings.set_indentation(true, 1000);
        assert_eq!(settings.edit.tab_width, 99);
    }

    #[test]
    fn indent_unit_follows_the_spaces_setting() {
        let mut settings = Settings::default();
        assert_eq!(settings.indent_unit(), "    ");
        settings.set_indentation(false, 4);
        assert_eq!(settings.indent_unit(), "\t");
    }

    #[test]
    fn settings_round_trip_through_json() {
        let mut settings = Settings { font_size: 15.5, ..Settings::default() };
        settings.window.show_sidebar = true;
        settings.appearance.line_height = 1.4;
        settings.recent_documents.push(PathBuf::from("/tmp/a.txt"));

        let json = serde_json::to_vec(&settings).unwrap();
        let restored: Settings = serde_json::from_slice(&json).unwrap();
        assert_eq!(restored, settings);
    }

    #[test]
    fn a_partial_settings_file_keeps_defaults_for_missing_keys() {
        let settings: Settings = serde_json::from_str(r#"{"tab_width": 8}"#).unwrap();
        assert_eq!(settings.tab_width, 8);
        assert_eq!(settings.font_size, Settings::default().font_size);
        assert_eq!(settings.appearance.window_alpha, 1.0);
    }

    #[test]
    fn recent_documents_are_deduplicated_and_capped() {
        let mut settings = Settings::default();
        settings.note_recent(std::path::Path::new("/tmp/a"));
        settings.note_recent(std::path::Path::new("/tmp/b"));
        settings.note_recent(std::path::Path::new("/tmp/a"));
        assert_eq!(settings.recent_documents[0], PathBuf::from("/tmp/a"));
        assert_eq!(settings.recent_documents.len(), 2);

        for index in 0..40 {
            settings.note_recent(&PathBuf::from(format!("/tmp/file-{index}")));
        }
        assert_eq!(settings.recent_documents.len(), 20);
    }
}
