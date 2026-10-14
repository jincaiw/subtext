//! The main window: documents, the chrome around them, and the command
//! dispatch that the menu and the key bindings both feed.
//!
//! CotEditor is a document-based Cocoa application — one `NSWindow` per
//! document, with a tab bar layered on. SubText keeps a single window and a
//! list of open documents, which is the same model with the window management
//! done in one place instead of by the system.
//!
//! # Why the text operations go through `Window::defer`
//!
//! GPUI panics if an entity is updated while it is already being updated, and
//! a view's own event handlers run *inside* that view's update. SubText
//! subscribes to each editor's `InputEvent::Change` so the document model
//! follows the buffer — which means a text operation applied directly from a
//! command handler would emit `Change`, re-enter the workspace, and trip that
//! assertion.
//!
//! [`Window::defer`] hands the work to the end of the current effect cycle,
//! after the workspace has been returned to the app. The editor can then emit
//! `Change` normally, the subscription fires, and the document model updates
//! itself exactly as it does when the user types. That is also why the
//! operations keep the editor's own undo history: `replace_all` is used rather
//! than `set_value`, so ⌘Z undoes a Shift Right the same way it undoes a
//! keystroke.

use std::ops::Range;
use std::path::PathBuf;

use gpui_kit::AnyWindowHandle;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Editor, Input, InputState},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, WeakEntity, Window, WindowOptions, div, px,
};

use subtext_core::{Document, LineEnding, TextEncoding, charinfo, encoding, finder, textops};
use subtext_syntax::{Syntax, SyntaxManager};
use subtext_theme::{Theme, ThemeManager};

use crate::dialogs::AboutView;
use crate::highlighter;
use crate::settings::{InspectorPane, Settings, SidebarPane};
use crate::settings_window::SettingsWindow;

/// How long a status message stays on screen.
const STATUS_LIFETIME: std::time::Duration = std::time::Duration::from_secs(4);

/// One open document and its editor.
pub struct OpenDocument {
    pub document: Document,
    pub editor: Entity<gpui_kit::base::input::EditorState>,
    pub syntax: SharedString,
    /// Set when the file changed underneath us since it was last read.
    pub modified_on_disk: bool,
    _subscriptions: Vec<Subscription>,
}

impl OpenDocument {
    pub fn title(&self) -> String {
        let mut title = self.document.display_name();
        if self.document.is_dirty() {
            title.push_str(" — Edited");
        }
        if self.modified_on_disk {
            title.push_str(" (modified on disk)");
        }
        title
    }
}

/// The find bar's state.
///
/// The query and the replacement live in their `InputState`s rather than being
/// mirrored here: one source of truth, and no subscription needed to keep a
/// copy in step.
#[derive(Default)]
pub struct FindState {
    pub visible: bool,
    pub show_replace: bool,
    /// Ranges the last search found, kept so "Highlight All" and the status
    /// bar can report them without re-running the search.
    pub matches: Vec<Range<usize>>,
    pub current: Option<usize>,
    pub error: Option<String>,
}

/// A transient message shown in the status bar.
struct Status {
    message: String,
    at: std::time::Instant,
}

pub struct Workspace {
    pub settings: Settings,
    syntaxes: SyntaxManager,
    themes: ThemeManager,
    theme: Theme,
    documents: Vec<OpenDocument>,
    active: usize,
    find: FindState,
    find_query: Entity<InputState>,
    find_replacement: Entity<InputState>,
    status: Option<Status>,
    /// Set while a document is being loaded, so the change handler does not
    /// mark it dirty.
    loading: bool,
    settings_window: Option<AnyWindowHandle>,
    about_window: Option<AnyWindowHandle>,
}

impl Workspace {
    pub fn new(
        syntaxes: SyntaxManager,
        themes: ThemeManager,
        settings: Settings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = settings
            .theme
            .as_deref()
            .and_then(|name| themes.by_name(name))
            .cloned()
            .unwrap_or_else(|| themes.default_theme().clone());

        let find_query = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
        let find_replacement = cx.new(|cx| InputState::new(window, cx).placeholder("Replace"));

        let mut this = Self {
            settings,
            syntaxes,
            themes,
            theme,
            documents: Vec::new(),
            active: 0,
            find: FindState::default(),
            find_query,
            find_replacement,
            status: None,
            loading: false,
            settings_window: None,
            about_window: None,
        };
        this.new_document(window, cx);
        this
    }

    // ---- document management --------------------------------------------

    fn new_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let document =
            Document::untitled(self.settings.encoding(), self.settings.default_line_ending);
        let syntax_name = self
            .settings
            .default_syntax
            .clone()
            .unwrap_or_else(|| self.syntaxes.plain_text().name.clone());
        self.push_document(document, syntax_name, window, cx);
    }

    /// Open a fresh, empty document beside the current one.
    pub fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_document(window, cx);
        self.set_status("New document", cx);
    }

    fn push_document(
        &mut self,
        document: Document,
        syntax_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let syntax = self
            .syntaxes
            .by_name(&syntax_name)
            .cloned()
            .unwrap_or_else(|| self.syntaxes.plain_text().clone());

        let text = document.text().to_string();
        let style = highlighter::editor_style(&self.theme);
        let language = syntax.name.clone();

        let editor = cx.new(|cx| {
            let mut state = gpui_kit::base::input::EditorState::new(window, cx);
            state.set_editor_style(style);
            if let Some(factory) = highlighter::current_factory() {
                state.set_highlighter_factory(factory, cx);
            }
            state.set_value(text, window, cx);
            state.set_highlighter(language, cx);
            state.set_line_number(self.settings.editor.show_line_numbers, window, cx);
            state.set_soft_wrap(self.settings.editor.wrap_lines, window, cx);
            state.set_show_whitespaces(self.settings.editor.show_invisibles, window, cx);
            state.set_indent_guides(self.settings.editor.show_indent_guides, window, cx);
            state.set_readonly(!self.settings.editor.editable, cx);
            state
        });

        let subscription = cx.subscribe_in(
            &editor,
            window,
            |this: &mut Self, _editor, event: &gpui_kit::base::input::InputEvent, window, cx| {
                if matches!(event, gpui_kit::base::input::InputEvent::Change) && !this.loading {
                    this.on_buffer_changed(window, cx);
                }
            },
        );

        self.documents.push(OpenDocument {
            document,
            editor,
            syntax: SharedString::from(syntax_name),
            modified_on_disk: false,
            _subscriptions: vec![subscription],
        });
        self.active = self.documents.len() - 1;
        cx.notify();
    }

    /// Mirror the editor buffer into the document model.
    fn on_buffer_changed(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let text = document.editor.read(cx).value().to_string();
        let current_line_ending = document.document.line_ending();
        document.document.set_text(text.clone(), true);
        // A document whose buffer no longer uses its declared ending has been
        // retyped; keep the model honest rather than lying in the status bar.
        let detected = subtext_core::line_ending::detect(&text);
        if detected != current_line_ending && !text.is_empty() {
            document.document.set_line_ending(detected);
        }
        self.find.matches.clear();
        self.find.current = None;
        cx.notify();
    }

    /// The buffer of the active document, straight from the editor.
    fn active_text(&self, cx: &App) -> Option<String> {
        let document = self.documents.get(self.active)?;
        Some(document.editor.read(cx).value().to_string())
    }

    /// The active document's selection, in byte offsets.
    fn active_selection(&self, cx: &App) -> Range<usize> {
        self.documents
            .get(self.active)
            .map(|document| document.editor.read(cx).selected_range())
            .unwrap_or(0..0)
    }

    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self
            .documents
            .iter()
            .position(|doc| doc.document.path() == Some(path.as_path()))
        {
            self.active = index;
            cx.notify();
            return;
        }

        match Document::open(&path, self.settings.encoding()) {
            Ok(document) => {
                let syntax_name = self.syntaxes.syntax_for_path(&path).name.clone();
                self.settings.note_recent(&path);
                let _ = self.settings.save();
                self.push_document(document, syntax_name, window, cx);
                self.set_status(format!("Opened {}", path.display()), cx);
            }
            Err(error) => self.set_status(error.to_string(), cx),
        }
    }

    /// Open several files at once, as the Open panel can.
    pub fn open_paths(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        for path in paths {
            self.open_path(path, window, cx);
        }
    }

    /// The Open panel.
    ///
    /// The picker is the platform's own, reached through GPUI's async prompt —
    /// the same `NSOpenPanel` CotEditor uses, so the dialog looks native on all
    /// three platforms instead of being a reimplementation of one.
    pub fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui_kit::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Open".into()),
        });
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(paths))) if !paths.is_empty() => {
                    let _ = handle.update(cx, move |_, window, cx| {
                        this.update(cx, |workspace, cx| {
                            workspace.open_paths(paths, window, cx);
                        })
                        .ok();
                    });
                }
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    let _ = handle.update(cx, move |_, _, cx| {
                        this.update(cx, |workspace, cx| {
                            workspace.notice(format!("Open failed: {error}"), cx);
                        })
                        .ok();
                    });
                }
                Err(_) => {}
            }
        })
        .detach();
    }

    /// Save the active document, asking for a path when it has none or when
    /// Save As was chosen.
    pub fn save(&mut self, as_new_path: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.documents.get(self.active).map(|_| self.active) else {
            return;
        };

        // Pull the buffer back out of the editor before writing.
        let text = self.documents[index].editor.read(cx).value().to_string();
        self.documents[index].document.set_text(text, true);

        if as_new_path || self.documents[index].document.is_untitled() {
            let name = self.documents[index].document.display_name();
            let directory = self.documents[index]
                .document
                .path()
                .and_then(|path| path.parent())
                .map(|path| path.to_path_buf())
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")));

            let receiver = cx.prompt_for_new_path(&directory, Some(&name));
            let handle = window.window_handle();
            cx.spawn(async move |this, cx| {
                let chosen = match receiver.await {
                    Ok(Ok(Some(path))) => path,
                    _ => return,
                };
                let _ = handle.update(cx, move |_, window, cx| {
                    this.update(cx, |workspace, cx| {
                        workspace.write_to(Some(chosen), window, cx);
                    })
                    .ok();
                });
            })
            .detach();
            return;
        }

        self.write_to(None, window, cx);
    }

    /// Write the active document, to `target` when one was chosen.
    fn write_to(&mut self, target: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.documents.get(self.active).map(|_| self.active) else {
            return;
        };
        let text = self.documents[index].editor.read(cx).value().to_string();
        self.documents[index].document.set_text(text, true);

        match self.documents[index].document.save(target) {
            Ok(()) => {
                if let Some(path) = self.documents[index].document.path() {
                    let path = path.to_path_buf();
                    let syntax = self.syntaxes.syntax_for_path(&path).name.clone();
                    self.documents[index].syntax = SharedString::from(syntax);
                    self.settings.note_recent(&path);
                    let _ = self.settings.save();
                }
                self.documents[index].modified_on_disk = false;
                let name = self.documents[index].document.display_name();
                self.set_status(format!("Saved {name}"), cx);
            }
            Err(error) => self.set_status(error.to_string(), cx),
        }
        let _ = window;
        cx.notify();
    }

    /// Reload the active document from disk, discarding the buffer.
    pub fn revert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        if document.document.is_untitled() {
            self.set_status("This document has never been saved", cx);
            return;
        }
        match document.document.revert() {
            Ok(()) => {
                let editor = document.editor.clone();
                let text = document.document.text().to_string();
                self.loading = true;
                editor.update(cx, |state, cx| state.set_value(text, window, cx));
                self.loading = false;
                document.modified_on_disk = false;
                self.set_status("Reverted to the saved version", cx);
            }
            Err(error) => self.set_status(error.to_string(), cx),
        }
    }

    /// Close the active document, keeping one open so the window stays usable.
    pub fn close_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.documents.len() <= 1 {
            // CotEditor closes the window with the last document; SubText keeps
            // one document open so the application stays usable.
            self.new_document(window, cx);
            return;
        }
        self.documents.remove(self.active);
        self.active = self.active.min(self.documents.len() - 1);
        cx.notify();
    }

    fn active_document(&self) -> Option<&OpenDocument> {
        self.documents.get(self.active)
    }

    fn set_status(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.status = Some(Status { message: message.into(), at: std::time::Instant::now() });
        cx.notify();
    }

    /// Say something in the status bar.
    pub fn notice(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.set_status(message, cx);
    }

    pub fn clear_recent(&mut self, cx: &mut Context<Self>) {
        self.settings.recent_documents.clear();
        let _ = self.settings.save();
        self.set_status("Cleared the recent documents", cx);
    }

    /// Reveal the active document in the platform's file manager.
    pub fn show_in_finder(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self
            .documents
            .get(self.active)
            .and_then(|document| document.document.path())
            .map(|path| path.to_path_buf())
        else {
            self.set_status("This document has never been saved", cx);
            return;
        };
        cx.reveal_path(&path);
    }

    pub fn copy_path(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self
            .documents
            .get(self.active)
            .and_then(|document| document.document.path())
            .map(|path| path.display().to_string())
        else {
            self.set_status("This document has never been saved", cx);
            return;
        };
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(path.clone()));
        self.set_status(format!("Copied {path}"), cx);
    }

    // ---- text operations -------------------------------------------------

    /// Run a text operation over the active document's selection.
    ///
    /// The work is deferred to the end of the effect cycle so the editor's
    /// `Change` event can reach the subscription without re-entering the
    /// workspace; see the module comment.
    pub fn text_op(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        operation: impl FnOnce(&str, &Range<usize>, &textops::CommentDelimiters) -> textops::Edit,
    ) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let editor = document.editor.clone();
        let delimiters = self
            .syntaxes
            .by_name(&document.syntax)
            .map(Syntax::comment_delimiters)
            .unwrap_or_default();

        // Read the buffer out before touching `cx` again: the borrow of the
        // editor ends at the last use of `state`, so the deferred update below
        // can take `cx` mutably.
        let state = editor.read(cx);
        let text = state.value().to_string();
        let selection = state.selected_range();

        let edit = operation(&text, &selection, &delimiters);
        let (new_text, new_selection) = (edit.text, edit.selection);

        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| {
                state.replace_all(new_text, window, cx);
                state.set_selected_range(new_selection, cx);
            });
        });
    }

    /// Wrap every selected line in `open`/`close`.
    pub fn surround(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        open: &str,
        close: &str,
    ) {
        let (open, close) = (open.to_string(), close.to_string());
        self.text_op(window, cx, move |text, selection, _| {
            textops::surround_lines(text, selection, &open, &close)
        });
    }

    /// Select the word under the caret, as ⌘D does.
    pub fn select_word(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let offset = self.active_selection(cx).start.min(text.len());
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let start = text[..offset]
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_word(*c))
            .last()
            .map_or(offset, |(index, _)| index);
        let end = text[offset..]
            .char_indices()
            .take_while(|(_, c)| is_word(*c))
            .last()
            .map_or(offset, |(index, c)| offset + index + c.len_utf8());
        if start == end {
            return;
        }
        self.set_selection(start..end, cx);
    }

    /// Select the whole line the caret sits on, as ⇧⌘A does.
    pub fn select_line(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let offset = self.active_selection(cx).start.min(text.len());
        let start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
        let end = text[offset..].find('\n').map_or(text.len(), |index| offset + index);
        self.set_selection(start..end, cx);
    }

    fn set_selection(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        document
            .editor
            .update(cx, |state, cx| state.set_selected_range(range, cx));
        cx.notify();
    }

    /// Put the caret at the start of `line` (1-based) and scroll it into view.
    pub fn goto_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let line = line.max(1);
        let offset = textops::offset_of_line(&text, line);
        self.set_selection(offset..offset, cx);
    }

    /// The Go to Line panel, in CotEditor's ⇧⌘L slot.
    pub fn goto_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Line number"));
        let weak = cx.entity().downgrade();
        let focus_input = input.clone();
        window.open_dialog(cx, move |dialog, window, cx| {
            if !focus_input.read(cx).focus_handle(cx).is_focused(window) {
                focus_input.focus_handle(cx).focus(window, cx);
            }
            let weak = weak.clone();
            let input_value = input.clone();
            dialog
                .title("Go to Line")
                .w(px(320.))
                .child(Input::new(&input))
                .button_props(
                    gpui_kit::component::dialog::DialogButtonProps::default()
                        .ok_text("Go")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .on_ok(move |_, _, cx| {
                    let raw = input_value.read(cx).value().to_string();
                    let line = raw.trim().parse::<usize>().unwrap_or(1);
                    weak.update(cx, |workspace, cx| workspace.goto_line(line, cx)).ok();
                    true
                })
        });
    }

    // ---- find ------------------------------------------------------------

    /// The options the find bar should search with.
    pub fn find_options(&self) -> finder::FindOptions {
        self.settings.find_options()
    }

    /// The query, straight from the find field.
    fn query(&self, cx: &App) -> String {
        self.find_query.read(cx).value().to_string()
    }

    fn replacement(&self, cx: &App) -> String {
        self.find_replacement.read(cx).value().to_string()
    }

    /// Show the find bar, optionally with the replace field open.
    pub fn show_find(&mut self, show_replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.find.visible = true;
        if show_replace {
            self.find.show_replace = true;
        }
        self.refresh_matches(cx);
        self.find_query
            .update(cx, |state, cx| state.focus_handle(cx).focus(window, cx));
        cx.notify();
    }

    /// Run the current query and remember where the matches are.
    pub fn refresh_matches(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let query = self.query(cx);
        if query.is_empty() {
            self.find.matches.clear();
            self.find.current = None;
            self.find.error = None;
            cx.notify();
            return;
        }
        let scope = 0..text.len();
        match finder::Searcher::new(&query, &self.find_options()) {
            Ok(searcher) => {
                self.find.matches = searcher.find_all(&text, &scope);
                self.find.error = None;
                if self.find.current.is_some_and(|index| index >= self.find.matches.len()) {
                    self.find.current = None;
                }
            }
            Err(error) => {
                self.find.matches.clear();
                self.find.error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    fn select_match(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(range) = self.find.matches.get(index).cloned() else {
            return;
        };
        self.find.current = Some(index);
        self.set_selection(range, cx);
    }

    /// Step through the matches, wrapping at either end when the settings say
    /// to — CotEditor's default is to wrap.
    pub fn find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.find.matches.is_empty() {
            self.refresh_matches(cx);
        }
        if self.find.matches.is_empty() {
            self.set_status("No matches", cx);
            return;
        }
        let count = self.find.matches.len();
        let next = match self.find.current {
            Some(current) if forward => (current + 1) % count,
            Some(current) => (current + count - 1) % count,
            None if forward => 0,
            None => count - 1,
        };
        self.select_match(next, cx);
        let label = format!("Match {} of {count}", next + 1);
        self.set_status(label, cx);
    }

    /// Select every match, which is what "Find All" does in CotEditor.
    pub fn find_all(&mut self, cx: &mut Context<Self>) {
        self.refresh_matches(cx);
        let count = self.find.matches.len();
        if count == 0 {
            self.set_status("No matches", cx);
            return;
        }
        self.set_status(format!("{count} match(es)"), cx);
    }

    /// Highlight All and Find All are the same query; the difference in
    /// CotEditor is that Highlight All paints without selecting.
    pub fn highlight_all(&mut self, cx: &mut Context<Self>) {
        self.find_all(cx);
    }

    pub fn clear_highlights(&mut self, cx: &mut Context<Self>) {
        self.find.matches.clear();
        self.find.current = None;
        self.set_status("Highlights removed", cx);
    }

    /// Take the selection as the search term.
    pub fn use_selection_for_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let selection = self.active_selection(cx);
        if selection.is_empty() || selection.end > text.len() {
            self.set_status("Select something first", cx);
            return;
        }
        let query = text[selection].to_string();
        self.find_query.update(cx, |state, cx| {
            state.set_value(query.clone(), window, cx);
        });
        self.find.visible = true;
        self.refresh_matches(cx);
        self.set_status(format!("Find: {query}"), cx);
    }

    pub fn use_selection_for_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let selection = self.active_selection(cx);
        if selection.is_empty() || selection.end > text.len() {
            self.set_status("Select something first", cx);
            return;
        }
        let replacement = text[selection].to_string();
        self.find_replacement.update(cx, |state, cx| {
            state.set_value(replacement.clone(), window, cx);
        });
        self.find.visible = true;
        self.find.show_replace = true;
        self.set_status(format!("Replace: {replacement}"), cx);
    }

    /// Replace the current match and move to the next one.
    pub fn replace_and_find_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.matches.is_empty() {
            self.refresh_matches(cx);
        }
        let Some(index) = self.find.current.or(if self.find.matches.is_empty() {
            None
        } else {
            Some(0)
        }) else {
            self.set_status("No matches", cx);
            return;
        };
        let Some(range) = self.find.matches.get(index).cloned() else {
            return;
        };
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let replacement = self.replacement(cx);
        let searcher = match finder::Searcher::new(&self.query(cx), &self.find_options()) {
            Ok(searcher) => searcher,
            Err(error) => {
                self.find.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let (replaced, caret) = finder::replace_one(
            &text,
            &searcher,
            &replacement,
            finder::ReplacementSyntax::Regex,
            &range,
        );

        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let editor = document.editor.clone();
        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| {
                state.replace_all(replaced, window, cx);
                state.set_selected_range(caret..caret, cx);
            });
        });

        self.refresh_matches(cx);
        self.find_step(true, cx);
    }

    /// Replace every match in the document.
    pub fn replace_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let searcher = match finder::Searcher::new(&self.query(cx), &self.find_options()) {
            Ok(searcher) => searcher,
            Err(error) => {
                self.find.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let replacement = self.replacement(cx);
        let scope = 0..text.len();
        let (replaced, count) = finder::replace_all(
            &text,
            &searcher,
            &replacement,
            finder::ReplacementSyntax::Regex,
            &scope,
        );
        if count == 0 {
            self.set_status("No matches", cx);
            return;
        }

        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let editor = document.editor.clone();
        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| state.replace_all(replaced, window, cx));
        });

        self.set_status(format!("Replaced {count} occurrence(s)"), cx);
    }

    // ---- view toggles ----------------------------------------------------

    /// Mutate an editor option and push the result to every open editor.
    pub fn update_editor_options(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        apply: impl FnOnce(&mut crate::settings::EditorOptions),
    ) {
        apply(&mut self.settings.editor);
        self.apply_settings(window, cx);
    }

    /// Re-apply every editor setting to every open document, and persist.
    ///
    /// Called after the settings window changes anything: the settings window
    /// edits the same `Settings` the workspace holds, so one pass here is what
    /// makes the change visible without a restart.
    pub fn apply_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let options = self.settings.editor;
        let editors: Vec<_> = self.documents.iter().map(|d| d.editor.clone()).collect();
        for editor in editors {
            editor.update(cx, |state, cx| {
                state.set_line_number(options.show_line_numbers, window, cx);
                state.set_soft_wrap(options.wrap_lines, window, cx);
                state.set_show_whitespaces(options.show_invisibles, window, cx);
                state.set_indent_guides(options.show_indent_guides, window, cx);
                state.set_readonly(!options.editable, cx);
                cx.notify();
            });
        }
        let _ = self.settings.save();
        cx.notify();
    }

    /// Mutate a window option.
    pub fn update_window_options(
        &mut self,
        cx: &mut Context<Self>,
        apply: impl FnOnce(&mut crate::settings::WindowOptions),
    ) {
        apply(&mut self.settings.window);
        let _ = self.settings.save();
        cx.notify();
    }

    pub fn show_sidebar(&mut self, pane: SidebarPane, cx: &mut Context<Self>) {
        self.settings.window.show_sidebar = true;
        self.settings.sidebar_pane = pane;
        let _ = self.settings.save();
        cx.notify();
    }

    pub fn show_inspector(&mut self, pane: InspectorPane, cx: &mut Context<Self>) {
        self.settings.window.show_inspector = true;
        self.settings.inspector_pane = pane;
        let _ = self.settings.save();
        cx.notify();
    }

    /// Move to the next or previous open document.
    pub fn next_tab(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.documents.len() < 2 {
            return;
        }
        let count = self.documents.len();
        self.active = if forward {
            (self.active + 1) % count
        } else {
            (self.active + count - 1) % count
        };
        cx.notify();
    }

    // ---- format ----------------------------------------------------------

    pub fn set_line_ending(
        &mut self,
        ending: LineEnding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let editor = document.editor.clone();
        document.document.set_line_ending(ending);
        let text = document.document.text().to_string();
        self.loading = true;
        editor.update(cx, |state, cx| state.set_value(text, window, cx));
        self.loading = false;
        self.set_status(format!("Line endings: {}", ending.abbreviation()), cx);
    }

    pub fn change_encoding(
        &mut self,
        encoding: TextEncoding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let editor = document.editor.clone();
        document.document.convert_encoding(encoding, encoding::Bom::None);
        let text = document.document.text().to_string();
        self.loading = true;
        editor.update(cx, |state, cx| state.set_value(text, window, cx));
        self.loading = false;
        let _ = window;
        self.set_status(format!("Encoding: {}", encoding.name), cx);
    }

    /// Reinterpret the bytes on disk with another encoding, without converting.
    pub fn reinterpret_encoding(
        &mut self,
        encoding: TextEncoding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        match document.document.reinterpret(encoding) {
            Ok(()) => {
                let editor = document.editor.clone();
                let text = document.document.text().to_string();
                self.loading = true;
                editor.update(cx, |state, cx| state.set_value(text, window, cx));
                self.loading = false;
                self.set_status(
                    format!("Reinterpreted as {}", encoding.name),
                    cx,
                );
            }
            Err(error) => self.set_status(error.to_string(), cx),
        }
    }

    pub fn set_syntax(&mut self, name: &str, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let editor = document.editor.clone();
        document.syntax = SharedString::from(name.to_string());
        editor.update(cx, |state, cx| state.set_highlighter(name.to_string(), cx));
        self.set_status(format!("Syntax: {name}"), cx);
    }

    /// Force the highlighter to run again, which is what "Reset Syntax
    /// Analysis" means in CotEditor.
    pub fn rehighlight(&mut self, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let name = document.syntax.to_string();
        let editor = document.editor.clone();
        editor.update(cx, |state, cx| {
            state.set_highlighter(name, cx);
            cx.notify();
        });
        cx.notify();
    }

    pub fn set_font(&mut self, family: Option<String>, cx: &mut Context<Self>) {
        self.settings.font_family = family;
        let _ = self.settings.save();
        cx.notify();
    }

    pub fn bump_font(&mut self, delta: f32, cx: &mut Context<Self>) {
        let size = (self.settings.font_size + delta).clamp(6.0, 96.0);
        self.set_font_size(size, cx);
    }

    pub fn set_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        self.settings.font_size = size.clamp(6.0, 96.0);
        let _ = self.settings.save();
        self.set_status(format!("Font size: {:.0} pt", self.settings.font_size), cx);
    }

    pub fn toggle_antialias(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.update_editor_options(window, cx, |options| options.antialias = !options.antialias);
        let on = self.settings.editor.antialias;
        self.set_status(format!("Antialiasing {}", if on { "on" } else { "off" }), cx);
    }

    pub fn toggle_ligatures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.update_editor_options(window, cx, |options| options.ligatures = !options.ligatures);
        let on = self.settings.editor.ligatures;
        self.set_status(format!("Ligatures {}", if on { "on" } else { "off" }), cx);
    }

    /// Step through the tab widths CotEditor offers: 2, 4, 8.
    pub fn cycle_tab_width(&mut self, cx: &mut Context<Self>) {
        self.settings.tab_width = match self.settings.tab_width {
            2 => 4,
            4 => 8,
            _ => 2,
        };
        let _ = self.settings.save();
        self.set_status(format!("Tab width: {}", self.settings.tab_width), cx);
    }

    pub fn toggle_auto_expand_tabs(&mut self, cx: &mut Context<Self>) {
        self.settings.auto_expand_tabs = !self.settings.auto_expand_tabs;
        self.settings.uses_spaces_for_indentation = self.settings.auto_expand_tabs;
        let _ = self.settings.save();
        let on = self.settings.auto_expand_tabs;
        self.set_status(
            if on { "Tabs expand to spaces" } else { "Tabs insert tab characters" },
            cx,
        );
    }

    // ---- theme -----------------------------------------------------------

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme.clone();
        self.settings.theme = Some(theme.name.clone());

        let mode = if theme.is_dark() {
            gpui_kit::component::ThemeMode::Dark
        } else {
            gpui_kit::component::ThemeMode::Light
        };
        gpui_kit::component::Theme::change(mode, None, cx);

        let style = highlighter::editor_style(&theme);
        let editors: Vec<_> = self.documents.iter().map(|d| d.editor.clone()).collect();
        for editor in editors {
            editor.update(cx, |state, cx| {
                state.set_editor_style(style.clone());
                cx.notify();
            });
        }
        let _ = self.settings.save();
        self.set_status(format!("Theme: {}", theme.name), cx);
        cx.refresh_windows();
    }

    pub fn apply_theme_by_name(&mut self, name: &str, cx: &mut Context<Self>) {
        let theme = self
            .themes
            .by_name(name)
            .cloned()
            .unwrap_or_else(|| self.themes.default_theme().clone());
        self.set_theme(theme, cx);
    }

    /// The names of every theme, for the Format ▸ Theme menu.
    pub fn theme_names(&self) -> Vec<String> {
        self.themes.all().iter().map(|theme| theme.name.clone()).collect()
    }

    /// The names of every syntax, for the Format ▸ Syntax menu.
    pub fn syntax_names(&self) -> Vec<String> {
        self.syntaxes.all().iter().map(|syntax| syntax.name.clone()).collect()
    }

    pub fn line_endings(&self) -> [LineEnding; 6] {
        subtext_core::line_ending::ALL
    }

    pub fn encodings(&self) -> &'static [TextEncoding] {
        subtext_core::encoding::ALL
    }

    // ---- inspectors ------------------------------------------------------

    /// The Text ▸ Inspect Character panel.
    pub fn inspect_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let selection = self.active_selection(cx);
        if selection.is_empty() || selection.end > text.len() {
            self.set_status("Select a character first", cx);
            return;
        }
        let character = text[selection.clone()].chars().next().unwrap_or(' ');
        let encoding = self
            .documents
            .get(self.active)
            .map_or(TextEncoding::UTF8, |document| document.document.encoding());
        let info = charinfo::CharacterInfo::new(character, encoding);

        let rows = vec![
            ("Character", info.character.to_string()),
            ("Name", info.name.clone()),
            ("Code Point", info.code_point.clone()),
            ("Category", info.category.to_string()),
            ("Block", info.block.to_string()),
            ("UTF-8", info.utf8.clone()),
            ("UTF-16", info.utf16.clone()),
            ("Decimal", info.decimal.to_string()),
            ("HTML Entity", info.html_entity.clone()),
            ("Width", info.width.to_string()),
        ];
        window.open_dialog(cx, move |dialog, _, _| {
            let mut body = v_flex().gap_1().p_1();
            for (label, value) in &rows {
                body = body.child(
                    h_flex()
                        .gap_3()
                        .justify_between()
                        .child(div().text_xs().child(label.to_string()))
                        .child(div().text_xs().child(value.clone())),
                );
            }
            dialog.title("Inspect Character").w(px(360.)).child(body)
        });
    }

    /// The advanced character counter, which CotEditor puts behind ⌥⌘C.
    pub fn inspect_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let encoding = self
            .documents
            .get(self.active)
            .map_or(TextEncoding::UTF8, |document| document.document.encoding());
        let count = charinfo::CharacterCount::of(&text, encoding);
        let rows = vec![
            ("Characters", count.characters.to_string()),
            ("Characters (no newlines)", count.characters_without_newlines.to_string()),
            ("Unicode Scalars", count.unicode_scalars.to_string()),
            ("UTF-16 Units", count.utf16_units.to_string()),
            ("UTF-8 Bytes", count.utf8_bytes.to_string()),
            ("Bytes (encoded)", count.encoded_bytes.to_string()),
            ("Lines", count.lines.to_string()),
            ("Words", count.words.to_string()),
            ("Paragraphs", count.paragraphs.to_string()),
        ];
        window.open_dialog(cx, move |dialog, _, _| {
            let mut body = v_flex().gap_1().p_1();
            for (label, value) in &rows {
                body = body.child(
                    h_flex()
                        .gap_3()
                        .justify_between()
                        .child(div().text_xs().child(label.to_string()))
                        .child(div().text_xs().child(value.clone())),
                );
            }
            dialog.title("Document Statistics").w(px(360.)).child(body)
        });
    }

    // ---- windows ---------------------------------------------------------

    /// Open the settings window, or bring the existing one forward.
    pub fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.settings_window {
            if handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
                return;
            }
            self.settings_window = None;
        }

        let workspace = cx.entity().downgrade();
        let options = WindowOptions {
            window_min_size: Some(gpui_kit::size(px(720.), px(480.))),
            ..Default::default()
        };
        match cx.open_window(options, move |window, cx| {
            cx.new(|cx| SettingsWindow::new(workspace, window, cx))
        }) {
            Ok(handle) => self.settings_window = Some(handle.into()),
            Err(error) => self.set_status(error.to_string(), cx),
        }
    }

    pub fn show_about(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.about_window {
            if handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
                return;
            }
            self.about_window = None;
        }
        let options = WindowOptions {
            window_min_size: Some(gpui_kit::size(px(420.), px(300.))),
            is_resizable: false,
            ..Default::default()
        };
        match cx.open_window(options, |_, cx| cx.new(|_| AboutView::new())) {
            Ok(handle) => self.about_window = Some(handle.into()),
            Err(error) => self.set_status(error.to_string(), cx),
        }
    }

    // ---- rendering -------------------------------------------------------

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let document = self.active_document();
        let title = document.map_or_else(|| "SubText".to_string(), OpenDocument::title);

        h_flex()
            .w_full()
            .px_3()
            .py_1p5()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("new")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::Plus)
                    .on_click(cx.listener(|this, _, window, cx| this.new_tab(window, cx))),
            )
            .child(
                Button::new("open")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::FolderOpen)
                    .on_click(cx.listener(|this, _, window, cx| this.open_dialog(window, cx))),
            )
            .child(
                Button::new("save")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::HardDrive)
                    .on_click(cx.listener(|this, _, window, cx| this.save(false, window, cx))),
            )
            .child(
                div()
                    .flex_1()
                    .text_center()
                    .text_sm()
                    .truncate()
                    .child(title),
            )
            .child(
                Button::new("sidebar")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::PanelLeft)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.update_window_options(cx, |window| {
                            window.show_sidebar = !window.show_sidebar
                        })
                    })),
            )
            .child(
                Button::new("inspector")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::PanelRight)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.update_window_options(cx, |window| {
                            window.show_inspector = !window.show_inspector
                        })
                    })),
            )
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = self
            .documents
            .iter()
            .enumerate()
            .map(|(index, document)| {
                let is_active = index == self.active;
                h_flex()
                    .id(SharedString::from(format!("tab-{index}")))
                    .px_3()
                    .py_1()
                    .gap_2()
                    .rounded_t_md()
                    .when(is_active, |this| this.bg(cx.theme().background))
                    .when(!is_active, |this| this.text_color(cx.theme().muted_foreground))
                    .cursor_pointer()
                    .child(div().text_sm().child(document.document.display_name()))
                    .when(document.document.is_dirty(), |this| this.child("•"))
                    .child(
                        // The close glyph is its own hit target, so it is
                        // wrapped rather than made clickable: `Icon` paints a
                        // glyph and has no interactivity of its own.
                        div()
                            .id(SharedString::from(format!("tab-close-{index}")))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.active = index;
                                this.close_active(window, cx);
                            }))
                            .child(Icon::new(gpui_kit::assets::IconName::Close).xsmall()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active = index;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        h_flex()
            .w_full()
            .px_2()
            .bg(cx.theme().secondary)
            .border_b_1()
            .border_color(cx.theme().border)
            .children(tabs)
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panes = [
            (SidebarPane::FileBrowser, "File Browser", gpui_kit::assets::IconName::Folder),
            (SidebarPane::FolderFind, "Folder Find", gpui_kit::assets::IconName::Search),
        ];
        let buttons = panes
            .into_iter()
            .map(|(pane, label, icon)| {
                let selected = self.settings.sidebar_pane == pane;
                Button::new(label)
                    .small()
                    .when(selected, |this| this.primary())
                    .when(!selected, |this| this.ghost())
                    .icon(icon)
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings.sidebar_pane = pane;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let body = match self.settings.sidebar_pane {
            SidebarPane::FileBrowser => self.render_file_browser(cx),
            SidebarPane::FolderFind => self.render_folder_find(cx),
        };

        v_flex()
            .w(px(240.))
            .h_full()
            .gap_2()
            .p_2()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(v_flex().gap_1().children(buttons))
            .child(div().flex_1().overflow_hidden().child(body))
    }

    fn render_file_browser(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let mut list = v_flex().gap_0p5();
        if self.settings.recent_documents.is_empty() {
            list = list.child(
                div().text_sm().text_color(cx.theme().muted_foreground).child("No recent files"),
            );
        }
        for (index, path) in self.settings.recent_documents.iter().take(30).enumerate() {
            let label = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let path = path.clone();
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("recent-{index}")))
                    .px_2()
                    .py_1()
                    .gap_2()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|this| this.bg(cx.theme().accent))
                    .child(Icon::new(gpui_kit::assets::IconName::File).xsmall())
                    .child(div().text_sm().truncate().child(label))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_path(path.clone(), window, cx)
                    })),
            );
        }
        list.into_any_element()
    }

    fn render_folder_find(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Search a folder for text, like CotEditor's Folder Find."),
            )
            .child(
                Button::new("choose-folder")
                    .small()
                    .label("Choose Folder…")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_status("Folder Find is not implemented yet", cx)
                    })),
            )
            .into_any_element()
    }

    fn render_inspector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panes = [
            (InspectorPane::Document, "Document"),
            (InspectorPane::Outline, "Outline"),
            (InspectorPane::Warnings, "Warnings"),
        ];
        let tabs = panes
            .into_iter()
            .map(|(pane, label)| {
                let selected = self.settings.inspector_pane == pane;
                Button::new(label)
                    .xsmall()
                    .when(selected, |this| this.primary())
                    .when(!selected, |this| this.ghost())
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings.inspector_pane = pane;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let body = match self.settings.inspector_pane {
            InspectorPane::Document => self.render_document_inspector(cx),
            InspectorPane::Outline => self.render_outline(cx),
            InspectorPane::Warnings => self.render_warnings(cx),
        };

        v_flex()
            .w(px(260.))
            .h_full()
            .gap_2()
            .p_2()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(h_flex().gap_1().children(tabs))
            .child(div().flex_1().overflow_hidden().child(body))
    }

    fn render_document_inspector(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let Some(document) = self.active_document() else {
            return v_flex().into_any_element();
        };
        let model = &document.document;
        let text = document.editor.read(cx).value().to_string();
        let count = charinfo::CharacterCount::of(&text, model.encoding());

        let rows: Vec<(&str, String)> = vec![
            ("File Name", model.display_name()),
            ("File Size", format_size(count.encoded_bytes)),
            ("Encoding", model.encoding().name.to_string()),
            ("BOM", model.bom().to_string()),
            ("Line Endings", model.line_ending().to_string()),
            ("Syntax", document.syntax.to_string()),
            ("Lines", count.lines.to_string()),
            ("Characters", count.characters.to_string()),
            ("Words", count.words.to_string()),
            ("Paragraphs", count.paragraphs.to_string()),
            ("UTF-16 Units", count.utf16_units.to_string()),
            ("UTF-8 Bytes", count.utf8_bytes.to_string()),
        ];

        let mut list = v_flex().gap_1();
        for (label, value) in rows {
            list = list.child(
                h_flex()
                    .gap_2()
                    .justify_between()
                    .child(
                        div().text_xs().text_color(cx.theme().muted_foreground).child(label),
                    )
                    .child(div().text_xs().truncate().child(value)),
            );
        }
        list.into_any_element()
    }

    fn render_outline(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let Some(document) = self.active_document() else {
            return v_flex().into_any_element();
        };
        let Some(syntax) = self.syntaxes.by_name(&document.syntax) else {
            return v_flex().into_any_element();
        };
        let text = document.editor.read(cx).value().to_string();
        let items = crate::outline::extract(&text, &syntax.outlines);

        if items.is_empty() {
            return v_flex()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No outline items"),
                )
                .into_any_element();
        }

        let mut list = v_flex().gap_0p5();
        for (index, item) in items.into_iter().enumerate() {
            if item.separator {
                list = list.child(div().my_1().h(px(1.)).bg(cx.theme().border));
                continue;
            }
            let indent = item.level as f32 * 10.0;
            let line = item.line;
            list = list.child(
                div()
                    .id(SharedString::from(format!("outline-{index}")))
                    .pl(px(indent))
                    .py_0p5()
                    .text_sm()
                    .truncate()
                    .cursor_pointer()
                    .hover(|this| this.bg(cx.theme().accent))
                    .child(item.title)
                    .on_click(cx.listener(move |this, _, _, cx| this.goto_line(line, cx))),
            );
        }
        list.into_any_element()
    }

    fn render_warnings(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let Some(document) = self.active_document() else {
            return v_flex().into_any_element();
        };
        let warnings = crate::outline::document_warnings(document, cx);
        if warnings.is_empty() {
            return v_flex()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No warnings"),
                )
                .into_any_element();
        }
        let mut list = v_flex().gap_1();
        for warning in warnings {
            list = list.child(div().text_sm().child(warning));
        }
        list.into_any_element()
    }

    fn render_navigation_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let document = self.active_document();
        let label = document
            .map(|document| {
                format!("{} — {}", document.syntax, document.document.line_ending())
            })
            .unwrap_or_else(|| "Plain Text".to_string());
        h_flex()
            .w_full()
            .px_3()
            .py_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(label))
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let document = self.active_document();
        let position = document
            .map(|document| {
                let state = document.editor.read(cx);
                let offset = state.selected_range().start;
                let text = state.value().to_string();
                textops::line_and_column(&text, offset)
            })
            .unwrap_or((1, 1));
        let stale = self
            .status
            .as_ref()
            .is_some_and(|status| status.at.elapsed() > STATUS_LIFETIME);
        let message = if stale {
            format!("Line {}, Column {}", position.0, position.1)
        } else {
            self.status.as_ref().map_or_else(
                || format!("Line {}, Column {}", position.0, position.1),
                |status| status.message.clone(),
            )
        };
        let count = self.find.matches.len();

        h_flex()
            .w_full()
            .px_3()
            .py_1()
            .gap_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().text_xs().truncate().child(message))
            .child(div().flex_1())
            .when(count > 0, |this| {
                this.child(div().text_xs().child(format!("{count} matches")))
            })
    }

    fn render_find_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let error = self.find.error.clone();
        let count = self.find.matches.len();
        let current = self.find.current.map_or(0, |index| index + 1);

        h_flex()
            .w_full()
            .px_3()
            .py_1p5()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .child(div().w(px(220.)).child(Input::new(&self.find_query).small()))
            .when(self.find.show_replace, |this| {
                this.child(div().w(px(220.)).child(Input::new(&self.find_replacement).small()))
            })
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .when(error.is_some(), |this| this.text_color(cx.theme().danger))
                    .child(match &error {
                        Some(error) => error.clone(),
                        None if count > 0 => format!("{current} of {count}"),
                        None => String::new(),
                    }),
            )
            .child(
                Button::new("find-prev")
                    .xsmall()
                    .label("Previous")
                    .on_click(cx.listener(|this, _, _, cx| this.find_step(false, cx))),
            )
            .child(
                Button::new("find-next")
                    .xsmall()
                    .label("Next")
                    .on_click(cx.listener(|this, _, _, cx| this.find_step(true, cx))),
            )
            .when(self.find.show_replace, |this| {
                this.child(
                    Button::new("replace-next")
                        .xsmall()
                        .label("Replace")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.replace_and_find_next(window, cx)
                        })),
                )
                .child(
                    Button::new("replace-all")
                        .xsmall()
                        .label("Replace All")
                        .on_click(cx.listener(|this, _, window, cx| this.replace_all(window, cx))),
                )
            })
            .child(
                Button::new("find-close")
                    .xsmall()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::Close)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.find.visible = false;
                        this.find.matches.clear();
                        cx.notify();
                    })),
            )
    }
}

/// Format a byte count the way CotEditor's inspector does.
pub fn format_size(bytes: usize) -> String {
    const UNITS: [&str; 4] = ["bytes", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.active_document().map_or_else(
            || "SubText".to_string(),
            |document| document.title(),
        );
        window.set_window_title(&title);

        let mut root = v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground);

        if self.settings.window.show_toolbar {
            root = root.child(self.render_toolbar(cx));
        }
        if self.settings.window.show_tab_bar && self.documents.len() > 1 {
            root = root.child(self.render_tab_bar(cx));
        }
        if self.find.visible {
            root = root.child(self.render_find_bar(cx));
        }

        let editor_area = match self.active_document() {
            Some(document) => {
                let editor = document.editor.clone();
                let font_family = self
                    .settings
                    .font_family
                    .clone()
                    .unwrap_or_else(|| cx.theme().mono_font_family.to_string());
                v_flex()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .child(
                        // `Editor`, not `Input`: the styled code editor, whose
                        // state carries line numbers, folding and the
                        // highlighter. `Input` only takes an `InputState`.
                        Editor::new(&editor)
                            .bordered(false)
                            .p_2()
                            .h(gpui_kit::relative(1.))
                            .font_family(font_family)
                            .text_size(px(self.settings.font_size))
                            .into_any_element(),
                    )
                    .into_any_element()
            }
            None => div().flex_1().into_any_element(),
        };

        let mut middle = h_flex().flex_1().min_h_0();
        if self.settings.window.show_sidebar {
            middle = middle.child(self.render_sidebar(cx));
        }
        middle = middle.child(editor_area);
        if self.settings.window.show_inspector {
            middle = middle.child(self.render_inspector(cx));
        }
        root = root.child(middle);

        if self.settings.window.show_navigation_bar {
            root = root.child(self.render_navigation_bar(cx));
        }
        if self.settings.window.show_status_bar {
            root = root.child(self.render_status_bar(cx));
        }
        root
    }
}

impl Workspace {
    pub fn themes(&self) -> &ThemeManager {
        &self.themes
    }

    pub fn syntaxes(&self) -> &SyntaxManager {
        &self.syntaxes
    }

    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    /// The workspace handle, for views that need to talk back to it.
    pub fn weak(cx: &Context<Self>) -> WeakEntity<Self> {
        cx.entity().downgrade()
    }
}

/// Load every `.cotsyntax` bundle the resource directory offers.
///
/// A user's own bundle with the same name wins, so a customised `Rust.cotsyntax`
/// in the support directory overrides the shipped one without a rebuild —
/// the same override contract CotEditor has.
pub fn load_syntaxes(resources: Option<&crate::resources::Resources>) -> SyntaxManager {
    let mut all: Vec<Syntax> = Vec::new();
    if let Some(resources) = resources
        && let Ok(manager) = SyntaxManager::load_from_directory(resources.syntaxes()) {
            all.extend(manager.all().iter().cloned());
        }
    let user = crate::resources::user_syntax_directory();
    if user.is_dir()
        && let Ok(manager) = SyntaxManager::load_from_directory(&user) {
            all.extend(manager.all().iter().cloned());
        }
    if all.is_empty() {
        all.push(Syntax::plain_text());
    }
    SyntaxManager::from_syntaxes(all)
}

/// Load every `.cottheme` the resource directory offers, falling back to the
/// two built-in palettes.
pub fn load_themes(resources: Option<&crate::resources::Resources>) -> ThemeManager {
    let mut themes: Vec<Theme> = Vec::new();
    if let Some(resources) = resources
        && let Ok(manager) = ThemeManager::load_from_directory(resources.themes()) {
            themes.extend(manager.all().iter().cloned());
        }
    let user = crate::resources::user_theme_directory();
    if user.is_dir()
        && let Ok(manager) = ThemeManager::load_from_directory(&user) {
            for theme in manager.all() {
                themes.retain(|existing| existing.name != theme.name);
                themes.push(theme.clone());
            }
        }
    if themes.is_empty() {
        return ThemeManager::builtin();
    }
    ThemeManager::from_themes(themes)
}
