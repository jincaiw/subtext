//! The Snippets pane's two tables, and the rows they are built from.
//!
//! CotEditor's Snippets pane is two tabs over two nearly identical tables. The
//! *Command* tab lists the snippets behind *Text > Insert Snippet* — a syntax
//! scope, a name and a key equivalent — and the *File Drop* tab lists what a
//! dropped file expands to — a syntax scope, a list of file extensions and a
//! description. Below both sits the same editor for the *insertion format*: the
//! text the item stands for, with tokens in it.
//!
//! # Where the state lives
//!
//! Each row is its own entity and each row's fields *are* its `InputState`s.
//! That is the choice the multiple-replace sheet makes, for the same reason: a
//! row can repaint its own warning without the table around it moving, and the
//! pane never has to keep a second copy of what is on screen in step.
//!
//! The one thing a row cannot hold is the insertion format, because upstream
//! edits it in a single editor under the table rather than in the row. So the
//! format sits in the row as plain data and the pane's `Textarea` is its
//! editor: the pane loads the selected row's format when the selection moves
//! and writes back on every keystroke.
//!
//! # What a row tells the pane
//!
//! A row's event handlers run inside that row's own update, so the pane must
//! not read the row back while one runs. Each handler therefore hands the pane
//! the value it has just built, and the pane writes *that* into the settings.
//! Upstream's version walks its own array instead, which works there because
//! SwiftUI's bindings are not borrows.
//!
//! # The key field
//!
//! Upstream's `ShortcutField` is a text field that never sees typing: it
//! installs a key-down monitor and turns whatever chord arrives into a value.
//! [`SnippetRow`] does the same with a focusable `div` and `capture_key_down`,
//! which GPUI runs during the capture phase — before the key map, so a chord
//! that a menu command already owns is still recorded and can be reported as a
//! conflict rather than silently running that command.

use std::rc::Rc;

use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    tooltip::Tooltip,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, App, AppContext as _, Context, Div, ElementId, Entity, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, KeyDownEvent, Keystroke, MouseButton, ParentElement as _,
    Render, StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window, div,
    rems,
};

use subtext_core::filedrop::FileDropItem;
use subtext_core::snippets::{Shortcut, Snippet};

use crate::keymap;
use crate::settings_window::SettingsWindow;

/// Which of the pane's two tables a row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Command,
    FileDrop,
}

impl Tab {
    /// The tab's label, which is also its position in the tab bar.
    pub fn label(self) -> &'static str {
        match self {
            Tab::Command => "Command",
            Tab::FileDrop => "File Drop",
        }
    }

    /// What the table holds, in upstream's wording.
    pub fn intro(self) -> &'static str {
        match self {
            Tab::Command => "Text to be inserted by a command in the menu or by keyboard shortcut:",
            Tab::FileDrop => "Text to be inserted by dropping files to the editor:",
        }
    }
}

/// What the pane pushes into a row.
///
/// A row renders while the pane is borrowed, so it cannot read the pane back;
/// the two things it needs from there — the syntax list its picker offers and
/// whether it is the selected row — are handed to it instead. The list is
/// shared rather than copied so that adding a row does not clone it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowState {
    pub syntaxes: Rc<Vec<String>>,
    pub selected: bool,
}

impl RowState {
    fn new(syntaxes: Rc<Vec<String>>) -> Self {
        Self {
            syntaxes,
            selected: false,
        }
    }
}

// ---- columns -------------------------------------------------------------

/// The Command table's columns, in the order the header and every row lay them
/// out. Upstream sizes them 160 points, the space left over, and 80 points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandColumn {
    Syntax,
    Name,
    Key,
}

impl CommandColumn {
    pub const ALL: [CommandColumn; 3] = [
        CommandColumn::Syntax,
        CommandColumn::Name,
        CommandColumn::Key,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CommandColumn::Syntax => "Syntax",
            CommandColumn::Name => "Name",
            CommandColumn::Key => "Key",
        }
    }

    /// The width of a fixed column, in rems. Zero means the column shares
    /// whatever the fixed ones leave.
    pub fn width(self) -> f32 {
        match self {
            // 160 and 80 points at the 16-point root the design system uses.
            CommandColumn::Syntax => 10.,
            CommandColumn::Key => 5.,
            CommandColumn::Name => 0.,
        }
    }
}

/// The File Drop table's columns. Upstream gives the syntax column the same
/// 160 points and lets the other two share the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDropColumn {
    Syntax,
    Extensions,
    Description,
}

impl FileDropColumn {
    pub const ALL: [FileDropColumn; 3] = [
        FileDropColumn::Syntax,
        FileDropColumn::Extensions,
        FileDropColumn::Description,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FileDropColumn::Syntax => "Syntax",
            FileDropColumn::Extensions => "Extensions",
            FileDropColumn::Description => "Description",
        }
    }

    pub fn width(self) -> f32 {
        match self {
            FileDropColumn::Syntax => 10.,
            FileDropColumn::Extensions | FileDropColumn::Description => 0.,
        }
    }
}

/// One table cell, sized to its column.
fn cell(width: f32, content: impl IntoElement) -> Div {
    let base = div().flex_shrink_0();
    if width > 0. {
        base.w(rems(width))
    } else {
        // `min_w(0)` so a long name shrinks the field instead of pushing the
        // columns to its right off the edge.
        base.flex_1().min_w(rems(0.)).overflow_hidden()
    }
    .child(content)
}

/// The header row of a table.
pub fn header(labels: impl IntoIterator<Item = (&'static str, f32)>, cx: &App) -> impl IntoElement {
    let mut row = h_flex().w_full().gap_2().px_1().py_1().flex_shrink_0();
    for (label, width) in labels {
        row = row.child(cell(
            width,
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        ));
    }
    row.border_b_1().border_color(cx.theme().border)
}

// ---- shortcuts -----------------------------------------------------------

/// The menu command that already answers to `shortcut`, if one does.
///
/// The answer comes from the generated shortcut table by way of the same key
/// map the application binds, so "already taken" here means exactly what it
/// means when the key is pressed. The comparison parses both sides: the table
/// writes its modifiers in the order GPUI documents them and a [`Shortcut`]
/// writes its own in Cocoa's order, and only the parsed form is order-free.
fn taken_by(shortcut: Shortcut) -> Option<&'static str> {
    let typed = Keystroke::parse(&shortcut.keystroke()).ok()?;
    let typed = std::slice::from_ref(&typed);
    keymap::default_bindings()
        .into_iter()
        .zip(keymap::SHORTCUTS)
        .find(|(binding, _)| binding.match_keystrokes(typed) == Some(false))
        .map(|(_, row)| command_name(row.0))
}

/// The last component of a menu path, which is what upstream names in its
/// "already taken" message.
fn command_name(path: &str) -> &str {
    path.rsplit(" > ").next().unwrap_or(path)
}

/// Why a shortcut cannot be stored, in CotEditor's wording, or `None`.
///
/// Upstream's `Shortcut.checkCustomizationAvailability`, in its order: the
/// combination's own three refusals first — a key the text view needs, Shift on
/// a letter, no modifier at all — and then the check against the menu, which
/// here is the generated table the menu bar itself is built from.
pub fn shortcut_rejection(shortcut: Shortcut) -> Option<String> {
    if let Some(rejection) = shortcut.rejection() {
        return Some(rejection.message(shortcut));
    }
    taken_by(shortcut).map(|command| {
        format!(
            "“{}” is already taken by the “{command}” command.",
            shortcut.symbol()
        )
    })
}

/// Read the shortcut a keystroke stands for, or `None` for a key with no
/// character of its own.
///
/// A key that is not one character — `escape`, `f2`, `left` — cannot be stored
/// in a [`Shortcut`], and a chord with no modifier is ordinary typing rather
/// than a command. Both are refused here so the field simply ignores them.
///
/// An uppercase key gains an explicit shift, which is `Shortcut::parse`'s own
/// normalisation: AppKit reports ⌘⇧c as `C`, so the two spellings have to
/// collapse into one value before they can be compared.
fn shortcut_of(keystroke: &Keystroke) -> Option<Shortcut> {
    let mut chars = keystroke.key.chars();
    let mut key = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let mut shift = keystroke.modifiers.shift;
    if key.is_ascii_uppercase() {
        key = key.to_ascii_lowercase();
        shift = true;
    }
    Some(Shortcut {
        key,
        control: keystroke.modifiers.control,
        option: keystroke.modifiers.alt,
        shift,
        command: keystroke.modifiers.platform,
    })
}

/// Whether a keystroke is a key on its own, with nothing held down.
fn is_bare(keystroke: &Keystroke) -> bool {
    !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.shift
        && !keystroke.modifiers.platform
}

// ---- names ---------------------------------------------------------------

/// A name for a new snippet, in CotEditor's spelling.
///
/// Upstream's `String.appendingUniqueNumber(in:)`: the number a name already
/// ends with is where the search starts, and the first free candidate wins —
/// the bare base for the first, then `base 2`, `base 3` and so on. So adding to
/// a list holding *Untitled* and *Untitled 2* yields *Untitled 3* rather than
/// *Untitled 1*.
pub fn unique_name(base: &str, taken: &[String]) -> String {
    let (stem, start) = numbering_components(base);
    (start..)
        .map(|number| {
            if number < 2 {
                stem.to_string()
            } else {
                format!("{stem} {number}")
            }
        })
        .find(|candidate| !taken.iter().any(|name| name == candidate))
        .expect("the range is unbounded, so a free name always turns up")
}

/// Split a name into the part before its trailing number, and that number.
///
/// Upstream's regex `(?<base>.+?)(?: (?<number>[0-9]+))?` matched against the
/// whole name: the shortest base that leaves a space and digits at the end, or
/// the whole name with a count of 1 when there are none.
fn numbering_components(name: &str) -> (&str, u32) {
    let trimmed = name.trim_end_matches(|character: char| character.is_ascii_digit());
    // The digits have to be preceded by a space, and the base has to keep at
    // least one character — which is what makes `"Untitled 3x"` one name
    // rather than a base of `"Untitled 3x"` with no number.
    match trimmed.strip_suffix(' ') {
        Some(stem) if !stem.is_empty() => {
            let digits = &name[trimmed.len()..];
            // A count of 0 counts from the bare base just as a count of 1 does,
            // which is what `number < 2` in `unique_name` spells out.
            match digits.parse::<u32>() {
                Ok(number) => (stem, number),
                _ => (name, 1),
            }
        }
        _ => (name, 1),
    }
}

// ---- the Command table ---------------------------------------------------

/// One row of the Command table, and the snippet it describes.
pub struct SnippetRow {
    id: u64,
    name: Entity<InputState>,
    shortcut: Option<Shortcut>,
    /// Why the shortcut cannot be used, if it cannot.
    ///
    /// Kept rather than worked out while rendering, because working it out
    /// means walking the whole key map; it is recomputed whenever the shortcut
    /// changes, which is the only thing that can change the answer.
    error: Option<String>,
    /// The key field's focus, which is what puts the row in "recording" mode.
    focus: FocusHandle,
    scope: Option<String>,
    format: String,
    state: RowState,
    pane: WeakEntity<SettingsWindow>,
    _subscriptions: Vec<Subscription>,
}

impl SnippetRow {
    pub fn new(
        id: u64,
        pane: WeakEntity<SettingsWindow>,
        snippet: Snippet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Name")
                .default_value(snippet.name.clone())
        });
        let focus = cx.focus_handle();

        let mut row = Self {
            id,
            name,
            error: snippet.shortcut.and_then(shortcut_rejection),
            shortcut: snippet.shortcut,
            focus,
            scope: snippet.scope,
            format: snippet.format,
            state: RowState::new(Rc::new(Vec::new())),
            pane,
            _subscriptions: Vec::new(),
        };
        row._subscriptions = vec![cx.subscribe_in(&row.name.clone(), window, Self::on_name_change)];
        row
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self, cx: &App) -> String {
        self.name.read(cx).value().to_string()
    }

    /// Why this row's shortcut is unusable, if it is.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The snippet as the row's fields currently describe it.
    pub fn snippet(&self, cx: &App) -> Snippet {
        Snippet {
            id: self.id,
            name: self.name(cx),
            scope: self.scope.clone(),
            shortcut: self.shortcut,
            format: self.format.clone(),
        }
    }

    /// Adopt the pane's view of the world.
    pub fn sync(&mut self, state: RowState, cx: &mut Context<Self>) {
        if self.state == state {
            return;
        }
        self.state = state;
        cx.notify();
    }

    /// Replace the insertion format, which the pane's editor owns.
    pub fn set_format(&mut self, format: String, cx: &mut Context<Self>) {
        if self.format == format {
            return;
        }
        self.format = format;
        cx.notify();
    }

    /// Put the caret in the name field, which is where *Add* leaves it.
    pub fn focus_name(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.name
            .update(cx, |state, cx| state.focus_handle(cx).focus(window, cx));
    }

    /// A keystroke in the name field: the settings follow the field.
    fn on_name_change(
        &mut self,
        _input: &Entity<InputState>,
        event: &gpui_kit::base::input::InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, gpui_kit::base::input::InputEvent::Change) {
            return;
        }
        self.report(cx);
    }

    /// Hand the pane what this row now says.
    ///
    /// The pane reads its rows back to assemble the list, which is safe here
    /// because a subscription handler runs as an effect — after the row's own
    /// update has ended and its lease has been returned.
    fn report(&self, cx: &mut Context<Self>) {
        self.pane
            .update(cx, |pane, cx| pane.snippets_edited(cx))
            .ok();
    }

    fn set_scope(&mut self, scope: Option<String>, cx: &mut Context<Self>) {
        if self.scope == scope {
            return;
        }
        self.scope = scope;
        self.report(cx);
        cx.notify();
    }

    /// Record a chord, which is what the Key field does with every keystroke it
    /// sees while it holds the focus.
    ///
    /// Upstream's three special cases, in its own order: Escape on its own
    /// cancels, Delete on its own clears, and anything else is the shortcut.
    /// The field stops recording afterwards either way — except when the
    /// shortcut is refused, where upstream puts the user back in the field to
    /// try again.
    fn record(&mut self, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        if is_bare(keystroke) && keystroke.key == "escape" {
            window.blur(cx);
            cx.notify();
            return;
        }
        if is_bare(keystroke) && matches!(keystroke.key.as_str(), "delete" | "backspace") {
            self.apply_shortcut(None, window, cx);
            return;
        }
        // A key with no character of its own is not a shortcut, and neither is
        // a bare letter; both are ignored rather than stored, which leaves the
        // field recording for the next chord.
        if let Some(shortcut) = shortcut_of(keystroke) {
            self.apply_shortcut(Some(shortcut), window, cx);
        }
    }

    fn apply_shortcut(
        &mut self,
        shortcut: Option<Shortcut>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.error = shortcut.and_then(shortcut_rejection);
        if self.error.is_none() {
            self.shortcut = shortcut;
            self.report(cx);
            window.blur(cx);
        } else {
            // Stay in the field so the next attempt starts where this one did.
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    fn element(&self, part: &'static str) -> ElementId {
        ElementId::NamedInteger(part.into(), self.id)
    }

    /// The Syntax cell: upstream's `SyntaxPicker`, whose first choice is *All*.
    fn render_syntax(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let row = cx.entity().downgrade();
        let syntaxes = self.state.syntaxes.clone();
        let label = self.scope.clone().unwrap_or_else(|| "All".to_string());

        Button::new(self.element("snippet-syntax"))
            .ghost()
            .small()
            .w_full()
            .label(label)
            .tooltip("Syntax in which this snippet is offered.")
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, _window, _cx| {
                let clear = row.clone();
                let mut menu = menu.item(PopupMenuItem::new("All").on_click(move |_, _, cx| {
                    clear.update(cx, |row, cx| row.set_scope(None, cx)).ok();
                }));
                menu = menu.separator();
                for name in syntaxes.iter() {
                    let row = row.clone();
                    let name = name.clone();
                    menu = menu.item(PopupMenuItem::new(name.clone()).on_click(move |_, _, cx| {
                        let name = name.clone();
                        row.update(cx, |row, cx| row.set_scope(Some(name), cx)).ok();
                    }));
                }
                menu
            })
    }

    /// The Key cell: a field that records the next chord rather than typing.
    fn render_shortcut(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let recording = self.focus.is_focused(window);
        let row = cx.entity().downgrade();
        let shown = match self.shortcut {
            Some(shortcut) => shortcut.symbol(),
            // Upstream leaves the field empty; the hint goes in the tooltip so
            // the cell does not read as a value the user can select.
            None => String::new(),
        };

        div()
            .id(self.element("snippet-key"))
            .w_full()
            .px_1()
            .py_0p5()
            .rounded_sm()
            .text_sm()
            .track_focus(&self.focus)
            .when(recording, |this| {
                this.border_1().border_color(cx.theme().primary)
            })
            .tooltip(move |_, cx| {
                cx.new(move |_| {
                    Tooltip::new(if recording {
                        "Press the key combination to record."
                    } else {
                        "Click, then press a key combination."
                    })
                })
                .into()
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.focus.focus(window, cx);
                    cx.notify();
                }),
            )
            .capture_key_down(move |event: &KeyDownEvent, window, cx| {
                row.update(cx, |row, cx| row.record(&event.keystroke, window, cx))
                    .ok();
            })
            .child(shown)
    }
}

impl Render for SnippetRow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.state.selected;
        let pane = self.pane.clone();
        let id = self.id;

        h_flex()
            .w_full()
            .gap_2()
            .px_1()
            .py_0p5()
            .items_center()
            .rounded_sm()
            .when(selected, |this| this.bg(cx.theme().accent))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                pane.update(cx, |pane, cx| pane.select_row(Tab::Command, id, cx))
                    .ok();
            })
            .child(cell(CommandColumn::Syntax.width(), self.render_syntax(cx)))
            .child(cell(
                CommandColumn::Name.width(),
                Input::new(&self.name).small().aria_label("Name"),
            ))
            .child(cell(
                CommandColumn::Key.width(),
                self.render_shortcut(window, cx),
            ))
    }
}

// ---- the File Drop table -------------------------------------------------

/// One row of the File Drop table, and the definition it describes.
pub struct FileDropRow {
    id: u64,
    extensions: Entity<InputState>,
    description: Entity<InputState>,
    scope: Option<String>,
    format: String,
    state: RowState,
    pane: WeakEntity<SettingsWindow>,
    _subscriptions: Vec<Subscription>,
}

impl FileDropRow {
    pub fn new(
        id: u64,
        pane: WeakEntity<SettingsWindow>,
        item: FileDropItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let extensions = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("All")
                .default_value(item.extensions.join(", "))
        });
        let description = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Description")
                .default_value(item.description.clone().unwrap_or_default())
        });

        let mut row = Self {
            id,
            extensions,
            description,
            scope: item.scope,
            format: item.format,
            state: RowState::new(Rc::new(Vec::new())),
            pane,
            _subscriptions: Vec::new(),
        };
        row._subscriptions = vec![
            cx.subscribe_in(&row.extensions.clone(), window, Self::on_change),
            cx.subscribe_in(&row.description.clone(), window, Self::on_change),
        ];
        row
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// The definition as the row's fields currently describe it.
    pub fn item(&self, cx: &App) -> FileDropItem {
        let description = self.description.read(cx).value().to_string();
        FileDropItem {
            format: self.format.clone(),
            extensions: parse_extensions(&self.extensions.read(cx).value()),
            scope: self.scope.clone(),
            // An empty description is no description, which is upstream's
            // `item.description ?? ""` read the other way round.
            description: (!description.is_empty()).then_some(description),
        }
    }

    pub fn sync(&mut self, state: RowState, cx: &mut Context<Self>) {
        if self.state == state {
            return;
        }
        self.state = state;
        cx.notify();
    }

    pub fn set_format(&mut self, format: String, cx: &mut Context<Self>) {
        if self.format == format {
            return;
        }
        self.format = format;
        cx.notify();
    }

    fn on_change(
        &mut self,
        _input: &Entity<InputState>,
        event: &gpui_kit::base::input::InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, gpui_kit::base::input::InputEvent::Change) {
            return;
        }
        self.report(cx);
    }

    fn set_scope(&mut self, scope: Option<String>, cx: &mut Context<Self>) {
        if self.scope == scope {
            return;
        }
        self.scope = scope;
        self.report(cx);
        cx.notify();
    }

    /// Hand the pane what this row now says — see [`SnippetRow::report`].
    fn report(&self, cx: &mut Context<Self>) {
        self.pane
            .update(cx, |pane, cx| pane.file_drop_edited(cx))
            .ok();
    }

    fn element(&self, part: &'static str) -> ElementId {
        ElementId::NamedInteger(part.into(), self.id)
    }

    fn render_syntax(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let row = cx.entity().downgrade();
        let syntaxes = self.state.syntaxes.clone();
        let label = self.scope.clone().unwrap_or_else(|| "All".to_string());

        Button::new(self.element("drop-syntax"))
            .ghost()
            .small()
            .w_full()
            .label(label)
            .tooltip("Syntax in which this file drop setting is used.")
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, _window, _cx| {
                let clear = row.clone();
                let mut menu = menu.item(PopupMenuItem::new("All").on_click(move |_, _, cx| {
                    clear.update(cx, |row, cx| row.set_scope(None, cx)).ok();
                }));
                menu = menu.separator();
                for name in syntaxes.iter() {
                    let row = row.clone();
                    let name = name.clone();
                    menu = menu.item(PopupMenuItem::new(name.clone()).on_click(move |_, _, cx| {
                        let name = name.clone();
                        row.update(cx, |row, cx| row.set_scope(Some(name), cx)).ok();
                    }));
                }
                menu
            })
    }
}

impl Render for FileDropRow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.state.selected;
        let pane = self.pane.clone();
        let id = self.id;

        h_flex()
            .w_full()
            .gap_2()
            .px_1()
            .py_0p5()
            .items_center()
            .rounded_sm()
            .when(selected, |this| this.bg(cx.theme().accent))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                pane.update(cx, |pane, cx| pane.select_row(Tab::FileDrop, id, cx))
                    .ok();
            })
            .child(cell(FileDropColumn::Syntax.width(), self.render_syntax(cx)))
            .child(cell(
                FileDropColumn::Extensions.width(),
                Input::new(&self.extensions)
                    .small()
                    .aria_label("Extensions"),
            ))
            .child(cell(
                FileDropColumn::Description.width(),
                Input::new(&self.description)
                    .small()
                    .aria_label("Description"),
            ))
    }
}

/// Split the comma-separated extensions a person types into the list stored.
///
/// Upstream's `format: .csv(omittingEmptyItems: true)`, and it stores them back
/// as a `", "`-joined string — which is why the field shows that spelling.
pub fn parse_extensions(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// The rows every pane shares.
pub fn row_state(syntaxes: &Rc<Vec<String>>, selected: bool) -> RowState {
    RowState {
        syntaxes: syntaxes.clone(),
        selected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- names -----------------------------------------------------------

    #[test]
    fn a_free_base_name_is_used_as_it_is() {
        assert_eq!(unique_name("Untitled", &[]), "Untitled");
        assert_eq!(unique_name("Untitled", &["Other".to_string()]), "Untitled");
    }

    /// Upstream's `appendingUniqueNumber(in:)` counts up from the number the
    /// name already ends with, which is what keeps *Untitled 3* from being
    /// handed out twice.
    #[test]
    fn a_taken_name_gains_the_smallest_free_number() {
        let taken = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };

        assert_eq!(unique_name("Untitled", &taken(&["Untitled"])), "Untitled 2");
        assert_eq!(
            unique_name("Untitled", &taken(&["Untitled", "Untitled 2"])),
            "Untitled 3"
        );
        // The number it already carries is where the search starts, so a gap
        // below it is left alone — upstream's own quirk.
        assert_eq!(
            unique_name("Untitled 3", &taken(&["Untitled 3"])),
            "Untitled 4"
        );
        assert_eq!(
            unique_name("Untitled 3", &taken(&["Untitled", "Untitled 3"])),
            "Untitled 4"
        );
    }

    #[test]
    fn a_number_is_only_a_number_when_a_space_comes_first() {
        // The digits have to be preceded by a space and run to the end.
        assert_eq!(numbering_components("Untitled"), ("Untitled", 1));
        assert_eq!(numbering_components("Untitled 3"), ("Untitled", 3));
        assert_eq!(numbering_components("Untitled 3x"), ("Untitled 3x", 1));
        assert_eq!(numbering_components("3"), ("3", 1));
        assert_eq!(numbering_components("Untitled "), ("Untitled ", 1));
        // A base of nothing would leave a bare number, so the space is kept.
        assert_eq!(numbering_components(" 3"), (" 3", 1));
        // Zero is a number the search can start from; the base is what is left.
        assert_eq!(numbering_components("Untitled 0"), ("Untitled", 0));
    }

    // ---- shortcuts -------------------------------------------------------

    fn key(text: &str) -> Keystroke {
        Keystroke::parse(text).expect(text)
    }

    #[test]
    fn a_chord_becomes_a_shortcut_in_cocoas_spelling() {
        let shortcut = shortcut_of(&key("secondary-shift-k")).unwrap();
        assert_eq!(shortcut.spec(), "$@k");
        assert_eq!(shortcut_of(&key("ctrl-alt-k")).unwrap().spec(), "^~k");
        assert_eq!(shortcut_of(&key("secondary-,")).unwrap().spec(), "@,");
    }

    /// AppKit hands ⌘⇧c back as `C`, so the two spellings have to collapse.
    #[test]
    fn an_uppercase_key_gains_a_shift() {
        assert_eq!(
            shortcut_of(&key("secondary-K")).unwrap(),
            Shortcut::parse("$@k").unwrap()
        );
        assert_eq!(shortcut_of(&key("secondary-K")).unwrap().spec(), "$@k");
    }

    #[test]
    fn a_key_with_no_character_is_not_a_shortcut() {
        // Escape, the function keys and the arrows are named keys: there is no
        // character to store in `Shortcut::key`.
        assert_eq!(shortcut_of(&key("escape")), None);
        assert_eq!(shortcut_of(&key("f2")), None);
        assert_eq!(shortcut_of(&key("left")), None);
        assert_eq!(shortcut_of(&key("secondary-left")), None);
    }

    #[test]
    fn only_a_key_on_its_own_is_bare() {
        assert!(is_bare(&key("escape")));
        assert!(is_bare(&key("backspace")));
        assert!(!is_bare(&key("secondary-backspace")));
        assert!(!is_bare(&key("shift-escape")));
    }

    /// Upstream's `checkCustomizationAvailability`, all four refusals, in its
    /// order: the three the combination answers on its own, then the menu.
    #[test]
    fn a_shortcut_is_refused_the_way_upstream_refuses_it() {
        // Tab moves the caret in a text view.
        let tab = Shortcut::parse("@\t").unwrap();
        assert_eq!(
            shortcut_rejection(tab).as_deref(),
            Some("The combination “⌘\u{2009}\t” is not supported for the shortcut customization.")
        );
        // Shift and a letter types a capital letter.
        assert_eq!(
            shortcut_rejection(Shortcut::parse("$k").unwrap()).as_deref(),
            Some("The Shift key can be used only with another modifier key.")
        );
        // One key with nothing to modify it.
        assert_eq!(
            shortcut_rejection(Shortcut::parse("k").unwrap()).as_deref(),
            Some("Single type is invalid for a shortcut.")
        );
        // And a combination a menu command already has.
        assert_eq!(
            shortcut_rejection(Shortcut::parse("@s").unwrap()).as_deref(),
            Some("“⌘\u{2009}S” is already taken by the “Save” command.")
        );
        // A combination nothing has is accepted.
        assert_eq!(shortcut_rejection(Shortcut::parse("^$k").unwrap()), None);
    }

    /// The check runs against the real key map, so it can only be as right as
    /// that is; this pins the two commands a person would notice first.
    #[test]
    fn the_taken_check_reads_the_generated_table() {
        assert_eq!(taken_by(Shortcut::parse("@s").unwrap()), Some("Save"));
        assert_eq!(taken_by(Shortcut::parse("@f").unwrap()), Some("Find…"));
        assert_eq!(taken_by(Shortcut::parse("@$k").unwrap()), None);
        // The keystrokes are written in a different modifier order on the two
        // sides, which is why the comparison parses rather than compares text.
        assert_eq!(taken_by(Shortcut::parse("@$s").unwrap()), Some("Save As…"));
    }

    #[test]
    fn a_menu_path_is_named_by_its_last_component() {
        assert_eq!(command_name("File > Save As…"), "Save As…");
        assert_eq!(command_name("Save"), "Save");
    }

    // ---- extensions ------------------------------------------------------

    #[test]
    fn extensions_are_split_on_commas_and_trimmed() {
        assert_eq!(parse_extensions("jpg, jpeg, gif"), ["jpg", "jpeg", "gif"]);
        assert_eq!(parse_extensions("jpg,,  ,png"), ["jpg", "png"]);
        assert_eq!(parse_extensions(""), Vec::<String>::new());
        // A single extension is the common case and has no comma in it.
        assert_eq!(parse_extensions("js"), ["js"]);
    }

    // ---- columns ---------------------------------------------------------

    #[test]
    fn the_headers_name_every_column_once() {
        let labels: Vec<&'static str> = CommandColumn::ALL.iter().map(|c| c.label()).collect();
        assert_eq!(labels, ["Syntax", "Name", "Key"]);
        let labels: Vec<&'static str> = FileDropColumn::ALL.iter().map(|c| c.label()).collect();
        assert_eq!(labels, ["Syntax", "Extensions", "Description"]);
    }

    #[test]
    fn the_syntax_column_is_fixed_and_the_text_columns_share_the_rest() {
        assert!(CommandColumn::Syntax.width() > 0.);
        assert!(CommandColumn::Key.width() > 0.);
        assert_eq!(CommandColumn::Name.width(), 0.);
        assert!(FileDropColumn::Syntax.width() > 0.);
        assert_eq!(FileDropColumn::Extensions.width(), 0.);
        assert_eq!(FileDropColumn::Description.width(), 0.);
    }

    // ---- tabs ------------------------------------------------------------

    #[test]
    fn the_tabs_are_named_and_described_the_way_upstream_words_them() {
        assert_eq!(Tab::Command.label(), "Command");
        assert_eq!(Tab::FileDrop.label(), "File Drop");
        assert_eq!(
            Tab::Command.intro(),
            "Text to be inserted by a command in the menu or by keyboard shortcut:"
        );
        assert_eq!(
            Tab::FileDrop.intro(),
            "Text to be inserted by dropping files to the editor:"
        );
    }
}
