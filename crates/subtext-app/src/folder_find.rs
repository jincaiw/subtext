//! The *Folder Find* sidebar pane and the sheets that configure it.
//!
//! CotEditor's Folder Find is a view on a *directory document*: a search field,
//! a Text/Regular-Expression menu, a case switch, a file-scope menu, a metrics
//! bar, and a list of files with the matching lines under each. SubText keeps
//! all of that; the folder is taken from the active document, because SubText
//! has no directory documents, and the pane lets the user point it elsewhere.
//!
//! # Where the work happens
//!
//! Searching a folder is unbounded work: the walk touches every file under the
//! root, and a large tree takes seconds. Doing it on the UI thread would freeze
//! the window for that long, so the walk runs on the background executor and
//! reports through [`subtext_core::folderfind::Progress`] — a mutex-guarded
//! snapshot the pane reads while it renders. A ticker repaints the pane every
//! [`PROGRESS_INTERVAL`] so the counters move, and stops as soon as the search
//! does. A search that is superseded is cancelled and its answer dropped, so a
//! slow walk cannot overwrite a newer one's results.
//!
//! # What is deliberately different from upstream
//!
//! * Upstream's file-scope editor is `NSRuleEditor`, a Cocoa control GPUI has
//!   no counterpart for. The rules are the same — target, comparison, value —
//!   laid out as rows, with an *Add Condition* button where `NSRuleEditor`
//!   grows a row on demand.
//! * Upstream's *Save as Named Scope…* is a sheet on top of a sheet. Here it is
//!   an inline name field: the nested sheet buys nothing in a dialog that is
//!   already modal.
//! * Upstream's *Manage Saved Scopes…* is a separate window; this is a dialog
//!   with the same three operations, and its rename and delete replace the
//!   context menu upstream puts on the list.
//! * *Reveal in File Browser* is dropped. SubText's File Browser is a list of
//!   recent documents, so there is nothing to reveal a result *in*; *Show in
//!   Finder* and *Copy Path* carry the useful half of that menu.
//! * Dragging a result onto another application is not reproduced: GPUI has no
//!   cross-application drag source.
//! * A result opens in a tab of this window rather than in a new window, which
//!   is SubText's one-window document model.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::base::input::InputEvent;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonCustomVariant, ButtonVariants as _},
    dialog::{DialogClose, DialogFooter},
    empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle},
    h_flex,
    input::{Input, InputState},
    menu::{ContextMenuExt as _, PopupMenuItem},
    popover::Popover,
    separator::Separator,
    spinner::Spinner,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Div, ElementId, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window, div, px, rems,
};

use subtext_core::folderfind::{
    Cancellation, Comparison, Conjunction, FileResult, FileScope, InclusionPredicate, Match,
    Metrics, Mode, Options, Progress, Query, Rule, Search, Summary, Target,
};

use crate::settings::Settings;
use crate::workspace::Workspace;

/// How often the pane repaints itself while a search is running.
///
/// Upstream drives its metrics bar from a `TimelineView` at the same interval.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(120);

/// How many characters of a line to keep in front of the match when the line
/// has to be shortened — upstream's `truncationHeadOffset`.
const TRUNCATION_HEAD_OFFSET: usize = 32;

/// The width of the sheets' label column.
const SHEET_LABEL_WIDTH: f32 = 9.;

// ---------------------------------------------------------------------------
// The pane
// ---------------------------------------------------------------------------

/// What the pane is doing right now — upstream's `FolderFinder.SearchState`,
/// one variant per arm of its `switch`.
enum SearchState {
    /// Nothing has been searched yet, or the find string was emptied.
    Idle,
    Searching {
        progress: Progress,
        cancel: Cancellation,
    },
    Finished(Summary),
    /// The search could not start, or could not finish. The string is the
    /// one-line explanation the overlay shows.
    Failed(String),
}

/// The *Folder Find* pane.
pub struct FolderFindPane {
    workspace: WeakEntity<Workspace>,

    query: Entity<InputState>,

    /// *Text* or *Regular Expression*, from the mode menu.
    uses_regular_expression: bool,
    /// Upstream's `folderFindIgnoresCase`, which defaults to `false`.
    ignores_case: bool,
    includes_hidden_files: bool,
    includes_other_file_types: bool,

    /// The name of the saved scope in use, when one is.
    scope_name: Option<String>,
    scope: FileScope,
    /// Named scopes, keyed by name.
    saved_scopes: BTreeMap<String, FileScope>,

    /// The folder to search. `None` means "wherever the active document is".
    root: Option<PathBuf>,

    /// Which popovers are open. Popovers are controlled, so their open state
    /// has to live somewhere that outlives a render.
    menus: HashMap<&'static str, bool>,

    state: SearchState,
    /// The find string the current `state` describes. A newer one means the
    /// user has typed on, and the pane drops the search's answer.
    submitted: String,
    /// Bumped once per search so a slow search's answer cannot overwrite a
    /// newer one's.
    generation: u64,

    /// Files the user has folded shut, by display path.
    collapsed: HashSet<String>,
    /// The file row the Delete key would drop.
    selected: Option<String>,
    results_focus: FocusHandle,

    _subscriptions: Vec<Subscription>,
}

impl FolderFindPane {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        settings: &Settings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search in Folder"));
        // Return starts a search, the way upstream's `.onSubmit` does; any
        // other edit means the results on screen no longer answer the question,
        // so they are dropped.
        let subscription = cx.subscribe(
            &query,
            |this: &mut Self, _input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => this.start_search(cx),
                InputEvent::Change => {
                    let current = this.query.read(cx).value().to_string();
                    if current != this.submitted {
                        this.invalidate(cx);
                    }
                }
                _ => {}
            },
        );

        Self {
            workspace,
            query,
            uses_regular_expression: settings.folder_find_uses_regular_expression,
            ignores_case: settings.folder_find_ignores_case,
            includes_hidden_files: settings.folder_find_includes_hidden_files,
            includes_other_file_types: settings.folder_find_includes_other_file_types,
            scope_name: None,
            scope: settings.folder_find_scope.clone().unwrap_or_default(),
            saved_scopes: settings.folder_find_saved_scopes.clone(),
            root: None,
            menus: HashMap::new(),
            state: SearchState::Idle,
            submitted: String::new(),
            generation: 0,
            collapsed: HashSet::new(),
            selected: None,
            results_focus: cx.focus_handle(),
            _subscriptions: vec![subscription],
        }
    }

    // ---- state -----------------------------------------------------------

    /// The folder a search would walk: the one the user chose, or the one the
    /// active document lives in.
    fn effective_root(&self, cx: &App) -> Option<PathBuf> {
        if let Some(root) = &self.root {
            return Some(root.clone());
        }
        self.workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).active_directory())
    }

    /// Forget the results, because the question they answer has changed.
    fn invalidate(&mut self, cx: &mut Context<Self>) {
        if let SearchState::Searching { cancel, .. } = &self.state {
            cancel.cancel();
        }
        self.generation += 1;
        self.state = SearchState::Idle;
        self.collapsed.clear();
        self.selected = None;
        cx.notify();
    }

    fn is_menu_open(&self, id: &str) -> bool {
        self.menus.get(id).copied().unwrap_or(false)
    }

    fn set_menu_open(&mut self, id: &'static str, open: bool) {
        self.menus.insert(id, open);
    }

    /// Write the pane's switches and scopes back to the settings file.
    ///
    /// CotEditor keeps all of this in `UserDefaults`, so it survives a quit;
    /// the settings file is SubText's equivalent store.
    fn persist(&self, cx: &mut Context<Self>) {
        let (regex, ignores_case, hidden, other) = (
            self.uses_regular_expression,
            self.ignores_case,
            self.includes_hidden_files,
            self.includes_other_file_types,
        );
        let scope = self.scope.clone();
        let saved = self.saved_scopes.clone();
        self.workspace
            .update(cx, move |workspace, cx| {
                let settings = &mut workspace.settings;
                settings.folder_find_uses_regular_expression = regex;
                settings.folder_find_ignores_case = ignores_case;
                settings.folder_find_includes_hidden_files = hidden;
                settings.folder_find_includes_other_file_types = other;
                settings.folder_find_scope = (!scope.is_empty()).then_some(scope);
                settings.folder_find_saved_scopes = saved;
                let _ = settings.save();
                cx.notify();
            })
            .ok();
    }

    // ---- searching -------------------------------------------------------

    /// Run the query over the folder.
    ///
    /// Everything that can fail before the walk starts — an empty find string,
    /// a pattern that does not compile, an unusable scope, a folder that is not
    /// there — is reported here, so the overlay shows the reason instead of the
    /// walk failing halfway.
    fn start_search(&mut self, cx: &mut Context<Self>) {
        let find_string = self.query.read(cx).value().to_string();
        self.submitted = find_string.clone();
        self.generation += 1;
        let generation = self.generation;
        self.collapsed.clear();
        self.selected = None;

        if let SearchState::Searching { cancel, .. } = &self.state {
            cancel.cancel();
        }

        if find_string.is_empty() {
            self.state = SearchState::Idle;
            cx.notify();
            return;
        }

        let Some(root) = self.effective_root(cx) else {
            self.state = SearchState::Failed("The folder cannot be found.".to_string());
            cx.notify();
            return;
        };
        if !root.is_dir() {
            self.state = SearchState::Failed("The folder cannot be found.".to_string());
            cx.notify();
            return;
        }

        let mode = Mode {
            uses_regular_expression: self.uses_regular_expression,
            ignores_case: self.ignores_case,
        };
        let query = Query::new(find_string.clone(), mode);
        if let Err(error) = query.searcher() {
            self.state = SearchState::Failed(error.message());
            cx.notify();
            return;
        }
        if let Err(error) = self.scope.validate() {
            self.state = SearchState::Failed(error.message());
            cx.notify();
            return;
        }

        let options = search_options(
            self.includes_hidden_files,
            self.includes_other_file_types,
            &self.scope,
        );
        let extra_extensions = self.syntax_extensions(cx);

        let progress = Progress::new(find_string);
        let cancel = Cancellation::new();
        self.state = SearchState::Searching {
            progress: progress.clone(),
            cancel: cancel.clone(),
        };
        cx.notify();

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            // The walk blocks and is unbounded, so it goes to a background
            // thread; the counters come back through `progress`, which the
            // ticker below reads while it runs.
            let outcome = executor
                .spawn(async move {
                    let is_included = syntax_predicate(extra_extensions);
                    match Search::new(root, &query, options, Some(is_included)) {
                        Ok(mut search) => match search.run(Some(&progress), Some(&cancel)) {
                            Ok(summary) => Some(Ok(summary)),
                            Err(_cancelled) => None,
                        },
                        Err(error) => Some(Err(error.message())),
                    }
                })
                .await;

            this.update(cx, |pane, cx| {
                if pane.generation != generation {
                    return;
                }
                match outcome {
                    // Cancelled: the pane has already moved on.
                    None => {}
                    Some(Ok(summary)) => {
                        pane.state = SearchState::Finished(summary);
                        cx.notify();
                    }
                    Some(Err(message)) => {
                        pane.state = SearchState::Failed(message);
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();

        self.start_progress_ticker(cx);
    }

    /// Repaint the pane while a search is running, and stop when it is not.
    fn start_progress_ticker(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PROGRESS_INTERVAL).await;
                let running = this
                    .update(cx, |pane, cx| {
                        if matches!(pane.state, SearchState::Searching { .. }) {
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();
    }

    /// Every file extension CotEditor has a syntax for, lower-cased.
    ///
    /// This is the predicate the search uses to widen its idea of "text": a
    /// `.tex` or a `.plist` is searchable because a syntax claims it, not
    /// because the core's extension table happens to list it.
    fn syntax_extensions(&self, cx: &App) -> HashSet<String> {
        let mut extensions = HashSet::new();
        let Some(workspace) = self.workspace.upgrade() else {
            return extensions;
        };
        for syntax in workspace.read(cx).syntaxes().all() {
            for extension in &syntax.info.file_map.extensions {
                extensions.insert(extension.to_lowercase());
            }
        }
        extensions
    }

    // ---- actions ---------------------------------------------------------

    /// The folder picker, which is the platform's own panel.
    fn choose_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui_kit::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = handle.update(cx, move |_, _, cx| {
                this.update(cx, |pane, cx| {
                    pane.root = Some(path);
                    pane.start_search(cx);
                })
                .ok();
            });
        })
        .detach();
    }

    /// Open a result, and put the caret on the match that was clicked.
    fn open_result(
        &self,
        path: PathBuf,
        range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| match range {
            Some(range) => workspace.open_and_select(path, range, window, cx),
            None => workspace.open_path(path, window, cx),
        });
    }

    /// Fold or unfold a file's matches.
    fn toggle_file(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&key) {
            self.collapsed.insert(key);
        }
        cx.notify();
    }

    /// Drop a file's results, as upstream's Delete key does.
    ///
    /// The metrics bar keeps its own totals: upstream leaves them alone too,
    /// because they describe the search rather than the list.
    fn remove_selected(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.selected.clone() else {
            return;
        };
        let SearchState::Finished(summary) = &mut self.state else {
            return;
        };
        summary
            .files
            .retain(|file| file.path.display().to_string() != key);
        self.selected = None;
        cx.notify();
    }

    /// The *File Scope* sheet.
    fn edit_file_scope(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = cx.entity().downgrade();
        let scope = self.scope.clone();
        let name = self.scope_name.clone();
        let saved = self.saved_scopes.clone();
        let sheet =
            cx.new(|cx| FileScopeSheet::new(pane, scope, name, saved, false, window, cx));
        window.open_dialog(cx, move |dialog, window, _| {
            dialog
                .title("File Scope")
                .w(window.rem_size() * 36.)
                .child(sheet.clone())
                // The sheet draws its own buttons; Enter belongs to them.
                .on_ok(|_, _, _| false)
        });
    }

    /// The *Manage Saved Scopes* dialog.
    fn manage_saved_scopes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = cx.entity().downgrade();
        let saved = self.saved_scopes.clone();
        let sheet = cx.new(|cx| SavedScopesSheet::new(pane, saved, cx));
        window.open_dialog(cx, move |dialog, window, _| {
            dialog
                .title("Saved Scopes")
                .w(window.rem_size() * 30.)
                .child(sheet.clone())
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("OK"))),
                )
                .on_ok(|_, _, _| false)
        });
    }

    // ---- applying a scope ------------------------------------------------

    /// Take the scope the sheet produced.
    fn apply_scope(
        &mut self,
        scope: FileScope,
        name: Option<String>,
        original: Option<String>,
        cx: &mut Context<Self>,
    ) {
        // Renaming a saved scope: the old key goes, the new one takes its place.
        if let (Some(original), Some(name)) = (&original, &name)
            && original != name
        {
            self.saved_scopes.remove(original);
        }
        if let Some(name) = &name {
            self.saved_scopes.insert(name.clone(), scope.clone());
        }
        self.scope_name = name;
        self.scope = scope;
        self.persist(cx);
        cx.notify();
    }

    /// Drop a saved scope by name.
    fn delete_saved_scope(&mut self, name: &str, cx: &mut Context<Self>) {
        self.saved_scopes.remove(name);
        if self.scope_name.as_deref() == Some(name) {
            self.scope_name = None;
            self.scope = FileScope::default();
        }
        self.persist(cx);
        cx.notify();
    }

    /// Rename a saved scope, keeping it selected if it was.
    fn rename_saved_scope(&mut self, original: &str, name: &str, cx: &mut Context<Self>) {
        let Some(scope) = self.saved_scopes.remove(original) else {
            return;
        };
        self.saved_scopes.insert(name.to_string(), scope);
        if self.scope_name.as_deref() == Some(original) {
            self.scope_name = Some(name.to_string());
        }
        self.persist(cx);
        cx.notify();
    }

    /// Select a saved scope, or clear the scope when `name` is `None`.
    fn select_saved_scope(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        match name {
            Some(name) => {
                if let Some(scope) = self.saved_scopes.get(&name).cloned() {
                    self.scope_name = Some(name);
                    self.scope = scope;
                }
            }
            None => {
                self.scope_name = None;
                self.scope = FileScope::default();
            }
        }
        self.persist(cx);
        cx.notify();
    }

    // ---- rendering -------------------------------------------------------

    /// The mode menu, the case switch, the search field, the folder row and the
    /// scope menu.
    fn render_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = cx.entity().downgrade();
        let mode_label = if self.uses_regular_expression {
            "Regular Expression"
        } else {
            "Text"
        };

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(mode_menu(
                        self.is_menu_open("mode"),
                        mode_label,
                        self.uses_regular_expression,
                        &pane,
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(
                        // Upstream's *Case Sensitive* toggle: a glyph button
                        // that reads as pressed when case matters. The pane is
                        // case sensitive unless the switch is on.
                        Button::new("folder-find-case")
                            .ghost()
                            .small()
                            .icon(gpui_kit::assets::IconName::CaseSensitive)
                            .selected(!self.ignores_case)
                            .accessibility_label("Case Sensitive")
                            .tooltip("Case Sensitive")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.ignores_case = !this.ignores_case;
                                this.persist(cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(Input::new(&self.query).small().cleanable(true))
            .child(self.render_folder_row(cx))
            .child(self.render_scope_menu(cx))
    }

    /// The row that says which folder is about to be searched.
    fn render_folder_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let root = self.effective_root(cx);
        let label = root.as_ref().map_or_else(
            || "No folder — open a document".to_string(),
            |root| {
                root.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.display().to_string())
            },
        );
        let detail = root
            .as_ref()
            .map(|root| root.display().to_string())
            .unwrap_or_default();
        let muted = cx.theme().muted_foreground;
        let is_chosen = self.root.is_some();

        h_flex()
            .w_full()
            .gap_2()
            .child(
                Icon::new(gpui_kit::assets::IconName::Folder)
                    .xsmall()
                    .flex_shrink_0(),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .gap_0p5()
                    .child(div().text_sm().truncate().child(label))
                    .when(!detail.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .truncate()
                                .child(detail),
                        )
                    }),
            )
            .when(is_chosen, |this| {
                this.child(
                    Button::new("folder-find-use-document")
                        .ghost()
                        .xsmall()
                        .icon(gpui_kit::assets::IconName::File)
                        .accessibility_label("Use the document's folder")
                        .tooltip("Use the document's folder")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.root = None;
                            cx.notify();
                        })),
                )
            })
            .child(
                Button::new("folder-find-choose")
                    .ghost()
                    .xsmall()
                    .icon(gpui_kit::assets::IconName::FolderOpen)
                    .accessibility_label("Choose Folder")
                    .tooltip("Choose Folder")
                    .on_click(cx.listener(|this, _, window, cx| this.choose_folder(window, cx))),
            )
    }

    /// The *File Scope* menu: the two include switches, the editor, the saved
    /// scopes, and the manager.
    ///
    /// The content is built inside the popover's callback rather than before it,
    /// because the callback runs on every render and an element can only be
    /// consumed once.
    fn render_scope_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = cx.entity().downgrade();
        let saved = self.saved_scopes.clone();
        let current = self.scope_name.clone();
        let has_scope = !self.scope.is_empty();
        let includes_hidden = self.includes_hidden_files;
        let includes_other = self.includes_other_file_types;
        let accent = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let trigger_label = self
            .scope_name
            .clone()
            .unwrap_or_else(|| "File Scope".to_string());

        Popover::new("folder-find-scope")
            .open(self.is_menu_open("scope"))
            .on_open_change({
                let pane = pane.clone();
                move |open, _, cx| {
                    let open = *open;
                    pane.update(cx, |pane, cx| {
                        pane.set_menu_open("scope", open);
                        cx.notify();
                    })
                    .ok();
                }
            })
            .trigger(
                Button::new("folder-find-scope-trigger")
                    .small()
                    .icon(gpui_kit::assets::IconName::TextSearch)
                    .label(trigger_label)
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .foreground(if has_scope { accent } else { muted }),
                    ),
            )
            .content({
                let pane = pane.clone();
                move |_, _, cx| {
                    let mut content = v_flex().w(rems(15.)).gap_0p5();

                    for (id, label, checked, hidden) in [
                        ("scope-hidden", "Include Hidden Files", includes_hidden, true),
                        (
                            "scope-other",
                            "Include Other File Types",
                            includes_other,
                            false,
                        ),
                    ] {
                        content = content.child(
                            menu_row(id, label, checked, cx).on_click({
                                let pane = pane.clone();
                                move |_, _, cx| {
                                    pane.update(cx, |pane, cx| {
                                        if hidden {
                                            pane.includes_hidden_files =
                                                !pane.includes_hidden_files;
                                        } else {
                                            pane.includes_other_file_types =
                                                !pane.includes_other_file_types;
                                        }
                                        pane.set_menu_open("scope", false);
                                        pane.persist(cx);
                                        cx.notify();
                                    })
                                    .ok();
                                }
                            }),
                        );
                    }

                    content = content.child(
                        menu_row("scope-edit", "Edit File Scope…", false, cx).on_click({
                            let pane = pane.clone();
                            move |_, window, cx| {
                                pane.update(cx, |pane, cx| {
                                    pane.set_menu_open("scope", false);
                                    pane.edit_file_scope(window, cx);
                                })
                                .ok();
                            }
                        }),
                    );

                    // A disabled menu item is not a component state GPUI Kit
                    // offers, so the row that cannot be used is simply not
                    // clickable.
                    let clear = menu_row("scope-clear", "Clear File Scope", false, cx);
                    content = content.child(if has_scope {
                        clear
                            .on_click({
                                let pane = pane.clone();
                                move |_, _, cx| {
                                    pane.update(cx, |pane, cx| {
                                        pane.set_menu_open("scope", false);
                                        pane.select_saved_scope(None, cx);
                                    })
                                    .ok();
                                }
                            })
                            .into_any_element()
                    } else {
                        clear.text_color(muted).into_any_element()
                    });

                    if !saved.is_empty() {
                        content = content
                            .child(div().py_1().child(Separator::horizontal().color(border)))
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .text_xs()
                                    .text_color(muted)
                                    .child("Saved Scopes"),
                            );
                        for name in saved.keys() {
                            let is_current = current.as_deref() == Some(name.as_str());
                            content = content.child(
                                menu_row(
                                    ElementId::Name(SharedString::from(format!(
                                        "scope-saved:{name}"
                                    ))),
                                    name.clone(),
                                    is_current,
                                    cx,
                                )
                                .on_click({
                                    let pane = pane.clone();
                                    let name = name.clone();
                                    move |_, _, cx| {
                                        pane.update(cx, |pane, cx| {
                                            pane.set_menu_open("scope", false);
                                            pane.select_saved_scope(Some(name.clone()), cx);
                                        })
                                        .ok();
                                    }
                                }),
                            );
                        }
                        content = content
                            .child(div().py_1().child(Separator::horizontal().color(border)))
                            .child(
                                menu_row("scope-manage", "Manage Saved Scopes…", false, cx)
                                    .on_click({
                                        let pane = pane.clone();
                                        move |_, window, cx| {
                                            pane.update(cx, |pane, cx| {
                                                pane.set_menu_open("scope", false);
                                                pane.manage_saved_scopes(window, cx);
                                            })
                                            .ok();
                                        }
                                    }),
                            );
                    }

                    content
                }
            })
            .into_any_element()
    }

    /// The *N matches in M files* bar, live while the search runs.
    fn render_metrics(&self, cx: &Context<Self>) -> gpui_kit::AnyElement {
        let (metrics, truncated) = match &self.state {
            SearchState::Searching { progress, .. } => {
                let metrics = progress.snapshot();
                let truncated = metrics.has_unsearched_files;
                (metrics, truncated)
            }
            SearchState::Finished(summary) => (summary.metrics.clone(), summary.is_truncated()),
            _ => return div().into_any_element(),
        };

        v_flex()
            .gap_1()
            .child(Separator::horizontal().color(cx.theme().border))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(metrics_message(&metrics, truncated)),
            )
            .child(Separator::horizontal().color(cx.theme().border))
            .into_any_element()
    }

    /// The list of files, or the overlay that explains why there is none.
    fn render_results(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        match &self.state {
            SearchState::Idle => v_flex()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Search a folder for text, like CotEditor's Folder Find."),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Type a find string and press Return."),
                )
                .into_any_element(),
            SearchState::Searching { .. } => v_flex()
                .gap_2()
                .items_center()
                .py_6()
                .child(
                    Spinner::new()
                        .icon(gpui_kit::assets::IconName::LoaderCircle)
                        .xsmall(),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Searching in folder…"),
                )
                .into_any_element(),
            SearchState::Failed(message) => overlay(
                gpui_kit::assets::IconName::TriangleAlert,
                "Search Failed",
                Some(message.clone()),
                cx,
            ),
            SearchState::Finished(summary) if summary.metrics.match_count == 0 => overlay(
                gpui_kit::assets::IconName::Search,
                "No Results",
                Some(format!(
                    "No matches for “{}” were found.",
                    summary.metrics.find_string
                )),
                cx,
            ),
            SearchState::Finished(summary) => self.render_file_list(summary, cx),
        }
    }

    fn render_file_list(&self, summary: &Summary, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let pane = cx.entity().downgrade();
        let mut list = v_flex().gap_1();

        for file in &summary.files {
            let key = file.path.display().to_string();
            let is_collapsed = self.collapsed.contains(&key);
            let is_selected = self.selected.as_deref() == Some(key.as_str());
            list = list.child(self.render_file_row(file, &key, is_collapsed, is_selected, &pane, cx));
        }

        if summary.metrics.has_unsearched_files {
            list = list.child(
                div()
                    .w_full()
                    .py_2()
                    .text_center()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Search Limit Reached"),
            );
        }

        list.into_any_element()
    }

    fn render_file_row(
        &self,
        file: &FileResult,
        key: &str,
        is_collapsed: bool,
        is_selected: bool,
        pane: &WeakEntity<Self>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let accent = cx.theme().accent;
        let muted = cx.theme().muted_foreground;
        let primary = cx.theme().primary;
        let directory = file.directory_path();
        let path = file.path.clone();
        let first_match = file.matches.first().map(|matched| matched.range.clone());
        let filename = file.filename();

        let header = h_flex()
            .w_full()
            .gap_1()
            .px_1()
            .py_0p5()
            .rounded_sm()
            .when(is_selected, |this| this.bg(accent))
            .hover(|this| this.bg(accent))
            // The disclosure control is a sibling of the label rather than a
            // child of it: GPUI bubbles a click up the tree, so a chevron
            // inside the clickable region would fold the file *and* open it.
            .child(
                Button::new(ElementId::Name(SharedString::from(format!(
                    "folder-find-fold:{key}"
                ))))
                .ghost()
                .xsmall()
                .icon(if is_collapsed {
                    gpui_kit::assets::IconName::ChevronRight
                } else {
                    gpui_kit::assets::IconName::ChevronDown
                })
                .accessibility_label(if is_collapsed {
                    "Expand this file's matches"
                } else {
                    "Collapse this file's matches"
                })
                .on_click({
                    let pane = pane.clone();
                    let key = key.to_string();
                    move |_, _, cx| {
                        pane.update(cx, |pane, cx| pane.toggle_file(key.clone(), cx))
                            .ok();
                    }
                }),
            )
            .child(
                h_flex()
                    .id(ElementId::Name(SharedString::from(format!(
                        "folder-find-file:{key}"
                    ))))
                    .flex_1()
                    .min_w(px(0.))
                    .gap_1()
                    .cursor_pointer()
                    .child(
                        Icon::new(gpui_kit::assets::IconName::FileText)
                            .xsmall()
                            .flex_shrink_0(),
                    )
                    .child(div().flex_1().min_w(px(0.)).text_sm().truncate().child(filename))
                    .when(!directory.is_empty(), |this| {
                        this.child(
                            div()
                                .max_w(rems(8.))
                                .text_xs()
                                .text_color(muted)
                                .truncate()
                                .child(directory),
                        )
                    })
                    .on_click({
                        let pane = pane.clone();
                        let key = key.to_string();
                        let path = path.clone();
                        let range = first_match.clone();
                        move |_, window, cx| {
                            // A click selects the row and opens the file, which
                            // is what upstream's `List` selection does; the
                            // Delete key then drops the selected row.
                            pane.update(cx, |pane, cx| {
                                pane.selected = Some(key.clone());
                                pane.results_focus.focus(window, cx);
                                cx.notify();
                                pane.open_result(path.clone(), range.clone(), window, cx);
                            })
                            .ok();
                        }
                    })
                    .context_menu({
                        let path = path.clone();
                        let pane = pane.clone();
                        move |menu, _, _| {
                            let finder_path = path.clone();
                            let open_path = path.clone();
                            let copy_path = path.clone();
                            menu.item(
                                PopupMenuItem::new("Show in Finder")
                                    .icon(gpui_kit::assets::IconName::FolderSearch)
                                    .on_click(move |_, _, cx| cx.reveal_path(&finder_path)),
                            )
                            .item(
                                PopupMenuItem::new("Open in New Tab")
                                    .icon(gpui_kit::assets::IconName::SquareArrowOutUpRight)
                                    .on_click({
                                        let pane = pane.clone();
                                        move |_, window, cx| {
                                            let path = open_path.clone();
                                            pane.update(cx, |pane, cx| {
                                                pane.open_result(path.clone(), None, window, cx)
                                            })
                                            .ok();
                                        }
                                    }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new("Copy Path")
                                    .icon(gpui_kit::assets::IconName::File)
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                                            copy_path.display().to_string(),
                                        ));
                                    }),
                            )
                        }
                    }),
            );

        let mut column = v_flex().gap_0p5().child(header);

        if !is_collapsed {
            for matched in &file.matches {
                column = column.child(render_match_row(matched, primary, muted));
            }
            if file.is_truncated {
                column = column.child(
                    div()
                        .pl(rems(2.))
                        .text_xs()
                        .text_color(muted)
                        .child(format!(
                            "Only the first {} matches were found.",
                            file.matches.len()
                        )),
                );
            }
        }

        column
    }
}

impl Render for FolderFindPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_2()
            .child(self.render_controls(cx))
            .child(self.render_metrics(cx))
            .child(
                // The results area takes the Delete key, which is how upstream
                // drops a row it no longer wants to look at.
                div()
                    .id("folder-find-results")
                    .flex_1()
                    .min_h_0()
                    .track_focus(&self.results_focus)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "delete" || event.keystroke.key == "backspace" {
                            this.remove_selected(cx);
                        }
                    }))
                    .child(self.render_results(cx)),
            )
    }
}

impl Focusable for FolderFindPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.results_focus.clone()
    }
}

// ---------------------------------------------------------------------------
// The file scope sheet
// ---------------------------------------------------------------------------

/// The rule editor, as a dialog.
///
/// `original` is the name a saved scope had before it was opened for editing;
/// `named_only` forces a name, which is what *Add* in the manager wants.
pub struct FileScopeSheet {
    pane: WeakEntity<FolderFindPane>,
    conjunction: Conjunction,
    /// The working copy of the rules, paired with the field that edits each
    /// one's value. The two move together, so a rule never loses its field.
    rules: Vec<(Rule, Entity<InputState>)>,
    name_input: Entity<InputState>,
    original: Option<String>,
    named_only: bool,
    saved_names: Vec<String>,
    error: Option<String>,
}

impl FileScopeSheet {
    fn new(
        pane: WeakEntity<FolderFindPane>,
        scope: FileScope,
        name: Option<String>,
        saved: BTreeMap<String, FileScope>,
        named_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let rules = scope
            .rules
            .into_iter()
            .map(|rule| {
                let value =
                    cx.new(|cx| InputState::new(window, cx).default_value(rule.value.clone()));
                (rule, value)
            })
            .collect::<Vec<_>>();
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Name")
                .default_value(name.clone().unwrap_or_default())
        });
        Self {
            pane,
            conjunction: scope.conjunction,
            rules,
            name_input,
            original: name,
            named_only,
            saved_names: saved.keys().cloned().collect(),
            error: None,
        }
    }

    /// Whether the name field is part of the form.
    fn wants_name(&self) -> bool {
        self.original.is_some() || self.named_only
    }

    /// Add an empty rule, the way `NSRuleEditor` grows a row.
    fn add_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = cx.new(|cx| InputState::new(window, cx));
        self.rules
            .push((Rule::new(Target::Filename, Comparison::Contains, ""), value));
        self.error = None;
        cx.notify();
    }

    fn remove_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.rules.len() {
            self.rules.remove(index);
        }
        self.error = None;
        cx.notify();
    }

    /// The scope as it stands, with each rule's value read out of its field.
    fn scope(&self, cx: &App) -> FileScope {
        FileScope::new(
            self.conjunction,
            self.rules
                .iter()
                .map(|(rule, value)| {
                    Rule::new(
                        rule.target,
                        rule.comparison,
                        value.read(cx).value().to_string(),
                    )
                })
                .collect(),
        )
    }

    /// The name the scope would be saved under, or `None` for the unnamed one.
    fn resolved_name(&self, cx: &App) -> Option<String> {
        self.wants_name()
            .then(|| self.name_input.read(cx).value().trim().to_string())
    }

    /// Why the sheet cannot be confirmed yet.
    fn problem(&self, cx: &App) -> Option<String> {
        let scope = self.scope(cx);
        if let Some(name) = self.resolved_name(cx) {
            let original = self.original.as_deref();
            if let Err(error) = validate_scope_name(&name, &self.saved_names, original) {
                return Some(error);
            }
        } else if self.named_only {
            return Some("The name cannot be empty.".to_string());
        }
        if let Err(error) = scope.validate() {
            return Some(error.message());
        }
        None
    }

    /// Confirm, if the scope is usable.
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(problem) = self.problem(cx) {
            self.error = Some(problem);
            cx.notify();
            return;
        }
        let scope = self.scope(cx);
        let name = self.resolved_name(cx);
        let original = self.original.clone();
        self.pane
            .update(cx, |pane, cx| pane.apply_scope(scope, name, original, cx))
            .ok();
        window.close_dialog(cx);
    }

    fn delete_named(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(name) = self.original.clone() {
            self.pane
                .update(cx, |pane, cx| pane.delete_saved_scope(&name, cx))
                .ok();
        }
        window.close_dialog(cx);
    }

    // ---- rendering -------------------------------------------------------

    fn render_conjunction(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sheet = cx.entity().downgrade();
        let current = self.conjunction;

        Popover::new("scope-conjunction")
            .trigger(
                Button::new("scope-conjunction-trigger")
                    .small()
                    .label(current.label()),
            )
            .content({
                let sheet = sheet.clone();
                move |_, _, cx| {
                    let mut content = v_flex().w(rems(6.)).gap_0p5();
                    for conjunction in [Conjunction::Any, Conjunction::All] {
                        content = content.child(
                            menu_row(
                                ElementId::Name(SharedString::from(format!(
                                    "scope-conjunction:{}",
                                    conjunction.label()
                                ))),
                                conjunction.label(),
                                conjunction == current,
                                cx,
                            )
                            .on_click({
                                let sheet = sheet.clone();
                                move |_, _, cx| {
                                    sheet
                                        .update(cx, |this, cx| {
                                            this.conjunction = conjunction;
                                            this.error = None;
                                            cx.notify();
                                        })
                                        .ok();
                                }
                            }),
                        );
                    }
                    content
                }
            })
    }

    fn render_rules(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sheet = cx.entity().downgrade();
        let mut column = v_flex().gap_1();
        if self.rules.is_empty() {
            column = column.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No conditions — every file is searched."),
            );
        }

        for (index, (rule, value)) in self.rules.iter().enumerate() {
            let target = rule.target;
            let comparison = rule.comparison;

            let target_menu = Popover::new(ElementId::Name(SharedString::from(format!(
                "scope-target:{index}"
            ))))
            .trigger(
                Button::new(ElementId::Name(SharedString::from(format!(
                    "scope-target-trigger:{index}"
                ))))
                .small()
                .label(target.label()),
            )
            .content({
                let sheet = sheet.clone();
                move |_, _, cx| {
                    let mut content = v_flex().w(rems(10.)).gap_0p5();
                    for option in Target::ALL {
                        content = content.child(
                            menu_row(
                                ElementId::Name(SharedString::from(format!(
                                    "scope-target:{index}:{}",
                                    option.label()
                                ))),
                                option.label(),
                                option == target,
                                cx,
                            )
                            .on_click({
                                let sheet = sheet.clone();
                                move |_, _, cx| {
                                    sheet
                                        .update(cx, |this, cx| {
                                            if let Some((rule, _)) = this.rules.get_mut(index) {
                                                rule.target = option;
                                            }
                                            this.error = None;
                                            cx.notify();
                                        })
                                        .ok();
                                }
                            }),
                        );
                    }
                    content
                }
            });

            let comparison_menu = Popover::new(ElementId::Name(SharedString::from(format!(
                "scope-comparison:{index}"
            ))))
            .trigger(
                Button::new(ElementId::Name(SharedString::from(format!(
                    "scope-comparison-trigger:{index}"
                ))))
                .small()
                .label(comparison.label()),
            )
            .content({
                let sheet = sheet.clone();
                move |_, _, cx| {
                    let mut content = v_flex().w(rems(14.)).gap_0p5();
                    for option in Comparison::ALL {
                        content = content.child(
                            menu_row(
                                ElementId::Name(SharedString::from(format!(
                                    "scope-comparison:{index}:{}",
                                    option.label()
                                ))),
                                option.label(),
                                option == comparison,
                                cx,
                            )
                            .on_click({
                                let sheet = sheet.clone();
                                move |_, _, cx| {
                                    sheet
                                        .update(cx, |this, cx| {
                                            if let Some((rule, _)) = this.rules.get_mut(index) {
                                                rule.comparison = option;
                                            }
                                            this.error = None;
                                            cx.notify();
                                        })
                                        .ok();
                                }
                            }),
                        );
                    }
                    content
                }
            });

            column = column.child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(target_menu)
                    .child(comparison_menu)
                    .child(div().flex_1().min_w(px(0.)).child(Input::new(value).small()))
                    .child(
                        Button::new(ElementId::Name(SharedString::from(format!(
                            "scope-remove:{index}"
                        ))))
                        .ghost()
                        .small()
                        .icon(gpui_kit::assets::IconName::Close)
                        .accessibility_label("Remove this condition")
                        .tooltip("Remove this condition")
                        .on_click(cx.listener(move |this, _, _, cx| this.remove_rule(index, cx))),
                    ),
            );
        }

        column
    }

    fn render_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let problem = self.problem(cx);
        let blocked = problem.is_some();
        let is_named = self.original.is_some();

        h_flex()
            .w_full()
            .gap_2()
            .when(is_named, |this| {
                this.child(
                    Button::new("scope-delete")
                        .small()
                        .danger()
                        .label("Delete")
                        .on_click(cx.listener(|this, _, window, cx| this.delete_named(window, cx))),
                )
            })
            .child(div().flex_1())
            .child(
                Button::new("scope-cancel")
                    .small()
                    .label("Cancel")
                    .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
            )
            .child(
                Button::new("scope-apply")
                    .small()
                    .primary()
                    .label("Apply")
                    .disabled(blocked)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            )
    }
}

impl Render for FileScopeSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = v_flex().gap_3().child(
            h_flex()
                .gap_1()
                .items_center()
                .child(div().text_sm().child("Match"))
                .child(self.render_conjunction(cx))
                .child(div().text_sm().child("of the following conditions:")),
        );

        if self.wants_name() {
            body = body.child(
                h_flex()
                    .gap_2()
                    .child(div().w(rems(SHEET_LABEL_WIDTH)).text_sm().child("Name"))
                    .child(div().flex_1().child(Input::new(&self.name_input).small())),
            );
        }

        body = body
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child("Conditions"))
                    .child(self.render_rules(cx))
                    .child(
                        Button::new("scope-add")
                            .small()
                            .ghost()
                            .icon(gpui_kit::assets::IconName::Plus)
                            .label("Add Condition")
                            .on_click(cx.listener(|this, _, window, cx| this.add_rule(window, cx))),
                    ),
            )
            .child(Separator::horizontal().color(cx.theme().border));

        if let Some(problem) = self.error.clone().or_else(|| self.problem(cx)) {
            body = body.child(div().text_xs().text_color(cx.theme().danger).child(problem));
        }

        body.child(self.render_buttons(cx))
    }
}

// ---------------------------------------------------------------------------
// The saved scopes manager
// ---------------------------------------------------------------------------

/// The list of named scopes, with the three things upstream puts behind a
/// context menu on that list.
pub struct SavedScopesSheet {
    pane: WeakEntity<FolderFindPane>,
    scopes: BTreeMap<String, FileScope>,
    renaming: Option<(String, Entity<InputState>)>,
}

impl SavedScopesSheet {
    fn new(pane: WeakEntity<FolderFindPane>, scopes: BTreeMap<String, FileScope>, _cx: &mut Context<Self>) -> Self {
        Self { pane, scopes, renaming: None }
    }

    fn names(&self) -> Vec<String> {
        self.scopes.keys().cloned().collect()
    }

    fn delete(&mut self, name: &str, cx: &mut Context<Self>) {
        self.scopes.remove(name);
        let pane = self.pane.clone();
        let name = name.to_string();
        pane.update(cx, |pane, cx| pane.delete_saved_scope(&name, cx))
            .ok();
        cx.notify();
    }

    fn start_rename(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name.to_string()));
        self.renaming = Some((name.to_string(), input));
        cx.notify();
    }

    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some((original, input)) = self.renaming.clone() else {
            return;
        };
        let name = input.read(cx).value().trim().to_string();
        let others = self
            .names()
            .into_iter()
            .filter(|existing| existing != &original)
            .collect::<Vec<_>>();
        if validate_scope_name(&name, &others, None).is_err() {
            return;
        }
        self.scopes.remove(&original);
        let pane = self.pane.clone();
        pane.update(cx, |pane, cx| pane.rename_saved_scope(&original, &name, cx))
            .ok();
        self.renaming = None;
        cx.notify();
    }

    /// Open the rule editor on a new, named scope.
    fn add_scope(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.pane.clone();
        let saved = self.scopes.clone();
        let sheet =
            cx.new(|cx| FileScopeSheet::new(pane, FileScope::default(), None, saved, true, window, cx));
        window.open_dialog(cx, move |dialog, window, _| {
            dialog
                .title("New Saved Scope")
                .w(window.rem_size() * 36.)
                .child(sheet.clone())
                .on_ok(|_, _, _| false)
        });
    }

    /// Open the rule editor on an existing named scope.
    fn edit_scope(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(scope) = self.scopes.get(name).cloned() else {
            return;
        };
        let pane = self.pane.clone();
        let saved = self.scopes.clone();
        let name = name.to_string();
        let sheet = cx.new(|cx| {
            FileScopeSheet::new(pane, scope, Some(name.clone()), saved, false, window, cx)
        });
        window.open_dialog(cx, move |dialog, window, _| {
            dialog
                .title("Edit Saved Scope")
                .w(window.rem_size() * 36.)
                .child(sheet.clone())
                .on_ok(|_, _, _| false)
        });
    }
}

impl Render for SavedScopesSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The nested scope editor writes through the pane, so the list is
        // re-read every render rather than cached across them.
        if let Some(pane) = self.pane.upgrade() {
            self.scopes = pane.read(cx).saved_scopes.clone();
        }

        let names = self.names();
        let mut list = v_flex().gap_0p5();
        if names.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No saved scopes."),
            );
        }

        for name in &names {
            let renaming = self
                .renaming
                .as_ref()
                .is_some_and(|(original, _)| original == name);

            let row = if renaming {
                let input = self.renaming.as_ref().map(|(_, input)| input.clone());
                let mut row = h_flex().w_full().gap_2().px_2().py_1();
                if let Some(input) = input {
                    row = row
                        .child(div().flex_1().min_w(px(0.)).child(Input::new(&input).small()))
                        .child(
                            Button::new("saved-scope-rename-confirm")
                                .small()
                                .primary()
                                .label("Rename")
                                .on_click(cx.listener(|this, _, _, cx| this.commit_rename(cx))),
                        );
                }
                row.into_any_element()
            } else {
                h_flex()
                    .id(ElementId::Name(SharedString::from(format!(
                        "saved-scope:{name}"
                    ))))
                    .w_full()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|this| this.bg(cx.theme().accent))
                    .child(
                        Icon::new(gpui_kit::assets::IconName::TextSearch)
                            .xsmall()
                            .flex_shrink_0(),
                    )
                    .child(div().flex_1().min_w(px(0.)).text_sm().truncate().child(name.clone()))
                    .child(
                        Button::new(ElementId::Name(SharedString::from(format!(
                            "saved-scope-rename:{name}"
                        ))))
                        .ghost()
                        .xsmall()
                        .icon(gpui_kit::assets::IconName::FileText)
                        .accessibility_label("Rename")
                        .tooltip("Rename")
                        .on_click(cx.listener({
                            let name = name.clone();
                            move |this, _, window, cx| this.start_rename(&name, window, cx)
                        })),
                    )
                    .child(
                        Button::new(ElementId::Name(SharedString::from(format!(
                            "saved-scope-delete:{name}"
                        ))))
                        .ghost()
                        .xsmall()
                        .icon(gpui_kit::assets::IconName::Close)
                        .accessibility_label("Delete")
                        .tooltip("Delete")
                        .on_click(cx.listener({
                            let name = name.clone();
                            move |this, _, _, cx| this.delete(&name, cx)
                        })),
                    )
                    .on_click(cx.listener({
                        let name = name.clone();
                        move |this, _, window, cx| this.edit_scope(&name, window, cx)
                    }))
                    .into_any_element()
            };
            list = list.child(row);
        }

        v_flex()
            .gap_2()
            .child(div().text_sm().child("Saved scopes:"))
            .child(list)
            .child(
                Button::new("saved-scope-add")
                    .small()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::Plus)
                    .label("Add")
                    .on_click(cx.listener(|this, _, window, cx| this.add_scope(window, cx))),
            )
    }
}

// ---------------------------------------------------------------------------
// Shared rendering
// ---------------------------------------------------------------------------

/// The *Text / Regular Expression* menu.
///
/// Upstream's `Picker` tints its label when the pattern mode is on, so the user
/// can see at a glance that what they type is a pattern.
fn mode_menu(
    open: bool,
    label: &'static str,
    is_pattern: bool,
    pane: &WeakEntity<FolderFindPane>,
    cx: &mut Context<FolderFindPane>,
) -> impl IntoElement {
    let accent = cx.theme().primary;
    let foreground = cx.theme().foreground;

    Popover::new("folder-find-mode")
        .open(open)
        .on_open_change({
            let pane = pane.clone();
            move |open, _, cx| {
                let open = *open;
                pane.update(cx, |pane, cx| {
                    pane.set_menu_open("mode", open);
                    cx.notify();
                })
                .ok();
            }
        })
        .trigger(
            Button::new("folder-find-mode-trigger")
                .small()
                .icon(gpui_kit::assets::IconName::Regex)
                .label(label)
                .custom(
                    ButtonCustomVariant::new(cx)
                        .foreground(if is_pattern { accent } else { foreground }),
                ),
        )
        .content({
            let pane = pane.clone();
            move |_, _, cx| {
                let mut content = v_flex().w(rems(11.)).gap_0p5();
                for (index, (option, pattern)) in
                    [("Text", false), ("Regular Expression", true)]
                        .into_iter()
                        .enumerate()
                {
                    content = content.child(
                        menu_row(
                            ElementId::Name(SharedString::from(format!(
                                "folder-find-mode:{index}"
                            ))),
                            option,
                            is_pattern == pattern,
                            cx,
                        )
                        .on_click({
                            let pane = pane.clone();
                            move |_, _, cx| {
                                pane.update(cx, |pane, cx| {
                                    pane.uses_regular_expression = pattern;
                                    pane.set_menu_open("mode", false);
                                    pane.persist(cx);
                                    cx.notify();
                                })
                                .ok();
                            }
                        }),
                    );
                }
                content
            }
        })
        .into_any_element()
}

/// One row inside a popover menu: a tick slot, a label and a hover fill.
fn menu_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    cx: &App,
) -> gpui_kit::Stateful<Div> {
    let accent = cx.theme().accent;
    let foreground = cx.theme().foreground;
    let label: SharedString = label.into();
    h_flex()
        .id(id)
        .w_full()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .text_sm()
        .text_color(foreground)
        .hover(move |this| this.bg(accent))
        .child(
            div().w(px(12.)).child(if checked {
                Icon::new(gpui_kit::assets::IconName::Check)
                    .xsmall()
                    .into_any_element()
            } else {
                div().into_any_element()
            }),
        )
        .child(div().flex_1().min_w(px(0.)).truncate().child(label))
}

/// A file row's matching line, with the hit emphasised and the head trimmed.
///
/// The head and the hit keep their width and the tail is what gets clipped, so
/// the reason the row is in the list is always the part that is visible.
fn render_match_row(
    matched: &Match,
    primary: gpui_kit::Hsla,
    muted: gpui_kit::Hsla,
) -> impl IntoElement {
    let (head, hit, tail) = split_display_line(matched);
    h_flex()
        .w_full()
        .pl(rems(2.))
        .gap_0()
        .text_xs()
        .text_color(muted)
        .overflow_hidden()
        .child(div().flex_shrink_0().child(head))
        .child(div().flex_shrink_0().text_color(primary).child(hit))
        .child(div().flex_1().min_w(px(0.)).truncate().child(tail))
}

/// The overlay that stands in for the list when there is nothing to show.
fn overlay(
    icon: gpui_kit::assets::IconName,
    title: &'static str,
    description: Option<String>,
    cx: &Context<FolderFindPane>,
) -> gpui_kit::AnyElement {
    let mut header = EmptyHeader::new()
        .media(EmptyMedia::new().child(Icon::new(icon).small()))
        .title(EmptyTitle::new().child(title));
    if let Some(description) = description {
        header = header.description(EmptyDescription::new().child(description));
    }
    Empty::new()
        .header(header)
        .py_6()
        .text_color(cx.theme().muted_foreground)
        .into_any_element()
}

/// `N matches in M files`, with a `+` when the count is a lower bound.
fn metrics_message(metrics: &Metrics, is_truncated: bool) -> String {
    let plus = if is_truncated { "+" } else { "" };
    format!(
        "{}{plus} matches in {} files",
        metrics.match_count, metrics.matched_file_count
    )
}

/// Split a match's line into the part before the hit, the hit, and the part
/// after — with the indentation removed and the head trimmed.
///
/// This is upstream's `ItemView.highlightedLine`: the leading whitespace goes,
/// but only when the hit sits past it, and the head keeps
/// [`TRUNCATION_HEAD_OFFSET`] characters in front of the hit behind an ellipsis.
fn split_display_line(matched: &Match) -> (String, String, String) {
    let line = matched.line.as_str();
    let mut start = matched.range_in_line.start.min(line.len());
    let mut end = matched.range_in_line.end.min(line.len());
    // The offsets are byte offsets, so a range that landed inside a character
    // is widened to the character it is in rather than panicking on the slice.
    while start > 0 && !line.is_char_boundary(start) {
        start -= 1;
    }
    while end < line.len() && !line.is_char_boundary(end) {
        end += 1;
    }
    if end < start {
        end = start;
    }

    let indentation = line.len() - line.trim_start().len();
    let (line, start, end) = if indentation > 0 && start >= indentation {
        (&line[indentation..], start - indentation, end - indentation)
    } else {
        (line, start, end)
    };

    let before = &line[..start];
    let (ellipsis, before) = match before
        .char_indices()
        .count()
        .checked_sub(TRUNCATION_HEAD_OFFSET)
    {
        Some(skip) if skip > 0 => {
            let cut = before
                .char_indices()
                .nth(skip)
                .map_or(before.len(), |(index, _)| index);
            ("…", &before[cut..])
        }
        _ => ("", before),
    };

    (
        format!("{ellipsis}{before}"),
        line[start..end].to_string(),
        line[end..].to_string(),
    )
}

/// The options a search runs with, given the pane's two include switches and
/// its scope.
///
/// Everything else is upstream's default, which lives on
/// [`Options::default`] so the two cannot drift.
fn search_options(includes_hidden: bool, includes_other: bool, scope: &FileScope) -> Options {
    Options {
        includes_other_file_types: includes_other,
        includes_hidden_files: includes_hidden,
        file_scope: (!scope.is_empty()).then(|| scope.clone()),
        ..Options::default()
    }
}

/// The predicate the search uses to widen its idea of "text".
///
/// It answers yes for a file whose extension a CotEditor syntax claims, and it
/// is only consulted after the core's own table has said no, so it can add
/// files to a search but never take them away.
fn syntax_predicate(extensions: HashSet<String>) -> InclusionPredicate {
    Box::new(move |path: &Path| {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension.to_lowercase()))
    })
}

/// Why a scope cannot be saved under `name`.
///
/// Upstream's `validateFileScopeName`: a name has to exist, has to be one line,
/// and has to be free — except when it is the name the scope already had.
fn validate_scope_name(name: &str, taken: &[String], original: Option<&str>) -> Result<(), String> {
    if name.is_empty() {
        return Err("The name cannot be empty.".to_string());
    }
    if name.contains('\n') || name.contains('\r') {
        return Err("The name cannot contain a line break.".to_string());
    }
    if Some(name) != original && taken.iter().any(|existing| existing == name) {
        return Err(format!("“{name}” is already in use."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        metrics_message, search_options, split_display_line, syntax_predicate, validate_scope_name,
    };
    use std::collections::HashSet;
    use std::ops::Range;
    use std::path::PathBuf;
    use subtext_core::folderfind::{
        Comparison, FileScope, Match, Metrics, Mode, Query, Rule, Search, Target,
    };

    fn matched(line: &str, range_in_line: Range<usize>) -> Match {
        Match {
            range: 0..range_in_line.len(),
            line: line.to_string(),
            range_in_line,
        }
    }

    #[test]
    fn a_short_line_is_shown_whole() {
        let (head, hit, tail) = split_display_line(&matched("foo bar baz", 4..7));
        assert_eq!((head.as_str(), hit.as_str(), tail.as_str()), ("foo ", "bar", " baz"));
    }

    #[test]
    fn the_indentation_goes_when_the_hit_sits_past_it() {
        let (head, hit, tail) = split_display_line(&matched("    let x = 1;", 8..9));
        assert_eq!((head.as_str(), hit.as_str(), tail.as_str()), ("let ", "x", " = 1;"));
    }

    #[test]
    fn the_indentation_stays_when_the_hit_is_inside_it() {
        // A hit in the leading whitespace has to keep it, or the hit would move
        // to where the ellipsis is.
        let (head, hit, _) = split_display_line(&matched("    x", 0..1));
        assert_eq!((head.as_str(), hit.as_str()), ("", " "));
    }

    #[test]
    fn a_far_hit_keeps_only_its_lead_in() {
        let line = format!("{}hit", "x".repeat(100));
        let (head, hit, tail) = split_display_line(&matched(&line, 100..103));
        assert_eq!(hit, "hit");
        assert_eq!(tail, "");
        assert_eq!(head, format!("…{}", "x".repeat(32)));
    }

    #[test]
    fn a_hit_exactly_at_the_offset_keeps_its_whole_head() {
        let line = format!("{}hit", "x".repeat(32));
        let (head, _, _) = split_display_line(&matched(&line, 32..35));
        assert_eq!(head, "x".repeat(32));
    }

    #[test]
    fn a_range_inside_a_character_is_widened_rather_than_panicking() {
        // "e" + combining acute: three bytes for one grapheme.
        let line = "e\u{301}x";
        let (_, hit, tail) = split_display_line(&matched(line, 1..2));
        assert_eq!(hit, "\u{301}");
        assert_eq!(tail, "x");
    }

    #[test]
    fn the_metrics_message_marks_a_lower_bound() {
        let metrics = Metrics {
            find_string: "x".to_string(),
            match_count: 3,
            matched_file_count: 2,
            has_unsearched_files: false,
        };
        assert_eq!(metrics_message(&metrics, false), "3 matches in 2 files");
        assert_eq!(metrics_message(&metrics, true), "3+ matches in 2 files");
    }

    #[test]
    fn a_scope_name_has_to_be_usable() {
        let taken = vec!["Swift".to_string()];
        assert!(validate_scope_name("Notes", &taken, None).is_ok());
        assert!(validate_scope_name("", &taken, None).is_err());
        assert!(validate_scope_name("two\nlines", &taken, None).is_err());
        assert!(validate_scope_name("Swift", &taken, None).is_err());
        // Renaming a scope to the name it already has is not a clash.
        assert!(validate_scope_name("Swift", &taken, Some("Swift")).is_ok());
    }

    /// The options and the predicate the pane builds really do drive a search.
    ///
    /// This is the seam the unit tests either side of it cannot reach: the core
    /// tests use their own options and predicate, and the pane's construction
    /// is not reachable without a window.
    #[test]
    fn the_panes_options_and_predicate_drive_a_real_search() {
        let root = TempDir::new();
        root.write("notes.txt", "alpha needle\nbeta\n");
        // A NUL byte, so it is binary whatever the extension table says.
        root.write_bytes("skip.bin", b"needle\x00\x01");
        root.write("odd.custom", "a needle here\n");

        let query = Query::new("needle", Mode::default());
        let options = search_options(false, false, &FileScope::default());

        // `.custom` is not in the core's text table, so only the syntax
        // predicate can make it searchable — which is the point of it.
        let without = Search::new(root.path(), &query, options.clone(), None)
            .expect("the query compiles")
            .run(None, None)
            .expect("the walk completes");
        assert_eq!(without.metrics.matched_file_count, 1);
        assert_eq!(without.metrics.match_count, 1);

        let mut extensions = HashSet::new();
        extensions.insert("custom".to_string());
        let widened = Search::new(root.path(), &query, options, Some(syntax_predicate(extensions)))
            .expect("the query compiles")
            .run(None, None)
            .expect("the walk completes");
        assert_eq!(widened.metrics.matched_file_count, 2);
        assert!(
            widened
                .files
                .iter()
                .any(|file| file.path.ends_with("odd.custom"))
        );
    }

    /// The pane's scope reaches the walk: a scope that rejects everything
    /// leaves nothing behind.
    #[test]
    fn a_scope_narrows_the_walk() {
        let root = TempDir::new();
        root.write("keep.md", "needle\n");
        root.write("drop.txt", "needle\n");

        let query = Query::new("needle", Mode::default());
        let scope = FileScope::new(
            subtext_core::folderfind::Conjunction::Any,
            vec![Rule::new(Target::FileExtension, Comparison::IsEqualTo, "md")],
        );
        let summary = Search::new(
            root.path(),
            &query,
            search_options(false, false, &scope),
            None,
        )
        .expect("the query compiles")
        .run(None, None)
        .expect("the walk completes");
        assert_eq!(summary.metrics.matched_file_count, 1);
        assert!(summary.files[0].path.ends_with("keep.md"));
    }

    /// A directory that deletes itself, so a failing assertion cannot leave
    /// litter behind.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos());
            let path = std::env::temp_dir().join(format!(
                "subtext-folder-find-{}-{nanos}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("the temporary directory is creatable");
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }

        fn write(&self, name: &str, contents: &str) {
            self.write_bytes(name, contents.as_bytes());
        }

        fn write_bytes(&self, name: &str, contents: &[u8]) {
            std::fs::write(self.0.join(name), contents).expect("the fixture is writable");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
