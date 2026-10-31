//! The document window's toolbar, and the palette that customizes it.
//!
//! CotEditor's toolbar is an `NSToolbar` driven by
//! `DocumentWindowController`'s `NSToolbarDelegate`. Its default set for a text
//! document is `syntax`, a flexible space and `toggleInspector`; the delegate
//! also offers a fixed list of allowed items, and *View ▸ Customize Toolbar…*
//! opens AppKit's palette, where the user drags items in and out and back into
//! order. The choice is autosaved per toolbar, so it survives the next launch.
//!
//! SubText has no `NSToolbar`, so this module holds the same three things as
//! data: the allowed identifiers in upstream's order, the default set, and the
//! labels / symbols each item shows. [`ToolbarSheet`] is the palette, and
//! [`ToolbarItemId`] is what the settings file stores — the identifiers are
//! upstream's own strings (`syntaxStyle`, `toggleInspector`, …), so a saved
//! toolbar can be compared with CotEditor's item by item.
//!
//! # What the palette does differently, and why
//!
//! AppKit's palette is a strip of every allowed item that the user drags in
//! and out of the toolbar. GPUI's drag-and-drop can move a row inside one list
//! but has no cross-list drop target that survives a dialog rebuild, so the
//! palette is two lists: the current items can be dragged into a new order,
//! and an item is added with a click on the list of the ones not in use. The
//! capability is the same in both directions — any allowed item can be added,
//! any movable one removed, and the order is the user's — but the gesture is
//! not, which is recorded in `docs/PLAN.md`.
//!
//! # What is shown but not honoured
//!
//! Three groups in upstream's allowed list control features SubText does not
//! have: writing direction, text orientation, and the macOS system panels
//! (spelling, emoji & symbols, print, share). They stay in the list because the
//! menu bar keeps their commands for the same reason — an item that disappears
//! is harder to account for than one that says what it cannot do when pressed.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    list::ListItem,
    v_flex,
};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    WeakEntity, Window, div, px, rems,
};
use serde::{Deserialize, Serialize};

use crate::dialog_escape::DismissOnEscape as _;
use crate::workspace::Workspace;

/// How tall each of the palette's two lists is, in rem.
///
/// The window may be as short as 480 points (the main window's minimum), and the
/// dialog has a hint and a button row around these lists, so this is the height
/// that still leaves the buttons on screen. Both lists scroll.
const LIST_HEIGHT: f32 = 9.;
/// How wide each of the palette's two lists is, in rem.
const COLUMN_WIDTH: f32 = 18.;

/// One identifier of the document toolbar.
///
/// The names in `serde` are upstream's `NSToolbarItem.Identifier` raw values.
/// `Unknown` exists so a settings file written by a newer build — or edited by
/// hand — does not make the whole `Settings` fail to deserialize and be
/// replaced with defaults; [`sanitize`](Self::sanitize) drops it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolbarItemId {
    #[serde(rename = "syntaxStyle")]
    Syntax,
    #[serde(rename = "toggleInspector")]
    ToggleInspector,
    #[serde(rename = "toggleSidebar")]
    ToggleSidebar,
    #[serde(rename = "textSize")]
    TextSize,
    #[serde(rename = "writingDirection")]
    WritingDirection,
    #[serde(rename = "textOrientation")]
    TextOrientation,
    #[serde(rename = "indent")]
    Indent,
    #[serde(rename = "comment")]
    Comment,
    #[serde(rename = "tabStyle")]
    TabStyle,
    #[serde(rename = "wrapLines")]
    WrapLines,
    #[serde(rename = "invisibles")]
    Invisibles,
    #[serde(rename = "indentGuides")]
    IndentGuides,
    #[serde(rename = "keepOnTop")]
    KeepOnTop,
    #[serde(rename = "editable")]
    Editable,
    #[serde(rename = "spellCheck")]
    SpellCheck,
    #[serde(rename = "emojiAndSymbols")]
    EmojiAndSymbols,
    #[serde(rename = "fonts")]
    Fonts,
    #[serde(rename = "find")]
    Find,
    #[serde(rename = "print")]
    Print,
    #[serde(rename = "share")]
    Share,
    #[serde(rename = "space")]
    Space,
    #[serde(rename = "flexibleSpace")]
    FlexibleSpace,
    #[serde(other)]
    Unknown,
}

impl ToolbarItemId {
    /// Everything the palette offers, in the order upstream's
    /// `toolbarAllowedItemIdentifiers(_:)` lists it.
    ///
    /// `ToggleSidebar` is the one addition. Upstream offers it only for
    /// directory documents, because a text window's sidebar toggle is part of
    /// the split view's own chrome; SubText's sidebar belongs to every
    /// document, so the item does too.
    pub const ALLOWED: [ToolbarItemId; 22] = [
        Self::Syntax,
        Self::ToggleInspector,
        Self::ToggleSidebar,
        Self::TextSize,
        Self::WritingDirection,
        Self::TextOrientation,
        Self::Indent,
        Self::Comment,
        Self::TabStyle,
        Self::WrapLines,
        Self::Invisibles,
        Self::IndentGuides,
        Self::KeepOnTop,
        Self::Editable,
        Self::SpellCheck,
        Self::EmojiAndSymbols,
        Self::Fonts,
        Self::Find,
        Self::Print,
        Self::Share,
        Self::Space,
        Self::FlexibleSpace,
    ];

    /// What a fresh window shows: upstream's `toolbarDefaultItemIdentifiers`
    /// for a text document.
    pub const DEFAULT: [ToolbarItemId; 3] =
        [Self::Syntax, Self::FlexibleSpace, Self::ToggleInspector];

    /// The items AppKit calls immovable: they cannot be taken out.
    pub const IMMOVABLE: [ToolbarItemId; 2] = [Self::FlexibleSpace, Self::ToggleInspector];

    /// The item's label, as the palette and the toolbar's accessible name show
    /// it. These are `Toolbar.*.label` from upstream's `Document` table.
    pub fn label(self) -> &'static str {
        match self {
            Self::Syntax => "Syntax",
            Self::ToggleInspector => "Inspector",
            Self::ToggleSidebar => "Sidebar",
            Self::TextSize => "Text Size",
            Self::WritingDirection => "Writing Direction",
            Self::TextOrientation => "Text Orientation",
            Self::Indent => "Indent",
            Self::Comment => "Comment",
            Self::TabStyle => "Tab Style",
            Self::WrapLines => "Line Wrapping",
            Self::Invisibles => "Invisibles",
            Self::IndentGuides => "Indent Guides",
            Self::KeepOnTop => "Keep on Top",
            Self::Editable => "Edit",
            Self::SpellCheck => "Spell Check",
            Self::EmojiAndSymbols => "Emoji & Symbols",
            Self::Fonts => "Fonts",
            Self::Find => "Find",
            Self::Print => "Print",
            Self::Share => "Share",
            Self::Space => "Space",
            Self::FlexibleSpace => "Flexible Space",
            Self::Unknown => "Unknown",
        }
    }

    /// A stable slug for element ids, so a row keeps its identity across a
    /// reorder. The settings name is what the row stands for.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Syntax => "syntaxStyle",
            Self::ToggleInspector => "toggleInspector",
            Self::ToggleSidebar => "toggleSidebar",
            Self::TextSize => "textSize",
            Self::WritingDirection => "writingDirection",
            Self::TextOrientation => "textOrientation",
            Self::Indent => "indent",
            Self::Comment => "comment",
            Self::TabStyle => "tabStyle",
            Self::WrapLines => "wrapLines",
            Self::Invisibles => "invisibles",
            Self::IndentGuides => "indentGuides",
            Self::KeepOnTop => "keepOnTop",
            Self::Editable => "editable",
            Self::SpellCheck => "spellCheck",
            Self::EmojiAndSymbols => "emojiAndSymbols",
            Self::Fonts => "fonts",
            Self::Find => "find",
            Self::Print => "print",
            Self::Share => "share",
            Self::Space => "space",
            Self::FlexibleSpace => "flexibleSpace",
            Self::Unknown => "unknown",
        }
    }

    /// The closest symbol in the Lucide set for upstream's SF Symbol.
    pub fn icon(self) -> IconName {
        match self {
            Self::Syntax => IconName::Languages,
            Self::ToggleInspector => IconName::PanelRight,
            Self::ToggleSidebar => IconName::PanelLeft,
            Self::TextSize => IconName::Type,
            Self::WritingDirection => IconName::TextAlignStart,
            Self::TextOrientation => IconName::RectangleHorizontal,
            Self::Indent => IconName::ListIndentIncrease,
            Self::Comment => IconName::MessageSquare,
            Self::TabStyle => IconName::Braces,
            Self::WrapLines => IconName::TextWrap,
            Self::Invisibles => IconName::Pilcrow,
            Self::IndentGuides => IconName::ListTree,
            Self::KeepOnTop => IconName::Pin,
            Self::Editable => IconName::Pencil,
            Self::SpellCheck => IconName::SpellCheck,
            Self::EmojiAndSymbols => IconName::FaceSlightlySmiling,
            Self::Fonts => IconName::Type,
            Self::Find => IconName::Search,
            Self::Print => IconName::Printer,
            Self::Share => IconName::Share2,
            Self::Space | Self::FlexibleSpace => IconName::Ellipsis,
            Self::Unknown => IconName::Ellipsis,
        }
    }

    /// Whether the item can be removed from the toolbar.
    pub fn is_immovable(self) -> bool {
        Self::IMMOVABLE.contains(&self)
    }

    /// A settings-file list made safe to render: no unknown identifiers, no
    /// duplicates, and both of AppKit's immovable items present.
    ///
    /// The immovable ones are restored rather than rejected, because a toolbar
    /// with no flexible space and no inspector toggle is not a toolbar CotEditor
    /// would ever show — and a hand-edited settings file is the only way to make
    /// one. A missing flexible space goes immediately before the inspector
    /// toggle, which is where the default set puts it.
    pub fn sanitize(items: &[ToolbarItemId]) -> Vec<ToolbarItemId> {
        let mut sanitized: Vec<ToolbarItemId> = Vec::with_capacity(items.len() + 2);
        for &item in items {
            if item != Self::Unknown && !sanitized.contains(&item) {
                sanitized.push(item);
            }
        }
        for required in Self::IMMOVABLE {
            if sanitized.contains(&required) {
                continue;
            }
            let at = if required == Self::FlexibleSpace {
                sanitized
                    .iter()
                    .position(|item| *item == Self::ToggleInspector)
                    .unwrap_or(sanitized.len())
            } else {
                sanitized.len()
            };
            sanitized.insert(at, required);
        }
        sanitized
    }

    /// Move the item at `from` to `to`, where `to` is the index it should end
    /// up at after the move.
    ///
    /// The same "drop on a row means take that row's place" rule the encoding
    /// list uses. A drag whose indices do not fit the list is a no-op rather
    /// than a panic: the list can change under a drag that was started a frame
    /// earlier.
    pub fn move_item(items: &mut Vec<ToolbarItemId>, from: usize, to: usize) {
        if from >= items.len() || to >= items.len() || from == to {
            return;
        }
        let item = items.remove(from);
        items.insert(to, item);
    }
}

/// The toolbar customization palette.
pub struct ToolbarSheet {
    workspace: WeakEntity<Workspace>,
    /// The items in the toolbar, in order. The sheet owns a copy so the rows
    /// can be moved and the lists re-render without asking the workspace; a
    /// successful edit is pushed back through
    /// [`Workspace::set_toolbar_items`], which sanitizes and saves it.
    items: Vec<ToolbarItemId>,
}

impl ToolbarSheet {
    pub fn new(workspace: WeakEntity<Workspace>, items: Vec<ToolbarItemId>) -> Self {
        Self {
            workspace,
            items: ToolbarItemId::sanitize(&items),
        }
    }

    /// The items currently in the toolbar, for tests and for the workspace.
    pub fn items(&self) -> &[ToolbarItemId] {
        &self.items
    }

    /// Add an item, if it is not already in the toolbar.
    ///
    /// It goes immediately left of the inspector toggle, which is the one item
    /// that is always at the trailing edge — that way a new item is visible
    /// rather than appended past the inspector. It can then be dragged anywhere
    /// else.
    pub fn add(&mut self, item: ToolbarItemId) {
        if item == ToolbarItemId::Unknown || self.items.contains(&item) {
            return;
        }
        let at = self
            .items
            .iter()
            .position(|existing| *existing == ToolbarItemId::ToggleInspector)
            .unwrap_or(self.items.len());
        self.items.insert(at, item);
    }

    /// Take an item out, unless it is immovable.
    pub fn remove(&mut self, item: ToolbarItemId) {
        if item.is_immovable() {
            return;
        }
        self.items.retain(|existing| *existing != item);
    }

    /// Put the shipped set back.
    pub fn reset(&mut self) {
        self.items = ToolbarItemId::DEFAULT.to_vec();
    }

    /// Move a row, as a drop reports it.
    pub fn move_row(&mut self, from: usize, to: usize) {
        ToolbarItemId::move_item(&mut self.items, from, to);
    }

    /// Push the current list to the workspace, which sanitizes and saves it.
    ///
    /// A sheet built without a workspace (the UI tests do this, so the rules can
    /// be driven without a document) still re-renders; it just has nowhere to
    /// save to.
    fn write(&mut self, cx: &mut Context<Self>) {
        self.items = ToolbarItemId::sanitize(&self.items);
        let items = self.items.clone();
        self.workspace
            .update(cx, move |workspace, cx| {
                workspace.set_toolbar_items(items, cx)
            })
            .ok();
        cx.notify();
    }

    /// The rows of the left-hand list.
    fn render_current(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = v_flex().gap_0p5().p_1();
        for (index, item) in self.items.iter().copied().enumerate() {
            rows = rows.child(self.render_current_row(index, item, cx));
        }

        div()
            .id("toolbar-current")
            .w(rems(COLUMN_WIDTH))
            .h(rems(LIST_HEIGHT))
            .overflow_y_scroll()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(rows)
            .into_any_element()
    }

    fn render_current_row(
        &self,
        index: usize,
        item: ToolbarItemId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dragged = Dragging {
            from: index,
            label: item.label().into(),
        };
        let dropped = {
            let sheet = cx.entity().downgrade();
            move |dragged: &Dragging, _window: &mut Window, cx: &mut App| {
                sheet
                    .update(cx, |sheet, cx| {
                        sheet.move_row(dragged.from, index);
                        sheet.write(cx);
                    })
                    .ok();
            }
        };
        let target = cx.theme().accent.opacity(0.15);
        let movable = !item.is_immovable();

        ListItem::new(("toolbar-row", item as usize))
            .w_full()
            .px_2()
            .py_1()
            .on_drag(dragged, |drag: &Dragging, _, _, cx| {
                let label = drag.label.clone();
                cx.new(|_| DragPreview { label })
            })
            .on_drop::<Dragging>(dropped)
            .drag_over::<Dragging>(move |style, _, _, _| style.bg(target))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(item.icon())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .child(item.label()),
                    )
                    .child(
                        Button::new(("toolbar-remove", item as usize))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Minus)
                            // A disabled icon still paints at full strength in
                            // the component library; fading it is what says the
                            // item is not removable.
                            .opacity(if movable { 1.0 } else { 0.35 })
                            .disabled(!movable)
                            .accessibility_label(format!("Remove {}", item.label()))
                            .tooltip(if movable {
                                "Remove from the toolbar"
                            } else {
                                "This item cannot be removed"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove(item);
                                this.write(cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// The rows of the right-hand list: the allowed items not in use.
    fn render_available(&self, cx: &mut Context<Self>) -> AnyElement {
        let available: Vec<ToolbarItemId> = ToolbarItemId::ALLOWED
            .into_iter()
            .filter(|item| !self.items.contains(item))
            .collect();

        let mut rows = v_flex().gap_0p5().p_1();
        if available.is_empty() {
            rows = rows.child(
                div()
                    .px_2()
                    .py_1()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Every item is already in the toolbar."),
            );
        }
        for item in available {
            rows = rows.child(
                ListItem::new(("toolbar-available", item as usize))
                    .w_full()
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .accessibility_label(format!("Add {}", item.label()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.add(item);
                        this.write(cx);
                    }))
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(item.icon())
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .truncate()
                                    .child(item.label()),
                            )
                            .child(IconName::Plus),
                    ),
            );
        }

        div()
            .id("toolbar-available")
            .w(rems(COLUMN_WIDTH))
            .h(rems(LIST_HEIGHT))
            .overflow_y_scroll()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(rows)
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .child(
                Button::new("toolbar-default-set")
                    .small()
                    .label("Default Set")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.reset();
                        this.write(cx);
                    })),
            )
            .child(
                Button::new("toolbar-done")
                    .small()
                    .primary()
                    .label("Done")
                    .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
            )
            .into_any_element()
    }
}

impl Render for ToolbarSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .dismiss_on_escape()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Drag a row to change its order. Click an item on the right to add it, \
                         or the minus button to take one out.",
                    ),
            )
            .child(
                h_flex()
                    .items_start()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(div().text_sm().child("Toolbar:"))
                            .child(self.render_current(cx)),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(div().text_sm().child("Available:"))
                            .child(self.render_available(cx)),
                    ),
            )
            .child(self.render_footer(cx))
    }
}

/// What a row carries while it is being dragged.
#[derive(Clone)]
struct Dragging {
    from: usize,
    label: SharedString,
}

/// The row under the pointer while it is in flight.
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

/// A fixed-width spacer, upstream's `.space` item.
pub fn fixed_space() -> AnyElement {
    div().w(px(16.)).flex_none().into_any_element()
}

/// The space that pushes everything after it to the trailing edge, upstream's
/// `.flexibleSpace`.
pub fn flexible_space() -> AnyElement {
    div().flex_1().min_w_0().into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_set_is_coteditors_text_document_toolbar() {
        // Upstream's `toolbarDefaultItemIdentifiers` for a document that is not
        // a directory: syntax, a flexible space, the inspector toggle.
        assert_eq!(
            ToolbarItemId::DEFAULT,
            [
                ToolbarItemId::Syntax,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::ToggleInspector
            ]
        );
        assert_eq!(
            ToolbarItemId::sanitize(&ToolbarItemId::DEFAULT),
            ToolbarItemId::DEFAULT
        );
    }

    #[test]
    fn the_immovable_items_are_the_ones_appkit_will_not_take_out() {
        assert_eq!(
            ToolbarItemId::IMMOVABLE,
            [ToolbarItemId::FlexibleSpace, ToolbarItemId::ToggleInspector]
        );
        assert!(ToolbarItemId::FlexibleSpace.is_immovable());
        assert!(ToolbarItemId::ToggleInspector.is_immovable());
        assert!(!ToolbarItemId::Syntax.is_immovable());
    }

    #[test]
    fn every_allowed_item_is_listed_once_and_has_its_own_identity() {
        let mut slugs: Vec<&str> = ToolbarItemId::ALLOWED
            .iter()
            .map(|item| item.slug())
            .collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), ToolbarItemId::ALLOWED.len());
        assert!(!slugs.contains(&"unknown"));
    }

    #[test]
    fn sanitize_drops_unknown_identifiers_and_duplicates() {
        let items = [
            ToolbarItemId::Syntax,
            ToolbarItemId::Unknown,
            ToolbarItemId::Comment,
            ToolbarItemId::Syntax,
            ToolbarItemId::FlexibleSpace,
            ToolbarItemId::ToggleInspector,
        ];
        assert_eq!(
            ToolbarItemId::sanitize(&items),
            [
                ToolbarItemId::Syntax,
                ToolbarItemId::Comment,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::ToggleInspector
            ]
        );
    }

    #[test]
    fn sanitize_puts_the_immovable_items_back() {
        // A hand-edited file that names only one item still gets the inspector
        // toggle, and a flexible space lands before it.
        assert_eq!(
            ToolbarItemId::sanitize(&[ToolbarItemId::Find]),
            [
                ToolbarItemId::Find,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::ToggleInspector
            ]
        );
        assert_eq!(
            ToolbarItemId::sanitize(&[ToolbarItemId::Find, ToolbarItemId::ToggleInspector]),
            [
                ToolbarItemId::Find,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::ToggleInspector
            ]
        );
        assert_eq!(ToolbarItemId::sanitize(&[]), ToolbarItemId::IMMOVABLE);
    }

    #[test]
    fn moving_an_item_puts_it_at_the_index_it_was_dropped_on() {
        let mut items = vec![
            ToolbarItemId::Syntax,
            ToolbarItemId::Comment,
            ToolbarItemId::Find,
            ToolbarItemId::FlexibleSpace,
        ];
        ToolbarItemId::move_item(&mut items, 0, 2);
        assert_eq!(
            items,
            [
                ToolbarItemId::Comment,
                ToolbarItemId::Find,
                ToolbarItemId::Syntax,
                ToolbarItemId::FlexibleSpace
            ]
        );
        ToolbarItemId::move_item(&mut items, 3, 0);
        assert_eq!(
            items,
            [
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::Comment,
                ToolbarItemId::Find,
                ToolbarItemId::Syntax
            ]
        );
    }

    #[test]
    fn a_move_that_does_not_fit_the_list_changes_nothing() {
        let mut items = vec![ToolbarItemId::Syntax, ToolbarItemId::FlexibleSpace];
        let before = items.clone();
        ToolbarItemId::move_item(&mut items, 5, 0);
        ToolbarItemId::move_item(&mut items, 0, 5);
        ToolbarItemId::move_item(&mut items, 1, 1);
        assert_eq!(items, before);
    }

    #[test]
    fn the_palette_adds_before_the_inspector_toggle_and_refuses_duplicates() {
        let mut sheet =
            ToolbarSheet::new(WeakEntity::new_invalid(), ToolbarItemId::DEFAULT.to_vec());
        sheet.add(ToolbarItemId::Find);
        sheet.add(ToolbarItemId::Find);
        assert_eq!(
            sheet.items(),
            [
                ToolbarItemId::Syntax,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::Find,
                ToolbarItemId::ToggleInspector
            ]
        );
    }

    #[test]
    fn the_palette_will_not_remove_an_immovable_item() {
        let mut sheet =
            ToolbarSheet::new(WeakEntity::new_invalid(), ToolbarItemId::DEFAULT.to_vec());
        sheet.remove(ToolbarItemId::FlexibleSpace);
        sheet.remove(ToolbarItemId::ToggleInspector);
        assert_eq!(sheet.items(), ToolbarItemId::DEFAULT);
        sheet.remove(ToolbarItemId::Syntax);
        assert_eq!(
            sheet.items(),
            [ToolbarItemId::FlexibleSpace, ToolbarItemId::ToggleInspector]
        );
    }

    #[test]
    fn default_set_restores_the_shipped_toolbar() {
        let mut sheet = ToolbarSheet::new(
            WeakEntity::new_invalid(),
            vec![ToolbarItemId::Find, ToolbarItemId::FlexibleSpace],
        );
        sheet.reset();
        assert_eq!(sheet.items(), ToolbarItemId::DEFAULT);
    }

    #[test]
    fn the_saved_names_are_upstream_identifiers() {
        let names = serde_json::to_string(&[
            ToolbarItemId::Syntax,
            ToolbarItemId::FlexibleSpace,
            ToolbarItemId::ToggleInspector,
            ToolbarItemId::EmojiAndSymbols,
        ])
        .expect("serialize");
        assert_eq!(
            names,
            r#"["syntaxStyle","flexibleSpace","toggleInspector","emojiAndSymbols"]"#
        );

        // An identifier a newer version might add does not poison the file: it
        // becomes `Unknown`, and `sanitize` drops it.
        let parsed: Vec<ToolbarItemId> =
            serde_json::from_str(r#"["syntaxStyle","somethingNew","flexibleSpace"]"#)
                .expect("unknown identifiers are tolerated");
        assert_eq!(
            ToolbarItemId::sanitize(&parsed),
            [
                ToolbarItemId::Syntax,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::ToggleInspector
            ]
        );
    }
}
