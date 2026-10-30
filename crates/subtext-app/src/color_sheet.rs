//! The *Edit Color Code* panel.
//!
//! CotEditor opens the system colour panel with an accessory view bolted on: a
//! monospaced text field holding a colour code, a menu of the eleven notations
//! that code can be written in, and an *Insert* button. Picking a colour in the
//! panel rewrites the field, typing a code in the field recolours the panel,
//! and *Insert* puts the field's text at the caret.
//!
//! SubText has no system colour panel to attach to, so the panel and its
//! accessory are one dialog: the same field, the same notation menu, the same
//! button, with GPUI Kit's colour picker standing in for the wheel. The panel
//! is modal where upstream's is not — the dialog is the overlay the rest of
//! SubText uses — so the caret cannot be moved while it is open.
//!
//! # Where the state lives
//!
//! The notation is the only thing that outlives the panel: CotEditor keeps it
//! in `colorCodeType`, so reopening the panel offers the notation last used,
//! and [`crate::settings::Settings`] keeps it for the same reason. The colour
//! itself is not persisted — upstream keeps it in the shared panel for the
//! length of a session, and [`crate::workspace::Workspace`] does the same with
//! its `last_color_code_color` field.
//!
//! # Letter case
//!
//! Upstream keeps the case of the code it started from: a panel opened on
//! `#FF6600` writes `#FF6600` back rather than `#ff6600`. The flag is
//! recomputed from the field's text on every edit, so a code the user types in
//! upper case stays upper case for as long as the panel is open.

use gpui_kit::component::{
    ActiveTheme as _, IndexPath, Sizable as _, Size, WindowExt as _,
    button::{Button, ButtonVariants as _},
    color_picker::{ColorPickerEvent, ColorPickerState, ColorSelect},
    h_flex,
    input::{Input, InputState},
    select::{SearchableVec, Select, SelectEvent, SelectGroup, SelectState},
    v_flex,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, Hsla, IntoElement, ParentElement as _,
    Render, Styled as _, Subscription, WeakEntity, Window, div, hsla, rems,
};

use subtext_core::colorcode::{self, Color, ColorCodeType};

use crate::dialog_escape::DismissOnEscape as _;
use crate::workspace::Workspace;

/// The label column, wide enough for the longest label the panel uses.
const LABEL_COLUMN: f32 = 8.;

/// The notation menu's delegate: two named sections of labels.
type KindDelegate = SearchableVec<SelectGroup<&'static str>>;
type KindState = SelectState<KindDelegate>;
type KindEvent = SelectEvent<KindDelegate>;

/// The *Edit Color Code* panel, and everything it remembers while open.
pub struct ColorSheet {
    workspace: WeakEntity<Workspace>,
    /// The colour code as text. The panel's single source of truth: the picker
    /// is written from it, and reads back into it.
    code: Entity<InputState>,
    picker: Entity<ColorPickerState>,
    kind_state: Entity<KindState>,
    /// The notation the field's text is written in.
    kind: ColorCodeType,
    /// Whether the code the panel started from used upper case, so the hex
    /// notations can be written back the way the user had them.
    uppercase: bool,
    /// Kept alive so the subscriptions below last as long as the panel.
    _subscriptions: Vec<Subscription>,
}

impl ColorSheet {
    /// Build the panel for `selected`, the text the caret was on.
    ///
    /// `selected` seeds the field when it parses as a colour, which is what
    /// makes the panel open on the code that was selected rather than on a
    /// blank field. When it does not parse — or nothing was selected — the
    /// field opens on `fallback` in `kind`, the notation the last panel was
    /// closed with.
    pub fn new(
        workspace: WeakEntity<Workspace>,
        selected: &str,
        kind: ColorCodeType,
        fallback: Color,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // A selection that parses wins over the stored notation: the user is
        // pointing at a colour and has already said which one it is.
        let (kind, color, text) = match Color::from_code(selected) {
            Some((kind, color)) => (kind, color, selected.trim().to_string()),
            None => (kind, fallback, String::new()),
        };
        let uppercase = text.chars().any(|character| character.is_ascii_uppercase());

        let code = cx.new(|cx| InputState::new(window, cx).placeholder("Color Code"));
        let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(hsla_of(color)));
        let kind_state = cx.new(|cx| SelectState::new(kinds(), Some(index_of(kind)), window, cx));

        let mut sheet = Self {
            workspace,
            code,
            picker,
            kind_state,
            kind,
            uppercase,
            _subscriptions: Vec::new(),
        };
        // An empty field would leave the panel with nothing to insert, so it
        // opens holding the code for the colour it starts on. Upstream reaches
        // the same state a moment later, through the publisher that fires when
        // the panel is first observed.
        sheet.write_code(color, window, cx);
        sheet
            .code
            .update(cx, |state, cx| state.focus_handle(cx).focus(window, cx));

        let subscriptions = vec![
            cx.subscribe_in(&sheet.code.clone(), window, Self::on_code_change),
            cx.subscribe_in(&sheet.picker.clone(), window, Self::on_color_change),
            cx.subscribe_in(&sheet.kind_state.clone(), window, Self::on_kind_change),
        ];
        sheet._subscriptions = subscriptions;
        sheet
    }

    // ---- events ----------------------------------------------------------

    /// The field changed: adopt whatever colour it now spells.
    ///
    /// Half-typed codes do not parse, and that is fine — the picker keeps the
    /// colour it had until the text becomes a colour again, so typing
    /// `#336699` does not flash the swatch through every prefix.
    fn on_code_change(
        &mut self,
        _input: &Entity<InputState>,
        event: &gpui_kit::base::input::InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, gpui_kit::base::input::InputEvent::Change) {
            return;
        }
        let text = self.code.read(cx).value().to_string();
        // The case flag follows what the user typed, so a code entered in
        // capitals is written back in capitals.
        self.uppercase = text.chars().any(|character| character.is_ascii_uppercase());
        let Some((kind, color)) = Color::from_code(&text) else {
            return;
        };
        self.kind = kind;
        self.select_kind(kind, window, cx);
        self.picker.update(cx, |picker, cx| {
            picker.set_value(hsla_of(color), window, cx)
        });
        cx.notify();
    }

    /// The picker changed: rewrite the field in the current notation.
    fn on_color_change(
        &mut self,
        _picker: &Entity<ColorPickerState>,
        event: &ColorPickerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // `None` means the picker was cleared, which is not a colour the panel
        // can write a code for.
        let ColorPickerEvent::Change(Some(value)) = event else {
            return;
        };
        let color = color_from_hsla(*value);
        self.write_code(color, window, cx);
    }

    /// The notation menu was used: rewrite the field in the notation chosen.
    fn on_kind_change(
        &mut self,
        _state: &Entity<KindState>,
        event: &KindEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let KindEvent::Confirm(Some(label)) = event else {
            return;
        };
        let Some(kind) = kind_for_label(label) else {
            return;
        };
        self.kind = kind;
        // Read the colour from the field rather than from a cached copy: the
        // user may have typed a code and gone straight for the menu.
        if let Some(color) = self.current_color(cx) {
            self.write_code(color, window, cx);
        }
        cx.notify();
    }

    // ---- committing ------------------------------------------------------

    /// Put the field's code at the caret, if it is a code.
    ///
    /// Returns whether anything was inserted, so the caller can choose between
    /// closing the panel and leaving it open with an explanation.
    pub fn submit(&mut self, cx: &mut Context<Self>) -> bool {
        let text = self.code.read(cx).value().to_string();
        let Some((_, color)) = Color::from_code(&text) else {
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.notice(format!("{} is not a color code", text.trim()), cx);
                })
                .ok();
            return false;
        };
        let code = self.write_style(color);
        let kind = self.kind;
        self.workspace
            .update_in(cx, move |workspace, window, cx| {
                workspace.insert_color_code(&code, kind, color, window, cx);
            })
            .ok();
        true
    }

    // ---- the colour on screen --------------------------------------------

    /// The colour the panel is currently showing: the field's, when it parses,
    /// and otherwise the picker's.
    fn current_color(&self, cx: &App) -> Option<Color> {
        if let Some((_, color)) = Color::from_code(&self.code.read(cx).value()) {
            return Some(color);
        }
        self.picker.read(cx).value().map(color_from_hsla)
    }

    /// Rewrite the field from `color`, keeping the panel's letter case.
    fn write_code(&mut self, color: Color, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.write_style(color);
        self.code
            .update(cx, |state, cx| state.set_value(text, window, cx));
        cx.notify();
    }

    /// `color` written in the panel's notation.
    ///
    /// Two notations can refuse: a short hex needs every channel to be a
    /// multiple of 17, and a keyword needs one of the 148 named colours.
    /// Upstream answers both by falling back to `Hex` — and moving the menu
    /// with it, so the panel never claims a notation it is not showing.
    fn write_style(&mut self, color: Color) -> String {
        let mut kind = self.kind;
        let mut text = color.code(kind);
        if text.is_none() {
            kind = ColorCodeType::Hex;
            text = color.code(kind);
        }
        self.kind = kind;
        let mut text = text.unwrap_or_default();
        if kind.is_hex() && self.uppercase {
            text = text.to_uppercase();
        }
        text
    }

    /// Point the notation menu at `kind` without going through the event that
    /// would rewrite the field.
    fn select_kind(&mut self, kind: ColorCodeType, window: &mut Window, cx: &mut Context<Self>) {
        self.kind_state.update(cx, |state, cx| {
            state.set_selected_index(Some(index_of(kind)), window, cx);
        });
    }

    // ---- rendering -------------------------------------------------------

    fn render_code(&self, cx: &Context<Self>) -> impl IntoElement {
        // The code is monospaced: a colour code is a token, not prose, and the
        // digits of `#336699` line up under `#336699` only in a fixed pitch.
        v_flex().w_full().gap_1().child(
            div().w_full().child(
                Input::new(&self.code)
                    .id("color-code-field")
                    .font_family(cx.theme().mono_font_family.clone()),
            ),
        )
    }

    fn render_picker(&self) -> impl IntoElement {
        h_flex()
            .gap_2()
            .child(div().w(rems(LABEL_COLUMN)).text_sm().child("Color:"))
            .child(
                div()
                    .flex_1()
                    .child(ColorSelect::new(&self.picker).with_size(Size::Small)),
            )
    }

    fn render_kind(&self) -> impl IntoElement {
        h_flex()
            .gap_2()
            .child(div().w(rems(LABEL_COLUMN)).text_sm().child("Notation:"))
            .child(
                div().flex_1().child(
                    Select::new(&self.kind_state)
                        .with_size(Size::Small)
                        .accessibility_label("Notation"),
                ),
            )
    }

    fn render_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .justify_end()
            .child(
                Button::new("color-cancel")
                    .label("Cancel")
                    .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
            )
            .child(
                Button::new("color-insert")
                    .primary()
                    .label("Insert")
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.submit(cx) {
                            window.close_dialog(cx);
                        }
                    })),
            )
    }
}

impl Render for ColorSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .dismiss_on_escape()
            .gap_3()
            .child(self.render_code(cx))
            .child(self.render_picker())
            .child(self.render_kind())
            .child(self.render_buttons(cx))
    }
}

/// The notation menu's sections, as `(title, labels)`.
///
/// CotEditor splits the menu this way, and the split is not decoration — the
/// four hex notations are the ones whose letter case is preserved.
fn kind_sections() -> [(&'static str, Vec<&'static str>); 2] {
    [
        (
            "Hexadecimal",
            ColorCodeType::HEX_TYPES
                .iter()
                .map(|kind| kind.label())
                .collect(),
        ),
        (
            "CSS",
            ColorCodeType::CSS_TYPES
                .iter()
                .map(|kind| kind.label())
                .collect(),
        ),
    ]
}

/// The notation menu.
fn kinds() -> KindDelegate {
    let mut groups: KindDelegate = SearchableVec::new(Vec::new());
    for (title, labels) in kind_sections() {
        groups.push(SelectGroup::new(title).items(labels));
    }
    groups
}

/// Where `kind` sits in the menu built by [`kinds`].
fn index_of(kind: ColorCodeType) -> IndexPath {
    match ColorCodeType::HEX_TYPES
        .iter()
        .position(|candidate| *candidate == kind)
    {
        Some(row) => IndexPath::new(row).section(0),
        None => {
            let row = ColorCodeType::CSS_TYPES
                .iter()
                .position(|candidate| *candidate == kind)
                .unwrap_or(0);
            IndexPath::new(row).section(1)
        }
    }
}

/// The notation a menu label names.
fn kind_for_label(label: &str) -> Option<ColorCodeType> {
    ColorCodeType::ALL
        .into_iter()
        .find(|kind| kind.label() == label)
}

/// The panel's `Hsla`, from this crate's `Color`.
///
/// GPUI keeps the hue as a fraction of a turn, exactly as
/// [`subtext_core::colorcode`] does, so nothing is converted but the numbers'
/// width.
fn hsla_of(color: Color) -> Hsla {
    let (hue, saturation, lightness, alpha) = color.hsla();
    hsla(hue, saturation, lightness, alpha)
}

/// The colour a picker `Hsla` names.
///
/// The picker reports HSL, so the round trip has to go back through the HSL
/// conversion. Reading the three components as RGB would write a colour the
/// user never picked.
fn color_from_hsla(value: Hsla) -> Color {
    colorcode::from_hsl(
        f64::from(value.h),
        f64::from(value.s),
        f64::from(value.l),
        f64::from(value.a),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_lists_every_notation_once() {
        let sections = kind_sections();
        assert_eq!(sections.len(), 2);

        let mut labels: Vec<&'static str> = Vec::new();
        for (_, section) in sections {
            labels.extend(section);
        }
        assert_eq!(labels.len(), ColorCodeType::ALL.len());
        for kind in ColorCodeType::ALL {
            assert!(labels.contains(&kind.label()), "{kind:?} is in no section");
        }
    }

    #[test]
    fn a_label_round_trips_to_its_notation() {
        for kind in ColorCodeType::ALL {
            assert_eq!(kind_for_label(kind.label()), Some(kind));
        }
        assert_eq!(kind_for_label("Nonsense"), None);
    }

    #[test]
    fn a_notation_sits_in_the_section_that_holds_it() {
        for (index, kind) in ColorCodeType::HEX_TYPES.iter().enumerate() {
            let path = index_of(*kind);
            assert_eq!(path.section, 0, "{kind:?}");
            assert_eq!(path.row, index, "{kind:?}");
        }
        for (index, kind) in ColorCodeType::CSS_TYPES.iter().enumerate() {
            let path = index_of(*kind);
            assert_eq!(path.section, 1, "{kind:?}");
            assert_eq!(path.row, index, "{kind:?}");
        }
    }

    #[test]
    fn a_round_trip_through_the_picker_keeps_the_colour() {
        // The picker speaks HSL and the field speaks RGB, so every colour the
        // picker reports goes through `from_hsl` on the way back. A colour that
        // does not survive that is a colour the panel would silently change.
        for code in ["#336699", "#ffffff", "#000000", "#ff6600", "#123456"] {
            let (_, color) = Color::from_code(code).unwrap();
            let (hue, saturation, lightness, alpha) = color.hsla();
            let back = color_from_hsla(hsla(hue, saturation, lightness, alpha));
            // 8-bit precision is what the field writes, so that is what the
            // comparison is worth.
            assert_eq!(back.byte_components(), color.byte_components(), "{code}");
        }
    }
}
