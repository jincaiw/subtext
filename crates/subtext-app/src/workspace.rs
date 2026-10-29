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

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash as _, Hasher as _};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::AnyWindowHandle;
use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::base::input::InputEvent;
use gpui_kit::base::{AccordionPanel, AccordionTrigger, MotionReveal, spring};
use gpui_kit::base::{Placement, Positioner};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, ThemeStyled as _,
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogClose, DialogFooter},
    h_flex, h_resizable,
    input::{Editor, Input, InputState},
    list::ListItem,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    popover::Popover,
    resizable_panel,
    tab::{Tab, TabBar},
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tree::{TreeEvent, TreeItem, TreeState, tree},
    v_flex, v_resizable,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, App, AppContext as _, Context, ElementId, Entity, ExternalPaths,
    Focusable as _, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions, div, hsla, percentage, px, rems,
};

use subtext_core::completion;
use subtext_core::scripts::{InputType, Invocation, Item as ScriptItem, OutputType};
use subtext_core::{
    Document, DroppedFile, FileEncoding, LineEnding, MatchLine, Normalization, TextEncoding,
    charinfo, colorcode, fileattrs, finder, findresult, imagesize, line_ending, multireplace,
    textops,
};
use subtext_syntax::{Syntax, SyntaxManager};
use subtext_theme::{Theme, ThemeManager};

use crate::color_sheet::ColorSheet;
use crate::command_bar::{self, CommandBar};
use crate::console;
use crate::count_panel::{CountOptionsView, CountSheet};
use crate::dialogs::AboutView;
use crate::find_panel::{FindPanel, apply_row_selection};
use crate::folder_find::FolderFindPane;
use crate::highlighter;
use crate::menus;
use crate::multiple_replace_sheet::MultipleReplaceSheet;
use crate::outline::{FilteredNode, OutlineEntry, OutlineKind, OutlineRow};
use crate::resources;
use crate::scripts;
use crate::settings::{
    DEFAULT_FIND_RESULT_FONT_SIZE, DEFAULT_OUTLINE_FONT_SIZE, INSPECTOR_MAX_WIDTH,
    INSPECTOR_MIN_WIDTH, InspectorPane, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH, Settings,
    SettingsPane, SidebarPane,
};
use crate::settings_window::SettingsWindow;
use crate::sort_sheet::SortSheet;
use crate::surround_sheet::SurroundSheet;

/// How long a status message stays on screen.
const STATUS_LIFETIME: std::time::Duration = std::time::Duration::from_secs(4);

/// How many characters of a path the inspector's Full Path row shows.
///
/// CotEditor elides by measured width (`truncationMode(.middle)`); GPUI has no
/// way to ask a label how wide it ended up, and the inspector is resizable, so
/// this is the count that fills the default width in `text_xs`. A wider panel
/// wastes a little room, a narrower one truncates the rest at the end — and the
/// whole path is the button's tooltip either way.
const INSPECTOR_PATH_ELISION: usize = 20;

/// How many editors one document may be split into.
///
/// CotEditor's `DocumentViewController.maximumNumberOfSplitEditors`. Past it
/// upstream beeps; SubText has no beep and says so in the status bar instead.
const MAXIMUM_SPLIT_EDITORS: usize = 4;

/// How often the script folder is looked at again.
///
/// The Script menu is built from a folder the user can add to at any time, and
/// upstream is *told* about a change by `NSFilePresenter`. GPUI has no file
/// watcher, so the folder is re-read on a timer instead. It is a handful of
/// entries, and the comparison in [`Workspace::refresh_menus`] throws the answer
/// away when nothing moved, so the cost is one `read_dir` every couple of
/// seconds.
const SCRIPT_POLL: std::time::Duration = std::time::Duration::from_secs(2);

/// How far one outline level is indented, in rem.
///
/// Three quarters of the base font. It is also the width of the disclosure
/// lane, so an indent step and a disclosure triangle occupy the same column and
/// a label's leading edge lands on a consistent spine at every depth.
const OUTLINE_INDENT: f32 = 0.75;

/// The Document inspector's four sections, in CotEditor's order.
///
/// Upstream's `DocumentInspectorView` is a `VStack` of four `DisclosureGroup`s
/// with these labels, and the panel draws them in this order. Naming them once
/// keeps the labels, the order and the per-section open state from drifting
/// apart — the state is an array indexed by position in this list.
const INSPECTOR_SECTIONS: [&str; 4] = ["File", "Text Settings", "Count", "Character"];
const INSPECTOR_SECTION_COUNT: usize = INSPECTOR_SECTIONS.len();

/// The three counts and the three positions, in CotEditor's order.
///
/// Its `CountType.countCases` followed by `CountType.positionCases`. Both the
/// status bar and the inspector's Count section draw these six in this order —
/// the labels are here so the two cannot disagree about what "Lines" is called
/// or where it sits. The three counts carry a selected half; the three
/// positions do not.
const COUNT_LABELS: [&str; 3] = ["Lines", "Characters", "Words"];
const POSITION_LABELS: [&str; 3] = ["Location", "Line", "Column"];

/// The element id of the middle row's split group.
///
/// The id carries which of the three panes are showing, because
/// `ResizableState` keys its panel sizes by group id *and panel position*: one
/// id shared by all four configurations would hand the sidebar's width to
/// whichever pane moved into slot 0 when the sidebar is hidden. One id per
/// configuration gives each its own sizes.
fn middle_group_id(sidebar: bool, inspector: bool) -> ElementId {
    ElementId::Name(SharedString::from(format!(
        "workspace-columns-{}{}",
        u8::from(sidebar),
        u8::from(inspector)
    )))
}

/// The text colour inside an outline match highlight.
///
/// The fill is a fixed bright yellow in either appearance, so the text on it
/// has to be fixed too. This is the second and last literal colour in the
/// workspace, and it is here for the same reason [`crate::outline`]'s symbol
/// palette is: the value is a property of the highlight, not of the theme, and
/// a theme foreground would be near-white in a dark theme and disappear into
/// the yellow. `hsla(0., 0., 0.1, 0.9)` is 90%-opaque near-black.
const MATCH_FOREGROUND: gpui_kit::Hsla = hsla(0., 0., 0.1, 0.9);

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

    /// The element ID for the group that holds this document's split panes.
    ///
    /// Keyed by document so each one keeps its own divider positions, the way
    /// each CotEditor window keeps its own `NSSplitView` autosave.
    fn split_group(self) -> ElementId {
        ElementId::NamedInteger("editor-split".into(), self.0)
    }
}

/// One editor pane of an open document.
///
/// The subscription is owned by the pane rather than by the document so that
/// closing one pane of a split drops exactly its subscription and leaves the
/// others alone.
struct Pane {
    editor: Entity<gpui_kit::base::input::EditorState>,
    /// The decoration collection the find indicator paints into.
    ///
    /// A collection is a handle into the editor it was made from, so each pane
    /// owns one for its lifetime: creating one per flash would leave a new
    /// collection behind on every find.
    indicator: gpui_kit::base::input::TextDecorationCollection,
    /// The collection the *unemphasized* marks use — every match while an
    /// incremental search is running, and the warnings inspector's offenders.
    ///
    /// Separate from `indicator` because the two have different lifetimes: the
    /// flash is a moment, the marks last as long as the thing that asked for
    /// them.
    marks: gpui_kit::base::input::TextDecorationCollection,
    _subscription: Subscription,
    /// The editor's repaint observer, which is how caret and selection moves
    /// reach the menu bar.
    _observed: Subscription,
}

/// One open document and its editors.
pub struct OpenDocument {
    /// Stable identity, independent of the tab's position.
    pub id: DocumentId,
    pub document: Document,
    /// One editor per split pane, in visual order. Never empty.
    ///
    /// CotEditor shares a single `NSTextStorage` between its split text views,
    /// so an edit in one is instantly visible in the rest. Each SubText pane
    /// owns its own buffer, so the workspace copies the text across — see
    /// [`Workspace::sync_panes`].
    panes: Vec<Pane>,
    /// The pane the commands act on, which is the one the user last typed in
    /// or clicked. CotEditor tracks the same thing in `focusedTextView`.
    active_pane: usize,
    pub syntax: SharedString,
    /// Set when the file changed underneath us since it was last read.
    pub modified_on_disk: bool,
    /// The status bar's three counts, kept from the last time the text changed.
    ///
    /// Counting a megabyte of text takes 24 ms in release — and the status bar
    /// is rebuilt on **every repaint**, which includes every caret move and the
    /// caret's blink. Recomputed when the text changes, not when the frame does.
    counts: Option<charinfo::CharacterCount>,
}

impl OpenDocument {
    /// The pane that has the focus.
    pub fn editor(&self) -> &Entity<gpui_kit::base::input::EditorState> {
        &self.panes[self.active_pane].editor
    }

    /// Every pane, in visual order.
    pub fn editors(&self) -> impl Iterator<Item = &Entity<gpui_kit::base::input::EditorState>> {
        self.panes.iter().map(|pane| &pane.editor)
    }

    /// How many editors the document is split into.
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// The index of the pane that has the focus.
    pub fn active_pane(&self) -> usize {
        self.active_pane
    }
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
    /// Ranges the last search found, kept so the panel and the status bar can
    /// report them without re-running the search.
    pub matches: Vec<Range<usize>>,
    /// Each match's capture groups, in the same order as [`Self::matches`] —
    /// group 0 first, `None` where a group did not participate. The result
    /// table paints these with the decomposed highlighter colours.
    pub capture_matches: Vec<Vec<Option<Range<usize>>>>,
    pub current: Option<usize>,
    pub error: Option<String>,
    /// The rows of the *Find All* result table.
    ///
    /// Empty until *Find All* runs. Upstream builds them in the same pass that
    /// highlights the matches and only when the result view is going to be
    /// shown, because a row costs a copy of its line — so this is a separate
    /// list rather than something derived from `matches` on every frame.
    /// Sorting the table re-orders this list in place, the way upstream sorts
    /// its own `matches` array, so this stays the one display order.
    pub results: Vec<MatchLine>,
    /// Whether the result section is showing.
    ///
    /// Upstream's middle split item is collapsed until a *Find All* finds
    /// something, and the panel's own chevron collapses it again.
    pub results_open: bool,
    /// The last replace action's outcome, as the count and the replacement
    /// text it ran with — the `"\(count) replaced"` message the panel draws at
    /// the replacement field's trailing edge, shown only while the field still
    /// holds that text, the way upstream clears it in
    /// `onChange(of: replacementString)`.
    pub replace_result: Option<(usize, String)>,
}

/// The **Find & Replace** panel's window size, in points.
///
/// A utility window is a short column of rows, so it opens at the size those
/// rows need. The minimum width is where the button row still fits on one line;
/// below it the row wraps and the trailing pair of stepper buttons is what
/// gives way first.
const FIND_PANEL_WIDTH: f32 = 520.;
const FIND_PANEL_MIN_WIDTH: f32 = 420.;
const FIND_PANEL_HEIGHT: f32 = 190.;

/// How much taller the panel becomes while the *Find All* result table is open.
///
/// Upstream's `FindPanelContentViewController.defaultResultViewHeight` — the
/// result split item opens at 200 points, which is what makes the panel grow.
const FIND_PANEL_RESULT_HEIGHT: f32 = 200.;

/// How long the buffer must be still before the automatic completion list
/// opens.
///
/// Upstream's `automaticCompletionDelay` is 0.25 s, and every keystroke
/// restarts it — the list appears when typing pauses, not on every character.
const AUTOMATIC_COMPLETION_DELAY: Duration = Duration::from_millis(250);

/// The shortest word the automatic list opens for.
///
/// Upstream's `minimumAutomaticCompletionLength`. Three characters is where a
/// list stops being noise: at two, every prefix in the document matches.
const MINIMUM_AUTOMATIC_COMPLETION_LENGTH: usize = 3;

/// How the find indicator fades out: one opacity factor per step.
///
/// `NSTextView.showFindIndicator(for:)` draws its yellow mark and then fades it
/// over roughly half a second. A text decoration cannot be animated per frame —
/// the editor repaints when its state changes, not on a clock — so the fade is
/// **stepped**: the mark is re-set at each factor and cleared at zero. Four
/// steps at [`FIND_INDICATOR_STEP`] each is 700 ms, the same lifetime the mark
/// had when it simply vanished.
const FIND_INDICATOR_FADE: [f32; 4] = [0.75, 0.5, 0.25, 0.0];

/// The pause between two fade steps.
const FIND_INDICATOR_STEP: Duration = Duration::from_millis(175);

/// How long the find field must be still before the incremental pass runs.
///
/// Upstream's `incrementalSearch` sleeps 0.2 s (tolerance 0.05) before each
/// pass, so a fast typist runs one search rather than one per character — and
/// sees one indicator, not one per keystroke.
const INCREMENTAL_SEARCH_DELAY: Duration = Duration::from_millis(200);

/// How much of the theme's yellow the indicator paints.
///
/// The decoration sits *behind* the glyphs, so the text keeps its own colour
/// and stays readable; 0.55 is enough to read as a highlight without washing
/// the line out.
const FIND_INDICATOR_OPACITY: f32 = 0.55;

/// A transient message shown in the status bar.
struct Status {
    message: String,
    at: std::time::Instant,
}

/// A column of the inspector's warnings tables.
///
/// Upstream names them by `KeyPathComparator`: `\.lowerBound` (the line),
/// `\.value.character`, `\.value.converted`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WarningColumn {
    Line,
    /// *Character* in the incompatible table, *Line Ending* in the other one.
    First,
    /// *Converted* in the incompatible table; the other table has no third
    /// column, so it is never sorted by this one.
    Second,
}

/// Which of the two warnings tables a click belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WarningTable {
    Incompatible,
    LineEndings,
}

/// One row of an inspector warnings table.
#[derive(Clone, Debug, PartialEq, Eq)]
struct WarningRow {
    /// 1-based line number, as upstream's `lineNumber(at:)` reports it.
    line: usize,
    /// The *Character* or *Line Ending* cell.
    first: String,
    /// The *Converted* cell; empty in the line-ending table.
    second: String,
    /// What the row selects in the editor.
    range: Range<usize>,
}

/// One step of dragging the counting capsule.
///
/// The capsule is pinned to the top-**right**, so dragging it right *reduces*
/// the right inset while dragging it down *grows* the top offset. Deltas are
/// what the pointer travelled between two positions, which is why the capsule
/// follows the pointer no matter where in the window the editor sits.
fn drag_capsule_offset(
    (right, down): (f32, f32),
    (last_x, last_y): (f32, f32),
    (x, y): (f32, f32),
) -> (f32, f32) {
    (right - (x - last_x), down + (y - last_y))
}

/// Sort `rows` the way a click on `column` does upstream.
///
/// A stable sort by the chosen key, then reversed for descending — which is
/// what comparing with a descending `KeyPathComparator` amounts to, ties
/// included. The line is compared **as a number**: `2` before `10`, which a
/// string comparison would get backwards.
fn sort_warning_rows(rows: &mut [WarningRow], column: WarningColumn, descending: bool) {
    match column {
        WarningColumn::Line => rows.sort_by_key(|row| row.line),
        WarningColumn::First => rows.sort_by(|left, right| left.first.cmp(&right.first)),
        WarningColumn::Second => rows.sort_by(|left, right| left.second.cmp(&right.second)),
    }
    if descending {
        rows.reverse();
    }
}

/// What a click on a header does: same column toggles the direction, a
/// different one starts ascending — SwiftUI's `sortOrder` behaviour.
fn next_sort(
    current: Option<(WarningColumn, bool)>,
    clicked: WarningColumn,
) -> Option<(WarningColumn, bool)> {
    match current {
        Some((column, descending)) if column == clicked => Some((clicked, !descending)),
        _ => Some((clicked, false)),
    }
}

/// The *Character* cell's text.
///
/// Upstream prints the code point instead of the character when it belongs to
/// an invisible general category (control, space, line and paragraph
/// separators) — a tab drawn as a tab is a blank cell, and a blank cell is not
/// a warning. `is_control`/`is_whitespace` are Rust's closest equivalents: they
/// cover those four categories, and the ones they cover extra are separators
/// upstream also hides.
fn character_cell(character: &str) -> String {
    match character.chars().next() {
        Some(c) if c.is_control() || c.is_whitespace() => format!("U+{:04X}", c as u32),
        _ => character.to_string(),
    }
}

/// The completion popup, while it is up.
///
/// Only the *anchor* is kept: the partial word's start, and which row is
/// highlighted. The candidates themselves are derived from the buffer and the
/// caret on every frame, which is what makes typing filter the list without
/// anything having to tell the popup to re-read anything — and why the popup
/// cannot show a word that is no longer there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Completion {
    /// Where the partial word begins.
    start: usize,
    /// The highlighted candidate.
    selected: usize,
}

/// What the Outline inspector's tree was last built from.///
/// The inspector is a virtualized list, so its items have to be *pushed* into
/// it rather than computed while rendering — and pushing them is a mutation,
/// which a render cannot do. It therefore refreshes from a deferred callback,
/// and this is what tells that callback whether anything it cares about has
/// changed. The buffer is reduced to a hash rather than kept: the point is only
/// to notice an edit, and a second copy of the document is a lot to hold for
/// that.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OutlineSource {
    document: DocumentId,
    syntax: SharedString,
    text: u64,
    filter: String,
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
    _find_query_changes: Subscription,
    find_replacement: Entity<InputState>,
    /// The result-table rows the user has selected, as a set of match ranges.
    ///
    /// Upstream's `FindPanelResultView` keeps a `Set<Match.ID>`; the match
    /// range is SubText's stable row identity, so a selection survives the
    /// table being re-sorted. Copy reads it back in display order.
    find_selected_ranges: Vec<Range<usize>>,
    /// The row every extended selection grows from — the last row chosen as
    /// the primary one, which is what shift-click and shift-arrow measure from.
    find_selection_anchor: Option<Range<usize>>,
    /// Whether the result table is the keyboard focus target, which routes
    /// ⌘C and the standard font-size commands to the table instead of the
    /// editor. Upstream gets this from the responder chain for free; here the
    /// panel's focus listeners keep it current.
    find_results_focused: bool,
    /// The sidebar's *Folder Find* pane. It owns its own query, options and
    /// results, because none of them belong to a document.
    folder_find: Entity<FolderFindPane>,
    status: Option<Status>,
    /// Set while a document is being loaded, so the change handler does not
    /// mark it dirty.
    loading: bool,
    settings_window: Option<AnyWindowHandle>,
    /// The settings window's view, for the commands that act on the window
    /// rather than on the document: *Edit Snippets…* asks for a pane and
    /// *Customize Encodings List…* asks the Format pane to present its sheet.
    ///
    /// Weak, because the window owns the view. The handle above says whether
    /// the window is still up; this says whether there is still something to
    /// talk to, and the two are cleared together.
    settings_view: Option<WeakEntity<SettingsWindow>>,
    about_window: Option<AnyWindowHandle>,
    /// The console, while it is up. Its log outlives the window — the window is
    /// only the view — so closing it and running `Show Console` again brings the
    /// same entries back.
    console_window: Option<AnyWindowHandle>,
    /// The bracket pair the caret was last next to, so a caret that slides
    /// along the same pair does not restart the flash on every key.
    matching_bracket: Option<std::ops::Range<usize>>,
    /// The selection the pair was worked out for. The editor notifies on every
    /// repaint — the caret's blink included — and a repaint that did not move
    /// the caret cannot change the answer.
    matching_bracket_selection: Option<std::ops::Range<usize>>,
    /// The Quick Actions bar, while it is up. It is a panel over this window
    /// rather than part of it, so it is tracked the way the settings window is.
    quick_actions_window: Option<AnyWindowHandle>,
    /// The colour the *Edit Color Code* panel opens on when the caret is not on
    /// a colour code.
    ///
    /// CotEditor keeps this in the shared `NSColorPanel`, which survives for the
    /// length of a session; SubText has no shared panel, so the workspace holds
    /// the last colour inserted instead. Like upstream's, it is not persisted.
    last_color_code_color: colorcode::Color,
    /// The Outline inspector's tree. See [`OutlineSource`] for why it is held
    /// rather than built while rendering.
    outline_tree: Entity<TreeState>,
    /// The Outline inspector's filter field, in CotEditor's bottom-of-pane slot.
    outline_filter: Entity<InputState>,
    /// The tree's nodes, as the last refresh left them.
    outline_nodes: Vec<FilteredNode>,
    /// What that tree was built from, so a stale one can be noticed.
    outline_built_from: Option<OutlineSource>,
    /// The flat entries behind `outline_nodes`, kept so that moving the caret
    /// can find the row it sits in without re-parsing the document.
    outline_entries: Vec<OutlineEntry>,
    /// Per-row drawing data, keyed by entry id: the tree hands its renderer an
    /// id and a label, and nothing else.
    outline_rows: Rc<HashMap<String, OutlineRow>>,
    /// The ids the user has opened. The tree owns the expansion state of the
    /// items it was handed, but every refresh hands it fresh ones, so the set is
    /// mirrored here and re-applied.
    outline_expanded: HashSet<String>,
    /// The ids of the tree the last refresh built, for the "which ids are new"
    /// comparison that decides what opens by itself.
    outline_ids: Vec<String>,
    /// The entry the caret sits in, so that a caret move can be noticed.
    outline_selected: Option<String>,
    /// Set while a refresh is queued, so that a frame which runs before the
    /// queue drains does not queue a second one.
    outline_refresh_queued: Cell<bool>,
    /// Kept alive for as long as the workspace lives: dropping a subscription
    /// unsubscribes, and the tree's expansion state is mirrored through this
    /// one.
    _outline_expansion: Subscription,
    /// The tree's row-selection observer.
    _outline_selection: Subscription,
    /// The filter field's `Change` subscription.
    _outline_filter_changes: Subscription,
    /// What the menu bar was last built from.
    ///
    /// The bar is application-wide, and four of its submenus are filled in from
    /// the document in front — the snippets, the line endings, the syntax and the
    /// encoding, each with the setting in force checked. CotEditor's
    /// `SnippetManager` watches `currentSyntaxName` for the same reason; this is
    /// what tells the workspace the watch has something to do.
    menus_built_for: Option<menus::MenuSignature>,
    /// The user's scripts, as the last menu rebuild read them.
    ///
    /// Held rather than read on demand because a command names a script by its
    /// path, and the path has to be resolved against the same list the item was
    /// built from — a script deleted between the two should report that it is
    /// gone rather than run a neighbour that happens to sort next to it.
    scripts: Vec<ScriptItem>,
    /// Which of the Document inspector's four sections are open.
    ///
    /// Upstream keeps one `@State private var isExpanded = true` per section,
    /// so all four start open and a collapse is forgotten when the view is
    /// recreated — that is, per document window. The workspace is the closest
    /// thing SubText has to a document window, and the state has to live
    /// *somewhere*: the accordion rebuilds its own state on every frame, and
    /// the inspector redraws on every keystroke.
    inspector_sections: [bool; INSPECTOR_SECTION_COUNT],
    /// The Inspect Character popover, while it is up: the selection it was
    /// opened for, and what that selection is.
    ///
    /// Held together so the surface cannot outlive its subject — the popover is
    /// only drawn while the selection is still the one it describes.
    inspect_character: Option<(Range<usize>, charinfo::CharacterReport)>,
    /// Which column the *Incompatible Characters* table is sorted by.
    ///
    /// `None` is upstream's initial `sortOrder = []`: the list arrives in
    /// document order and stays there until a header is clicked.
    warnings_sort: Option<(WarningColumn, bool)>,
    /// The same for the *Inconsistent Line Endings* table.
    ///
    /// Separate because upstream's two tables own separate `@State`
    /// `sortOrder`s — sorting one must not reorder the other.
    line_endings_sort: Option<(WarningColumn, bool)>,
    /// The row of the warnings tables that was last clicked, so it can stay
    /// highlighted the way a selected table row does upstream.
    warnings_selected: Option<Range<usize>>,
    /// Whether each Block row of that popover is showing the name the interface
    /// uses rather than the one `Blocks.txt` gives.
    ///
    /// One per row, because upstream keeps `isBlockNameLocalized` as `@State`
    /// inside `ScalarDetailView`, so every row remembers its own.
    inspect_block_localized: Vec<bool>,
    /// Which of the popover's per-scalar rows are open.
    inspect_scalar_open: Vec<bool>,
    /// The advanced counter's options panel, while it is up.
    ///
    /// Built once when the sheet opens and kept afterwards: the running
    /// counter's gear shows the same entity, because a second one would be a
    /// second set of selects to keep in step with the settings.
    count_options: Option<Entity<CountOptionsView>>,
    /// Whether the counter is running, which is what shows the capsule.
    count_running: bool,
    /// Whether the capsule's gear popover is open.
    count_options_open: bool,
    /// The **Find & Replace** panel's window, while it is up.
    ///
    /// Weak in the same way the settings window is: the window owns the view,
    /// and this only says whether the window is still there.
    find_window: Option<AnyWindowHandle>,
    /// The document window the panel belongs to.
    ///
    /// A command that edits the buffer has to be deferred onto the window that
    /// owns the editor, and the panel is a window of its own — so it asks for
    /// this handle rather than using its own.
    find_window_owner: Option<AnyWindowHandle>,
    /// The word completion popup, while it is up.
    completion: Option<Completion>,
    /// The pending automatic-completion timer, while one is running.
    ///
    /// Upstream holds a `Debouncer` that is rescheduled by every keystroke and
    /// cancelled when the list is accepted or dismissed. A `Task` dropped is a
    /// task cancelled, so replacing this is the same gesture.
    completion_auto: Option<gpui_kit::Task<()>>,
    /// Which find indicator is up.
    ///
    /// Each flash bumps this, and the timer that clears the decoration only
    /// acts if it still owns the current number — so a second find inside the
    /// first one's lifetime does not have its mark wiped by the older timer.
    find_indicator: u64,
    /// The pending incremental-search timer, while one is running.
    ///
    /// Upstream cancels and restarts its `findTask` on every keystroke in the
    /// find field; a `Task` dropped is a task cancelled.
    incremental_auto: Option<gpui_kit::Task<()>>,
    /// The incompatible-character scan, memoised.
    ///
    /// The scan is a codec round trip per chunk, so it cannot run on every
    /// frame; the key is the buffer's hash and the encoding, and the answer is
    /// thrown away when either moves. A memo rather than state: nothing outside
    /// the pane reads it, and recomputing it does not change what is drawn.
    incompatible_cache:
        std::cell::RefCell<Option<(u64, TextEncoding, Vec<subtext_core::IncompatibleCharacter>)>>,
    /// How far the counting capsule has been dragged from its corner, in
    /// pixels: `(right, down)` — the two edges it is anchored to.
    ///
    /// Upstream's `DraggableHostingView` lets the reader put the capsule
    /// anywhere over the text; this is that position, measured from the corner
    /// it starts in rather than from the window, so a resize does not move it.
    count_capsule_offset: (f32, f32),
    /// The pointer's last position while the capsule is being dragged.
    ///
    /// Drag deltas come from comparing successive window positions, which keeps
    /// the arithmetic in one frame: the capsule's own coordinates are relative
    /// to the editor area.
    count_capsule_drag: Option<(f32, f32)>,
    /// Whether the menu bar was last built with an empty selection.
    ///
    /// The bar is a tree that is set once, so the `Straighten Quotes` item is
    /// built already greyed or already live. This is what notices the flip —
    /// and it is compared on every edit, which is a boolean, rather than
    /// rebuilding the bar (and re-reading the script folder) on every keystroke.
    menu_selection_empty: bool,
    /// Watches for the find panel closing.
    ///
    /// The panel is a window of its own and reports nothing on the way out, but
    /// its incremental-search marks live on in the document — upstream drops
    /// them in `unhighlight(nil)`, which the panel's close triggers. Watching
    /// window closes is the equivalent seam here.
    _find_panel_closed: Option<Subscription>,
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
        // Typing in the find field searches as it goes — upstream's
        // `findSearchesIncrementally`, default on. The search leaves the
        // *Find All* table alone; it only updates the match state.
        let find_query_changes =
            cx.subscribe(&find_query, |this: &mut Self, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.incremental_search(cx);
                }
            });
        // Upstream's second placeholder is *Replace with*, not *Replace* — it is
        // a prepositional phrase there, and the panel's window title already
        // says which command this is.
        let find_replacement = cx.new(|cx| InputState::new(window, cx).placeholder("Replace with"));

        // The pane reads its switches out of the settings at construction and
        // writes them back as they change, so it is built from the same
        // `Settings` this workspace is about to take ownership of.
        let folder_find = {
            let workspace = cx.entity().downgrade();
            cx.new(|cx| FolderFindPane::new(workspace, &settings, window, cx))
        };

        // The Outline inspector's tree and its filter field. Both need wiring
        // before they are stored, so they are created here rather than inline.
        let outline_tree = cx.new(|cx| TreeState::new(cx).items(Vec::<TreeItem>::new()));
        let outline_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter"));

        // Opening and closing a row is the tree's own business — it keeps the
        // state on the item it was handed — but a refresh hands it fresh items,
        // so the state has to be mirrored into the workspace and re-applied.
        let outline_expansion = cx.subscribe(
            &outline_tree,
            |this: &mut Self, _tree, event: &TreeEvent, _cx| match event {
                TreeEvent::Expanded(id) => {
                    this.outline_expanded.insert(id.to_string());
                }
                TreeEvent::Collapsed(id) => {
                    this.outline_expanded.remove(id.as_ref());
                }
            },
        );

        // Selecting a row in the tree — by click or by arrow key — is what
        // moves the caret, which is CotEditor's `selectItem(id:)`. `observe_in`
        // rather than `observe` because the row has to be *centred*, and that
        // needs the window: see `on_outline_row_selected`.
        let outline_selection = cx.observe_in(
            &outline_tree,
            window,
            |this: &mut Self, tree, window, cx| {
                this.on_outline_row_selected(tree, window, cx);
            },
        );

        // Typing in the filter changes nothing but the filter, and the refresh
        // that follows is what notices it; the repaint has to be asked for.
        let outline_filter_changes = cx.subscribe_in(
            &outline_filter,
            window,
            |_this: &mut Self, _input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        );

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
            _find_query_changes: find_query_changes,
            find_replacement,
            find_selected_ranges: Vec::new(),
            find_selection_anchor: None,
            find_results_focused: false,
            folder_find,
            status: None,
            loading: false,
            settings_window: None,
            settings_view: None,
            about_window: None,
            console_window: None,
            matching_bracket: None,
            matching_bracket_selection: None,
            quick_actions_window: None,
            last_color_code_color: colorcode::Color::BLACK,
            outline_tree,
            outline_filter,
            outline_nodes: Vec::new(),
            outline_built_from: None,
            outline_entries: Vec::new(),
            outline_rows: Rc::new(HashMap::new()),
            outline_expanded: HashSet::new(),
            outline_ids: Vec::new(),
            outline_selected: None,
            outline_refresh_queued: Cell::new(false),
            _outline_expansion: outline_expansion,
            _outline_selection: outline_selection,
            _outline_filter_changes: outline_filter_changes,
            menus_built_for: None,
            scripts: Vec::new(),
            inspector_sections: [true; INSPECTOR_SECTION_COUNT],
            inspect_character: None,
            warnings_sort: None,
            line_endings_sort: None,
            warnings_selected: None,
            inspect_block_localized: Vec::new(),
            inspect_scalar_open: Vec::new(),
            count_options: None,
            count_running: false,
            count_options_open: false,
            find_window: None,
            find_window_owner: None,
            completion: None,
            completion_auto: None,
            find_indicator: 0,
            incremental_auto: None,
            incompatible_cache: std::cell::RefCell::new(None),
            menu_selection_empty: true,
            count_capsule_offset: (0., 0.),
            count_capsule_drag: None,
            _find_panel_closed: None,
        };
        // The incremental search's grey marks belong to the panel's lifetime,
        // not to the document's.
        let weak = cx.entity().downgrade();
        this._find_panel_closed = Some(cx.on_window_closed(move |cx, id| {
            weak.update(cx, |workspace, cx| {
                if workspace
                    .find_window
                    .is_some_and(|handle| handle.window_id() == id)
                {
                    workspace.find_window = None;
                    workspace.clear_editor_marks(cx);
                }
            })
            .ok();
        }));
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SCRIPT_POLL).await;
                // Once the window is gone the entity is gone with it, and there
                // is nothing left to keep in step.
                if this
                    .update(cx, |workspace, cx| workspace.refresh_menus(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        this.new_document(window, cx);
        this
    }

    // ---- document management --------------------------------------------

    fn new_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let document = Document::untitled(
            self.settings.default_file_encoding(),
            self.settings.default_line_ending,
        );
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

    /// Open a new document already holding `text`, in `syntax_name`.
    ///
    /// Used by the one command that creates a document with something in it —
    /// `Help ▸ Create Issue Report`, whose whole point is that the template and
    /// the environment are already written. The document is dirty, because
    /// closing it without saving throws the text away.
    fn new_document_with(
        &mut self,
        text: &str,
        syntax_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut document = Document::untitled(
            self.settings.default_file_encoding(),
            self.settings.default_line_ending,
        );
        document.set_text(text.to_string(), true);
        self.push_document(document, syntax_name.to_string(), window, cx);
    }

    /// `Edit ▸ Copy as Rich Text` — the selection with its syntax colours.
    ///
    /// Upstream's `copyWithStyle(_:)` builds an `NSAttributedString` from the
    /// selection (plain text plus the colour the highlighter put on each run)
    /// and hands it to `NSPasteboard.writeObjects`, which publishes RTF.
    ///
    /// GPUI's clipboard carries text or an image and has no styled-text entry,
    /// so the RTF is built in `subtext_core::richtext` and put on the pasteboard
    /// through `osascript` — the one call that can write a pasteboard type GPUI
    /// does not model. A platform without `osascript` falls back to plain text
    /// rather than pretending: the status bar says which one happened.
    pub fn copy_as_rich_text(&mut self, cx: &mut Context<Self>) {
        let selection = self.active_selection(cx);
        if selection.is_empty() {
            // Upstream beeps here; a status line says the same thing and is
            // still there a second later.
            self.set_status("Select something to copy", cx);
            return;
        }
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let text = document.editor().read(cx).value().to_string();
        if selection.end > text.len() {
            return;
        }
        let syntax_name = document.syntax.clone();

        // The colours come from the same two pieces the editor paints with: the
        // bundle's rules (or its tree-sitter query) and the theme resolver.
        let runs = match self.syntaxes.by_name(&syntax_name) {
            Some(syntax) => {
                let mut highlighter = crate::highlighter::CotHighlighter::new(syntax);
                highlighter.rehighlight_text(&text);
                let resolver = crate::highlighter::CotStyleResolver::new(&self.theme);
                highlighter.styled_runs(&selection, &resolver)
            }
            None => Vec::new(),
        };

        // A byte the highlighter said nothing about keeps the document's own
        // text colour rather than RTF's "automatic", which would come out in
        // whatever the receiving application happens to use.
        let body = rgb_of_theme(self.theme.text_color());
        let mut segments: Vec<subtext_core::richtext::Run<'_>> = Vec::new();
        let mut cursor = selection.start;
        for (range, style) in runs {
            if range.start > cursor {
                segments.push(subtext_core::richtext::Run {
                    text: &text[cursor..range.start],
                    color: Some(body),
                });
            }
            segments.push(subtext_core::richtext::Run {
                text: &text[range.clone()],
                color: Some(style.color.map(rgb_of).unwrap_or(body)),
            });
            cursor = range.end;
        }
        if cursor < selection.end {
            segments.push(subtext_core::richtext::Run {
                text: &text[cursor..selection.end],
                color: Some(body),
            });
        }

        let font = self
            .settings
            .font_family
            .clone()
            .unwrap_or_else(|| cx.theme().mono_font_family.to_string());
        let rtf = subtext_core::richtext::rtf(&segments, &font, self.settings.font_size);

        let plain = text[selection].to_string();
        match put_rich_text_on_the_pasteboard(&rtf, &plain, cx) {
            Ok(true) => self.set_status("Copied with syntax colours", cx),
            Ok(false) => self.set_status("Copied as plain text — no RTF pasteboard here", cx),
            Err(error) => self.set_status(format!("The clipboard refused the text: {error}"), cx),
        }
    }

    /// `Help ▸ Create Issue Report` — a new Markdown document holding the bug
    /// report template, with this machine's version, system and language
    /// already filled in.
    ///
    /// Upstream's `createBugReport(_:)` does exactly this: it opens an untitled
    /// document with `IssueReport().template` and sets its syntax to Markdown.
    /// The version and the system are the two things a report most often gets
    /// wrong, which is why they are not left to the person writing it.
    pub fn create_bug_report(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let environment = subtext_core::issuereport::Environment::detect();
        let template = subtext_core::issuereport::IssueReport::template(&environment);
        self.new_document_with(&template, "Markdown", window, cx);
        self.set_status("Bug report template created — fill it in and post it", cx);
    }

    fn push_document(
        &mut self,
        document: Document,
        syntax_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = document.text().to_string();
        let editor = self.new_editor(text, &syntax_name, window, cx);
        let pane = self.new_pane(editor, window, cx);

        let id = DocumentId(self.next_document_id);
        self.next_document_id += 1;

        self.documents.push(OpenDocument {
            id,
            document,
            panes: vec![pane],
            active_pane: 0,
            syntax: SharedString::from(syntax_name),
            modified_on_disk: false,
            // Filled on the first repaint: `push_document` has just read the
            // text, and counting it there would put a megabyte's worth of work
            // in front of the window appearing.
            counts: None,
        });
        self.active = self.documents.len() - 1;
        self.refresh_menus(cx);
        cx.notify();
    }

    /// Build one editor pane showing `text`.
    ///
    /// Split panes differ only in their buffer, their caret and their scroll
    /// position; everything else — font, size, line numbers, soft wrap,
    /// whitespace, indent guides, syntax and theme — comes from the same
    /// settings, which is why a pane is built from settings rather than
    /// copied from a sibling.
    fn new_editor(
        &self,
        text: String,
        syntax_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<gpui_kit::base::input::EditorState> {
        // No `set_editor_style` here on purpose. The pane is a component
        // `Editor`, and `Input::render` re-applies the style from the component
        // theme on every frame — a style set on the state is overwritten before
        // it is read. The colours come from `highlighter::project_theme`
        // instead; see the note on it.
        let language = syntax_name.to_string();
        let options = self.settings.editor;

        cx.new(|cx| {
            let mut state = gpui_kit::base::input::EditorState::new(window, cx);
            if let Some(factory) = highlighter::current_factory() {
                state.set_highlighter_factory(factory, cx);
            }
            state.set_value(text, window, cx);
            state.set_highlighter(language, cx);
            state.set_line_number(options.show_line_numbers, window, cx);
            state.set_soft_wrap(options.wrap_lines, window, cx);
            state.set_show_whitespaces(options.show_invisibles, window, cx);
            state.set_indent_guides(options.show_indent_guides, window, cx);
            state.set_readonly(!options.editable, cx);
            state
        })
    }

    /// One pane: its editor, the subscription that mirrors it into the
    /// document, and the collection its find indicator paints into.
    fn new_pane(
        &mut self,
        editor: Entity<gpui_kit::base::input::EditorState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Pane {
        let indicator = editor.update(cx, |state, cx| {
            state.create_decorations_collection(Vec::new(), cx)
        });
        let marks = editor.update(cx, |state, cx| {
            state.create_decorations_collection(Vec::new(), cx)
        });
        let (subscription, observed) = self.subscribe_pane(&editor, window, cx);
        Pane {
            editor,
            indicator,
            marks,
            _subscription: subscription,
            _observed: observed,
        }
    }

    /// Follow one pane's buffer back into the document model.
    fn subscribe_pane(
        &mut self,
        editor: &Entity<gpui_kit::base::input::EditorState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Subscription, Subscription) {
        // The editor notifies whenever it repaints, which includes every caret
        // move and every selection change — the two things the menu bar has to
        // follow and the two it never reports as an `InputEvent`. The handler
        // is a boolean compare, so a cursor blink costs one `is_empty`.
        // The editor notifies *during* our own update of it, so touching the
        // workspace from here would hit GPUI's re-entrancy guard. The check is
        // therefore queued, and queued once: a burst of repaints (a blink, a
        // scroll) collapses into a single comparison.
        let weak = cx.entity().downgrade();
        let queued = std::rc::Rc::new(std::cell::Cell::new(false));
        let observed = cx.observe(editor, move |_, _, cx| {
            if queued.replace(true) {
                return;
            }
            let weak = weak.clone();
            let queued = queued.clone();
            cx.defer(move |cx| {
                queued.set(false);
                weak.update(cx, |workspace, cx| {
                    workspace.sync_menu_gates(cx);
                    // Upstream runs the bracket match on every selection
                    // change, not only on an edit — and the editor notifies on
                    // exactly that (see the note above).
                    workspace.flash_matching_bracket(cx);
                })
                .ok();
            });
        });
        let changed = cx.subscribe_in(
            editor,
            window,
            |this: &mut Self, editor, event: &gpui_kit::base::input::InputEvent, window, cx| {
                if matches!(event, gpui_kit::base::input::InputEvent::Change) && !this.loading {
                    this.on_pane_changed(editor, window, cx);
                }
            },
        );
        (changed, observed)
    }

    /// Mirror one pane's buffer into the document model and its siblings.
    fn on_pane_changed(
        &mut self,
        editor: &Entity<gpui_kit::base::input::EditorState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .documents
            .iter()
            .position(|document| document.editors().any(|pane| pane == editor))
        else {
            return;
        };
        let pane = self.documents[index]
            .editors()
            .position(|candidate| candidate == editor)
            .unwrap_or(0);
        self.documents[index].active_pane = pane;

        let text = editor.read(cx).value().to_string();
        // Bringing a sibling up to date emits `Change` as well. Its text is
        // already what the document holds, so there is nothing to mirror and
        // nothing further to copy — which is what keeps the round trip from
        // repeating.
        if self.documents[index].document.text() == text {
            return;
        }

        let current_line_ending = self.documents[index].document.line_ending();
        // Upstream opens the automatic list from `insertText`, so only a
        // buffer that *grew* counts as typing: an undo or a deletion is not,
        // and offering a completion over one would be the interruption the
        // guards exist to avoid.
        //
        // Growth is the closest this seam gets: the editor reports one `Change`
        // per edit and does not say whether the text arrived from a keystroke,
        // an input method commit, or a paste. The first two are upstream's
        // trigger; a paste that happens to land at the end of a word is not,
        // and is recorded in the gap table.
        let grew = text.len() > self.documents[index].document.text().len();
        self.documents[index].document.set_text(text.clone(), true);
        // A document whose buffer no longer uses its declared ending has been
        // retyped; keep the model honest rather than lying in the status bar.
        let detected = subtext_core::line_ending::detect(&text);
        if detected != current_line_ending && !text.is_empty() {
            self.documents[index].document.set_line_ending(detected);
        }
        self.find.matches.clear();
        self.find.current = None;
        self.sync_panes(index, pane, text, window, cx);
        if grew {
            self.schedule_automatic_completion(cx);
        }
        // The text changed, so the status bar's counts are stale.
        self.documents[index].counts = None;
        // The text moved under the caret, so the pair may have changed even
        // though the selection did not — the de-duplication the flash does for
        // repaints must not skip this one.
        self.matching_bracket_selection = None;
        self.flash_matching_bracket(cx);
        cx.notify();
    }

    /// Copy `text` into every pane of `index` but `source`.
    ///
    /// Deferred for the same reason every other programmatic text write is:
    /// `replace_all` emits `Change`, and this runs inside the workspace update
    /// that the source pane's own change started. Each target pane's caret and
    /// scroll position are put back afterwards, because replacing the whole
    /// text resets both — CotEditor's panes share one text storage and keep
    /// their own selections, so a pane the user is not typing in must not jump
    /// to the top of the document on every keystroke elsewhere.
    fn sync_panes(
        &mut self,
        index: usize,
        source: usize,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let targets: Vec<_> = self.documents[index]
            .panes
            .iter()
            .enumerate()
            .filter(|(pane, _)| *pane != source)
            .map(|(_, pane)| pane.editor.clone())
            .collect();
        if targets.is_empty() {
            return;
        }

        window.defer(cx, move |window, cx| {
            for editor in targets {
                editor.update(cx, |state, cx| {
                    if state.value().as_ref() == text.as_str() {
                        return;
                    }
                    let caret = state.selected_range();
                    let scroll = state.scroll_offset();
                    state.replace_all(text.clone(), window, cx);
                    state.set_selected_range(caret, cx);
                    state.set_scroll_offset(scroll, cx);
                });
            }
        });
    }

    /// The buffer of the active document, straight from the focused editor.
    fn active_text(&self, cx: &App) -> Option<String> {
        let document = self.documents.get(self.active)?;
        Some(document.editor().read(cx).value().to_string())
    }

    /// The active document's selection, in byte offsets.
    fn active_selection(&self, cx: &App) -> Range<usize> {
        self.documents
            .get(self.active)
            .map(|document| document.editor().read(cx).selected_range())
            .unwrap_or(0..0)
    }

    /// Every pane of every open document.
    fn all_panes(&self) -> Vec<Entity<gpui_kit::base::input::EditorState>> {
        self.documents
            .iter()
            .flat_map(|document| document.editors().cloned())
            .collect()
    }

    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self
            .documents
            .iter()
            .position(|doc| doc.document.path() == Some(path.as_path()))
        {
            self.active = index;
            self.refresh_menus(cx);
            cx.notify();
            return;
        }

        let candidates = self.settings.detection_candidates();
        let options = self.settings.detection_options(&candidates);
        match Document::open(&path, self.settings.encoding(), &options) {
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
        cx.spawn(async move |this, cx| match receiver.await {
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
        })
        .detach();
    }

    /// Save the active document, asking for a path when it has none or when
    /// Save As was chosen.
    pub fn save(&mut self, as_new_path: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.documents.get(self.active).map(|document| document.id) else {
            return;
        };
        self.save_document(id, as_new_path, false, window, cx);
    }

    /// Save one document by identity, asking for a path when it has none or
    /// when Save As was chosen.
    ///
    /// Identity rather than position because the close flow saves the tab the
    /// reader asked to close, which is not necessarily the one in front — and
    /// because the save panel answers after the list may have moved.
    ///
    /// `close_after` is upstream's close flow, which continues once the save
    /// panel answers. It travels *with the write* rather than being parked on
    /// the workspace: a dismissed panel then leaves nothing behind that could
    /// close the tab on some later save.
    fn save_document(
        &mut self,
        id: DocumentId,
        as_new_path: bool,
        close_after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.find_document(id) else {
            return;
        };

        // Pull the buffer back out of the editor before writing.
        let text = self.documents[index].editor().read(cx).value().to_string();
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
                // A dismissed panel is a cancelled save: no write, and so no
                // close either. Nothing is left behind to remember otherwise.
                let Ok(Ok(Some(path))) = receiver.await else {
                    return;
                };
                let _ = handle.update(cx, move |_, window, cx| {
                    this.update(cx, |workspace, cx| {
                        workspace.write_document(id, Some(path), close_after, window, cx);
                    })
                    .ok();
                });
            })
            .detach();
            return;
        }

        self.write_document(id, None, close_after, window, cx);
    }

    /// Write one document, to `target` when one was chosen.
    fn write_document(
        &mut self,
        id: DocumentId,
        target: Option<PathBuf>,
        close_after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.find_document(id) else {
            return;
        };
        let text = self.documents[index].editor().read(cx).value().to_string();
        self.documents[index].document.set_text(text, true);

        let written = match self.documents[index].document.save(target.clone()) {
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
                true
            }
            Err(subtext_core::DocumentError::LossyEncoding(name)) => {
                // The alert asks, so the write may yet happen — and with it the
                // close this save was asked to complete, which is why the
                // intent is handed to the alert.
                self.ask_lossy_saving(id, name, target, close_after, window, cx);
                return;
            }
            Err(error) => {
                self.set_status(error.to_string(), cx);
                false
            }
        };
        // A save the reader started from the close alert is also the answer to
        // that alert: upstream closes the window once the write returns. A
        // failed write closes nothing — the tab stays up with the message.
        if written && close_after {
            if let Some(index) = self.find_document(id) {
                self.close_document_now(index, window, cx);
            }
            return;
        }
        let _ = window;
        cx.notify();
    }

    /// The position of a document by identity.
    fn find_document(&self, id: DocumentId) -> Option<usize> {
        self.documents.iter().position(|document| document.id == id)
    }

    /// The lossy-save alert — upstream's `DocumentSavingError(code:
    /// .lossyEncoding)`. *Save Available Text* permits lossy saving for the
    /// rest of the session and writes; *Review Incompatible Characters*
    /// opens the Warnings inspector without saving; *Cancel* does nothing.
    fn ask_lossy_saving(
        &mut self,
        id: DocumentId,
        name: &'static str,
        target: Option<PathBuf>,
        close_after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message = format!("The document contains characters incompatible with “{name}”.");
        let body = "Incompatible characters are either substituted or removed in saving. \
                    Do you want to save anyway?";
        let workspace = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, _| {
            let save = workspace.clone();
            let review = workspace.clone();
            let save_target = target.clone();
            dialog
                .w(window.rem_size() * 26.)
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().child(message.clone()))
                        .child(div().text_sm().child(body)),
                )
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("Cancel")))
                        .child(
                            Button::new("lossy-review")
                                .label("Review Incompatible Characters")
                                .on_click(move |_, window, cx| {
                                    review
                                        .update(cx, |workspace, cx| {
                                            workspace.show_inspector(InspectorPane::Warnings, cx)
                                        })
                                        .ok();
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("lossy-save")
                                .label("Save Available Text")
                                .primary()
                                .on_click(move |_, window, cx| {
                                    save.update(cx, |workspace, cx| {
                                        let Some(index) = workspace.find_document(id) else {
                                            return;
                                        };
                                        workspace.documents[index]
                                            .document
                                            .set_allows_lossy_saving(true);
                                        workspace.write_document(
                                            id,
                                            save_target.clone(),
                                            close_after,
                                            window,
                                            cx,
                                        );
                                    })
                                    .ok();
                                    window.close_dialog(cx);
                                }),
                        ),
                )
                .on_ok(|_, _, _| false)
        });
    }

    /// Reload the active document from disk, discarding the buffer.
    pub fn revert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let candidates = self.settings.detection_candidates();
        let options = self.settings.detection_options(&candidates);
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        if document.document.is_untitled() {
            self.set_status("This document has never been saved", cx);
            return;
        }
        match document.document.revert(&options) {
            Ok(()) => {
                let text = document.document.text().to_string();
                self.loading = true;
                for editor in document.editors() {
                    editor.update(cx, |state, cx| state.set_value(text.clone(), window, cx));
                }
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
    ///
    /// A document with unsaved changes asks first — upstream's `canClose` —
    /// and a clean one closes without a word (upstream also suppresses the
    /// question for an empty draft).
    pub fn close_document(&mut self, id: DocumentId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.documents.iter().position(|document| document.id == id) else {
            return;
        };
        if self.documents[index].document.is_dirty() {
            self.ask_close_document(id, window, cx);
            return;
        }
        if self.documents.len() <= 1 {
            // CotEditor closes the window with the last document; SubText keeps
            // one document open so the application stays usable.
            self.new_document(window, cx);
            return;
        }
        self.close_document_now(index, window, cx);
    }

    /// The unsaved-changes alert — upstream's `DocumentClosingAlert`. A
    /// lossy-to-save document replaces *Save* with *Save Available Text* and
    /// says so in the body, exactly as upstream swaps its buttons.
    ///
    /// The *Save* answer writes a named document and then closes it. An
    /// untitled one opens the save panel, whose outcome arrives
    /// asynchronously, so the tab stays open either way and a second close on
    /// the now-clean document succeeds without asking.
    fn ask_close_document(&mut self, id: DocumentId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.documents.iter().position(|document| document.id == id) else {
            return;
        };
        let open = &self.documents[index];
        let name = open.document.display_name();
        let encoding_name = open.document.encoding().name;
        let lossy = !open.document.allows_lossy_saving()
            && !subtext_core::incompatible_characters(
                open.document.text(),
                open.document.encoding(),
            )
            .is_empty();

        let message = if lossy {
            format!(
                "Do you want to save the changes made to the document “{name}”?\n\
                 The document contains characters incompatible with “{encoding_name}”."
            )
        } else {
            format!("Do you want to save the changes made to the document “{name}”?")
        };
        let body = if lossy {
            "Your changes will be lost if you don’t save them. Incompatible characters \
             are either substituted or removed in saving."
        } else {
            "Your changes will be lost if you don’t save them."
        };

        let workspace = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, _| {
            let save = workspace.clone();
            let discard = workspace.clone();
            let save_label = if lossy { "Save Available Text" } else { "Save" };
            let save_button =
                Button::new("close-save")
                    .label(save_label)
                    .on_click(move |_, window, cx| {
                        save.update(cx, |workspace, cx| {
                            let Some(index) = workspace.find_document(id) else {
                                return;
                            };
                            // The alert's button *is* "save the available text":
                            // upstream permits lossy saving from here rather than
                            // asking a second question.
                            workspace.documents[index]
                                .document
                                .set_allows_lossy_saving(true);
                            // Save this very document and close it once the write
                            // returns. An untitled one has no path yet, so the
                            // panel opens and the close waits for its answer —
                            // which is the whole reason this is one call rather
                            // than a save followed by an unrelated close.
                            workspace.save_document(id, false, true, window, cx);
                        })
                        .ok();
                        window.close_dialog(cx);
                    });
            dialog
                .w(window.rem_size() * 26.)
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().child(message.clone()))
                        .child(div().text_sm().child(body)),
                )
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("Cancel")))
                        .child(
                            Button::new("close-discard")
                                .label("Don’t Save")
                                .danger()
                                .on_click(move |_, window, cx| {
                                    discard
                                        .update(cx, |workspace, cx| {
                                            let index = workspace
                                                .documents
                                                .iter()
                                                .position(|document| document.id == id);
                                            if let Some(index) = index {
                                                workspace.close_document_now(index, window, cx);
                                            }
                                        })
                                        .ok();
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(save_button),
                )
                .on_ok(|_, _, _| false)
        });
    }

    /// The removal itself, once a close has been confirmed.
    fn close_document_now(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.documents.len() <= 1 {
            // The confirmation may also land on the last document's tab; a
            // confirmed close there still leaves one document open.
            self.new_document(window, cx);
            return;
        }
        self.documents.remove(index);
        // Keep the selection on the same document where possible, and on the
        // neighbour that took its place when the closed one was selected.
        self.active = self.active.min(self.documents.len() - 1);
        self.refresh_menus(cx);
        cx.notify();
    }

    fn active_document(&self) -> Option<&OpenDocument> {
        self.documents.get(self.active)
    }

    fn set_status(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.status = Some(Status {
            message: message.into(),
            at: std::time::Instant::now(),
        });
        cx.notify();
    }

    /// Say something in the status bar.
    pub fn notice(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.set_status(message, cx);
    }

    pub fn clear_recent(&mut self, cx: &mut Context<Self>) {
        self.settings.recent_documents.clear();
        let _ = self.settings.save();
        // The list is what File ▸ Open Recent is built from, so the bar has to
        // hear about it. `refresh_menus` compares the signature and rebuilds, so
        // this is one rebuild rather than one per row.
        self.refresh_menus(cx);
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

    /// Text ▸ Unicode Normalizations, for the form the menu named.
    pub fn normalize_unicode(
        &mut self,
        form: Normalization,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.text_op(window, cx, move |text, selection, _| {
            textops::normalize(text, selection, form)
        });
    }

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
        let editor = document.editor().clone();
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

    /// Edit ▸ Substitutions ▸ Straighten Quotes.
    ///
    /// Upstream greys this item out while the selection is empty, even though
    /// its command would work on the word under the cursor, and it says why in
    /// its own source: to keep the item's state the same as *Replace Quotes*,
    /// which does need a selection. A GPUI menu item carries one fixed disabled
    /// flag and no way to be re-asked per selection, and rebuilding the whole
    /// bar on every caret move to keep one item in step is not a trade worth
    /// making — so the item stays live and says what it needs instead. The text
    /// is not touched either way.
    pub fn straighten_quotes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_selection(cx).is_empty() {
            self.set_status("Select something first", cx);
            return;
        }
        self.text_op(window, cx, |text, selection, _| {
            textops::straighten_quotes(text, selection)
        });
    }

    /// The *Custom…* surround sheet, and the command it runs.    ///
    /// CotEditor opens the sheet with the pair last used — `UserDefaults` under
    /// `beginCustomSurroundString` / `endCustomSurroundString` — and wraps the
    /// selection in whatever the sheet ends up holding. The pair is written
    /// back here rather than by the sheet, so the setting and the edit are one
    /// step from the workspace's point of view.
    pub fn custom_surround(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        begin: &str,
        end: &str,
    ) {
        self.settings.custom_surround_begin = begin.to_string();
        self.settings.custom_surround_end = end.to_string();
        let _ = self.settings.save();
        self.surround(window, cx, begin, end);
    }

    /// Wrap the selection in `open`/`close` — CotEditor's *Surround Selection
    /// With* commands, including the one the custom sheet drives.
    ///
    /// The delimiters go around the selection, not around the lines it touches:
    /// upstream's `surroundSelections(begin:end:)` hands
    /// `String.surround(in:begin:end:)` the selected *ranges*. With a bare
    /// caret the selection is an empty range, so the pair is inserted and the
    /// caret is left between its halves.
    pub fn surround(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        open: &str,
        close: &str,
    ) {
        let (open, close) = (open.to_string(), close.to_string());
        self.text_op(window, cx, move |text, selection, _| {
            textops::surround(text, selection, &open, &close)
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
        let end = text[offset..]
            .find('\n')
            .map_or(text.len(), |index| offset + index);
        SharedString::from(text[start..end].to_string())
    }

    /// The *Custom…* surround sheet.
    ///
    /// Upstream's sheet has no title — it is a bare `beginSheet { … }` — so
    /// the title here is SubText's, named after the command that opens it. The
    /// two fields are seeded from the pair last used.
    pub fn surround_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = Workspace::weak(cx);
        let begin = self.settings.custom_surround_begin.clone();
        let end = self.settings.custom_surround_end.clone();
        let sheet = cx.new(|cx| SurroundSheet::new(workspace, &begin, &end, window, cx));
        window.open_dialog(cx, move |dialog, window, _| {
            let submit = sheet.clone();
            let confirm = sheet.clone();
            dialog
                .title("Custom Surround")
                // `Dialog::w` takes pixels, not a `Length`, so the interface
                // scale is applied by hand rather than by the layout engine.
                // 30 rem holds both field pairs side by side: 4 rem of label
                // plus 6 rem of field, twice, plus the gap and the padding.
                .w(window.rem_size() * 32.)
                .child(sheet.clone())
                // A footer, not buttons drawn by the sheet: a dialog without
                // one collapses to `min_h`, and the sheet's own button row was
                // being cut off. Enter reaches the dialog's own actions, so the
                // callbacks are installed there as well.
                .footer(
                    // Two `DialogClose`s, both with custom triggers: in this
                    // version a plain `Button` inside a `DialogFooter` is not
                    // rendered at all, while a `DialogClose` is. The OK trigger
                    // submits on the way out.
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("Cancel")))
                        .child(DialogClose::new().trigger(move |button| {
                            button.label("OK").primary().on_click(move |_, _, cx| {
                                confirm.update(cx, |sheet, cx| sheet.submit(cx));
                            })
                        })),
                )
                .on_ok(move |_, _, cx| {
                    submit.update(cx, |sheet, cx| sheet.submit(cx));
                    true
                })
        });
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
        let (start, end) = (
            selection.start.min(text.len()),
            selection.end.min(text.len()),
        );
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
        let sheet = cx.new(|cx| ColorSheet::new(workspace, &selected, kind, fallback, window, cx));
        window.open_dialog(cx, move |dialog, window, _| {
            let submit = sheet.clone();
            dialog
                .title("Edit Color Code")
                // `Dialog::w` takes pixels, not a `Length`, so the interface
                // scale is applied by hand rather than by the layout engine.
                .w(window.rem_size() * 32.)
                .child(sheet.clone())
                // See the note in `surround_sheet`: without a footer the dialog
                // collapses to its minimum height and cuts the sheet's buttons.
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("Cancel"))),
                )
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
        let editor = document.editor().clone();
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
        let line_ending = self
            .active_document()
            .map(|document| document.document.line_ending());

        let multireplace::Outcome {
            string,
            selected_range,
            count,
        } = definition.replace(&text, selection, in_selection, line_ending);
        if count == 0 {
            self.set_status("No matches", cx);
            return Some(0);
        }

        let editor = self.documents.get(self.active)?.editor().clone();
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
        let end = text[offset..]
            .find('\n')
            .map_or(text.len(), |index| offset + index);
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

    /// ⌃⌘↑ — the outline entry before the caret's, as `Select Previous Outline
    /// Item` does.
    pub fn select_previous_outline_item(&mut self, cx: &mut Context<Self>) {
        self.step_outline_item(false, cx);
    }

    /// ⌃⌘↓ — the outline entry after the caret's.
    pub fn select_next_outline_item(&mut self, cx: &mut Context<Self>) {
        self.step_outline_item(true, cx);
    }

    /// Move the selection onto the neighbouring outline entry.
    ///
    /// CotEditor's `selectPreviousItem()` / `selectNextItem()`, which read the
    /// items off the syntax controller and so work whether or not the Outline
    /// inspector is on screen. `outline_entries` is only as fresh as that
    /// pane's last render, so this parses the outline again rather than
    /// reading it: one pass over the buffer, on a keystroke the user asked for
    /// by hand.
    ///
    /// Finding nothing is an ordinary state rather than a failure — upstream
    /// greys the menu item out, and a GPUI menu item cannot change with the
    /// caret — so the status bar says which way there is nothing to go, and
    /// the selection stays where it was. The text is untouched either way.
    fn step_outline_item(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some((text, _)) = self.outline_input(cx) else {
            return;
        };
        let selection = self.active_selection(cx);
        let entries = match self.active_document() {
            Some(document) => match self.syntaxes.by_name(&document.syntax) {
                Some(syntax) => crate::outline::entries(&text, syntax),
                None => Vec::new(),
            },
            None => return,
        };
        let item = if forward {
            crate::outline::next_item(&entries, &selection)
        } else {
            crate::outline::previous_item(&entries, &selection)
        };
        let Some(range) = item.map(|entry| entry.range.clone()) else {
            self.set_status(
                if forward {
                    "No next outline item"
                } else {
                    "No previous outline item"
                },
                cx,
            );
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
        document.editor().update(cx, |state, cx| {
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
        let editor = document.editor().clone();
        let text = text.to_string();
        // Deferred for the same reason the text operations are: `replace`
        // emits `Change`, and the subscription that follows the buffer back
        // into the document model must not run inside this update.
        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| state.replace(text, window, cx));
        });
    }

    // ---- snippets ---------------------------------------------------------

    /// Insert the snippet with this id over the selection.
    ///
    /// CotEditor's `insertSnippet(_:)`: expand the snippet against the selected
    /// text and the indentation of the line the selection starts on, replace
    /// the selection with the result, and leave the caret where the format's
    /// `<<<CURSOR>>>` token was. Returns whether a snippet with that id exists,
    /// which is what tells the menu dispatch that the command was recognised.
    ///
    /// The expansion itself is [`subtext_core::Snippet::insertion`], so the two
    /// subtleties — indent before splitting, split before substituting — are
    /// the ones the unit tests in `subtext-core` hold down.
    pub fn insert_snippet(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(snippet) = self
            .settings
            .snippets
            .iter()
            .find(|snippet| snippet.id == id)
        else {
            return false;
        };
        let Some(document) = self.documents.get(self.active) else {
            return false;
        };
        let editor = document.editor().clone();

        // One read of the buffer answers both questions the expansion asks, and
        // the buffer is the whole document: `EditorState::value` materialises
        // it, so asking twice would copy it twice.
        let (selected, start, indent) = {
            let state = editor.read(cx);
            let text = state.value().to_string();
            let selection = state.selected_range();
            let selected = text.get(selection.clone()).unwrap_or_default().to_string();
            let indent = subtext_core::snippets::indent_at(&text, selection.start).to_string();
            (selected, selection.start, indent)
        };

        let insertion = snippet.insertion(&selected, &indent);
        let caret = start + insertion.caret();
        let text = insertion.text;

        // Deferred for the same reason the text operations are: `replace` emits
        // `Change`, and the subscription that follows the buffer back into the
        // document model must not run inside this update. `replace` rather than
        // `set_value`, so ⌘Z undoes a snippet the way it undoes a keystroke.
        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| {
                state.replace(text, window, cx);
                state.set_selected_range(caret..caret, cx);
            });
        });
        true
    }

    /// Rebuild the menu bar, if anything it is built from has moved.
    ///
    /// Cheap to call from anywhere a document is opened, closed or switched: the
    /// comparison is what makes it a no-op the rest of the time, and a rebuild is
    /// a full `App::set_menus` over the whole bar.
    ///
    /// The tables — the syntax list, the theme list, the snippets — are not part
    /// of the comparison. They change only when the settings do, which is
    /// [`rebuild_menus`](Self::rebuild_menus).
    pub fn refresh_menus(&mut self, cx: &mut Context<Self>) {
        // Read before comparing. The Script menu is built from a folder the user
        // can add to at any time and there is no event to wait for, so one
        // `read_dir` per refresh is what notices a script that appeared or went;
        // the comparison is what keeps a folder that has not changed from
        // rebuilding the bar.
        let scripts = scripts::scan();
        let signature = menus::MenuSignature::of(
            self.menu_document(),
            self.theme.name.clone(),
            scripts.clone(),
            self.settings.recent_documents.clone(),
            self.menu_selection_empty,
        );
        if self.menus_built_for.as_ref() == Some(&signature) {
            return;
        }
        self.scripts = scripts;
        self.menus_built_for = Some(signature);
        crate::menus::install(cx, &self.menu_state());
    }

    /// Keep the menu bar's selection-dependent items in step with the caret.
    ///
    /// The bar is a tree that is set once, so `Straighten Quotes` is built
    /// either greyed or live. Rebuilding re-reads the script folder, so this
    /// compares first: one boolean per editor repaint, and a rebuild only when
    /// the answer flips.
    fn sync_menu_gates(&mut self, cx: &mut Context<Self>) {
        let empty = self.active_selection(cx).is_empty();
        if empty == self.menu_selection_empty {
            return;
        }
        self.menu_selection_empty = empty;
        self.refresh_menus(cx);
    }

    /// Rebuild the menu bar and the key bindings unconditionally.
    ///
    /// The settings pane calls this after the snippet list is edited, where the
    /// document in front has not moved but the items under it have.
    pub fn rebuild_menus(&mut self, cx: &mut Context<Self>) {
        self.scripts = scripts::scan();
        let state = self.menu_state();
        self.menus_built_for = Some(state.signature());
        crate::menus::install(cx, &state);
    }

    /// Everything the menu bar is built from, as it is right now.
    fn menu_state(&self) -> menus::MenuState {
        menus::MenuState {
            document: self.menu_document(),
            theme: self.theme.name.clone(),
            syntaxes: self.syntax_names(),
            themes: self.theme_names(),
            encoding_list: self.settings.encoding_list.clone(),
            snippets: self.settings.snippets.clone(),
            scripts: self.scripts.clone(),
            recent_documents: self.settings.recent_documents.clone(),
            gates: menus::MenuGates {
                selection_empty: self.menu_selection_empty,
            },
        }
    }

    /// The frontmost document, as the menu bar needs to see it.
    fn menu_document(&self) -> Option<menus::DocumentState> {
        let document = self.active_document()?;
        Some(menus::DocumentState {
            syntax: document.syntax.to_string(),
            line_ending: document.document.line_ending(),
            // The pair, not the codec: a UTF-8 document with a byte-order mark
            // is not in the same encoding as one without, and upstream checks
            // one menu item for each.
            encoding: FileEncoding::of_document(
                document.document.encoding(),
                document.document.bom(),
            ),
        })
    }

    // ---- file drop --------------------------------------------------------

    /// Insert the text the dropped files stand for.
    ///
    /// CotEditor's `editorTextView(_:readDroppedURLs:)`, in the same order: the
    /// first file-drop item that answers to a dropped file's extension *and* to
    /// the document's syntax expands it, and a file no item answers to
    /// contributes its own absolute path — which is what a plain text view
    /// would have inserted, and what upstream falls back to.
    ///
    /// Returns whether the drop was handled at all, which is what upstream's
    /// delegate answer means. It is `true` for a drop that expands to nothing,
    /// so that a file the user deliberately described as empty does not fall
    /// through to the platform's own behaviour.
    ///
    /// Two things upstream does are absent here. A macOS `.textClipping` file
    /// is unpacked by AppKit and has no counterpart on the other two platforms,
    /// so it is not special-cased. And a dropped web address arrives as a
    /// `PathBuf`, because that is what GPUI hands over, so it is treated as the
    /// text of its own path rather than as a URL.
    pub fn drop_files(
        &mut self,
        paths: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if paths.is_empty() || self.settings.file_drop.is_empty() {
            return false;
        }
        let Some(document) = self.documents.get(self.active) else {
            return false;
        };
        let editor = document.editor().clone();
        let scope = document.syntax.to_string();
        let document_path = document.document.path().map(Path::to_path_buf);
        let ending = document.document.line_ending();

        let mut text = String::new();
        for path in paths {
            let extension = path
                .extension()
                .map(|part| part.to_string_lossy().into_owned());
            let item = self
                .settings
                .file_drop
                .iter()
                .find(|item| item.supports(extension.as_deref(), Some(&scope)));

            let Some(item) = item else {
                if !text.is_empty() {
                    text.push_str(ending.as_str());
                }
                text.push_str(&path.to_string_lossy());
                continue;
            };

            // The two things that need the file itself are read only when the
            // format asks for them, so dropping a folder of pictures into a
            // Markdown document does not read any of them.
            let mut file = DroppedFile::new(path, document_path.as_deref());
            if item.needs_content() {
                file = file.with_content(std::fs::read_to_string(path).ok());
            }
            if item.needs_image_size() {
                file = file.with_image_size(imagesize::read(path));
            }
            text.push_str(&item.drop_text(&file));
        }

        if text.is_empty() {
            return true;
        }

        // The format's own line breaks are the user's; which of them the file
        // uses is the document's.
        let text = line_ending::convert(&text, ending);
        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| state.replace(text, window, cx));
        });
        true
    }

    fn set_selection(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        document
            .editor()
            .update(cx, |state, cx| state.set_selected_range(range, cx));
        cx.notify();
    }

    /// Scroll the active editor so the caret sits in the middle of the view.
    ///
    /// Upstream's `centerSelectionInVisibleArea`. `EditorState` has no such
    /// call, but it does expose the caret's geometry from the last prepaint
    /// (`prepaint_caret_geometry`) and a public `set_scroll_offset` — which is
    /// enough to do it by hand: move the viewport by however far the caret is
    /// from the middle.
    ///
    /// The offset is applied after the next layout, so this reads the geometry
    /// of the frame that is on screen — a caller that has *just* moved the caret
    /// would centre where it used to be.
    pub fn center_selection(&mut self, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let editor = document.editor().clone();
        editor.update(cx, |state, cx| {
            let Some(geometry) = state.prepaint_caret_geometry() else {
                return;
            };
            let caret = geometry.cursor_bounds();
            let viewport = geometry.input_bounds();
            if viewport.size.height <= px(0.) {
                return;
            }
            let centre = viewport.origin.y + viewport.size.height / 2.0;
            // `cursor_bounds` is measured against the content, not the window —
            // it does not move when the view scrolls — so putting the caret in
            // the middle is one subtraction: where the middle is, minus where
            // the caret is.
            let target = centre - caret.origin.y;
            let offset = state.scroll_offset();
            if (offset.y - target).abs() < px(0.5) {
                return;
            }
            state.set_scroll_offset(gpui_kit::point(offset.x, target), cx);
        });
    }

    /// Centre the caret once its geometry reflects a selection that was just
    /// set.
    ///
    /// [`center_selection`](Self::center_selection) reads the caret out of
    /// `prepaint_caret_geometry`, and that is filled in during a frame's
    /// prepaint — so it always describes the frame that is *already on screen*.
    /// Right after a `set_selection` the new caret is two frames away from
    /// being readable:
    ///
    /// 1. the notification schedules a frame — call it N — whose prepaint picks
    ///    up the new caret;
    /// 2. an `on_next_frame` callback runs *before* the frame it was registered
    ///    for, so the first one still sees the caret from before the selection;
    /// 3. the second one runs before N+1, by which point N has been prepared.
    ///
    /// Measured, not guessed: with one hop the offset landed on `0` — the
    /// geometry of the caret at the top of the document — and the view snapped
    /// back to the first line. With two it lands on the caret's own row. See
    /// §22.10 in `docs/PLAN.md`.
    fn center_selection_when_settled(&self, window: &mut Window, cx: &mut Context<Self>) {
        cx.on_next_frame(window, |_this, window, cx| {
            cx.on_next_frame(window, |this, _window, cx| this.center_selection(cx));
        });
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
                    weak.update(cx, |workspace, cx| workspace.goto_line(line, cx))
                        .ok();
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

    /// The find panel's two fields.
    ///
    /// The panel reads them rather than owning them: every find command in the
    /// menu works on the same query, and two `InputState`s would be two answers
    /// to "what is being searched for".
    pub fn find_fields(&self) -> (Entity<InputState>, Entity<InputState>) {
        (self.find_query.clone(), self.find_replacement.clone())
    }

    /// What the last search found: how many matches, which one is current, and
    /// the error if the query could not be compiled.
    pub fn find_status(&self) -> (usize, usize, Option<String>) {
        (
            self.find.matches.len(),
            self.find.current.map_or(0, |index| index + 1),
            self.find.error.clone(),
        )
    }

    /// The document window, for the views that live in a window of their own.
    pub fn document_window(&self) -> Option<AnyWindowHandle> {
        self.find_window_owner
    }

    /// `Edit ▸ Find ▸ Find…` (⌘F) and *Find and Replace…* (⌥⌘F).
    ///
    /// Upstream's `FindPanelController` is an `NSPanel` — a floating utility
    /// window titled *Find & Replace* — rather than a bar in the document
    /// window, and it is shared: a second ⌘F brings the same panel forward
    /// instead of opening a second one. The panel shows both fields either
    /// way, so the two commands differ only in which field the caret lands in.
    pub fn show_find(&mut self, show_replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        let owner = window.window_handle();
        if let Some(handle) = self.find_window {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
                self.find_window_owner = Some(owner);
                self.remember_find_frame(cx);
                // `refresh_matches` drops the previous Find All's rows, so a
                // panel that was left open with the result table showing comes
                // back collapsed and at the collapsed height.
                self.refresh_matches(cx);
                self.resize_find_panel(cx);
                self.focus_find_field(handle, show_replace, cx);
                return;
            }
            self.find_window = None;
        }

        self.find_window_owner = Some(owner);
        self.refresh_matches(cx);

        let weak = cx.entity().downgrade();
        // The frame is read before the defer: the closure must not borrow
        // `self`, and the value cannot change between here and the next tick
        // in any way the panel did not already account for.
        let saved_frame = self.settings.find_panel_frame;
        cx.defer(move |cx| {
            if weak.upgrade().is_none() {
                return;
            }
            // Reopen at the frame the user left the panel at — upstream's
            // `windowFrameAutosaveName`. A stored frame without a screen to
            // hold it is the platform's problem there; GPUI clamps a windowed
            // frame on its own as well.
            let window_bounds = match saved_frame {
                Some([x, y, width, height]) => WindowBounds::Windowed(gpui_kit::Bounds {
                    origin: gpui_kit::point(px(x), px(y)),
                    size: gpui_kit::size(px(width), px(height)),
                }),
                None => WindowBounds::centered(
                    gpui_kit::size(px(FIND_PANEL_WIDTH), px(FIND_PANEL_HEIGHT)),
                    cx,
                ),
            };
            let options = WindowOptions {
                // A utility window is a short column of rows, so it opens at
                // the size those rows need rather than filling the screen, and
                // the minimum is the width below which the button row wraps.
                window_bounds: Some(window_bounds),
                window_min_size: Some(gpui_kit::size(px(FIND_PANEL_MIN_WIDTH), px(150.))),
                ..Default::default()
            };
            let panel = weak.clone();
            let focus = weak.clone();
            let opened = gpui_kit::open_window(options, cx, move |window, cx| {
                let view = cx.new(|cx| FindPanel::new(panel, window, cx));
                // Upstream makes the find text view the panel's initial first
                // responder, so the caret is in the field the moment the panel
                // appears. It has to be done *here*: the panel's window is the
                // only one that can hold a handle it draws.
                focus
                    .update(cx, |workspace, cx| {
                        workspace.focus_find_field_in(window, show_replace, cx);
                    })
                    .ok();
                view
            });
            weak.update(cx, |workspace, cx| match opened {
                Ok((handle, _)) => {
                    workspace.find_window = Some(handle);
                    // The panel opens at the collapsed height, but a *Find All*
                    // can run before this window exists — the window is created
                    // in this very defer — so the size the results ask for is
                    // applied here rather than only where the results change.
                    workspace.resize_find_panel(cx);
                }
                Err(error) => workspace.set_status(error.to_string(), cx),
            })
            .ok();
        });
    }

    /// Put the caret in one of the panel's two fields, in the panel's window.
    ///
    /// A focus handle belongs to the window that focused it. Focusing the query
    /// from the document window — which is where every find command is invoked
    /// from — would leave the panel's field unfocused and the document window
    /// holding a handle it does not draw.
    fn focus_find_field(&self, panel: AnyWindowHandle, show_replace: bool, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        panel
            .update(cx, move |_, window, cx| {
                weak.update(cx, |workspace, cx| {
                    workspace.focus_find_field_in(window, show_replace, cx);
                })
                .ok();
            })
            .ok();
    }

    /// Put the caret in one of the panel's two fields, given the panel's window.
    fn focus_find_field_in(&self, window: &mut Window, show_replace: bool, cx: &mut Context<Self>) {
        let field = if show_replace {
            self.find_replacement.clone()
        } else {
            self.find_query.clone()
        };
        field.update(cx, |state, cx| state.focus_handle(cx).focus(window, cx));
    }

    /// Run the current query and remember where the matches are.
    ///
    /// The result rows are dropped rather than rebuilt: they belong to one
    /// *Find All* pass, and a stale row would point at a range the new query
    /// never produced. Upstream collapses the result view on the same events.
    pub fn refresh_matches(&mut self, cx: &mut Context<Self>) {
        let was_open = self.find.results_open;
        self.find.results.clear();
        self.find.capture_matches.clear();
        // The rows are about to be rebuilt, so no selection can survive: the
        // same pass upstream prunes stale ids with. The replace message also
        // dies with its query, upstream's `onChange(of: findString)`.
        self.find_selected_ranges.clear();
        self.find_selection_anchor = None;
        self.find.replace_result = None;
        self.find.results_open = false;
        if was_open {
            self.resize_find_panel(cx);
        }
        self.run_search(cx);
    }

    /// Search again without touching the *Find All* result table — upstream's
    /// `incrementalSearch`. It updates the match state only: the rows belong
    /// to the last *Find All* pass and keep showing it.
    ///
    /// Upstream's `shouldSearchIncrementally` gates this the same way: the
    /// setting is on, the search is not scoped to the selection, and the query
    /// is neither empty nor (for a regex) invalid.
    pub fn incremental_search(&mut self, cx: &mut Context<Self>) {
        // Every keystroke restarts the timer, and a query that cannot be
        // searched cancels it outright — upstream's `findTask?.cancel()`.
        self.incremental_auto = None;
        if !self.incremental_search_applies(cx) {
            return;
        }
        self.incremental_auto = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(INCREMENTAL_SEARCH_DELAY)
                .await;
            this.update(cx, |workspace, cx| {
                if workspace.incremental_search_applies(cx) {
                    workspace.run_incremental_search(cx);
                }
            })
            .ok();
        }));
    }

    /// The guards upstream applies before an incremental pass.
    fn incremental_search_applies(&self, cx: &App) -> bool {
        if !self.settings.find_searches_incrementally || self.settings.find_in_selection {
            return false;
        }
        let query = self.query(cx);
        if query.is_empty() {
            return false;
        }
        // An invalid regex would only clear the state on every keystroke;
        // upstream's guard skips the search instead.
        if self.settings.find_regex && finder::Searcher::new(&query, &self.find_options()).is_err()
        {
            return false;
        }
        true
    }

    /// One incremental pass — upstream's `find(forward: true, isIncremental: true)`.
    ///
    /// It searches, moves the caret to the match at or after it, and flashes
    /// that match. The *current match* index stays unset: upstream passes
    /// `matchedRange = nil` for an incremental pass, so the panel keeps
    /// reporting the count while the caret moves.
    fn run_incremental_search(&mut self, cx: &mut Context<Self>) {
        self.run_search(cx);
        // Upstream marks every match while the query is being typed:
        // `client.updateBackgroundColor(.unemphasizedSelectedTextBackgroundColor,
        // ranges: matches)`. The marks go away with the panel (see the
        // window-closed observer) or when the query is emptied.
        let matches = self.find.matches.clone();
        self.set_editor_marks(&matches, cx);
        let selection = self.active_selection(cx);
        // A match starting at the caret counts as the one to show — upstream's
        // `includingSelection`, which is what keeps typing on the same match
        // instead of skipping past it.
        let mut next = self
            .find
            .matches
            .iter()
            .find(|range| range.start >= selection.start)
            .cloned();
        if next.is_none() && self.settings.find_wrap_around {
            next = self.find.matches.first().cloned();
        }
        let Some(range) = next else {
            return;
        };
        self.set_selection(range.clone(), cx);
        self.flash_find_indicator(range, cx);
    }

    /// The search itself, shared by the full refresh and the incremental one.
    fn run_search(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.active_text(cx) else {
            return;
        };
        let query = self.query(cx);
        if query.is_empty() {
            self.find.matches.clear();
            self.find.capture_matches.clear();
            self.find.current = None;
            self.find.error = None;
            self.clear_editor_marks(cx);
            cx.notify();
            return;
        }
        // *In Selection* narrows the scope rather than the pattern: the
        // matches come back as offsets into the whole buffer either way, so
        // nothing downstream has to know the search was bounded.
        let selection = self.active_selection(cx);
        let scope = if self.settings.find_in_selection && !selection.is_empty() {
            selection
        } else {
            0..text.len()
        };
        match finder::Searcher::new(&query, &self.find_options()) {
            Ok(searcher) => {
                self.find.capture_matches = searcher.find_all_with_captures(&text, &scope);
                self.find.matches = self
                    .find
                    .capture_matches
                    .iter()
                    .map(|captures| captures[0].clone().expect("group 0 always participates"))
                    .collect();
                self.find.error = None;
                if self
                    .find
                    .current
                    .is_some_and(|index| index >= self.find.matches.len())
                {
                    self.find.current = None;
                }
            }
            Err(error) => {
                self.find.matches.clear();
                self.find.capture_matches.clear();
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
        self.set_selection(range.clone(), cx);
        self.flash_find_indicator(range, cx);
    }

    /// Flash a range the way `NSTextView.showFindIndicator(for:)` does.
    ///
    /// Upstream calls this from four places: after a successful find step,
    /// after a click in the *Find All* list, after a folder-find hit, and for
    /// each matched bracket when the caret jumps to its pair. The mark is a
    /// background behind the glyphs, so the text keeps its own colour.
    fn flash_find_indicator(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        // A second find inside the first one's lifetime must not have its mark
        // faded out by the older timer, so each flash takes a number and only
        // the holder of the current one touches the decoration.
        self.find_indicator = self.find_indicator.wrapping_add(1);
        let generation = self.find_indicator;
        self.paint_find_indicator(&range, 1.0, cx);

        cx.spawn(async move |this, cx| {
            for factor in FIND_INDICATOR_FADE {
                cx.background_executor().timer(FIND_INDICATOR_STEP).await;
                let alive = this
                    .update(cx, |workspace, cx| {
                        if workspace.find_indicator != generation {
                            return;
                        }
                        workspace.paint_find_indicator(&range, factor, cx);
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    /// Paint the indicator at `factor` of its full strength.
    ///
    /// `0.0` clears it, which is the last fade step. The decoration lives on the
    /// pane — so a split pane only marks its own — and every step replaces the
    /// range's style rather than adding to it.
    fn paint_find_indicator(&mut self, range: &Range<usize>, factor: f32, cx: &mut Context<Self>) {
        let Some(indicator) = self
            .documents
            .get(self.active)
            .map(|document| document.panes[document.active_pane].indicator.clone())
        else {
            return;
        };
        if factor <= 0. {
            indicator.clear(cx);
            return;
        }
        let style = gpui_kit::HighlightStyle {
            background_color: Some(cx.theme().yellow.opacity(FIND_INDICATOR_OPACITY * factor)),
            ..Default::default()
        };
        indicator.set(
            vec![gpui_kit::base::input::TextDecoration::new(
                range.clone(),
                style,
            )],
            cx,
        );
    }

    /// Flash the bracket that pairs with the one just typed.
    ///
    /// Upstream's `highlightMatchingSymbol`, which runs on every selection
    /// change while `highlightsBraces` is on. The editor reports *edits*, not
    /// caret moves, so this fires when the text changes and the caret then sits
    /// right after a bracket — which is the case the feature is for: typing
    /// `(` leaves the caret inside the pair, and the `)` lights up.
    ///
    /// Only `()`, `{}` and `[]` are considered, as upstream does here; quotes
    /// need the double-delimiter scan and a syntax of their own, and that path
    /// upstream feeds a different candidate list.
    fn flash_matching_bracket(&mut self, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let selection = document.editor().read(cx).selected_range();

        // Answering "did the caret move at all" is one range compare; a repaint
        // that did not move it — the blink, a scroll — stops here and never
        // scans for a pair.
        if self.matching_bracket_selection.as_ref() == Some(&selection) {
            return;
        }
        self.matching_bracket_selection = Some(selection.clone());

        let pair = self.matching_bracket_at_caret(&selection, cx);
        // A caret that walks along the same pair — one arrow key at a time past
        // `foo(bar)` — must not restart the fade on every step. Leaving the pair
        // clears the record, so coming back to it flashes again.
        if self.matching_bracket == pair {
            return;
        }
        self.matching_bracket = pair.clone();
        if let Some(range) = pair {
            self.flash_find_indicator(range, cx);
        }
    }

    /// The pair a collapsed caret sits beside, if it sits beside one.
    ///
    /// Read straight off the editor rather than through `active_text`, which
    /// copies the buffer.
    fn matching_bracket_at_caret(
        &self,
        selection: &std::ops::Range<usize>,
        cx: &App,
    ) -> Option<std::ops::Range<usize>> {
        if !selection.is_empty() || selection.start == 0 {
            return None;
        }
        let document = self.documents.get(self.active)?;
        let state = document.editor().read(cx);
        let text = state.value();
        let caret = selection.start;
        if caret > text.len() {
            return None;
        }
        let (start, _) = text[..caret].char_indices().next_back()?;
        textops::matching_symbol_pair(&text, start, &textops::SymbolPair::BRACES)
    }

    /// The active pane's marks collection.
    fn marks_collection(&self) -> Option<gpui_kit::base::input::TextDecorationCollection> {
        self.documents
            .get(self.active)
            .map(|document| document.panes[document.active_pane].marks.clone())
    }

    /// Paint `ranges` with the unemphasized-selection tint.
    ///
    /// This is upstream's `updateBackgroundColor(.unemphasizedSelectedText‌BackgroundColor,
    /// ranges:)` — the grey macOS puts behind every match while an incremental
    /// search is running, and behind every character the warnings inspector
    /// lists. The tint is the theme's selection colour used as a **text
    /// background**, so the glyphs keep their own colour.
    fn set_editor_marks(&mut self, ranges: &[Range<usize>], cx: &mut Context<Self>) {
        let style = gpui_kit::HighlightStyle {
            background_color: Some(cx.theme().muted),
            ..Default::default()
        };
        let Some(marks) = self.marks_collection() else {
            return;
        };
        marks.set(
            ranges
                .iter()
                .cloned()
                .map(|range| gpui_kit::base::input::TextDecoration::new(range, style))
                .collect(),
            cx,
        );
    }

    /// Drop the marks, if there are any.
    fn clear_editor_marks(&mut self, cx: &mut Context<Self>) {
        if let Some(marks) = self.marks_collection() {
            marks.clear(cx);
        }
    }

    /// Step through the matches, wrapping at either end when the settings say
    /// to — CotEditor's default is to wrap.
    pub fn find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        // Every find command lands the query in the history, matched or not —
        // upstream notes it after the beep as well; only the empty string is
        // skipped, which the append guard handles.
        self.note_find_history(cx);
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

    /// `Find All ▸ Select All` — put every match into the editor's selections.
    ///
    /// Upstream's `selectAllMatches`: it re-runs the search, assigns every
    /// matched range as a selection, and reports the count. The editor has
    /// supported multiple selections all along (`add_selection`); what was
    /// missing was a command that fills them, which is why this item spent its
    /// first rounds greyed out.
    pub fn select_all_find_matches(&mut self, cx: &mut Context<Self>) {
        self.run_search(cx);
        let matches = self.find.matches.clone();
        if matches.is_empty() {
            self.set_status("No matches", cx);
            return;
        }
        let count = matches.len();
        let Some(document) = self.active_document() else {
            return;
        };
        let editor = document.editor().clone();
        editor.update(cx, |state, cx| {
            // The first one replaces whatever was selected; the rest are added,
            // which is how the editor models several ranges at once.
            state.set_selected_range(matches[0].clone(), cx);
            for range in &matches[1..] {
                state.add_selection(range.clone(), cx);
            }
        });
        self.note_find_history(cx);
        self.set_status(format!("{count} match(es) selected"), cx);
    }

    /// Select every match, which is what "Find All" does in CotEditor.
    ///
    /// It is also the only command that fills the result table, and the only
    /// one that opens the middle section of the panel — upstream collapses it
    /// again as soon as the panel closes, and leaves it collapsed when nothing
    /// was found.
    pub fn find_all(&mut self, cx: &mut Context<Self>) {
        self.refresh_matches(cx);
        let count = self.find.matches.len();
        if count == 0 {
            self.set_status("No matches", cx);
            return;
        }
        if let Some(text) = self.active_text(cx) {
            // The rows carry their capture-group ranges: the table paints each
            // group with the decomposed highlighter colours, group 0 included,
            // exactly as upstream builds the attributed line string.
            self.find.results =
                findresult::match_lines_with_captures(&text, &self.find.capture_matches);
            self.find.results_open = true;
            self.resize_find_panel(cx);
        }
        self.note_find_history(cx);
        self.set_status(format!("{count} match(es)"), cx);
    }

    /// Collapse the result section — the panel's `⌃` button.
    pub fn close_find_results(&mut self, cx: &mut Context<Self>) {
        self.find.results_open = false;
        self.resize_find_panel(cx);
        cx.notify();
    }

    /// Grow or shrink the panel to fit the result section.
    ///
    /// Upstream's result view is a split item that opens to 200 points, which
    /// makes the panel taller; the panel is resizable there and the user can
    /// drag it back. Here the window owns its own size, so opening and closing
    /// the section adds and removes the same 200 points.
    fn resize_find_panel(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.find_window else {
            return;
        };
        let height = FIND_PANEL_HEIGHT
            + if self.find.results_open {
                FIND_PANEL_RESULT_HEIGHT
            } else {
                0.0
            };
        handle
            .update(cx, |_, window, _| {
                // Keep whatever width the user has dragged the panel to; only
                // the height is ours to decide.
                let width = window.bounds().size.width;
                window.resize(gpui_kit::size(width, px(height)));
            })
            .ok();
        self.remember_find_frame(cx);
    }

    /// Save the panel's current frame for the next opening — the write side of
    /// upstream's `windowFrameAutosaveName`, which macOS updates whenever the
    /// window moves or resizes. SubText has no such callback, so the frame is
    /// remembered at the moments the workspace touches the panel: showing and
    /// resizing it.
    fn remember_find_frame(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.find_window else {
            return;
        };
        if let Ok(frame) = handle.update(cx, |_, window, _| {
            let bounds = window.bounds();
            [
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            ]
        }) {
            self.settings.find_panel_frame = Some(frame);
            let _ = self.settings.save();
        }
    }

    /// Apply a click on the result row whose match is `range`.
    ///
    /// This is upstream's `Table(selection:)` behaviour: a plain click selects
    /// that one row, ⌘-click toggles it in the set, and ⇧-click selects the
    /// display-order span from the anchor row. With exactly one row selected
    /// the editor's selection moves to the match, the way upstream's
    /// `onChange(of: selection)` calls `selectMatch` only for a single row.
    pub fn select_find_result_range(
        &mut self,
        range: Range<usize>,
        modifiers: gpui_kit::Modifiers,
        cx: &mut Context<Self>,
    ) {
        let visible = self
            .find
            .results
            .iter()
            .map(|row| row.range.clone())
            .collect::<Vec<_>>();
        let Some(target) = visible.iter().position(|row| *row == range) else {
            return;
        };
        let (selected, anchor) = apply_row_selection(
            &self.find_selected_ranges,
            self.find_selection_anchor
                .as_ref()
                .and_then(|anchor| visible.iter().position(|row| *row == *anchor)),
            target,
            modifiers.shift,
            modifiers.platform || modifiers.control,
            &visible,
        );
        self.find_selected_ranges = selected;
        self.find_selection_anchor = anchor.and_then(|index| visible.get(index).cloned());
        // With exactly one row selected the editor's selection moves to the
        // match, the way upstream's `onChange(of: selection)` does.
        if let [only] = &self.find_selected_ranges[..]
            && let Some(index) = self.find.matches.iter().position(|match_| match_ == only)
        {
            self.select_match(index, cx);
        }
        cx.notify();
    }

    /// Select a result row by its display position — the keyboard path.
    pub fn select_find_result_index(
        &mut self,
        row_ix: usize,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(range) = self.find.results.get(row_ix).map(|row| row.range.clone()) else {
            return;
        };
        self.select_find_result_range(
            range,
            gpui_kit::Modifiers {
                shift: extend,
                ..Default::default()
            },
            cx,
        );
    }

    /// Sort the result rows by their `Line` column — upstream's
    /// `KeyPathComparator(\.range.location)` — re-ordering the rows in place so
    /// this list stays the one display order the table and *Copy* both read.
    pub fn sort_find_results(&mut self, descending: bool, cx: &mut Context<Self>) {
        if descending {
            self.find
                .results
                .sort_by_key(|row| std::cmp::Reverse(row.range.start));
        } else {
            self.find.results.sort_by_key(|row| row.range.start);
        }
        cx.notify();
    }

    /// Copy the selected rows' line fragments, newline-separated, in display
    /// order — upstream's `.copyable(matches.filter(with: selection))`.
    pub fn copy_find_results(&self, cx: &mut Context<Self>) {
        let lines = self
            .find
            .results
            .iter()
            .filter(|row| self.find_selected_ranges.contains(&row.range))
            .map(|row| row.line.clone())
            .collect::<Vec<_>>();
        if lines.is_empty() {
            return;
        }
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(lines.join("\n")));
    }

    /// The last replace action's outcome — the count and the replacement text
    /// it ran with — for the panel's replacement-field message.
    pub fn replace_result_message(&self) -> Option<(usize, String)> {
        self.find.replace_result.clone()
    }

    /// Whether the result table is the keyboard focus target.
    pub fn find_results_focused(&self) -> bool {
        self.find_results_focused
    }

    /// Record whether the result table has the keyboard focus.
    ///
    /// The panel's focus listeners keep this current; it only routes commands,
    /// so a change that nobody observed is not worth a redraw.
    pub fn focus_find_results(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.find_results_focused != focused {
            self.find_results_focused = focused;
            cx.notify();
        }
    }

    /// The result-table rows the user has selected.
    pub fn selected_find_result_ranges(&self) -> &[Range<usize>] {
        &self.find_selected_ranges
    }

    /// Select the match a result row stands for.
    ///
    /// The row's `range` is the match itself, so this is the same selection the
    /// editor's own *Find Next* would make.
    pub fn select_find_result(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.select_find_result_range(range, gpui_kit::Modifiers::default(), cx);
    }

    /// The result rows and whether the section is open, for the panel.
    pub fn find_results(&self) -> (&[MatchLine], bool) {
        (&self.find.results, self.find.results_open)
    }

    /// The active document's display name, for the result table's header.
    pub fn document_name(&self) -> String {
        self.documents[self.active_index()].document.display_name()
    }

    /// The result table's text size, in points.
    pub fn find_result_font_size(&self) -> f32 {
        self.settings.find_result_font_size
    }

    /// *Bigger* / *Smaller* from the result table's context menu.
    ///
    /// Upstream steps by one point and stops *Smaller* at the default — the
    /// table may be enlarged past it but not shrunk below it.
    pub fn step_find_result_font_size(&mut self, bigger: bool, cx: &mut Context<Self>) {
        let current = self.settings.find_result_font_size;
        let size = if bigger {
            current + 1.0
        } else {
            (current - 1.0).max(DEFAULT_FIND_RESULT_FONT_SIZE)
        };
        if size != current {
            self.settings.find_result_font_size = size;
            let _ = self.settings.save();
        }
        cx.notify();
    }

    /// *Reset to Default* — upstream's `UserDefaults.restore(key:)`.
    pub fn reset_find_result_font_size(&mut self, cx: &mut Context<Self>) {
        self.settings.find_result_font_size = DEFAULT_FIND_RESULT_FONT_SIZE;
        let _ = self.settings.save();
        cx.notify();
    }

    /// Highlight All and Find All are the same query; the difference in
    /// CotEditor is that Highlight All paints without selecting.
    pub fn highlight_all(&mut self, cx: &mut Context<Self>) {
        self.find_all(cx);
    }

    /// Record the current find string in the recent-searches history —
    /// upstream's `noteFindHistory`, which every find action calls (the
    /// incremental ones aside, which SubText does not have). An empty query is
    /// never recorded, upstream's `appendHistory` guard.
    fn note_find_history(&mut self, cx: &mut Context<Self>) {
        let value = self.query(cx).to_string();
        if value.is_empty() {
            return;
        }
        self.settings.find_history = crate::find_panel::append_unique_history(
            std::mem::take(&mut self.settings.find_history),
            value,
        );
        let _ = self.settings.save();
    }

    /// Record the current find **and** replacement strings — upstream's
    /// `noteReplaceHistory`, which every replace action calls.
    fn note_replace_history(&mut self, cx: &mut Context<Self>) {
        let find = self.query(cx).to_string();
        let replace = self.replacement(cx).to_string();
        if !find.is_empty() {
            self.settings.find_history = crate::find_panel::append_unique_history(
                std::mem::take(&mut self.settings.find_history),
                find,
            );
        }
        if !replace.is_empty() {
            self.settings.replace_history = crate::find_panel::append_unique_history(
                std::mem::take(&mut self.settings.replace_history),
                replace,
            );
        }
        let _ = self.settings.save();
    }

    /// Empty one of the histories — the menus' *Clear Recent …* item.
    pub fn clear_find_history(&mut self, cx: &mut Context<Self>) {
        self.settings.find_history.clear();
        let _ = self.settings.save();
        cx.notify();
    }

    /// Empty the replacement history — the replacement menu's *Clear* item.
    pub fn clear_replace_history(&mut self, cx: &mut Context<Self>) {
        self.settings.replace_history.clear();
        let _ = self.settings.save();
        cx.notify();
    }

    /// Fill one of the two fields from a history menu, **without** starting a
    /// search — upstream writes the binding, and a binding write does not run
    /// the text view's action.
    pub fn set_find_field(
        &mut self,
        field: crate::find_panel::FindField,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match field {
            crate::find_panel::FindField::Query => {
                self.find_query
                    .update(cx, |state, cx| state.set_value(&value, window, cx));
            }
            crate::find_panel::FindField::Replacement => {
                self.find_replacement
                    .update(cx, |state, cx| state.set_value(&value, window, cx));
            }
        }
        cx.notify();
    }

    pub fn clear_highlights(&mut self, cx: &mut Context<Self>) {
        self.find.matches.clear();
        self.find.current = None;
        // The result rows stand for one Find All pass over those matches, so
        // removing the highlights removes the list as well.
        self.find.results.clear();
        if self.find.results_open {
            self.find.results_open = false;
            self.resize_find_panel(cx);
        }
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
        // ⌘E puts the selection in the query *and* brings the panel up, which
        // is what upstream's `useSelectionForFind` does.
        self.show_find(false, window, cx);
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
        self.show_find(true, window, cx);
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
        let replacement =
            if self.settings.find_regex && self.settings.find_regex_unescapes_replacement {
                finder::unescape(&self.replacement(cx))
            } else {
                self.replacement(cx)
            };
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
        let editor = document.editor().clone();
        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| {
                state.replace_all(replaced, window, cx);
                state.set_selected_range(caret..caret, cx);
            });
        });

        // Upstream's `replaceAndFind` notes both histories and reports the
        // single replacement at the replacement field.
        self.note_replace_history(cx);
        self.find.replace_result = Some((1, replacement.clone()));
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
        let typed_replacement = self.replacement(cx);
        // The message compares against what the user typed, so it records the
        // raw text; the unescaping happens only where the replacement runs.
        let replacement =
            if self.settings.find_regex && self.settings.find_regex_unescapes_replacement {
                finder::unescape(&typed_replacement)
            } else {
                typed_replacement.clone()
            };
        // *In Selection* narrows the scope rather than the pattern: the
        // matches come back as offsets into the whole buffer either way, so
        // nothing downstream has to know the search was bounded.
        let selection = self.active_selection(cx);
        let scope = if self.settings.find_in_selection && !selection.is_empty() {
            selection
        } else {
            0..text.len()
        };
        let (replaced, count) = finder::replace_all(
            &text,
            &searcher,
            &replacement,
            finder::ReplacementSyntax::Regex,
            &scope,
        );
        // The histories and the field message are recorded whatever the count
        // was — upstream notifies "0 replaced" and notes the history all the
        // same; only an empty string is skipped, by the append guard.
        self.note_replace_history(cx);
        self.find.replace_result = Some((count, typed_replacement.clone()));
        if count == 0 {
            self.set_status("No matches", cx);
            return;
        }

        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let editor = document.editor().clone();
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
        let editors = self.all_panes();
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
        // The warnings pane marks its offenders in the text too: upstream
        // paints every incompatible range when the scan finishes and clears
        // them in `onDisappear`. Leaving the pane is our equivalent of that.
        if pane == InspectorPane::Warnings {
            let ranges: Vec<Range<usize>> = self
                .incompatible_characters(cx)
                .into_iter()
                .map(|item| item.range)
                .collect();
            self.set_editor_marks(&ranges, cx);
        } else {
            self.clear_editor_marks(cx);
        }
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

    // ---- split editors ---------------------------------------------------

    /// Split the active document's editor in two, after the pane in focus.
    ///
    /// CotEditor's `openSplitTextView`: the new pane opens *below* (or right
    /// of) the current one, starts on the same selection, and takes the focus.
    /// The cap is four, as upstream's is.
    pub fn open_split_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.active;
        let Some(document) = self.documents.get(index) else {
            return;
        };
        if document.pane_count() >= MAXIMUM_SPLIT_EDITORS {
            self.set_status("An editor can be split into four at most", cx);
            return;
        }

        let pane = document.active_pane();
        let syntax = document.syntax.to_string();
        let text = document.editor().read(cx).value().to_string();
        let caret = document.editor().read(cx).selected_range();

        let editor = self.new_editor(text, &syntax, window, cx);
        let split = self.new_pane(editor, window, cx);
        // CotEditor copies the current selection into the new pane and scrolls
        // both to it, so the split starts where the user was looking.
        split
            .editor
            .update(cx, |state, cx| state.set_selected_range(caret, cx));

        let document = &mut self.documents[index];
        document.panes.insert(pane + 1, split);
        document.active_pane = pane + 1;
        cx.notify();
    }

    /// Close the pane in focus, keeping one editor per document.
    ///
    /// CotEditor's `closeSplitTextView`: focus goes to the pane before the one
    /// that closed, or to the last pane when the first one closed.
    pub fn close_split_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.active;
        let Some(document) = self.documents.get_mut(index) else {
            return;
        };
        let closing = document.active_pane;
        let count = document.pane_count();
        if count < 2 {
            return;
        }

        document.panes.remove(closing);
        let successor = successor_pane(closing, count);
        document.active_pane = successor;
        let successor = document.panes[successor].editor.clone();

        successor.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    /// Stack the split editors the other way round.
    ///
    /// CotEditor's `toggleSplitOrientation`, which persists the choice in
    /// `UserDefaults`; here it goes into the settings file, so the next launch
    /// opens the same way.
    pub fn toggle_split_orientation(&mut self, cx: &mut Context<Self>) {
        self.settings.split_view_vertical = !self.settings.split_view_vertical;
        let _ = self.settings.save();
        let message = if self.settings.split_view_vertical {
            "Editors now sit side by side"
        } else {
            "Editors now stack one above the other"
        };
        self.set_status(message, cx);
    }

    /// Make `pane` the one the commands act on, and put the caret in it.
    fn focus_pane(&mut self, pane: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let Some(editor) = document.editors().nth(pane).cloned() else {
            return;
        };
        document.active_pane = pane;
        editor.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    /// Move the focus to the next or previous editor.
    ///
    /// CotEditor's ⌃\` and ⇧⌃\`, which cycle the panes of the window with
    /// wraparound and do nothing when the editor is not split. SubText's tabs
    /// are its windows, though, and those keys would otherwise be dead on an
    /// unsplit document — so there they fall back to switching documents,
    /// which is what they did before the editor could be split at all.
    pub fn focus_next_editor(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = self.active;
        let Some(document) = self.documents.get(index) else {
            return;
        };
        let count = document.pane_count();
        if count < 2 {
            self.next_tab(forward, cx);
            return;
        }

        let current = document.active_pane();
        self.focus_pane(cycled_pane(current, count, forward), window, cx);
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
        document.document.set_line_ending(ending);
        let text = document.document.text().to_string();
        self.loading = true;
        for editor in document.editors() {
            editor.update(cx, |state, cx| state.set_value(text.clone(), window, cx));
        }
        self.loading = false;
        // The Format ▸ Line Endings submenu checks the one in use.
        self.refresh_menus(cx);
        self.set_status(format!("Line endings: {}", ending.abbreviation()), cx);
    }

    /// Convert the document to `encoding`.
    ///
    /// The byte-order mark comes from the file encoding rather than from the
    /// caller: UTF-8 with a mark and UTF-8 without one are two entries in the
    /// menu, and the four wide Unicode forms are written with one because their
    /// bytes are ambiguous without it.
    pub fn change_encoding(
        &mut self,
        encoding: FileEncoding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        document
            .document
            .convert_encoding(encoding.encoding, encoding.bom());
        let text = document.document.text().to_string();
        self.loading = true;
        for editor in document.editors() {
            editor.update(cx, |state, cx| state.set_value(text.clone(), window, cx));
        }
        self.loading = false;
        let _ = window;
        // The Format ▸ Text Encoding submenu checks the encoding in use, and
        // moves the document's own to the top of the list when the list does
        // not already hold it.
        self.refresh_menus(cx);
        self.set_status(format!("Encoding: {}", encoding.name()), cx);
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
                let text = document.document.text().to_string();
                self.loading = true;
                for editor in document.editors() {
                    editor.update(cx, |state, cx| state.set_value(text.clone(), window, cx));
                }
                self.loading = false;
                self.refresh_menus(cx);
                self.set_status(format!("Reinterpreted as {}", encoding.name), cx);
            }
            Err(error) => self.set_status(error.to_string(), cx),
        }
    }

    // ---- scripts ----------------------------------------------------------

    /// Run the script at `path` over the document in front.
    ///
    /// CotEditor's `ScriptManager.launchScript`, minus the modifier keys: a GPUI
    /// menu item carries no modifier state, so a click always runs the script
    /// rather than opening it in an editor or showing it in the Finder.
    ///
    /// The script's own source decides what it is given and where its answer
    /// goes — the two `%%%{CotEditorX…}%%%` tokens — so the source is read here,
    /// on this thread, because both answers come from the document. The process
    /// itself runs on the background executor: it is the user's own program, and
    /// there is no telling how long it will take.
    pub fn run_script(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let Some(script) = self
            .scripts
            .iter()
            .find_map(|item| item.find(path))
            .cloned()
        else {
            self.notice(format!("There is no script at {}", path.display()), cx);
            return;
        };

        let invocation = match std::fs::read_to_string(&script.path) {
            Ok(source) => Invocation::scan(&source),
            Err(error) => {
                self.notice(
                    format!("The script “{}” could not be read: {error}", script.name),
                    cx,
                );
                return;
            }
        };

        let input = invocation
            .input
            .and_then(|kind| self.script_input(kind, cx));
        let document = self
            .documents
            .get(self.active)
            .and_then(|document| document.document.path())
            .map(Path::to_path_buf);
        let output = invocation.output;
        let name = script.name.clone();

        let executor = cx.background_executor().clone();
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let outcome = executor
                .spawn(
                    async move { scripts::execute(&script, input.as_deref(), document.as_deref()) },
                )
                .await;
            let _ = handle.update(cx, |_, window, cx| {
                this.update(cx, |workspace, cx| {
                    workspace.finish_script(&name, outcome, output, window, cx);
                })
                .ok();
            });
        })
        .detach();
    }

    /// Say how a script ended, and put its output where it asked for it.
    fn finish_script(
        &mut self,
        name: &str,
        outcome: scripts::Outcome,
        output: Option<OutputType>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(failure) = outcome.failure {
            // Upstream hands a script's standard error to `NSApp.presentError`,
            // and for a `ScriptError` that means "append it to the console and
            // bring the window forward" — not a modal alert. The status bar
            // cannot hold a traceback: it is one line, and everything past the
            // first was being dropped.
            self.log_to_console(Some(name.to_string()), failure, cx);
            self.show_console(cx);
            self.set_status(format!("The script “{name}” failed — see the console"), cx);
            return;
        }
        match output {
            Some(kind) => self.apply_script_output(kind, outcome.output, window, cx),
            // Upstream says nothing at all when a script declares no output,
            // which leaves one that did its work indistinguishable from one
            // that did nothing.
            None => self.set_status(format!("Ran {name}"), cx),
        }
    }

    /// Put a script's output where its `%%%{CotEditorXOutput=…}%%%` token said.
    fn apply_script_output(
        &mut self,
        kind: OutputType,
        output: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match kind {
            OutputType::Pasteboard => {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(output));
                self.set_status("Script output copied to the clipboard", cx);
            }
            OutputType::NewDocument => {
                // Upstream makes the document only when there is something to
                // put in it, so a script that produced nothing does not leave an
                // empty tab behind.
                if output.is_empty() {
                    self.set_status("The script produced no output", cx);
                    return;
                }
                let mut document = Document::untitled(
                    self.settings.default_file_encoding(),
                    self.settings.default_line_ending,
                );
                document.set_text(output, true);
                let syntax = self
                    .settings
                    .default_syntax
                    .clone()
                    .unwrap_or_else(|| self.syntaxes.plain_text().name.clone());
                self.push_document(document, syntax, window, cx);
                self.set_status("Script output opened in a new document", cx);
            }
            _ => {
                let Some(document) = self.documents.get(self.active) else {
                    self.notice("There is no document to put the script's output into", cx);
                    return;
                };
                let editor = document.editor().clone();
                let range = {
                    let state = editor.read(cx);
                    kind.range(state.value().as_ref(), &state.selected_range())
                };
                let Some(range) = range else {
                    return;
                };
                let caret = range.start + output.len();
                // Deferred for the same reason a snippet is: `replace` emits
                // `Change`, and the subscription that follows the buffer back
                // into the document model must not run inside this update.
                window.defer(cx, move |window, cx| {
                    editor.update(cx, |state, cx| {
                        state.set_selected_range(range.clone(), cx);
                        state.replace(output, window, cx);
                        state.set_selected_range(caret..caret, cx);
                    });
                });
            }
        }
    }

    /// The document text a script asked for.
    fn script_input(&self, kind: InputType, cx: &App) -> Option<String> {
        let document = self.documents.get(self.active)?;
        let editor = document.editor().read(cx);
        let text = editor.value().to_string();
        let selection = editor.selected_range();

        match kind {
            InputType::Selection => text.get(selection).map(str::to_string),
            InputType::CurrentLine => subtext_core::scripts::line_range(&text, selection)
                .map(|range| text[range].to_string()),
            InputType::AllText => Some(text),
        }
    }

    /// Open the folder the Script menu is built from.
    ///
    /// Upstream only reveals it: a sandboxed application cannot create its own
    /// scripts folder, so the system has already made one. SubText is not
    /// sandboxed, and on the two platforms that have no such folder it would
    /// otherwise never exist at all — so the folder is made first. It is also
    /// the only way to get a script into a menu that is built from it.
    pub fn open_scripts_folder(&mut self, cx: &mut Context<Self>) {
        let directory = resources::script_directory();
        if let Err(error) = std::fs::create_dir_all(&directory) {
            self.notice(
                format!("Could not make {}: {error}", directory.display()),
                cx,
            );
            return;
        }
        cx.reveal_path(&directory);
    }

    pub fn set_syntax(&mut self, name: &str, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        document.syntax = SharedString::from(name.to_string());
        for editor in document.editors() {
            editor.update(cx, |state, cx| state.set_highlighter(name.to_string(), cx));
        }
        // A snippet may be limited to one syntax, so the submenu's contents
        // follow the document's — and the submenu checks the syntax in use.
        self.refresh_menus(cx);
        self.set_status(format!("Syntax: {name}"), cx);
    }

    /// Force the highlighter to run again, which is what "Reset Syntax
    /// Analysis" means in CotEditor.
    pub fn rehighlight(&mut self, cx: &mut Context<Self>) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let name = document.syntax.to_string();
        for editor in document.editors() {
            editor.update(cx, |state, cx| {
                state.set_highlighter(name.clone(), cx);
                cx.notify();
            });
        }
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
        self.set_status(
            format!("Antialiasing {}", if on { "on" } else { "off" }),
            cx,
        );
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
            if uses_spaces {
                "Tabs expand to spaces"
            } else {
                "Tabs insert tab characters"
            },
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
        // The document pane is a component `Editor`, which re-applies its own
        // style from the component theme every frame — so the colours have to
        // go there, not onto the editor state.
        highlighter::project_theme(&theme, cx);

        // The panes pick the new colours up from the component theme — see
        // `highlighter::project_theme` for why they cannot be set on the state.
        for editor in self.all_panes() {
            editor.update(cx, |_, cx| cx.notify());
        }
        let _ = self.settings.save();
        // The Format ▸ Theme submenu checks the theme in force.
        self.refresh_menus(cx);
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
        self.themes
            .all()
            .iter()
            .map(|theme| theme.name.clone())
            .collect()
    }

    /// The names of every syntax, for the Format ▸ Syntax menu.
    pub fn syntax_names(&self) -> Vec<String> {
        self.syntaxes
            .all()
            .iter()
            .map(|syntax| syntax.name.clone())
            .collect()
    }

    pub fn line_endings(&self) -> [LineEnding; 6] {
        subtext_core::line_ending::ALL
    }

    // ---- inspectors ------------------------------------------------------

    /// `Text ▸ Inspect Character` (⌃⌘I) — upstream's `showSelectionInfo`.
    ///
    /// Upstream shows this as a **popover anchored to the selected character**
    /// (`present(_:asPopoverRelativeTo:of:preferredEdge: .minY, behavior:
    /// .semitransient)`), not as a window: it is a question about one character,
    /// and a modal dialog for it interrupts the editing it is meant to serve.
    ///
    /// Upstream's guard is `textView.selectsSingleCharacter` followed by an
    /// `assertionFailure()`, so in a release build a multi-character selection
    /// simply does nothing. SubText says why instead of swallowing the
    /// keystroke — the menu item cannot grey itself out, for the reason
    /// `Straighten Quotes` cannot either (see the gap table).
    pub fn inspect_selection(&mut self, cx: &mut Context<Self>) {
        let text = self.active_text(cx);
        let selection = self.active_selection(cx);
        let selected = text.as_deref().and_then(|text| text.get(selection.clone()));
        let report = selected.map_or_else(charinfo::CharacterReport::default, |selected| {
            charinfo::CharacterReport::of(selected)
        });
        if report.is_empty() {
            self.set_status("Select a single character to inspect", cx);
            return;
        }

        // One toggle per row: upstream keeps `isBlockNameLocalized` as `@State`
        // *inside* `ScalarDetailView`, so each row remembers its own. The
        // per-scalar rows start closed, which is what a `DisclosureGroup` with
        // no `isExpanded` binding does.
        self.inspect_block_localized = vec![true; report.scalars.len()];
        self.inspect_scalar_open = vec![false; report.scalars.len()];
        self.inspect_character = Some((selection.clone(), report));
        // Upstream scrolls the character into view and flashes it before the
        // popover appears, so the reader can see which character is described.
        self.flash_find_indicator(selection, cx);
        cx.notify();
    }

    /// The Inspect Character popover — upstream's `CharacterInspectorView`.
    ///
    /// `None` while it is not up, and also once the selection is no longer the
    /// one it describes: a surface anchored to a selection must not outlive it,
    /// which is what upstream's `.semitransient` behaviour amounts to here.
    fn render_inspect_character(
        &self,
        rem: gpui_kit::Pixels,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (range, report) = self.inspect_character.as_ref()?;
        let editor = self.active_document()?.editor().clone();
        let anchor = {
            let state = editor.read(cx);
            if state.selected_range() != *range {
                return None;
            }
            // `None` while the character is scrolled out of view — there is
            // nothing on screen to point at.
            state.range_to_bounds(range)?
        };

        let detail = self.render_character_detail(report, cx);

        // The glyph is drawn large, and greyed when it is a *picture* of an
        // invisible control character rather than the character itself —
        // upstream's `textColor = pictureCharacter != nil ? .tertiaryLabelColor
        // : .labelColor`. The face is the serif one upstream asks AppKit for.
        let glyph = report.picture().map_or_else(
            || (report.character.clone(), false),
            |picture| (picture.to_string(), true),
        );
        let surface = h_flex()
            .items_start()
            .gap_3()
            .p_3()
            .popover_style(cx)
            .shadow_md()
            .child(
                div()
                    .flex_none()
                    .min_w(rems(4.))
                    .text_size(rems(4.))
                    .font_family(serif_family())
                    .text_color(if glyph.1 {
                        cx.theme().muted_foreground
                    } else {
                        cx.theme().foreground
                    })
                    .child(glyph.0),
            )
            .child(detail);

        Some(
            Positioner::side(anchor)
                // Upstream's `preferredEdge: .minY` — above the character.
                // `Positioner` flips it below when there is no room, which is
                // what AppKit does with the same preference.
                .placement(Placement::Top)
                // `Positioner` takes pixels, so the design scale is applied
                // by hand here the way `Dialog::w` and `resizable_panel().size()`
                // do.
                .offset(rem * 0.5)
                .margin(rem * 0.5)
                .occlude()
                .child(surface)
                .into_any_element(),
        )
    }

    /// The right-hand column of the Inspect Character popover: the headline,
    /// then either one scalar's detail or one collapsible row per scalar.
    fn render_character_detail(
        &self,
        report: &charinfo::CharacterReport,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut detail = v_flex().gap_1();
        match report.description() {
            Some(description) => {
                detail = detail.child(
                    div()
                        .when(!report.is_complex(), |this| {
                            this.font_weight(FontWeight::SEMIBOLD)
                        })
                        .child(description),
                );
            }
            // Upstream's "Unknown": the character has no Unicode name to show.
            None => {
                detail = detail.child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child("Unknown"),
                );
            }
        }

        if report.is_single_scalar() {
            detail = detail.child(self.render_scalar_detail(&report.scalars[0], 0, true, cx));
        }

        // A character of several scalars gets a row each, so the panel can say
        // what every part of it is. Upstream shows only the block and the
        // category there — the code point and the version are in the row's own
        // label and in the single-scalar case.
        if report.scalars.len() > 1 {
            let mut rows = v_flex().gap_1();
            for (index, scalar) in report.scalars.iter().enumerate() {
                rows = rows.child(self.render_scalar_row(index, scalar, cx));
            }
            detail = detail.child(rows);
        }

        detail.into_any_element()
    }

    /// One collapsible row of a multi-scalar character.
    fn render_scalar_row(
        &self,
        index: usize,
        scalar: &charinfo::ScalarInfo,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity().downgrade();
        let open = self
            .inspect_scalar_open
            .get(index)
            .copied()
            .unwrap_or(false);
        let trigger = AccordionTrigger::new(("inspect-scalar", index))
            .open(open)
            .h_flex()
            .w_full()
            .gap_1()
            .on_change(move |open, _, _, cx| {
                workspace
                    .update(cx, |this, cx| {
                        if let Some(row) = this.inspect_scalar_open.get_mut(index) {
                            *row = open;
                        }
                        cx.notify();
                    })
                    .ok();
            })
            .child(
                Icon::new(IconName::ChevronRight)
                    .xsmall()
                    .flex_none()
                    .text_color(cx.theme().muted_foreground)
                    .rotate(percentage(if open { 0.25 } else { 0. })),
            )
            // Upstream's label is the code point, in tabular figures so the
            // column of them lines up, then the name in medium weight.
            .child(
                div()
                    .flex_none()
                    .min_w(rems(3.5))
                    .child(scalar.code_point.clone()),
            )
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(scalar.display_name().to_string()),
            );

        v_flex()
            .w_full()
            .child(trigger)
            .when(open, |this| {
                this.child(
                    h_flex()
                        .items_start()
                        .gap_2()
                        .pb_1()
                        .child(
                            div()
                                .flex_none()
                                .min_w(rems(2.))
                                .text_size(rems(1.75))
                                .text_color(cx.theme().muted_foreground)
                                .child(scalar.picture.map_or_else(
                                    || {
                                        char::from_u32(scalar.value)
                                            .map(String::from)
                                            .unwrap_or_default()
                                    },
                                    String::from,
                                )),
                        )
                        .child(self.render_scalar_detail(scalar, index, false, cx)),
                )
            })
            .into_any_element()
    }

    /// A `Grid` of label/value rows — upstream's `ScalarDetailView`.
    ///
    /// `with_code_point` is its `items:` set: `.all` for the single-scalar case,
    /// `[.block, .category]` for a row of a multi-scalar character.
    fn render_scalar_detail(
        &self,
        scalar: &charinfo::ScalarInfo,
        index: usize,
        with_code_point: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = v_flex().gap_0p5();
        let row = |label: &str, value: AnyElement, cx: &App| {
            h_flex()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .min_w(rems(4.5))
                        .text_right()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{label}:")),
                )
                .child(div().min_w_0().child(value))
        };

        if with_code_point {
            let code_point = scalar.code_point_with_surrogates();
            rows = rows.child(row(
                "Code Point",
                h_flex()
                    .gap_1()
                    .child(code_point)
                    // Upstream's `DeprecatedBadge`: a bordered word beside the
                    // code point, only when Unicode has deprecated the scalar.
                    .when(scalar.is_deprecated, |this| {
                        this.child(
                            div()
                                .px_0p5()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().border)
                                .text_color(cx.theme().muted_foreground)
                                .text_xs()
                                .child("deprecated"),
                        )
                    })
                    .into_any_element(),
                cx,
            ));
        }

        // The block is a button, not a value: clicking it swaps the name the
        // interface uses for the one `Blocks.txt` gives. An unassigned scalar
        // has no block to toggle, so it stays a muted word.
        let localized = self
            .inspect_block_localized
            .get(index)
            .copied()
            .unwrap_or(true);
        let block = match scalar.block {
            Some(block) if localized => charinfo::display_block_name(block).to_string(),
            Some(block) => block.to_string(),
            None => "No Block".to_string(),
        };
        let mut block_value = Button::new(("inspect-block", index))
            .ghost()
            .xsmall()
            .label(block)
            .tooltip("Show the Unicode block name");
        if scalar.block.is_none() {
            block_value = block_value.disabled(true);
        }
        rows = rows.child(row(
            "Block",
            block_value
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(localized) = this.inspect_block_localized.get_mut(index) {
                        *localized = !*localized;
                    }
                    cx.notify();
                }))
                .into_any_element(),
            cx,
        ));

        rows = rows.child(row(
            "Category",
            div().child(scalar.category.to_string()).into_any_element(),
            cx,
        ));

        // The version is drawn only when Unicode has assigned the scalar, which
        // is upstream's `if let age`.
        if let Some(version) = scalar.version() {
            rows = rows.child(row("Version", div().child(version).into_any_element(), cx));
        }

        rows.into_any_element()
    }

    /// `Text ▸ Advanced Character Count…` — upstream's `toggleAdvancedCounter:`.
    ///
    /// A toggle: while the counter is running the command stops it, which is
    /// what upstream's `dismissAdvancedCharacterCounter` does. It is the same
    /// command the capsule's *Stop Count* runs.
    pub fn advanced_counter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.count_running {
            self.stop_advanced_count(cx);
            return;
        }
        self.advanced_counter_sheet(window, cx);
    }

    /// The sheet that starts the counter — upstream's `CharacterCountOptionsSheetView`.
    ///
    /// Upstream's menu item has no key equivalent, so there is nothing to bind.
    fn advanced_counter_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = Workspace::weak(cx);
        // The panel is built here and kept: the running counter shows the same
        // entity behind its gear, and two of them would be two sets of selects
        // to keep in step.
        let options =
            cx.new(|cx| CountOptionsView::new(workspace.clone(), &self.settings, window, cx));
        self.count_options = Some(options.clone());
        let sheet = cx.new(|cx| CountSheet::new(workspace, options, window, cx));
        window.open_dialog(cx, move |dialog, window, _| {
            let submit = sheet.clone();
            dialog
                .title("Advanced Character Count")
                // `Dialog::w` takes pixels, not a `Length`, so the interface
                // scale is applied by hand rather than by the layout engine.
                .w(window.rem_size() * 32.)
                .child(sheet.clone())
                .on_ok(move |_, window, cx| {
                    submit.update(cx, |sheet, cx| sheet.submit(window, cx));
                    true
                })
        });
    }

    /// Start counting, which is what the sheet's *Start* button does.
    pub fn start_advanced_count(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.count_running = true;
        cx.notify();
    }

    /// Stop counting, and forget the options panel with it.
    pub fn stop_advanced_count(&mut self, cx: &mut Context<Self>) {
        self.count_running = false;
        self.count_options = None;
        self.count_options_open = false;
        // The next count starts in the corner again, as a freshly created
        // capsule does upstream.
        self.count_capsule_offset = (0., 0.);
        self.count_capsule_drag = None;
        cx.notify();
    }

    /// The incompatible characters in the active document, memoised.
    ///
    /// CotEditor's guard is upstream's: a Unicode encoding has nothing to be
    /// incompatible with, and an empty buffer has nothing in it.
    fn incompatible_characters(&self, cx: &App) -> Vec<subtext_core::IncompatibleCharacter> {
        let Some(document) = self.active_document() else {
            return Vec::new();
        };
        let encoding = document.document.encoding();
        if encoding.is_unicode() {
            return Vec::new();
        }
        let Some(text) = self.active_text(cx) else {
            return Vec::new();
        };
        let key = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut hasher);
            hasher.finish()
        };
        if let Some((cached_key, cached_encoding, items)) =
            self.incompatible_cache.borrow().as_ref()
            && *cached_key == key
            && *cached_encoding == encoding
        {
            return items.clone();
        }
        let items = subtext_core::incompatible_characters(&text, encoding);
        *self.incompatible_cache.borrow_mut() = Some((key, encoding, items.clone()));
        items
    }

    /// `Edit ▸ Delete` — `NSTextView.delete:` behind ⌘⌫.
    ///
    /// The text engine handles a bare ⌫ on its own; this is the menu's own
    /// shortcut, which deletes backwards as well rather than to the beginning
    /// of the line, because the menu's key equivalent wins over AppKit's
    /// standard binding.
    pub fn delete_backward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.text_op(window, cx, |text, selection, _| {
            textops::delete_backward(text, selection)
        });
    }

    /// `Edit ▸ Complete` (⌥⎋) — upstream's `complete:`.
    ///
    /// Upstream calls `NSTextView.complete(_:)` and lets AppKit draw the list.
    /// SubText draws its own, anchored to the caret, and offers the same
    /// candidates.
    pub fn complete(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // A command replaces whatever the typing timer was about to do.
        self.completion_auto = None;
        if self.open_completion(cx) {
            return;
        }
        // Upstream's `completions(forPartialWordRange:)` returns nil or an
        // empty list and AppKit then shows nothing at all. SubText says why,
        // which is the same rule the rest of the unimplemented commands follow.
        self.set_status("No completions for this word", cx);
    }

    /// Open the list for the word at the caret.
    ///
    /// Returns whether there is anything to show. A list already open on the
    /// same word keeps its highlight: the automatic timer fires again as the
    /// user pauses, and moving the selection back to the first row each time
    /// would undo a choice they had already made.
    fn open_completion(&mut self, cx: &mut Context<Self>) -> bool {
        let previous = self.completion.take();
        let Some((partial, _)) = self.completions_at_caret(cx) else {
            return false;
        };
        let selected = previous
            .filter(|state| state.start == partial.start)
            .map_or(0, |state| state.selected);
        self.completion = Some(Completion {
            start: partial.start,
            selected,
        });
        cx.notify();
        true
    }

    /// Which sources the Mode pane has switched on.
    fn completion_sources(&self) -> completion::CompletionSources {
        completion::CompletionSources {
            standard: self.settings.mode.completion_standard_words,
            document: self.settings.mode.completion_document_words,
            syntax: self.settings.mode.completion_syntax_words,
        }
    }

    /// The words the active document's syntax defines.
    fn syntax_completion_words(&self) -> Vec<String> {
        self.active_document()
            .and_then(|document| self.syntaxes.by_name(&document.syntax))
            .map(|syntax| {
                syntax
                    .completions
                    .iter()
                    .map(|item| item.text.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether the automatic list should be up — upstream's `performCompletion`.
    ///
    /// Everything it asks for is asked of the live buffer, so the same check
    /// gates the timer and validates it once it fires.
    fn automatic_completion_ready(&self, cx: &App) -> bool {
        if !self.settings.mode.automatic_completion || !self.completion_sources().any() {
            return false;
        }
        // Upstream refuses while a selection is up: completing would replace
        // the selection rather than continue a word.
        let selection = self.active_selection(cx);
        if !selection.is_empty() {
            return false;
        }
        let Some(text) = self.active_text(cx) else {
            return false;
        };
        let syntax_words = self.syntax_completion_words();
        let first_letters: Vec<char> = syntax_words
            .iter()
            .filter_map(|word| word.chars().next())
            .collect();
        let partial = completion::partial_word_range(&text, selection.start, &first_letters);
        completion::should_complete_automatically(
            &text,
            &partial,
            MINIMUM_AUTOMATIC_COMPLETION_LENGTH,
        )
    }

    /// Restart the automatic-completion timer.
    ///
    /// Called on every insertion. Dropping the previous task cancels it, which
    /// is upstream's `Debouncer.schedule`: the list appears when typing pauses
    /// for [`AUTOMATIC_COMPLETION_DELAY`], not on every character.
    fn schedule_automatic_completion(&mut self, cx: &mut Context<Self>) {
        self.completion_auto = None;
        if !self.automatic_completion_ready(cx) {
            return;
        }
        self.completion_auto = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(AUTOMATIC_COMPLETION_DELAY)
                .await;
            // The caret may have moved, the word may be gone, the setting may
            // have been switched off, or the window may have closed. Upstream
            // re-checks the same guards after the delay; so does this.
            this.update(cx, |workspace, cx| {
                if workspace.automatic_completion_ready(cx) {
                    workspace.open_completion(cx);
                }
            })
            .ok();
        }));
    }

    /// The candidates for the word at the caret.
    ///
    /// `None` when there is no word to complete, no source switched on, or
    /// nothing to offer.
    fn completions_at_caret(&self, cx: &App) -> Option<(Range<usize>, Vec<String>)> {
        let text = self.active_text(cx)?;
        let caret = self.active_selection(cx).start.min(text.len());
        let syntax_words = self.syntax_completion_words();
        let first_letters: Vec<char> = syntax_words
            .iter()
            .filter_map(|word| word.chars().next())
            .collect();
        let partial = completion::partial_word_range(&text, caret, &first_letters);

        let sources = self.completion_sources();
        let candidates = completion::candidates(&text, &partial, sources, &syntax_words);
        (!candidates.is_empty()).then_some((partial, candidates))
    }

    /// What the popup should be showing right now.
    ///
    /// The popup belongs to the *word* it was opened on: typing another
    /// character of that word extends it and re-filters the list, while moving
    /// to a different word abandons the popup.
    fn completion_popup(&self, cx: &App) -> Option<(Range<usize>, Vec<String>)> {
        let anchor = self.completion.as_ref()?;
        let (partial, candidates) = self.completions_at_caret(cx)?;
        (partial.start == anchor.start).then_some((partial, candidates))
    }

    /// Move the highlight, wrapping at both ends as AppKit's list does.
    fn move_completion(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some((_, candidates)) = self.completion_popup(cx) else {
            return;
        };
        let Some(state) = self.completion.as_mut() else {
            return;
        };
        let count = candidates.len();
        state.selected = if down {
            (state.selected + 1) % count
        } else {
            (state.selected + count - 1) % count
        };
        cx.notify();
    }

    /// Put the highlighted candidate in place of the partial word.
    ///
    /// Upstream's `insertCompletion(_:forPartialWordRange:movement:isFinal:)`:
    /// the word replaces the partial, and a word with a parenthesised part
    /// leaves the caret inside it.
    fn apply_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((partial, candidates)) = self.completion_popup(cx) else {
            self.completion = None;
            return;
        };
        let Some(word) = self
            .completion
            .and_then(|state| candidates.get(state.selected).cloned())
        else {
            self.completion = None;
            return;
        };
        self.completion = None;

        let selection = completion::selection_after_insert(&word, &partial);
        self.text_op(window, cx, move |text, _, _| {
            let mut out = String::with_capacity(text.len() + word.len());
            out.push_str(&text[..partial.start]);
            out.push_str(&word);
            out.push_str(&text[partial.end..]);
            textops::Edit {
                text: out,
                selection,
            }
        });
        // The insertion above grew the buffer, which scheduled the next
        // automatic list. Upstream's `insertCompletion` cancels the debouncer
        // instead of re-arming it, so the list does not reopen over the word
        // the user just chose.
        self.completion_auto = None;
    }

    /// Dismiss the popup without inserting anything.
    fn cancel_completion(&mut self, cx: &mut Context<Self>) {
        self.completion_auto = None;
        if self.completion.take().is_some() {
            cx.notify();
        }
    }

    /// The completion popup itself: a list anchored to the word being completed.
    ///
    /// The list is not virtualized. It is a transient popup whose length is
    /// bounded by the words sharing a prefix, and its rows are one line of text
    /// each — see the gap table.
    fn render_completion(
        &self,
        rem: gpui_kit::Pixels,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (partial, candidates) = self.completion_popup(cx)?;
        let selected = self
            .completion
            .map_or(0, |state| state.selected)
            .min(candidates.len() - 1);
        let editor = self.active_document()?.editor().clone();
        // Anchored to the *end* of the partial word — the caret.
        let anchor = editor
            .read(cx)
            .range_to_bounds(&(partial.end..partial.end))?;

        let mut rows = v_flex().w_full();
        for (index, candidate) in candidates.iter().enumerate() {
            rows = rows.child(
                div()
                    .id(("completion", index))
                    .w_full()
                    .px_2()
                    .py_0p5()
                    .truncate()
                    .when(index == selected, |this| {
                        this.bg(cx.theme().accent)
                            .text_color(cx.theme().accent_foreground)
                    })
                    // A click picks a row, which is what a pointer user expects
                    // of a list that has a highlight.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            if let Some(state) = this.completion.as_mut() {
                                state.selected = index;
                            }
                            this.apply_completion(window, cx);
                        }),
                    )
                    .child(candidate.clone()),
            );
        }

        Some(
            Positioner::side(anchor)
                .placement(Placement::Bottom)
                // `Positioner` takes pixels, so the design scale is applied
                // by hand here the way `Dialog::w` does.
                .offset(rem * 0.25)
                .margin(rem * 0.5)
                .occlude()
                .child(
                    v_flex()
                        .id("completion-list")
                        .min_w(rems(12.))
                        .max_w(rems(28.))
                        .max_h(rems(14.))
                        .overflow_y_scroll()
                        .p_1()
                        .popover_style(cx)
                        .shadow_md()
                        .child(rows),
                )
                .into_any_element(),
        )
    }

    /// The running counter's capsule.
    ///
    /// `None` while the counter is stopped. Upstream floats it over the text at
    /// the top-right and lets the user drag it anywhere; SubText pins it there
    /// and does not drag — see the gap table.
    fn render_count_capsule(
        &self,
        rem: gpui_kit::Pixels,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.count_running {
            return None;
        }
        let options = self.settings.count_options();
        let (text, selection) = {
            let document = self.active_document()?;
            let state = document.editor().read(cx);
            (state.value().to_string(), state.selected_range())
        };
        // Upstream counts the selection while there is one and the whole
        // document otherwise, which is what makes the capsule useful while
        // editing rather than only after.
        let count = if selection.is_empty() {
            options.count(&text)
        } else {
            text.get(selection)
                .and_then(|selected| options.count(selected))
        };
        let (number, suffix) = crate::count_panel::count_parts(count, options.unit);

        let mut row = h_flex().items_center().gap_2();
        if count.is_none() {
            // Upstream's "failed" is drawn with a filled warning triangle.
            row = row.child(
                Icon::new(IconName::TriangleAlert)
                    .xsmall()
                    .text_color(cx.theme().danger),
            );
        }
        row = row
            .when(!number.is_empty(), |this| {
                this.child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(number),
                )
            })
            .child(div().text_color(cx.theme().muted_foreground).child(suffix));

        // The gear opens the same options panel the sheet showed, so the count
        // can be adjusted while it is watched.
        if let Some(options_view) = self.count_options.clone() {
            row = row.child(
                Popover::new("count-options")
                    .open(self.count_options_open)
                    .on_open_change({
                        let workspace = cx.entity().downgrade();
                        move |open, _, cx| {
                            let open = *open;
                            workspace
                                .update(cx, |workspace, cx| {
                                    workspace.count_options_open = open;
                                    cx.notify();
                                })
                                .ok();
                        }
                    })
                    .trigger(
                        Button::new("count-options-trigger")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Settings)
                            .accessibility_label("Show options")
                            .tooltip("Show options"),
                    )
                    .content(move |_, _, _| {
                        div().p_3().child(options_view.clone()).into_any_element()
                    }),
            );
        }

        let capsule = h_flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .rounded_full()
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_md()
            .child(row)
            // Dragging is the capsule's own business: a press records where the
            // pointer was, each move shifts the offset by the distance it
            // travelled, and releasing ends the drag. Deltas come from
            // successive window positions, so no frame conversion is needed.
            //
            // Nothing is clamped: the capsule is clipped by the editor area it
            // floats over, which is what happens upstream at the edge too.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui_kit::MouseDownEvent, _, cx| {
                    this.count_capsule_drag =
                        Some((f32::from(event.position.x), f32::from(event.position.y)));
                    cx.notify();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui_kit::MouseMoveEvent, _, cx| {
                    let Some((last_x, last_y)) = this.count_capsule_drag else {
                        return;
                    };
                    let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                    this.count_capsule_offset =
                        drag_capsule_offset(this.count_capsule_offset, (last_x, last_y), (x, y));
                    this.count_capsule_drag = Some((x, y));
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.count_capsule_drag.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            // Upstream's context menu: the number, then the way out. The copy
            // is the bare count, not the sentence around it.
            .context_menu({
                let count = count.map(crate::count_panel::grouped_count);
                let stop = cx.entity().downgrade();
                move |menu, _, _| {
                    let menu = match &count {
                        Some(number) => menu
                            .item(PopupMenuItem::new("Copy").on_click({
                                let number = number.clone();
                                move |_, window, cx| {
                                    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                                        number.clone(),
                                    ));
                                    let _ = window;
                                }
                            }))
                            .separator(),
                        None => menu,
                    };
                    menu.item(PopupMenuItem::new("Stop Count").on_click({
                        let workspace = stop.clone();
                        move |_, _, cx| {
                            workspace
                                .update(cx, |workspace, cx| {
                                    workspace.stop_advanced_count(cx);
                                })
                                .ok();
                        }
                    }))
                }
            })
            .into_any_element();

        let (right, down) = self.count_capsule_offset;
        Some(
            div()
                .absolute()
                .top(rem * 0.5 + px(down))
                .right(rem * 0.5 + px(right))
                .child(capsule)
                .into_any_element(),
        )
    }

    // ---- windows ---------------------------------------------------------

    /// Open the settings window, or bring the existing one forward.
    pub fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.open_settings(None, cx);
    }

    /// Show the settings window, on a particular pane.
    ///
    /// *Edit Snippets…* uses this: the command lives in the menu bar, but what
    /// it opens is one pane of the settings window. CotEditor reaches it the
    /// same way — `SnippetManager` asks the settings window controller to come
    /// forward on the Snippets pane.
    pub fn show_settings_pane(&mut self, pane: SettingsPane, cx: &mut Context<Self>) {
        self.open_settings(Some(pane), cx);
    }

    /// Bring up the encoding list editor: the settings window on the Format
    /// pane, with the *Edit List…* dialog over it.
    ///
    /// Upstream's `showEncodingsListEditor` is these two steps in this order —
    /// `openPane(.format)`, then a request the pane answers by presenting the
    /// sheet it owns. SubText has the pane open the dialog itself, so what
    /// crosses between the two windows is a call rather than a request; but the
    /// call has to wait for the window, because `open_settings` defers the
    /// window's creation and there is nothing to call until that has run. Both
    /// defers go on the same queue, in this order, so the second one sees the
    /// handle the first one stored.
    pub fn show_encoding_list_editor(&mut self, cx: &mut Context<Self>) {
        self.open_settings(Some(SettingsPane::Format), cx);

        let weak = cx.entity().downgrade();
        cx.defer(move |cx| {
            // Read and release the Workspace borrow before touching the
            // settings window: `edit_encoding_list` reads the settings back
            // through this Workspace, and nesting that read inside its own
            // update would hit GPUI's re-entrant entity guard.
            //
            // The view is held weakly, so `WeakEntity::update_in` is the call
            // for it: it finds the entity's own window instead of taking one,
            // which is what a deferred effect has. `Entity::update_in` would
            // need a `VisualContext`, and an effect only has the `App`.
            let view = weak
                .read_with(cx, |workspace, _| workspace.settings_view.clone())
                .ok()
                .flatten();
            let Some(view) = view else {
                return;
            };
            view.update_in(cx, |settings, window, cx| {
                settings.edit_encoding_list(window, cx);
            })
            .ok();
        });
    }

    /// Bring the settings window forward, opening it if it is not up.
    ///
    /// `pane` is `None` when the caller does not care which pane is showing,
    /// which is the plain Settings… command: moving a window the user has
    /// already arranged would be a surprise.
    fn open_settings(&mut self, pane: Option<SettingsPane>, cx: &mut Context<Self>) {
        if let Some(handle) = self.settings_window {
            // The window's root view is the Base `Root`, not the settings view:
            // `gpui_kit::open_window` wraps whatever the builder returns, so
            // that the window carries the component facilities — overlays,
            // notifications, focus restoration — a view cannot install itself.
            // The view is therefore held separately, and the handle is only
            // asked whether the window is still up.
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
                if let (Some(pane), Some(view)) = (
                    pane,
                    self.settings_view.as_ref().and_then(WeakEntity::upgrade),
                ) {
                    view.update(cx, |settings, cx| settings.show(pane, cx));
                }
                return;
            }
            self.settings_window = None;
            self.settings_view = None;
        }

        let workspace = cx.entity();
        let weak = workspace.downgrade();
        let options = WindowOptions {
            // A settings sheet is a column of labelled controls, not a document:
            // without an explicit size the window opened at the main window's
            // width and left two thirds of itself empty. 760 × 580 holds the
            // widest pane (Format's encoding list) with its sidebar.
            //
            // Platform window geometry, not design scale: a window size is a
            // constraint on the OS window, so it stays in pixels.
            window_bounds: Some(WindowBounds::centered(
                gpui_kit::size(px(760.), px(580.)),
                cx,
            )),
            window_min_size: Some(gpui_kit::size(px(720.), px(480.))),
            ..Default::default()
        };

        // The settings window reads the settings and the syntax list *through*
        // this workspace — it holds a weak handle rather than a copy, so that
        // there is only ever one source of truth. But `open_window` builds and
        // draws the new root view before it returns, and every command that
        // reaches here is dispatched from inside `Workspace::update`, which has
        // the workspace leased and therefore unreadable. Deferring the whole
        // thing to the end of the effect cycle is what makes the read legal:
        // by then the update has returned and the workspace is back in the
        // entity map. Opening a window is not something the caller waits on, so
        // nothing is lost by it happening a moment later.
        cx.defer(move |cx| {
            // Nothing to attach the window to if the workspace has gone.
            if weak.upgrade().is_none() {
                return;
            }
            let root = weak.clone();
            let opened = gpui_kit::open_window(options, cx, move |window, cx| {
                cx.new(|cx| SettingsWindow::new(root, pane, window, cx))
            });
            workspace.update(cx, |workspace, cx| match opened {
                Ok((handle, settings)) => {
                    workspace.settings_window = Some(handle);
                    workspace.settings_view = Some(settings.downgrade());
                }
                Err(error) => workspace.set_status(error.to_string(), cx),
            });
        });
    }

    pub fn show_about(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.about_window {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
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
        match gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| AboutView::new())) {
            Ok((handle, _about)) => self.about_window = Some(handle),
            Err(error) => self.set_status(error.to_string(), cx),
        }
    }

    /// Show the console, or bring it forward — CotEditor's `Script ▸ Show
    /// Console`.
    ///
    /// The window holds no state of its own: the log is a `Global`, so the
    /// window is only a view onto it and reopening it shows the entries from
    /// before it was closed, which is what a console is for.
    pub fn show_console(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.console_window {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
                return;
            }
            self.console_window = None;
        }

        let Some(console) = console::ConsoleHandle::get(cx) else {
            self.set_status("The console is not available", cx);
            return;
        };
        let log = console.entity().clone();
        let options = WindowOptions {
            // Upstream's panel opens at 360 × 200 — an OS window geometry, not
            // a design measurement, so it stays in pixels.
            window_bounds: Some(WindowBounds::centered(
                gpui_kit::size(px(360.), px(200.)),
                cx,
            )),
            window_min_size: Some(gpui_kit::size(px(280.), px(140.))),
            ..Default::default()
        };
        match gpui_kit::open_window(options, cx, |_, cx| {
            cx.new(|cx| console::ConsoleView::new(log, cx))
        }) {
            Ok((handle, _)) => self.console_window = Some(handle),
            Err(error) => self.set_status(error.to_string(), cx),
        }
    }

    /// Write one entry to the console, whether or not its window is open.
    fn log_to_console(&mut self, title: Option<String>, message: String, cx: &mut Context<Self>) {
        // The console is created at launch, so a missing one only happens in a
        // test that builds a workspace without it.
        if let Some(console) = console::ConsoleHandle::get(cx) {
            console.entity().update(cx, |console, cx| {
                console.append(console::Log::now(title, message), cx)
            });
        }
    }

    /// The Quick Actions bar — CotEditor's ⌘K.
    ///
    /// A floating panel over this window, sized to its content and centred, as
    /// upstream's `NSPanel` is. It is opened the way the settings window is,
    /// and for the same reason: `open_window` builds and draws the new root
    /// view before it returns, and this is reached from inside
    /// `Workspace::update`, which has the workspace leased and therefore
    /// unreadable. Deferring the whole thing to the end of the effect cycle is
    /// what makes the new view's read of the workspace legal.
    pub fn show_quick_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = self.quick_actions_window {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
                return;
            }
            self.quick_actions_window = None;
        }

        let workspace = cx.entity();
        let weak = workspace.downgrade();
        // The panel runs its commands against *this* window, whichever window
        // happens to be key when a command is confirmed.
        let document_window = window.window_handle();
        // The commands are read off the bar as it stands now, which is what
        // upstream does when it shows the panel and the only way the runtime
        // items can be in the list at all: what the bar offers depends on the
        // document in front and on the scripts in the user's folder.
        let commands = menus::commands(&self.menu_state());
        // The panel is as tall as its content, so it opens at the height of an
        // empty bar and grows as the query brings results in. The metrics come
        // from this window because the two share a theme.
        let bounds = WindowBounds::centered(
            gpui_kit::size(
                window.rem_size() * command_bar::WIDTH,
                command_bar::window_height(0, window),
            ),
            cx,
        );
        let options = WindowOptions {
            window_bounds: Some(bounds),
            // Upstream's panel hides all three window buttons, hides its
            // title, and lets the whole surface drag the panel. `titlebar:
            // None` is the one style mask that leaves the buttons off.
            titlebar: None,
            kind: WindowKind::Floating,
            // The panel draws its own rounded surface, so the window behind it
            // has to be see-through at the corners.
            window_background: WindowBackgroundAppearance::Transparent,
            is_resizable: false,
            ..Default::default()
        };

        cx.defer(move |cx| {
            // Nothing to attach the panel to if the workspace has gone.
            if weak.upgrade().is_none() {
                return;
            }
            let root = weak.clone();
            let opened = gpui_kit::open_window(options, cx, move |window, cx| {
                cx.new(|cx| CommandBar::new(commands, root, document_window, window, cx))
            });
            workspace.update(cx, |workspace, cx| match opened {
                Ok((handle, _panel)) => workspace.quick_actions_window = Some(handle),
                Err(error) => workspace.set_status(error.to_string(), cx),
            });
        });
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
                            this.refresh_menus(cx);
                            cx.notify();
                        }
                    })),
            )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panes = [
            (
                SidebarPane::FileBrowser,
                "File Browser",
                gpui_kit::assets::IconName::Folder,
            ),
            (
                SidebarPane::FolderFind,
                "Folder Find",
                gpui_kit::assets::IconName::Search,
            ),
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
            .w(rems(15.))
            .min_w(rems(11.))
            .max_w(rems(24.))
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
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No recent files"),
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

    /// The *Folder Find* pane.
    ///
    /// The pane is an entity of its own: it owns the query, the options and the
    /// search results, and it drives the background walk itself. The workspace
    /// only hands it a place in the sidebar and the folder of the active
    /// document to fall back on.
    fn render_folder_find(&self, _cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        self.folder_find.clone().into_any_element()
    }

    fn render_inspector(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        let is_outline = self.settings.inspector_pane == InspectorPane::Outline;
        let body = match self.settings.inspector_pane {
            InspectorPane::Document => self.render_document_inspector(window, cx),
            InspectorPane::Outline => self.render_outline(window, cx),
            InspectorPane::Warnings => self.render_warnings(cx),
        };

        v_flex()
            .w(rems(16.))
            .min_w(rems(12.))
            .max_w(rems(26.))
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
                    // The outline is a virtualized list and scrolls itself;
                    // wrapping it in a second scroller would leave the two
                    // fighting over the wheel.
                    .when(!is_outline, |this| this.overflow_y_scroll())
                    .child(body),
            )
    }

    /// The Full Path row, which carries upstream's *Show in Finder* button.
    ///
    /// Upstream draws the path with `truncationMode(.middle)`, a `help` tooltip
    /// and a bordered-circle arrow that reveals the file. GPUI truncates at the
    /// end and a plain `div` carries no tooltip, so those two decorations are
    /// lost; the button is the part that *does* something, and it is kept. It
    /// only appears for a document that has a file, which is when upstream
    /// shows it too.
    fn inspector_full_path(&self, path: Option<&Path>, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = path
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        // A button rather than a plain `div` because only the interactive
        // elements carry a tooltip in this component set — and a tooltip is
        // where the rest of an elided path can be read. Clicking copies it,
        // which is the job upstream's `textSelection(.enabled)` leaves to
        // dragging across the text.
        let elided = subtext_core::textops::middle_elided(&shown, INSPECTOR_PATH_ELISION);
        let path_button = (!shown.is_empty()).then(|| {
            let full = shown.clone();
            Button::new("inspector-path")
                .ghost()
                .xsmall()
                .flex_none()
                .min_w_0()
                .max_w_full()
                .label(elided)
                .tooltip(full.clone())
                .on_click(cx.listener(move |_this, _, _, cx| {
                    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(full.clone()));
                }))
        });
        let reveal = path.map(Path::to_path_buf).map(|path| {
            Button::new("inspector-reveal")
                .ghost()
                .xsmall()
                .icon(IconName::CircleArrowRight)
                .tooltip("Show in Finder")
                .on_click(cx.listener(move |_this, _, _, _| reveal_in_file_manager(&path)))
        });

        h_flex()
            .w_full()
            .gap_2()
            .items_start()
            .justify_between()
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Full Path"),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .justify_end()
                    .when_some(path_button, |this, button| this.child(button))
                    .when_some(reveal, |this, button| this.child(button)),
            )
    }

    /// The Document inspector — upstream's `DocumentInspectorView`.
    ///
    /// Four `DisclosureGroup`s, each a `Form` of label/value rows:
    ///
    /// ```text
    /// File            Created · Modified · Size · Tags · Permissions · Owner · Full Path
    /// Text Settings   Encoding · Line Endings · Mode
    /// Count           Lines · Characters · Words · Location · Line · Column
    /// Character       Code Point(s) · Name · Block · Category   (only when one character is selected)
    /// ```
    ///
    /// Everything the rows need is read in one pass here, because the buffer and
    /// the selection have to be copied out together anyway.
    fn render_document_inspector(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let Some(document) = self.active_document() else {
            return v_flex().into_any_element();
        };
        let model = &document.document;
        let encoding = FileEncoding::of_document(model.encoding(), model.bom());
        let (text, selection) = {
            let state = document.editor().read(cx);
            (state.value().to_string(), state.selected_range())
        };
        let selected_text = text.get(selection.clone()).unwrap_or_default().to_string();

        // ---- File ---------------------------------------------------------
        let attributes = model.path().and_then(fileattrs::FileAttributes::read);
        let file_rows = {
            let (created, modified) = match &attributes {
                Some(attributes) => (
                    attributes.created.and_then(fileattrs::format_timestamp),
                    attributes.modified.and_then(fileattrs::format_timestamp),
                ),
                None => (None, None),
            };
            let size = attributes
                .as_ref()
                .map(|attributes| format_size(attributes.size, true));
            let permissions = attributes
                .as_ref()
                .map(|attributes| attributes.permissions.formatted());
            let owner = attributes
                .as_ref()
                .and_then(|attributes| attributes.owner.clone());

            v_flex()
                .w_full()
                .gap_1()
                .child(inspector_row("Created", created.unwrap_or_default(), cx))
                .child(inspector_row("Modified", modified.unwrap_or_default(), cx))
                .child(inspector_row("Size", size.unwrap_or_default(), cx))
                .child(inspector_tags_row(
                    attributes
                        .as_ref()
                        .map_or(&[][..], |attributes| &attributes.tags),
                    cx,
                ))
                .child(inspector_row(
                    "Permissions",
                    permissions.unwrap_or_default(),
                    cx,
                ))
                .child(inspector_row("Owner", owner.unwrap_or_default(), cx))
                .child(self.inspector_full_path(model.path(), cx))
        };

        // ---- Text Settings ------------------------------------------------
        let text_settings_rows = v_flex()
            .w_full()
            .gap_1()
            .child(inspector_row("Encoding", encoding.name(), cx))
            .child(inspector_row(
                "Line Endings",
                model.line_ending().to_string(),
                cx,
            ))
            // Upstream's `Mode` is the syntax in force — its `Syntax.Kind` label
            // for a built-in kind, the syntax's own name otherwise. SubText's
            // syntaxes are the same bundles, so the name is the same name.
            .child(inspector_row("Mode", document.syntax.to_string(), cx));

        // ---- Count --------------------------------------------------------
        // The count is a cache: it is only recomputed when the text changes, so
        // a repaint that merely moved the caret pays nothing for it. The fallback
        // covers the first frame of a document.
        let count = document
            .counts
            .clone()
            .unwrap_or_else(|| charinfo::CharacterCount::of(&text, encoding.encoding));
        let selected = (!selection.is_empty())
            .then(|| charinfo::CharacterCount::of(&selected_text, encoding.encoding));
        let counts = count.counts(selected.as_ref());
        let (line, column, location) = textops::status_position(&text, selection.start);
        let count_rows = {
            let mut rows = v_flex().w_full().gap_1();
            for (label, count) in COUNT_LABELS.iter().zip(counts) {
                rows = rows.child(inspector_row(*label, count.formatted(), cx));
            }
            // Upstream's counts sit in one `Form.Section` and the positions in
            // a second, so the two groups are separated by the form's own
            // spacing rather than by a rule.
            rows = rows
                .child(inspector_row(POSITION_LABELS[0], location.to_string(), cx))
                .child(inspector_row(POSITION_LABELS[1], line.to_string(), cx))
                .child(inspector_row(POSITION_LABELS[2], column.to_string(), cx));
            rows
        };

        // ---- Character ----------------------------------------------------
        let character = charinfo::CharacterReport::of(&selected_text);
        let character_rows = {
            let mut rows = v_flex().w_full().gap_1();
            if character.is_empty() {
                rows = rows.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Not selected"),
                );
            } else {
                // One code point is labelled in the singular, several in the
                // plural — upstream asks `scalars.count == 1` for the same two
                // strings.
                let label = if character.is_single_scalar() {
                    "Code Point"
                } else {
                    "Code Points"
                };
                // Upstream draws each code point in a rounded box: monospaced
                // digits, two points of horizontal padding, and a
                // `RoundedRectangle(cornerRadius: 3.5).strokeBorder(.tertiary)`
                // around it. A selection of several scalars wraps.
                let mut chips = h_flex().flex_wrap().justify_end().gap_x(px(2.)).gap_y_1();
                for point in character.code_points() {
                    chips = chips.child(
                        div()
                            .px(px(2.))
                            .rounded(px(3.5))
                            .border_1()
                            .border_color(cx.theme().border)
                            .font_family(cx.theme().mono_font_family.clone())
                            .child(point.to_string()),
                    );
                }
                rows = rows.child(inspector_wrapping_row(label, chips, cx));
                if character.is_single_scalar() {
                    let scalar = &character.scalars[0];
                    rows = rows
                        .child(inspector_row("Name", scalar.display_name().to_string(), cx))
                        .child(inspector_row(
                            "Block",
                            scalar.display_block().to_string(),
                            cx,
                        ))
                        .child(inspector_row("Category", scalar.category.to_string(), cx));
                }
            }
            rows
        };

        let bodies = [
            file_rows.into_any_element(),
            text_settings_rows.into_any_element(),
            count_rows.into_any_element(),
            character_rows.into_any_element(),
        ];

        let mut sections = v_flex().w_full().gap_1();
        for (index, body) in bodies.into_iter().enumerate() {
            sections = sections.child(self.inspector_section(index, body, window, cx));
        }
        sections.into_any_element()
    }

    /// One `DisclosureGroup` of the Document inspector.
    ///
    /// Upstream styles its sections with `InspectorDisclosureGroupStyle`: a
    /// 12-point semibold label in the secondary colour, and the disclosure
    /// triangle where `DisclosureGroup` puts it, in the leading position.
    /// `Accordion`'s item renders its chevron trailing, so this is built on the
    /// trigger the accordion itself is built on — [`AccordionTrigger`], which is
    /// deliberately unstyled and leaves the icon and the layout to the
    /// application, while keeping the focus, keyboard and accessibility
    /// behaviour of the component.
    ///
    /// The open state is the workspace's, not the element's: this panel is
    /// redrawn on every keystroke, and an element that owned its own state
    /// would spring back open at the next character typed.
    fn inspector_section(
        &self,
        index: usize,
        body: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.inspector_sections[index];
        // The trigger reports the click through a four-argument handler
        // (open, event, window, app), which `Context::listener` — one event
        // argument — cannot wrap. The workspace is reached through a weak
        // handle instead, which is also what keeps the element free of a
        // borrow of the view it is being rendered into.
        let workspace = cx.entity().downgrade();
        let trigger = AccordionTrigger::new(("inspector-section", index))
            .open(open)
            .h_flex()
            .w_full()
            .gap_1()
            .py_1()
            .on_change(move |open, _, _, cx| {
                workspace
                    .update(cx, |this, cx| {
                        this.inspector_sections[index] = open;
                        cx.notify();
                    })
                    .ok();
            })
            .child(
                // A quarter turn from pointing right to pointing down, which is
                // the pair of positions AppKit's triangle has.
                Icon::new(IconName::ChevronRight)
                    .xsmall()
                    .flex_none()
                    .text_color(cx.theme().muted_foreground)
                    .rotate(percentage(if open { 0.25 } else { 0. })),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child(INSPECTOR_SECTIONS[index]),
            );

        // Upstream's `DisclosureGroup` animates its content; the collapsed
        // panel settles at exactly zero, and a panel at zero is unmounted so a
        // closed section costs no layout.
        let progress = spring(
            gpui_kit::ElementId::from(("inspector-section-body", index)),
            if open { 1. } else { 0. },
            cx.theme().motion_tokens().spring_control,
            window,
            cx,
        );

        v_flex()
            .w_full()
            .child(trigger)
            .child(
                AccordionPanel::new()
                    .id(("inspector-section-panel", index))
                    .open(open)
                    .w_full()
                    .keep_mounted(progress != 0. || cx.reduce_motion())
                    .child(MotionReveal::new(
                        ("inspector-section-content", index),
                        progress,
                        body,
                    )),
            )
            .into_any_element()
    }

    // ---- the Outline inspector -------------------------------------------

    /// The active document's buffer and where its caret is, in one read.
    ///
    /// The buffer is copied out because the caret's line is counted over it and
    /// the tree's staleness is hashed from it, and both want the same copy.
    fn outline_input(&self, cx: &App) -> Option<(String, usize)> {
        let document = self.documents.get(self.active)?;
        let editor = document.editor().read(cx);
        let text = editor.value().to_string();
        let caret = editor.selected_range().start;
        Some((text, caret))
    }

    /// What a tree built right now would be built from.
    fn outline_source(&self, cx: &App, text: &str) -> Option<OutlineSource> {
        let document = self.documents.get(self.active)?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        Some(OutlineSource {
            document: document.id,
            syntax: document.syntax.clone(),
            text: hasher.finish(),
            filter: self.outline_filter.read(cx).value().to_string(),
        })
    }

    /// Whether the tree on screen is out of step with the document.
    ///
    /// This is what lets the inspector refresh itself instead of every place
    /// that can change a buffer having to remember to say so — a keystroke, a
    /// new tab, a re-read from disk, a syntax change and a filter edit all
    /// arrive here as the same answer.
    fn outline_is_stale(&self, cx: &App) -> bool {
        match self.outline_input(cx) {
            Some((text, _)) => match self.outline_source(cx, &text) {
                Some(source) => self.outline_built_from.as_ref() != Some(&source),
                None => self.outline_built_from.is_some(),
            },
            None => self.outline_built_from.is_some(),
        }
    }

    /// The id of the row the caret sits in, if any.
    ///
    /// Both directions of the caret/row link go through here, so they cannot
    /// disagree about which row the caret is in — and the same question, asked
    /// by the same rule, is what the two stepping commands step off.
    fn outline_caret_target(&self, caret: usize) -> Option<&str> {
        crate::outline::entry_at(&self.outline_entries, caret).map(|entry| entry.id.as_str())
    }

    /// Whether a rebuild or a selection move is due.
    ///
    /// One read of the buffer answers both, because `EditorState::value()`
    /// materialises the whole rope on every call — checking the two questions
    /// separately would copy the document twice per keystroke.
    fn outline_needs_refresh(&self, cx: &App) -> bool {
        let Some((text, caret)) = self.outline_input(cx) else {
            return self.outline_built_from.is_some();
        };
        let stale = match self.outline_source(cx, &text) {
            Some(source) => self.outline_built_from.as_ref() != Some(&source),
            None => self.outline_built_from.is_some(),
        };
        stale || self.outline_caret_target(caret) != self.outline_selected.as_deref()
    }

    /// Queue a rebuild, if one is due.
    ///
    /// The inspector is a virtualized list, and its items can only be replaced
    /// by an update — which a render must not do. The work is therefore handed
    /// to the end of the effect cycle, and the flag stops a frame that runs
    /// before that queue drains from queueing a second one.
    fn queue_outline_refresh(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.outline_refresh_queued.get() || !self.outline_needs_refresh(cx) {
            return;
        }
        self.outline_refresh_queued.set(true);
        cx.defer_in(window, |this, _window, cx| {
            this.outline_refresh_queued.set(false);
            if this.outline_is_stale(cx) {
                this.refresh_outline(cx);
            }
            this.sync_outline_selection(cx);
        });
    }

    /// Rebuild the tree from the active document, the filter and the expansion
    /// state.
    ///
    /// Upstream's `rebuildOutline` and `rebuildSnapshot`, plus the `onChange`
    /// that follows them and decides what opens by itself, in one pass.
    fn refresh_outline(&mut self, cx: &mut Context<Self>) {
        let Some((text, _)) = self.outline_input(cx) else {
            self.outline_entries.clear();
            self.outline_nodes.clear();
            self.outline_rows = Rc::new(HashMap::new());
            self.outline_built_from = None;
            self.outline_ids.clear();
            self.outline_selected = None;
            self.outline_tree
                .update(cx, |state, cx| state.set_items(Vec::new(), cx));
            return;
        };
        let Some(source) = self.outline_source(cx, &text) else {
            return;
        };

        let entries = match self.syntaxes.by_name(&source.syntax) {
            Some(syntax) => crate::outline::entries(&text, syntax),
            None => Vec::new(),
        };
        let nodes = crate::outline::build_tree(&entries);
        let all_ids = crate::outline::collect_ids(&nodes);
        let filtered = crate::outline::filtered_tree(&nodes, &source.filter);

        // Which rows open by themselves. Upstream: with no filter, keep the
        // rows that are still there and open the ones that are new; with a
        // filter, open everything that *can* open, because a collapsed match
        // would be invisible.
        if source.filter.is_empty() {
            let current: HashSet<&String> = all_ids.iter().collect();
            self.outline_expanded.retain(|id| current.contains(id));
            for id in &all_ids {
                if !self.outline_ids.contains(id) {
                    self.outline_expanded.insert(id.clone());
                }
            }
        } else {
            self.outline_expanded = crate::outline::expandable_ids(&filtered);
        }

        self.outline_rows = Rc::new(crate::outline::row_map(&filtered));
        self.outline_entries = entries;
        self.outline_nodes = filtered;
        self.outline_ids = all_ids;
        self.outline_built_from = Some(source);
        self.push_outline_items(cx);
        self.sync_outline_selection(cx);
    }

    /// Hand the tree its items, with the expansion state applied.
    fn push_outline_items(&mut self, cx: &mut Context<Self>) {
        let items = outline_items(&self.outline_nodes, &self.outline_expanded);
        self.outline_tree
            .update(cx, |state, cx| state.set_items(items, cx));
        self.restore_outline_selection(cx);
    }

    /// Put the tree's selection where the caret says it should be.
    ///
    /// Replacing the items clears the selection, so every rebuild has to
    /// restore it — which is also what upstream does, by calling
    /// `invalidateCurrentItem` straight after the outline is rebuilt.
    fn restore_outline_selection(&mut self, cx: &mut Context<Self>) {
        let index = self.outline_selected.as_ref().and_then(|id| {
            let id: SharedString = id.clone().into();
            self.outline_tree.read(cx).index_of(&id)
        });
        self.outline_tree
            .update(cx, |state, cx| state.set_selected_index(index, cx));
    }

    /// Follow the caret: the row the caret is in becomes the tree's selection.
    ///
    /// CotEditor's `invalidateCurrentItem`. A row inside a collapsed parent has
    /// no entry to select, so nothing happens — upstream does not open the path
    /// to the caret either.
    fn sync_outline_selection(&mut self, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.outline_input(cx) else {
            return;
        };
        let target = self.outline_caret_target(caret).map(str::to_string);
        if target == self.outline_selected {
            return;
        }
        self.outline_selected = target;
        self.restore_outline_selection(cx);
    }

    /// Move the caret to the row the tree just selected.
    ///
    /// CotEditor's `selectItem(id:)`: selecting an outline row puts the
    /// editor's selection on the range the entry describes and scrolls it into
    /// view. A row the caret already sits in is left alone, which is what keeps
    /// the two directions — caret to row, row to caret — from ping-ponging.
    ///
    /// The range rather than the line, because that is what upstream selects:
    /// clicking `fn main` selects `fn main`, not the start of its line. The
    /// caret lands at the *end* of the selection, which is inside the entry,
    /// so the row that was just clicked is still the row the caret is in.
    fn on_outline_row_selected(
        &mut self,
        tree: Entity<TreeState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = tree
            .read(cx)
            .selected_entry()
            .map(|entry| entry.item().id.to_string());
        let Some(id) = selected else {
            return;
        };
        if self.outline_selected.as_deref() == Some(id.as_str()) {
            return;
        }
        let Some(range) = self
            .outline_entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.range.clone())
        else {
            return;
        };
        self.outline_selected = Some(id);
        self.set_selection(range, cx);
        // Upstream's `selectItem(id:)` ends with
        // `centerSelectionInVisibleArea`, so the entry lands in the middle of
        // the view rather than at whichever edge it came in from.
        self.center_selection_when_settled(window, cx);
    }

    /// The pane's title row, with CotEditor's Options menu at its trailing edge.
    fn render_outline_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // A menu item's handler runs on the popup's context, not the
        // workspace's, so it reaches the workspace through a weak handle rather
        // than through `cx.listener`.
        let workspace = cx.entity().downgrade();
        // The saved *Wrap Lines* preference, which the menu shows but cannot
        // act on — see the item's comment below.
        let wraps = self.settings.outline_wraps_lines;

        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child("Outline"),
            )
            .child(
                Button::new("outline-options")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Ellipsis)
                    .accessibility_label("Options")
                    .tooltip("Options")
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                        let bigger = workspace.clone();
                        let smaller = workspace.clone();
                        let reset = workspace.clone();
                        menu
                            // Upstream's *Wrap Lines* toggle, shown with the
                            // saved preference and disabled: the rows of a
                            // virtualized list are all one height, so a wrapped
                            // row cannot be laid out. See the Outline section of
                            // `docs/PLAN.md`.
                            .item(
                                PopupMenuItem::new("Wrap Lines")
                                    .checked(wraps)
                                    .disabled(true),
                            )
                            .separator()
                            .label("Text Size")
                            .item(
                                PopupMenuItem::new("Bigger").on_click(move |_, _window, cx| {
                                    bigger
                                        .update(cx, |this, cx| this.bump_outline_font(1.0, cx))
                                        .ok();
                                }),
                            )
                            .item(
                                PopupMenuItem::new("Smaller").on_click(move |_, _window, cx| {
                                    smaller
                                        .update(cx, |this, cx| this.bump_outline_font(-1.0, cx))
                                        .ok();
                                }),
                            )
                            .item(PopupMenuItem::new("Reset to Default").on_click(
                                move |_, _window, cx| {
                                    reset
                                        .update(cx, |this, cx| {
                                            this.settings.outline_font_size =
                                                DEFAULT_OUTLINE_FONT_SIZE;
                                            let _ = this.settings.save();
                                            cx.notify();
                                        })
                                        .ok();
                                },
                            ))
                    }),
            )
    }

    /// Change the outline's row font size.
    ///
    /// CotEditor's `biggerFont` / `smallerFont`, including the floor: the
    /// smaller size never goes below the small system font size, which is the
    /// default and what *Reset to Default* returns to.
    fn bump_outline_font(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.settings.outline_font_size =
            (self.settings.outline_font_size + delta).max(DEFAULT_OUTLINE_FONT_SIZE);
        let _ = self.settings.save();
        cx.notify();
    }

    /// The outline itself: a virtualized tree, one row per entry.
    fn render_outline_tree(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let rows = self.outline_rows.clone();
        let font_size = self.settings.outline_font_size;
        let highlight = cx.theme().yellow;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;

        tree(
            &self.outline_tree,
            move |_ix, entry, selected, _window, _cx| {
                let id = entry.item().id.clone();
                let label = entry.item().label.clone();
                let row = rows.get(id.as_ref()).cloned();
                let base = ListItem::new(id)
                    .selected(selected)
                    .w_full()
                    .py_0p5()
                    .px_1()
                    // The outline's own type size is a user setting — Outline's
                    // *Bigger Font* / *Smaller Font* — so it is a pixel value like
                    // the editor's, not part of the interface scale. The row's
                    // *spacing* is rem, which is why an indented row keeps its
                    // proportions when the interface zooms while the text does not.
                    .text_size(px(font_size));

                let Some(row) = row else {
                    return base.child(div().truncate().child(label));
                };

                // A separator is a hairline rather than a label. Upstream draws a
                // thin `Divider`; the rows of a virtualized list are all one height,
                // so the line sits in the middle of a full row instead. One physical
                // pixel is the intent — a rule that scaled with the interface font
                // would stop reading as a rule — so this is a `px` value on purpose,
                // like the settings window's `divider`.
                if row.kind == Some(OutlineKind::Separator) {
                    return base.child(div().w_full().h(px(1.)).bg(border));
                }

                let mut line = h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_1()
                    // One level of nesting is three quarters of the base font, so
                    // the indent keeps its proportion to the row's own text when
                    // the interface scale changes.
                    .pl(rems(entry.depth() as f32 * OUTLINE_INDENT))
                    // The disclosure control. The tree opens and closes a row on a
                    // click anywhere along it, so this shows the state rather than
                    // being the only way to change it — the one place SubText's
                    // outline behaves differently from upstream's.
                    .child(
                        div()
                            // A fixed lane, so a label does not shift sideways as it
                            // gains or loses a disclosure triangle. Same width as one
                            // indent step, which is what makes the two read as one
                            // system rather than two coincidences.
                            .w(rems(OUTLINE_INDENT))
                            .flex_shrink_0()
                            .when(entry.is_folder(), |this| {
                                this.child(
                                    Icon::new(if entry.is_expanded() {
                                        IconName::ChevronDown
                                    } else {
                                        IconName::ChevronRight
                                    })
                                    .xsmall()
                                    .text_color(muted),
                                )
                            }),
                    );

                // A whitespace indent is drawn as it was captured, which is what
                // upstream's `Text(indent)` does.
                if let Some(indent) = row.indent.as_ref() {
                    line = line.child(div().whitespace_nowrap().child(indent.clone()));
                }

                if let Some(icon) = row.icon() {
                    line = line.child(
                        Icon::new(icon)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(row.icon_color()),
                    );
                }

                line = line.child(if row.matches.is_empty() {
                    // One element, so a long label truncates with an ellipsis.
                    div().flex_1().min_w_0().truncate().child(label.clone())
                } else {
                    let mut runs = h_flex()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap();
                    for (range, is_match) in
                        crate::outline::highlight_segments(&label, &row.matches)
                    {
                        let text = label.get(range).unwrap_or_default().to_string();
                        runs = runs.child(if is_match {
                            // Upstream's find-highlight: a bright fill with dark
                            // text. The foreground is a near-black literal rather
                            // than `cx.theme().background`, because the fill is a
                            // fixed bright yellow in *both* appearances — a theme
                            // foreground would be light in a dark theme and vanish
                            // into it. See `MATCH_FOREGROUND`.
                            div().bg(highlight).text_color(MATCH_FOREGROUND).child(text)
                        } else {
                            div().child(text)
                        });
                    }
                    runs
                });

                if row.kind.is_some_and(OutlineKind::is_bold) {
                    line = line.font_weight(FontWeight::SEMIBOLD);
                }

                base.child(line)
            },
        )
        .into_any_element()
    }

    /// The Outline pane: title row, tree, filter field.
    fn render_outline(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        self.queue_outline_refresh(window, cx);

        let filter = self.outline_filter.read(cx).value().to_string();
        let body = if self.outline_nodes.is_empty() {
            let message = if filter.is_empty() {
                // Not upstream: CotEditor shows a plain empty list here.
                // SubText's placeholder predates this pane's tree and is kept.
                "No outline items"
            } else {
                "No Filter Results"
            };
            v_flex()
                .p_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(message),
                )
                .into_any_element()
        } else {
            self.render_outline_tree(cx)
        };

        v_flex()
            .h_full()
            .min_h_0()
            .gap_2()
            .child(self.render_outline_header(cx))
            .child(div().flex_1().min_h_0().child(body))
            .child(Input::new(&self.outline_filter).small().cleanable(true))
            .into_any_element()
    }

    /// The Warnings inspector — upstream's `WarningInspectorView`.
    ///
    /// A `VSplitView` of two panes: the incompatible characters on top and the
    /// inconsistent line endings below, each resizable. They are two scans of
    /// the same buffer answering two different questions — "what would not
    /// survive being saved" and "what is not the line ending the document says
    /// it uses" — and CotEditor keeps them apart rather than merging them into
    /// one list.
    fn render_warnings(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let Some(_) = self.active_document() else {
            return v_flex().into_any_element();
        };

        let mut split = v_resizable("warnings-panes");
        if let Some(panel) = self.render_incompatible_characters(cx) {
            split = split.child(
                resizable_panel()
                    // `resizable_panel().size()` takes pixels, so the design
                    // scale is applied by hand, as in the window's own split.
                    .size(cx.theme().font_size * 9.)
                    .size_range(cx.theme().font_size * 5.0..cx.theme().font_size * 20.)
                    .child(panel),
            );
        }
        split
            .child(resizable_panel().child(self.render_inconsistent_line_endings(cx)))
            .into_any_element()
    }

    /// The *Incompatible Characters* pane.
    ///
    /// `None` while there is nothing to say — the encoding is a Unicode one, so
    /// nothing can be incompatible, or the buffer is empty. Upstream draws the
    /// pane anyway in that case, with its title and "No issues found."; the
    /// title is what tells the reader which of the two lists is which, so it is
    /// drawn either way and only the table is conditional.
    fn render_incompatible_characters(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        let document = self.active_document()?;
        let encoding = document.document.encoding();
        let items = self.incompatible_characters(cx);
        let text = self.active_text(cx).unwrap_or_default();

        let status = if items.is_empty() {
            "No issues found.".to_string()
        } else {
            format!("Found {} incompatible characters.", items.len())
        };

        let mut pane = v_flex()
            .gap_2()
            .h_full()
            .min_h_0()
            .child(section_title("Incompatible Characters", cx))
            .child(
                div()
                    .text_sm()
                    .when(items.is_empty(), |this| {
                        this.text_color(cx.theme().muted_foreground)
                    })
                    .child(status),
            );

        if !items.is_empty() {
            // Upstream shows the character, and for one the encoding cannot
            // spell at all a `�` stands in for it.
            let converted = |item: &subtext_core::IncompatibleCharacter| {
                item.converted
                    .clone()
                    .unwrap_or_else(|| "\u{FFFD}".to_string())
            };
            let _ = encoding;
            let rows: Vec<WarningRow> = items
                .iter()
                .map(|item| {
                    let line = textops::status_position(&text, item.range.start).0;
                    WarningRow {
                        line,
                        // A tab drawn as a tab is a blank cell, so the invisible
                        // ones are spelled as code points, as upstream does.
                        first: character_cell(&item.character),
                        second: converted(item),
                        range: item.range.clone(),
                    }
                })
                .collect();
            pane = pane.child(self.render_warning_table(
                "incompatible",
                WarningTable::Incompatible,
                ["Line", "Character", "Converted"],
                rows,
                self.warnings_sort,
                cx,
            ));
        }

        Some(pane.into_any_element())
    }

    /// The *Inconsistent Line Endings* pane.
    fn render_inconsistent_line_endings(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let Some(document) = self.active_document() else {
            return v_flex().into_any_element();
        };
        let base = document.document.line_ending();
        let text = self.active_text(cx).unwrap_or_default();
        let items = subtext_core::inconsistent_line_endings(&text, base);

        let status = if items.is_empty() {
            "No issues found.".to_string()
        } else {
            format!(
                "Found {} line endings other than {}.",
                items.len(),
                base.abbreviation()
            )
        };

        let mut pane = v_flex()
            .gap_2()
            .h_full()
            .min_h_0()
            .child(section_title("Inconsistent Line Endings", cx))
            .child(
                div()
                    .text_sm()
                    .when(items.is_empty(), |this| {
                        this.text_color(cx.theme().muted_foreground)
                    })
                    .child(status),
            );

        if !items.is_empty() {
            let rows: Vec<WarningRow> = items
                .iter()
                .map(|(range, ending)| {
                    let line = textops::status_position(&text, range.start).0;
                    WarningRow {
                        line,
                        first: ending.abbreviation().to_string(),
                        second: String::new(),
                        range: range.clone(),
                    }
                })
                .collect();
            pane = pane.child(self.render_warning_table(
                "line-endings",
                WarningTable::LineEndings,
                ["Line", "Line Ending", ""],
                rows,
                self.line_endings_sort,
                cx,
            ));
        }

        pane.into_any_element()
    }

    /// One of the inspector's two tables.
    ///
    /// The stateless `Table` rather than the `DataTable`: upstream's tables can
    /// be sorted by clicking a header, but the rows here are a handful of
    /// offenders in a document that is being written in a legacy encoding, and
    /// what matters for the panel is that the two lists are laid out as tables
    /// at all — three columns, a header, a border. Clicking a row selects the
    /// offender in the editor, which is what upstream's selection does.
    ///
    /// The fourth element of each row is the range to select; the third is
    /// unused by the line-ending table, which has only two columns.
    fn render_warning_table(
        &self,
        id: &'static str,
        table: WarningTable,
        headers: [&'static str; 3],
        mut rows: Vec<WarningRow>,
        sort: Option<(WarningColumn, bool)>,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        if let Some((column, descending)) = sort {
            sort_warning_rows(&mut rows, column, descending);
        }
        let columns = if headers[2].is_empty() { 2 } else { 3 };
        let mut header = TableRow::new();
        for (index, label) in headers.iter().take(columns).enumerate() {
            let column = match index {
                0 => WarningColumn::Line,
                1 => WarningColumn::First,
                _ => WarningColumn::Second,
            };
            // The active column carries its direction: an arrow is the whole
            // difference between "sorted this way" and "not sorted".
            let indicator = match sort {
                Some((active, descending)) if active == column => Some(if descending {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronUp
                }),
                _ => None,
            };
            let mut head = TableHead::new().when(index == 0, |this| this.text_right());
            head = head.child(
                div()
                    .id((id, 1000 + index))
                    .w_full()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.toggle_warning_sort(table, column, cx);
                        }),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .when(index == 0, |this| this.justify_end())
                            .child(*label)
                            .when_some(indicator, |this, icon| {
                                this.child(
                                    Icon::new(icon)
                                        .xsmall()
                                        .text_color(cx.theme().muted_foreground),
                                )
                            }),
                    ),
            );
            header = header.child(head);
        }

        // A `TableRow` is not an interactive element, so the click lives on the
        // cell contents — one handler per cell, which makes the whole row
        // clickable rather than just its first column.
        let mut body = TableBody::new();
        for (index, row) in rows.into_iter().enumerate() {
            let selected = self.warnings_selected.as_ref() == Some(&row.range);
            let cells = [row.line.to_string(), row.first, row.second];
            let mut table_row = TableRow::new();
            for (column, text) in cells.into_iter().take(columns).enumerate() {
                let range = row.range.clone();
                table_row = table_row.child(
                    TableCell::new().child(
                        div()
                            .id((id, index * 3 + column))
                            .w_full()
                            .cursor_pointer()
                            .when(column == 0, |this| this.text_right())
                            .when(selected, |this| {
                                this.bg(cx.theme().tokens.table_active).rounded_sm()
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    this.warnings_selected = Some(range.clone());
                                    this.select_range_in_editor(range.clone(), window, cx);
                                    cx.notify();
                                }),
                            )
                            .child(text),
                    ),
                );
            }
            body = body.child(table_row);
        }

        // The table's cells each have a minimum width, so three of them do not
        // fit a narrow inspector. The panel scrolls sideways rather than
        // dropping a column: a warning whose "Converted" half is unreachable is
        // not a warning.
        div()
            .id(id)
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .overflow_x_scroll()
            .overflow_y_scroll()
            .child(
                Table::new()
                    .small()
                    .child(TableHeader::new().child(header))
                    .child(body),
            )
            .into_any_element()
    }

    /// A click on a warnings-table header.
    ///
    /// Same column reverses the direction, another column starts ascending —
    /// SwiftUI's `Table` behaves this way, which is what upstream is.
    fn toggle_warning_sort(
        &mut self,
        table: WarningTable,
        column: WarningColumn,
        cx: &mut Context<Self>,
    ) {
        match table {
            WarningTable::Incompatible => {
                self.warnings_sort = next_sort(self.warnings_sort, column);
            }
            WarningTable::LineEndings => {
                self.line_endings_sort = next_sort(self.line_endings_sort, column);
            }
        }
        cx.notify();
    }

    /// Put the editor's selection on `range` and bring it into view.
    ///
    /// Upstream centres the selection; SubText scrolls it into view, because
    /// `EditorState` has no way to centre a range — the same note the outline's
    /// navigation carries.
    fn select_range_in_editor(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let editor = document.editor().clone();
        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| {
                state.set_selected_range(range, cx);
                let _ = window;
            });
        });
    }

    /// The status bar's encoding picker.
    ///
    /// Upstream's `Picker` over `encodingManager.fileEncodings`, with the
    /// document's own file encoding spliced to the top when the list lacks it —
    /// the same rule the Format menu and the settings picker follow, because it
    /// is the same list.
    ///
    /// Choosing one asks whether to convert or reinterpret
    /// ([`ask_changing_encoding`](Self::ask_changing_encoding)); it does not
    /// silently pick one. The menu items go through the same door.
    fn render_encoding_picker(
        &self,
        document: DocumentId,
        encoding: FileEncoding,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut items = FileEncoding::menu_items(&self.settings.encoding_list);
        if !items.iter().flatten().any(|item| *item == encoding) {
            items.insert(0, Some(encoding));
            items.insert(1, None);
        }
        let workspace = cx.entity().downgrade();

        Button::new("status-encoding")
            .ghost()
            .xsmall()
            .label(encoding.name())
            .accessibility_label("Text Encoding")
            .tooltip("Text Encoding")
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                let mut menu = menu;
                for item in items.iter().copied() {
                    let Some(item) = item else {
                        menu = menu.separator();
                        continue;
                    };
                    let workspace = workspace.clone();
                    menu = menu.item(
                        PopupMenuItem::new(item.name())
                            .checked(item == encoding)
                            .on_click(move |_, window, cx| {
                                workspace
                                    .update(cx, |workspace, cx| {
                                        workspace.ask_changing_encoding(document, item, window, cx);
                                    })
                                    .ok();
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// The status bar's line-ending picker.
    ///
    /// Upstream's `LineEndingPicker`: an unbordered popup whose selected title is
    /// the abbreviation and whose items are the six endings. It is disabled with
    /// the document, because changing the ending rewrites the buffer.
    ///
    /// Upstream titles the items with the abbreviation and puts the long name in
    /// a tooltip; a GPUI menu item has no tooltip, so the description goes in
    /// the label instead — the same compromise the Format ▸ Line Endings submenu
    /// already makes.
    fn render_line_ending_picker(
        &self,
        current: LineEnding,
        editable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity().downgrade();

        Button::new("status-line-ending")
            .ghost()
            .xsmall()
            .label(current.abbreviation())
            .disabled(!editable)
            .accessibility_label("Line Endings")
            .tooltip("Line Endings")
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                let mut menu = menu;
                for ending in subtext_core::line_ending::ALL {
                    let workspace = workspace.clone();
                    menu = menu.item(
                        PopupMenuItem::new(format!(
                            "{} ({})",
                            ending.description(),
                            ending.abbreviation()
                        ))
                        .checked(ending == current)
                        .on_click(move |_, window, cx| {
                            workspace
                                .update(cx, |workspace, cx| {
                                    workspace.set_line_ending(ending, window, cx)
                                })
                                .ok();
                        }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// Ask whether to convert or reinterpret, then do it.
    ///
    /// Upstream's `Document.askChangingEncoding`: when the document has no file
    /// or holds no text there is nothing to lose, so it just changes; otherwise
    /// it asks with an alert whose buttons are *Convert*, *Reinterpret* and
    /// *Cancel*.
    ///
    /// SubText used to convert silently — from the menu as well as from the
    /// status bar — but upstream's `changeEncoding(_:)` is one action behind
    /// both entry points, and it always asks.
    ///
    /// **One deviation, recorded**: upstream warns about a lossy conversion in a
    /// *second* sheet, whose only extra option is *Change Encoding* — that is,
    /// "do it anyway". The warning is folded into this alert instead, so the
    /// user sees it before choosing rather than after, and there is one surface
    /// rather than two. The characters that cannot survive are the same either
    /// way; the warning is `TextEncoding::encode`'s own error flag, which is what
    /// the write path would hit.
    pub fn ask_changing_encoding(
        &mut self,
        document: DocumentId,
        encoding: FileEncoding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.documents.iter().find(|open| open.id == document) else {
            return;
        };
        let current = FileEncoding::of_document(open.document.encoding(), open.document.bom());
        if current == encoding {
            return;
        }
        // Nothing on disk and nothing typed: there is no text to convert and no
        // interpretation to change, so upstream changes it without asking.
        let nothing_to_lose = open.document.path().is_none() || open.document.text().is_empty();
        if nothing_to_lose {
            self.change_encoding(encoding, window, cx);
            return;
        }

        let lossy = encoding.encoding.encode(open.document.text()).1;
        let name = encoding.name();
        let body = if lossy {
            format!(
                "The document contains characters incompatible with “{name}”. \
                 Incompatible characters are either substituted or removed in saving. \
                 Do you want to convert or reinterpret this document using “{name}”?"
            )
        } else {
            format!("Do you want to convert or reinterpret this document using “{name}”?")
        };

        let workspace = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, _| {
            let convert = workspace.clone();
            let reinterpret = workspace.clone();
            dialog
                .title("Text encoding change")
                .w(window.rem_size() * 26.)
                .child(div().text_sm().child(body.clone()))
                .footer(
                    DialogFooter::new()
                        // Escape and the dialog's close button both cancel, and
                        // a dialog dismisses without a callback of its own.
                        .child(DialogClose::new().trigger(|button| button.label("Cancel")))
                        .child(
                            Button::new("encoding-reinterpret")
                                .label("Reinterpret")
                                .on_click(move |_, window, cx| {
                                    reinterpret
                                        .update(cx, |workspace, cx| {
                                            workspace.reinterpret_encoding(
                                                encoding.encoding,
                                                window,
                                                cx,
                                            );
                                        })
                                        .ok();
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("encoding-convert")
                                .label("Convert")
                                .primary()
                                .on_click(move |_, window, cx| {
                                    convert
                                        .update(cx, |workspace, cx| {
                                            workspace.change_encoding(encoding, window, cx);
                                        })
                                        .ok();
                                    window.close_dialog(cx);
                                }),
                        ),
                )
                // Enter reaches the dialog's own confirm action; there is no
                // separate one here, and the three buttons say what they do.
                .on_ok(|_, _, _| false)
        });
    }

    /// The navigation bar above one editor pane.
    ///
    /// Upstream's `EditorViewController` is itself a split view whose first item
    /// is this bar, so every pane has its own: *Close Split Editor*, a rule, the
    /// outline navigator, a spacer, and *Split Editor* with its orientation menu
    /// on the context menu. The whole bar is 24 points tall on the window
    /// background with a four-point inset.
    ///
    /// It used to be one bar at the bottom of the window showing the syntax and
    /// the line ending, which is not what upstream's navigation bar is at all —
    /// those two belong to the status bar's pickers.
    fn render_navigation_bar(&self, pane: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(document) = self.active_document() else {
            return div().into_any_element();
        };
        let can_close = document.pane_count() > 1;
        let is_vertical = self.settings.split_view_vertical;

        let (entries, selection) = self.nav_outline(cx);
        // The two buttons are greyed the way upstream's are — by asking the same
        // walk that the commands use, rather than by guessing from the index.
        let can_previous = crate::outline::previous_item(&entries, &selection).is_some();
        let can_next = crate::outline::next_item(&entries, &selection).is_some();

        let workspace = cx.entity().downgrade();
        let close = workspace.clone();
        let split = workspace.clone();
        let orientation = workspace.clone();

        h_flex()
            .w_full()
            .h(rems(1.5))
            .flex_shrink_0()
            .gap_1()
            .px_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                Button::new("nav-close-split")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Close)
                    .disabled(!can_close)
                    .accessibility_label("Close Split Editor")
                    .tooltip("Close split editor")
                    .on_click(move |_, window, cx| {
                        // Upstream sends the action from the bar's own text view
                        // and the responder chain resolves it to *that* pane.
                        // GPUI has no responder chain, so the pane is named
                        // first and the command then acts on the active one.
                        close
                            .update(cx, |workspace, cx| {
                                workspace.focus_pane(pane, window, cx);
                                workspace.close_split_editor(window, cx);
                            })
                            .ok();
                    }),
            )
            .when(can_close && !entries.is_empty(), |this| {
                this.child(div().w(px(1.)).h(rems(0.75)).bg(cx.theme().border))
            })
            .when(!entries.is_empty(), |this| {
                this.child(
                    h_flex()
                        .gap_0p5()
                        .child(
                            Button::new("nav-outline-previous")
                                .ghost()
                                .xsmall()
                                .icon(if is_vertical {
                                    IconName::ChevronLeft
                                } else {
                                    IconName::ChevronUp
                                })
                                .disabled(!can_previous)
                                .accessibility_label("Previous Outline Item")
                                .tooltip("Jump to previous outline item")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.select_previous_outline_item(cx)
                                })),
                        )
                        .child(
                            Button::new("nav-outline-next")
                                .ghost()
                                .xsmall()
                                .icon(if is_vertical {
                                    IconName::ChevronRight
                                } else {
                                    IconName::ChevronDown
                                })
                                .disabled(!can_next)
                                .accessibility_label("Next Outline Item")
                                .tooltip("Jump to next outline item")
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.select_next_outline_item(cx)),
                                ),
                        )
                        .child(self.render_outline_menu(&entries, selection.clone(), cx)),
                )
            })
            .child(div().flex_1())
            .child(
                Button::new("nav-split")
                    .ghost()
                    .xsmall()
                    .icon(
                        // Upstream draws one symbol and turns it a quarter turn
                        // when the split is vertical. `Icon::rotate` takes a
                        // fraction of a full turn, so this is that same -90°.
                        Icon::new(IconName::SquareSplitHorizontal)
                            .rotate(percentage(if is_vertical { -0.25 } else { 0. })),
                    )
                    .accessibility_label("Split Editor")
                    .tooltip("Split editor")
                    .on_click(move |_, window, cx| {
                        split
                            .update(cx, |workspace, cx| {
                                workspace.focus_pane(pane, window, cx);
                                workspace.open_split_editor(window, cx);
                            })
                            .ok();
                    })
                    // Upstream puts the orientation on the button's *context*
                    // menu, so the click still splits.
                    .context_menu(move |menu, _window, _cx| {
                        let orientation = orientation.clone();
                        menu.item(
                            PopupMenuItem::new(if is_vertical {
                                "Stack Editors Horizontally"
                            } else {
                                "Stack Editors Vertically"
                            })
                            .on_click(move |_, _, cx| {
                                orientation
                                    .update(cx, |workspace, cx| {
                                        workspace.toggle_split_orientation(cx);
                                    })
                                    .ok();
                            }),
                        )
                    }),
            )
            .into_any_element()
    }

    /// The outline menu in the navigation bar: every entry, jump on choose.
    ///
    /// Upstream's `OutlinePicker`, an AppKit popup it opens programmatically.
    /// The label carries the entry's own indentation, which is what the picker
    /// shows — the outline's titles are pre-indented by the extractor.
    fn render_outline_menu(
        &self,
        entries: &[OutlineEntry],
        selection: Range<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity().downgrade();
        let current = crate::outline::index_of_item(entries, selection.start);
        // The menu builder runs after this frame, so the rows are owned rather
        // than borrowed from the entries.
        let rows: Vec<(String, Range<usize>)> = entries
            .iter()
            .map(|entry| (entry.title.clone(), entry.range.clone()))
            .collect();

        Button::new("nav-outline-menu")
            .ghost()
            .xsmall()
            .icon(IconName::List)
            .accessibility_label("Outline Menu")
            .tooltip("Outline Menu")
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, _cx| {
                let mut menu = menu;
                for (index, (title, range)) in rows.iter().enumerate() {
                    let workspace = workspace.clone();
                    let range = range.clone();
                    menu = menu.item(
                        PopupMenuItem::new(title.clone())
                            .checked(Some(index) == current)
                            .on_click(move |_, _, cx| {
                                workspace
                                    .update(cx, |workspace, cx| {
                                        workspace.set_selection(range.clone(), cx);
                                    })
                                    .ok();
                            }),
                    );
                }
                menu.max_h(window.rem_size() * 18.).scrollable(true)
            })
            .into_any_element()
    }

    /// The outline entries and the selection, for the navigation bar.
    ///
    /// Extracted rather than read from `outline_rows`: those are built while the
    /// Outline inspector renders, and the navigation bar is there whether or not
    /// the inspector is open. Upstream reads `syntaxController.outlineItems`,
    /// which is likewise independent of the inspector.
    fn nav_outline(&self, cx: &App) -> (Vec<OutlineEntry>, Range<usize>) {
        let selection = self.active_selection(cx);
        let Some(document) = self.active_document() else {
            return (Vec::new(), selection);
        };
        let Some(syntax) = self.syntaxes.by_name(&document.syntax) else {
            return (Vec::new(), selection);
        };
        let text = document.editor().read(cx).value().to_string();
        (crate::outline::entries(&text, syntax), selection)
    }

    /// The status bar, as upstream's `StatusBar` lays it out.
    ///
    /// `[not-editable badge] [counts] … [file size] │ [encoding] │ [line endings]`
    ///
    /// It belongs to the **content** column, not the window: upstream hangs it
    /// off the content split item as a bottom-aligned accessory, so it stops at
    /// the sidebar's edge while the sidebar and the inspector run the full
    /// height of the window. It used to be a bar across the whole window here.
    ///
    /// The six *Status Bar* switches in the Window pane pick which of the six
    /// counts show. Until this rewrite nothing read them, so they were six
    /// switches that changed nothing — the same defect the byte-order-mark
    /// switch had (see `docs/PLAN.md` §8.14).
    /// Take the status bar's counts again if the text changed since last time.
    ///
    /// Counting a megabyte costs 24 ms in release. The status bar is rebuilt on
    /// every repaint — caret moves and the caret's blink included — so the
    /// answer is kept on the document and only thrown away when an edit makes
    /// it wrong.
    fn ensure_status_counts(&mut self, cx: &mut Context<Self>) {
        let index = self.active;
        let Some(document) = self.documents.get(index) else {
            return;
        };
        if document.counts.is_some() {
            return;
        }
        let (text, encoding) = {
            let state = document.editor().read(cx);
            let encoding =
                FileEncoding::of_document(document.document.encoding(), document.document.bom())
                    .encoding;
            (state.value().to_string(), encoding)
        };
        let counts = charinfo::CharacterCount::of(&text, encoding);
        if let Some(document) = self.documents.get_mut(index) {
            document.counts = Some(counts);
        }
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let Some(document) = self.active_document() else {
            return div().into_any_element();
        };
        let encoding =
            FileEncoding::of_document(document.document.encoding(), document.document.bom());
        let line_ending = document.document.line_ending();
        let file_size = document.document.file_size();
        let editable = self.settings.editor.editable;

        let (text, selection) = {
            let state = document.editor().read(cx);
            (state.value().to_string(), state.selected_range())
        };

        let count = charinfo::CharacterCount::of(&text, encoding.encoding);
        // Upstream shows `entire (selected)` while a selection is up, which is
        // the whole reason the counts are worth a glance mid-edit. The rule for
        // that lives with the pair of numbers, in `charinfo::Count`, because
        // the inspector's Count section draws the same three pairs.
        let selected = (!selection.is_empty()).then(|| {
            let slice = text.get(selection.start..selection.end).unwrap_or_default();
            charinfo::CharacterCount::of(slice, encoding.encoding)
        });
        let counts = count.counts(selected.as_ref());
        let (line, column, location) = textops::status_position(&text, selection.start);

        let window = self.settings.window;
        let mut items: Vec<(bool, &str, String)> = vec![
            (
                window.show_status_bar_lines,
                COUNT_LABELS[0],
                counts[0].formatted(),
            ),
            (
                window.show_status_bar_chars,
                COUNT_LABELS[1],
                counts[1].formatted(),
            ),
            (
                window.show_status_bar_words,
                COUNT_LABELS[2],
                counts[2].formatted(),
            ),
            (
                window.show_status_bar_location,
                POSITION_LABELS[0],
                location.to_string(),
            ),
            (
                window.show_status_bar_line,
                POSITION_LABELS[1],
                line.to_string(),
            ),
            (
                window.show_status_bar_column,
                POSITION_LABELS[2],
                column.to_string(),
            ),
        ];
        items.retain(|(shown, _, _)| *shown);

        // SubText's own transient notices share this row. Upstream has no such
        // line — it uses an alert — so the notice *replaces* the counts for the
        // four seconds it lives rather than adding a second lane, which keeps
        // the layout identical to upstream's whenever nothing is being said.
        let notice = self
            .status
            .as_ref()
            .filter(|status| status.at.elapsed() <= STATUS_LIFETIME);

        h_flex()
            .w_full()
            .flex_shrink_0()
            .gap_2()
            .px_3()
            .py_1()
            .text_xs()
            .border_t_1()
            .border_color(cx.theme().border)
            .when(!editable, |this| {
                this.child(
                    // Upstream's `NotEditableBadge` is a `Label` with a help
                    // tooltip. A disabled icon button is the closest thing that
                    // can carry one, and it reads the same: an inert pencil.
                    Button::new("status-not-editable")
                        .ghost()
                        .xsmall()
                        .icon(IconName::PenOff)
                        .disabled(true)
                        .accessibility_label("Not editable")
                        .tooltip("The document is not editable."),
                )
            })
            .child(
                h_flex()
                    .gap_3()
                    .min_w_0()
                    .truncate()
                    .text_color(cx.theme().muted_foreground)
                    .when_some(notice, |this, status| {
                        this.child(div().truncate().child(status.message.clone()))
                    })
                    .when(notice.is_none(), |this| {
                        this.children(
                            items
                                .into_iter()
                                .map(|(_, label, value)| div().child(format!("{label}: {value}"))),
                        )
                    }),
            )
            .child(div().flex_1())
            .when_some(file_size, |this, size| {
                this.child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(format_size(size, false)),
                )
            })
            .child(div().w(px(1.)).h(rems(0.75)).bg(cx.theme().border))
            .child(self.render_encoding_picker(document.id, encoding, cx))
            .child(div().w(px(1.)).h(rems(0.75)).bg(cx.theme().border))
            .child(self.render_line_ending_picker(line_ending, editable, cx))
            .into_any_element()
    }

    /// The editor panes of the active document.
    ///
    /// One pane is the document itself. Two or more are stacked inside a
    /// resizable group — vertically by default, side by side when
    /// *Stack Editors …* has been used — with a draggable divider between
    /// each pair, which is what `NSSplitViewController` gives CotEditor.
    ///
    /// Every pane is a wrapper `div` around the editor so that a click can
    /// name the pane it landed in: the editor handles its own caret placement,
    /// and the wrapper's listener does not stop the event, so the click both
    /// focuses the editor and tells the workspace which pane commands now act
    /// on.
    fn render_editor_area(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let Some(document) = self.active_document() else {
            return div().flex_1().into_any_element();
        };

        let font_family = self
            .settings
            .font_family
            .clone()
            .unwrap_or_else(|| cx.theme().mono_font_family.to_string());
        // The document's own type size is user data, not application zoom: it
        // is what Format ▸ Bigger Font changes, and it must not move when the
        // interface scale does.
        let text_size = px(self.settings.font_size);
        let document_id = document.id;

        let panes: Vec<gpui_kit::AnyElement> = document
            .editors()
            .enumerate()
            .map(|(pane, editor)| {
                let editor = editor.clone();
                // The drop is handled by the pane the pointer is over, not by
                // the focused one: dragging a file onto the right-hand editor
                // should put it there. The workspace then decides what to
                // insert, and `drop_files` reads the *active* pane's buffer, so
                // the click that a drop begins with has already moved the focus
                // — the same ordering CotEditor relies on.
                let workspace = cx.entity().downgrade();
                v_flex()
                    .id(ElementId::NamedInteger(
                        "editor-pane".into(),
                        (document_id.0 << 8) | pane as u64,
                    ))
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .h_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| this.focus_pane(pane, window, cx)),
                    )
                    .on_drop::<ExternalPaths>(move |paths, _, cx| {
                        workspace
                            .update_in(cx, |workspace, window, cx| {
                                workspace.drop_files(paths.0.as_slice(), window, cx);
                            })
                            .ok();
                    })
                    // The navigation bar is per pane, not per window: upstream's
                    // `EditorViewController` is a split view whose first item is
                    // this bar, so each pane carries its own close and split
                    // buttons and its own outline navigator.
                    .when(self.settings.window.show_navigation_bar, |this| {
                        this.child(self.render_navigation_bar(pane, cx))
                    })
                    .child(
                        // `Editor`, not `Input`: the styled code editor, whose
                        // state carries line numbers, folding and the
                        // highlighter. `Input` only takes an `InputState`.
                        Editor::new(&editor)
                            .bordered(false)
                            .p_2()
                            .flex_1()
                            .min_h_0()
                            .font_family(font_family.clone())
                            .text_size(text_size)
                            .into_any_element(),
                    )
                    .into_any_element()
            })
            .collect();

        if panes.len() < 2 {
            return v_flex()
                .flex_1()
                .h_full()
                .min_w_0()
                .children(panes)
                .into_any_element();
        }

        let group = if self.settings.split_view_vertical {
            h_resizable(document_id.split_group())
        } else {
            v_resizable(document_id.split_group())
        };
        group
            .children(panes.into_iter().map(|pane| resizable_panel().child(pane)))
            .into_any_element()
    }
}

/// A byte count as a file size, the way the platform's `ByteCountFormatStyle`
/// does with `style: .file`.
///
/// Decimal units — 1 kB is 1000 bytes — and one decimal place below ten, none
/// above, which is what `.formatted(.byteCount(style: .file))` produces.
///
/// `includes_actual_byte_count` appends the exact count in parentheses. That is
/// what the *document inspector* shows; the status bar shows the same number
/// without it.
pub fn format_size(bytes: u64, includes_actual_byte_count: bool) -> String {
    const UNITS: [(&str, f64); 4] = [("kB", 1e3), ("MB", 1e6), ("GB", 1e9), ("TB", 1e12)];
    let value = bytes as f64;
    let scaled = if value < 1e3 {
        format!("{bytes} bytes")
    } else {
        let (unit, scale) = UNITS
            .iter()
            .rev()
            .find(|(_, scale)| value >= *scale)
            .copied()
            .unwrap_or(("kB", 1e3));
        let value = value / scale;
        if value < 10.0 {
            format!("{value:.1} {unit}")
        } else {
            format!("{value:.0} {unit}")
        }
    };

    if includes_actual_byte_count {
        format!("{scaled} ({bytes} bytes)")
    } else {
        scaled
    }
}

/// The heading over one of the Warnings inspector's two lists.
///
/// Upstream draws both as 12-point semibold text in the secondary colour —
/// the same style the Document inspector's four sections use, and the reason
/// the two lists read as two things rather than one.
fn section_title(label: &'static str, cx: &App) -> impl IntoElement {
    div()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(label)
}

/// One `LabeledContent` row of the Document inspector.
///
/// Upstream pairs a label with its value inside a grouped `Form`, which puts
/// the label in a leading column and the value against the trailing edge. This
/// is that pair. The label never shrinks, so it is never the half that gets
/// cut off; the value takes what is left and truncates, which is what keeps a
/// long full path from pushing its own label out of a 12-rem pane.
fn inspector_row(
    label: impl Into<SharedString>,
    value: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .gap_3()
        .items_start()
        .justify_between()
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(div().min_w_0().text_xs().truncate().child(value))
}

/// A serif face for the character the inspector draws large.
///
/// Upstream asks AppKit for the system font with `.serif` design, which is
/// *New York* on macOS. GPUI takes a family name rather than a design, so this
/// names the face each platform ships: New York where it exists, Georgia on
/// Windows, DejaVu Serif on the rest. A machine that has none of them falls
/// back to the interface font — which is what this was before.
fn serif_family() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "New York"
    }
    #[cfg(target_os = "windows")]
    {
        "Georgia"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "DejaVu Serif"
    }
}

/// A labelled row whose value wraps instead of truncating.
///
/// The inspector's rows are one line each, which is what the File and Count
/// sections want. The code points are a row of chips that has to break when the
/// selection has more scalars than fit — upstream puts them in a
/// `WrappingHStack`, so this is the same shape without the truncation.
fn inspector_wrapping_row(
    label: impl Into<SharedString>,
    value: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .gap_3()
        .items_start()
        .justify_between()
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(div().min_w_0().text_xs().child(value))
}

/// The File section's *Tags* row.
///
/// Upstream right-aligns a **wrapping** stack of colour dot + name pairs
/// (`WrappingHStack(alignment: .trailing, horizontalSpacing: 7)`), and draws an
/// en dash in the tertiary colour when the file has none. The dot is a filled
/// circle at 82% opacity with a ring of its own colour — or, for a tag with no
/// colour, a hollow circle ringed in the secondary colour.
fn inspector_tags_row(tags: &[fileattrs::FinderTag], cx: &App) -> impl IntoElement {
    let value: gpui_kit::AnyElement = if tags.is_empty() {
        div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("–")
            .into_any_element()
    } else {
        let mut list = h_flex()
            .flex_wrap()
            .justify_end()
            // Upstream's `horizontalSpacing: 7` and the 4 points between a dot
            // and its name.
            .gap_x(rems(0.4375))
            .gap_y_1();
        for tag in tags {
            let dot = match tag.color.rgb() {
                Some((red, green, blue)) => {
                    let colour = gpui_kit::Rgba {
                        r: red as f32 / 255.,
                        g: green as f32 / 255.,
                        b: blue as f32 / 255.,
                        a: 0.82,
                    };
                    let colour: gpui_kit::Hsla = colour.into();
                    div()
                        .w(px(9.))
                        .h(px(9.))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(colour)
                        .border_1()
                        .border_color(colour)
                }
                None => div()
                    .w(px(9.))
                    .h(px(9.))
                    .flex_shrink_0()
                    .rounded_full()
                    .border_1()
                    .border_color(cx.theme().muted_foreground),
            };
            list = list.child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(dot)
                    .child(div().text_xs().child(tag.name.clone())),
            );
        }
        list.into_any_element()
    };

    h_flex()
        .w_full()
        .gap_3()
        .items_start()
        .justify_between()
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Tags"),
        )
        .child(div().min_w_0().child(value))
}

/// Hand `path` to the platform's file manager, with the file selected.
///
/// Upstream calls `NSWorkspace.activateFileViewerSelecting`, which is the same
/// idea on each of the three systems and a one-line command on each of them
/// too. Failure is ignored: the worst case is that nothing comes to the front.
fn reveal_in_file_manager(path: &Path) {
    let (program, arguments) = if cfg!(target_os = "macos") {
        ("open", vec![std::ffi::OsString::from("-R"), path.into()])
    } else if cfg!(target_os = "windows") {
        (
            "explorer",
            vec![std::ffi::OsString::from(format!(
                "/select,{}",
                path.display()
            ))],
        )
    } else {
        // The desktop environments have no "select this file" switch; opening
        // the containing folder is the closest thing that is not a dependency.
        let folder = path.parent().unwrap_or(path);
        ("xdg-open", vec![folder.into()])
    };
    let _ = std::process::Command::new(program).args(arguments).spawn();
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_status_counts(cx);
        let title = self
            .active_document()
            .map_or_else(|| "SubText".to_string(), |document| document.title());
        window.set_window_title(&title);

        let mut root = v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            // Escape dismisses the Inspect Character popover. The capture phase
            // is the only place the workspace can see that key: the popover
            // deliberately does not take focus — upstream's does not either, and
            // a popover that stole the caret would end the edit it is meant to
            // describe — so the editor would otherwise be first in line.
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                // The completion popup takes the keys a list needs, and lets
                // everything else through: typing another character has to
                // reach the editor, because that is what re-filters the list.
                if this.completion.is_some() {
                    match event.keystroke.key.as_str() {
                        "up" => {
                            this.move_completion(false, cx);
                            window.prevent_default();
                            return;
                        }
                        "down" => {
                            this.move_completion(true, cx);
                            window.prevent_default();
                            return;
                        }
                        "enter" | "tab" => {
                            this.apply_completion(window, cx);
                            window.prevent_default();
                            return;
                        }
                        "escape" => {
                            this.cancel_completion(cx);
                            window.prevent_default();
                            return;
                        }
                        // A horizontal move abandons the list and moves the
                        // caret, which is AppKit's "treat as cancelled"
                        // movement — so the key is *not* taken from the editor.
                        "left" | "right" | "home" | "end" => this.cancel_completion(cx),
                        _ => {}
                    }
                }
                if this.inspect_character.is_some() && event.keystroke.key == "escape" {
                    this.inspect_character = None;
                    window.prevent_default();
                    cx.notify();
                }
            }));

        if self.settings.window.show_toolbar {
            root = root.child(self.render_toolbar(cx));
        }
        if self.settings.window.show_tab_bar && self.documents.len() > 1 {
            root = root.child(self.render_tab_bar(cx));
        }

        let editor_area = self.render_editor_area(cx);

        // The middle row is a split group, not three fixed-width columns:
        // upstream's `WindowContentViewController` is an `NSSplitViewController`
        // whose sidebar and inspector items are both draggable and whose split
        // view autosaves its position, and a sidebar the user cannot widen is a
        // sidebar that cannot show a long file name. The widths come from the
        // settings and go back there when a drag ends, which is what the
        // autosave name does upstream.
        //
        // The group's element id carries which panes are showing. `ResizableState`
        // keys its sizes by group id *and panel position*, so a single id for all
        // four configurations would hand the sidebar's width to whatever pane
        // moved into slot 0 when the sidebar is hidden — the editor would come
        // back at fifteen rem. One id per configuration gives each its own sizes,
        // and each starts from the widths in the settings.
        let sidebar = self.settings.window.show_sidebar;
        let inspector = self.settings.window.show_inspector;
        // `resizable_panel().size()` takes `impl Into<Pixels>`, not a `Length`,
        // so the design scale is applied by hand here the way `Dialog::w` does.
        // Recomputing each frame is what keeps a dragged width honest when the
        // interface scale changes.
        let rem = window.rem_size().as_f32();
        let mut panels = Vec::new();
        if sidebar {
            panels.push(
                resizable_panel()
                    .size(px(self.settings.window.sidebar_width * rem))
                    .size_range(px(SIDEBAR_MIN_WIDTH * rem)..px(SIDEBAR_MAX_WIDTH * rem))
                    // Fixed rather than growing: the document takes the surplus
                    // when the window widens, which is what upstream's sidebar
                    // item does and what keeps the reading column the widest
                    // thing on screen.
                    .flex_none()
                    .child(self.render_sidebar(cx)),
            );
        }
        // The content column is the editor *and* the status bar: upstream hangs
        // the bar off the content split item as a bottom-aligned accessory, so
        // it spans the document and stops at the sidebar's edge, while the
        // sidebar and the inspector run the full height of the window. A bar
        // across the whole window would put it under them too, which is what
        // this used to do.
        // The advanced counter's capsule floats over the text, which is where
        // upstream puts it — a `DraggableHostingView` pinned to the top-right
        // of the editor, over the text rather than beside it. `relative()` is
        // what makes the `absolute()` capsule inside it measure against this
        // column rather than against the window.
        let count_capsule = self.render_count_capsule(window.rem_size(), cx);
        panels.push(
            resizable_panel().child(
                v_flex()
                    .relative()
                    .h_full()
                    .min_h_0()
                    .child(editor_area)
                    .when(self.settings.window.show_status_bar, |this| {
                        this.child(self.render_status_bar(cx))
                    })
                    .children(count_capsule),
            ),
        );
        if inspector {
            panels.push(
                resizable_panel()
                    .size(px(self.settings.window.inspector_width * rem))
                    .size_range(px(INSPECTOR_MIN_WIDTH * rem)..px(INSPECTOR_MAX_WIDTH * rem))
                    .flex_none()
                    .child(self.render_inspector(window, cx)),
            );
        }

        let group = h_resizable(middle_group_id(sidebar, inspector))
            .children(panels)
            .on_resize({
                let workspace = cx.entity().downgrade();
                move |state, _, cx| {
                    // Read the resolved widths and release the borrow before
                    // touching the workspace: `sizes()` hands back a slice of
                    // the state we are about to notify through.
                    //
                    // Rem comes from the theme's base font, which is what the
                    // Component Root hands the window as its rem size; an effect
                    // has no window to ask.
                    let rem = cx.theme().font_size.as_f32();
                    let (sidebar_width, inspector_width) = {
                        let state = state.read(cx);
                        let sizes = state.sizes();
                        (
                            sizes.first().map(|size| size.as_f32() / rem),
                            sizes.last().map(|size| size.as_f32() / rem),
                        )
                    };
                    workspace
                        .update(cx, |workspace, _| {
                            // `sizes()` is positional, so the first entry is the
                            // sidebar only when the sidebar is showing and the
                            // last is the inspector only when it is. Writing a
                            // width for a hidden pane would be writing the
                            // editor's.
                            //
                            // Clamped on the way in as well as by the panel's
                            // own range: the value is about to become a file on
                            // disk, and a hand-edited one must not come back as
                            // a sidebar wider than the window.
                            let window = &mut workspace.settings.window;
                            if let Some(width) = sidebar.then_some(sidebar_width).flatten() {
                                window.sidebar_width =
                                    width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                            }
                            if let Some(width) = inspector.then_some(inspector_width).flatten() {
                                window.inspector_width =
                                    width.clamp(INSPECTOR_MIN_WIDTH, INSPECTOR_MAX_WIDTH);
                            }
                            let _ = workspace.settings.save();
                        })
                        .ok();
                }
            });

        // The group fills the row it is given; the wrapper is what asks for the
        // remaining height, because `ResizablePanelGroup` is not `Styled` and so
        // cannot take `flex_1()` itself.
        root = root.child(div().flex_1().min_h_0().child(group));

        // No navigation bar and no status bar here: upstream puts the status bar
        // inside the content column (above) and the navigation bar inside each
        // editor pane (`render_editor_area`). A bar at this level would span the
        // sidebar and the inspector as well, which neither of upstream's does.
        //
        // The Inspect Character popover is the last child, so it is laid out
        // after everything it points at and painted over it. Upstream's is
        // `.semitransient`: the first click outside dismisses it, and that
        // click does not reach what is underneath. The full-size element behind
        // the popover is that dismissal.
        // The completion popup, on the same layer and for the same reason: it
        // is anchored to a rectangle in the editor and has to be painted over
        // it. It is added first so the Inspect Character popover, if both were
        // somehow up, would be the one on top.
        if let Some(completion) = self.render_completion(window.rem_size(), cx) {
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    // A click anywhere else dismisses it, as a popup's overlay
                    // does. The list itself is a later child, so it is on top.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.cancel_completion(cx)),
                    )
                    .child(completion),
            );
        }

        if let Some(popover) = self.render_inspect_character(window.rem_size(), cx) {
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(div().absolute().inset_0().occlude().on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.inspect_character = None;
                            cx.notify();
                        }),
                    ))
                    .child(popover),
            );
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

    /// The folder the active document lives in.
    ///
    /// CotEditor's Folder Find searches the document's folder because a folder
    /// opened as a document *is* the document. SubText has no directory
    /// documents, so the folder of the file on screen is the same answer for
    /// every document that has one — and an unsaved document has none.
    pub fn active_directory(&self) -> Option<PathBuf> {
        self.active_document()?
            .document
            .path()
            .and_then(|path| path.parent())
            .map(PathBuf::from)
    }

    /// Open a file and put the caret on `range` in it.
    ///
    /// Used by Folder Find: clicking a match opens its file and selects the
    /// hit, so the result list doubles as a jump list. The selection is only
    /// moved when the file actually became the active document — a result in a
    /// file that cannot be opened leaves the current document alone.
    pub fn open_and_select(
        &mut self,
        path: PathBuf,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_path(path.clone(), window, cx);
        let arrived = self
            .active_document()
            .is_some_and(|document| document.document.path() == Some(path.as_path()));
        if arrived {
            self.set_selection(range.clone(), cx);
            // Upstream's folder find flashes the hit it jumped to, the same
            // mark a find step leaves.
            self.flash_find_indicator(range, cx);
        }
    }
}

/// Turn the filtered tree into the items the `Tree` component renders.
///
/// The tree keeps its expansion state in the items themselves, and the
/// workspace mirrors it from the `Expanded` / `Collapsed` events. A rebuild
/// therefore has to hand the state back with the items, or replacing them would
/// close the whole outline every time a keystroke lands.
fn outline_items(nodes: &[FilteredNode], expanded: &HashSet<String>) -> Vec<TreeItem> {
    nodes
        .iter()
        .map(|node| outline_item(node, expanded))
        .collect()
}

fn outline_item(node: &FilteredNode, expanded: &HashSet<String>) -> TreeItem {
    // A separator carries its id like any other row, so it can be scrolled to
    // and reasoned about by id; its label is empty because it is drawn as a
    // hairline rather than as text.
    TreeItem::new(node.entry.id.clone(), node.entry.title.clone())
        .children(
            node.children
                .iter()
                .map(|child| outline_item(child, expanded)),
        )
        .expanded(expanded.contains(&node.entry.id))
}

/// An `Rgba` as the eight-bit channels an RTF colour table takes.
fn rgb_of_rgba(rgba: gpui_kit::Rgba) -> subtext_core::richtext::Rgb {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    subtext_core::richtext::Rgb {
        red: channel(rgba.r),
        green: channel(rgba.g),
        blue: channel(rgba.b),
    }
}

/// The same for a `.cottheme` colour, which is already eight-bit.
fn rgb_of_theme(color: subtext_theme::Rgba) -> subtext_core::richtext::Rgb {
    subtext_core::richtext::Rgb {
        red: color.r,
        green: color.g,
        blue: color.b,
    }
}

/// The same for a colour the highlighter hands out, which is an `Hsla`.
fn rgb_of(color: gpui_kit::Hsla) -> subtext_core::richtext::Rgb {
    rgb_of_rgba(color.to_rgb())
}

/// Escape `text` for an AppleScript string literal.
///
/// Only the backslash and the double quote end a literal early; a real line
/// break is a legal character inside one, which is why there is no `\n` case.
fn applescript_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Put styled text on the pasteboard, falling back to plain text.
///
/// `Ok(true)` means the RTF flavour was written. `Ok(false)` means this platform
/// has no way to write one and `plain` went to the clipboard instead — the
/// caller says so, because silently copying less than asked is the kind of quiet
/// difference this project writes gaps about.
///
/// The RTF goes through `osascript` because GPUI's `ClipboardEntry` has three
/// variants — string, image, file paths — and styled text is none of them.
/// `StandardAdditions`' `set the clipboard to` is the only call that writes a
/// pasteboard type the framework does not model, and it needs no automation
/// permission of its own.
fn put_rich_text_on_the_pasteboard(rtf: &str, plain: &str, cx: &mut App) -> std::io::Result<bool> {
    #[cfg(target_os = "macos")]
    {
        let path = std::env::temp_dir().join(format!("subtext-{}.rtf", std::process::id()));
        std::fs::write(&path, rtf)?;
        // `«class RTF »` is the four-character code with a trailing space; the
        // path sits inside a quoted literal, which is enough because a
        // temporary directory does not contain a double quote.
        // A record, not a bare value: that is what makes StandardAdditions
        // write the plain-text flavours alongside the RTF. Upstream gets the
        // same set because it writes an `NSAttributedString`; without it,
        // pasting into a target that only takes text would find nothing.
        let script = format!(
            "set the clipboard to {{«class RTF »:(read (POSIX file \"{}\") as «class RTF »), \
             string:\"{}\"}}",
            path.display(),
            applescript_string(plain)
        );
        let written = matches!(
            std::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .status(),
            Ok(status) if status.success()
        );
        let _ = std::fs::remove_file(&path);
        if written {
            return Ok(true);
        }
    }
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(plain.to_string()));
    Ok(false)
}

/// Load every `.cotsyntax` bundle the resource directory offers.
///
/// A user's own bundle with the same name wins, so a customised `Rust.cotsyntax`
/// in the support directory overrides the shipped one without a rebuild —
/// the same override contract CotEditor has.
pub fn load_syntaxes(resources: Option<&crate::resources::Resources>) -> SyntaxManager {
    let mut all: Vec<Syntax> = Vec::new();
    if let Some(resources) = resources
        && let Ok(manager) = SyntaxManager::load_from_directory(resources.syntaxes())
    {
        all.extend(manager.all().iter().cloned());
    }
    let user = crate::resources::user_syntax_directory();
    if user.is_dir()
        && let Ok(manager) = SyntaxManager::load_from_directory(&user)
    {
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
        && let Ok(manager) = ThemeManager::load_from_directory(resources.themes())
    {
        themes.extend(manager.all().iter().cloned());
    }
    let user = crate::resources::user_theme_directory();
    if user.is_dir()
        && let Ok(manager) = ThemeManager::load_from_directory(&user)
    {
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

/// Which pane takes the focus once `closing` is removed from a split of
/// `count` panes.
///
/// CotEditor's rule, from `closeSplitTextView`: the pane *before* the one that
/// closed, or the last pane when the first one closed. The answer indexes the
/// panes that are left, which is what makes the two cases asymmetric — closing
/// the first pane of four hands the focus to the fourth, which slides down to
/// third place.
///
/// `count` is the pane count *before* the removal, so it must be at least 2;
/// anything less keeps the single pane there is.
fn successor_pane(closing: usize, count: usize) -> usize {
    if count < 2 {
        return 0;
    }
    if closing == 0 { count - 2 } else { closing - 1 }
}

/// The pane the focus moves to when cycling a split of `count` panes.
///
/// CotEditor's `focusNextSplitEditor` wraps in both directions, so ⇧⌃` from the
/// first pane lands on the last.
fn cycled_pane(current: usize, count: usize, forward: bool) -> usize {
    if count == 0 {
        return 0;
    }
    if forward {
        (current + 1) % count
    } else {
        (current + count - 1) % count
    }
}

/// Open every panel once, in order, and quit.
///
/// A manual smoke for the surfaces that no unit test can reach: a panic in a
/// sheet, a settings pane or an inspector would otherwise only show up when a
/// user clicks it. `SUBTEXT_PANEL_SWEEP=1` runs it at launch (debug builds
/// only); the log line per panel is the evidence, and the exit status is the
/// gate.
///
/// ```text
/// SUBTEXT_PANEL_SWEEP=1 HOME=/tmp/smoke ./target/debug/subtext a.txt
/// ```
impl Workspace {
    #[cfg(debug_assertions)]
    pub fn panel_sweep(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            for step in 0..21u32 {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let done = this.update_in(cx, |workspace, window, cx| {
                    workspace.open_panel(step, window, cx);
                    eprintln!("panel sweep: step {step} ok");
                    window.close_dialog(cx);
                });
                if done.is_err() {
                    eprintln!("panel sweep: step {step} FAILED — the workspace is gone");
                    let _ = this.update_in(cx, |_, _, cx| cx.quit());
                    return;
                }
            }
            eprintln!("panel sweep: 21 of 21 opened");
            let _ = this.update_in(cx, |_, _, cx| cx.quit());
        })
        .detach();
    }

    /// Open one of the sweep's panels and stay there.
    ///
    /// The sweep answers "does it crash"; this answers "does it look right",
    /// which needs the window on screen long enough to be photographed.
    ///
    /// ```text
    /// SUBTEXT_PANEL_PEEK=8 HOME=/tmp/peek ./target/debug/subtext a.txt
    /// ```
    #[cfg(debug_assertions)]
    pub fn panel_peek(&mut self, step: u32, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(400))
                .await;
            let _ = this.update_in(cx, |workspace, window, cx| {
                workspace.open_panel(step, window, cx);
                eprintln!("panel peek: step {step} open");
            });
        })
        .detach();
    }

    /// Open panel number `step`. Shared by the sweep and the peek.
    #[cfg(debug_assertions)]
    fn open_panel(&mut self, step: u32, window: &mut Window, cx: &mut Context<Self>) {
        match step {
            0 => self.surround_sheet(window, cx),
            1 => self.color_code_dialog(window, cx),
            2 => self.multiple_replace_dialog(window, cx),
            3 => self.show_inspector(InspectorPane::Warnings, cx),
            4 => self.show_inspector(InspectorPane::Document, cx),
            5 => self.show_inspector(InspectorPane::Outline, cx),
            6 => self.show_find(true, window, cx),
            7..=15 => {
                let pane = match step - 7 {
                    0 => SettingsPane::General,
                    1 => SettingsPane::Appearance,
                    2 => SettingsPane::Window,
                    3 => SettingsPane::Edit,
                    4 => SettingsPane::Mode,
                    5 => SettingsPane::Format,
                    6 => SettingsPane::Snippets,
                    7 => SettingsPane::Shortcuts,
                    _ => SettingsPane::Integration,
                };
                self.show_settings_pane(pane, cx);
            }
            16 => self.show_about(cx),
            17 => self.show_console(cx),
            18 => {
                self.settings.window.show_sidebar = true;
                self.settings.sidebar_pane = SidebarPane::FolderFind;
                cx.notify();
            }
            19 => {
                // Show it too, not just select it: the sweep runs step 18 first
                // so it could assume the sidebar was open, but `panel_peek`
                // opens one step on its own and showed nothing.
                self.settings.window.show_sidebar = true;
                self.settings.sidebar_pane = SidebarPane::FileBrowser;
                cx.notify();
            }
            // Not a panel, but a whole document the command builds.
            20 => self.create_bug_report(window, cx),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_the_capsule_moves_it_with_the_pointer() {
        // The capsule hugs the top-right corner, so the two deltas work in
        // opposite senses: right is an inset, down is an offset.
        let start = (0., 0.);
        let after = drag_capsule_offset(start, (100., 100.), (140., 70.));
        assert_eq!(after, (-40., -30.));
        // Each step accumulates, so two steps equal one longer drag.
        let step = drag_capsule_offset(after, (140., 70.), (150., 90.));
        assert_eq!(step, (-50., -10.));
        assert_eq!(drag_capsule_offset(start, (100., 100.), (150., 90.)), step);
        // Dragging back to where it started leaves it where it started.
        assert_eq!(drag_capsule_offset(step, (150., 90.), (100., 100.)), start);
    }

    fn warning_row(line: usize, first: &str, second: &str) -> WarningRow {
        WarningRow {
            line,
            first: first.to_string(),
            second: second.to_string(),
            range: 0..0,
        }
    }

    #[test]
    fn the_line_column_sorts_as_a_number_not_a_string() {
        // The trap this guards: "10" < "2" as text, so a string sort would put
        // the tenth line before the second.
        let mut rows = vec![
            warning_row(10, "b", ""),
            warning_row(2, "a", ""),
            warning_row(1, "c", ""),
        ];
        sort_warning_rows(&mut rows, WarningColumn::Line, false);
        assert_eq!(
            rows.iter().map(|row| row.line).collect::<Vec<_>>(),
            vec![1, 2, 10]
        );

        sort_warning_rows(&mut rows, WarningColumn::Line, true);
        assert_eq!(
            rows.iter().map(|row| row.line).collect::<Vec<_>>(),
            vec![10, 2, 1]
        );
    }

    #[test]
    fn the_character_and_converted_columns_sort_by_their_own_text() {
        let mut rows = vec![
            warning_row(1, "~", "—"),
            warning_row(2, "a", "\u{FFFD}"),
            warning_row(3, "Z", "z"),
        ];
        sort_warning_rows(&mut rows, WarningColumn::First, false);
        // Upper case sorts before lower case, as `Character`'s `<` does.
        assert_eq!(
            rows.iter()
                .map(|row| row.first.as_str())
                .collect::<Vec<_>>(),
            vec!["Z", "a", "~"]
        );

        sort_warning_rows(&mut rows, WarningColumn::Second, false);
        assert_eq!(
            rows.iter()
                .map(|row| row.second.as_str())
                .collect::<Vec<_>>(),
            vec!["z", "—", "\u{FFFD}"]
        );
    }

    #[test]
    fn sorting_is_stable_so_equal_keys_keep_document_order() {
        let mut rows = vec![
            warning_row(5, "same", ""),
            warning_row(9, "same", ""),
            warning_row(1, "same", ""),
        ];
        sort_warning_rows(&mut rows, WarningColumn::First, false);
        assert_eq!(
            rows.iter().map(|row| row.line).collect::<Vec<_>>(),
            vec![5, 9, 1]
        );
    }

    #[test]
    fn a_header_click_starts_ascending_and_then_reverses() {
        // Nothing sorted yet: the first click sorts the column, ascending.
        let first = next_sort(None, WarningColumn::Line);
        assert_eq!(first, Some((WarningColumn::Line, false)));
        // Clicking the same header again reverses it, and again restores it.
        let second = next_sort(first, WarningColumn::Line);
        assert_eq!(second, Some((WarningColumn::Line, true)));
        assert_eq!(next_sort(second, WarningColumn::Line), first);
        // A different header starts over at ascending rather than inheriting
        // the previous column's direction.
        assert_eq!(
            next_sort(second, WarningColumn::First),
            Some((WarningColumn::First, false))
        );
    }

    #[test]
    fn invisible_characters_are_cells_of_code_points() {
        // A tab and a space drawn as themselves are empty cells; upstream
        // prints the code point instead so the row says what it is.
        assert_eq!(character_cell("\t"), "U+0009");
        assert_eq!(character_cell(" "), "U+0020");
        assert_eq!(character_cell("\u{A0}"), "U+00A0");
        assert_eq!(character_cell("\u{2028}"), "U+2028");
        // A visible character is its own cell.
        assert_eq!(character_cell("~"), "~");
        assert_eq!(character_cell("€"), "€");
    }

    /// Closing the first pane is the case upstream gets wrong-looking on
    /// purpose: the focus lands on the *last* pane, not the second.
    #[test]
    fn closing_the_first_pane_hands_the_focus_to_the_last() {
        assert_eq!(successor_pane(0, 4), 2);
        assert_eq!(successor_pane(0, 3), 1);
        assert_eq!(successor_pane(0, 2), 0);
    }

    /// Any other pane hands the focus to the one before it, whose index does
    /// not move when the closed pane is removed.
    #[test]
    fn closing_another_pane_hands_the_focus_to_the_one_before_it() {
        assert_eq!(successor_pane(1, 4), 0);
        assert_eq!(successor_pane(2, 4), 1);
        assert_eq!(successor_pane(3, 4), 2);
    }

    /// There is nothing to hand the focus to below two panes; the caller
    /// refuses to close in that case, and the rule must not underflow if it
    /// ever did.
    #[test]
    fn a_single_pane_keeps_the_focus() {
        assert_eq!(successor_pane(0, 1), 0);
        assert_eq!(successor_pane(0, 0), 0);
    }

    /// Cycling wraps at both ends, so neither key is a dead end.
    #[test]
    fn cycling_editors_wraps_around() {
        assert_eq!(cycled_pane(0, 3, true), 1);
        assert_eq!(cycled_pane(2, 3, true), 0);
        assert_eq!(cycled_pane(0, 3, false), 2);
        assert_eq!(cycled_pane(2, 3, false), 1);
    }

    /// Cycling is only reached with a real split, but a zero count must not
    /// divide by zero if the rule is ever reused elsewhere.
    #[test]
    fn cycling_no_editors_is_harmless() {
        assert_eq!(cycled_pane(0, 0, true), 0);
        assert_eq!(cycled_pane(0, 0, false), 0);
    }

    // ---- the outline's bridge to the tree --------------------------------

    fn node(id: &str, title: &str, children: Vec<FilteredNode>) -> FilteredNode {
        FilteredNode {
            entry: OutlineEntry {
                id: id.to_string(),
                title: title.to_string(),
                range: 0..1,
                indent: crate::outline::Indent::Level(0),
                kind: Some(OutlineKind::Function),
            },
            children,
            matches: Vec::new(),
        }
    }

    /// The items carry the ids the workspace looks rows up by, and the
    /// expansion state the mirror holds. A rebuild that dropped the latter
    /// would close the whole outline on every keystroke.
    #[test]
    fn outline_items_carry_the_expansion_state() {
        let nodes = vec![node(
            "parent",
            "Parent",
            vec![node("child", "Child", Vec::new())],
        )];
        let expanded = HashSet::from(["parent".to_string()]);

        let items = outline_items(&nodes, &expanded);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id.as_ref(), "parent");
        assert_eq!(items[0].label.as_ref(), "Parent");
        assert!(items[0].is_folder());
        assert!(items[0].is_expanded());
        assert_eq!(items[0].children[0].id.as_ref(), "child");
        assert!(!items[0].children[0].is_expanded());
    }

    /// An id the mirror does not know is closed — which is what makes a row
    /// the user just closed stay closed across the next rebuild.
    #[test]
    fn a_folder_outside_the_mirror_is_collapsed() {
        let nodes = vec![node(
            "parent",
            "Parent",
            vec![node("child", "Child", Vec::new())],
        )];
        let items = outline_items(&nodes, &HashSet::new());
        assert!(items[0].is_folder());
        assert!(!items[0].is_expanded());
    }

    /// A leaf has nothing to expand, so the mirror's opinion of it does not
    /// change how it is handed over.
    #[test]
    fn a_leaf_is_never_a_folder() {
        let items = outline_items(&[node("leaf", "Leaf", Vec::new())], &HashSet::new());
        assert!(!items[0].is_folder());
    }
}
