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
    ActiveTheme as _, Icon, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogClose, DialogFooter},
    h_flex,
    input::{Editor, Input, InputState},
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, ElementId, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, WeakEntity, Window, WindowOptions, div, px,
};

use subtext_core::{
    Document, LineEnding, TextEncoding, charinfo, colorcode, encoding, finder, multireplace,
    textops,
};
use subtext_syntax::{Syntax, SyntaxManager};
use subtext_theme::{Theme, ThemeManager};

use crate::color_sheet::ColorSheet;
use crate::dialogs::AboutView;
use crate::highlighter;
use crate::multiple_replace_sheet::MultipleReplaceSheet;
use crate::settings::{InspectorPane, Settings, SidebarPane};
use crate::settings_window::SettingsWindow;
use crate::sort_sheet::SortSheet;

/// How long a status message stays on screen.
const STATUS_LIFETIME: std::time::Duration = std::time::Duration::from_secs(4);

/// Stable identity for an open document.
///
/// A document's element identity must not be its position in `documents`.
/// Tabs are closed, and reordering is on the way; keying a tab by its index
/// means closing the first one hands every remaining tab a different
/// `ElementId`, which resets the hover, scroll and transition state of tabs the
/// user never touched. The counter is monotonic for the life of the workspace.
///
/// `TabBar` assigns each of its children a positional id of its own, so this
/// identity is carried by the parts the strip does not own — the close
/// affordance in each tab's suffix slot, and the document's editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(u64);

impl DocumentId {
    /// The element ID for the close affordance inside this document's tab.
    fn tab_close(self) -> ElementId {
        ElementId::NamedInteger("tab-close".into(), self.0)
    }
}

/// One open document and its editor.
pub struct OpenDocument {
    /// Stable identity, independent of the tab's position.
    pub id: DocumentId,
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
    /// Hands out [`DocumentId`]s. Monotonic, never reused, so a closed
    /// document's identity is never inherited by a new one.
    next_document_id: u64,
    find: FindState,
    find_query: Entity<InputState>,
    find_replacement: Entity<InputState>,
    status: Option<Status>,
    /// Set while a document is being loaded, so the change handler does not
    /// mark it dirty.
    loading: bool,
    settings_window: Option<AnyWindowHandle>,
    about_window: Option<AnyWindowHandle>,
    /// The colour the *Edit Color Code* panel opens on when the caret is not on
    /// a colour code.
    ///
    /// CotEditor keeps this in the shared `NSColorPanel`, which survives for the
    /// length of a session; SubText has no shared panel, so the workspace holds
    /// the last colour inserted instead. Like upstream's, it is not persisted.
    last_color_code_color: colorcode::Color,
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
            next_document_id: 1,
            find: FindState::default(),
            find_query,
            find_replacement,
            status: None,
            loading: false,
            settings_window: None,
            about_window: None,
            last_color_code_color: colorcode::Color::BLACK,
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

        let id = DocumentId(self.next_document_id);
        self.next_document_id += 1;

        self.documents.push(OpenDocument {
            id,
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
        match self.active_document() {
            Some(document) => {
                let id = document.id;
                self.close_document(id, window, cx);
            }
            None => self.new_document(window, cx),
        }
    }

    /// Close one document by identity.
    ///
    /// Closing by identity rather than by position is what lets a tab's close
    /// affordance be wired to the tab it belongs to: the active index is not
    /// the clicked tab, and by the time the click runs the list may have moved.
    pub fn close_document(
        &mut self,
        id: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.documents.iter().position(|document| document.id == id) else {
            return;
        };
        if self.documents.len() <= 1 {
            // CotEditor closes the window with the last document; SubText keeps
            // one document open so the application stays usable.
            self.new_document(window, cx);
            return;
        }
        self.documents.remove(index);
        // Keep the selection on the same document where possible, and on the
        // neighbour that took its place when the closed one was selected.
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

    /// Sort the selection by a pattern sort key, as the sheet describes one.
    pub fn sort_lines_by(
        &mut self,
        key: &textops::SortKey,
        options: &textops::SortOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (key, options) = (key.clone(), options.clone());
        self.text_op(window, cx, move |text, selection, _| {
            textops::sort_lines(text, selection, &key, &options)
        });
    }

    /// The line the Pattern Sort sheet shows as its sample: the one the caret
    /// sits on, which is CotEditor's rule as well.
    fn sample_line(&self, cx: &App) -> SharedString {
        let Some(text) = self.active_text(cx) else {
            return SharedString::default();
        };
        let offset = self.active_selection(cx).start.min(text.len());
        let start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
        let end = text[offset..].find('\n').map_or(text.len(), |index| offset + index);
        SharedString::from(text[start..end].to_string())
    }

    /// The *Sort Lines by Pattern* sheet.
    pub fn pattern_sort_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sample = self.sample_line(cx);
        let workspace = Workspace::weak(cx);
        let sheet = cx.new(|cx| SortSheet::new(workspace, sample, window, cx));
        window.open_dialog(cx, move |dialog, window, _| {
            let submit = sheet.clone();
            dialog
                .title("Sort Lines by Pattern")
                // `Dialog::w` takes pixels, not a `Length`, so the interface
                // scale is applied by hand rather than by the layout engine.
                .w(window.rem_size() * 32.)
                .child(sheet.clone())
                // Enter and Escape reach the dialog's own actions, not the
                // buttons the sheet draws, so the callbacks are installed here
                // too. `submit` re-checks the key, so confirming an unfinished
                // pattern is a no-op rather than a surprise.
                .on_ok(move |_, _, cx| {
                    submit.update(cx, |sheet, cx| sheet.submit(cx));
                    true
                })
        });
    }

    /// The text the caret has selected, or empty when it is a bare caret.
    ///
    /// The *Edit Color Code* panel opens on this, the way CotEditor's does.
    fn selected_text(&self, cx: &App) -> String {
        let Some(text) = self.active_text(cx) else {
            return String::new();
        };
        let selection = self.active_selection(cx);
        let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
        if start >= end {
            return String::new();
        }
        // `get` rather than `[..]`: a selection reported in the middle of a
        // multi-byte character would otherwise panic, and a panel opening on an
        // empty field is a better failure than a crash.
        text.get(start..end).unwrap_or_default().to_string()
    }

    /// The *Edit Color Code* panel.
    pub fn color_code_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.selected_text(cx);
        let kind = colorcode::ColorCodeType::from_raw_value(self.settings.color_code_type);
        let fallback = self.last_color_code_color;
        let workspace = Workspace::weak(cx);
        let sheet =
            cx.new(|cx| ColorSheet::new(workspace, &selected, kind, fallback, window, cx));
        window.open_dialog(cx, move |dialog, window, _| {
            let submit = sheet.clone();
            dialog
                .title("Edit Color Code")
                // `Dialog::w` takes pixels, not a `Length`, so the interface
                // scale is applied by hand rather than by the layout engine.
                .w(window.rem_size() * 26.)
                .child(sheet.clone())
                // Enter and Escape reach the dialog's own actions, not the
                // buttons the panel draws. Confirming something that is not a
                // colour code leaves the panel open with the reason in the
                // status bar, rather than closing on a no-op.
                .on_ok(move |_, _, cx| submit.update(cx, |sheet, cx| sheet.submit(cx)))
        });
    }

    /// Put a colour code at the caret, leaving it selected.
    ///
    /// The notation and the colour are remembered past the panel's lifetime:
    /// the notation is CotEditor's `colorCodeType`, which outlives the
    /// application, and the colour stands in for the shared `NSColorPanel`
    /// upstream keeps for the length of a session.
    pub fn insert_color_code(
        &mut self,
        code: &str,
        kind: colorcode::ColorCodeType,
        color: colorcode::Color,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.last_color_code_color = color;
        if self.settings.color_code_type != kind.raw_value() {
            self.settings.color_code_type = kind.raw_value();
            let _ = self.settings.save();
        }

        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let editor = document.editor.clone();
        let code = code.to_string();
        // Deferred for the same reason every other text change is: `replace`
        // emits `Change`, and the subscription that follows the buffer back
        // into the document model must not run inside this update.
        window.defer(cx, {
            let code = code.clone();
            move |window, cx| {
                editor.update(cx, |state, cx| {
                    let start = state.selected_range().start;
                    state.replace(code.clone(), window, cx);
                    // CotEditor leaves the inserted code selected, so pressing
                    // Insert twice replaces the code rather than stacking two.
                    state.set_selected_range(start..start + code.len(), cx);
                });
            }
        });
        self.set_status(format!("Inserted {code}"), cx);
    }

    // ---- multiple replace -------------------------------------------------

    /// The *Multiple Replace* sheet.
    ///
    /// CotEditor's panel is a window with a sidebar of named rule sets; SubText
    /// opens the rule table alone, as a dialog, because there is no set to name
    /// and no file to keep it in yet.
    pub fn multiple_replace_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = Workspace::weak(cx);
        let sheet = cx.new(|cx| MultipleReplaceSheet::new(workspace, window, cx));
        window.open_dialog(cx, move |dialog, window, _| {
            dialog
                .title("Multiple Replace")
                // `Dialog::w` takes pixels, not a `Length`, so the interface
                // scale is applied by hand rather than by the layout engine.
                .w(window.rem_size() * 34.)
                .child(sheet.clone())
                // The panel upstream is a window, and its way out is the window's
                // close button. A dialog needs the same affordance spelled out,
                // and `DialogClose` is the component that owns dismissing one.
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("Done"))),
                )
                // Enter reaches the dialog's confirm action, and there is
                // nothing here for it to confirm: *Replace All* is the button
                // that does the work, and it says so on its own.
                .on_ok(|_, _, _| false)
        });
    }

    /// Run the *Multiple Replace* rules over the active document.
    ///
    /// Returns how many matches were replaced, or `None` when there is no
    /// document to run them over. The sheet shows the number; the status bar
    /// gets it too, so the outcome is not lost if the sheet is behind.
    pub fn apply_multiple_replace(
        &mut self,
        definition: &multireplace::MultipleReplace,
        in_selection: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let text = self.active_text(cx)?;
        let selection = self.active_selection(cx);
        let line_ending = self.active_document().map(|document| document.document.line_ending());

        let multireplace::Outcome { string, selected_range, count } =
            definition.replace(&text, selection, in_selection, line_ending);
        if count == 0 {
            self.set_status("No matches", cx);
            return Some(0);
        }

        let editor = self.documents.get(self.active)?.editor.clone();
        // Deferred for the same reason every other text change is: `replace_all`
        // emits `Change`, and the subscription that follows the buffer back into
        // the document model must not run inside this update.
        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| {
                state.replace_all(string, window, cx);
                // Only a batch confined to a selection reports one; upstream
                // leaves the caret alone when the rules ran over the whole
                // document.
                if let Some(range) = selected_range {
                    state.set_selected_range(range, cx);
                }
            });
        });
        self.set_status(format!("Replaced {count} occurrence(s)"), cx);
        Some(count)
    }

    /// Paint every match the *Multiple Replace* rules describe.
    ///
    /// Returns how many matches were found, or `None` when there is no
    /// document. The ranges go into the same channel the find bar uses, so
    /// *Find Next* steps through them afterwards — SubText's find bar does not
    /// paint its own matches either, and painting them here would be the first
    /// highlight in the application that nothing can turn off.
    pub fn highlight_multiple_replace(
        &mut self,
        definition: &multireplace::MultipleReplace,
        in_selection: bool,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let text = self.active_text(cx)?;
        let selection = self.active_selection(cx);
        let mut ranges = definition.find(&text, selection, in_selection);
        // `find` hands back the hits in rule order, which is the order upstream
        // returns them in; the caller that paints them sorts them first.
        ranges.sort_by_key(|range| range.start);
        let count = ranges.len();
        self.find.matches = ranges;
        self.find.current = None;
        cx.notify();
        Some(count)
    }

    /// Select the word under the caret, as ⌘D does.
    pub fn select_word(&mut self, cx: &mut Context<Self>) {        let Some(text) = self.active_text(cx) else {
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

    /// Select the nearest enclosing bracketed region, delimiters included.
    ///
    /// Like the two commands above, this stays quiet when there is nothing to
    /// select: the caret sitting outside every pair is an ordinary state, not
    /// a failure worth interrupting the user over.
    pub fn select_enclosing_symbols(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let selection = self.active_selection(cx);
        let Some(range) = textops::range_of_enclosing_symbol_pair(
            &text,
            &selection,
            &textops::SymbolPair::ENCLOSING,
        ) else {
            return;
        };
        self.set_selection(range, cx);
    }

    /// Split the selection into one selection per line it covers, so the next
    /// keystroke lands on every line at once — CotEditor's ⌃L.
    ///
    /// The editor exposes only the active selection for reading, so this
    /// splits that one and adds the rest. A document already carrying several
    /// cursors therefore splits the active cursor and leaves the others where
    /// they are.
    pub fn split_selection_by_lines(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let selection = self.active_selection(cx);
        let ranges = textops::line_content_ranges(&text, &selection);
        let Some((first, rest)) = ranges.split_first() else {
            return;
        };
        let (first, rest) = (first.clone(), rest.to_vec());

        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        document.editor.update(cx, |state, cx| {
            state.set_selected_range(first, cx);
            for range in rest {
                state.add_selection(range, cx);
            }
        });
        cx.notify();
    }

    /// Insert `text` at the caret, replacing the selection.
    ///
    /// The two `Insert …` menu items use this: ⌘Y and ⌘¥ exist because a
    /// Japanese keyboard cannot type `\` or `¥` directly, so the menu has to
    /// stand in for a key the hardware does not have.
    pub fn insert_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let editor = document.editor.clone();
        let text = text.to_string();
        // Deferred for the same reason the text operations are: `replace`
        // emits `Change`, and the subscription that follows the buffer back
        // into the document model must not run inside this update.
        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| state.replace(text, window, cx));
        });
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
            let go_value = input.clone();
            // `button_props` wires Enter and Escape to the dialog's own
            // actions but renders nothing on a plain `Dialog`, so the footer
            // is built here as well — otherwise the panel would be a field
            // with no visible way to confirm it.
            let footer_weak = weak.clone();
            let footer = h_flex()
                .w_full()
                .gap_2()
                .justify_end()
                .child(
                    Button::new("goto-cancel")
                        .label("Cancel")
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(Button::new("goto-apply").primary().label("Go").on_click(
                    move |_, window, cx| {
                        let raw = go_value.read(cx).value().to_string();
                        let line = raw.trim().parse::<usize>().unwrap_or(1);
                        footer_weak
                            .update(cx, |workspace, cx| workspace.goto_line(line, cx))
                            .ok();
                        window.close_dialog(cx);
                    },
                ));
            dialog
                .title("Go to Line")
                // `Dialog::w` takes pixels, not a `Length`, so the interface
                // scale is applied by hand rather than by the layout engine.
                .w(window.rem_size() * 20.)
                .child(Input::new(&input))
                .footer(footer)
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
        let width = match self.settings.tab_width {
            2 => 4,
            4 => 8,
            _ => 2,
        };
        let uses_spaces = self.settings.uses_spaces_for_indentation;
        self.settings.set_indentation(uses_spaces, width);
        let _ = self.settings.save();
        self.set_status(format!("Tab width: {}", self.settings.tab_width), cx);
    }

    pub fn toggle_auto_expand_tabs(&mut self, cx: &mut Context<Self>) {
        let uses_spaces = !self.settings.uses_spaces_for_indentation;
        let width = self.settings.tab_width;
        self.settings.set_indentation(uses_spaces, width);
        let _ = self.settings.save();
        self.set_status(
            if uses_spaces { "Tabs expand to spaces" } else { "Tabs insert tab characters" },
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
        window.open_dialog(cx, move |dialog, window, _| {
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
            // `Dialog::w` takes pixels; scale by hand, as above.
            dialog.title("Inspect Character").w(window.rem_size() * 22.).child(body)
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
        window.open_dialog(cx, move |dialog, window, _| {
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
            // `Dialog::w` takes pixels; scale by hand, as above.
            dialog.title("Document Statistics").w(window.rem_size() * 22.).child(body)
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
            // Platform window geometry, not design scale: a minimum window size
            // is a constraint on the OS window, so it stays in pixels.
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
            // As above: an OS window constraint, not a design measurement.
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

        // Every button here is icon-only, so each one carries both an
        // accessible name (for the accessibility tree) and a tooltip (for the
        // pointer). A glyph on its own is not a label.
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
                    .accessibility_label("New document")
                    .tooltip("New document")
                    .on_click(cx.listener(|this, _, window, cx| this.new_tab(window, cx))),
            )
            .child(
                Button::new("open")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::FolderOpen)
                    .accessibility_label("Open document")
                    .tooltip("Open document")
                    .on_click(cx.listener(|this, _, window, cx| this.open_dialog(window, cx))),
            )
            .child(
                Button::new("save")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::HardDrive)
                    .accessibility_label("Save document")
                    .tooltip("Save document")
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
                // The sidebar and inspector toggles show their state the way a
                // toolbar toggle should: pressed when the region is visible.
                Button::new("sidebar")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::PanelLeft)
                    .selected(self.settings.window.show_sidebar)
                    .accessibility_label("Show sidebar")
                    .tooltip("Show sidebar")
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
                    .selected(self.settings.window.show_inspector)
                    .accessibility_label("Show inspector")
                    .tooltip("Show inspector")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.update_window_options(cx, |window| {
                            window.show_inspector = !window.show_inspector
                        })
                    })),
            )
    }

    /// The open-document tab strip.
    ///
    /// Built from `TabBar` rather than clickable `div`s so the strip gets the
    /// component's keyboard navigation, focus ring, selection indicator and
    /// accessibility contract for free. Each tab's close affordance rides in
    /// the tab's `suffix` slot.
    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = self
            .documents
            .iter()
            .map(|document| {
                let id = document.id;
                let mut label = document.document.display_name();
                if document.document.is_dirty() {
                    label.push_str(" — Edited");
                }
                // A `Button`, not a clickable `div`: the close affordance needs
                // focus, a hit area and an accessible name, and the component
                // already supplies all three.
                let close = Button::new(id.tab_close())
                    .ghost()
                    .xsmall()
                    .icon(gpui_kit::assets::IconName::Close)
                    .accessibility_label(format!("Close {label}"))
                    .tooltip("Close tab")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_document(id, window, cx);
                    }));
                Tab::new().label(label).suffix(close)
            })
            .collect::<Vec<_>>();

        let active = self.active.min(tabs.len().saturating_sub(1));

        h_flex()
            .w_full()
            .px_2()
            .bg(cx.theme().secondary)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                TabBar::new("document-tabs")
                    .children(tabs)
                    .selected_index(active)
                    .on_click(cx.listener(|this, index, _, cx| {
                        if *index < this.documents.len() {
                            this.active = *index;
                            cx.notify();
                        }
                    })),
            )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panes = [
            (SidebarPane::FileBrowser, "File Browser", gpui_kit::assets::IconName::Folder),
            (SidebarPane::FolderFind, "Folder Find", gpui_kit::assets::IconName::Search),
        ];
        // A segmented TabBar, not two Buttons with `primary()` on the selected
        // one: this is one control with a current value, so it gets one
        // silhouette, arrow-key navigation and a focus ring.
        let selected = panes
            .iter()
            .position(|(pane, _, _)| *pane == self.settings.sidebar_pane)
            .unwrap_or(0);
        let tabs = panes
            .iter()
            .map(|(_, label, icon)| Tab::new().label(*label).icon(*icon))
            .collect::<Vec<_>>();

        let body = match self.settings.sidebar_pane {
            SidebarPane::FileBrowser => self.render_file_browser(cx),
            SidebarPane::FolderFind => self.render_folder_find(cx),
        };

        v_flex()
            // A navigation sidebar gets a minimum, a preferred and a maximum
            // width, so a long label cannot squeeze it and a short one cannot
            // leave a gap.
            .w(gpui_kit::rems(15.))
            .min_w(gpui_kit::rems(11.))
            .max_w(gpui_kit::rems(24.))
            .h_full()
            .gap_2()
            .p_2()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                TabBar::new("sidebar-panes")
                    .segmented()
                    .small()
                    .children(tabs)
                    .selected_index(selected)
                    .on_click(cx.listener(|this, index, _, cx| {
                        let pane = match index {
                            1 => SidebarPane::FolderFind,
                            _ => SidebarPane::FileBrowser,
                        };
                        this.settings.sidebar_pane = pane;
                        cx.notify();
                    })),
            )
            .child(
                // The body is the scroll owner for this panel: the content can
                // outgrow the pane, and without this the overflow would simply
                // be unreachable.
                div()
                    .id("sidebar-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
    }

    fn render_file_browser(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let mut list = v_flex().gap_0p5();
        if self.settings.recent_documents.is_empty() {
            list = list.child(
                div().text_sm().text_color(cx.theme().muted_foreground).child("No recent files"),
            );
        }
        for path in self.settings.recent_documents.iter().take(30) {
            let label = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            // The path is the row's identity: it is what the user recognises
            // the row by, and it survives the list being reordered by a new
            // document being opened or the history being cleared.
            let id = SharedString::from(format!("recent:{}", path.display()));
            let target = path.clone();
            list = list.child(
                h_flex()
                    .id(id)
                    .px_2()
                    .py_1()
                    .gap_2()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|this| this.bg(cx.theme().accent))
                    .child(Icon::new(gpui_kit::assets::IconName::File).xsmall())
                    .child(div().text_sm().truncate().child(label))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_path(target.clone(), window, cx)
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
        let selected = panes
            .iter()
            .position(|(pane, _)| *pane == self.settings.inspector_pane)
            .unwrap_or(0);
        let tabs = panes
            .iter()
            .map(|(_, label)| Tab::new().label(*label))
            .collect::<Vec<_>>();

        let body = match self.settings.inspector_pane {
            InspectorPane::Document => self.render_document_inspector(cx),
            InspectorPane::Outline => self.render_outline(cx),
            InspectorPane::Warnings => self.render_warnings(cx),
        };

        v_flex()
            .w(gpui_kit::rems(16.))
            .min_w(gpui_kit::rems(12.))
            .max_w(gpui_kit::rems(26.))
            .h_full()
            .gap_2()
            .p_2()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                TabBar::new("inspector-panes")
                    .segmented()
                    .small()
                    .children(tabs)
                    .selected_index(selected)
                    .on_click(cx.listener(|this, index, _, cx| {
                        this.settings.inspector_pane = match index {
                            1 => InspectorPane::Outline,
                            2 => InspectorPane::Warnings,
                            _ => InspectorPane::Document,
                        };
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("inspector-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
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
        for item in items {
            if item.separator {
                // A hairline divider: one physical pixel is the intent, so this
                // is one of the places a raw `px` value is correct.
                list = list.child(div().my_1().h(px(1.)).bg(cx.theme().border));
                continue;
            }
            // The entry's line number is its identity: it is stable for as long
            // as the entry exists, and it is what the entry is *about*.
            let id = ElementId::NamedInteger("outline".into(), item.line as u64);
            // Nesting is expressed in rem so it tracks the base font with the
            // rest of the interface.
            let indent = gpui_kit::rems(item.level as f32 * 0.75);
            let line = item.line;
            let title = item.title.clone();
            list = list.child(
                div()
                    .id(id)
                    .pl(indent)
                    .py_0p5()
                    .rounded_sm()
                    .text_sm()
                    .truncate()
                    .cursor_pointer()
                    .hover(|this| this.bg(cx.theme().accent))
                    .child(title)
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
            .child(div().w(gpui_kit::rems(14.)).child(Input::new(&self.find_query).small()))
            .when(self.find.show_replace, |this| {
                this.child(div().w(gpui_kit::rems(14.)).child(Input::new(&self.find_replacement).small()))
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
                    .accessibility_label("Close the find bar")
                    .tooltip("Close the find bar")
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
                            .h_full()
                            .font_family(font_family)
                            // The document's own type size is user data, not
                            // application zoom: it is what Format ▸ Bigger
                            // Font changes, and it must not move when the
                            // interface scale does.
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
