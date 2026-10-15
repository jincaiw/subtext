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

use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Entity, Focusable as _, IntoElement, ParentElement as _, Render, Styled as _,
    WeakEntity, Window, div, px, rems,
};

use crate::workspace::Workspace;

/// How tall the two text fields are.
///
/// Upstream's `.frame(minHeight: 44)` — 44 points, which is 2.75 rem at the
/// default root size.
const FIELD_HEIGHT: f32 = 2.75;

/// How wide the result message may get before it truncates.
///
/// Upstream reserves exactly the message's own width as the text field's
/// trailing inset, so a long message does not run under the typed query. A
/// maximum is the same protection with a bound the layout can rely on.
const MESSAGE_MAX_WIDTH: f32 = 14.;

/// The **Find & Replace** panel.
pub struct FindPanel {
    workspace: WeakEntity<Workspace>,
}

impl FindPanel {
    pub fn new(workspace: WeakEntity<Workspace>, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { workspace }
    }

    /// Run a command on the document window.
    ///
    /// `None` while the document window has gone — the panel is a satellite of
    /// it, and every one of its buttons acts on the document.
    fn on_document(
        &self,
        cx: &mut Context<Self>,
        command: fn(&mut Workspace, &mut Window, &mut Context<Workspace>),
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
            .child(self.render_buttons(cx))
            .into_any_element()
    }
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
    use super::result_message;

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
