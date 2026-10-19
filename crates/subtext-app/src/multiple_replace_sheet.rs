//! The *Multiple Replace* sheet.
//!
//! CotEditor opens a window holding a sidebar of named rule sets and, beside it,
//! a table of rules: an enable checkbox, the find string, the replacement, an
//! `RE` switch for regular expressions, an `IC` switch for case-insensitivity,
//! and a free-text description. Below the table sit *Highlight All*, *Replace
//! All*, an *In Selection* checkbox, a result line, and a button that discloses
//! the advanced find options.
//!
//! SubText opens the table as a dialog. The sidebar of named sets is
//! `ReplacementManager`'s job — files on disk, import and export, rename and
//! duplicate — and is not part of this sheet; what is here is the rule table
//! itself and everything it can do to the document.
//!
//! # Where the state lives
//!
//! Each row is its own entity, and each row's fields *are* its `InputState`s:
//! the sheet asks the rows for the rules it runs rather than keeping a second
//! copy in step. That is the same choice the find bar makes with its query, and
//! it is what lets a row repaint its own warning icon while the rest of the
//! table stays where it is.
//!
//! The two things a row needs from the sheet — the advanced settings, and
//! whether the row is the selected one — are pushed into the row rather than
//! pulled from the sheet, because the sheet reads every row while it renders
//! and a row that read the sheet back would be asking for the entity that is
//! already being borrowed.
//!
//! The rules are not persisted. Upstream persists them because it has a sidebar
//! of definitions to persist them *into*; SubText has nowhere to put a set, so a
//! sheet's rules last as long as the sheet does.
//!
//! # Two deliberate differences
//!
//! *The advanced options are disclosed inline* rather than in a popover. A
//! popover is anchored to a control inside the sheet, and the sheet is a modal
//! dialog whose scrim would swallow the click that opens it; a `Collapsible`
//! section gives the same eight switches, the same labels and the same
//! enablement without fighting the overlay.
//!
//! *The sheet opens holding one blank rule.* Upstream's table can be empty,
//! because the sidebar has already created a definition to put rules into and
//! the empty state sits beside a file list that explains it. Here the table is
//! the whole feature, so an empty one would read as broken. Removing the last
//! rule puts a blank one back, which is upstream's own behaviour.

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    collapsible::Collapsible,
    h_flex,
    input::{Input, InputState},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, ElementId, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, Render, StatefulInteractiveElement as _,
    Styled as _, Subscription, WeakEntity, Window, div, rems,
};

use subtext_core::multireplace::{MultipleReplace, PatternError, Replacement, Settings};

use crate::workspace::Workspace;

/// Stable identity for one rule.
///
/// A rule's element ids must not be its position in the table: removing the
/// first rule would hand every rule below a different id, resetting the focus
/// and the hover state of rows the user never touched. The counter is monotonic
/// for the life of the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RuleId(u64);

impl RuleId {
    /// The element id for one control inside this rule's row.
    fn element(self, part: &'static str) -> ElementId {
        ElementId::NamedInteger(part.into(), self.0)
    }
}

/// What a batch of rules was asked to do, which decides the wording of the
/// result line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Find,
    Replace,
}

/// The line under the table, which is upstream's `FindResult.message`.
fn result_message(action: Action, count: usize) -> String {
    match action {
        Action::Find => format!("{count} matches"),
        Action::Replace => format!("{count} replaced"),
    }
}

/// The table's columns, in the order the header and every row lay them out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Enabled,
    Find,
    Replacement,
    RegularExpression,
    IgnoresCase,
    Description,
}

impl Column {
    const ALL: [Column; 6] = [
        Column::Enabled,
        Column::Find,
        Column::Replacement,
        Column::RegularExpression,
        Column::IgnoresCase,
        Column::Description,
    ];

    /// The header text. Upstream leaves the enable column's title blank and
    /// names it only in the header's tooltip.
    fn label(self) -> &'static str {
        match self {
            Column::Enabled => "",
            Column::Find => "Find",
            Column::Replacement => "Replace With",
            Column::RegularExpression => "RE",
            Column::IgnoresCase => "IC",
            Column::Description => "Description",
        }
    }

    /// The width of a fixed column, in rems.
    ///
    /// Upstream's table is 20/104/104/28/28/107 points wide; the three fixed
    /// columns keep their proportions here, and the three text columns share
    /// whatever is left.
    fn width(self) -> f32 {
        match self {
            Column::Enabled => 1.25,
            Column::RegularExpression | Column::IgnoresCase => 1.75,
            Column::Find | Column::Replacement | Column::Description => 0.,
        }
    }

    /// Whether the column is sized to its content rather than to the space left
    /// over.
    fn is_fixed(self) -> bool {
        self.width() > 0.
    }
}

/// One table cell, sized to the column it belongs to.
fn cell(column: Column, content: impl IntoElement) -> gpui_kit::Div {
    let base = div().flex_shrink_0();
    let base = if column.is_fixed() {
        base.w(rems(column.width()))
    } else {
        // `min_w(0)` so a long find string shrinks the field instead of pushing
        // the columns to its right off the edge.
        base.flex_1().min_w(rems(0.)).overflow_hidden()
    };
    base.child(content)
}

/// The header row.
fn header(cx: &App) -> impl IntoElement {
    let mut row = h_flex().w_full().gap_2().px_1().py_1().flex_shrink_0();
    for column in Column::ALL {
        row = row.child(cell(
            column,
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(column.label()),
        ));
    }
    row.border_b_1().border_color(cx.theme().border)
}

/// A muted hint under a control.
fn note(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

/// Where `id` sits in the table.
fn position_of(rows: &[Entity<RuleRow>], id: RuleId, cx: &App) -> Option<usize> {
    rows.iter().position(|row| row.read(cx).id == id)
}

// ---- one rule -------------------------------------------------------------

/// One row of the table, and the rule it describes.
pub struct RuleRow {
    id: RuleId,
    find: Entity<InputState>,
    replacement: Entity<InputState>,
    description: Entity<InputState>,
    uses_regular_expression: bool,
    ignores_case: bool,
    is_enabled: bool,
    /// The advanced settings in force, pushed in by the sheet.
    settings: Settings,
    /// Whether this is the row the sheet's *Remove* would delete.
    selected: bool,
    /// The sheet, so a click on the row can move its selection and a keystroke
    /// can refresh its footer. Only ever used from an event handler.
    sheet: WeakEntity<MultipleReplaceSheet>,
    /// Kept alive so a keystroke re-renders the row's own warning icon.
    _subscriptions: Vec<Subscription>,
}

impl RuleRow {
    fn new(
        id: RuleId,
        sheet: WeakEntity<MultipleReplaceSheet>,
        rule: Replacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let find = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Find")
                .default_value(rule.find_string.clone())
        });
        let replacement = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Replace With")
                .default_value(rule.replacement_string.clone())
        });
        let description = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Description")
                .default_value(rule.description.clone().unwrap_or_default())
        });

        let subscriptions = vec![
            cx.subscribe_in(&find, window, Self::on_input_change),
            cx.subscribe_in(&replacement, window, Self::on_input_change),
            cx.subscribe_in(&description, window, Self::on_input_change),
        ];

        Self {
            id,
            find,
            replacement,
            description,
            uses_regular_expression: rule.uses_regular_expression,
            ignores_case: rule.ignores_case,
            is_enabled: rule.is_enabled,
            settings: Settings::default(),
            selected: false,
            sheet,
            _subscriptions: subscriptions,
        }
    }

    /// The rule as the row's fields currently describe it.
    fn rule(&self, cx: &App) -> Replacement {
        let description = self.description.read(cx).value().to_string();
        Replacement {
            find_string: self.find.read(cx).value().to_string(),
            replacement_string: self.replacement.read(cx).value().to_string(),
            uses_regular_expression: self.uses_regular_expression,
            ignores_case: self.ignores_case,
            // An empty description is no description: upstream stores `nil`
            // there, and the difference decides whether a rule counts as
            // content at all.
            description: (!description.is_empty()).then_some(description),
            is_enabled: self.is_enabled,
        }
    }

    /// The reason this rule cannot be used, if it cannot.
    fn error(&self, cx: &App) -> Option<PatternError> {
        self.rule(cx).validate(&self.settings).err()
    }

    /// Adopt the sheet's view of the world.
    fn sync(&mut self, settings: Settings, selected: bool, cx: &mut Context<Self>) {
        if self.settings == settings && self.selected == selected {
            return;
        }
        self.settings = settings;
        self.selected = selected;
        cx.notify();
    }

    /// A keystroke in one of the fields: repaint the row's warning and the
    /// sheet's footer, both of which describe what the fields now say.
    fn on_input_change(
        &mut self,
        _input: &Entity<InputState>,
        event: &gpui_kit::base::input::InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, gpui_kit::base::input::InputEvent::Change) {
            return;
        }
        self.sheet.update(cx, |_, cx| cx.notify()).ok();
        cx.notify();
    }

    fn select(&mut self, cx: &mut Context<Self>) {
        let id = self.id;
        self.sheet.update(cx, |sheet, cx| sheet.select(id, cx)).ok();
    }

    /// Focus this row's find field, which is where *Add* leaves the caret.
    fn focus_find(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.find
            .update(cx, |state, cx| state.focus_handle(cx).focus(window, cx));
    }

    fn render_find(
        &self,
        error: Option<&PatternError>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_1()
            .items_center()
            // Upstream puts a warning image in the find cell of a rule that will
            // not compile. Its message goes in that image's tooltip there; here
            // it is repeated under the table, because a bare glyph is not a
            // label and a tooltip needs a component to hang off.
            .when_some(error, |this, _| {
                this.child(
                    Icon::new(gpui_kit::assets::IconName::TriangleAlert)
                        .xsmall()
                        .flex_shrink_0()
                        .text_color(cx.theme().danger),
                )
            })
            .child(
                // The field takes whatever the warning icon leaves: `Input`
                // sizes itself to its container, so an unwrapped one beside the
                // icon would ask for the whole cell.
                div().flex_1().min_w(rems(0.)).child(
                    Input::new(&self.find)
                        .small()
                        .disabled(!self.is_enabled)
                        .aria_label("Find"),
                ),
            )
    }
}

impl Render for RuleRow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let error = self.error(cx);
        let enabled = self.is_enabled;
        let selected = self.selected;

        h_flex()
            .w_full()
            .gap_2()
            .px_1()
            .py_0p5()
            .items_center()
            .rounded_sm()
            .when(selected, |this| this.bg(cx.theme().accent))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.select(cx)),
            )
            .child(cell(
                Column::Enabled,
                Checkbox::new(self.id.element("enabled"))
                    .checked(self.is_enabled)
                    .accessibility_label("Enable")
                    .tooltip("Select to use this replacement rule.")
                    .on_click(cx.listener(|this, checked, _, cx| {
                        this.is_enabled = *checked;
                        this.sheet.update(cx, |_, cx| cx.notify()).ok();
                        cx.notify();
                    })),
            ))
            .child(cell(Column::Find, self.render_find(error.as_ref(), cx)))
            .child(cell(
                Column::Replacement,
                Input::new(&self.replacement)
                    .small()
                    .disabled(!enabled)
                    .aria_label("Replace With"),
            ))
            .child(cell(
                Column::RegularExpression,
                Checkbox::new(self.id.element("regular-expression"))
                    .checked(self.uses_regular_expression)
                    .disabled(!enabled)
                    .accessibility_label("RE")
                    .tooltip("Select to search with regular expression.")
                    .on_click(cx.listener(|this, checked, _, cx| {
                        this.uses_regular_expression = *checked;
                        this.sheet.update(cx, |_, cx| cx.notify()).ok();
                        cx.notify();
                    })),
            ))
            .child(cell(
                Column::IgnoresCase,
                Checkbox::new(self.id.element("ignores-case"))
                    .checked(self.ignores_case)
                    .disabled(!enabled)
                    .accessibility_label("IC")
                    .tooltip("Select to ignore character case on search.")
                    .on_click(cx.listener(|this, checked, _, cx| {
                        this.ignores_case = *checked;
                        cx.notify();
                    })),
            ))
            .child(cell(
                Column::Description,
                Input::new(&self.description)
                    .small()
                    .disabled(!enabled)
                    .aria_label("Description"),
            ))
    }
}

// ---- the sheet ------------------------------------------------------------

/// The *Multiple Replace* sheet, and everything it remembers while open.
pub struct MultipleReplaceSheet {
    workspace: WeakEntity<Workspace>,
    rows: Vec<Entity<RuleRow>>,
    next_rule_id: u64,
    selected: Option<RuleId>,
    settings: Settings,
    in_selection: bool,
    /// Whether the advanced options are disclosed.
    options_open: bool,
    /// The line under the table: how the last batch went.
    result: Option<String>,
}

impl MultipleReplaceSheet {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut sheet = Self {
            workspace,
            rows: Vec::new(),
            next_rule_id: 1,
            selected: None,
            settings: Settings::default(),
            in_selection: false,
            options_open: false,
            result: None,
        };
        sheet.add_rule(window, cx);
        sheet
    }

    /// The rules as the table currently describes them.
    pub fn definition(&self, cx: &App) -> MultipleReplace {
        MultipleReplace {
            replacements: self.rows.iter().map(|row| row.read(cx).rule(cx)).collect(),
            settings: self.settings.clone(),
        }
    }

    // ---- editing the table -----------------------------------------------

    /// Push the sheet's settings and selection into every row.
    fn sync_rows(&self, cx: &mut Context<Self>) {
        for row in &self.rows {
            let id = row.read(cx).id;
            let settings = self.settings.clone();
            let selected = self.selected == Some(id);
            row.update(cx, |row, cx| row.sync(settings, selected, cx));
        }
    }

    fn select(&mut self, id: RuleId, cx: &mut Context<Self>) {
        if self.selected == Some(id) {
            return;
        }
        self.selected = Some(id);
        self.sync_rows(cx);
        cx.notify();
    }

    /// Insert a blank rule after the selected one, and put the caret in it.
    fn add_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = RuleId(self.next_rule_id);
        self.next_rule_id += 1;
        let sheet = cx.weak_entity();
        let row = cx.new(|cx| RuleRow::new(id, sheet, Replacement::default(), window, cx));

        // Upstream inserts below the last selected row rather than at the end,
        // so adding several rules in a row keeps them together.
        let at = self
            .selected
            .and_then(|selected| position_of(&self.rows, selected, cx))
            .map_or(self.rows.len(), |index| index + 1);
        self.rows.insert(at, row.clone());
        self.selected = Some(id);
        self.result = None;
        self.sync_rows(cx);
        row.update(cx, |row, cx| row.focus_find(window, cx));
        cx.notify();
    }

    /// Remove the selected rule.
    fn remove_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self
            .selected
            .and_then(|selected| position_of(&self.rows, selected, cx))
        else {
            return;
        };
        self.rows.remove(index);
        self.result = None;
        // Upstream puts a blank rule back when the table empties, so the sheet
        // never becomes a dead end.
        if self.rows.is_empty() {
            self.selected = None;
            self.add_rule(window, cx);
            return;
        }
        let next = self.rows[index.min(self.rows.len() - 1)].read(cx).id;
        self.selected = Some(next);
        self.sync_rows(cx);
        cx.notify();
    }

    // ---- running the rules ------------------------------------------------

    /// Replace every match the rules describe, and report how many there were.
    pub fn replace_all(&mut self, cx: &mut Context<Self>) {
        let definition = self.definition(cx);
        let in_selection = self.in_selection;
        let outcome = self.workspace.update_in(cx, move |workspace, window, cx| {
            workspace.apply_multiple_replace(&definition, in_selection, window, cx)
        });
        if let Ok(Some(count)) = outcome {
            self.result = Some(result_message(Action::Replace, count));
        }
        cx.notify();
    }

    /// Paint every match the rules describe, and report how many there were.
    pub fn highlight_all(&mut self, cx: &mut Context<Self>) {
        let definition = self.definition(cx);
        let in_selection = self.in_selection;
        let outcome = self.workspace.update_in(cx, move |workspace, _window, cx| {
            workspace.highlight_multiple_replace(&definition, in_selection, cx)
        });
        if let Ok(Some(count)) = outcome {
            self.result = Some(result_message(Action::Find, count));
        }
        cx.notify();
    }

    // ---- rendering --------------------------------------------------------

    /// The reason the batch would skip something, if it would.
    ///
    /// The first broken rule is enough: fixing it moves the message along to the
    /// next one.
    fn first_error(&self, cx: &App) -> Option<PatternError> {
        self.rows.iter().find_map(|row| row.read(cx).error(cx))
    }

    fn render_advanced(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(
                Button::new("multiple-replace-options")
                    .small()
                    .icon(gpui_kit::assets::IconName::Settings)
                    .label("Advanced")
                    .accessibility_label("Advanced options")
                    .tooltip("Show advanced options")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.options_open = !this.options_open;
                        cx.notify();
                    })),
            )
            .child(
                Collapsible::new()
                    .open(self.options_open)
                    .motion_id("multiple-replace-options-panel")
                    .content(self.render_options(cx)),
            )
    }

    /// The eight switches of upstream's *Advanced Find Options*.
    ///
    /// Four of them move, and four report a decision the engine has already
    /// made; those render disabled with the reason, rather than looking live and
    /// doing nothing.
    fn render_options(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.settings.clone();
        v_flex()
            .gap_1()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(note("Advanced Find Options", cx))
            .child(option_switch(
                "mr-matches-full-word",
                "Match only whole words",
                settings.matches_full_word,
                false,
                "Restrict search results to whole words.",
                cx,
                |settings, value| settings.matches_full_word = value,
            ))
            .child(option_switch(
                "mr-is-literal-search",
                "Distinguish characters strictly",
                settings.is_literal_search,
                true,
                "SubText's search compares characters exactly, which is what this asks for.",
                cx,
                |settings, value| settings.is_literal_search = value,
            ))
            .child(option_switch(
                "mr-ignores-diacritic-marks",
                "Ignore diacritical marks",
                settings.ignores_diacritic_marks,
                true,
                "Needs the ICU collation SubText does not carry",
                cx,
                |settings, value| settings.ignores_diacritic_marks = value,
            ))
            .child(option_switch(
                "mr-ignores-width",
                "Ignore width differences",
                settings.ignores_width,
                true,
                "Needs the ICU collation SubText does not carry",
                cx,
                |settings, value| settings.ignores_width = value,
            ))
            .child(option_switch(
                "mr-is-single-line",
                "Dot matches line separators",
                settings.is_single_line,
                false,
                "Allow . to match any character, including newline characters (singleline).",
                cx,
                |settings, value| settings.is_single_line = value,
            ))
            .child(option_switch(
                "mr-is-multiline",
                "Anchors match lines",
                settings.is_multiline,
                false,
                "Allow ^ and $ to match the start and end of lines (multiline).",
                cx,
                |settings, value| settings.is_multiline = value,
            ))
            .child(option_switch(
                "mr-uses-unicode-boundaries",
                "Use Unicode word boundaries",
                settings.uses_unicode_boundaries,
                true,
                "SubText's word boundaries are Unicode-aware by definition.",
                cx,
                |settings, value| settings.uses_unicode_boundaries = value,
            ))
            .child(option_switch(
                "mr-unescapes-replacement-string",
                "Unescape replacement text",
                settings.unescapes_replacement_string,
                false,
                "Unescape metacharacters with backslash in replacement text.",
                cx,
                |settings, value| settings.unescapes_replacement_string = value,
            ))
    }

    fn render_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(
                Button::new("multiple-replace-add")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::Plus)
                    .accessibility_label("Add")
                    .tooltip("Add new item")
                    .on_click(cx.listener(|this, _, window, cx| this.add_rule(window, cx))),
            )
            .child(
                Button::new("multiple-replace-remove")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::Minus)
                    .accessibility_label("Remove")
                    .tooltip("Delete selected items")
                    .disabled(self.selected.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.remove_rule(window, cx))),
            )
            .child(div().flex_1())
            .child(self.render_advanced(cx))
    }
}

impl Render for MultipleReplaceSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let error = self.first_error(cx);
        let rows: Vec<gpui_kit::AnyElement> = self
            .rows
            .iter()
            .map(|row| row.clone().into_any_element())
            .collect();

        v_flex()
            .w_full()
            .gap_3()
            .child(
                div().text_sm().child(
                    "Perform replacement using these rules in succession from the top down:",
                ),
            )
            .child(
                v_flex()
                    .w_full()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(header(cx))
                    .child(
                        v_flex()
                            .id("multiple-replace-rows")
                            .w_full()
                            .max_h(rems(16.))
                            .overflow_y_scroll()
                            .py_1()
                            .children(rows),
                    ),
            )
            .child(self.render_buttons(cx))
            .child(
                v_flex()
                    .gap_1()
                    .child(note("Invalid rules will be skipped.", cx))
                    .when_some(error, |this, error| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().danger)
                                .child(error.message()),
                        )
                    })
                    .when_some(self.result.clone(), |this, result| {
                        this.child(div().text_sm().child(result))
                    }),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        Checkbox::new("multiple-replace-in-selection")
                            .checked(self.in_selection)
                            .label("In Selection")
                            .tooltip("Select to replace text only in selection.")
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.in_selection = *checked;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("multiple-replace-highlight")
                            .label("Highlight All")
                            .tooltip("Highlight all matches of the replacement rules.")
                            .on_click(cx.listener(|this, _, _, cx| this.highlight_all(cx))),
                    )
                    .child(
                        Button::new("multiple-replace-replace-all")
                            .primary()
                            .label("Replace All")
                            .tooltip("Replace all matches according to the replacement rules.")
                            .on_click(cx.listener(|this, _, _, cx| this.replace_all(cx))),
                    ),
            )
    }
}

/// One of the advanced switches.
///
/// `apply` is a function pointer rather than a closure so it stays `Copy` and
/// can be moved into the listener. `inert` marks a switch that reports a
/// decision the engine has already made; it renders disabled and carries the
/// reason as its tooltip.
fn option_switch(
    id: &'static str,
    label: &'static str,
    checked: bool,
    inert: bool,
    tooltip: &'static str,
    cx: &mut Context<MultipleReplaceSheet>,
    apply: fn(&mut Settings, bool),
) -> impl IntoElement {
    Checkbox::new(id)
        .checked(checked)
        .disabled(inert)
        .label(label)
        .tooltip(tooltip)
        .on_click(cx.listener(move |this, checked, _, cx| {
            apply(&mut this.settings, *checked);
            // The switches decide whether a rule compiles, so the rows have to
            // hear about a change before they repaint their warning icons.
            this.sync_rows(cx);
            cx.notify();
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_names_every_column_once() {
        assert_eq!(Column::ALL.len(), 6);
        let labels: Vec<&'static str> = Column::ALL.iter().map(|column| column.label()).collect();
        assert_eq!(
            labels,
            vec!["", "Find", "Replace With", "RE", "IC", "Description"]
        );
    }

    #[test]
    fn the_text_columns_share_the_leftover_space() {
        for column in [Column::Find, Column::Replacement, Column::Description] {
            assert!(!column.is_fixed(), "{column:?}");
        }
        for column in [
            Column::Enabled,
            Column::RegularExpression,
            Column::IgnoresCase,
        ] {
            assert!(column.is_fixed(), "{column:?}");
        }
    }

    #[test]
    fn the_result_line_reads_the_way_upstream_words_it() {
        assert_eq!(result_message(Action::Find, 3), "3 matches");
        assert_eq!(result_message(Action::Replace, 0), "0 replaced");
        assert_eq!(result_message(Action::Replace, 12), "12 replaced");
    }

    #[test]
    fn a_rule_id_keeps_its_element_ids_apart() {
        assert_ne!(
            RuleId(1).element("enabled"),
            RuleId(2).element("enabled"),
            "two rules must not share a control id"
        );
        assert_ne!(
            RuleId(1).element("enabled"),
            RuleId(1).element("ignores-case")
        );
    }

    #[test]
    fn the_sheet_starts_from_the_switches_that_are_actually_in_force() {
        // The switches the sheet renders come straight from `Settings`, so its
        // defaults are what the user sees on opening: the four that move at
        // upstream's defaults, and the four inert ones reporting the engine.
        let settings = Settings::default();
        assert!(!settings.matches_full_word);
        assert!(!settings.is_single_line);
        assert!(settings.is_multiline);
        assert!(settings.unescapes_replacement_string);
        assert!(settings.is_literal_search);
        assert!(settings.uses_unicode_boundaries);
        assert!(!settings.ignores_diacritic_marks);
        assert!(!settings.ignores_width);
    }

    #[test]
    fn a_row_that_has_never_been_typed_into_is_an_empty_rule() {
        // `RuleRow::rule` is the only place the sheet's model is assembled, so
        // the shape of a blank row is worth pinning: no description means
        // `None`, not `Some("")`.
        let rule = Replacement::default();
        assert!(rule.is_empty());
        assert_eq!(rule.description, None);
        assert!(rule.is_enabled, "a new rule is switched on");
    }

    #[test]
    fn an_empty_set_of_rules_is_valid() {
        let definition = MultipleReplace::default();
        assert!(definition.is_empty());
        assert!(definition.validate(), "no rules is not an invalid set");
    }
}
