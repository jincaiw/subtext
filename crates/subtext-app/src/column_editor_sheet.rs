//! The *Column Editor* sheet.
//!
//! Notepad++'s column editor, which CotEditor has no equivalent of: insert the
//! same text — or a numbered run — at one display column on every selected
//! line, padding the lines that are too short with spaces. That padding is
//! what turns a sequence of inserts into a column.
//!
//! # What the sheet knows before it opens
//!
//! The workspace seeds it with the caret's display column, the document's tab
//! width and the number of lines the selection covers. The column is shown
//! 1-based because that is how people count columns; the text engine works in
//! 0-based display columns, so the field is converted in one place
//! ([`ColumnEditorSheet::request`]).
//!
//! # Modes
//!
//! Notepad++ decides between text and numbering by which control the user
//! edited last. A sheet that silently preferred one of them would be a sheet
//! whose result depends on where the focus happened to be, so the mode is an
//! explicit pair of radio buttons here. Both modes produce the same shape of
//! edit; only the inserted string differs.

use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState, NumberInput},
    radio::RadioGroup,
    v_flex,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, Styled as _, WeakEntity, Window, div, rems,
};

use subtext_core::textops::{ColumnContent, ColumnInsert, ColumnNumberFormat, ColumnNumbering};

use crate::dialog_escape::DismissOnEscape as _;
use crate::localization;
use crate::workspace::Workspace;

/// How wide the label column beside each field is.
const LABEL_COLUMN: f32 = 8.;

/// Which half of the sheet is live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Text,
    Number,
}

impl Mode {
    /// The radio group's order, which is also its index order.
    const ALL: [Mode; 2] = [Self::Text, Self::Number];

    fn label(self) -> &'static str {
        match self {
            Mode::Text => "Text",
            Mode::Number => "Number",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0)
    }

    fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::Text)
    }
}

/// The base the numbered run is written in, in Notepad++'s order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormatChoice {
    Decimal,
    Hexadecimal,
    Octal,
    Binary,
}

impl FormatChoice {
    const ALL: [FormatChoice; 4] = [Self::Decimal, Self::Hexadecimal, Self::Octal, Self::Binary];

    fn label(self) -> &'static str {
        match self {
            Self::Decimal => "Decimal",
            Self::Hexadecimal => "Hex",
            Self::Octal => "Octal",
            Self::Binary => "Binary",
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|format| *format == self)
            .unwrap_or(0)
    }

    fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::Decimal)
    }

    fn format(self) -> ColumnNumberFormat {
        match self {
            Self::Decimal => ColumnNumberFormat::Decimal,
            Self::Hexadecimal => ColumnNumberFormat::Hexadecimal,
            Self::Octal => ColumnNumberFormat::Octal,
            Self::Binary => ColumnNumberFormat::Binary,
        }
    }
}

/// The *Column Editor* sheet, and everything it remembers while open.
pub struct ColumnEditorSheet {
    workspace: WeakEntity<Workspace>,
    /// The document's tab width, for the display-column conversions.
    tab_width: usize,
    /// How many lines the selection covers, shown as a check on the target.
    lines: usize,
    mode: Mode,
    text: Entity<InputState>,
    column: Entity<InputState>,
    start: Entity<InputState>,
    step: Entity<InputState>,
    leading_zeros: bool,
    format: FormatChoice,
    /// Whether the sheet has already put the caret in its first field.
    focused_once: bool,
}

impl ColumnEditorSheet {
    /// Build the sheet on the caret's column.
    ///
    /// `column` is a 0-based display column; the field shows it 1-based.
    pub fn new(
        workspace: WeakEntity<Workspace>,
        tab_width: usize,
        column: usize,
        lines: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let text = cx.new(|cx| InputState::new(window, cx));
        let column_state = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value((column + 1).to_string())
                .min(1.0)
        });
        let start = cx.new(|cx| InputState::new(window, cx).default_value("1"));
        let step = cx.new(|cx| InputState::new(window, cx).default_value("1"));

        Self {
            workspace,
            tab_width,
            lines,
            mode: Mode::Text,
            text,
            column: column_state,
            start,
            step,
            leading_zeros: false,
            format: FormatChoice::Decimal,
            focused_once: false,
        }
    }

    /// The insertion the controls currently describe.
    fn request(&self, cx: &App) -> ColumnInsert {
        let content = match self.mode {
            Mode::Text => ColumnContent::Text(self.text.read(cx).value().to_string()),
            Mode::Number => ColumnContent::Number(ColumnNumbering {
                start: integer(self.start.read(cx).value().as_ref(), 1),
                step: integer(self.step.read(cx).value().as_ref(), 1),
                leading_zeros: self.leading_zeros,
                format: self.format.format(),
            }),
        };
        ColumnInsert {
            // The field counts from 1; the text engine counts from 0.
            column: number(self.column.read(cx).value().as_ref(), 1).saturating_sub(1),
            tab_width: self.tab_width,
            content,
        }
    }

    /// Send the insertion to the document.
    ///
    /// The window comes from the click listener rather than from the
    /// workspace's own binding: a dialog can be raised before the workspace
    /// entity has a current window (and a UI test opens exactly that way), and
    /// `update_in` would then fail with "entity has no current window" — a
    /// silent no-op after the sheet has already closed.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let request = self.request(cx);
        // The workspace owns this dialog, so the weak handle resolves; a
        // document closed while the sheet was open leaves the click with
        // nothing to edit, which is not an error worth reporting.
        let _ = self.workspace.update(cx, |workspace, cx| {
            workspace.insert_at_column(request, window, cx);
        });
    }

    fn render_text_field(&self) -> impl IntoElement {
        field(
            localization::t("Text to insert:"),
            Input::new(&self.text)
                .id("column-editor-text")
                .w_full()
                .small(),
        )
    }

    fn render_number_fields(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(field(
                localization::t("Initial number:"),
                NumberInput::new(&self.start)
                    .small()
                    .accessibility_label(localization::t("Initial number:")),
            ))
            .child(field(
                localization::t("Increase by:"),
                NumberInput::new(&self.step)
                    .small()
                    .accessibility_label(localization::t("Increase by:")),
            ))
            .child(field(
                localization::t("Format:"),
                RadioGroup::horizontal("column-editor-format")
                    .selected_index(Some(self.format.index()))
                    .children(FormatChoice::ALL.map(|format| localization::t(format.label())))
                    .on_click(cx.listener(|this, index: &usize, _, cx| {
                        this.format = FormatChoice::from_index(*index);
                        cx.notify();
                    })),
            ))
            .child(
                Checkbox::new("column-editor-leading-zeros")
                    .checked(self.leading_zeros)
                    .label(localization::t("Leading zeros"))
                    .on_click(cx.listener(|this, checked, _, cx| {
                        this.leading_zeros = *checked;
                        cx.notify();
                    })),
            )
    }

    fn render_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .justify_end()
            .child(
                Button::new("column-editor-cancel")
                    .label(localization::t("Cancel"))
                    .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
            )
            .child(
                Button::new("column-editor-apply")
                    .primary()
                    .label(localization::t("Insert"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit(window, cx);
                        window.close_dialog(cx);
                    })),
            )
    }
}

impl Render for ColumnEditorSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The caret goes into the first field, so the keyboard stops belonging
        // to the editor behind and Escape/Enter reach the sheet. Deferred from
        // the first render, as in the other sheets: focus needs a laid-out
        // tree.
        if !self.focused_once {
            self.focused_once = true;
            let caret =
                gpui_kit::component::input::AnyInputState::from(self.text.clone()).focus_handle(cx);
            window.focus(&caret, cx);
        }

        let mut body = v_flex()
            .gap_2()
            .child(field(
                localization::t("Column:"),
                NumberInput::new(&self.column)
                    .small()
                    .accessibility_label(localization::t("Column:")),
            ))
            .child(field(
                localization::t("Insert:"),
                RadioGroup::horizontal("column-editor-mode")
                    .selected_index(Some(self.mode.index()))
                    .children(Mode::ALL.map(|mode| localization::t(mode.label())))
                    .on_click(cx.listener(|this, index: &usize, window, cx| {
                        this.mode = Mode::from_index(*index);
                        // Move the caret to the field that just appeared: the
                        // mode's controls replace the other mode's, and focus
                        // left on a field that is no longer drawn would send
                        // Enter nowhere.
                        let field = match this.mode {
                            Mode::Text => this.text.clone(),
                            Mode::Number => this.start.clone(),
                        };
                        let caret =
                            gpui_kit::component::input::AnyInputState::from(field).focus_handle(cx);
                        window.focus(&caret, cx);
                        cx.notify();
                    })),
            ));
        body = match self.mode {
            Mode::Text => body.child(self.render_text_field()),
            Mode::Number => body.child(self.render_number_fields(cx)),
        };

        let lines = self.lines;
        v_flex()
            .gap_3()
            .dismiss_on_escape()
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" {
                    this.submit(window, cx);
                    window.close_dialog(cx);
                }
            }))
            .child(body)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if lines == 1 {
                        localization::t(
                            "Applies to the caret's line. Short lines are padded with spaces.",
                        )
                        .to_string()
                    } else {
                        localization::t(
                            "Applies to {} selected lines. Short lines are padded with spaces.",
                        )
                        .replace("{}", &lines.to_string())
                    }),
            )
            .child(self.render_buttons(cx))
    }
}

/// A label beside its control, so the rows line up.
fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_2()
        .child(div().w(rems(LABEL_COLUMN)).text_sm().child(label))
        .child(div().flex_1().child(control))
}

/// The unsigned number a field holds, or `fallback` when it holds something
/// else.
///
/// `NumberInput` normally holds digits, but a half-typed value has to mean
/// something on Enter: the fallback keeps the sheet's promise that confirming
/// inserts a column rather than doing nothing.
fn number(value: &str, fallback: usize) -> usize {
    value.trim().parse().unwrap_or(fallback)
}

/// The signed number a numbered run's fields hold.
///
/// A negative *increase* counts down, which is why this one is signed.
fn integer(value: &str, fallback: i64) -> i64 {
    value.trim().parse().unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_field_falls_back_when_it_holds_nothing_useful() {
        assert_eq!(number("3", 1), 3);
        assert_eq!(number(" 12 ", 1), 12);
        assert_eq!(number("", 1), 1);
        assert_eq!(number("-", 1), 1);
        assert_eq!(number("0", 1), 0);
        // The numbered run's fields are signed, so a decrease is typeable.
        assert_eq!(integer("-2", 1), -2);
        assert_eq!(integer("", 1), 1);
    }

    #[test]
    fn the_mode_round_trips_through_its_index() {
        for mode in Mode::ALL {
            assert_eq!(Mode::from_index(mode.index()), mode);
        }
        for format in FormatChoice::ALL {
            assert_eq!(FormatChoice::from_index(format.index()), format);
        }
        // An index off the end is the first choice, not a panic.
        assert_eq!(Mode::from_index(9), Mode::Text);
        assert_eq!(FormatChoice::from_index(9), FormatChoice::Decimal);
    }
}
