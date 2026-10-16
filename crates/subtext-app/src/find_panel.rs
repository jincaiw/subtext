//! The **Find & Replace** panel — a window of its own.
//!
//! CotEditor's `FindPanelController` is an `NSPanel`: a floating utility window
//! titled *Find & Replace*, not a bar inside the document window. That is why
//! its content is a vertical split of three parts — the two fields, the result
//! list, and the buttons — with the result list collapsed until something is
//! found; a bar in the window's chrome has no room for any of that.
//!
//! # What this panel does not own
//!
//! Nothing. The query and the replacement are the *workspace's* `InputState`s
//! and the matches are its `FindState`, because every find command in the menu
//! works on them too and there must not be two answers to "what is being
//! searched for". The panel reads them through its weak handle and sends its
//! buttons back through the same door.
//!
//! # Why the buttons go through the document window
//!
//! A command that edits the buffer has to be deferred onto the window that owns
//! the editor: `Window::defer` schedules work on the window it was called on,
//! and this panel is a different window. The panel therefore asks the workspace
//! for the document window's handle and runs the command *there*, which is also
//! what makes the editor's own event cycle see the change.

use std::ops::Range;

use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
    table::{Column, ColumnSort, DataTable, TableDelegate, TableState},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, FontWeight, InteractiveElement as _,
    IntoElement, KeyDownEvent, MouseButton, ParentElement as _, Render, Styled as _, Subscription,
    WeakEntity, Window, div, hsla, px, rems,
};

use subtext_core::findresult::CELL_HEAD_OFFSET;
use subtext_core::truncate_head_with_ranges;

use crate::workspace::Workspace;

/// How tall the two text fields are.
///
/// Upstream's `.frame(minHeight: 44)` — 44 points, which is 2.75 rem at the
/// default root size.
const FIELD_HEIGHT: f32 = 2.75;

/// How wide the result table's line-number lane is, in points.
///
/// Upstream gives the `Line` column `width(ideal: 30, max: 64)` points — a
/// narrow lane rather than an equal share of the table. The DataTable's columns
/// are fixed-pixel, so the ideal is what ships: a width would only be widened
/// by hand here, and the upstream ideal is what the default layout shows.
const LINE_COLUMN_WIDTH: f32 = 30.;
/// The lane's upper bound, upstream's `max: 64`.
const LINE_COLUMN_MAX_WIDTH: f32 = 64.;

/// How much slack the matched-text column keeps inside the viewport, in
/// points: the viewport's own 1-point border on each side.
const CELL_HORIZONTAL_SLACK: f32 = 2.;

/// How tall the result table may get before it scrolls, in rem.
///
/// Upstream's result view opens at 200 points (`defaultResultViewHeight`) and
/// the split view then lets the user drag it. 12.5 rem is 200 at the default
/// base size, and it is a maximum rather than a fixed height so a short list
/// does not leave a hole under itself.
const RESULT_TABLE_MAX_HEIGHT: f32 = 12.5;

/// How wide the result message may get before it truncates.
///
/// Upstream reserves exactly the message's own width as the text field's
/// trailing inset, so a long message does not run under the typed query. A
/// maximum is the same protection with a bound the layout can rely on.
const MESSAGE_MAX_WIDTH: f32 = 14.;

/// The **Find & Replace** panel.
///
/// The panel view owns the result table's component state
/// (`Entity<TableState<_>>`): a table state must be created in its own entity's
/// context, and the panel window is the only place that context exists. The
/// table is a *view* of the workspace's find data — the delegate reads the
/// workspace live through its weak handle, so rows, selection and sort order
/// each have exactly one owner.
pub struct FindPanel {
    workspace: WeakEntity<Workspace>,
    table: Entity<TableState<FindResultTable>>,
    /// Keeps the blur subscription alive: it is what clears the workspace's
    /// *results focused* flag when the keyboard focus leaves the table.
    _results_blur: Option<Subscription>,
}

impl FindPanel {
    pub fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| {
            TableState::new(FindResultTable::new(workspace.clone()), window, cx)
        });
        // While the table has the keyboard focus, ⌘C and the standard Text
        // Size commands belong to it rather than to the editor. A blur on this
        // handle is the moment that stops being true.
        let blur = {
            let focus = table.focus_handle(cx);
            let workspace = workspace.clone();
            Some(cx.on_blur(&focus, window, move |_, _, cx| {
                if let Some(workspace) = workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| workspace.focus_find_results(false, cx));
                }
            }))
        };
        Self { workspace, table, _results_blur: blur }
    }

    /// Run a command on the document window.
    ///
    /// `None` while the document window has gone — the panel is a satellite of
    /// it, and every one of its buttons acts on the document.
    fn on_document(
        &self,
        cx: &mut Context<Self>,
        command: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let Some(handle) = workspace.read(cx).document_window() else {
            return;
        };
        // The window handle gives an `AnyView`, not the workspace, so the
        // workspace is reached through its own weak handle — and `update` on
        // *that* is what hands the command a `Context<Workspace>`.
        let weak = workspace.downgrade();
        handle
            .update(cx, move |_, window, cx| {
                weak.update(cx, |workspace, cx| command(workspace, window, cx)).ok();
            })
            .ok();
    }
}

impl Render for FindPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Upstream titles the panel's window itself, and it is the window's own
        // title bar that offers the close button.
        window.set_window_title("Find & Replace");
        let Some(workspace) = self.workspace.upgrade() else {
            return div().into_any_element();
        };
        let (query, replacement) = workspace.read(cx).find_fields();
        let (regex, case_sensitive, in_selection) = {
            let workspace = workspace.read(cx);
            let settings = &workspace.settings;
            (settings.find_regex, settings.find_case_sensitive, settings.find_in_selection)
        };
        let (count, current, error) = workspace.read(cx).find_status();
        let results_open = workspace.read(cx).find_results().1;

        // Upstream's `FindPanelFieldAccessoryView` draws the result message at
        // the *trailing edge of the find field*, and draws nothing at all while
        // the field is empty. A row of its own would be a row upstream does not
        // have, and would push the buttons down by its height.
        let message = if query.read(cx).value().is_empty() {
            None
        } else {
            result_message(count, current, error.as_deref())
        };

        v_flex()
            .size_full()
            .gap_2()
            .p_4()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_field(&query, message, window, cx))
            .child(self.render_field(&replacement, None, window, cx))
            .child(options_row(
                self.workspace.clone(),
                regex,
                case_sensitive,
                in_selection,
            ))
            // Upstream's panel is a three-part vertical split — the fields, the
            // result list, the buttons — and the middle part is collapsed until
            // *Find All* finds something. The result item is the one that
            // flexes: upstream's split grows by the same 200 points and the
            // item fills them.
            .when(results_open, |this| {
                this.child(
                    div()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .max_h(rems(RESULT_TABLE_MAX_HEIGHT))
                        .child(self.render_results(window, cx)),
                )
            })
            .child(self.render_buttons(cx))
            .into_any_element()
    }
}

impl FindPanel {
    /// The **Find All** result list: the middle section of upstream's split.
    ///
    /// Upstream's `FindPanelResultView` is a `⌃` collapse button and a bold
    /// `Found N matches in “name.”` on one line, `Find text: query` on the
    /// next, and then a bordered two-column table — the line number, right
    /// aligned in a narrow lane, and the matched line with each capture group
    /// painted in the decomposed highlighter colours.
    fn render_results(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let Some(workspace) = self.workspace.upgrade() else {
            return div().into_any_element();
        };
        let (rows, _) = workspace.read(cx).find_results();
        let count = rows.len();
        let document = workspace.read(cx).document_name();
        let (query, _) = workspace.read(cx).find_fields();
        let query = query.read(cx).value().to_string();
        let font_size = workspace.read(cx).find_result_font_size();

        // The DataTable's columns are fixed-pixel, so the matched-text lane is
        // sized to what the viewport leaves after the line lane. Recomputing it
        // per render is what keeps the column trailing the window's edge the
        // way SwiftUI's last column does.
        let viewport_width = window.bounds().size.width;
        let matched_width = (viewport_width
            - px(2. * 16.)      // the panel's own horizontal padding
            - px(LINE_COLUMN_WIDTH)
            - px(2.)            // the viewport's border
            - px(CELL_HORIZONTAL_SLACK))
        .max(px(120.));
        self.table.update(cx, |table, cx| {
            table.delegate_mut().font_size = px(font_size);
            table.delegate_mut().matched_column_width = matched_width;
            // The table is a live view of the workspace's rows; this refresh is
            // the bridge that replays a workspace change into the table's own
            // render cycle.
            table.refresh(cx);
        });
        let focus_handle = self.table.focus_handle(cx);
        let keyboard_table = self.table.clone();

        let stop = self.workspace.clone();
        v_flex()
            .size_full()
            .gap_1()
            .child(
                h_flex()
                    .flex_none()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("find-result-close")
                            .ghost()
                            .compact()
                            .icon(gpui_kit::assets::IconName::ChevronUp)
                            .accessibility_label("Close")
                            .tooltip("Close find result.")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.on_document(cx, |workspace, _, cx| {
                                    workspace.close_find_results(cx);
                                });
                            })),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .truncate()
                            // Upstream's `.unknown` is the fallback for a window
                            // whose document has gone; this section cannot be
                            // open without one, so the fallback would be a
                            // spelling of "never" rather than a state a reader
                            // meets.
                            .child(format!("Found {count} matches in “{document}.”")),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .text_sm()
                    .truncate()
                    .child(format!("Find text: {query}")),
            )
            .child(
                // The table is the section's scroll owner: the split item is
                // bounded, so a long match list scrolls inside the table's own
                // virtual list rather than growing the window.
                div()
                    .id("find-result-table")
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .w_full()
                    .border_1()
                    .border_color(cx.theme().border)
                    .overflow_hidden()
                    .track_focus(&focus_handle)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        // A click on the table makes it the keyboard target —
                        // the same responder hand-off AppKit does for free.
                        this.table.focus_handle(cx).focus(window, cx);
                        if let Some(workspace) = this.workspace.upgrade() {
                            workspace.update(cx, |workspace, cx| {
                                workspace.focus_find_results(true, cx)
                            });
                        }
                    }))
                    .on_key_down(cx.listener(
                        move |this, event: &KeyDownEvent, _, cx| {
                            let key = event.keystroke.key.as_str();
                            let modifiers = event.keystroke.modifiers;
                            let Some(workspace) = this.workspace.upgrade() else { return };
                            // ⌘C copies the selected rows, upstream's `.copyable`.
                            if key == "c" && (modifiers.platform || modifiers.control) {
                                workspace
                                    .update(cx, |workspace, cx| workspace.copy_find_results(cx));
                                cx.stop_propagation();
                                return;
                            }
                            let count = workspace.read(cx).find_results().0.len();
                            if count == 0 {
                                return;
                            }
                            // The keyboard row is the anchor row: the one a
                            // shift-extension measures from.
                            let anchor = workspace
                                .read(cx)
                                .selected_find_result_ranges()
                                .first()
                                .cloned();
                            let anchor_ix = anchor.and_then(|anchor| {
                                workspace
                                    .read(cx)
                                    .find_results()
                                    .0
                                    .iter()
                                    .position(|row| row.range == anchor)
                            });
                            let target = match key {
                                "up" => {
                                    Some(anchor_ix.map_or(count - 1, |ix| ix.saturating_sub(1)))
                                }
                                "down" => Some(anchor_ix.map_or(0, |ix| (ix + 1).min(count - 1))),
                                "home" => Some(0),
                                "end" => Some(count - 1),
                                _ => None,
                            };
                            if let Some(target) = target {
                                workspace.update(cx, |workspace, cx| {
                                    workspace.select_find_result_index(
                                        target,
                                        modifiers.shift,
                                        cx,
                                    )
                                });
                                keyboard_table.update(cx, |table, cx| {
                                    table.scroll_to_row(target, cx)
                                });
                                cx.stop_propagation();
                            }
                        },
                    ))
                    // Upstream's table context menu is the Text Size menu; the
                    // same three commands are also on the standard Text Size
                    // menu items.
                    .context_menu(move |menu, window, cx| {
                        let bigger = stop.clone();
                        let smaller = stop.clone();
                        let reset = stop.clone();
                        let size_menu = PopupMenu::build(window, cx, move |menu, _, _| {
                            menu.item(PopupMenuItem::new("Bigger").on_click(move |_, _, cx| {
                                bigger
                                    .update(cx, |workspace, cx| {
                                        workspace.step_find_result_font_size(true, cx);
                                    })
                                    .ok();
                            }))
                            .item(PopupMenuItem::new("Smaller").on_click(move |_, _, cx| {
                                smaller
                                    .update(cx, |workspace, cx| {
                                        workspace.step_find_result_font_size(false, cx);
                                    })
                                    .ok();
                            }))
                            .separator()
                            .item(PopupMenuItem::new("Reset to Default").on_click(
                                move |_, _, cx| {
                                    reset
                                        .update(cx, |workspace, cx| {
                                            workspace.reset_find_result_font_size(cx);
                                        })
                                        .ok();
                                },
                            ))
                        });
                        menu.item(PopupMenuItem::submenu("Text Size", size_menu))
                    })
                    .child(DataTable::new(&self.table).small().stripe(true).bordered(false)),
            )
            .into_any_element()
    }
}

/// The state behind the result table.
///
/// The delegate owns no data: rows, selection and the display order all live in
/// the workspace, and every read goes through the weak handle. What it does
/// hold is the two things only the panel window knows — the matched-text
/// column's pixel width and the table's font size — which the panel pushes in
/// on each render.
pub struct FindResultTable {
    workspace: WeakEntity<Workspace>,
    matched_column_width: gpui_kit::Pixels,
    font_size: gpui_kit::Pixels,
}

impl FindResultTable {
    fn new(workspace: WeakEntity<Workspace>) -> Self {
        Self {
            workspace,
            matched_column_width: px(120.),
            font_size: px(11.),
        }
    }
}

impl TableDelegate for FindResultTable {
    fn columns_count(&self, _: &App) -> usize {
        2
    }

    fn rows_count(&self, cx: &App) -> usize {
        self.workspace
            .upgrade()
            .map_or(0, |workspace| workspace.read(cx).find_results().0.len())
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        match col_ix {
            // Upstream's `TableColumn(..., value: \.range.location)` is the
            // only sortable column; the matched-text column carries no value
            // keypath and so does not sort.
            0 => Column::new("line", "Line")
                .width(px(LINE_COLUMN_WIDTH))
                .min_width(px(LINE_COLUMN_WIDTH))
                .max_width(px(LINE_COLUMN_MAX_WIDTH))
                .text_right()
                .sortable(),
            _ => Column::new("matched-text", "Matched Text")
                .width(self.matched_column_width)
                .min_width(px(120.)),
        }
    }

    /// The component's trailing spacer column defaults to 12 points, which
    /// would leave a visible gap after the matched-text column; upstream's
    /// last column runs to the table's edge, so the spacer collapses here.
    fn render_last_empty_col(
        &mut self,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        h_flex().w_0().h_full().flex_shrink_0()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(workspace) = self.workspace.upgrade() else {
            return div().into_any_element();
        };
        let Some(row) = workspace.read(cx).find_results().0.get(row_ix) else {
            return div().into_any_element();
        };
        let row = row.clone();
        let selected = workspace
            .read(cx)
            .selected_find_result_ranges()
            .contains(&row.range);
        let font_size = self.font_size;
        match col_ix {
            0 => result_cell(self.workspace.clone(), row.range, font_size, cx)
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .justify_end()
                .pr_2()
                .when(selected, |cell| cell.bg(cx.theme().tokens.table_active))
                .child(row.line_number.to_string())
                .into_any_element(),
            _ => {
                let colors = highlighter_colors(row.capture_ranges.len());
                let (displayed, capture_ranges) = truncate_head_with_ranges(
                    &row.line,
                    row.inline_location,
                    CELL_HEAD_OFFSET,
                    &row.capture_ranges,
                );
                result_cell(self.workspace.clone(), row.range, font_size, cx)
                    .w_full()
                    .h_full()
                    .min_w_0()
                    .pl_2()
                    .overflow_hidden()
                    .when(selected, |cell| cell.bg(cx.theme().tokens.table_active))
                    .child(render_highlighted_line(
                        &displayed,
                        &capture_ranges,
                        &colors,
                        font_size,
                    ))
                    .into_any_element()
            }
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        if col_ix != 0 {
            return;
        }
        let descending = matches!(sort, ColumnSort::Descending);
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.sort_find_results(descending, cx)
            });
        }
    }
}

/// One result cell: the text with a modifier-aware click that reports the row's
/// match to the workspace, where the selection set lives.
///
/// The handler sits on the cell because the DataTable consumes row clicks
/// itself and never surfaces the event's modifiers; a plain `⌘`/`⇧` click has
/// to be read at the element that was actually hit.
fn result_cell(
    workspace: WeakEntity<Workspace>,
    range: Range<usize>,
    font_size: gpui_kit::Pixels,
    cx: &mut Context<TableState<FindResultTable>>,
) -> gpui_kit::Div {
    div()
        .text_size(font_size)
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_, event: &gpui_kit::MouseDownEvent, _, cx| {
                let range = range.clone();
                workspace
                    .update(cx, |workspace, cx| {
                        workspace.select_find_result_range(range, event.modifiers, cx)
                    })
                    .ok();
            }),
        )
}

/// The matched line's cell content: the displayed fragment with each capture
/// group painted in its decomposed highlighter colour — group 0 included, the
/// way upstream builds the attributed line string.
///
/// Where groups overlap, the finest participating group wins: upstream adds
/// the background attributes in group order, so a later group replaces the
/// earlier one's colour on the range they share.
fn render_highlighted_line(
    displayed: &str,
    capture_ranges: &[Option<Range<usize>>],
    colors: &[gpui_kit::Hsla],
    font_size: gpui_kit::Pixels,
) -> gpui_kit::AnyElement {
    let owner = capture_owners(displayed.len(), capture_ranges);

    let mut content = h_flex().min_w_0().items_center().gap_0();
    let mut cursor = 0;
    while cursor < displayed.len() {
        let Some(group) = owner[cursor] else {
            // One plain run up to the next painted byte.
            let run_end = owner[cursor..]
                .iter()
                .position(|slot| slot.is_some())
                .map_or(displayed.len(), |offset| cursor + offset);
            if let Some(text) = displayed.get(cursor..run_end) {
                content = content.child(div().text_size(font_size).child(text.to_string()));
            }
            cursor = run_end;
            continue;
        };
        let run_end = cursor
            + owner[cursor..]
                .iter()
                .take_while(|slot| **slot == Some(group))
                .count();
        if let Some(text) = displayed.get(cursor..run_end) {
            content = content.child(
                div()
                    .text_size(font_size)
                    .bg(colors[group % colors.len()])
                    .child(text.to_string()),
            );
        }
        cursor = run_end;
    }
    div().w_full().min_w_0().truncate().child(content).into_any_element()
}

/// The owning group per byte of the displayed fragment — `None` where no
/// capture paints. Later (finer) groups overwrite earlier ones, matching the
/// attribute-stack order upstream adds them in.
fn capture_owners(displayed_len: usize, capture_ranges: &[Option<Range<usize>>]) -> Vec<Option<usize>> {
    let mut owner: Vec<Option<usize>> = vec![None; displayed_len];
    for (capture_ix, capture) in capture_ranges.iter().enumerate() {
        let Some(range) = capture.as_ref() else { continue };
        let end = range.end.min(displayed_len);
        for slot in owner.iter_mut().take(end).skip(range.start) {
            *slot = Some(capture_ix);
        }
    }
    owner
}

/// How opaque the capture-group paints are: upstream's
/// `textHighlighterColor` is the accent colour `withAlphaComponent(0.4)`.
const HIGHLIGHTER_ALPHA: f32 = 0.4;

/// The base colour the capture sequence decomposes, as calibrated HSB.
///
/// Upstream decomposes the *system accent colour* (`NSColor.accent` from
/// WFColorCode); macOS exposes no system-accent colour to GPUI, so SubText
/// fixes the sequence to the measured default accent blue —
/// `controlAccentColor` in the genericRGB space reads
/// `hue 0.6085, saturation 0.9591, brightness 0.9984`. Following a user's
/// customised accent is out of reach and is recorded in FEATURE-MATRIX.
const HIGHLIGHTER_BASE: (f32, f32, f32) = (0.6085, 0.9591, 0.9984);

/// The per-capture-group background colours — upstream's
/// `NSColor.textHighlighterColor.decompose(into: numberOfCaptureGroups + 1)`.
///
/// The decomposition walks the hue wheel in even steps from the base colour,
/// holding saturation and brightness. This is data produced by the upstream
/// algorithm rather than a theme role, and it is the third of the
/// colour-literal exceptions recorded in FEATURE-MATRIX.
fn highlighter_colors(groups: usize) -> Vec<gpui_kit::Hsla> {
    let groups = groups.max(1);
    let (hue, saturation, brightness) = HIGHLIGHTER_BASE;
    (0..groups)
        .map(|index| {
            let hue = (hue + index as f32 / groups as f32) % 1.0;
            let (saturation, lightness) = hsb_components_to_hsl(saturation, brightness);
            hsla(hue, saturation, lightness, HIGHLIGHTER_ALPHA)
        })
        .collect()
}

/// HSB → HSL's saturation/lightness pair, keeping hue.
fn hsb_components_to_hsl(saturation: f32, brightness: f32) -> (f32, f32) {
    let lightness = brightness * (1. - saturation / 2.);
    let denominator = 1. - (2. * lightness - 1.).abs();
    let saturation = if denominator <= 0. { 0. } else { brightness * saturation / denominator };
    (saturation, lightness)
}

/// The selection model behind the result table's clicks and arrow keys.
///
/// A plain click selects that one row; ⇧ grows the selection to the
/// display-order span from the anchor; ⌘ toggles the row in the set. The
/// anchor — the row extensions measure from — is returned so the caller can
/// keep it with the set. `visible` is the current display order and every
/// index into it is a row position.
pub(crate) fn apply_row_selection(
    selected: &[Range<usize>],
    anchor: Option<usize>,
    target: usize,
    shift: bool,
    command: bool,
    visible: &[Range<usize>],
) -> (Vec<Range<usize>>, Option<usize>) {
    if shift && let Some(anchor) = anchor {
        let (lo, hi) = if anchor <= target { (anchor, target) } else { (target, anchor) };
        return (visible[lo..=hi].to_vec(), Some(anchor));
    }
    if command {
        let mut next = selected.to_vec();
        let row = &visible[target];
        if let Some(position) = next.iter().position(|range| range == row) {
            next.remove(position);
        } else {
            next.push((*row).clone());
        }
        return (next, Some(target));
    }
    (vec![visible[target].clone()], Some(target))
}

impl FindPanel {
    /// One of the panel's two text fields.
    ///
    /// `message` is the result line upstream draws at the field's trailing edge
    /// — `None` while the field is empty, which is when upstream's accessory
    /// view is not drawn either. The clear button is part of that same
    /// accessory: `[message] [Clear]`, in that order.
    ///
    /// Upstream strokes the field with the separator colour at 7 points and
    /// gives it an *exterior* focus ring. The component's own input would draw
    /// a second box inside that one, so its appearance is turned off and the
    /// ring is drawn here instead — one box, as upstream has one.
    fn render_field(
        &self,
        state: &Entity<InputState>,
        message: Option<(String, bool)>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let danger = cx.theme().danger;
        let muted = cx.theme().muted_foreground;
        let border = if state.read(cx).focus_handle(cx).is_focused(window) {
            cx.theme().ring
        } else {
            cx.theme().border
        };
        let state = state.clone();

        div()
            .h(rems(FIELD_HEIGHT))
            .w_full()
            .rounded(px(7.))
            .border_1()
            .border_color(border)
            .overflow_hidden()
            .child(
                Input::new(&state)
                    .appearance(false)
                    .w_full()
                    .h_full()
                    .when_some(message, |input, (text, is_error)| {
                        input.suffix(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .child(
                                    div()
                                        .max_w(rems(MESSAGE_MAX_WIDTH))
                                        .truncate()
                                        .text_sm()
                                        .text_color(if is_error { danger } else { muted })
                                        .child(text),
                                )
                                .child(
                                    Button::new("find-clear")
                                        .ghost()
                                        .compact()
                                        .icon(gpui_kit::assets::IconName::CircleX)
                                        .accessibility_label("Clear")
                                        .tooltip("Clear the field.")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            state.update(cx, |state, cx| {
                                                state.set_value("", window, cx);
                                            });
                                            if let Some(workspace) = this.workspace.upgrade() {
                                                workspace.update(cx, |workspace, cx| {
                                                    workspace.refresh_matches(cx);
                                                });
                                            }
                                        })),
                                ),
                        )
                    }),
            )
    }

    /// The button row: `[Find All ▾] [Replace All] … [Replace] [◀|▶]`.
    fn render_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(
                // Upstream's *Find All* is a menu with a primary action: the
                // click finds, the chevron offers the two other ways of using
                // the same matches.
                Button::new("find-all")
                    .small()
                    .label("Find All")
                    .tooltip("Find and list all matches.")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.on_document(cx, |workspace, _window, cx| workspace.find_all(cx));
                        let _ = window;
                    })),
            )
            .child(
                Button::new("replace-all")
                    .small()
                    .label("Replace All")
                    .tooltip("Replace all matches with the replacement text.")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.on_document(cx, |workspace, window, cx| {
                            workspace.replace_all(window, cx)
                        });
                    })),
            )
            .child(div().flex_1())
            .child(
                Button::new("replace")
                    .small()
                    .label("Replace")
                    .tooltip("Replace the current selection with the replacement text, then find the next match.")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.on_document(cx, |workspace, window, cx| {
                            workspace.replace_and_find_next(window, cx)
                        });
                    })),
            )
            // Upstream's `ControlGroup` of two icon-only buttons, 70 points
            // wide between them.
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("find-previous")
                            .small()
                            .icon(gpui_kit::assets::IconName::ChevronLeft)
                            .accessibility_label("Find Previous")
                            .tooltip("Find previous match.")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.on_document(cx, |workspace, _window, cx| {
                                    workspace.find_step(false, cx)
                                });
                            })),
                    )
                    .child(
                        Button::new("find-next")
                            .small()
                            .icon(gpui_kit::assets::IconName::ChevronRight)
                            .accessibility_label("Find Next")
                            .tooltip("Find next match.")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.on_document(cx, |workspace, _window, cx| {
                                    workspace.find_step(true, cx)
                                });
                            })),
                    ),
            )
    }
}

/// The short result message, spelled the way upstream's `FindResult` spells it.
///
/// `positionMessage` wins over `message` while one match is current — and it is
/// `index/count`, *not* "index of count". The second field of the pair says
/// whether the text is an error, which is drawn in the danger colour.
fn result_message(count: usize, current: usize, error: Option<&str>) -> Option<(String, bool)> {
    match error {
        Some(error) => Some((error.to_string(), true)),
        None if count > 0 && current > 0 => Some((format!("{current}/{count}"), false)),
        None if count > 0 => Some((format!("{count} matches"), false)),
        None => None,
    }
}

/// The row of switches between the fields and the buttons.
fn options_row(
    workspace: WeakEntity<Workspace>,
    regex: bool,
    case_sensitive: bool,
    in_selection: bool,
) -> impl IntoElement {
    let switch = |id: &'static str, label: &'static str, checked: bool, field: fn(&mut Workspace, bool)| {
        let workspace = workspace.clone();
        Checkbox::new(id)
            .checked(checked)
            .label(label)
            .on_click(move |checked, _, cx| {
                let checked = *checked;
                workspace
                    .update(cx, |workspace, cx| {
                        field(workspace, checked);
                        let _ = workspace.settings.save();
                        workspace.refresh_matches(cx);
                    })
                    .ok();
            })
    };

    h_flex()
        .w_full()
        .gap_4()
        .items_center()
        .child(switch("find-regex", "Regular Expression", regex, |workspace, on| {
            workspace.settings.find_regex = on;
        }))
        .child(switch(
            "find-case",
            "Ignore Case",
            !case_sensitive,
            |workspace, on| {
                workspace.settings.find_case_sensitive = !on;
            },
        ))
        .child(switch(
            "find-in-selection",
            "In Selection",
            in_selection,
            |workspace, on| {
                workspace.settings.find_in_selection = on;
            },
        ))
        .child(div().flex_1())
}

#[cfg(test)]
mod tests {
    use super::{apply_row_selection, hsb_components_to_hsl, highlighter_colors, result_message};

    #[test]
    fn a_plain_click_selects_one_row_and_moves_the_anchor() {
        let visible = vec![0..5, 11..16, 20..24];
        let (selected, anchor) = apply_row_selection(&[], None, 1, false, false, &visible);
        assert_eq!(selected, vec![11..16]);
        assert_eq!(anchor, Some(1));
    }

    #[test]
    fn a_shift_click_selects_the_display_order_span_from_the_anchor() {
        let visible = vec![0..5, 11..16, 20..24];
        let (selected, anchor) =
            apply_row_selection(std::slice::from_ref(&(0..5)), Some(0), 2, true, false, &visible);
        assert_eq!(selected, vec![0..5, 11..16, 20..24]);
        // The anchor stays where the extension started from.
        assert_eq!(anchor, Some(0));
    }

    #[test]
    fn a_backwards_shift_click_extends_upwards_from_the_anchor() {
        let visible = vec![0..5, 11..16, 20..24];
        let (selected, anchor) = apply_row_selection(
            std::slice::from_ref(&(20..24)),
            Some(2),
            0,
            true,
            false,
            &visible,
        );
        assert_eq!(selected, visible);
        assert_eq!(anchor, Some(2));
    }

    #[test]
    fn a_command_click_toggles_a_row_in_the_set() {
        let visible = vec![0..5, 11..16, 20..24];
        let (selected, anchor) = apply_row_selection(
            &[0..5, 11..16],
            Some(0),
            1,
            false,
            true,
            &visible,
        );
        assert_eq!(selected, vec![0..5]);
        assert_eq!(anchor, Some(1));

        let (selected, anchor) = apply_row_selection(&selected, anchor, 1, false, true, &visible);
        assert_eq!(selected, vec![0..5, 11..16]);
        assert_eq!(anchor, Some(1));
    }

    #[test]
    fn a_shift_click_without_an_anchor_behaves_like_a_plain_click() {
        let visible = vec![0..5, 11..16];
        let (selected, anchor) = apply_row_selection(&[], None, 1, true, false, &visible);
        assert_eq!(selected, vec![11..16]);
        assert_eq!(anchor, Some(1));
    }

    #[test]
    fn the_highlighter_sequence_walks_the_hue_wheel_from_the_base() {
        let one = highlighter_colors(1);
        assert_eq!(one.len(), 1);
        assert!((one[0].h - 0.6085).abs() < 1e-4);
        assert_eq!(one[0].a, 0.4);

        // The hue steps by one nth of the wheel; saturation and brightness are
        // held, so every entry decodes to the same HSL pair.
        let four = highlighter_colors(4);
        let (base_hue, saturation, brightness) = (0.6085_f32, 0.9591, 0.9984);
        let (s, l) = hsb_components_to_hsl(saturation, brightness);
        for (index, color) in four.iter().enumerate() {
            let expected_hue = (base_hue + index as f32 / 4.) % 1.;
            assert!((color.h - expected_hue).abs() < 1e-4);
            assert!((color.s - s).abs() < 1e-4);
            assert!((color.l - l).abs() < 1e-4);
            assert_eq!(color.a, 0.4);
        }
    }

    #[test]
    fn overlapping_groups_paint_with_the_finest_participating_group() {
        // Upstream adds the background attributes in group order, so on the
        // range a finer group shares with group 0 the finer colour wins —
        // "alpha" inside "alpha-123" is group 1's colour, not group 0's.
        use super::capture_owners;
        let owners = capture_owners(
            9,
            &[Some(0..9), Some(0..5), Some(5..6), Some(6..9)],
        );
        assert_eq!(
            owners,
            vec![Some(1), Some(1), Some(1), Some(1), Some(1), Some(2), Some(3), Some(3), Some(3)]
        );
    }

    #[test]
    fn a_group_that_participated_but_fell_out_of_the_fragment_paints_nothing() {
        use super::capture_owners;
        // The third group matched past the fragment's end, and the second did
        // not participate at all.
        let owners = capture_owners(5, &[Some(0..5), Some(0..5), None, Some(8..10)]);
        assert_eq!(owners, vec![Some(1), Some(1), Some(1), Some(1), Some(1)]);
    }

    #[test]
    fn the_hsb_to_hsl_conversion_matches_the_decomposition_output() {
        // The colours the table paints are the conversion's own output of the
        // measured base: a strongly saturated, very light blue.
        let (s, l) = hsb_components_to_hsl(0.9591, 0.9984);
        assert!(s > 0.9 && s <= 1.0);
        assert!((l - 0.9984 * (1. - 0.9591 / 2.)).abs() < 1e-4);
    }

    #[test]
    fn a_current_match_is_reported_as_a_position_not_a_phrase() {
        // Upstream: `"\(index)/\(count)"`. "2 of 3" would be a different string
        // in the same place, and the two are not interchangeable.
        assert_eq!(result_message(3, 2, None), Some(("2/3".to_string(), false)));
    }

    #[test]
    fn a_count_with_no_current_match_is_a_plain_count() {
        assert_eq!(result_message(3, 0, None), Some(("3 matches".to_string(), false)));
    }

    #[test]
    fn nothing_found_and_nothing_asked_for_are_both_silent() {
        assert_eq!(result_message(0, 0, None), None);
    }

    #[test]
    fn an_error_replaces_the_count_and_is_marked_as_one() {
        assert_eq!(
            result_message(0, 0, Some("Invalid regular expression")),
            Some(("Invalid regular expression".to_string(), true))
        );
    }
}
