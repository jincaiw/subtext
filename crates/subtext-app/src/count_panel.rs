//! The *Advanced Character Count*: its options, the sheet that starts it, and
//! the capsule the running counter shows.
//!
//! `Text ▸ Advanced Character Count…` opens a sheet in CotEditor holding one
//! options panel and a *Start* button. Starting puts a small capsule over the
//! document, which shows the count and — behind a gear — the same options
//! panel again, so the count can be adjusted while watching it change.
//! Right-clicking the capsule offers *Copy* and *Stop Count*.
//!
//! # One options panel, two surfaces
//!
//! Upstream's `CharacterCountOptionsView` is drawn in both places, and this is
//! that one panel, so the two cannot drift apart. It is an entity rather than a
//! builder function because three of its controls are `Select`s, and a `Select`
//! keeps its state in an entity.
//!
//! # Where the state lives
//!
//! The seven options are CotEditor's `UserDefaults` keys and live in
//! [`crate::settings::Settings`], one field each. The count itself is *not*
//! state: it is derived from the buffer and the selection on every frame, which
//! is why the capsule keeps up while typing without anything telling it to.
//!
//! # Deliberate differences
//!
//! *The normalization menu offers all seven forms.* Upstream splits them into
//! `standardForms` and `modifiedForms`, and so does this menu — five, a
//! separator, two. NFKC Case-Fold used to be withheld because its case-fold
//! table was missing; the table is generated from Foundation now (see
//! `subtext_core::unicode_tables`), so every form listed is a form applied.

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IndexPath, Sizable as _, Size, WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    select::{SearchableVec, Select, SelectEvent, SelectState},
    v_flex,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, WeakEntity, Window, div, rems,
};

use subtext_core::charinfo::CharacterUnit;
use subtext_core::textops::Normalization;

use crate::settings::Settings;
use crate::workspace::Workspace;

/// The delegate every select in this panel uses: a flat list of labels.
///
/// A flat list rather than named groups, because the only menu upstream splits
/// into sections is the normalization form — and the section that survives is
/// the one upstream leaves unlabelled.
type OptionsDelegate = SearchableVec<&'static str>;
type OptionsState = SelectState<OptionsDelegate>;
type OptionsEvent = SelectEvent<OptionsDelegate>;

/// The options panel — upstream's `CharacterCountOptionsView`.
pub struct CountOptionsView {
    workspace: WeakEntity<Workspace>,
    unit: Entity<OptionsState>,
    normalization: Entity<OptionsState>,
    encoding: Entity<OptionsState>,
    /// Kept alive so the three subscriptions last as long as the panel.
    _subscriptions: Vec<Subscription>,
}

impl CountOptionsView {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        settings: &Settings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let unit = cx.new(|cx| {
            SelectState::new(
                units(),
                Some(index_of(&unit_labels(), settings.count_unit.label())),
                window,
                cx,
            )
        });
        let normalization = cx.new(|cx| {
            SelectState::new(
                normalizations(),
                Some(index_of(
                    &normalization_labels(),
                    settings.count_normalization_form.name(),
                )),
                window,
                cx,
            )
        });
        let encoding = cx.new(|cx| {
            SelectState::new(
                encodings(),
                Some(index_of(&encoding_labels(), &settings.count_encoding)),
                window,
                cx,
            )
        });

        let mut view = Self {
            workspace,
            unit: unit.clone(),
            normalization: normalization.clone(),
            encoding: encoding.clone(),
            _subscriptions: Vec::new(),
        };
        view._subscriptions = vec![
            cx.subscribe_in(&unit, window, Self::on_unit),
            cx.subscribe_in(&normalization, window, Self::on_normalization),
            cx.subscribe_in(&encoding, window, Self::on_encoding),
        ];
        view
    }

    fn on_unit(
        &mut self,
        _state: &Entity<OptionsState>,
        event: &OptionsEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let OptionsEvent::Confirm(Some(label)) = event else {
            return;
        };
        let Some(unit) = CharacterUnit::ALL
            .into_iter()
            .find(|unit| unit.label() == *label)
        else {
            return;
        };
        self.write(|settings| settings.count_unit = unit, cx);
    }

    fn on_normalization(
        &mut self,
        _state: &Entity<OptionsState>,
        event: &OptionsEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let OptionsEvent::Confirm(Some(label)) = event else {
            return;
        };
        let Some(form) = Normalization::ALL
            .into_iter()
            .find(|form| form.name() == *label)
        else {
            return;
        };
        self.write(|settings| settings.count_normalization_form = form, cx);
    }

    fn on_encoding(
        &mut self,
        _state: &Entity<OptionsState>,
        event: &OptionsEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let OptionsEvent::Confirm(Some(label)) = event else {
            return;
        };
        let Some(encoding) = subtext_core::encoding::AVAILABLE
            .iter()
            .find(|encoding| encoding.name == *label)
        else {
            return;
        };
        let identifier = encoding.identifier().to_string();
        self.write(|settings| settings.count_encoding = identifier, cx);
    }

    /// Apply `change` to the settings and save them.
    ///
    /// Every control goes through here, so there is one place that knows a
    /// change has to reach the file rather than only the field on screen.
    fn write(&mut self, change: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| {
                change(&mut workspace.settings);
                let _ = workspace.settings.save();
                cx.notify();
            })
            .ok();
        cx.notify();
    }
}

/// The label of each unit, in upstream's order.
fn unit_labels() -> Vec<&'static str> {
    CharacterUnit::ALL.iter().map(|unit| unit.label()).collect()
}

fn units() -> OptionsDelegate {
    SearchableVec::new(unit_labels())
}

/// The normalization forms, in upstream's order.
fn normalization_labels() -> Vec<&'static str> {
    Normalization::ALL.iter().map(|form| form.name()).collect()
}

fn normalizations() -> OptionsDelegate {
    SearchableVec::new(normalization_labels())
}

/// Every encoding SubText can write, which is what the byte unit can read.
fn encoding_labels() -> Vec<&'static str> {
    subtext_core::encoding::AVAILABLE
        .iter()
        .map(|encoding| encoding.name)
        .collect()
}

fn encodings() -> OptionsDelegate {
    SearchableVec::new(encoding_labels())
}

/// Where `label` sits among `labels`.
fn index_of(labels: &[&'static str], label: &str) -> IndexPath {
    let row = labels
        .iter()
        .position(|candidate| *candidate == label)
        .unwrap_or(0);
    IndexPath::new(row).section(0)
}

/// The whole options panel, ready to drop into a sheet or a popover.
impl Render for CountOptionsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self
            .workspace
            .upgrade()
            .map(|workspace| workspace.read(cx).settings.clone())
            .unwrap_or_default();
        // Upstream disables the collapsing checkbox only when there would be
        // nothing left to collapse.
        let collapsing_is_pointless =
            settings.count_ignores_newlines && settings.count_ignores_whitespaces;
        let unit = settings.count_unit;
        let byte_unit = unit == CharacterUnit::Byte;
        let grapheme_unit = unit == CharacterUnit::GraphemeCluster;

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(checkbox(
                        "count-ignore-newlines",
                        "Ignore line endings",
                        settings.count_ignores_newlines,
                        true,
                        {
                            let view = cx.entity().downgrade();
                            move |checked, cx| {
                                view.update(cx, |view, cx| {
                                    view.write(
                                        |settings| settings.count_ignores_newlines = checked,
                                        cx,
                                    );
                                })
                                .ok();
                            }
                        },
                    ))
                    .child(checkbox(
                        "count-ignore-whitespaces",
                        "Ignore whitespace",
                        settings.count_ignores_whitespaces,
                        true,
                        {
                            let view = cx.entity().downgrade();
                            move |checked, cx| {
                                view.update(cx, |view, cx| {
                                    view.write(
                                        |settings| settings.count_ignores_whitespaces = checked,
                                        cx,
                                    );
                                })
                                .ok();
                            }
                        },
                    ))
                    .child(checkbox(
                        "count-collapse-whitespace",
                        "Treat consecutive whitespace as one space",
                        settings.count_treats_consecutive_whitespace_as_single,
                        !collapsing_is_pointless,
                        {
                            let view = cx.entity().downgrade();
                            move |checked, cx| {
                                view.update(cx, |view, cx| {
                                    view.write(
                                        |settings| {
                                            settings
                                                .count_treats_consecutive_whitespace_as_single =
                                                checked;
                                        },
                                        cx,
                                    );
                                })
                                .ok();
                            }
                        },
                    )),
            )
            .child(field("Unit:", {
                let mut block = v_flex()
                    .gap_1()
                    .child(Select::new(&self.unit).with_size(Size::Small))
                    .child(
                        div()
                            .max_w(rems(24.))
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(unit.description()),
                    );
                if byte_unit {
                    block = block.child(field(
                        "Encoding:",
                        Select::new(&self.encoding).with_size(Size::Small),
                    ));
                }
                if !grapheme_unit {
                    block = block.child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(checkbox(
                                "count-normalizes",
                                "Normalization:",
                                settings.count_normalizes,
                                true,
                                {
                                    let view = cx.entity().downgrade();
                                    move |checked, cx| {
                                        view.update(cx, |view, cx| {
                                            view.write(
                                                |settings| settings.count_normalizes = checked,
                                                cx,
                                            );
                                        })
                                        .ok();
                                    }
                                },
                            ))
                            .child(
                                div().w(rems(12.)).child(
                                    Select::new(&self.normalization)
                                        .with_size(Size::Small)
                                        .disabled(!settings.count_normalizes),
                                ),
                            ),
                    );
                }
                block
            }))
    }
}

/// One labelled checkbox.
fn checkbox(
    id: &'static str,
    label: &'static str,
    checked: bool,
    enabled: bool,
    on_click: impl Fn(bool, &mut App) + 'static,
) -> impl IntoElement {
    Checkbox::new(id)
        .checked(checked)
        .label(label)
        .disabled(!enabled)
        .on_click(move |checked, _, cx| on_click(*checked, cx))
}

/// A label beside its control, so the panel's rows line up.
fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap_2()
        .child(div().flex_none().w(rems(6.)).text_sm().child(label))
        .child(div().flex_1().min_w_0().child(control))
}

/// The sheet that starts the counter — upstream's `CharacterCountOptionsSheetView`.
pub struct CountSheet {
    workspace: WeakEntity<Workspace>,
    options: Entity<CountOptionsView>,
}

impl CountSheet {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        options: Entity<CountOptionsView>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self { workspace, options }
    }

    /// Start the counter and close the sheet.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.start_advanced_count(window, cx)
            })
            .ok();
        window.close_dialog(cx);
    }
}

impl Render for CountSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_4().child(self.options.clone()).child(
            h_flex()
                .w_full()
                .gap_2()
                .justify_end()
                .child(
                    Button::new("count-cancel")
                        .label("Cancel")
                        .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
                )
                .child(
                    Button::new("count-start")
                        .primary()
                        .label("Start")
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                ),
        )
    }
}

/// The capsule's two halves: the number, and the word after it.
///
/// Upstream emphasises the number — monospaced digits at medium weight — and
/// leaves the unit in the secondary colour, so the capsule needs them apart.
/// The byte unit is spelled out as bytes and every other unit as
/// `character(s)`; `None` is upstream's *failed*, which only the byte unit can
/// reach.
pub fn count_parts(count: Option<usize>, unit: CharacterUnit) -> (String, String) {
    match count {
        None => (String::new(), "failed".to_string()),
        Some(count) => {
            let suffix = match unit {
                CharacterUnit::Byte => "bytes",
                _ => "character(s)",
            };
            (grouped_count(count), suffix.to_string())
        }
    }
}

/// `1234` → `1,234`, which is what `.formatted()` does on the platform.
pub fn grouped_count(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_byte_unit_is_spelled_out_and_the_rest_are_characters() {
        assert_eq!(
            count_parts(Some(12), CharacterUnit::Byte),
            ("12".into(), "bytes".into())
        );
        assert_eq!(
            count_parts(Some(12), CharacterUnit::Utf16),
            ("12".into(), "character(s)".into())
        );
        assert_eq!(
            count_parts(Some(12), CharacterUnit::GraphemeCluster),
            ("12".into(), "character(s)".into())
        );
        // A failed count has no number to show, only the word.
        assert_eq!(
            count_parts(None, CharacterUnit::Byte),
            (String::new(), "failed".into())
        );
    }

    #[test]
    fn the_count_is_grouped_like_the_platform_groups_it() {
        assert_eq!(grouped_count(0), "0");
        assert_eq!(grouped_count(999), "999");
        assert_eq!(grouped_count(1000), "1,000");
        assert_eq!(grouped_count(1_234_567), "1,234,567");
    }

    #[test]
    fn the_normalization_menu_offers_every_form_upstream_lists() {
        // All seven, in upstream's order — the five standard forms and the two
        // modified ones. NFKC Case-Fold used to be left out because the case
        // fold table was missing; the table is generated now, so there is
        // nothing left to leave out.
        let labels = normalization_labels();
        assert_eq!(
            labels,
            [
                "NFD",
                "NFC",
                "NFKD",
                "NFKC",
                "NFKC Case-Fold",
                "Modified NFD",
                "Modified NFC",
            ]
        );
    }

    #[test]
    fn every_unit_and_every_encoding_is_offered() {
        assert_eq!(unit_labels().len(), CharacterUnit::ALL.len());
        assert_eq!(
            encoding_labels().len(),
            subtext_core::encoding::AVAILABLE.len()
        );
        // The default is on the list, which is what the panel selects.
        assert!(index_of(&unit_labels(), CharacterUnit::GraphemeCluster.label()).row == 0);
    }
}
