//! The *Custom…* surround sheet.
//!
//! `Text ▸ Surround Selection With ▸ Custom…` (⇧⌘L) opens a sheet in CotEditor
//! with two fields — what to put before the selection and what to put after it
//! — and a pair of buttons. Submitting wraps the selection in what they hold,
//! and the pair is remembered so the next sheet opens on it.
//!
//! # Where the state lives
//!
//! The pair outlives the sheet, in `UserDefaults` upstream and in
//! [`crate::settings::Settings`] here — the sheet's own fields are seeded from
//! it and written back on submit. The selection is not touched until then.
//!
//! # Two deliberate differences
//!
//! *An empty end field means "the same as the beginning"* — upstream's
//! `submit`, and the placeholder says so by showing whatever the begin field
//! currently holds. Upstream passes that as a `prompt:` at construction, which
//! would freeze it at the value the sheet opened with; here the field's
//! placeholder follows the begin field as it is typed.
//!
//! *An empty begin field cannot be submitted.* Upstream leaves the button live
//! and beeps; SubText disables it, which is the same rule with a visible
//! reason. Enter reaches the dialog's own action rather than the button, so
//! [`SurroundSheet::submit`] checks again.

use gpui_kit::component::{
    Disableable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, WeakEntity, Window, div, rems,
};

use crate::workspace::Workspace;

/// How wide the label column beside each field is.
const LABEL_COLUMN: f32 = 4.;

/// How wide each of the two fields is.
///
/// Upstream pins both to 48 points, which fits about four characters — enough
/// for the pairs people actually use (`**`, `<!--`, `"""`) and narrow enough
/// that the sheet stays a one-line decision.
const FIELD_WIDTH: f32 = 6.;

/// The *Custom…* surround sheet, and the two fields it is made of.
pub struct SurroundSheet {
    workspace: WeakEntity<Workspace>,
    begin: Entity<InputState>,
    end: Entity<InputState>,
    /// Kept alive so the begin field's subscription lasts as long as the sheet.
    _subscriptions: Vec<Subscription>,
}

impl SurroundSheet {
    /// Build the sheet on the pair last used, which may be two empty strings.
    pub fn new(
        workspace: WeakEntity<Workspace>,
        begin: &str,
        end: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let begin_state = cx.new(|cx| InputState::new(window, cx).default_value(begin));
        let end_state =
            cx.new(|cx| InputState::new(window, cx).default_value(end).placeholder(begin));

        let mut sheet = Self {
            workspace,
            begin: begin_state.clone(),
            end: end_state,
            _subscriptions: Vec::new(),
        };
        sheet._subscriptions = vec![cx.subscribe_in(&begin_state, window, Self::on_begin_change)];
        sheet
    }

    /// The end field's placeholder follows the begin field.
    fn on_begin_change(
        &mut self,
        _input: &Entity<InputState>,
        event: &gpui_kit::base::input::InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, gpui_kit::base::input::InputEvent::Change) {
            return;
        }
        let begin = self.begin.read(cx).value().to_string();
        self.end
            .update(cx, |state, cx| state.set_placeholder(begin, window, cx));
        cx.notify();
    }

    /// The pair as the fields hold it, or `None` while the begin field is
    /// empty — the one state the command refuses.
    fn pair(&self, cx: &App) -> Option<(String, String)> {
        pair_from(&self.begin.read(cx).value(), &self.end.read(cx).value())
    }

    /// Surround the selection and remember the pair.
    ///
    /// The check is repeated here rather than trusted from the button's
    /// disabled state, because Enter confirms the dialog without going near
    /// that button.
    pub fn submit(&mut self, cx: &mut Context<Self>) {
        let Some((begin, end)) = self.pair(cx) else {
            return;
        };
        self.workspace
            .update_in(cx, move |workspace, window, cx| {
                workspace.custom_surround(window, cx, &begin, &end);
            })
            .ok();
    }

    fn render_buttons(&self, can_submit: bool, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .justify_end()
            .child(
                Button::new("surround-cancel")
                    .label("Cancel")
                    .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
            )
            .child(
                Button::new("surround-apply")
                    .primary()
                    .label("OK")
                    .disabled(!can_submit)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit(cx);
                        window.close_dialog(cx);
                    })),
            )
    }
}

impl Render for SurroundSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let can_submit = self.pair(cx).is_some();
        v_flex()
            .gap_3()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Surround with:"),
            )
            .child(
                h_flex()
                    .items_start()
                    .gap_4()
                    .child(field("Begin:", Input::new(&self.begin).w(rems(FIELD_WIDTH))))
                    .child(field("End:", Input::new(&self.end).w(rems(FIELD_WIDTH)))),
            )
            .child(self.render_buttons(can_submit, cx))
    }
}

/// A label beside its field, so the two pairs line up.
fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_2()
        .child(div().w(rems(LABEL_COLUMN)).text_sm().child(label))
        .child(control)
}

/// The pair two field values describe, or `None` when they describe nothing.
///
/// Split out from [`SurroundSheet::pair`] so the one rule with a decision in it
/// can be checked without a window: an empty beginning is not a pair, and an
/// empty end means "the same as the beginning".
fn pair_from(begin: &str, end: &str) -> Option<(String, String)> {
    if begin.is_empty() {
        return None;
    }
    Some(if end.is_empty() {
        (begin.to_string(), begin.to_string())
    } else {
        (begin.to_string(), end.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_beginning_is_not_a_pair() {
        assert_eq!(pair_from("", ""), None);
        assert_eq!(pair_from("", "**"), None);
    }

    #[test]
    fn an_empty_end_means_the_same_as_the_beginning() {
        // The common case: the user types `"` once and gets `"…"`.
        assert_eq!(pair_from("\"", ""), Some(("\"".into(), "\"".into())));
        assert_eq!(pair_from("**", ""), Some(("**".into(), "**".into())));
        // Two different halves are kept as typed.
        assert_eq!(pair_from("<!--", "-->"), Some(("<!--".into(), "-->".into())));
        // Whitespace is a delimiter like any other — upstream only asks whether
        // the string is empty.
        assert_eq!(pair_from(" ", ""), Some((" ".into(), " ".into())));
    }
}
