//! The *Sort Lines by Pattern* sheet.
//!
//! CotEditor opens this as an `NSWindow` sheet: three sort keys — the whole
//! line, one field of a delimited record, or whatever a regular expression
//! matches — five switches, and a sample line whose sort key is highlighted so
//! the user can see what is about to be compared before committing. SubText
//! opens the same thing as a modal dialog against the main window.
//!
//! # Why the sheet owns its state
//!
//! None of it belongs to the document. The user can pick a key, type a pattern,
//! look at the preview, and press Cancel, and the document has to be exactly as
//! it was. So the state lives here, and only [`SortSheet::submit`] reaches
//! across to touch the text — through the workspace, so the sort lands in the
//! editor's own undo history like every other Text-menu command.
//!
//! # What is shown but not honoured
//!
//! *Respect language rules* asks for locale-aware collation. Rust's `str::cmp`
//! is not locale-aware and SubText will not grow an ICU dependency to pretend
//! otherwise, so the switch renders disabled with a note — the same principle
//! the settings window follows: a control that looks live but does nothing is
//! worse than one that admits it is inert.

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState, NumberInput},
    radio::RadioGroup,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, WeakEntity, Window, div, rems,
};

use subtext_core::textops::{SortKey, SortOptions};

use crate::workspace::Workspace;

/// Which sort key the sheet is on, in CotEditor's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyChoice {
    EntireLine,
    Column,
    RegularExpression,
}

impl KeyChoice {
    /// The radio group's order, which is also its index order.
    const ALL: [KeyChoice; 3] = [Self::EntireLine, Self::Column, Self::RegularExpression];

    fn label(self) -> &'static str {
        match self {
            KeyChoice::EntireLine => "Entire line",
            KeyChoice::Column => "Column",
            KeyChoice::RegularExpression => "Regular expression",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|choice| *choice == self).unwrap_or(0)
    }

    fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::EntireLine)
    }
}

/// The label column, wide enough for the longest label the sheet uses.
const LABEL_COLUMN: f32 = 8.;

/// The *Sort Lines by Pattern* sheet, and everything it remembers while open.
pub struct SortSheet {
    workspace: WeakEntity<Workspace>,
    /// The line the sort key is read from. CotEditor samples the line the
    /// caret is on, so the preview shows the user's own data rather than an
    /// invented example.
    sample: SharedString,
    key: KeyChoice,
    delimiter: Entity<InputState>,
    position: Entity<InputState>,
    pattern: Entity<InputState>,
    pattern_ignores_case: bool,
    uses_capture_group: bool,
    group: Entity<InputState>,
    options: SortOptions,
}

impl SortSheet {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        sample: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let delimiter = cx.new(|cx| InputState::new(window, cx).placeholder(","));
        // Both number fields start at 1: the first column, and capture group 1
        // — group 0 being the whole match, which the "use a group" switch is
        // there to opt into.
        let position = cx.new(|cx| InputState::new(window, cx).default_value("1").min(1.0));
        let pattern = cx.new(|cx| InputState::new(window, cx).placeholder("Regular Expression"));
        let group = cx.new(|cx| InputState::new(window, cx).default_value("1").min(0.0));

        Self {
            workspace,
            sample,
            // CotEditor's sheet opens on *Column*, not on the whole line.
            key: KeyChoice::Column,
            delimiter,
            position,
            pattern,
            pattern_ignores_case: true,
            uses_capture_group: false,
            group,
            options: SortOptions::default(),
        }
    }

    /// The sort key as the controls currently describe it.
    fn sort_key(&self, cx: &App) -> SortKey {
        match self.key {
            KeyChoice::EntireLine => SortKey::EntireLine,
            KeyChoice::Column => SortKey::Column {
                delimiter: self.delimiter.read(cx).value().to_string(),
                column: number(self.position.read(cx).value().as_ref(), 1),
            },
            KeyChoice::RegularExpression => SortKey::RegularExpression {
                pattern: self.pattern.read(cx).value().to_string(),
                ignore_case: self.pattern_ignores_case,
                group: self
                    .uses_capture_group
                    .then(|| number(self.group.read(cx).value().as_ref(), 1)),
            },
        }
    }

    /// Run the sort, if the key is usable.
    ///
    /// The check is repeated here rather than trusted from the button's
    /// disabled state, because Enter confirms the dialog without going near
    /// that button.
    pub fn submit(&mut self, cx: &mut Context<Self>) {
        let key = self.sort_key(cx);
        if key.validate().is_some() {
            return;
        }
        let options = self.options.clone();
        self.workspace
            .update_in(cx, move |workspace, window, cx| {
                workspace.sort_lines_by(&key, &options, window, cx);
            })
            .ok();
    }

    // ---- rendering -------------------------------------------------------

    /// The sample line, with the part the sort key will read tinted.
    fn render_sample(&self, key: &SortKey, cx: &Context<Self>) -> impl IntoElement {
        let sample = self.sample.to_string();
        let mut line = h_flex()
            .font_family(cx.theme().mono_font_family.clone())
            .text_sm()
            .truncate();

        match key.span_for(&sample) {
            Some(span) if !span.is_empty() => {
                line = line
                    .child(div().child(sample[..span.start].to_string()))
                    .child(
                        div()
                            .bg(cx.theme().primary.opacity(0.3))
                            .rounded_sm()
                            .child(sample[span.clone()].to_string()),
                    )
                    .child(div().child(sample[span.end..].to_string()));
            }
            // No span means the line has no key at all — a record too short
            // for the column, or a line the pattern does not match. Those
            // lines sort to the end, and there is nothing to point at.
            _ => line = line.child(div().child(sample)),
        }

        v_flex()
            .gap_1()
            .child(div().text_sm().child("Sample:"))
            .child(
                div()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(line),
            )
            .child(note(
                "The highlighted part is what the lines are compared on.",
                cx,
            ))
    }

    fn render_column(&self) -> impl IntoElement {
        h_flex()
            .gap_4()
            .child(
                h_flex()
                    .gap_2()
                    .child(div().text_sm().child("Delimiter:"))
                    .child(div().w(rems(5.)).child(Input::new(&self.delimiter).small())),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(div().text_sm().child("Position:"))
                    .child(div().w(rems(7.)).child(NumberInput::new(&self.position).small())),
            )
    }

    fn render_pattern(&self, error: Option<&'static str>, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(div().text_sm().child("Pattern:"))
                    .child(div().flex_1().child(Input::new(&self.pattern).small())),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(Checkbox::new("sort-pattern-case")
                        .checked(self.pattern_ignores_case)
                        .label("Ignore case")
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.pattern_ignores_case = *checked;
                            cx.notify();
                        }))),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(Checkbox::new("sort-capture-group")
                        .checked(self.uses_capture_group)
                        .label("Use captured group:")
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.uses_capture_group = *checked;
                            cx.notify();
                        })))
                    .child(
                        div()
                            .w(rems(7.))
                            .child(NumberInput::new(&self.group).small().disabled(!self.uses_capture_group)),
                    ),
            )
            .when_some(error, |this, message| {
                this.child(div().text_xs().text_color(cx.theme().danger).child(message))
            })
    }

    fn render_options(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(option_switch(
                "sort-ignore-case",
                "Ignore case",
                self.options.ignores_case,
                cx,
                |options, value| options.ignores_case = value,
            ))
            .child(option_switch(
                "sort-numeric",
                "Treat numbers as numeric values",
                self.options.numeric,
                cx,
                |options, value| options.numeric = value,
            ))
            .child(option_switch(
                "sort-keep-first",
                "Keep the first line at the top",
                self.options.keeps_first_line,
                cx,
                |options, value| options.keeps_first_line = value,
            ))
            .child(option_switch(
                "sort-descending",
                "In descending order",
                self.options.descending,
                cx,
                |options, value| options.descending = value,
            ))
            // CotEditor's fifth switch, rendered inert on purpose: locale
            // collation is the one part of its sort SubText cannot reproduce.
            .child(
                Checkbox::new("sort-localized")
                    .checked(false)
                    .disabled(true)
                    .label("Respect language rules")
                    .tooltip("Locale-aware collation needs an ICU dependency SubText does not carry"),
            )
    }

    fn render_buttons(&self, can_sort: bool, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .justify_end()
            .child(
                Button::new("sort-cancel")
                    .label("Cancel")
                    .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
            )
            .child(
                Button::new("sort-apply")
                    .primary()
                    .label("Sort")
                    .disabled(!can_sort)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit(cx);
                        window.close_dialog(cx);
                    })),
            )
    }
}

impl Render for SortSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let key = self.sort_key(cx);
        let error = key.validate().map(|error| error.message());

        let mut keys = v_flex().gap_2().child(
            RadioGroup::horizontal("sort-key")
                .selected_index(Some(self.key.index()))
                .children(KeyChoice::ALL.map(KeyChoice::label))
                .on_click(cx.listener(|this, index: &usize, _, cx| {
                    this.key = KeyChoice::from_index(*index);
                    cx.notify();
                })),
        );
        match self.key {
            KeyChoice::Column => keys = keys.child(self.render_column()),
            KeyChoice::RegularExpression => keys = keys.child(self.render_pattern(error, cx)),
            // The whole line has no sub-controls: the key is the line itself.
            KeyChoice::EntireLine => {}
        }

        v_flex()
            .gap_3()
            .child(self.render_sample(&key, cx))
            .child(field("Sort key:", keys))
            .child(field("Sort option:", self.render_options(cx)))
            .child(self.render_buttons(error.is_none(), cx))
    }
}

/// A label column beside a control, so the two panels line up.
fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap_2()
        .child(div().w(rems(LABEL_COLUMN)).text_sm().child(label))
        .child(div().flex_1().child(control))
}

/// A muted hint under a control.
fn note(text: &'static str, cx: &Context<SortSheet>) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

/// One of the sort switches.
///
/// `apply` is a function pointer rather than a closure so it stays `Copy` and
/// can be moved into the listener.
fn option_switch(
    id: &'static str,
    label: &'static str,
    checked: bool,
    cx: &mut Context<SortSheet>,
    apply: fn(&mut SortOptions, bool),
) -> impl IntoElement {
    Checkbox::new(id)
        .checked(checked)
        .label(label)
        .on_click(cx.listener(move |this, checked, _, cx| {
            apply(&mut this.options, *checked);
            cx.notify();
        }))
}

/// The number a field holds, or `fallback` when it holds something else.
///
/// The field is a `NumberInput`, so it usually holds digits; typing a stray
/// letter and pressing Sort should still sort rather than refuse.
fn number(value: &str, fallback: usize) -> usize {
    value.trim().parse().unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::number;

    #[test]
    fn a_number_field_falls_back_when_it_holds_nothing_useful() {
        assert_eq!(number("3", 1), 3);
        assert_eq!(number(" 12 ", 1), 12);
        assert_eq!(number("", 1), 1);
        assert_eq!(number("-", 1), 1);
        assert_eq!(number("0", 1), 0);
    }
}
