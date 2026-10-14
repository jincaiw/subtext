//! The Quick Actions bar: CotEditor's ⌘K command palette.
//!
//! CotEditor's `CommandBarWindowController` floats a borderless panel over the
//! document window and fills it with `CommandBarView` — a search field, and
//! under it the menu commands the query matches. SubText keeps both halves: the
//! panel is a floating window of the same width, in the same place, closing on
//! the same keys; the list is `gpui-component`'s command palette, which owns
//! the selection, focus, keyboard, dismissal and layout contract that a
//! palette has to get right and that a hand-rolled box would have to reinvent.
//!
//! # What the query matches
//!
//! [`crate::menu::COMMANDS`] is the whole menu bar in one table. A command is
//! searched on its path from the second component onwards plus its own title:
//! `Format > Tab Width > Custom…` is matched as `Tab Width` and then
//! `Custom…`. The first component is the menu bar's own title and is dropped,
//! which is what lets `tab` find the item without naming `Format` first —
//! upstream's `ActionCommand.match(command:)`, which drops `paths[0]` for the
//! same reason.
//!
//! Each component is matched with [`subtext_core::matcher`], the same
//! subsequence search the outline filter uses: characters rather than
//! substrings, so `tw` finds `Tab Width`. The characters that were matched are
//! drawn bold, and the components that were not matched at all are drawn under
//! them as the breadcrumb, which is upstream's two-line row.
//!
//! An empty query matches nothing. That is upstream's behaviour rather than an
//! oversight: the bar opens with an empty field and no list, and the list
//! appears as soon as there is something to search for.
//!
//! # Where this differs from upstream
//!
//! * The Script menu is one of the runtime placeholders in [`crate::menu`], so
//!   every command in the table is a menu command. Upstream's second kind — a
//!   script — has no rows to draw until that menu is filled in.
//! * Upstream closes the panel when it stops being the key window. GPUI has no
//!   window-activation event to hang that on, so the bar closes on Escape, on
//!   Enter, or on a click on a row, and stays up if the user clicks the
//!   document behind it.
//! * The frame around the rows — the search field's type size, the row's
//!   padding and corner radius, the list's inset — is the component family's,
//!   as the design guides require of a palette. The *content* of a row is
//!   upstream's, down to the bolded match.
//!
//! # Working with the palette
//!
//! Two of GPUI's rules shape this file, and both bite silently at runtime
//! rather than at compile time:
//!
//! * **The panel is opened from a deferred callback.** Opening a window runs
//!   the root view's first layout and draw before `open_window` returns, and
//!   the command that opens this one arrives from inside a `Workspace`
//!   update — where the workspace is leased out of the entity map and cannot
//!   be read. Deferring the open is the same fix the settings window needs.
//! * **A row's content is built while the palette is being rendered**, so it
//!   must not touch the palette's entity: reading an entity from inside its
//!   own render is a double lease, and the palette measures and draws every
//!   row through that builder. The highlighted row therefore travels into the
//!   builder as a plain [`bool`] rather than being looked up from the palette.

use std::ops::Range;

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IndexPath, Sizable as _,
    command::{Command, CommandItem, CommandState},
    h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Pixels, Render,
    SharedString, Styled as _, Subscription, WeakEntity, Window, div, px, rems, size,
};

use crate::keymap;
use crate::menu;
use crate::outline::highlight_segments;
use crate::workspace::Workspace;

/// How wide the panel is, in rem — CotEditor's 500 points.
pub const WIDTH: f32 = 31.25;

/// The tallest the list may get, in rem — the palette's own cap of 300 points.
const LIST_MAX: f32 = 18.75;

/// The height of the search row, in rem.
///
/// The palette's field is the component library's default input height (two
/// rem) plus the one-pixel rule under it.
const SEARCH_ROW: f32 = 2.0625;

/// The list's own inset, in rem: the palette pads the list by a quarter rem on
/// every side, which is half a rem top and bottom.
const LIST_INSET: f32 = 0.5;

/// The name the panel is titled and the field is labelled with.
///
/// Upstream has no placeholder — the field is empty and the *panel* is titled
/// "Quick Actions" for VoiceOver. The palette derives the field's accessible
/// name from its placeholder and offers no other way to set one, so the same
/// string does both jobs here.
const TITLE: &str = "Quick Actions";

/// How the application menu is spelled in the generated table.
///
/// [`menu::COMMANDS`] spells every path with CotEditor's own name because
/// [`keymap::SHORTCUTS`] does, and the two are joined on the path. The display
/// is the one place the two applications have to differ.
const UPSTREAM_MENU_TITLE: &str = "CotEditor";

/// How tall the panel is for a given number of results.
///
/// Upstream's panel is sized by SwiftUI to fit its content, so it opens as
/// little more than the search field and grows as the query brings results in.
/// GPUI windows have no content sizing, so the height is computed here — from
/// the window's own line height rather than a constant, so a theme that
/// changes the type scale moves the panel with it.
pub fn window_height(rows: usize, window: &Window) -> Pixels {
    let rem = window.rem_size().as_f32();
    let row = row_height(window).as_f32();
    let list = (row * rows as f32 + rem * LIST_INSET).min(rem * LIST_MAX);
    px(rem * SEARCH_ROW + list)
}

/// How tall one result row is, in pixels.
///
/// The palette pads a row by three quarters of a rem above and below, and the
/// content is two lines: the matched path at `text_sm` and the breadcrumb at
/// `text_xs`. GPUI's line height is a fraction of the type size, so both are
/// taken from the window's line height scaled by those two sizes.
fn row_height(window: &Window) -> Pixels {
    window.line_height() * (0.875 + 0.75) + window.rem_size() * 0.75
}

// ---- the model -----------------------------------------------------------

/// One path component of a command, and where the query landed inside it.
#[derive(Clone, Debug)]
pub struct MatchedPath {
    /// The component, as the menu bar spells it.
    pub string: &'static str,
    /// Byte ranges of `string` the query matched, one per character and in
    /// order — upstream reports a range per character, and the bold run is
    /// built from them.
    pub ranges: Vec<Range<usize>>,
}

/// A command the query matches, with the evidence.
#[derive(Clone)]
pub struct Candidate {
    /// The menu entry, which carries the path and the command's name.
    pub entry: &'static menu::Command,
    /// The components the query was matched against, in path order.
    pub matches: Vec<MatchedPath>,
    /// How far the match spans, summed over the components. Lower is better.
    pub score: usize,
}

/// The components of a command's path that a query is matched against.
///
/// The path is `Format > Syntax > Swift`; the searchable part is `Syntax` and
/// `Swift`. See the module docs for why the first component is dropped.
fn searchable(path: &'static str) -> impl Iterator<Item = &'static str> {
    path.split(" > ").skip(1)
}

/// The components of a command's path that are *not* searched: what the row
/// shows underneath the matched line.
fn breadcrumbs(path: &'static str) -> Vec<&'static str> {
    let mut parts: Vec<&'static str> = path.split(" > ").collect();
    parts.pop();
    parts
}

/// The name a path component is shown under.
fn shown(component: &'static str) -> &'static str {
    if component == UPSTREAM_MENU_TITLE {
        menu::APP_MENU_TITLE
    } else {
        component
    }
}

/// The shortcut the menu bar shows for a command, if it has one.
///
/// The key map is generated from the same table as the menu, so the path is
/// the join key and the two cannot disagree about which command a shortcut
/// belongs to.
fn shortcut_for(path: &str) -> Option<&'static str> {
    keymap::SHORTCUTS
        .iter()
        .find(|row| row.0 == path)
        .map(|row| row.1)
}

/// Match `query` against one command, the way `ActionCommand.match(command:)`
/// does.
///
/// Components are tried in order and the query has to be used up in order: a
/// component that cannot take the next characters is not a match, and whatever
/// the query has left when the components run out has to be whitespace. The
/// whitespace after a matched component is treated as the separator between
/// two menu levels, so `format font` finds `Format > Font` without either
/// component containing the space.
pub fn match_command(
    entry: &'static menu::Command,
    query: &str,
) -> Option<(Vec<MatchedPath>, usize)> {
    if query.is_empty() {
        return None;
    }

    let mut matches: Vec<MatchedPath> = Vec::new();
    let mut score = 0;
    let mut remaining = query.to_string();

    for string in searchable(entry.path) {
        let search = if matches.is_empty() {
            remaining.clone()
        } else {
            remaining.trim_start().to_string()
        };
        let found = subtext_core::matcher::abbreviated_match(string, &search);

        // A leading component the query does not touch is simply not part of
        // the answer — but once one has matched, every component from there on
        // is kept, so the row shows the whole path the query walked.
        if matches.is_empty() && found.is_none() {
            continue;
        }

        matches.push(MatchedPath {
            string,
            ranges: found
                .as_ref()
                .map(|found| found.ranges.clone())
                .unwrap_or_default(),
        });
        score += found.as_ref().map_or(0, |found| found.score);
        remaining = found.map_or(search, |found| found.remaining);
    }

    if matches.is_empty() || !remaining.chars().all(char::is_whitespace) {
        return None;
    }

    Some((matches, score))
}

/// Every command the query matches, best first.
pub fn candidates_for(query: &str) -> Vec<Candidate> {
    let mut found: Vec<Candidate> = menu::COMMANDS
        .iter()
        .filter_map(|entry| {
            let (matches, score) = match_command(entry, query)?;
            Some(Candidate { entry, matches, score })
        })
        .collect();

    // Upstream sorts by score ascending, so the shortest span that covers the
    // query comes first. The sort is stable, which leaves commands that tie in
    // menu order — the only ordering that means anything to the user.
    found.sort_by_key(|candidate| candidate.score);
    found
}

// ---- the panel -----------------------------------------------------------

pub struct CommandBar {
    /// The palette's own interaction state: the query, the highlighted row,
    /// the focus and the scroll position.
    state: Entity<CommandState>,
    /// The commands the current query matches, best first. The palette is
    /// handed these as its items on every render, so this is the list the
    /// rows are built from.
    candidates: Vec<Candidate>,
    /// The query the candidates were computed for.
    query: SharedString,
    /// The row the palette has highlighted, as a position in `candidates`.
    ///
    /// A copy, because the rows are built while the palette is being rendered
    /// and cannot read the palette then. It is kept in step by `refresh`.
    selected: Option<usize>,
    /// How many rows the window was last sized for.
    sized_for: Option<usize>,
    /// The workspace a chosen command runs in, and the window it runs against.
    workspace: WeakEntity<Workspace>,
    workspace_window: AnyWindowHandle,
    /// Recomputing the candidates is driven by the palette's own state rather
    /// than by a callback on the query, because the palette is rebuilt on
    /// every render and a callback would have to reach back into this view.
    _state: Subscription,
}

impl CommandBar {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        workspace_window: AnyWindowHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = cx.new(|cx| CommandState::new(window, cx));
        let state_subscription = cx.observe_in(&state, window, |bar: &mut Self, state, window, cx| {
            bar.refresh(&state, window, cx);
        });

        // The field takes the keyboard as soon as the panel is up, which is
        // what makes the bar usable without a click. Deferred, because the
        // focus handle has to be in a rendered tree to hold focus.
        let focused = state.clone();
        window.defer(cx, move |window, cx| {
            focused.update(cx, |state, cx| state.focus(window, cx));
        });

        Self {
            state,
            candidates: Vec::new(),
            query: SharedString::default(),
            selected: None,
            // The panel is opened at the height of an empty bar; `refresh`
            // moves it from there.
            sized_for: Some(0),
            workspace,
            workspace_window,
            _state: state_subscription,
        }
    }

    /// Follow the palette: recompute the results when the query moved, fit the
    /// window to them, and keep the highlighted row on hand for the rows to
    /// read.
    ///
    /// This is the palette's observer, so it runs once the palette has been
    /// updated and is free to read it.
    fn refresh(&mut self, state: &Entity<CommandState>, window: &mut Window, cx: &mut Context<Self>) {
        let query = state.read(cx).query(cx);

        if query != self.query {
            // The palette can only move its highlight among rows it already
            // has. When it has none the write below would *clear* the
            // highlight instead of moving it, and a cleared highlight stays
            // cleared — so it is skipped, and the palette picks the first row
            // itself as its model installs. That is where the write would have
            // put it anyway.
            let has_rows = !self.candidates.is_empty();

            self.query = query;
            self.candidates = candidates_for(&self.query);
            self.resize(window);

            // Upstream selects the first candidate on every query change, and
            // the first candidate is the best one — the list is sorted by
            // score, so the highlight lands on the closest match. The palette
            // would keep the highlight where it was instead, and a position
            // means nothing here: the item list is rebuilt from scratch on
            // every keystroke.
            if has_rows {
                state.update(cx, |state, cx| {
                    state.set_selected_index(Some(IndexPath::new(0)), window, cx);
                });
            }

            // An unfiltered, ungrouped palette numbers its rows from zero, so
            // the highlight is the first candidate — or nothing at all, when
            // the query matched nothing.
            self.selected = (!self.candidates.is_empty()).then_some(0);
            cx.notify();
            return;
        }

        // A move inside the list: the arrow keys and the mouse both go through
        // the palette, which notifies. Read back where the highlight landed.
        let selected = state.read(cx).selected_index().map(|index| index.row);
        if selected != self.selected {
            self.selected = selected;
            cx.notify();
        }
    }

    /// Fit the window to the number of results.
    fn resize(&mut self, window: &mut Window) {
        if self.sized_for == Some(self.candidates.len()) {
            return;
        }
        self.sized_for = Some(self.candidates.len());
        window.resize(size(
            window.rem_size() * WIDTH,
            window_height(self.candidates.len(), window),
        ));
    }

    /// Run the command the row at `row` stands for.
    fn run(&self, row: usize, cx: &mut App) {
        let Some(candidate) = self.candidates.get(row) else {
            return;
        };
        let name = candidate.entry.action;
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        // The command runs against the *document* window rather than the
        // panel: a command that opens a dialog, asks for a file, or reveals a
        // path has to be answered by the window the user is editing in. That
        // is also why upstream closes the panel before it performs anything.
        self.workspace_window
            .update(cx, |_, window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.dispatch(name, window, cx);
                });
            })
            .ok();
    }
}

impl Render for CommandBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(TITLE);

        let mut palette = Command::new(&self.state)
            // The window is the panel, so the palette draws no frame of its
            // own; the surface below does.
            .bordered(false)
            // SubText does the matching. The palette's own filter is a
            // case-insensitive substring test over one label, which cannot
            // express a subsequence match over a menu path.
            .filterable(false)
            .max_h(rems(LIST_MAX))
            .placeholder(TITLE)
            // Upstream shows nothing at all when nothing matches, and the
            // panel is then only the search field. The palette's default is a
            // "no results" message, which upstream does not have.
            .empty(|_, _, _| div())
            .on_cancel(|window, _| window.remove_window())
            .on_confirm({
                let bar = cx.entity().downgrade();
                move |index: IndexPath, window, cx| {
                    // Close first, as upstream does, so the command is
                    // delivered to the document window and not to the panel.
                    window.remove_window();
                    bar.update(cx, |bar, cx| bar.run(index.row, cx)).ok();
                }
            });

        for (row, candidate) in self.candidates.iter().enumerate() {
            palette = palette.item(item(candidate, self.selected == Some(row)));
        }

        div()
            .size_full()
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .rounded(cx.theme().radius_lg)
            .border_1()
            .border_color(cx.theme().border)
            .overflow_hidden()
            .capture_key_down(|event: &KeyDownEvent, window, _| {
                // Upstream's panel closes on Escape whatever the field holds.
                // The palette's own Cancel clears a non-empty query first and
                // closes on the second press, so Escape is taken here — during
                // the capture phase, before the field sees it.
                if event.keystroke.key == "escape" {
                    window.remove_window();
                }
            })
            .child(palette)
    }
}

// ---- a row ---------------------------------------------------------------

/// One result row, as the palette wants it.
///
/// Whether the row is the highlighted one is passed in rather than looked up.
/// The palette paints the highlight itself, but it gives the content builder
/// no way to tell whether the row it is building is the highlighted one — and
/// the builder runs from *inside* the palette's own render, where reading the
/// palette would be a double lease. The palette measures and draws every row
/// through that builder, so the answer has to arrive as a plain value.
fn item(candidate: &Candidate, selected: bool) -> CommandItem {
    let matches = candidate.matches.clone();
    let path = breadcrumbs(candidate.entry.path);
    let shortcut = shortcut_for(candidate.entry.path);

    CommandItem::new().child(move |_, cx| draw_row(&matches, &path, shortcut, selected, cx))
}

/// The row: the kind icon, the matched path, the breadcrumb, the shortcut.
fn draw_row(
    matches: &[MatchedPath],
    path: &[&'static str],
    shortcut: Option<&'static str>,
    selected: bool,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let strong = cx.theme().accent_foreground;

    // Upstream draws `filemenu.and.selection` here for a menu command and an
    // AppleScript glyph for a script. SubText's Script menu is a runtime
    // placeholder, so every row in the table is a menu command and one glyph
    // covers them all. The slot is a fixed box, as upstream's is, so the text
    // of every row starts at the same place.
    let icon = div()
        .w(rems(1.625))
        .h(rems(1.375))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .text_color(if selected { strong } else { muted })
        .child(Icon::new(IconName::SquareMenu).small());

    let mut matched = line();
    for (offset, component) in matches.iter().enumerate() {
        if offset > 0 {
            matched = matched.child(chevron(muted));
        }
        matched = matched.child(runs(component.string, &component.ranges));
    }

    let mut trail = line().text_xs();
    for (offset, component) in path.iter().enumerate() {
        if offset > 0 {
            trail = trail.child(chevron(muted));
        }
        trail = trail.child(div().child(shown(component)));
    }
    if !selected {
        // Selected, the row's own colour is the one that reads on the accent
        // fill and this is left alone; unselected, the breadcrumb is the quiet
        // half of the row.
        trail = trail.text_color(muted);
    }

    let mut row = h_flex()
        .flex_1()
        .min_w(px(0.))
        .gap(rems(0.25))
        .items_center()
        .child(icon)
        .child(v_flex().flex_1().min_w(px(0.)).child(matched).child(trail));

    if let Some(shortcut) = shortcut {
        row = row.child(
            div()
                .flex_none()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(if selected { strong } else { muted })
                .child(shortcut),
        );
    }

    row.into_any_element()
}

/// A line of the row: no wrapping, and no wider than the row itself.
fn line() -> gpui_kit::Div {
    h_flex().gap(rems(0.25)).items_center().whitespace_nowrap().overflow_hidden()
}

/// The separator between two path components.
fn chevron(color: Hsla) -> AnyElement {
    Icon::new(IconName::ChevronRight)
        .xsmall()
        .flex_shrink_0()
        .text_color(color)
        .into_any_element()
}

/// One path component, with the characters the query matched in bold.
///
/// The ranges come from the same matcher the outline filter uses, so a command
/// and an outline entry highlight a query the same way. Upstream bolds the
/// matched characters and leaves their colour to the row.
fn runs(text: &str, ranges: &[Range<usize>]) -> AnyElement {
    let mut run = h_flex().gap_0().whitespace_nowrap();
    for (range, is_match) in highlight_segments(text, ranges) {
        let part = text.get(range).unwrap_or_default().to_string();
        run = run.child(
            div()
                .when(is_match, |this| this.font_weight(FontWeight::BOLD))
                .child(part),
        );
    }
    run.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(path: &'static str) -> &'static menu::Command {
        menu::COMMANDS
            .iter()
            .find(|command| command.path == path)
            .unwrap_or_else(|| panic!("{path} is not in the menu"))
    }

    /// The searched components are the path from the second one on.
    ///
    /// The first is the menu bar's own title, which upstream drops: a query
    /// does not have to name the menu bar to find a command under it.
    #[test]
    fn the_search_skips_the_menu_bar_title() {
        assert_eq!(
            searchable("Format > Syntax > Swift").collect::<Vec<_>>(),
            ["Syntax", "Swift"]
        );
        assert_eq!(searchable("File > New").collect::<Vec<_>>(), ["New"]);
    }

    /// The breadcrumb is the path without the command's own title, which is
    /// what upstream draws under the matched line.
    #[test]
    fn the_breadcrumb_is_the_path_without_the_title() {
        assert_eq!(breadcrumbs("Format > Syntax > Swift"), ["Format", "Syntax"]);
        assert_eq!(breadcrumbs("File > New"), ["File"]);
    }

    /// A query that names the command finds it, and the match is the title.
    #[test]
    fn a_query_finds_a_command_by_its_title() {
        let (matches, _) = match_command(command("File > Save"), "save").unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].string, "Save");
        assert_eq!(matches[0].ranges, [0..1, 1..2, 2..3, 3..4]);
    }

    /// The components are searched in order, so a query can walk a path.
    #[test]
    fn a_query_can_name_two_components_in_order() {
        let entry = command("Format > Tab Width > Custom…");
        let (matches, _) = match_command(entry, "tab custom").unwrap();
        assert_eq!(
            matches.iter().map(|m| m.string).collect::<Vec<_>>(),
            ["Tab Width", "Custom…"]
        );

        // The other way round is not a match: the search only moves forward.
        assert!(match_command(entry, "custom tab").is_none());
    }

    /// The whitespace between two components is a separator, not a character
    /// either component has to contain.
    #[test]
    fn the_whitespace_between_components_is_a_separator() {
        let entry = command("Format > Tab Width > Custom…");
        let (matches, _) = match_command(entry, "tab   custom").unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[1].string, "Custom…");
    }

    /// A leading component the query does not touch is skipped, and the row is
    /// built from the component that did match.
    #[test]
    fn a_skipped_component_is_not_part_of_the_match() {
        let entry = command("Edit > Select > Select All");
        let (matches, _) = match_command(entry, "all").unwrap();
        assert_eq!(
            matches.iter().map(|m| m.string).collect::<Vec<_>>(),
            ["Select All"]
        );
    }

    /// Once a component has matched, every component after it is kept — the
    /// row shows the whole path the query walked, and the ones the query ran
    /// out before are drawn with nothing highlighted.
    #[test]
    fn a_component_after_the_match_is_kept_with_nothing_to_highlight() {
        let entry = command("Edit > Select > Select All");
        let (matches, _) = match_command(entry, "select").unwrap();
        assert_eq!(
            matches.iter().map(|m| m.string).collect::<Vec<_>>(),
            ["Select", "Select All"]
        );
        assert!(matches[1].ranges.is_empty());
    }

    /// Whatever the query has left when the components run out has to be
    /// whitespace, or it was never matched.
    #[test]
    fn a_query_the_path_cannot_absorb_is_not_a_match() {
        assert!(match_command(command("File > Save"), "save document").is_none());
        // Trailing whitespace is what a separator ends as, so it is allowed.
        assert!(match_command(command("File > Save"), "save ").is_some());
    }

    /// An empty query matches nothing at all — upstream's `guard`.
    #[test]
    fn an_empty_query_matches_nothing() {
        assert!(match_command(command("File > Save"), "").is_none());
        assert!(candidates_for("").is_empty());
    }

    /// The results are sorted by how far the match spans, so the closest match
    /// to the query is the one Enter runs.
    #[test]
    fn the_closest_match_comes_first() {
        let found = candidates_for("save");
        assert!(!found.is_empty());
        assert_eq!(found[0].entry.path, "File > Save");
        assert!(
            found.windows(2).all(|pair| pair[0].score <= pair[1].score),
            "the results are not sorted by score"
        );
    }

    /// Every command is reachable: naming it in full finds it, and nothing in
    /// the table is missing a shortcut lookup that would panic.
    #[test]
    fn every_command_can_be_found_by_its_own_title() {
        for entry in menu::COMMANDS {
            let title = entry.path.rsplit(" > ").next().expect("a path has a title");
            let found = candidates_for(title);
            assert!(
                found.iter().any(|candidate| candidate.entry.path == entry.path),
                "{title} does not find {}",
                entry.path
            );
            // A row draws the shortcut when there is one; the lookup is by
            // path, so a path spelled differently would silently lose it.
            let _ = shortcut_for(entry.path);
        }
    }

    /// Every shortcut in the key map belongs to a command the bar lists, so a
    /// shortcut can never be shown on a row that does not exist.
    #[test]
    fn every_shortcut_belongs_to_a_listed_command() {
        for row in keymap::SHORTCUTS {
            assert!(
                menu::COMMANDS.iter().any(|command| command.path == row.0),
                "{} has a shortcut but is not a command",
                row.0
            );
        }
    }

    /// The application menu is shown under SubText's name, and every other
    /// component is left as the table spells it.
    #[test]
    fn the_application_menu_is_renamed_for_display() {
        assert_eq!(shown("CotEditor"), menu::APP_MENU_TITLE);
        assert_eq!(shown("Format"), "Format");
    }
}
