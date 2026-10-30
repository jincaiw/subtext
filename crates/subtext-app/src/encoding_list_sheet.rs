//! The *Edit List…* sheet behind `Format ▸ Text Encoding`.
//!
//! CotEditor's `EncodingListView`: the encoding priority list as the user can
//! change it — add an encoding or a separator, take one out, move one up or
//! down, or put the shipped list back. SubText opens it as a dialog against the
//! settings window, because that is where the button that opens it lives.
//!
//! # The sheet edits a copy
//!
//! Nothing reaches the settings until *OK*. Upstream's model works on its own
//! array and calls `save()` on submit, so Cancel — and the dialog's Escape —
//! leaves the settings exactly as they were. The copy is also what *Restore
//! Defaults* is measured against: the button is live exactly when the working
//! list differs from the shipped one.
//!
//! # What the sheet refuses
//!
//! Two entries cannot be taken out, and the sheet asks
//! [`EncodingList::can_remove`] rather than deciding for itself: UTF-8, which is
//! what a file with no mark and no declaration is tried against first, and the
//! encoding new documents are created with. Upstream disables the Delete button
//! and puts the reason in its tooltip, which is what this does — the refusal is
//! the same call that greys the button, so the rule and the message cannot
//! drift apart.
//!
//! # What is done differently, and why
//!
//! Upstream's list is a SwiftUI `List` with `onMove` and `onDeleteCommand`, so
//! the user drags a row and presses Delete to remove one. GPUI's component
//! library has no drag-to-reorder and no list that reports a Delete key, so the
//! moves are made with arrow buttons and the removals with the Delete button.
//! The capability is the same in both directions — any entry can be moved a
//! place at a time and any removable entry can go — but the gestures are not,
//! and both gaps are recorded in `docs/PLAN.md`. The line above the list says
//! what the gestures are, which is why it reads *Move encodings up or down*
//! where upstream's reads *Drag encodings to change the order*: an instruction
//! the user cannot follow is worse than a differently worded one they can.
//!
//! Selection is by row, and it is what the buttons act on: one row, or several
//! with ⇧-click and ⌘-click, which is what upstream's `Set<Item.ID>` holds. The
//! keyboard path is the buttons themselves — each row, each move and both
//! commands are `Button`s, so Tab reaches them and Enter or Space fires them.
//!
//! # The editing methods are pure
//!
//! None of them takes a context: they change the list and the rows and stop
//! there, and the click handlers call `cx.notify()` once afterwards. That keeps
//! the whole of the editing rules — the refusals, the insertion point, the
//! shape of a move — testable without a window, which is where every rule that
//! can be wrong about the *data* lives.

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::Button,
    h_flex,
    list::ListItem,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex,
};
use gpui_kit::{
    Anchor, AnyElement, App, AppContext as _, ClickEvent, Context, FocusHandle,
    InteractiveElement as _, IntoElement, KeyDownEvent, Modifiers, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, rems,
};

use subtext_core::encoding::{EncodingList, FileEncoding, TextEncoding};

use crate::workspace::Workspace;

/// How tall the list is, in rem.
///
/// Upstream asks for `minHeight: 100, idealHeight: 250` points; this is the
/// ideal, which is about what the list settles at on the default list of
/// sixty-odd entries.
const LIST_HEIGHT: f32 = 15.;

/// What the Delete button is allowed to do with the current selection.
///
/// Upstream returns a `RemovalError?` from `canRemove(ids:)` and treats a `nil`
/// message as "no tooltip", which is the empty selection. Three cases read
/// better than two nested options.
enum Removal {
    /// Nothing is selected. The button is greyed and says nothing about why.
    Nothing,
    /// The selection holds an entry that cannot go.
    Refused(String),
    /// The selection can go.
    Allowed,
}

/// One row of the editor: an entry of the list, plus the identity the row keeps
/// while the user moves it around.
///
/// The identity cannot be the row's position, because a move changes it; and it
/// cannot come from the entry, because a separator has nothing to derive one
/// from and a hand-edited settings file can name one encoding twice. A counter
/// is the only thing that is unique and stable at once.
struct Row {
    id: u64,
    item: Option<TextEncoding>,
}

/// The encoding priority list editor.
pub struct EncodingListSheet {
    workspace: WeakEntity<Workspace>,
    /// The working copy. Written back on submit and thrown away otherwise.
    list: EncodingList,
    /// The rows, parallel to `list`. Every mutation below moves both together,
    /// and [`assert_in_step`](Self::assert_in_step) is what says so.
    rows: Vec<Row>,
    next_id: u64,
    /// The selected rows, by identity.
    selection: Vec<u64>,
    /// Where a ⇧-click extends from.
    anchor: Option<u64>,
    /// The encoding new documents are created with — the second entry that
    /// cannot be taken out.
    default_encoding: TextEncoding,
    /// What the Add menu offers: the registry less what the list holds, sorted
    /// by name. Rebuilt whenever the list changes rather than on every frame,
    /// because it sorts the whole registry.
    addable: Vec<TextEncoding>,
    /// The list's own focus, which is where ⌫ arrives.
    ///
    /// Upstream hangs `.onDeleteCommand` off the `List`, so the key only
    /// deletes while the list is what has the focus. The same here: the rows
    /// are not focusable themselves, and clicking one focuses the list.
    ///
    /// `None` only in tests, which build the sheet without a window: a
    /// `FocusHandle` cannot be made without an `App`, and every rule the tests
    /// are about lives in the model rather than in this handle.
    focus: Option<FocusHandle>,
}

impl EncodingListSheet {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        list: EncodingList,
        default_encoding: TextEncoding,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            workspace,
            rows: Vec::new(),
            list,
            next_id: 0,
            selection: Vec::new(),
            anchor: None,
            default_encoding,
            addable: Vec::new(),
            focus: Some(cx.focus_handle()),
        };
        this.rebuild_rows();
        this.refresh(cx);
        this
    }

    /// Write the list back to the settings, and rebuild the menu bar.
    ///
    /// The menu is rebuilt because the list *is* the `Format ▸ Text Encoding`
    /// submenu's contents — the same reason the Snippets pane rebuilds it.
    pub fn submit(&mut self, cx: &mut App) {
        let list = self.list.clone();
        self.workspace
            .update_in(cx, move |workspace, window, cx| {
                workspace.settings.encoding_list = list;
                workspace.apply_settings(window, cx);
                workspace.rebuild_menus(cx);
            })
            .ok();
    }

    // ---- the two parallel structures -------------------------------------

    /// Give every entry of the list a fresh row.
    ///
    /// Used when the list is replaced wholesale rather than edited — opening
    /// the sheet, and *Restore Defaults*. The identities are new because the
    /// rows are: nothing that was selected before survives.
    fn rebuild_rows(&mut self) {
        let mut rows = Vec::with_capacity(self.list.len());
        for index in 0..self.list.len() {
            let id = self.next_id;
            self.next_id += 1;
            rows.push(Row {
                id,
                item: self.list.item(index).flatten(),
            });
        }
        self.rows = rows;
    }

    /// The two structures hold the same entries in the same order.
    ///
    /// A debug assertion rather than a test, because it is an invariant of
    /// every method below rather than a behaviour: the tests call it after each
    /// edit, and a debug build calls it after every edit the user makes.
    fn assert_in_step(&self) {
        debug_assert_eq!(
            self.rows.len(),
            self.list.len(),
            "the rows and the list hold different numbers of entries"
        );
        for (index, row) in self.rows.iter().enumerate() {
            debug_assert_eq!(
                row.item,
                self.list.item(index).flatten(),
                "the rows and the list disagree at {index}"
            );
        }
    }

    /// Rebuild what is derived from the list, and drop anything the list no
    /// longer holds.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.assert_in_step();
        self.addable = self.list.addable();
        self.selection
            .retain(|id| self.rows.iter().any(|row| row.id == *id));
        if let Some(anchor) = self.anchor
            && !self.rows.iter().any(|row| row.id == anchor)
        {
            self.anchor = None;
        }
        cx.notify();
    }

    // ---- selection -------------------------------------------------------

    fn index_of(&self, id: u64) -> Option<usize> {
        self.rows.iter().position(|row| row.id == id)
    }

    fn is_selected(&self, id: u64) -> bool {
        self.selection.contains(&id)
    }

    /// Handle a click on a row, the way a `List(selection:)` does.
    ///
    /// A plain click replaces the selection and sets the anchor; ⌘-click
    /// toggles the row; ⇧-click takes everything between the anchor and the row;
    /// and ⇧⌘-click adds that range to what is already selected. Those are
    /// SwiftUI's four, which is what upstream's list is.
    fn click(&mut self, id: u64, modifiers: Modifiers) {
        let Some(index) = self.index_of(id) else {
            return;
        };

        match (modifiers.shift, modifiers.platform || modifiers.control) {
            (true, true) => {
                for row in self.range_to_anchor(index) {
                    if !self.selection.contains(&row) {
                        self.selection.push(row);
                    }
                }
            }
            (true, false) => {
                self.selection = self.range_to_anchor(index);
            }
            (false, true) => {
                match self.selection.iter().position(|selected| *selected == id) {
                    Some(position) => {
                        self.selection.remove(position);
                    }
                    None => self.selection.push(id),
                }
                self.anchor = Some(id);
            }
            (false, false) => {
                self.selection = vec![id];
                self.anchor = Some(id);
            }
        }
    }

    /// The row identities from the anchor to `index`, inclusive.
    ///
    /// With no anchor — the first click being a ⇧-click — the row is its own
    /// range, which is what a `List` does with nothing to extend from.
    fn range_to_anchor(&self, index: usize) -> Vec<u64> {
        let Some(anchor) = self.anchor.and_then(|anchor| self.index_of(anchor)) else {
            return self
                .rows
                .get(index)
                .map(|row| vec![row.id])
                .unwrap_or_default();
        };
        let (start, end) = if anchor <= index {
            (anchor, index)
        } else {
            (index, anchor)
        };
        self.rows[start..=end].iter().map(|row| row.id).collect()
    }

    // ---- editing ---------------------------------------------------------

    /// Where a new entry goes: below the last row the user selected, or at the
    /// top when they have selected nothing.
    ///
    /// Upstream's own placement, from the help page: "select the item at the
    /// location to insert, click the Add + button".
    fn insertion_point(&self) -> Option<usize> {
        self.selection
            .iter()
            .filter_map(|id| self.index_of(*id))
            .max()
    }

    /// Put `item` in at the insertion point and select it, so that the next Add
    /// lands below it — which is how a run of additions is made.
    fn add(&mut self, item: Option<TextEncoding>) {
        let index = self.list.insert_after(self.insertion_point(), item);
        let id = self.next_id;
        self.next_id += 1;
        self.rows.insert(index, Row { id, item });
        self.selection = vec![id];
        self.anchor = Some(id);
    }

    /// What the Delete button may do with the selection.
    ///
    /// The rows are walked in list order rather than in the order they were
    /// selected, because that is what upstream's `canRemove(ids:)` does — it
    /// filters `items` and takes the first refusal — and the two disagree when
    /// the selection holds both UTF-8 and the default encoding: the message
    /// belongs to whichever is nearer the top of the list.
    fn removal(&self) -> Removal {
        if self.selection.is_empty() {
            return Removal::Nothing;
        }
        for (index, row) in self.rows.iter().enumerate() {
            if !self.is_selected(row.id) {
                continue;
            }
            if let Some(error) = self.list.can_remove(index, self.default_encoding) {
                // Both refusals are about encodings, so there is always a name
                // to put in the message; a separator can never reach here.
                let name = row.item.map_or("", |encoding| encoding.name);
                return Removal::Refused(error.message(name));
            }
        }
        Removal::Allowed
    }

    /// Take every removable selected row out.
    ///
    /// Walking backwards is what keeps the indices valid as the rows come out;
    /// the selection is cleared afterwards, which is what upstream's
    /// `remove(ids:)` leaves behind too.
    fn remove_selected(&mut self) {
        if !matches!(self.removal(), Removal::Allowed) {
            return;
        }
        let mut indices: Vec<usize> = self
            .selection
            .iter()
            .filter_map(|id| self.index_of(*id))
            .collect();
        indices.sort_unstable();
        for index in indices.into_iter().rev() {
            self.list.remove(index);
            self.rows.remove(index);
        }
        self.selection.clear();
        self.anchor = None;
    }

    /// Move the rows that are being dragged so that they land at `target`.
    ///
    /// Dragging a selected row drags the whole selection, which is what
    /// SwiftUI's `List` does; dragging an unselected one drags just that row.
    /// The run keeps its internal order either way.
    ///
    /// Dropping *onto* a row means taking that row's place: the run goes where
    /// the row was and the row steps aside. So a run dragged downwards lands
    /// after its target and one dragged upwards lands before it — the only rule
    /// under which dropping on the last row can reach the end of the list.
    fn move_dragged(&mut self, dragged: u64, target: u64) {
        let (Some(dragged_index), Some(target_index)) =
            (self.index_of(dragged), self.index_of(target))
        else {
            return;
        };
        let mut from: Vec<usize> = if self.is_selected(dragged) {
            self.selection
                .iter()
                .filter_map(|id| self.index_of(*id))
                .collect()
        } else {
            vec![dragged_index]
        };
        from.sort_unstable();
        if from.contains(&target_index) {
            return;
        }
        let downwards = target_index > *from.last().expect("a drag carries at least one row");
        let to = if downwards {
            target_index + 1
        } else {
            target_index
        };
        self.move_run(&from, to);
    }

    /// Take the rows at `from` out and put them back before `to`.
    ///
    /// `to` is an index into the list as it stands, which is how Foundation's
    /// `move(fromOffsets:toOffset:)` counts — so the landing place is `to` less
    /// however many of the moved rows sat above it. Both structures are moved
    /// in the same pass, which is what keeps them in step.
    fn move_run(&mut self, from: &[usize], to: usize) {
        if from.is_empty() || to > self.rows.len() {
            return;
        }
        let landing = landing_index(from, to, self.rows.len());
        let mut moved = Vec::with_capacity(from.len());
        for index in from.iter().rev() {
            let item = self.list.item(*index).flatten();
            self.list.remove(*index);
            moved.push((self.rows.remove(*index), item));
        }
        moved.reverse();
        for (offset, (row, item)) in moved.into_iter().enumerate() {
            self.rows.insert(landing + offset, row);
            self.list.insert_at(landing + offset, item);
        }
    }

    /// Put the shipped list back and forget the selection.
    fn restore_defaults(&mut self) {
        self.list = EncodingList::defaults();
        self.rebuild_rows();
        self.selection.clear();
        self.anchor = None;
    }

    // ---- rendering -------------------------------------------------------

    /// One row: the name with the spelling it is stored by beside it, or a rule
    /// where the list holds a separator.
    ///
    /// The separator is a row like any other rather than a gap, because it is
    /// one: the user can select it, move it and delete it, which is what
    /// upstream's `EncodingItem.separator` is for.
    ///
    /// A UTF-8 entry draws *two* lines, as upstream's `EncodingView` pair does:
    /// with and without a byte-order mark. The pair is one entry of the list —
    /// the mark is a property of the file, not a second codec — but it is two
    /// items of the Encoding menu, and the editor is the only place where that
    /// shows.
    fn render_row(&self, row: &Row, cx: &Context<Self>) -> AnyElement {
        let id = row.id;
        let sheet = cx.entity().downgrade();
        let on_click = move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
            let modifiers = event.modifiers();
            sheet
                .update(cx, |sheet, cx| {
                    // Clicking a row takes the focus to the list, which is
                    // where ⌫ arrives — upstream's `List` does the same, and it
                    // is what makes `.onDeleteCommand` work without selecting a
                    // row twice.
                    if let Some(focus) = &sheet.focus {
                        focus.focus(window, cx);
                    }
                    sheet.click(id, modifiers);
                    cx.notify();
                })
                .ok();
        };

        let accessible_label = match row.item {
            Some(encoding) if encoding == TextEncoding::UTF8 => format!(
                "{}, {}",
                encoding.name,
                FileEncoding::utf8_with_bom().name()
            ),
            Some(encoding) => encoding.name.to_string(),
            None => "Separator".to_string(),
        };

        // What the pointer carries: the row's identity, and the label the
        // floating preview draws. The preview is an entity of its own because
        // GPUI draws it above the window for as long as the drag lasts.
        let label: SharedString = match row.item {
            Some(encoding) => encoding.name.into(),
            None => "Separator".into(),
        };
        let dragged = Dragging {
            id,
            label: label.clone(),
        };
        let dropped = {
            let sheet = cx.entity().downgrade();
            move |dragged: &Dragging, _window: &mut Window, cx: &mut App| {
                let dragged = dragged.id;
                sheet
                    .update(cx, |sheet, cx| {
                        sheet.move_dragged(dragged, id);
                        sheet.refresh(cx);
                    })
                    .ok();
            }
        };
        // The row under the pointer, while a row is in flight. A drop lands on
        // the row that lights up, so the highlight *is* the answer to "where
        // will this go?" — the same thing SwiftUI's insertion gap says.
        let target = cx.theme().accent.opacity(0.15);

        let row_item = ListItem::new(("encoding-row", id))
            .selected(self.is_selected(id))
            .w_full()
            .px_2()
            .py_1()
            .accessibility_label(accessible_label)
            .on_click(on_click)
            .on_drag(dragged, |drag: &Dragging, _, _, cx| {
                let label = drag.label.clone();
                cx.new(|_| DragPreview { label })
            })
            .on_drop::<Dragging>(dropped)
            .drag_over::<Dragging>(move |style, _, _, _| style.bg(target));

        match row.item {
            Some(encoding) => {
                let muted = cx.theme().muted_foreground;
                // Upstream prints the IANA name beside each line, which is the
                // same spelling for both halves of the UTF-8 pair.
                let line = |name: String, identifier: &str| {
                    h_flex()
                        .w_full()
                        .gap_3()
                        .justify_between()
                        .child(div().child(name))
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(identifier.to_string()),
                        )
                };
                let mut body = v_flex()
                    .w_full()
                    .gap_1()
                    .child(line(encoding.name.to_string(), encoding.identifier()));
                if encoding == TextEncoding::UTF8 {
                    body = body.child(line(
                        FileEncoding::utf8_with_bom().name(),
                        encoding.identifier(),
                    ));
                }
                row_item.child(body).into_any_element()
            }
            None => row_item
                .child(div().w_full().border_t_1().border_color(cx.theme().border))
                .into_any_element(),
        }
    }

    /// The list itself: one scroll region, with the rows inside it.
    ///
    /// The frame is the scroll owner and the inset belongs to the column inside
    /// it, so the scrollbar runs down the frame's inner edge instead of through
    /// the padding.
    fn render_list(&self, cx: &Context<Self>) -> AnyElement {
        let mut rows = v_flex().gap_0p5().p_1();

        for row in self.rows.iter() {
            rows = rows.child(self.render_row(row, cx));
        }

        let mut list = div()
            .id("encoding-list")
            .h(rems(LIST_HEIGHT))
            .overflow_y_scroll()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            // Upstream hangs `.onDeleteCommand` off its `List`, so ⌫ deletes
            // only while the list has the focus. Clicking a row takes it there.
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if matches!(event.keystroke.key.as_str(), "backspace" | "delete") {
                    this.remove_selected();
                    this.refresh(cx);
                    cx.stop_propagation();
                }
            }));
        if let Some(focus) = &self.focus {
            list = list.track_focus(focus);
        }
        list.child(rows).into_any_element()
    }

    /// The Add button: a separator, then every encoding the list does not hold.
    ///
    /// Upstream's `Menu` with a plus, down to the two sections and the order —
    /// `String.availableStringEncodings` less what is listed, sorted by name.
    fn render_add(&self, cx: &Context<Self>) -> AnyElement {
        let sheet = cx.entity().downgrade();
        let addable = self.addable.clone();

        Button::new("encoding-add")
            .small()
            .icon(gpui_kit::assets::IconName::Plus)
            .accessibility_label("Add encoding or separator")
            .tooltip("Add a text encoding or a separator")
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, window, _cx| {
                let separator = sheet.clone();
                let mut menu =
                    menu.item(PopupMenuItem::new("Separator").on_click(move |_, _, cx| {
                        separator
                            .update(cx, |sheet, cx| {
                                sheet.add(None);
                                sheet.refresh(cx);
                            })
                            .ok();
                    }));
                menu = menu.separator();
                menu = menu.item(PopupMenuItem::label("Text Encoding"));
                for encoding in addable.iter().copied() {
                    let sheet = sheet.clone();
                    menu =
                        menu.item(PopupMenuItem::new(encoding.name).on_click(move |_, _, cx| {
                            sheet
                                .update(cx, |sheet, cx| {
                                    sheet.add(Some(encoding));
                                    sheet.refresh(cx);
                                })
                                .ok();
                        }));
                }
                // `max_h` takes pixels, so the design scale is applied by hand
                // as it is everywhere else a `rems` value meets a `Pixels` one.
                menu.max_h(window.rem_size() * LIST_HEIGHT).scrollable(true)
            })
            .into_any_element()
    }

    /// The Delete button, greyed with its reason whenever the selection cannot
    /// go.
    fn render_remove(&self, cx: &Context<Self>) -> AnyElement {
        let (allowed, reason) = match self.removal() {
            Removal::Allowed => (true, String::new()),
            Removal::Nothing => (false, String::new()),
            Removal::Refused(reason) => (false, reason),
        };
        Button::new("encoding-remove")
            .small()
            .icon(gpui_kit::assets::IconName::Minus)
            .accessibility_label("Delete selected entries")
            .disabled(!allowed)
            .tooltip(reason)
            .on_click(cx.listener(|this, _, _, cx| {
                this.remove_selected();
                this.refresh(cx);
            }))
            .into_any_element()
    }
}

/// What a row carries while it is being dragged.
///
/// The identity is what the drop needs; the label is what the floating preview
/// draws, because that preview is an entity of its own and cannot read the
/// sheet the drag came from.
#[derive(Clone)]
struct Dragging {
    id: u64,
    label: SharedString,
}

/// The row under the pointer while it is in flight.
///
/// GPUI asks the drag's origin for one of these and draws it above the window
/// for as long as the gesture lasts, which is what tells the user they are
/// carrying a row rather than dragging a selection.
struct DragPreview {
    label: SharedString,
}

impl Render for DragPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_md()
            .text_sm()
            .child(self.label.clone())
    }
}

/// Where a run taken out of `from` lands when it is put back before `to`.
///
/// Foundation's `move(fromOffsets:toOffset:)` counts `to` in the array as it was
/// before anything came out, so the landing place is `to` less the moved rows
/// that sat above it. Clamped: `to` can name one place past the end (an append),
/// and a run that is the whole list has nowhere to go.
fn landing_index(from: &[usize], to: usize, len: usize) -> usize {
    let removed_above = from.iter().filter(|index| **index < to).count();
    to.saturating_sub(removed_above).min(len - from.len())
}

impl Render for EncodingListSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_default = self.list.is_default();

        v_flex()
            .w_full()
            .gap_3()
            .text_sm()
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("Drag encodings to change the order:"),
            )
            .child(self.render_list(cx))
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(self.render_add(cx))
                    .child(self.render_remove(cx))
                    .child(div().flex_1())
                    .child(
                        Button::new("encoding-restore")
                            .small()
                            .label("Restore Defaults")
                            .disabled(is_default)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.restore_defaults();
                                this.refresh(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "This order is used for the Encoding menu and for encoding \
                         detection. Only the encodings listed here are considered during \
                         detection, with higher items taking priority.",
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The editing rules are what these tests are about, and none of them
    /// reaches across to the workspace: only `submit` does, and it is the one
    /// method that needs a live entity to write to.
    fn sheet(names: &[Option<&str>]) -> EncodingListSheet {
        let list = EncodingList::from_identifiers(
            &names
                .iter()
                .map(|name| name.map(str::to_owned))
                .collect::<Vec<_>>(),
        );
        let mut sheet = EncodingListSheet {
            workspace: WeakEntity::new_invalid(),
            rows: Vec::new(),
            list,
            next_id: 0,
            selection: Vec::new(),
            anchor: None,
            default_encoding: TextEncoding::UTF8,
            addable: Vec::new(),
            // No window, so no focus: the tests are about the model.
            focus: None,
        };
        sheet.rebuild_rows();
        sheet.addable = sheet.list.addable();
        sheet
    }

    /// The entries the list holds, as identifiers — `None` for a separator.
    fn entries(sheet: &EncodingListSheet) -> Vec<Option<&'static str>> {
        sheet.list.identifiers()
    }

    /// The same, read off the rows rather than the list, so that a test can
    /// tell the two apart when they have come unstuck.
    fn row_entries(sheet: &EncodingListSheet) -> Vec<Option<&'static str>> {
        sheet
            .rows
            .iter()
            .map(|row| row.item.map(|e| e.identifier()))
            .collect()
    }

    fn assert_in_step(sheet: &EncodingListSheet) {
        assert_eq!(
            entries(sheet),
            row_entries(sheet),
            "the rows and the list came apart"
        );
        assert_eq!(sheet.rows.len(), sheet.list.len());
    }

    fn ids(sheet: &EncodingListSheet) -> Vec<u64> {
        sheet.rows.iter().map(|row| row.id).collect()
    }

    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Default::default()
        }
    }

    fn command() -> Modifiers {
        Modifiers {
            platform: true,
            ..Default::default()
        }
    }

    fn both() -> Modifiers {
        Modifiers {
            shift: true,
            platform: true,
            ..Default::default()
        }
    }

    #[test]
    fn the_rows_carry_the_list_and_their_own_identity() {
        let sheet = sheet(&[Some("utf-8"), None, Some("shift_jis")]);
        assert_in_step(&sheet);
        assert_eq!(sheet.rows.len(), 3);
        assert_eq!(
            ids(&sheet),
            vec![0, 1, 2],
            "identities are handed out in order"
        );
        assert_eq!(
            sheet.next_id, 3,
            "the counter is past the identities it gave out"
        );
    }

    /// A plain click replaces the selection; ⌘-click toggles; ⇧-click takes the
    /// range back to the anchor.
    #[test]
    fn clicking_a_row_selects_it_the_way_a_list_does() {
        let mut sheet = sheet(&[Some("utf-8"), Some("shift_jis"), Some("windows-1252")]);
        let rows = ids(&sheet);

        sheet.click(rows[0], Modifiers::default());
        assert_eq!(sheet.selection, vec![rows[0]]);

        sheet.click(rows[2], shift());
        assert_eq!(sheet.selection, vec![rows[0], rows[1], rows[2]]);

        // The anchor is still the first row, so a ⇧-click back up narrows it.
        sheet.click(rows[1], shift());
        assert_eq!(sheet.selection, vec![rows[0], rows[1]]);

        sheet.click(rows[2], command());
        assert_eq!(sheet.selection, vec![rows[0], rows[1], rows[2]]);
        sheet.click(rows[2], command());
        assert_eq!(sheet.selection, vec![rows[0], rows[1]]);

        // ⌘-click moves the anchor, so the next ⇧-click extends from there.
        sheet.click(rows[2], Modifiers::default());
        sheet.click(rows[0], shift());
        assert_eq!(sheet.selection, vec![rows[0], rows[1], rows[2]]);
    }

    /// ⇧⌘-click adds the range to what is already selected rather than
    /// replacing it.
    #[test]
    fn a_shift_command_click_adds_the_range() {
        let mut sheet = sheet(&[Some("utf-8"), Some("shift_jis"), Some("windows-1252")]);
        let rows = ids(&sheet);

        sheet.click(rows[2], Modifiers::default());
        sheet.click(rows[0], both());
        assert_eq!(sheet.selection, vec![rows[2], rows[0], rows[1]]);

        // Adding a range already held adds nothing twice.
        sheet.click(rows[0], both());
        assert_eq!(sheet.selection.len(), 3);
    }

    /// A ⇧-click with nothing to extend from is just a click.
    #[test]
    fn a_range_with_no_anchor_is_the_row_itself() {
        let mut sheet = sheet(&[Some("utf-8"), Some("shift_jis")]);
        let rows = ids(&sheet);

        sheet.click(rows[1], shift());
        assert_eq!(sheet.selection, vec![rows[1]]);
    }

    /// Dragging a row onto one below it puts it after that row.
    ///
    /// "Take the target's place" is the rule: the row that was dragged lands
    /// where the target was, and the target steps up. It is the only rule under
    /// which dropping on the last row can reach the end of the list.
    #[test]
    fn dragging_down_lands_after_the_target() {
        let mut sheet = sheet(&[Some("utf-8"), Some("shift_jis"), Some("windows-1252")]);
        let rows = ids(&sheet);

        sheet.move_dragged(rows[0], rows[2]);
        assert_in_step(&sheet);
        assert_eq!(
            entries(&sheet),
            vec![Some("shift_jis"), Some("windows-1252"), Some("utf-8")]
        );
        assert_eq!(
            sheet.rows[2].id, rows[0],
            "the identity travelled with the row"
        );
    }

    /// Dragging a row onto one above it puts it before that row.
    #[test]
    fn dragging_up_lands_before_the_target() {
        let mut sheet = sheet(&[Some("utf-8"), Some("shift_jis"), Some("windows-1252")]);
        let rows = ids(&sheet);

        sheet.move_dragged(rows[2], rows[0]);
        assert_in_step(&sheet);
        assert_eq!(
            entries(&sheet),
            vec![Some("windows-1252"), Some("utf-8"), Some("shift_jis")]
        );
    }

    /// A selected row drags the whole selection, and the run keeps its order.
    #[test]
    fn dragging_a_selected_row_takes_the_whole_run() {
        let mut sheet = sheet(&[
            Some("utf-8"),
            Some("shift_jis"),
            Some("windows-1252"),
            Some("euc-jp"),
            Some("koi8-r"),
        ]);
        let rows = ids(&sheet);

        sheet.selection = rows[1..=3].to_vec();
        sheet.move_dragged(rows[2], rows[4]);
        assert_in_step(&sheet);
        assert_eq!(
            entries(&sheet),
            vec![
                Some("utf-8"),
                Some("koi8-r"),
                Some("shift_jis"),
                Some("windows-1252"),
                Some("euc-jp")
            ],
            "the run is moved as a block, not reversed"
        );
        assert_eq!(sheet.selection, rows[1..=3].to_vec());
    }

    /// Dragging a row that is not selected moves just that row, even when
    /// something else is selected.
    #[test]
    fn dragging_an_unselected_row_leaves_the_selection_alone() {
        let mut sheet = sheet(&[Some("utf-8"), Some("shift_jis"), Some("windows-1252")]);
        let rows = ids(&sheet);

        sheet.selection = vec![rows[0]];
        sheet.move_dragged(rows[2], rows[0]);
        assert_in_step(&sheet);
        assert_eq!(
            entries(&sheet),
            vec![Some("windows-1252"), Some("utf-8"), Some("shift_jis")]
        );
        assert_eq!(
            sheet.selection,
            vec![rows[0]],
            "the selection did not travel with a row it was not on"
        );
    }

    /// A row dropped on itself, or on a row of its own run, changes nothing.
    #[test]
    fn dropping_a_run_on_itself_changes_nothing() {
        let mut sheet = sheet(&[Some("utf-8"), Some("shift_jis"), Some("windows-1252")]);
        let rows = ids(&sheet);
        let before = entries(&sheet);

        sheet.move_dragged(rows[1], rows[1]);
        assert_eq!(entries(&sheet), before);

        sheet.selection = vec![rows[0], rows[1]];
        sheet.move_dragged(rows[0], rows[1]);
        assert_eq!(entries(&sheet), before, "a drop inside the run is a no-op");
    }

    /// A separator drags like any other row — it is a row, not a gap.
    #[test]
    fn a_separator_drags_like_any_other_row() {
        let mut sheet = sheet(&[Some("utf-8"), None, Some("shift_jis")]);
        let rows = ids(&sheet);

        sheet.move_dragged(rows[1], rows[0]);
        assert_in_step(&sheet);
        assert_eq!(
            entries(&sheet),
            vec![None, Some("utf-8"), Some("shift_jis")]
        );
    }

    /// Foundation's `move(fromOffsets:toOffset:)`, which is what SwiftUI's
    /// `.onMove` calls, on the cases a drag produces.
    #[test]
    fn the_landing_place_is_foundations() {
        // One row down over the row below it: [a, b, c] -> [b, a, c].
        assert_eq!(landing_index(&[0], 2, 3), 1);
        // One row up over the row above it: [a, b, c] -> [c, a, b].
        assert_eq!(landing_index(&[2], 0, 3), 0);
        // A run of two down past one row: [a, b, c, d] -> [c, a, b, d].
        assert_eq!(landing_index(&[0, 1], 3, 4), 1);
        // A run that is the whole list has nowhere to go.
        assert_eq!(landing_index(&[0, 1, 2], 0, 3), 0);
        // An append: `to` one past the end.
        assert_eq!(landing_index(&[0], 3, 3), 2);
    }

    /// UTF-8 and the default encoding refuse, and the reason names the entry.
    #[test]
    fn the_delete_button_is_refused_for_utf8_and_the_default() {
        let mut sheet = sheet(&[Some("utf-8"), Some("shift_jis"), Some("windows-1252")]);
        let rows = ids(&sheet);

        sheet.selection = vec![rows[0]];
        match sheet.removal() {
            Removal::Refused(reason) => assert_eq!(reason, "Unicode (UTF-8) can’t be removed."),
            _ => panic!("UTF-8 was not refused"),
        }
        sheet.remove_selected();
        assert_eq!(
            entries(&sheet).len(),
            3,
            "a refused removal changed the list"
        );

        // Shift JIS is the default here, so it refuses too — with the message
        // that does not name the encoding, because the user cannot tell which
        // one it is from the list alone.
        sheet.default_encoding = TextEncoding::from_identifier("shift_jis").expect("registered");
        sheet.selection = vec![rows[1]];
        match sheet.removal() {
            Removal::Refused(reason) => {
                assert_eq!(reason, "The encoding set as default can’t be removed.")
            }
            _ => panic!("the default encoding was not refused"),
        }

        sheet.selection = vec![rows[2]];
        assert!(matches!(sheet.removal(), Removal::Allowed));
        sheet.remove_selected();
        assert_in_step(&sheet);
        assert_eq!(entries(&sheet), vec![Some("utf-8"), Some("shift_jis")]);
    }

    /// When the selection holds both refusals, the message is the one that
    /// belongs to the entry nearest the top of the list — which is what
    /// upstream's `canRemove(ids:)` reports, because it filters `items` and
    /// takes the first, rather than following the selection.
    #[test]
    fn the_first_refusal_in_list_order_is_the_one_reported() {
        let shift_jis = TextEncoding::from_identifier("shift_jis").expect("registered");

        let mut above = sheet(&[Some("windows-1252"), Some("utf-8"), Some("shift_jis")]);
        above.default_encoding = shift_jis;
        above.selection = ids(&above);
        match above.removal() {
            Removal::Refused(reason) => assert_eq!(reason, "Unicode (UTF-8) can’t be removed."),
            _ => panic!("a refusal was not reported"),
        }

        let mut below = sheet(&[Some("shift_jis"), Some("utf-8"), Some("windows-1252")]);
        below.default_encoding = shift_jis;
        below.selection = ids(&below);
        match below.removal() {
            Removal::Refused(reason) => {
                assert_eq!(reason, "The encoding set as default can’t be removed.")
            }
            _ => panic!("a refusal was not reported"),
        }
    }

    /// The selection is cleared after a removal, which is what upstream's
    /// `remove(ids:)` leaves behind.
    #[test]
    fn a_removal_clears_the_selection() {
        let mut sheet = sheet(&[Some("utf-8"), Some("shift_jis"), Some("windows-1252")]);
        let rows = ids(&sheet);

        sheet.selection = vec![rows[1]];
        sheet.remove_selected();
        assert!(sheet.selection.is_empty());
        assert_eq!(sheet.anchor, None);
    }

    /// A new entry goes below the last selected row, and is selected itself so
    /// that a run of additions reads in the order it was made.
    #[test]
    fn an_entry_is_added_below_the_selection_and_selected() {
        let mut sheet = sheet(&[Some("utf-8"), Some("shift_jis")]);
        let rows = ids(&sheet);
        let koi8 = TextEncoding::from_identifier("koi8-r").expect("registered");

        sheet.selection = vec![rows[0]];
        sheet.add(Some(koi8));
        assert_in_step(&sheet);
        assert_eq!(
            entries(&sheet),
            vec![Some("utf-8"), Some("koi8-r"), Some("shift_jis")]
        );
        assert_eq!(
            sheet.selection,
            vec![sheet.rows[1].id],
            "the added row is selected"
        );

        // With nothing selected the entry goes to the top, as upstream's
        // `?? 0` does.
        sheet.selection.clear();
        sheet.add(None);
        assert_in_step(&sheet);
        assert_eq!(entries(&sheet)[0], None);
    }

    /// Restore Defaults puts the shipped list back and drops the selection,
    /// because every identity in it is new.
    #[test]
    fn restoring_the_defaults_rebuilds_the_rows() {
        let mut sheet = sheet(&[Some("windows-1252"), None, Some("koi8-r")]);
        let rows = ids(&sheet);
        sheet.selection = vec![rows[0]];
        assert!(!sheet.list.is_default());

        sheet.restore_defaults();
        assert_in_step(&sheet);
        assert!(sheet.list.is_default());
        assert!(sheet.selection.is_empty());
        assert_eq!(sheet.anchor, None);
        assert_eq!(
            sheet.rows.len(),
            EncodingList::defaults().len(),
            "every entry of the shipped list is a row"
        );
    }

    /// The Add menu is the registry less what the list holds, and it shrinks as
    /// entries are added.
    #[test]
    fn the_add_menu_follows_the_list() {
        let mut sheet = sheet(&[Some("utf-8")]);
        let before = sheet.addable.len();

        sheet.add(Some(
            TextEncoding::from_identifier("koi8-r").expect("registered"),
        ));
        sheet.addable = sheet.list.addable();
        assert_eq!(sheet.addable.len(), before - 1);
        assert!(
            !sheet.addable.iter().any(|e| e.identifier() == "koi8-r"),
            "an added encoding is still on offer"
        );
    }
}
