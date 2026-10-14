//! The application's persisted preferences.
//!
//! CotEditor's settings window has five panes; SubText mirrors the ones that
//! change editor behaviour and drops the ones that only exist to talk to
//! macOS (`General ▸ Services`, for instance). Everything here round-trips
//! through a JSON file in the user's config directory.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use subtext_core::encoding::TextEncoding;
use subtext_core::line_ending::LineEnding;

/// Which pane of the settings window is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SettingsPane {
    #[default]
    General,
    Appearance,
    Editor,
    Format,
    KeyBindings,
}

impl SettingsPane {
    pub const ALL: [SettingsPane; 5] = [
        SettingsPane::General,
        SettingsPane::Appearance,
        SettingsPane::Editor,
        SettingsPane::Format,
        SettingsPane::KeyBindings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            SettingsPane::General => "General",
            SettingsPane::Appearance => "Appearance",
            SettingsPane::Editor => "Editor",
            SettingsPane::Format => "Format",
            SettingsPane::KeyBindings => "Key Bindings",
        }
    }

    /// The SF Symbol-ish name used for the pane's sidebar icon.
    pub fn icon(self) -> &'static str {
        match self {
            SettingsPane::General => "settings",
            SettingsPane::Appearance => "palette",
            SettingsPane::Editor => "text-cursor",
            SettingsPane::Format => "file-code",
            SettingsPane::KeyBindings => "keyboard",
        }
    }
}

/// What appears in the editor's gutter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EditorOptions {
    pub show_line_numbers: bool,
    pub show_invisibles: bool,
    pub show_indent_guides: bool,
    pub show_page_guide: bool,
    pub wrap_lines: bool,
    pub highlight_current_line: bool,
    pub antialias: bool,
    pub ligatures: bool,
    pub editable: bool,
}

impl EditorOptions {
    /// CotEditor's defaults for a new document.
    pub fn coteditor_defaults() -> Self {
        Self {
            show_line_numbers: true,
            show_invisibles: false,
            show_indent_guides: false,
            show_page_guide: false,
            wrap_lines: false,
            highlight_current_line: true,
            antialias: true,
            ligatures: false,
            editable: true,
        }
    }
}

/// Which chrome is visible around the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WindowOptions {
    pub show_tab_bar: bool,
    pub show_toolbar: bool,
    pub show_sidebar: bool,
    pub show_inspector: bool,
    pub show_navigation_bar: bool,
    pub show_status_bar: bool,
    pub keep_on_top: bool,
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
        }
    }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub editor: EditorOptions,
    pub window: WindowOptions,
    pub sidebar_pane: SidebarPane,
    pub inspector_pane: InspectorPane,

    /// The theme applied to the editor. `None` means "follow the system".
    pub theme: Option<String>,
    pub font_family: Option<String>,
    pub font_size: f32,

    pub tab_width: usize,
    pub uses_spaces_for_indentation: bool,
    pub auto_expand_tabs: bool,

    pub default_encoding: String,
    pub default_line_ending: LineEnding,
    pub default_syntax: Option<String>,

    /// Continue a find from the other end of the document.
    pub find_wrap_around: bool,
    /// Interpret `\n` and friends in the find field.
    pub find_escape_sequences: bool,
    pub find_regex: bool,
    pub find_case_sensitive: bool,
    pub find_whole_word: bool,

    /// The most recently opened documents, newest first.
    pub recent_documents: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            editor: EditorOptions::coteditor_defaults(),
            window: WindowOptions::coteditor_defaults(),
            sidebar_pane: SidebarPane::default(),
            inspector_pane: InspectorPane::default(),
            theme: None,
            font_family: None,
            font_size: 13.0,
            tab_width: 4,
            uses_spaces_for_indentation: true,
            auto_expand_tabs: true,
            default_encoding: TextEncoding::UTF8.identifier(),
            default_line_ending: LineEnding::LF,
            default_syntax: None,
            find_wrap_around: true,
            find_escape_sequences: false,
            find_regex: false,
            find_case_sensitive: false,
            find_whole_word: false,
            recent_documents: Vec::new(),
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
    fn defaults_match_coteditor() {
        let settings = Settings::default();
        assert_eq!(settings.tab_width, 4);
        assert!(settings.uses_spaces_for_indentation);
        assert_eq!(settings.default_line_ending, LineEnding::LF);
        assert!(settings.window.show_tab_bar);
        assert!(!settings.window.show_sidebar);
        assert!(settings.editor.show_line_numbers);
        assert!(!settings.editor.wrap_lines);
    }

    #[test]
    fn indent_unit_follows_the_spaces_setting() {
        let mut settings = Settings::default();
        assert_eq!(settings.indent_unit(), "    ");
        settings.uses_spaces_for_indentation = false;
        assert_eq!(settings.indent_unit(), "\t");
    }

    #[test]
    fn settings_round_trip_through_json() {
        let mut settings = Settings { font_size: 15.5, ..Settings::default() };
        settings.window.show_sidebar = true;
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
