//! The settings window.
//!
//! CotEditor puts its preferences in a separate window with nine panes on a
//! toolbar across the top, in this order: General, Appearance, Window, Edit,
//! Mode, Format, Snippets, Shortcuts and Integration. The open pane's name is
//! the window title. SubText keeps that shape — the same nine panes, the same
//! labels, and the same grouping of controls inside each pane — because a
//! preference the user knows by position should not move.
//!
//! The window holds a weak handle to the workspace rather than a copy of the
//! settings. A copy would be a second source of truth, and the two would
//! disagree the first time the menu bar changed something. A change made here
//! is therefore visible in the open documents as soon as it is made: there is
//! no Apply button in either application.
//!
//! ## What is shown but not honoured
//!
//! Several upstream switches drive AppKit behaviour that has no counterpart
//! here: spell checking, smart quotes and dashes, text replacement, services,
//! software updates. They are modelled in [`crate::settings`] and rendered —
//! so the pane reads the way the user expects — but the ones SubText cannot
//! act on are drawn disabled, next to a note saying why. A switch that looks
//! live but does nothing is worse than one that admits it is inert.

use std::rc::Rc;

use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, TitleBar,
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    dialog::{DialogAction, DialogClose, DialogFooter},
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    menu::{DropdownMenu as _, PopupMenuItem},
    radio::RadioGroup,
    slider::{Slider, SliderEvent, SliderState},
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, App, AppContext as _, Bounds, Context, ElementId, Entity, Focusable as _,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, PathBuilder, Pixels,
    Render, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, TextAlign, WeakEntity, Window, canvas, div, point, px, rems,
};

use subtext_core::filedrop::FileDropItem;
use subtext_core::snippets::Snippet;
use subtext_core::{FileEncoding, LineEnding};

use crate::encoding_list_sheet::EncodingListSheet;
use crate::fonts;
use crate::keymap;
use crate::localization::{self, Language};
use crate::settings::{
    AppearanceOptions, DocumentAppearance, DocumentConflictOption, EditOptions, FontType,
    ModeIndent, NoDocumentOnLaunchOption, Settings, SettingsPane, WindowTabbing, WritingDirection,
    clamp_window_alpha,
};
use crate::snippet_pane::{
    CommandColumn, FileDropColumn, FileDropRow, SnippetRow, Tab as SnippetTab, header, unique_name,
};
use crate::workspace::Workspace;

/// How wide the label column of a settings row is, in rem, per language.
///
/// CotEditor's `Form` gives the label column a share of the pane's width, sized
/// by its widest label; a constant can only approximate that, and the two
/// languages need different ones. The longest English label that sits in this
/// column — *When nothing else is open:* — is about 11.5 rem wide, so 12 rem is
/// its fit. The longest Chinese one — *自动换行缩进字符数：* — is about 8.5 rem.
/// Drawing the English width in Chinese spends 40 points on nothing and pushes
/// every control 40 points to the right; drawing the Chinese width in English
/// wraps that one label onto two lines.
///
/// The per-pane columns below ([`MODE_LABEL_COLUMN`],
/// [`APPEARANCE_LABEL_COLUMN`]) stay fixed because their panes are narrower for
/// both languages and their labels are short in both.
fn label_column() -> f32 {
    match localization::language() {
        Language::Chinese => 9.5,
        Language::English => 12.,
    }
}

/// The label column inside the Mode pane's options half.
///
/// That half is a column *beside* the mode list, so it has about 340 px to work
/// with — 12 rem of label column leaves 150 for the switches, and short labels
/// like *Smart copy/paste* wrapped onto two lines. Upstream's `Form` gives the
/// label a share of the width rather than a constant, which is the same idea
/// with a smaller number.
const MODE_LABEL_COLUMN: f32 = 7.;

/// The Appearance pane's label column, in rem.
///
/// This pane carries the window's widest control: the opacity row is a
/// 20-point sample, a slider, a second sample and a 64-point percent field.
/// Upstream caps that slider at 260 points, and with [`label_column`]'s 12 rem
/// (English) the 760-point settings window leaves only ~190 points for it. The
/// labels in this pane are short (the longest, *Monospaced font:*, is under 100
/// points), so a 7.5 rem column — still wider than any of them in either
/// language — hands the slider its full 260. Upstream's panes are separate views
/// with their own `Form`, so a per-pane column is what its layout does too.
const APPEARANCE_LABEL_COLUMN: f32 = 7.5;

/// How tall the Snippets pane's tab body is, in rem.
///
/// Upstream gives its `TabView` a fixed 380 points, which is what keeps the
/// table, the buttons and the insertion-format editor in the same places on
/// both tabs. The capsule of tabs straddles the top edge of the grey card, so
/// 11 points of this height are the part above that edge and the card itself
/// gets the remaining 369, matching the reference capture.
const SNIPPETS_TAB_HEIGHT: f32 = 23.75;

/// How tall the insertion-format editor is, in rem — upstream's 97.5 points.
const FORMAT_EDITOR_HEIGHT: f32 = 6.09375;

/// One row of the snippets table, in rem — upstream's 23.5 points.
///
/// The empty-state stripes use this fixed height rather than `flex_1`: inside
/// the auto-height scroll area a flexible child has nothing to divide, so the
/// stripes collapsed to zero and the empty table read as a blank field.
const SNIPPET_ROW_HEIGHT: f32 = 1.46875;

pub struct SettingsWindow {
    workspace: WeakEntity<Workspace>,
    pane: SettingsPane,
    /// Which tab of the Snippets pane is showing.
    snippet_tab: usize,
    /// The Command table's rows, in menu order.
    snippets: Vec<Entity<SnippetRow>>,
    snippet_selection: Option<u64>,
    /// The File Drop table's rows, in matching order.
    file_drop: Vec<Entity<FileDropRow>>,
    file_drop_selection: Option<u64>,
    /// The insertion-format editor of each tab.
    ///
    /// Upstream edits the selected item's format in one editor under the table
    /// rather than in the row, so each tab has one of these and the rows hold
    /// their formats as plain data.
    snippet_format: Entity<TextareaState>,
    file_drop_format: Entity<TextareaState>,
    /// Which row the format editor is currently holding.
    ///
    /// The editor reloads only when this changes, so typing does not fight the
    /// value the pane is about to push back into it. The two tables share the
    /// field because their row ids come from one counter and so cannot collide.
    format_owner: Option<u64>,
    /// The last shortcut refusal, which upstream shows beside the buttons.
    snippet_error: Option<String>,
    /// Row identity for both tables, monotonic for the life of the window.
    next_row_id: u64,
    /// The Window pane's two size fields.
    ///
    /// They are write-only: the pane pushes whatever they say into the
    /// settings when editing ends, and never writes the settings back into
    /// them. A round trip would move the caret while the user types.
    window_width: Entity<InputState>,
    window_height: Entity<InputState>,
    /// The Appearance pane's opacity slider and the percent field beside it.
    ///
    /// Two controls over one value, both live. The slider writes the setting as
    /// it moves so the document behind the window changes under the drag; the
    /// field writes it when editing ends. The field is rewritten only on
    /// release, which is why the two do not fight over the value mid-drag.
    opacity: Entity<SliderState>,
    opacity_percent: Entity<InputState>,
    /// The pane whose natural height the window was last fitted to, and that
    /// height.
    ///
    /// Upstream's settings window is not resizable: switching panes resizes it
    /// to the new pane's content instead. The height comes from the layout, so
    /// it is measured after painting and the window is resized a cycle later.
    /// Keeping the measurement here stops a short pane from being "fitted" up
    /// to the viewport it inherited from the tall pane before it.
    fit: Option<(SettingsPane, f32)>,
    _format_subscriptions: Vec<Subscription>,
    _window_size_subscriptions: Vec<Subscription>,
    _opacity_subscriptions: Vec<Subscription>,
}

impl SettingsWindow {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        pane: Option<SettingsPane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = workspace
            .read_with(cx, |workspace, _| workspace.settings.clone())
            .unwrap_or_default();

        // No placeholders: upstream's editor is an empty white field until an
        // item is selected, and its prompt is drawn by the pane, not by the
        // field.
        let snippet_format = cx.new(|cx| TextareaState::new(window, cx));
        let file_drop_format = cx.new(|cx| TextareaState::new(window, cx));
        let window_width = cx.new(|cx| window_size_input(settings.window.window_width, window, cx));
        let window_height =
            cx.new(|cx| window_size_input(settings.window.window_height, window, cx));
        // The document window clamps this value, so the pane has to start from
        // the clamped one too — otherwise a hand-written 5.0 shows as `500%`
        // while the window paints 100%.
        let window_alpha = clamp_window_alpha(settings.appearance.window_alpha);
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(*AppearanceOptions::WINDOW_ALPHA_RANGE.start())
                .max(*AppearanceOptions::WINDOW_ALPHA_RANGE.end())
                .step(0.01)
                .default_value(window_alpha)
        });
        let opacity_percent = cx.new(|cx| opacity_input(window_alpha, window, cx));

        let mut this = Self {
            workspace,
            pane: pane.unwrap_or_default(),
            snippet_tab: 0,
            snippets: Vec::new(),
            snippet_selection: None,
            file_drop: Vec::new(),
            file_drop_selection: None,
            snippet_format: snippet_format.clone(),
            file_drop_format: file_drop_format.clone(),
            format_owner: None,
            snippet_error: None,
            next_row_id: 1,
            window_width: window_width.clone(),
            window_height: window_height.clone(),
            opacity: opacity.clone(),
            opacity_percent: opacity_percent.clone(),
            fit: None,
            _format_subscriptions: Vec::new(),
            _window_size_subscriptions: Vec::new(),
            _opacity_subscriptions: Vec::new(),
        };

        for snippet in settings.snippets {
            let id = this.take_row_id();
            let pane = cx.weak_entity();
            let row = cx.new(|cx| SnippetRow::new(id, pane, snippet, window, cx));
            this.snippets.push(row);
        }
        for item in settings.file_drop {
            let id = this.take_row_id();
            let pane = cx.weak_entity();
            let row = cx.new(|cx| FileDropRow::new(id, pane, item, window, cx));
            this.file_drop.push(row);
        }

        // Upstream opens on the first row of each table, so the format editor
        // has something to show straight away.
        this.snippet_selection = this.snippets.first().map(|row| row.read(cx).id());
        this.file_drop_selection = this.file_drop.first().map(|row| row.read(cx).id());

        this._format_subscriptions = vec![
            cx.subscribe_in(&snippet_format, window, Self::on_format_change),
            cx.subscribe_in(&file_drop_format, window, Self::on_format_change),
        ];
        this._window_size_subscriptions = vec![
            cx.subscribe_in(&window_width, window, Self::on_window_size_change),
            cx.subscribe_in(&window_height, window, Self::on_window_size_change),
        ];
        this._opacity_subscriptions = vec![
            cx.subscribe_in(&opacity, window, Self::on_opacity_change),
            cx.subscribe_in(&opacity_percent, window, Self::on_opacity_percent),
        ];
        this.sync_rows(cx);
        this
    }

    /// Move to `pane`, which is what *Edit Snippets…* asks for.
    pub fn show(&mut self, pane: SettingsPane, cx: &mut Context<Self>) {
        self.pane = pane;
        cx.notify();
    }

    /// Resize the window to the pane that was just measured.
    ///
    /// Upstream's window has `[.closable, .titled]` and no resize grip: it
    /// takes the selected pane's content height and keeps its top-left corner.
    /// The height is only known after layout, so this runs from the paint hook
    /// and applies the resize on the next cycle. Panes taller than the screen
    /// are clamped to the display's visible area and scroll, which is why the
    /// scroll owner stays.
    fn on_pane_measured(
        &mut self,
        children: &[Bounds<Pixels>],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane_height) = children
            .first()
            .map(|bounds| bounds.size.height.as_f32())
            .filter(|height| *height > 0.)
        else {
            return;
        };

        // The hook runs every frame. Repaint only when the measurement
        // actually moves, or a rounding jitter would resize forever.
        if let Some((pane, height)) = self.fit
            && pane == self.pane
            && (height - pane_height).abs() <= 0.5
        {
            return;
        }
        self.fit = Some((self.pane, pane_height));

        /// The self-drawn title and toolbar rows.
        const CHROME_HEIGHT: f32 = 88.;
        /// The smallest the window may get, matching `window_min_size`.
        const MIN_HEIGHT: f32 = 480.;

        let mut target = (pane_height + CHROME_HEIGHT).max(MIN_HEIGHT);
        if let Some(display) = cx.primary_display() {
            let visible = display.visible_bounds().size.height.as_f32();
            target = target.min(visible.max(MIN_HEIGHT));
        }

        let current = window.bounds().size.height.as_f32();
        if (current - target).abs() <= 0.5 {
            return;
        }
        let width = window.bounds().size.width;
        window.defer(cx, move |window, _| {
            window.resize(gpui_kit::size(width, px(target)));
        });
    }

    /// The settings as they stand, read through the workspace.
    fn settings(&self, cx: &App) -> Settings {
        self.workspace
            .read_with(cx, |workspace, _| workspace.settings.clone())
            .unwrap_or_default()
    }

    /// Run `apply` on the workspace, in the document window it is drawn in.
    ///
    /// The settings window cannot reach the workspace through
    /// `WeakEntity::update_in`: that resolves an entity's window from the last
    /// window that *read* it, which is this one while a pane renders, and a
    /// window refuses its own handle while it dispatches the event that led
    /// here. The update would be dropped without an error the user can see.
    /// The workspace keeps its own window handle, so the update is addressed
    /// to the document window instead.
    fn edit_workspace(
        &self,
        cx: &mut App,
        apply: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    ) -> bool {
        Workspace::update_in_own_window(&self.workspace, cx, apply)
    }

    /// Run `apply` against the workspace's settings and re-apply them.
    ///
    /// Every pane writes through here, so a control cannot change the model
    /// without the open documents hearing about it.
    fn edit(&self, cx: &mut Context<Self>, apply: impl FnOnce(&mut Settings) + 'static) {
        let mut apply = Some(apply);
        if self.edit_workspace(cx, move |workspace, window, cx| {
            if let Some(apply) = apply.take() {
                apply(&mut workspace.settings);
            }
            workspace.apply_settings(window, cx);
        }) {
            // This window reads the settings through the workspace. Refresh it
            // after a write so controlled controls (notably the default
            // encoding picker) do not wait for an unrelated redraw.
            cx.notify();
        }
    }

    /// Commit the Window pane's size fields when editing ends.
    ///
    /// Upstream's text fields apply on end editing, and so do these: half a
    /// number is not a window width. Whatever the field cannot parse is
    /// replaced by the value the settings still hold, so the field never keeps
    /// text that the next window would not honour.
    fn on_window_size_change(
        &mut self,
        _input: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
            return;
        }
        // Each field commits on its own: a bad height must not throw away a
        // width the user just typed.
        let width = self.window_width.read(cx).value().to_string();
        if let Some(width) = parse_window_size(&width) {
            self.edit(cx, move |settings| settings.window.window_width = width);
        }
        let height = self.window_height.read(cx).value().to_string();
        if let Some(height) = parse_window_size(&height) {
            self.edit(cx, move |settings| settings.window.window_height = height);
        }
        // Show the value that is now in effect — the parsed number, or nothing
        // when the setting is "Auto".
        let current = self.settings(cx).window;
        let width = window_size_text(current.window_width);
        let height = window_size_text(current.window_height);
        self.window_width
            .update(cx, |state, cx| state.set_value(width, window, cx));
        self.window_height
            .update(cx, |state, cx| state.set_value(height, window, cx));
    }

    /// Follow the opacity slider.
    ///
    /// The slider and the percent field are two controls over one value, so
    /// each has to write the other without writing itself back: `set_value` on
    /// either is a plain assignment that raises no event, which is what keeps
    /// this from looping.
    fn on_opacity_change(
        &mut self,
        _slider: &Entity<SliderState>,
        event: &SliderEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (value, released) = match event {
            SliderEvent::Change(value) => (value.start(), false),
            SliderEvent::Release(value) => (value.start(), true),
        };
        let alpha = clamp_window_alpha(value);
        self.edit(cx, move |settings| settings.appearance.window_alpha = alpha);
        // While the field has focus the user is typing into it; moving the
        // slider underneath them would erase the keystrokes. On release the
        // field follows whatever the user settled on.
        if released
            || !self
                .opacity_percent
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        {
            self.write_percent(alpha, window, cx);
        }
    }

    /// Commit the percent field when editing ends.
    ///
    /// Like the Window pane's size fields, half a number is not a value: the
    /// field applies on end editing, and text it cannot parse is replaced by the
    /// opacity the settings still hold.
    fn on_opacity_percent(
        &mut self,
        _input: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
            return;
        }
        let current = self.settings(cx).appearance.window_alpha;
        let text = self.opacity_percent.read(cx).value().to_string();
        let alpha = parse_percent(&text).map_or(current, clamp_window_alpha);
        self.edit(cx, move |settings| settings.appearance.window_alpha = alpha);
        // Carry the committed value to the slider first; `write_percent` then
        // normalises the field's own text (a bare `55` becomes `55%`).
        self.opacity
            .update(cx, |state, cx| state.set_value(alpha, window, cx));
        self.write_percent(alpha, window, cx);
    }

    /// Put `alpha` in the percent field without raising a change event.
    fn write_percent(&self, alpha: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.opacity_percent.update(cx, |state, cx| {
            state.set_value(opacity_text(alpha), window, cx)
        });
    }

    /// Store the family chosen for one of the two font rows.
    fn set_font_family(&self, key: &'static str, family: Option<String>, cx: &mut Context<Self>) {
        self.edit(cx, move |settings| {
            if key == "standard" {
                settings.appearance.standard_font_family = family;
            } else {
                settings.appearance.monospaced_font_family = family;
            }
        });
    }

    /// Take a family from the *Select…* menu.
    ///
    /// Upstream asks before it keeps a proportional family in the monospaced
    /// row. The choice is applied first — the sample has to follow the click —
    /// and Cancel is what puts the previous family back, so dismissing the alert
    /// is the "no" rather than the "maybe".
    fn choose_font(
        &mut self,
        key: &'static str,
        family: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let appearance = self.settings(cx).appearance;
        let previous = if key == "standard" {
            appearance.standard_font_family
        } else {
            appearance.monospaced_font_family
        };
        if previous
            .as_deref()
            .is_some_and(|previous| previous.eq_ignore_ascii_case(&family))
        {
            return;
        }

        let warn = key == "monospaced" && !fonts::is_monospaced(cx, &family);
        self.set_font_family(key, Some(family), cx);
        if !warn {
            return;
        }

        let pane = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let pane = pane.clone();
            let previous = previous.clone();
            alert
                .title(SharedString::from(localization::t(
                    "The selected font doesn’t seem to be monospaced.",
                )))
                .description(SharedString::from(localization::t(
                    "Do you want to use it for the monospaced font?",
                )))
                .show_cancel(true)
                .on_ok(|_, _, _| true)
                .on_cancel(move |_, _, cx| {
                    pane.update(cx, |this, cx| {
                        this.set_font_family(key, previous.clone(), cx);
                    })
                    .ok();
                    true
                })
        });
    }

    /// Step one font row's size, clamped where upstream clamps it: 1 to 100.
    fn adjust_font_size(&self, key: &'static str, delta: f32, cx: &mut Context<Self>) {
        self.edit(cx, move |settings| {
            let size = if key == "standard" {
                &mut settings.appearance.standard_font_size
            } else {
                &mut settings.appearance.monospaced_font_size
            };
            *size = (*size + delta).clamp(1.0, 100.0);
        });
    }

    /// Switch the interface language.
    ///
    /// This is not `edit`: changing the language has to rebuild the menu bar
    /// and repaint every window, and `apply_settings` does neither. The order
    /// matters — the language is stored first so the pass below reads it, then
    /// the settings are saved, then the menu is rebuilt in the new language.
    fn choose_language(&mut self, language: Language, cx: &mut Context<Self>) {
        if self.settings(cx).language == language {
            return;
        }
        localization::set_language(language);
        self.edit_workspace(cx, move |workspace, window, cx| {
            workspace.settings.language = language;
            workspace.apply_settings(window, cx);
            workspace.rebuild_menus(cx);
            // The panes, the sheets and every open document draw strings
            // through `t()`, and none of them is subscribed to the
            // language: the whole window tree is repainted instead.
            cx.refresh_windows();
        });
        cx.notify();
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Upstream names the open pane in the titlebar, so the pane does not
        // repeat its own name in a heading above the controls. The title is the
        // one place the name sits, and it changes as the toolbar selection does.
        // This window paints that titlebar itself (see the root below), so the
        // call here is for the system rather than the screen: the Window menu,
        // Mission Control and assistive technology read the name even though
        // the native title is not shown.
        window.set_window_title(localization::t(self.pane.title()));
        let settings = self.settings(cx);

        // `title_bar`, not `background`: this window is AppKit chrome, not the
        // editor. `project_theme` fills this token with
        // `NSColor.windowBackgroundColor` — white in the light appearance,
        // #1E1E1E in the dark one — which is the surface upstream's settings
        // panes sit on. `background` carries the syntax theme's paper (#fafafa
        // for Anura), and would tint the preferences window to the editor's
        // colour, which upstream never does.
        let surface = cx.theme().title_bar;
        let border = cx.theme().border;
        let foreground = cx.theme().foreground;
        let muted_foreground = cx.theme().muted_foreground;

        // Upstream's preference toolbar: nine cells, icon over label, centred.
        // The selected cell is a raised white pill rather than a tinted row —
        // it is the one piece of this window the user reads as a position
        // rather than as a button — and its glyph takes CotEditor's accent
        // green, which is the same `primary` the rest of the chrome uses.
        //
        // Not a `Button`: the component always lays icon and label out in a
        // row, and this strip needs them stacked. The div carries its own
        // accessible label so assistive technology still names each cell.
        //
        // The title stays English in [`crate::settings`] because it names the
        // pane as well as labelling it; it is translated here, where it is
        // drawn.
        let accent = cx.theme().primary;
        let toolbar = SettingsPane::ALL
            .into_iter()
            .map(|pane| {
                let selected = self.pane == pane;
                let tint = if selected { accent } else { muted_foreground };
                v_flex()
                    .id(SharedString::from(format!("settings-tab-{}", pane.title())))
                    .flex_none()
                    .w(px(56.))
                    .h(px(48.))
                    .items_center()
                    .justify_center()
                    .gap_0p5()
                    .rounded_md()
                    .when(selected, |this| {
                        this.bg(cx.theme().table)
                            .border_1()
                            .border_color(cx.theme().border)
                            .shadow_sm()
                    })
                    .when(!selected, |this| {
                        this.hover(|this| this.bg(cx.theme().accent))
                    })
                    .aria_label(format!("{} settings", localization::t(pane.title())))
                    .child(
                        Icon::new(icon_for(pane))
                            .with_size(px(24.))
                            .text_color(tint),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(tint)
                            .child(localization::t(pane.title())),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pane = pane;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        // The title row and the tab row, centred over the window. `TitleBar`
        // indents its children past the traffic lights — 80 points on macOS,
        // 12 where the window manager draws its own controls — so the column
        // is pulled back by exactly that much and given the window's width.
        let titlebar_indent = if cfg!(target_os = "macos") {
            px(80.)
        } else {
            px(12.)
        };
        let chrome = v_flex()
            .flex_none()
            .h_full()
            .ml(-titlebar_indent)
            .w(window.viewport_size().width)
            .child(
                h_flex()
                    .w_full()
                    .h(px(32.))
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .child(localization::t(self.pane.title())),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .h(px(56.))
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .children(toolbar),
            );

        let body = match self.pane {
            SettingsPane::General => self.render_general(&settings, cx),
            SettingsPane::Appearance => self.render_appearance(&settings, cx),
            SettingsPane::Window => self.render_window(&settings, cx),
            SettingsPane::Edit => self.render_edit(&settings, cx),
            SettingsPane::Mode => self.render_mode(&settings, cx),
            SettingsPane::Format => self.render_format(&settings, cx),
            SettingsPane::Snippets => self.render_snippets(window, cx),
            SettingsPane::Shortcuts => self.render_shortcuts(cx),
            SettingsPane::Integration => self.render_integration(&settings, cx),
        };

        // The prepaint hook reports the pane's laid-out height, but it cannot
        // take `cx.listener`: that passes the event by reference, and the hook
        // hands it over by value. A weak handle lets the hook update the fit
        // cache without capturing `self`.
        let measured = cx.entity().downgrade();

        v_flex()
            .size_full()
            .bg(surface)
            .text_color(foreground)
            .child(
                // Upstream draws both the title and the toolbar itself: the
                // pane's name is centred above the nine cells, and the two
                // rows together are the window's 88 points of chrome. A native
                // titlebar cannot do that — macOS left-aligns its title beside
                // the traffic lights — so this window, like the editor, hands
                // its titlebar to `TitleBar` and paints the chrome inside the
                // content view (`TitleBar::window_options()`).
                //
                // `TitleBar` indents its children by the width the traffic
                // lights occupy and lays them out in the strip after them. The
                // chrome is centred on the *window*, though, so the column is
                // pulled back over that indent and given the viewport's full
                // width. AppKit draws the traffic lights above the content, and
                // the title sits far from them, so nothing overlaps.
                TitleBar::new()
                    .h(px(88.))
                    .bg(surface)
                    .border_color(border)
                    .child(chrome),
            )
            .child(
                // The scroll owner is the pane itself, so its scrollbar sits
                // against the window's trailing edge; the inset belongs to the
                // column *inside* it. Padding on the scroll owner would push the
                // scrollbar inwards, leaving it floating between the content and
                // the edge of the surface that scrolls.
                div()
                    // `on_children_prepainted` lives on `Div` itself: the
                    // `Stateful` returned by `.id()` does not forward it.
                    .on_children_prepainted(move |children, window, cx| {
                        let Some(settings) = measured.upgrade() else {
                            return;
                        };
                        settings.update(cx, |this, cx| {
                            Self::on_pane_measured(this, &children, window, cx)
                        });
                    })
                    .id(SharedString::from(format!(
                        "settings-pane-{}",
                        self.pane.title()
                    )))
                    .w_full()
                    .flex_1()
                    .min_h(rems(0.))
                    // Column, not the default row: a row stretches its single
                    // child to the viewport's height, which would report the
                    // window's height as the pane's natural height and never
                    // shrink. In a column the padded pane keeps its own height.
                    .flex_col()
                    .overflow_y_scroll()
                    // 20 points, upstream's `scenePadding()`, not the 16 a
                    // `p_4` would give: the group boxes inside each pane are
                    // measured from that inset.
                    .child(v_flex().w_full().p_5().gap_4().child(body)),
            )
    }
}

impl SettingsWindow {
    // ---- General ---------------------------------------------------------

    /// The General pane, in `GeneralSettingsView.swift`'s order.
    fn render_general(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let general = settings.general;

        // The interface language, first because it decides what every other
        // row in this window says. CotEditor follows the system language and
        // has no such row; SubText defaults to Chinese and has to let the
        // reader leave.
        //
        // The two names are drawn in their own language and are deliberately
        // not translated: an English reader looking for English should not
        // have to find it spelled 英文.
        let language = radio_row(
            "interface-language",
            &Language::ALL.map(Language::label),
            Language::ALL
                .iter()
                .position(|language| *language == settings.language)
                .unwrap_or(0),
            cx.listener(|this, index: &usize, _, cx| {
                let language = Language::ALL[*index];
                this.choose_language(language, cx);
            }),
        );

        let startup = v_flex()
            .gap_2()
            .child(
                Checkbox::new("quit-always-keeps-windows")
                    .checked(general.quit_always_keeps_windows)
                    .label(localization::t("Reopen windows from last session"))
                    .on_click(cx.listener(|this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| {
                            settings.general.quit_always_keeps_windows = checked
                        });
                    })),
            )
            // Upstream puts the launch picker on its own line under this
            // label, indented 20 pt. SubText draws radios where CotEditor draws
            // a menu — the same equivalence the rest of this window uses — and
            // they need the full control column, so the label sits above them.
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .child(localization::t("When nothing else is open:")),
                    )
                    .child(
                        div().pl_5().child(radio_row(
                            "no-document-on-launch",
                            &NoDocumentOnLaunchOption::ALL
                                .map(|option| localization::t(option.label())),
                            general.no_document_on_launch_option as usize,
                            cx.listener(|this, index: &usize, _, cx| {
                                let option = NoDocumentOnLaunchOption::ALL[*index];
                                this.edit(cx, move |settings| {
                                    settings.general.no_document_on_launch_option = option
                                });
                            }),
                        )),
                    ),
            );

        let saving = v_flex()
            .gap_1()
            .child(
                Checkbox::new("enables-autosave-in-place")
                    .checked(general.enables_autosave_in_place)
                    .label(localization::t("Enable Auto Save with Versions"))
                    .on_click(cx.listener(|this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| {
                            settings.general.enables_autosave_in_place = checked
                        });
                    })),
            )
            // Upstream's note describes autosaving and version history, which
            // is a system feature (`NSDocument` + NSDocumentController's
            // versions menu). The toggle is stored like every other preference,
            // but nothing reads it yet, so the sentence above it has to say that
            // rather than promise a Revert To item that lists one version.
            .child(div().pl_5().child(note(
                localization::t(
                    "Not implemented yet. SubText writes a file only when you save, and \
                         keeps no earlier versions to revert to.",
                ),
                cx,
            )));

        // Upstream spans this label across both grid columns and leaves the
        // first cell of the next row empty, so the radios line up with the
        // controls above them. The spacer is that empty cell.
        let conflicts = v_flex()
            .gap_2()
            .child(div().text_sm().child(localization::t(
                "When document is changed by another application:",
            )))
            .child(
                h_flex()
                    .items_start()
                    .gap_4()
                    .child(div().w(rems(label_column())).flex_shrink_0())
                    .child(div().flex_1().min_w_0().child(radio_column(
                        "document-conflict",
                        &DocumentConflictOption::ALL.map(|option| localization::t(option.label())),
                        general.document_conflict_option as usize,
                        cx.listener(|this, index: &usize, _, cx| {
                            let option = DocumentConflictOption::ALL[*index];
                            this.edit(cx, move |settings| {
                                settings.general.document_conflict_option = option
                            });
                        }),
                    ))),
            );

        let transfers = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("settings-export")
                            .small()
                            .label(localization::t("Export…"))
                            .disabled(true),
                    )
                    .child(
                        Button::new("settings-import")
                            .small()
                            .label(localization::t("Import…"))
                            .disabled(true),
                    ),
            )
            .child(note(localization::t("Not implemented yet. The settings file can be edited directly, at the path below."),
                cx,
            ))
            .child(note(Settings::path().display().to_string(), cx));

        let warnings = v_flex()
            .gap_1()
            // In a column the cross axis stretches, so a button here fills the
            // row; the `h_flex` is what keeps it the width of its label.
            .child(
                h_flex().child(
                    Button::new("manage-warnings")
                        .small()
                        .label(localization::t("Manage Warnings…"))
                        .disabled(true),
                ),
            )
            .child(note(
                localization::t(
                    "Not implemented yet. CotEditor lets you switch off each dialog it can show; \
                     SubText shows no such dialogs.",
                ),
                cx,
            ));

        let updates = v_flex()
            .gap_1()
            .child(
                Checkbox::new("automatic-update-checks")
                    .checked(general.enable_automatic_update_checks)
                    .label(localization::t("Check for updates automatically"))
                    .on_click(cx.listener(|this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| {
                            settings.general.enable_automatic_update_checks = checked
                        });
                    })),
            )
            .child(
                Checkbox::new("checks-updates-for-beta")
                    .checked(general.checks_updates_for_beta)
                    .disabled(!general.enable_automatic_update_checks)
                    .label(localization::t("Update to prereleases when available"))
                    .on_click(cx.listener(|this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| {
                            settings.general.checks_updates_for_beta = checked
                        });
                    })),
            );

        // One `Grid`: a right-aligned label column and a control column, the
        // same shape the Edit and Mode panes already use. CotEditor's General
        // pane is that grid, not a stack of headed sections.
        v_flex()
            .gap_4()
            .child(field(localization::t("Interface language:"), language))
            .child(field(localization::t("On startup:"), startup))
            .child(field(localization::t("Document save:"), saving))
            .child(conflicts)
            .child(field(
                localization::t("Settings import & export:"),
                transfers,
            ))
            .child(field(localization::t("Dialog warnings:"), warnings))
            // Upstream puts a rule above the updater, which is a separate view
            // rather than another setting of this pane.
            .child(divider(cx))
            .child(field(localization::t("Software update:"), updates))
            .child(self.help_link_row())
            .into_any_element()
    }

    // ---- Appearance ------------------------------------------------------

    /// The Appearance pane, in `AppearanceSettingsView.swift`'s order.
    fn render_appearance(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let appearance = settings.appearance.clone();

        let standard = self.render_font(
            "standard",
            appearance.standard_font_family.clone(),
            appearance.standard_font_size,
            appearance.standard_antialias,
            appearance.standard_ligature,
            cx,
        );
        let monospaced = self.render_font(
            "monospaced",
            appearance.monospaced_font_family.clone(),
            appearance.monospaced_font_size,
            appearance.monospaced_antialias,
            appearance.monospaced_ligature,
            cx,
        );

        let line_height = stepper(
            "line-height",
            format!("{:.1}", appearance.line_height),
            cx,
            |this, cx| {
                this.edit(cx, |settings| {
                    let value = (settings.appearance.line_height - 0.1).clamp(0.1, 10.0);
                    settings.appearance.line_height = (value * 10.0).round() / 10.0;
                })
            },
            |this, cx| {
                this.edit(cx, |settings| {
                    let value = (settings.appearance.line_height + 0.1).clamp(0.1, 10.0);
                    settings.appearance.line_height = (value * 10.0).round() / 10.0;
                })
            },
        );

        let appearance_mode = radio_row(
            "document-appearance",
            &DocumentAppearance::ALL.map(|mode| localization::t(mode.label())),
            DocumentAppearance::ALL
                .iter()
                .position(|mode| *mode == appearance.document_appearance)
                .unwrap_or(0),
            cx.listener(|this, index: &usize, _, cx| {
                let mode = DocumentAppearance::ALL[*index];
                this.edit(cx, move |settings| {
                    settings.appearance.document_appearance = mode
                });
            }),
        );

        let status_bar = radio_row(
            "status-bar-background",
            &[localization::t("Tinted"), localization::t("Opaque")],
            usize::from(settings.window.prefers_opaque_bar_background),
            cx.listener(|this, index: &usize, _, cx| {
                let opaque = *index == 1;
                this.edit(cx, move |settings| {
                    settings.window.prefers_opaque_bar_background = opaque
                });
            }),
        );

        // Upstream's `OpacityView`: a 20-point sample at each end of a slider
        // 260 points wide, then the value as a percent field. The samples are
        // the fill of the window at its minimum and maximum, so the row says
        // what the slider does before it is touched.
        let opacity = h_flex()
            .gap_3()
            .items_center()
            .child(opacity_sample(
                *AppearanceOptions::WINDOW_ALPHA_RANGE.start(),
                localization::t("Transparent"),
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .max_w(px(260.))
                    .child(Slider::new(&self.opacity).horizontal()),
            )
            .child(opacity_sample(
                *AppearanceOptions::WINDOW_ALPHA_RANGE.end(),
                localization::t("Opaque"),
                cx,
            ))
            .child(
                Input::new(&self.opacity_percent)
                    .small()
                    .w(px(64.))
                    .text_align(TextAlign::Right),
            );

        let themes = self
            .workspace
            .read_with(cx, |workspace, _| workspace.theme_names())
            .unwrap_or_default();
        let current = settings
            .theme
            .clone()
            .and_then(|name| themes.iter().position(|theme| *theme == name));
        let theme_items = themes
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>();
        let theme_list = pick_list(
            "theme-list",
            theme_items,
            current,
            cx,
            cx.listener(|this, index: &usize, _, cx| {
                let name = this
                    .workspace
                    .read_with(cx, |workspace, _| {
                        workspace.theme_names().get(*index).cloned()
                    })
                    .ok()
                    .flatten();
                if let Some(name) = name {
                    this.workspace
                        .update(cx, |workspace, cx| workspace.apply_theme_by_name(&name, cx))
                        .ok();
                }
            }),
        );

        v_flex()
            .gap_5()
            .child(field_with_column(
                localization::t("Standard font:"),
                APPEARANCE_LABEL_COLUMN,
                standard,
            ))
            .child(field_with_column(
                localization::t("Monospaced font:"),
                APPEARANCE_LABEL_COLUMN,
                monospaced,
            ))
            .child(field_with_column(
                localization::t("Line height:"),
                APPEARANCE_LABEL_COLUMN,
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(line_height)
                    .child(note(localization::t("times"), cx)),
            ))
            .child(field_with_column(
                localization::t("Appearance:"),
                APPEARANCE_LABEL_COLUMN,
                appearance_mode,
            ))
            .child(field_with_column(
                localization::t("Status bar:"),
                APPEARANCE_LABEL_COLUMN,
                status_bar,
            ))
            .child(field_with_column(
                localization::t("Editor opacity:"),
                APPEARANCE_LABEL_COLUMN,
                opacity,
            ))
            .child(section(localization::t("Theme"), theme_list))
            .child(self.help_link_row())
            .into_any_element()
    }

    /// One font row, as upstream's `FontSettingView` lays it out: the sample
    /// (family and size, drawn in the family), a bare-number size stepper, and
    /// a *Select…* button; a second row of small *Antialias* and *Ligatures*
    /// toggles underneath.
    fn render_font(
        &self,
        key: &'static str,
        family: Option<String>,
        size: f32,
        antialias: bool,
        ligature: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // The sample names the point size; it is drawn at the interface size.
        // Upstream's `FontSettingView` renders `displayNameAndSize` in the
        // control's own font, so the row stays one line high whatever size the
        // document is set to.
        const SAMPLE_SIZE: f32 = 13.;

        let fallback = if key == "standard" {
            cx.theme().font_family.to_string()
        } else {
            cx.theme().mono_font_family.to_string()
        };
        let resolved = fonts::resolve(cx, family.as_deref(), &fallback);

        let sample = div().flex_1().min_w_0().max_w(px(260.)).child(
            fonts::sample(
                fonts::display_name_and_size(&resolved, size),
                &resolved,
                SAMPLE_SIZE,
            )
            .overflow_hidden()
            .truncate(),
        );

        let size_id = if key == "standard" {
            "standard-font-size"
        } else {
            "monospaced-font-size"
        };
        let size_stepper = stepper(
            size_id,
            format!("{size:.0}"),
            cx,
            move |this, cx| this.adjust_font_size(key, -1.0, cx),
            move |this, cx| this.adjust_font_size(key, 1.0, cx),
        );

        // The picker is a menu rather than AppKit's font panel: the families
        // the platform reports, in the same list the theme resolves against.
        // The chosen name is checked so the menu opens showing where the user
        // is, and it is stored in the platform's own spelling so a later
        // resolution is an exact match.
        let pane = cx.weak_entity();
        let picker = Button::new(ElementId::Name(SharedString::from(format!(
            "{key}-font-family"
        ))))
        .small()
        .label(localization::t("Select…"))
        .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, _window, cx| {
            let mut menu = menu;
            for name in fonts::installed_families(cx) {
                let pane = pane.clone();
                let family = name.clone();
                let checked = name.eq_ignore_ascii_case(&resolved);
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(name.clone()))
                        .checked(checked)
                        .on_click(move |_, window, cx| {
                            pane.update(cx, |this, cx| {
                                this.choose_font(key, family.clone(), window, cx);
                            })
                            .ok();
                        }),
                );
            }
            // Every family the platform reports — a menu that outgrows the
            // screen has to scroll, as the font picker it stands in for does.
            menu.scrollable(true)
        });

        // The two switches under the row. They write the appearance options
        // only: the document renderer reads them from there, so the pane and the
        // page cannot disagree.
        let antialias_toggle = font_toggle(
            format!("{key}-antialias"),
            localization::t("Antialias"),
            antialias,
            if key == "standard" {
                |appearance: &mut AppearanceOptions, on| appearance.standard_antialias = on
            } else {
                |appearance: &mut AppearanceOptions, on| appearance.monospaced_antialias = on
            },
            cx,
        );
        let ligature_toggle = font_toggle(
            format!("{key}-ligature"),
            localization::t("Ligatures"),
            ligature,
            if key == "standard" {
                |appearance: &mut AppearanceOptions, on| appearance.standard_ligature = on
            } else {
                |appearance: &mut AppearanceOptions, on| appearance.monospaced_ligature = on
            },
            cx,
        );

        v_flex()
            .gap_2()
            .items_start()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .w_full()
                    .child(sample)
                    .child(size_stepper)
                    .child(picker),
            )
            .child(
                h_flex()
                    .gap_4()
                    .items_center()
                    .child(antialias_toggle)
                    .child(ligature_toggle),
            )
            .into_any_element()
    }

    // ---- Window ----------------------------------------------------------

    /// The Window pane, in `WindowSettingsView.swift`'s order.
    fn render_window(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let window = settings.window;
        let editor = settings.editor;

        // Upstream draws this as a `Picker`: one popup button, with the
        // system-following choice set apart from the three explicit ones. It is
        // a real button (bezel + caret) that hugs its label, not a full-width
        // row — a stretched ghost button centers "Respect System Setting"
        // halfway across the pane, far from the label it belongs to.
        let pane = cx.weak_entity();
        let tabbing = Button::new("window-tabbing")
            .small()
            .dropdown_caret(true)
            .label(localization::t(window.window_tabbing.label()))
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, _window, _cx| {
                let mut menu = menu;
                for (index, option) in WindowTabbing::ALL.iter().enumerate() {
                    // The system-following choice is its own group upstream.
                    if index == 1 {
                        menu = menu.separator();
                    }
                    let pane = pane.clone();
                    let option = *option;
                    menu = menu.item(
                        PopupMenuItem::new(localization::t(option.label())).on_click(
                            move |_, _, cx| {
                                pane.update(cx, |this, cx| {
                                    this.edit(cx, move |settings| {
                                        settings.window.window_tabbing = option
                                    })
                                })
                                .ok();
                            },
                        ),
                    );
                }
                menu
            });

        let size = h_flex()
            .gap_3()
            .items_start()
            .child(window_size_field(
                self.window_width.clone(),
                localization::t("Width"),
                cx,
            ))
            .child(window_size_field(
                self.window_height.clone(),
                localization::t("Height"),
                cx,
            ));

        let show = v_flex()
            .gap_1()
            .child(edit_switch(
                "show-line-numbers",
                localization::t("Line numbers"),
                editor.show_line_numbers,
                cx,
                |settings, checked| settings.editor.show_line_numbers = checked,
            ))
            .child(edit_switch(
                "show-invisibles",
                localization::t("Invisible characters"),
                editor.show_invisibles,
                cx,
                |settings, checked| settings.editor.show_invisibles = checked,
            ))
            .child(
                // Upstream's 3-column grid under the parent switch, read
                // column-first: line ending over other whitespace, tab over
                // other control characters, space alone. Upstream leaves these
                // live while *Invisible characters* is off — they only decide
                // which characters the parent switch would show — so they use
                // the plain switch, not the disabled-when-parent-is-off one.
                h_flex()
                    .gap_5()
                    .items_start()
                    .pl_5()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(edit_switch(
                                "show-invisible-new-line",
                                localization::t("Line ending"),
                                editor.show_invisible_new_line,
                                cx,
                                |settings, checked| {
                                    settings.editor.show_invisible_new_line = checked
                                },
                            ))
                            .child(edit_switch(
                                "show-invisible-whitespaces",
                                localization::t("Other whitespace"),
                                editor.show_invisible_whitespaces,
                                cx,
                                |settings, checked| {
                                    settings.editor.show_invisible_whitespaces = checked
                                },
                            )),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(edit_switch(
                                "show-invisible-tab",
                                localization::t("Tab"),
                                editor.show_invisible_tab,
                                cx,
                                |settings, checked| settings.editor.show_invisible_tab = checked,
                            ))
                            .child(edit_switch(
                                "show-invisible-control",
                                localization::t("Other control characters"),
                                editor.show_invisible_control,
                                cx,
                                |settings, checked| {
                                    settings.editor.show_invisible_control = checked
                                },
                            )),
                    )
                    .child(v_flex().gap_1().child(edit_switch(
                        "show-invisible-space",
                        localization::t("Space"),
                        editor.show_invisible_space,
                        cx,
                        |settings, checked| settings.editor.show_invisible_space = checked,
                    ))),
            )
            .child(edit_switch(
                "show-indent-guides",
                localization::t("Indent guides"),
                editor.show_indent_guides,
                cx,
                |settings, checked| settings.editor.show_indent_guides = checked,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(edit_switch(
                        "show-page-guide",
                        localization::t("Page guide at column:"),
                        editor.show_page_guide,
                        cx,
                        |settings, checked| settings.editor.show_page_guide = checked,
                    ))
                    .child(stepper_when(
                        "page-guide-column",
                        format!("{}", editor.page_guide_column),
                        editor.show_page_guide,
                        cx,
                        |this, cx| {
                            this.edit(cx, |settings| {
                                settings.editor.page_guide_column =
                                    settings.editor.page_guide_column.saturating_sub(1).max(1)
                            })
                        },
                        |this, cx| {
                            this.edit(cx, |settings| {
                                settings.editor.page_guide_column =
                                    (settings.editor.page_guide_column + 1).min(999)
                            })
                        },
                    )),
            );

        let wrapping = v_flex()
            .gap_1()
            .child(edit_switch(
                "wrap-lines",
                localization::t("Wrap lines to editor width"),
                editor.wrap_lines,
                cx,
                |settings, checked| settings.editor.wrap_lines = checked,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    // The label, a stepper and the word "spaces" are wider than
                    // the column at the window's default width, and the tail was
                    // being cut off at the edge. Wrapping keeps it whole.
                    .flex_wrap()
                    .child(edit_switch(
                        "hanging-indent",
                        localization::t("Indent wrapped lines by"),
                        window.enables_hanging_indent,
                        cx,
                        |settings, checked| settings.window.enables_hanging_indent = checked,
                    ))
                    .child(stepper_when(
                        "hanging-indent-width",
                        format!("{:.0}", window.hanging_indent_width),
                        window.enables_hanging_indent,
                        cx,
                        |this, cx| {
                            this.edit(cx, |settings| {
                                settings.window.hanging_indent_width =
                                    (settings.window.hanging_indent_width - 1.0).max(0.0)
                            })
                        },
                        |this, cx| {
                            this.edit(cx, |settings| {
                                settings.window.hanging_indent_width =
                                    (settings.window.hanging_indent_width + 1.0).min(99.0)
                            })
                        },
                    ))
                    .child(note(localization::t("spaces"), cx)),
            );

        // Upstream offers a third choice, "Vertical"; GPUI has no vertical
        // text layout, so the radio has the two directions this editor can
        // actually draw. Adding a dead third button would be the worse lie.
        let writing_direction = radio_row(
            "writing-direction",
            &[
                localization::t("Left to right"),
                localization::t("Right to left"),
            ],
            usize::from(window.writing_direction == WritingDirection::RightToLeft),
            cx.listener(|this, index: &usize, _, cx| {
                let direction = if *index == 1 {
                    WritingDirection::RightToLeft
                } else {
                    WritingDirection::LeftToRight
                };
                this.edit(cx, move |settings| {
                    settings.window.writing_direction = direction
                });
            }),
        );

        // The rate is a fraction of the viewport kept scrollable below the
        // last line, stepped in tenths and read as a percentage.
        let overscroll = stepper(
            "overscroll-rate",
            format!("{:.0}%", window.overscroll_rate * 100.0),
            cx,
            |this, cx| {
                this.edit(cx, |settings| {
                    settings.window.overscroll_rate =
                        ((settings.window.overscroll_rate * 10.0).round() - 1.0).max(0.0) / 10.0
                })
            },
            |this, cx| {
                this.edit(cx, |settings| {
                    settings.window.overscroll_rate =
                        ((settings.window.overscroll_rate * 10.0).round() + 1.0).min(10.0) / 10.0
                })
            },
        );

        let status_bar = h_flex()
            .gap_5()
            .items_start()
            .child(
                v_flex()
                    .gap_1()
                    .child(edit_switch(
                        "status-lines",
                        localization::t("Line count"),
                        window.show_status_bar_lines,
                        cx,
                        |settings, checked| settings.window.show_status_bar_lines = checked,
                    ))
                    .child(edit_switch(
                        "status-chars",
                        localization::t("Character count"),
                        window.show_status_bar_chars,
                        cx,
                        |settings, checked| settings.window.show_status_bar_chars = checked,
                    ))
                    .child(edit_switch(
                        "status-words",
                        localization::t("Word count"),
                        window.show_status_bar_words,
                        cx,
                        |settings, checked| settings.window.show_status_bar_words = checked,
                    )),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(edit_switch(
                        "status-location",
                        localization::t("Location"),
                        window.show_status_bar_location,
                        cx,
                        |settings, checked| settings.window.show_status_bar_location = checked,
                    ))
                    .child(edit_switch(
                        "status-line",
                        localization::t("Current line"),
                        window.show_status_bar_line,
                        cx,
                        |settings, checked| settings.window.show_status_bar_line = checked,
                    ))
                    .child(edit_switch(
                        "status-column",
                        localization::t("Current column"),
                        window.show_status_bar_column,
                        cx,
                        |settings, checked| settings.window.show_status_bar_column = checked,
                    )),
            );

        v_flex()
            .gap_4()
            .child(field(
                localization::t("Open documents in tabs:"),
                h_flex().child(tabbing),
            ))
            .child(field(localization::t("Window size:"), size))
            .child(divider(cx))
            .child(field(localization::t("Show:"), show))
            .child(field(
                localization::t("Current line:"),
                edit_switch(
                    "highlight-current-line",
                    localization::t("Change background color"),
                    editor.highlight_current_line,
                    cx,
                    |settings, checked| settings.editor.highlight_current_line = checked,
                ),
            ))
            .child(divider(cx))
            .child(field(localization::t("Line wrapping:"), wrapping))
            .child(field(
                localization::t("Writing direction:"),
                writing_direction,
            ))
            .child(field(localization::t("Overscroll:"), overscroll))
            .child(divider(cx))
            .child(field(localization::t("Status bar shows:"), status_bar))
            .child(self.help_link_row())
            .into_any_element()
    }

    // ---- Edit ------------------------------------------------------------

    /// The Edit pane, in `EditSettingsView.swift`'s order.
    fn render_edit(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let edit: EditOptions = settings.edit;

        let indentation = v_flex()
            .gap_2()
            // Upstream's `Picker` draws its label inline, next to the popup:
            // "Prefer using [Spaces ▾]". The pane-wide 12 rem label column is
            // already spent on the category ("Indentation:"), so these two
            // inner rows label themselves and sit at the control column.
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().text_sm().child(localization::t("Prefer using:")))
                    .child(radio_row(
                        "indent-style",
                        &[localization::t("Spaces"), localization::t("Tabs")],
                        usize::from(!edit.auto_expand_tab),
                        cx.listener(|this, index: &usize, _, cx| {
                            let uses_spaces = *index == 0;
                            this.edit(cx, move |settings| {
                                let width = settings.edit.tab_width;
                                settings.set_indentation(uses_spaces, width);
                            });
                        }),
                    )),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().text_sm().child(localization::t("Indent width:")))
                    .child(stepper(
                        "indent-width",
                        format!("{}", edit.tab_width),
                        cx,
                        |this, cx| {
                            this.edit(cx, |settings| {
                                let width = settings.edit.tab_width.saturating_sub(1).max(1);
                                let spaces = settings.edit.auto_expand_tab;
                                settings.set_indentation(spaces, width);
                            })
                        },
                        |this, cx| {
                            this.edit(cx, |settings| {
                                let width = (settings.edit.tab_width + 1).min(99);
                                let spaces = settings.edit.auto_expand_tab;
                                settings.set_indentation(spaces, width);
                            })
                        },
                    ))
                    .child(note(localization::t("spaces"), cx)),
            )
            .child(edit_switch(
                "detects-indent-style",
                localization::t("Detect indent style on document opening"),
                edit.detects_indent_style,
                cx,
                |settings, checked| settings.edit.detects_indent_style = checked,
            ))
            .child(edit_switch(
                "auto-indent",
                localization::t("Automatically indent while typing"),
                edit.auto_indent,
                cx,
                |settings, checked| settings.edit.auto_indent = checked,
            ))
            .child(edit_switch(
                "indent-with-tab-key",
                localization::t("Indent selection with Tab key"),
                edit.indent_with_tab_key,
                cx,
                |settings, checked| settings.edit.indent_with_tab_key = checked,
            ));

        let whitespace = v_flex()
            .gap_2()
            .child(edit_switch(
                "trim-trailing-whitespace",
                localization::t("Automatically trim trailing whitespace"),
                edit.auto_trims_trailing_whitespace,
                cx,
                |settings, checked| settings.edit.auto_trims_trailing_whitespace = checked,
            ))
            // 20 points, as upstream's `.padding(.leading, 20)`; the switch is
            // subordinate to the one above it, and only live while that one is.
            .child(v_flex().pl_5().child(child_switch(
                "trim-whitespace-only-lines",
                localization::t("Including whitespace-only lines"),
                edit.trims_whitespace_only_lines,
                edit.auto_trims_trailing_whitespace,
                cx,
                |settings, checked| settings.edit.trims_whitespace_only_lines = checked,
            )));

        let comment = v_flex()
            .gap_2()
            .child(edit_switch(
                "comment-delimiters-after-indent",
                localization::t("Insert comment delimiters after indent"),
                edit.inserts_comment_delimiters_after_indent,
                cx,
                |settings, checked| settings.edit.inserts_comment_delimiters_after_indent = checked,
            ))
            .child(edit_switch(
                "comment-spacer",
                localization::t("Add a space to comment delimiters"),
                edit.appends_comment_spacer,
                cx,
                |settings, checked| settings.edit.appends_comment_spacer = checked,
            ));

        let delay_enabled = edit.highlight_selection_instance;
        let content_parse = v_flex()
            .gap_2()
            .child(edit_switch(
                "auto-link-detection",
                localization::t("Link URLs in document"),
                edit.auto_link_detection,
                cx,
                |settings, checked| settings.edit.auto_link_detection = checked,
            ))
            .child(edit_switch(
                "highlight-braces",
                localization::t("Highlight matching braces"),
                edit.highlight_braces,
                cx,
                |settings, checked| settings.edit.highlight_braces = checked,
            ))
            .child(edit_switch(
                "highlight-selection-instance",
                localization::t("Highlight instances of selected text"),
                edit.highlight_selection_instance,
                cx,
                |settings, checked| settings.edit.highlight_selection_instance = checked,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .pl_5()
                    // Upstream binds this row to the switch above it: the value
                    // stays visible, greyed out, and stops accepting clicks.
                    .when(!delay_enabled, |this| {
                        this.text_color(cx.theme().muted_foreground)
                    })
                    .child(div().text_sm().child(localization::t("Delay:")))
                    .child(stepper_when(
                        "selection-instance-delay",
                        format!("{:.2}", edit.selection_instance_highlight_delay),
                        delay_enabled,
                        cx,
                        |this, cx| {
                            this.edit(cx, |settings| {
                                let value = (settings.edit.selection_instance_highlight_delay
                                    - 0.25)
                                    .max(0.0);
                                settings.edit.selection_instance_highlight_delay =
                                    (value * 100.0).round() / 100.0;
                            })
                        },
                        |this, cx| {
                            this.edit(cx, |settings| {
                                let value = (settings.edit.selection_instance_highlight_delay
                                    + 0.25)
                                    .min(10.0);
                                settings.edit.selection_instance_highlight_delay =
                                    (value * 100.0).round() / 100.0;
                            })
                        },
                    ))
                    .child(note(localization::t("seconds"), cx)),
            );

        // Upstream's Edit pane is one `Grid` of four rows, not four headed
        // sections: the category is a right-aligned label in the same column as
        // every other pane's labels, and each row's controls start at the
        // control column. Its `verticalSpacing` is 18 points; 16 is the nearest
        // step on this app's spacing scale, and the same gap the General and
        // Appearance panes use between rows.
        //
        // There is no "Saving" section: the byte-order mark is not an Edit-pane
        // switch in CotEditor at all — `saveUTF8BOM` is the mark half of the
        // *Format* pane's default file encoding, and that is where it is set.
        v_flex()
            .gap_4()
            .child(field(localization::t("Indentation:"), indentation))
            .child(field(localization::t("Whitespace:"), whitespace))
            .child(field(localization::t("Comment:"), comment))
            .child(field(localization::t("Content parse:"), content_parse))
            .child(self.help_link_row())
            .into_any_element()
    }

    // ---- Mode ------------------------------------------------------------

    /// The Mode pane: a list of modes on the left, that mode's options on the
    /// right, as `ModeSettingsView.swift` lays it out.
    fn render_mode(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let mode = settings.mode;

        let options = v_flex()
            .gap_4()
            .child(field_with_column(
                localization::t("Font:"),
                MODE_LABEL_COLUMN,
                radio_row(
                    "mode-font",
                    &[localization::t("Standard"), localization::t("Monospaced")],
                    usize::from(mode.font_type == FontType::Monospaced),
                    cx.listener(|this, index: &usize, _, cx| {
                        let font = if *index == 1 {
                            FontType::Monospaced
                        } else {
                            FontType::Standard
                        };
                        this.edit(cx, move |settings| settings.mode.font_type = font);
                    }),
                ),
            ))
            .child(field_with_column(
                localization::t("Substitution:"),
                MODE_LABEL_COLUMN,
                v_flex()
                    .gap_2()
                    .child(mode_switch(
                        "smart-insert-delete",
                        localization::t("Smart copy/paste"),
                        mode.smart_insert_delete,
                        false,
                        cx,
                        |settings, checked| settings.mode.smart_insert_delete = checked,
                    ))
                    .child(mode_switch(
                        "smart-quotes",
                        localization::t("Smart quotes"),
                        mode.automatic_quote_substitution,
                        true,
                        cx,
                        |settings, checked| settings.mode.automatic_quote_substitution = checked,
                    ))
                    .child(mode_switch(
                        "smart-dashes",
                        localization::t("Smart dashes"),
                        mode.automatic_dash_substitution,
                        true,
                        cx,
                        |settings, checked| settings.mode.automatic_dash_substitution = checked,
                    ))
                    .child(mode_switch(
                        "text-replacement",
                        localization::t("Text replacement"),
                        mode.automatic_text_replacement,
                        true,
                        cx,
                        |settings, checked| settings.mode.automatic_text_replacement = checked,
                    ))
                    .child(mode_switch(
                        "period-substitution",
                        localization::t("Add period with double-space"),
                        mode.automatic_period_substitution,
                        true,
                        cx,
                        |settings, checked| settings.mode.automatic_period_substitution = checked,
                    ))
                    .child(mode_switch(
                        "symbol-balancing",
                        localization::t("Automatically insert closing brackets and quotes"),
                        mode.automatic_symbol_balancing,
                        false,
                        cx,
                        |settings, checked| settings.mode.automatic_symbol_balancing = checked,
                    )),
            ))
            .child(field_with_column(
                localization::t("Spelling:"),
                MODE_LABEL_COLUMN,
                v_flex()
                    .gap_2()
                    .child(mode_switch(
                        "spell-checking",
                        localization::t("Check spelling while typing"),
                        mode.continuous_spell_checking,
                        true,
                        cx,
                        |settings, checked| settings.mode.continuous_spell_checking = checked,
                    ))
                    .child(mode_switch(
                        "grammar-checking",
                        localization::t("Check grammar with spelling"),
                        mode.grammar_checking,
                        true,
                        cx,
                        |settings, checked| settings.mode.grammar_checking = checked,
                    ))
                    .child(mode_switch(
                        "spelling-correction",
                        localization::t("Correct spelling automatically"),
                        mode.automatic_spelling_correction,
                        true,
                        cx,
                        |settings, checked| settings.mode.automatic_spelling_correction = checked,
                    )),
            ))
            .child(field_with_column(
                localization::t("Completion:"),
                MODE_LABEL_COLUMN,
                v_flex()
                    .gap_2()
                    // Upstream draws this line as ordinary text, not as a
                    // secondary caption; it introduces the three switches
                    // under it.
                    .child(
                        div()
                            .text_sm()
                            .child(localization::t("Completion list includes:")),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .pl_5()
                            .child(mode_switch(
                                "completion-standard",
                                localization::t("Standard words"),
                                mode.completion_standard_words,
                                true,
                                cx,
                                |settings, checked| {
                                    settings.mode.completion_standard_words = checked
                                },
                            ))
                            .child(mode_switch(
                                "completion-document",
                                localization::t("Words in document"),
                                mode.completion_document_words,
                                false,
                                cx,
                                |settings, checked| {
                                    settings.mode.completion_document_words = checked
                                },
                            ))
                            .child(mode_switch(
                                "completion-syntax",
                                localization::t("Words defined in syntax"),
                                mode.completion_syntax_words,
                                false,
                                cx,
                                |settings, checked| settings.mode.completion_syntax_words = checked,
                            )),
                    )
                    .child(mode_switch(
                        "automatic-completion",
                        localization::t("Suggest completions while typing"),
                        mode.automatic_completion,
                        false,
                        cx,
                        |settings, checked| settings.mode.automatic_completion = checked,
                    )),
            ))
            // Upstream's `ModeIndentOptionsView`. Turning the switch on copies
            // the Edit pane's two values into the mode, so the mode starts from
            // what the document already does rather than from zero; turning it
            // off removes the override and the Edit pane takes over again.
            .child(field_with_column(
                localization::t("Indentation:"),
                MODE_LABEL_COLUMN,
                v_flex()
                    .gap_2()
                    .child(
                        Checkbox::new("mode-custom-indentation")
                            .small()
                            .checked(mode.indent.is_some())
                            .label(localization::t("Use custom settings"))
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                let checked = *checked;
                                this.edit(cx, move |settings| {
                                    settings.mode.indent = if checked {
                                        Some(ModeIndent {
                                            expands_tab: settings.edit.auto_expand_tab,
                                            width: settings.edit.tab_width,
                                        })
                                    } else {
                                        None
                                    };
                                    settings.sync_indentation();
                                });
                            })),
                    )
                    .child({
                        let enabled = mode.indent.is_some();
                        // While the override is off these rows show the values
                        // the switch would seed — upstream's getters fall back
                        // to the defaults — so the group still reads as a
                        // preview of what turning it on means.
                        let indent = mode.indent.unwrap_or(ModeIndent {
                            expands_tab: settings.edit.auto_expand_tab,
                            width: settings.edit.tab_width,
                        });
                        // Captured by value: the closure sits beside a stepper
                        // that needs `cx` mutably, and a borrow of `cx` here
                        // would keep the two from coexisting.
                        let tint_color = (!enabled).then(|| cx.theme().muted_foreground);
                        let tint = move |text: &str| {
                            let label = div().text_sm().child(SharedString::from(text));
                            match tint_color {
                                Some(color) => label.text_color(color),
                                None => label,
                            }
                        };
                        v_flex()
                            .gap_2()
                            .pl_5()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(tint(localization::t("Prefer using:")))
                                    .child(radio_row_when(
                                        "mode-indent-style",
                                        &[localization::t("Spaces"), localization::t("Tabs")],
                                        usize::from(!indent.expands_tab),
                                        enabled,
                                        cx.listener(|this, index: &usize, _, cx| {
                                            let uses_spaces = *index == 0;
                                            this.edit(cx, move |settings| {
                                                // The mode's own width, not the
                                                // Edit pane's: the two can differ
                                                // once the override is on.
                                                let width = settings.effective_indentation().1;
                                                settings
                                                    .set_effective_indentation(uses_spaces, width);
                                            });
                                        }),
                                    )),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(tint(localization::t("Indent width:")))
                                    .child(stepper_when(
                                        "mode-indent-width",
                                        format!("{}", indent.width),
                                        enabled,
                                        cx,
                                        |this, cx| {
                                            this.edit(cx, |settings| {
                                                let (spaces, width) =
                                                    settings.effective_indentation();
                                                settings.set_effective_indentation(
                                                    spaces,
                                                    width.saturating_sub(1).max(1),
                                                );
                                            })
                                        },
                                        |this, cx| {
                                            this.edit(cx, |settings| {
                                                let (spaces, width) =
                                                    settings.effective_indentation();
                                                settings.set_effective_indentation(
                                                    spaces,
                                                    (width + 1).min(99),
                                                );
                                            })
                                        },
                                    ))
                                    .child(tint(localization::t("spaces"))),
                            )
                    }),
            ));

        let modes = v_flex()
            .id("mode-list")
            .gap_0p5()
            .p_1()
            // Upstream's `ModeListView` is pinned to 120 points.
            .w(rems(7.5))
            .flex_shrink_0()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("mode-general")
                    .ghost()
                    .small()
                    .w_full()
                    .selected(true)
                    .label(localization::t("General"))
                    .accessibility_label(localization::t("General mode")),
            )
            .child(
                Button::new("mode-code")
                    .ghost()
                    .small()
                    .w_full()
                    .selected(false)
                    .disabled(true)
                    .label(localization::t("Code"))
                    .tooltip(localization::t(
                        "Per-syntax modes arrive with the syntax editor",
                    )),
            );

        // Upstream groups the options in a `GroupBox` beside the list, and has
        // no prose above them. The paragraph that used to sit here explained
        // which part of CotEditor's mode model SubText implements — true, but
        // it is development history, in English, in an otherwise Chinese
        // window. The disabled Code row already says what is not there yet.
        v_flex()
            .gap_3()
            .child(
                h_flex().gap_4().items_start().child(modes).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().border)
                        .p_3()
                        .child(options),
                ),
            )
            .child(self.help_link_row())
            .into_any_element()
    }

    // ---- Format ----------------------------------------------------------

    /// The *Edit List…* dialog: the encoding priority list as a sheet over this
    /// window.
    ///
    /// Upstream presents `EncodingListView` as a SwiftUI `.sheet` owned by the
    /// Format pane, and the menu's *Customize Encodings List…* reaches it
    /// through `FormatSettingsView.Presentation`, which asks the pane to
    /// present the sheet it already holds. A dialog over the settings window is
    /// the same shape — one window, one modal, the button and the sheet in the
    /// same place — and it needs no channel between the two windows, which is
    /// what the request object upstream is there to provide.
    ///
    /// The dialog's OK writes the list back through the sheet, and the pane
    /// underneath is redrawn because it is built from the same list: the
    /// default encoding picker offers it.
    pub fn edit_encoding_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.settings(cx);
        let sheet = cx.new(|cx| {
            EncodingListSheet::new(
                self.workspace.clone(),
                settings.encoding_list.clone(),
                settings.encoding(),
                cx,
            )
        });
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, _| {
            let submit = sheet.clone();
            let refresh = this.clone();
            dialog
                .title(localization::t("Encoding List"))
                // `Dialog::w` takes pixels, not a `Length`, so the interface
                // scale is applied by hand rather than by the layout engine.
                // Upstream asks for `idealWidth: 440` points.
                .w(window.rem_size() * 27.5)
                .child(sheet.clone())
                .footer(
                    DialogFooter::new()
                        // Escape and the dialog's own close button both reach
                        // `Cancel`, and the sheet edits a copy, so cancelling
                        // costs nothing.
                        .child(
                            DialogClose::new()
                                .trigger(|button| button.label(localization::t("Cancel"))),
                        )
                        // Enter reaches `Confirm`; the footer's own button has
                        // to dispatch it by hand, which is what `DialogAction`
                        // is for.
                        .child(
                            DialogAction::new().child(
                                Button::new("encoding-list-ok")
                                    .label(localization::t("OK"))
                                    .primary(),
                            ),
                        ),
                )
                .on_ok(move |_, _, cx| {
                    submit.update(cx, |sheet, cx| sheet.submit(cx));
                    refresh.update(cx, |_, cx| cx.notify()).ok();
                    true
                })
        });
    }

    /// The Format pane, in `FormatSettingsView.swift`'s order.
    ///
    /// Upstream is one `Grid`: five right-aligned labels, a rule after the line
    /// endings and another after the encoding rows. The line ending, encoding
    /// and syntax rows are popup buttons — a value that opens its list — not
    /// lists standing in the pane; only *Available syntaxes* is a list, and it
    /// is a fixed 260 × 140 box in the control column.
    fn render_format(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let endings = LineEnding::BASIC;
        let ending_index = endings
            .iter()
            .position(|ending| *ending == settings.default_line_ending)
            .unwrap_or(0);
        // Upstream labels each ending "\(description) (\(label))", e.g.
        // "macOS / Unix (LF)" — the platform name first, the abbreviation in
        // parentheses. Its zh-Hans localization leaves both in English.
        let ending_text = |ending: LineEnding| {
            SharedString::from(format!(
                "{} ({})",
                ending.platform_description(),
                ending.abbreviation()
            ))
        };
        let ending_labels = endings
            .iter()
            .map(|ending| Some(ending_text(*ending)))
            .collect::<Vec<_>>();

        let line_endings = popup_pick_list(
            "default-line-ending",
            ending_text(endings[ending_index]),
            ending_labels,
            Some(ending_index),
            cx,
            |this, index, cx| {
                let ending = LineEnding::BASIC[index];
                this.edit(cx, move |settings| settings.default_line_ending = ending);
            },
        );

        // The picker offers the user's own list, because that is the list the
        // encoding menu offers and the one detection works through — a default
        // the user cannot see anywhere else would be a setting they cannot
        // reason about. It is the *file* encoding that is picked, so the list
        // carries the UTF-8-with-BOM item too and its separators are drawn as
        // rules, which is what upstream's `ForEach(self.encodingManager
        // .fileEncodings)` does.
        //
        // A settings file can still hold a spelling the list does not: one of
        // the encodings the Add menu can put back, chosen in an earlier build,
        // or one the user has since taken out. It still decodes with it, so the
        // picker has to show it rather than come up blank. Same rule as the
        // encoding menu's: the encoding in force goes to the top when the list
        // lacks it.
        let current = settings.default_file_encoding();
        let mut items = FileEncoding::menu_items(&settings.encoding_list);
        if !items.iter().flatten().any(|item| *item == current) {
            items.insert(0, Some(current));
        }
        let encoding_index = items.iter().position(|item| *item == Some(current));
        let encoding_items = items
            .iter()
            .map(|item| item.map(|encoding| SharedString::from(encoding.name())))
            .collect::<Vec<_>>();
        let default_encoding = popup_pick_list(
            "default-encoding",
            SharedString::from(current.name()),
            encoding_items,
            encoding_index,
            cx,
            move |this, index, cx| {
                let picked = items.get(index).copied().flatten();
                if let Some(picked) = picked {
                    // One door for both halves: *Unicode (UTF-8) with BOM* sets
                    // the codec and the mark together, which is what makes a new
                    // document come out with the mark on it.
                    this.edit(cx, move |settings| {
                        settings.set_default_file_encoding(picked)
                    });
                }
            },
        );

        let priorities = v_flex()
            .gap_1()
            // Same as the General pane's *Manage Warnings*: a column stretches
            // its children on the cross axis, so the button needs a row.
            .child(
                h_flex().child(
                    Button::new("encoding-priorities")
                        .small()
                        .label(localization::t("Edit List…"))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.edit_encoding_list(window, cx)),
                        ),
                ),
            )
            .child(
                Checkbox::new("refer-to-encoding-tag")
                    .checked(settings.refer_to_encoding_tag)
                    .label(localization::t(
                        "Refer to the encoding declaration in the document",
                    ))
                    .on_click(cx.listener(|this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| settings.refer_to_encoding_tag = checked);
                    })),
            );

        let syntaxes = self
            .workspace
            .read_with(cx, |workspace, _| workspace.syntax_names())
            .unwrap_or_default();
        // Upstream's syntax picker: *None*, a rule, then the syntax names. A
        // syntax named by the settings but absent from this build still shows,
        // because a document can name one the app has since lost.
        let none_label = SharedString::from(localization::t("None"));
        let mut syntax_items: Vec<Option<SharedString>> = vec![Some(none_label.clone()), None];
        // What each row picks, parallel to `syntax_items`: the *None* row picks
        // nothing, the rule picks nothing (it is a separator and not clickable),
        // and every name row picks that name.
        let mut syntax_picks: Vec<Option<SharedString>> = vec![None, None];
        let (syntax_index, syntax_label) = match &settings.default_syntax {
            None => (0, none_label),
            Some(name) => {
                let label = SharedString::from(name.clone());
                match syntaxes.iter().position(|syntax| syntax == name) {
                    Some(position) => (position + 2, label),
                    None => {
                        syntax_items.push(Some(label.clone()));
                        syntax_picks.push(Some(label.clone()));
                        (2, label)
                    }
                }
            }
        };
        syntax_items.extend(
            syntaxes
                .iter()
                .map(|name| Some(SharedString::from(name.clone()))),
        );
        syntax_picks.extend(
            syntaxes
                .iter()
                .map(|name| Some(SharedString::from(name.clone()))),
        );
        let default_syntax = popup_pick_list(
            "default-syntax",
            syntax_label,
            syntax_items,
            Some(syntax_index),
            cx,
            move |this, index, cx| {
                let name = syntax_picks
                    .get(index)
                    .cloned()
                    .flatten()
                    .map(|name| name.to_string());
                this.edit(cx, move |settings| settings.default_syntax = name);
            },
        );

        // Upstream's `SyntaxListView`: a fixed 260 × 140 box, one row per
        // syntax. Its add/remove/edit accessories belong to CotEditor's syntax
        // manager, which SubText does not have; the box is a list here, not a
        // pair of buttons that would do nothing.
        let mut available = v_flex()
            .id("available-syntaxes")
            .w(rems(16.25))
            .h(rems(8.75))
            .p_1()
            .gap_0p5()
            .overflow_y_scroll()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border);
        for name in &syntaxes {
            available = available.child(
                div()
                    .px_2()
                    .py_1()
                    .text_sm()
                    .child(SharedString::from(name.clone())),
            );
        }

        // `Grid` rows: a rule after the line-ending row and another after the
        // two encoding rows, matching `Divider().padding(.vertical, 6)` at the
        // pane's 8-pt row spacing.
        let rule = || {
            div()
                .py(px(6.))
                .child(div().h(px(1.)).w_full().bg(cx.theme().border))
        };
        v_flex()
            .gap_2()
            .child(field(
                localization::t("Default line endings:"),
                line_endings,
            ))
            .child(rule())
            .child(field(
                localization::t("Default text encoding:"),
                default_encoding,
            ))
            .child(field(localization::t("Encoding priorities:"), priorities))
            .child(rule())
            .child(field(localization::t("Default syntax:"), default_syntax))
            // `test_support` is the harness's way to see this box: the four
            // pickers above are Buttons and register themselves, but a plain
            // `v_flex` is not in the observed element tree, so the regression
            // test could measure the buttons and never the list.
            .child(field(
                localization::t("Available syntaxes:"),
                available.test_support(),
            ))
            .child(self.help_link_row())
            .into_any_element()
    }

    // ---- Snippets --------------------------------------------------------

    /// The Snippets pane: two tables over one insertion-format editor.
    ///
    /// CotEditor's `SnippetsSettingsView`, whose `TabView` holds a
    /// `CommandSnippetsView` and a `FileDropView`. The two are the same shape —
    /// an explanatory line, a three-column table, an add/remove pair, and the
    /// editor for the selected item's format — and differ in what the columns
    /// hold and in what sits at the right-hand end of the button row: the last
    /// shortcut refusal on one, *Restore Defaults* on the other.
    fn render_snippets(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.sync_format_editor(window, cx);

        let tab = if self.snippet_tab == 0 {
            SnippetTab::Command
        } else {
            SnippetTab::FileDrop
        };

        let tabs = [SnippetTab::Command, SnippetTab::FileDrop]
            .into_iter()
            .map(|tab| Tab::new().label(localization::t(tab.label())))
            .collect::<Vec<_>>();

        let tab_bar = TabBar::new("snippet-tabs")
            .segmented()
            .small()
            .children(tabs)
            .selected_index(self.snippet_tab)
            .on_click(cx.listener(|this, index: &usize, _, cx| {
                this.snippet_tab = *index;
                cx.notify();
            }));

        let (rows, selection): (Vec<AnyElement>, Option<u64>) = match tab {
            SnippetTab::Command => (
                self.snippets
                    .iter()
                    .map(|row| row.clone().into_any_element())
                    .collect(),
                self.snippet_selection,
            ),
            SnippetTab::FileDrop => (
                self.file_drop
                    .iter()
                    .map(|row| row.clone().into_any_element())
                    .collect(),
                self.file_drop_selection,
            ),
        };

        let header = match tab {
            SnippetTab::Command => header(
                CommandColumn::ALL
                    .iter()
                    .map(|column| (localization::t(column.label()), column.width())),
                cx,
            )
            .into_any_element(),
            SnippetTab::FileDrop => header(
                FileDropColumn::ALL
                    .iter()
                    .map(|column| (localization::t(column.label()), column.width())),
                cx,
            )
            .into_any_element(),
        };

        // Upstream's table is white behind both the header and the rows, with
        // the card showing only at its edges. The stripes are drawn even when
        // the table is empty, so the field reads as a table waiting for an
        // item rather than as a blank panel with an apology in it.
        let empty_stripes = v_flex()
            .id("snippet-empty-stripes")
            .w_full()
            .flex_shrink_0()
            .children((0..4).map(|index| {
                div()
                    .w_full()
                    .h(rems(SNIPPET_ROW_HEIGHT))
                    .when(index % 2 == 0, |this| this.bg(cx.theme().table_even))
            }));

        let table = div()
            .id("snippet-table")
            .w_full()
            .flex_1()
            // Not `min_h(0)`: an empty table still has to show its header and
            // enough stripes to be recognisable as a table.
            .min_h(rems(5.))
            .mt(px(6.))
            .overflow_hidden()
            // White, not `background` (#fafafa): the even-row stripes are
            // #fafafa, so on the window background they were invisible.
            .bg(cx.theme().table)
            .border_1()
            .border_color(cx.theme().border)
            .child(header)
            .child(
                v_flex()
                    .id("snippet-rows")
                    .w_full()
                    .flex_1()
                    .min_h(rems(0.))
                    .overflow_y_scroll()
                    .when(rows.is_empty(), |this| this.child(empty_stripes))
                    .children(rows.into_iter().enumerate().map(|(index, row)| {
                        // Upstream alternates the row backgrounds, first row
                        // shaded. The wrapper spans the full width so the
                        // stripe reaches both borders, as it does there.
                        div()
                            .w_full()
                            .flex_shrink_0()
                            .when(index % 2 == 0, |this| this.bg(cx.theme().table_even))
                            .child(row)
                    })),
            );

        // The grey card and the capsule of tabs that straddles its top edge.
        // The card is laid out 11 points down so the capsule can sit at the
        // top of the body and paint half in, half out, as upstream's
        // `TabView(.tabBarOnly)` does. Spacing inside is per-child rather than
        // one gap: upstream's gaps differ (6 above the table, 11 under it,
        // 21 before the format editor), and a uniform gap put the editor in
        // the wrong place.
        let card = v_flex()
            .id("snippet-card")
            .w_full()
            .flex_1()
            .min_h(rems(0.))
            .rounded_lg()
            .bg(cx.theme().group_box)
            .px(px(20.))
            .pt(px(17.))
            .pb(px(14.))
            .child(note(localization::t(tab.intro()), cx))
            .child(table)
            .child(
                div()
                    .w_full()
                    .mt(px(11.))
                    .child(self.render_snippet_buttons(tab, selection, cx)),
            )
            .child(
                div()
                    .w_full()
                    .mt(px(21.))
                    .child(self.render_format_editor(tab, selection, cx)),
            );

        let body = v_flex()
            .relative()
            .w_full()
            .h(rems(SNIPPETS_TAB_HEIGHT))
            .pt(px(11.))
            .child(card)
            .child(
                h_flex()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .justify_center()
                    .child(tab_bar),
            );

        v_flex()
            .w_full()
            .gap_2()
            .child(body)
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .child(self.render_help_link()),
            )
            .into_any_element()
    }

    /// The circular *?* at the end of a pane — upstream's
    /// `HelpLink(anchor:)`.
    ///
    /// Upstream closes every pane with this control, pointing it at that
    /// pane's help anchor. SubText opens the same documentation page for all
    /// of them, so one parameterised control serves every pane; the id is
    /// per-pane only so a switch starts with clean hover state.
    ///
    /// Upstream draws a filled secondary-coloured disc with a bold question
    /// mark, not the outlined circle symbol; a `secondary` label button with
    /// an explicit 24-point box is the same control, and it keeps the disc
    /// visible when the pointer is elsewhere.
    fn render_help_link(&self) -> AnyElement {
        Button::new(SharedString::from(format!(
            "settings-help-{}",
            self.pane.title()
        )))
        .secondary()
        .compact()
        // Upstream's disc measures 24 points across (48 px in a 2×
        // capture). `.compact()` also sets `min_w_6()`, so `.w()` alone
        // cannot shrink it — that is what keeps this a circle.
        .w(px(24.))
        .min_w(px(24.))
        .h(px(24.))
        .px_0()
        .rounded_full()
        .text_sm()
        .font_semibold()
        .label("?")
        .accessibility_label(localization::t("Open help"))
        .tooltip(localization::t("Open help."))
        .on_click(|_, _, cx| cx.open_url(crate::dispatch::DOCS_URL))
        .into_any_element()
    }

    /// The help link row every pane but Snippets ends with.
    ///
    /// Snippets already builds its own row: its table has a fixed height, so
    /// the link belongs to the column that holds the table and the editor
    /// rather than to the pane body.
    fn help_link_row(&self) -> AnyElement {
        h_flex()
            .w_full()
            .justify_end()
            .child(self.render_help_link())
            .into_any_element()
    }

    /// The row under the table: *Add* and *Delete*, and what sits at the far
    /// end of them.
    fn render_snippet_buttons(
        &self,
        tab: SnippetTab,
        selection: Option<u64>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let add = cx.listener(move |this, _, window, cx| match tab {
            SnippetTab::Command => this.add_snippet(window, cx),
            SnippetTab::FileDrop => this.add_file_drop(window, cx),
        });
        let remove = cx.listener(move |this, _, _, cx| match tab {
            SnippetTab::Command => this.remove_snippet(cx),
            SnippetTab::FileDrop => this.remove_file_drop(cx),
        });

        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(
                // One joined capsule, as upstream's two `NSButton`s sit
                // edge-to-edge in a single bordered control. Separate bordered
                // buttons read as two unrelated commands; the divider is what
                // makes them one add/remove pair.
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().secondary)
                    .overflow_hidden()
                    .child(
                        Button::new(ElementId::Name(SharedString::from(format!(
                            "snippet-add-{tab:?}"
                        ))))
                        .ghost()
                        .small()
                        .icon(gpui_kit::assets::IconName::Plus)
                        .accessibility_label(localization::t("Add"))
                        .tooltip(localization::t("Add new item"))
                        .on_click(add),
                    )
                    .child(
                        div()
                            .w(px(1.))
                            .h(px(16.))
                            .flex_shrink_0()
                            .bg(cx.theme().border),
                    )
                    .child(
                        Button::new(ElementId::Name(SharedString::from(format!(
                            "snippet-delete-{tab:?}"
                        ))))
                        .ghost()
                        .small()
                        .icon(gpui_kit::assets::IconName::Minus)
                        .accessibility_label(localization::t("Delete"))
                        .tooltip(localization::t("Delete selected items"))
                        .disabled(selection.is_none())
                        .on_click(remove),
                    ),
            )
            .child(div().flex_1())
            .when(tab == SnippetTab::Command, |this| {
                this.when_some(self.snippet_error.clone(), |this, error| {
                    this.child(div().text_xs().text_color(cx.theme().danger).child(error))
                })
            })
            .when(tab == SnippetTab::FileDrop, |this| {
                this.child(
                    Button::new("file-drop-restore-defaults")
                        .small()
                        .label(localization::t("Restore Defaults"))
                        .tooltip(localization::t(
                            "Put the file-drop settings back to the ones CotEditor ships.",
                        ))
                        .disabled(!self.can_restore_file_drop(cx))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.restore_file_drop(window, cx)),
                        ),
                )
            })
            .into_any_element()
    }

    /// The insertion-format editor, which upstream calls
    /// `InsertionFormatView`.
    ///
    /// Disabled unless exactly one row is selected, because a format belongs to
    /// one item; the prompt says why. Upstream can also say *Multiple items
    /// selected*, which cannot happen here — see the deviation note in
    /// `docs/PLAN.md`.
    fn render_format_editor(
        &self,
        tab: SnippetTab,
        selection: Option<u64>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor = match tab {
            SnippetTab::Command => self.snippet_format.clone(),
            SnippetTab::FileDrop => self.file_drop_format.clone(),
        };
        let disabled = selection.is_none();

        let header = h_flex()
            .w_full()
            .items_end()
            .gap_2()
            .child(div().text_sm().child(localization::t("Insertion format:")))
            .child(div().flex_1())
            .child(self.render_variable_menu(tab, disabled, cx));

        let body = div()
            .relative()
            .w_full()
            .h(rems(FORMAT_EDITOR_HEIGHT))
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            // The field is white upstream, a shade above the grey card.
            .bg(cx.theme().table)
            .overflow_hidden();

        // No editor at all while nothing is selected: upstream draws an empty
        // white field and puts the prompt in the middle of it. A disabled
        // `Textarea` would still paint its own empty document, and its caret
        // and hit area would suggest the field is live.
        let body = if disabled {
            body.flex().items_center().justify_center().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(localization::t("No item selected")),
            )
        } else {
            body.child(
                Textarea::new(&editor)
                    .bordered(false)
                    .h(rems(FORMAT_EDITOR_HEIGHT))
                    .aria_label(localization::t("Insertion format")),
            )
        };

        v_flex()
            .w_full()
            .gap_2()
            .child(header)
            .child(body)
            .into_any_element()
    }

    /// The *Insert Variable* menu, which inserts a token at the caret.
    ///
    /// Upstream's `InsertionMenuContent`: each entry shows the variable's name
    /// and, under it, what it stands for. The File Drop menu is three groups —
    /// the path tokens with no heading, then *Text File* and *Image File* —
    /// which is the shape `Section` gives it there and a label item gives it
    /// here.
    fn render_variable_menu(
        &self,
        tab: SnippetTab,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor = match tab {
            SnippetTab::Command => self.snippet_format.clone(),
            SnippetTab::FileDrop => self.file_drop_format.clone(),
        };
        let pane = cx.entity().downgrade();

        Button::new(ElementId::Name(SharedString::from(format!(
            "insert-variable-{tab:?}"
        ))))
        .outline()
        .small()
        .label(localization::t("Insert Variable"))
        .dropdown_caret(true)
        .tooltip(localization::t(
            "Insert a variable into the insertion format.",
        ))
        .disabled(disabled)
        .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, _window, _cx| {
            let mut menu = menu;
            match tab {
                SnippetTab::Command => {
                    for variable in subtext_core::snippets::Variable::ALL {
                        menu = menu.item(variable_item(
                            variable.name(),
                            variable.description(),
                            variable.token(),
                            &editor,
                            &pane,
                        ));
                    }
                }
                SnippetTab::FileDrop => {
                    for (heading, group) in [
                        (None, subtext_core::filedrop::Variable::PATH.as_slice()),
                        (
                            Some("Text File"),
                            subtext_core::filedrop::Variable::TEXT.as_slice(),
                        ),
                        (
                            Some("Image File"),
                            subtext_core::filedrop::Variable::IMAGE.as_slice(),
                        ),
                    ] {
                        if let Some(heading) = heading {
                            menu = menu.separator().label(heading);
                        }
                        for variable in group {
                            menu = menu.item(variable_item(
                                variable.name(),
                                variable.description(),
                                variable.token(),
                                &editor,
                                &pane,
                            ));
                        }
                    }
                }
            }
            menu
        })
        .into_any_element()
    }

    // ---- the snippet tables ----------------------------------------------

    /// Row identity for both tables.
    fn take_row_id(&mut self) -> u64 {
        let id = self.next_row_id;
        self.next_row_id += 1;
        id
    }

    /// The syntaxes the rows' pickers offer.
    fn syntax_names(&self, cx: &App) -> Vec<String> {
        self.workspace
            .read_with(cx, |workspace, _| workspace.syntax_names())
            .unwrap_or_default()
    }

    /// Push the syntax list and the selection into every row.
    fn sync_rows(&self, cx: &mut Context<Self>) {
        let syntaxes = Rc::new(self.syntax_names(cx));
        for row in &self.snippets {
            let id = row.read(cx).id();
            let state =
                crate::snippet_pane::row_state(&syntaxes, self.snippet_selection == Some(id));
            row.update(cx, |row, cx| row.sync(state, cx));
        }
        for row in &self.file_drop {
            let id = row.read(cx).id();
            let state =
                crate::snippet_pane::row_state(&syntaxes, self.file_drop_selection == Some(id));
            row.update(cx, |row, cx| row.sync(state, cx));
        }
    }

    /// Select a row, which is what clicking one does.
    pub fn select_row(&mut self, tab: SnippetTab, id: u64, cx: &mut Context<Self>) {
        let slot = match tab {
            SnippetTab::Command => &mut self.snippet_selection,
            SnippetTab::FileDrop => &mut self.file_drop_selection,
        };
        if *slot == Some(id) {
            return;
        }
        *slot = Some(id);
        self.sync_rows(cx);
        cx.notify();
    }

    /// A Command row has changed; the settings and the menu follow it.
    ///
    /// The rows are read back rather than handed in, which is safe because a
    /// subscription handler runs as an effect — after the row's own update has
    /// ended. The error is recomputed here rather than pushed, so a row that
    /// was loaded from a hand-edited settings file with a conflicting shortcut
    /// shows it as soon as the pane opens.
    pub fn snippets_edited(&mut self, cx: &mut Context<Self>) {
        self.snippet_error = self
            .snippets
            .iter()
            .find_map(|row| row.read(cx).error().map(str::to_string));
        self.commit_snippets(cx);
    }

    /// The same for the File Drop table, which has no error to show.
    pub fn file_drop_edited(&mut self, cx: &mut Context<Self>) {
        self.commit_file_drop(cx);
    }

    /// Write the snippet list back, and rebuild the menu and key map from it.
    fn edit_snippets(&self, cx: &mut App, apply: impl FnOnce(&mut Settings) + 'static) {
        self.edit_snippets_and(cx, apply, true);
    }

    fn edit_file_drop(&self, cx: &mut App, apply: impl FnOnce(&mut Settings) + 'static) {
        self.edit_snippets_and(cx, apply, false);
    }

    /// Run `apply`, save, and rebuild what depends on it.
    ///
    /// The snippet list is what the *Insert Snippet* submenu and its key
    /// equivalents are built from, so a change to it has to reach both; the
    /// file-drop list is read only when a file is dropped, so it needs the save
    /// and nothing else.
    fn edit_snippets_and(
        &self,
        cx: &mut App,
        apply: impl FnOnce(&mut Settings) + 'static,
        rebuild_menu: bool,
    ) {
        let mut apply = Some(apply);
        self.edit_workspace(cx, move |workspace, window, cx| {
            if let Some(apply) = apply.take() {
                apply(&mut workspace.settings);
            }
            workspace.apply_settings(window, cx);
            if rebuild_menu {
                workspace.rebuild_menus(cx);
            }
        });
    }

    /// *Add* on the Command tab.
    ///
    /// The name is upstream's: *Untitled*, with the smallest number that is
    /// free. The row goes after the last selected one, or at the end when
    /// nothing is selected — upstream's own placement.
    fn add_snippet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let taken: Vec<String> = self
            .snippets
            .iter()
            .map(|row| row.read(cx).name(cx))
            .collect();
        let id = self.take_row_id();
        let snippet = Snippet::new(id, unique_name("Untitled", &taken));
        let pane = cx.weak_entity();
        let row = cx.new(|cx| SnippetRow::new(id, pane, snippet, window, cx));

        let at = self
            .snippet_selection
            .and_then(|selected| {
                self.snippets
                    .iter()
                    .position(|row| row.read(cx).id() == selected)
            })
            .map_or(self.snippets.len(), |index| index + 1);
        self.snippets.insert(at, row.clone());
        self.snippet_selection = Some(id);
        self.snippet_error = None;
        self.sync_rows(cx);
        self.commit_snippets(cx);
        row.update(cx, |row, cx| row.focus_name(window, cx));
        cx.notify();
    }

    /// *Delete* on the Command tab, which clears the selection afterwards.
    fn remove_snippet(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = self.snippet_selection else {
            return;
        };
        let Some(index) = self
            .snippets
            .iter()
            .position(|row| row.read(cx).id() == selected)
        else {
            return;
        };
        self.snippets.remove(index);
        self.snippet_selection = None;
        self.snippet_error = None;
        self.sync_rows(cx);
        self.commit_snippets(cx);
        cx.notify();
    }

    /// *Add* on the File Drop tab: a blank item, at the same place.
    fn add_file_drop(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let id = self.take_row_id();
        let pane = cx.weak_entity();
        let row = cx.new(|cx| FileDropRow::new(id, pane, FileDropItem::new(), _window, cx));

        let at = self
            .file_drop_selection
            .and_then(|selected| {
                self.file_drop
                    .iter()
                    .position(|row| row.read(cx).id() == selected)
            })
            .map_or(self.file_drop.len(), |index| index + 1);
        self.file_drop.insert(at, row);
        self.file_drop_selection = Some(id);
        self.sync_rows(cx);
        self.commit_file_drop(cx);
        cx.notify();
    }

    /// *Delete* on the File Drop tab.
    fn remove_file_drop(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = self.file_drop_selection else {
            return;
        };
        let Some(index) = self
            .file_drop
            .iter()
            .position(|row| row.read(cx).id() == selected)
        else {
            return;
        };
        self.file_drop.remove(index);
        self.file_drop_selection = None;
        self.sync_rows(cx);
        self.commit_file_drop(cx);
        cx.notify();
    }

    /// *Restore Defaults* on the File Drop tab.
    fn restore_file_drop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.file_drop.clear();
        self.file_drop_selection = None;
        let defaults = FileDropItem::defaults();
        for item in defaults {
            let id = self.take_row_id();
            let pane = cx.weak_entity();
            let row = cx.new(|cx| FileDropRow::new(id, pane, item, window, cx));
            self.file_drop.push(row);
        }
        self.file_drop_selection = self.file_drop.first().map(|row| row.read(cx).id());
        self.sync_rows(cx);
        self.commit_file_drop(cx);
        cx.notify();
    }

    /// Whether *Restore Defaults* has anything to restore.
    ///
    /// Upstream keeps the answer in a flag it sets while saving; here it is
    /// derived, because the list is always written out and so can simply be
    /// compared with the shipped one.
    fn can_restore_file_drop(&self, cx: &App) -> bool {
        self.file_drop_items(cx) != FileDropItem::defaults()
    }

    /// The File Drop list as the table currently describes it.
    ///
    /// Upstream drops the items whose format is empty before saving, so a row
    /// that has just been added and not yet described is not written out.
    fn file_drop_items(&self, cx: &App) -> Vec<FileDropItem> {
        self.file_drop
            .iter()
            .map(|row| row.read(cx).item(cx))
            .filter(|item| !item.format.is_empty())
            .collect()
    }

    fn commit_snippets(&self, cx: &mut Context<Self>) {
        let snippets: Vec<Snippet> = self
            .snippets
            .iter()
            .map(|row| row.read(cx).snippet(cx))
            .collect();
        self.edit_snippets(cx, move |settings| settings.snippets = snippets);
    }

    fn commit_file_drop(&self, cx: &mut Context<Self>) {
        let items = self.file_drop_items(cx);
        self.edit_file_drop(cx, move |settings| settings.file_drop = items);
    }

    /// Point the format editor at the selected row's format.
    ///
    /// Only when the selection has moved: the editor is also what writes the
    /// format back, so reloading it on every frame would overwrite what the
    /// user is typing with what the pane last heard about.
    fn sync_format_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (owner, format) = match self.snippet_tab {
            0 => (
                self.snippet_selection,
                self.snippet_selection.and_then(|id| {
                    self.snippets
                        .iter()
                        .find(|row| row.read(cx).id() == id)
                        .map(|row| row.read(cx).snippet(cx).format)
                }),
            ),
            _ => (
                self.file_drop_selection,
                self.file_drop_selection.and_then(|id| {
                    self.file_drop
                        .iter()
                        .find(|row| row.read(cx).id() == id)
                        .map(|row| row.read(cx).item(cx).format)
                }),
            ),
        };
        if self.format_owner == owner {
            return;
        }
        self.format_owner = owner;
        let editor = self.format_editor().clone();
        let text = format.unwrap_or_default();
        editor.update(cx, |state, cx| state.set_value(text, window, cx));
    }

    /// The format editor of the tab that is showing.
    fn format_editor(&self) -> &Entity<TextareaState> {
        match self.snippet_tab {
            0 => &self.snippet_format,
            _ => &self.file_drop_format,
        }
    }

    /// A keystroke in the format editor: the row and the settings follow it.
    fn on_format_change(
        &mut self,
        _editor: &Entity<TextareaState>,
        event: &gpui_kit::base::input::InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, gpui_kit::base::input::InputEvent::Change) {
            return;
        }
        self.commit_format(cx);
    }

    /// Take whatever the format editor now says and put it in the selected row.
    ///
    /// Called both when the user types and when *Insert Variable* puts a token
    /// in: `insert` is one of the state's silent edits, so it raises no event
    /// of its own and the menu has to say so explicitly.
    fn commit_format(&mut self, cx: &mut Context<Self>) {
        let Some(owner) = self.format_owner else {
            return;
        };
        let text = self.format_editor().read(cx).value().to_string();
        match self.snippet_tab {
            0 => {
                if let Some(row) = self
                    .snippets
                    .iter()
                    .find(|row| row.read(cx).id() == owner)
                    .cloned()
                {
                    row.update(cx, |row, cx| row.set_format(text, cx));
                }
                self.commit_snippets(cx);
            }
            _ => {
                if let Some(row) = self
                    .file_drop
                    .iter()
                    .find(|row| row.read(cx).id() == owner)
                    .cloned()
                {
                    row.update(cx, |row, cx| row.set_format(text, cx));
                }
                self.commit_file_drop(cx);
            }
        }
    }

    // ---- Shortcuts -------------------------------------------------------

    /// The Shortcuts pane: every menu command's key equivalent, grouped by the
    /// submenu it lives in. The table is the same one the menu bar is generated
    /// from, so the two cannot disagree.
    ///
    /// A group is the command's *whole* parent path, not just its top-level
    /// menu: `Edit ▸ Select` holds six commands, and grouping them under `Edit`
    /// labelled each row `Select` — six identical rows with different keys. The
    /// path is the one thing that tells them apart, and it is also what the
    /// menu bar shows.
    ///
    /// The paths are English because the keymap keys on English names; the
    /// headings and the command labels are translated where they are drawn,
    /// the same way the menu bar itself is (`localization::localize_menus`).
    fn render_shortcuts(&self, cx: &mut Context<Self>) -> AnyElement {
        let groups = shortcut_groups();

        let mut list = v_flex().id("shortcut-list").gap_3().p_2();
        for (menu, rows) in &groups {
            // `Edit ▸ Select` in the file, `编辑 › 选择` on screen. The
            // separator is a chevron rather than the file's `>`, which reads as
            // part of a command name at this size.
            let heading = menu
                .split(" > ")
                .map(localization::t)
                .collect::<Vec<_>>()
                .join(" › ");
            let mut column = v_flex().gap_0p5().child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child(heading),
            );
            for (command, shortcut) in rows {
                column = column.child(
                    h_flex()
                        .justify_between()
                        .gap_4()
                        .child(
                            div()
                                .text_sm()
                                .truncate()
                                .child(localization::t(command).to_string()),
                        )
                        .child(
                            div()
                                .text_sm()
                                .flex_shrink_0()
                                .text_color(cx.theme().muted_foreground)
                                .child((*shortcut).to_string()),
                        ),
                );
            }
            list = list.child(column);
        }

        v_flex()
            .gap_3()
            .child(note(
                localization::t(
                    "To change a shortcut, use the menu bar's own key equivalents. ⌘ maps to \
                     Control on Windows and Linux, where Command does not exist.",
                ),
                cx,
            ))
            .child(
                v_flex()
                    .id("shortcut-scroll")
                    // 22 rem, not 28: the pane scrolls as a whole, so a list
                    // taller than the space left under the note pushes the
                    // *Restore Defaults* row past the window's bottom edge and
                    // it takes a scroll to discover. 28 rem did exactly that.
                    .max_h(rems(22.))
                    .overflow_y_scroll()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(list),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        Button::new("restore-shortcuts")
                            .small()
                            .label(localization::t("Restore Defaults"))
                            .disabled(true)
                            .tooltip(localization::t("Nothing has been customised yet")),
                    )
                    .child(note(
                        // The count is a value, not part of the sentence:
                        // `tf` looks the sentence up first, so the translation
                        // decides where the number goes.
                        localization::tf(
                            "{} shortcuts, all at their CotEditor defaults",
                            &[&keymap::SHORTCUTS.len()],
                        ),
                        cx,
                    )),
            )
            .child(self.help_link_row())
            .into_any_element()
    }

    // ---- Integration -----------------------------------------------------

    /// The Integration pane, in `IntegrationSettingsView.swift`'s order.
    fn render_integration(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let command_line = v_flex()
            .gap_2()
            // The usual column-stretches-its-children case: a row keeps the
            // button the width of its label.
            .child(
                h_flex().child(
                    // Upstream hands this to `NSHelpManager` with the
                    // `about_cot` anchor. SubText has no help book, and the
                    // pane's own help link already opens the documentation,
                    // which is where that anchor lives — so the button stays
                    // live and opens the same page rather than sitting
                    // permanently greyed out.
                    Button::new("learn-more-cli")
                        .small()
                        .label(localization::t("Learn More…"))
                        .on_click(|_, _, cx| cx.open_url(crate::dispatch::DOCS_URL)),
                ),
            )
            .child(note(
                // No backticks: this is interface copy, not a README. The
                // sentence has to stay true, too: nothing installs a launcher
                // on the PATH, but the executable does open the paths it is
                // handed, which is what the copy tells the reader to use.
                localization::t(
                    "Run “subtext FILE” to open a file from a terminal. No installer puts \
                     “subtext” on your PATH yet, so call the executable by its full path.",
                ),
                cx,
            ));

        let remote = v_flex()
            .gap_2()
            .child(
                Checkbox::new("enables-remote-editing")
                    .checked(settings.enables_remote_editing)
                    .label(localization::t("Accept rmate connections"))
                    .on_click(cx.listener(|this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| {
                            settings.enables_remote_editing = checked
                        });
                    })),
            )
            // Upstream greys the `Port:` label along with the field it names,
            // so the row reads as one disabled control rather than a live label
            // over a dead field.
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .pl_5()
                    .child(
                        div()
                            .text_sm()
                            .when(!settings.enables_remote_editing, |this| {
                                this.text_color(cx.theme().muted_foreground)
                            })
                            .child(localization::t("Port:")),
                    )
                    .child(stepper_when(
                        "remote-editing-port",
                        format!("{}", settings.remote_editing_port),
                        settings.enables_remote_editing,
                        cx,
                        |this, cx| {
                            this.edit(cx, |settings| {
                                settings.remote_editing_port =
                                    settings.remote_editing_port.saturating_sub(1).max(1)
                            })
                        },
                        |this, cx| {
                            this.edit(cx, |settings| {
                                settings.remote_editing_port =
                                    settings.remote_editing_port.saturating_add(1)
                            })
                        },
                    )),
            )
            .child(div().pl_5().child(note(
                localization::t(
                    "Allow remote files to be opened in SubText over the rmate protocol. The \
                         port is stored, but the listener is not implemented yet: no socket is \
                         opened and no remote file can be received.",
                ),
                cx,
            )));

        v_flex()
            .gap_4()
            .child(field(localization::t("Command-line tool:"), command_line))
            .child(field(localization::t("Remote editing:"), remote))
            .child(self.help_link_row())
            .into_any_element()
    }
}

/// `keymap::SHORTCUTS` split into the groups the Shortcuts pane draws: one
/// group per submenu, in table order.
///
/// The group key is the command's whole parent path (`"Edit > Select"`), not
/// its top-level menu. Six `Edit > Select > …` commands share the leaf names
/// `Select All`, `Select Line`, … only distinguishable by that parent, and the
/// menu bar shows exactly that path.
fn shortcut_groups() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    let mut groups: Vec<(&'static str, Vec<(&'static str, &'static str)>)> = Vec::new();
    for entry in keymap::SHORTCUTS.iter() {
        // Copy out of the `&&'static str` the iterator hands back, so the
        // split borrows the literals themselves and the result can outlive the
        // loop.
        let path: &'static str = entry.0;
        let shortcut: &'static str = entry.1;
        let (menu, command) = path.rsplit_once(" > ").unwrap_or((path, path));
        // A submenu's commands are not adjacent in the table: the keymap is in
        // menu-bar order, so `Edit > Select` and `Edit > Spelling and Grammar`
        // sit between the last `Edit > Insert` row and the remaining `Edit`
        // ones. Merging only into the previous group would draw `Edit` twice
        // and split one menu across headings.
        match groups.iter_mut().find(|(name, _)| *name == menu) {
            Some((_, rows)) => rows.push((command, shortcut)),
            None => groups.push((menu, vec![(command, shortcut)])),
        }
    }
    groups
}

// ---- shared pieces -------------------------------------------------------

/// One *Insert Variable* entry: the variable's name, and under it what it
/// stands for. Choosing it puts the token where the caret is.
fn variable_item(
    name: &'static str,
    description: &'static str,
    token: String,
    editor: &Entity<TextareaState>,
    pane: &WeakEntity<SettingsWindow>,
) -> PopupMenuItem {
    let editor = editor.clone();
    let pane = pane.clone();

    PopupMenuItem::element(move |_, cx| {
        v_flex().gap_0p5().child(div().text_sm().child(name)).child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(description),
        )
    })
    .on_click(move |_, window, cx| {
        editor.update(cx, |state, cx| state.insert(token.clone(), window, cx));
        // `insert` is one of the state's silent edits, so it raises no event of
        // its own and the pane has to be told the format moved.
        pane.update(cx, |pane, cx| pane.commit_format(cx)).ok();
    })
}

/// A titled group of settings.
fn section(title: impl Into<SharedString>, body: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .child(body)
}

/// A two-column row: a right-aligned label, then its control.
///
/// This is CotEditor's `Grid(alignment: .leadingFirstTextBaseline)`. The label
/// column is a fixed width so the controls line up down the pane.
fn field(label: impl Into<SharedString>, control: impl IntoElement) -> impl IntoElement {
    field_with_column(label, label_column(), control)
}

/// The same row with a narrower label column, for panes that share their width
/// with a second column.
fn field_with_column(
    label: impl Into<SharedString>,
    column: f32,
    control: impl IntoElement,
) -> impl IntoElement {
    let label = label.into();
    let has_label = !label.is_empty();

    h_flex()
        .items_start()
        .gap_4()
        .when(has_label, |this| {
            this.child(
                div()
                    .w(rems(column))
                    .flex_shrink_0()
                    .flex()
                    .justify_end()
                    .child(div().text_sm().child(label)),
            )
        })
        .child(div().flex_1().min_w_0().child(control))
}

/// A one-line explanation in the secondary text colour.
fn note(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// A hairline rule between groups of rows.
///
/// One physical pixel is the intent here, so this is the one place a `px` value
/// is right: a rule that scaled with the interface font would stop reading as a
/// rule.
fn divider(cx: &App) -> impl IntoElement {
    div().h(px(1.)).w_full().bg(cx.theme().border)
}

/// A small stepper: a minus button, the value, and a plus button.
fn stepper(
    id: &'static str,
    value: String,
    cx: &mut Context<SettingsWindow>,
    on_decrement: impl Fn(&mut SettingsWindow, &mut Context<SettingsWindow>) + 'static,
    on_increment: impl Fn(&mut SettingsWindow, &mut Context<SettingsWindow>) + 'static,
) -> AnyElement {
    stepper_when(id, value, true, cx, on_decrement, on_increment)
}

/// The same stepper, greyed out while its switch is off.
///
/// Upstream binds a stepper's `isEnabled` to the switch above it, so the row
/// still shows the value the switch would use, but stops accepting clicks.
fn stepper_when(
    id: &'static str,
    value: String,
    enabled: bool,
    cx: &mut Context<SettingsWindow>,
    on_decrement: impl Fn(&mut SettingsWindow, &mut Context<SettingsWindow>) + 'static,
    on_increment: impl Fn(&mut SettingsWindow, &mut Context<SettingsWindow>) + 'static,
) -> AnyElement {
    let minus = cx.listener(move |this, _, _, cx| on_decrement(this, cx));
    let plus = cx.listener(move |this, _, _, cx| on_increment(this, cx));

    h_flex()
        .gap_1()
        .items_center()
        .child(
            Button::new(ElementId::Name(SharedString::from(format!(
                "{id}-decrement"
            ))))
            .small()
            .label("−")
            .accessibility_label(localization::t("Decrease"))
            .disabled(!enabled)
            .on_click(minus),
        )
        // 2.5 rem holds three digits, which is as wide as any of these
        // steppers goes; 4 rem left a gap that pushed the row past the window's
        // trailing edge on the Window pane.
        //
        // The value dims with the buttons: upstream's disabled `TextField`
        // draws its text in the tertiary colour, and a black number between two
        // grey buttons reads as a live control.
        .child(
            div()
                .min_w(rems(2.5))
                .text_sm()
                .text_center()
                .when(!enabled, |this| {
                    this.text_color(cx.theme().muted_foreground)
                })
                .child(value),
        )
        .child(
            Button::new(ElementId::Name(SharedString::from(format!(
                "{id}-increment"
            ))))
            .small()
            .label("+")
            .accessibility_label(localization::t("Increase"))
            .disabled(!enabled)
            .on_click(plus),
        )
        .into_any_element()
}

/// One of the two small switches under a font row in the Appearance pane.
///
/// `set` names the option the switch writes, so the row and its switch cannot
/// drift apart: the caller passes the same option the row was rendered from.
fn font_toggle(
    id: String,
    label: &'static str,
    checked: bool,
    set: fn(&mut AppearanceOptions, bool),
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let listener = cx.listener(move |this, checked: &bool, _, cx| {
        let checked = *checked;
        this.edit(cx, move |settings| set(&mut settings.appearance, checked));
    });

    Checkbox::new(ElementId::Name(SharedString::from(id)))
        .small()
        .checked(checked)
        .label(label)
        .on_click(listener)
        .into_any_element()
}

/// A horizontal radio group: CotEditor's `.pickerStyle(.radioGroup)`.
///
/// The caller passes a `Context::listener`, which is what carries the window
/// entity into the callback — the group itself has no entity to hand back.
fn radio_row(
    id: &'static str,
    options: &[&'static str],
    selected: usize,
    on_click: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> AnyElement {
    radio_row_when(id, options, selected, true, on_click)
}

/// The same group, greyed out while its switch is off.
///
/// Disabled radios do not call the handler, which is what keeps a mode's
/// indentation rows from writing an override that is switched off.
fn radio_row_when(
    id: &'static str,
    options: &[&'static str],
    selected: usize,
    enabled: bool,
    on_click: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> AnyElement {
    RadioGroup::horizontal(id)
        .selected_index(Some(selected))
        .children(options.iter().copied())
        .disabled(!enabled)
        .on_click(on_click)
        .into_any_element()
}

/// A radio group stacked top to bottom.
///
/// CotEditor's `.pickerStyle(.radioGroup)` inside a `VStack` is this shape. The
/// General pane uses it where the options are long enough that a horizontal row
/// would wrap: three English labels and their buttons do not fit the control
/// column of a 620 pt settings window.
fn radio_column(
    id: &'static str,
    options: &[&'static str],
    selected: usize,
    on_click: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> AnyElement {
    RadioGroup::vertical(id)
        .selected_index(Some(selected))
        .children(options.iter().copied())
        .on_click(on_click)
        .into_any_element()
}

/// The click handler a `pick_list` row runs, shared across the rows.
type ListClick = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// A bordered, scrollable list of mutually exclusive choices.
///
/// Used where CotEditor uses a list rather than a popup — the Appearance pane's
/// theme list, whose `ThemeListView` is a real list upstream. Each row is a
/// `Button` in its `selected` state, so selection is drawn by the design system
/// and announced by the accessibility layer.
fn pick_list(
    id: &'static str,
    items: Vec<SharedString>,
    selected: Option<usize>,
    cx: &App,
    on_click: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> AnyElement {
    pick_list_with_gaps(
        id,
        items.into_iter().map(Some).collect(),
        selected,
        cx,
        on_click,
    )
}

/// The same list, with a rule where the caller passes `None`.
///
/// Upstream's *Default text encoding* picker is the encoding priority list, and
/// that list holds separators — it draws them as `Divider()`s between the
/// groups. The rows keep their position in `items` as their index, so a gap
/// does not shift the values the click handler sees.
fn pick_list_with_gaps(
    id: &'static str,
    items: Vec<Option<SharedString>>,
    selected: Option<usize>,
    cx: &App,
    on_click: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let on_click: ListClick = Rc::new(on_click);

    // The frame is the scroll owner and the padding is a child of it: the rows
    // scroll inside the frame's inner edge, and the scrollbar runs down that
    // edge rather than through the inset.
    let mut rows = v_flex().gap_0p5().p_1();

    for (index, label) in items.into_iter().enumerate() {
        let Some(label) = label else {
            rows = rows.child(divider(cx));
            continue;
        };
        let on_click = on_click.clone();
        rows = rows.child(
            // The list is not reorderable, so the index is a stable identity
            // for the row; the name keeps it from colliding with other lists.
            Button::new(ElementId::NamedInteger(
                SharedString::from(id),
                index as u64,
            ))
            .ghost()
            .small()
            .w_full()
            .selected(selected == Some(index))
            // `Button` centres its label, and a list of centred names reads as
            // a column of headings rather than rows. A full-width child puts
            // the text against the leading edge, which is where a table cell
            // puts it.
            .child(div().w_full().child(label))
            .on_click(move |_, window, cx| on_click(&index, window, cx)),
        );
    }

    div()
        .id(id)
        .max_h(rems(18.))
        .overflow_y_scroll()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .child(rows)
        .into_any_element()
}

/// A popup picker: the value on a small button, the choices in the menu that
/// drops from it.
///
/// Upstream's Format pane uses `Picker`s capped at 260 pt, which is this
/// button's 16.25 rem. `items` holds `Option`s so a grouped list — the
/// encodings' separators, the rule under the syntax picker's *None* — keeps its
/// rules where the data puts them, and the row's position is still the index the
/// callback receives.
fn popup_pick_list<T: 'static>(
    id: &'static str,
    current: impl Into<SharedString>,
    items: Vec<Option<SharedString>>,
    selected: Option<usize>,
    cx: &mut Context<T>,
    on_pick: impl Fn(&mut T, usize, &mut Context<T>) + 'static,
) -> AnyElement {
    let entity = cx.entity().downgrade();
    let on_pick = Rc::new(on_pick);

    Button::new(id)
        .small()
        .w(rems(16.25))
        .dropdown_caret(true)
        .label(current)
        .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, window, _cx| {
            let mut menu = menu
                .max_w(window.rem_size() * 16.25)
                .max_h(window.rem_size() * 18.);
            for (index, item) in items.iter().enumerate() {
                let Some(label) = item.clone() else {
                    menu = menu.item(PopupMenuItem::separator());
                    continue;
                };
                let entity = entity.clone();
                let on_pick = on_pick.clone();
                menu = menu.item(
                    PopupMenuItem::new(label)
                        .checked(selected == Some(index))
                        .on_click(move |_, _, cx: &mut App| {
                            entity.update(cx, |this, cx| on_pick(this, index, cx)).ok();
                        }),
                );
            }
            menu.scrollable(true)
        })
        .into_any_element()
}

/// A checkbox whose value is written into one field of the settings.
///
/// The writer is a plain `fn`, not a closure, so it can be moved into the
/// click handler every time the switch is clicked.
fn edit_switch(
    id: &'static str,
    label: &'static str,
    checked: bool,
    cx: &mut Context<SettingsWindow>,
    apply: fn(&mut Settings, bool),
) -> impl IntoElement {
    Checkbox::new(id)
        .checked(checked)
        .label(label)
        .on_click(cx.listener(move |this, checked, _, cx| {
            let checked = *checked;
            this.edit(cx, move |settings| apply(settings, checked));
        }))
}

/// A checkbox that only makes sense while its parent switch is on.
fn child_switch(
    id: &'static str,
    label: &'static str,
    checked: bool,
    parent_on: bool,
    cx: &mut Context<SettingsWindow>,
    apply: fn(&mut Settings, bool),
) -> impl IntoElement {
    Checkbox::new(id)
        .checked(checked)
        .disabled(!parent_on)
        .label(label)
        .on_click(cx.listener(move |this, checked, _, cx| {
            let checked = *checked;
            this.edit(cx, move |settings| apply(settings, checked));
        }))
}

/// A Mode-pane checkbox. `appkit_only` marks the options whose behaviour lives
/// in AppKit's text engine, which are therefore shown disabled.
fn mode_switch(
    id: &'static str,
    label: &'static str,
    checked: bool,
    appkit_only: bool,
    cx: &mut Context<SettingsWindow>,
    apply: fn(&mut Settings, bool),
) -> AnyElement {
    Checkbox::new(id)
        .checked(checked)
        .disabled(appkit_only)
        .label(label)
        // Only when there is something to say: `.tooltip("")` still builds a
        // bubble, so a switch that has no note would hover up an empty card.
        .when(appkit_only, |this| {
            this.tooltip(localization::t(
                "Provided by the system text engine, which SubText does not use",
            ))
        })
        .on_click(cx.listener(move |this, checked, _, cx| {
            let checked = *checked;
            this.edit(cx, move |settings| apply(settings, checked));
        }))
        .into_any_element()
}

fn icon_for(pane: SettingsPane) -> gpui_kit::assets::IconName {
    use gpui_kit::assets::IconName;
    match pane {
        SettingsPane::General => IconName::Settings,
        SettingsPane::Appearance => IconName::Eye,
        SettingsPane::Window => IconName::Columns2,
        SettingsPane::Edit => IconName::SquarePen,
        SettingsPane::Mode => IconName::ToggleLeft,
        SettingsPane::Format => IconName::FileText,
        SettingsPane::Snippets => IconName::ScanText,
        SettingsPane::Shortcuts => IconName::Keyboard,
        SettingsPane::Integration => IconName::Terminal,
    }
}

/// One of the Window pane's two size fields: a 64-point trailing-aligned text
/// field over its dimension name, followed by the unit.
///
/// Upstream's field is a bare `NSTextField` 64 points wide with a trailing
/// alignment and an "Auto" placeholder; the dimension is the field's label
/// underneath it. Keeping that shape means the two size rows read as one block
/// rather than as two unrelated inputs.
fn window_size_field(state: Entity<InputState>, label: &'static str, cx: &App) -> AnyElement {
    h_flex()
        .gap_1p5()
        .items_start()
        .child(
            v_flex()
                .gap_1()
                .items_end()
                .child(
                    Input::new(&state)
                        .small()
                        .w(px(64.))
                        .text_align(TextAlign::Right),
                )
                .child(note(label, cx)),
        )
        .child(note(localization::t("px"), cx))
        .into_any_element()
}

/// The initial state of a size field: the saved number, or nothing with the
/// "Auto" placeholder standing in for `None`.
fn window_size_input(
    value: Option<f64>,
    window: &mut Window,
    cx: &mut Context<InputState>,
) -> InputState {
    let state = InputState::new(window, cx).placeholder(localization::t("Auto"));
    match value {
        Some(value) => state.default_value(window_size_text(Some(value))),
        None => state,
    }
}

/// What the field shows for a saved size, and what `parse_window_size` rounds
/// a typed size to: whole points, which is what upstream stores.
fn window_size_text(value: Option<f64>) -> SharedString {
    match value {
        Some(value) => SharedString::from(format!("{value:.0}")),
        None => SharedString::new_static(""),
    }
}

/// Reads a size field the way upstream reads its text field.
///
/// `None` means the text is not a size and the field should be put back;
/// `Some(None)` means the user cleared it, which stands for "Auto"; and
/// `Some(Some(n))` is a size in whole points.
fn parse_window_size(text: &str) -> Option<Option<f64>> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("auto") {
        return Some(None);
    }
    let value: f64 = text.parse().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    // The setting is stored as f64 and cast to f32 when the window resizes;
    // anything past this bound would saturate that cast to infinity. 100 000 pt
    // is already far wider than any display, so clamping costs nothing real.
    const LARGEST_WINDOW_SIZE: f64 = 100_000.0;
    Some(Some(value.min(LARGEST_WINDOW_SIZE).round()))
}

/// The initial state of the opacity field: the saved alpha as a percentage.
fn opacity_input(alpha: f32, window: &mut Window, cx: &mut Context<InputState>) -> InputState {
    InputState::new(window, cx).default_value(opacity_text(alpha))
}

/// What the percent field shows, and what it is normalised to after editing:
/// a whole percent, which is the precision upstream's slider shows.
fn opacity_text(alpha: f32) -> SharedString {
    SharedString::from(format!("{:.0}%", alpha * 100.0))
}

/// Read the percent field. A bare `55` means 55%, which is what a user typing
/// into a field labelled `%` expects; anything unparsable is `None`.
fn parse_percent(text: &str) -> Option<f32> {
    let text = text.trim().trim_end_matches('%').trim();
    let value: f32 = text.parse().ok()?;
    value.is_finite().then_some(value / 100.0)
}

/// One end of the opacity row: a small rounded square filled with the window
/// background and marked with a triangle in the label ink.
///
/// Upstream's `OpacitySample` fills the top-left half of the square with
/// `.primary` — AppKit's label colour, not the accent — at `1 - opacity`, and
/// skips the triangle altogether at full opacity. So "Transparent" carries a
/// dark mark and "Opaque" is an empty square; the two read as the ends of the
/// slider before it is touched. The label is the accessible name — the art
/// itself says nothing to a screen reader.
fn opacity_sample(opacity: f32, label: &'static str, cx: &App) -> AnyElement {
    let ink = cx.theme().foreground.opacity(1.0 - opacity);
    let marked = opacity < 1.0;

    div()
        .id(label)
        .flex_shrink_0()
        .w(px(20.))
        .h(px(20.))
        .rounded(px(20. / 3.))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .overflow_hidden()
        .role(gpui_kit::Role::Image)
        .aria_label(label)
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    if !marked {
                        return;
                    }
                    let inset = px(3.);
                    let left = bounds.origin.x + inset;
                    let right = bounds.origin.x + bounds.size.width - inset;
                    let top = bounds.origin.y + inset;
                    let bottom = bounds.origin.y + bounds.size.height - inset;

                    // Upstream's `Triangle`: the top-left half, its hypotenuse
                    // running from the bottom-left corner to the top-right.
                    let mut path = PathBuilder::fill();
                    path.move_to(point(left, bottom));
                    path.line_to(point(left, top));
                    path.line_to(point(right, top));
                    path.close();
                    if let Ok(path) = path.build() {
                        window.paint_path(path, ink);
                    }
                },
            )
            .size_full(),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key-equivalent of the menu table has to reach the pane exactly
    /// once. A row silently dropped here is a shortcut the user can trigger
    /// from the menu bar but cannot look up.
    #[test]
    fn the_shortcut_pane_lists_every_binding_once() {
        let groups = shortcut_groups();
        let rows: usize = groups.iter().map(|(_, rows)| rows.len()).sum();
        assert_eq!(rows, keymap::SHORTCUTS.len());
    }

    /// A group is a whole submenu path. Reading only the top-level menu put six
    /// `Edit > Select > …` commands under one `Edit` heading, where the label
    /// alone (`Select All`, `Select Line`, …) was the only thing telling them
    /// apart — and two different submenus can reuse a leaf name.
    #[test]
    fn groups_are_submenus_not_top_level_menus() {
        let groups = shortcut_groups();
        let select = groups
            .iter()
            .find(|(name, _)| *name == "Edit > Select")
            .expect("Edit ▸ Select is a group of its own");
        assert_eq!(select.1.len(), 6);

        // `Edit` on its own would mean some path had a single segment, which
        // the menu table never has.
        assert!(
            groups
                .iter()
                .all(|(name, _)| !keymap::SHORTCUTS.iter().any(|entry| entry.0 == *name)),
            "a group name is a parent path, never a command path"
        );
    }

    /// Two headings with the same name split one menu across sections. The
    /// keymap is in menu-bar order, so a submenu's rows are not contiguous and
    /// the grouping has to merge by name rather than by adjacency.
    #[test]
    fn every_submenu_heading_appears_once() {
        let groups = shortcut_groups();
        for (i, (name, _)) in groups.iter().enumerate() {
            assert!(
                !groups[..i].iter().any(|(seen, _)| seen == name),
                "`{name}` is listed twice"
            );
        }
    }

    /// The stored size is cast to `f32` when the window resizes; a value past
    /// that range would saturate to infinity and hand `px(inf)` to AppKit.
    #[test]
    fn a_window_size_too_large_for_the_cast_is_capped() {
        assert_eq!(parse_window_size("1e300"), Some(Some(100_000.0)));
        assert_eq!(parse_window_size("800"), Some(Some(800.0)));
        assert_eq!(parse_window_size("-1"), None);
        assert_eq!(parse_window_size("auto"), Some(None));
    }

    /// Two identical labels in one column are indistinguishable to the reader:
    /// the heading says which submenu, and the label is meant to say which
    /// command.
    #[test]
    fn no_group_repeats_a_command_label() {
        for (name, rows) in shortcut_groups() {
            for (i, (label, _)) in rows.iter().enumerate() {
                assert!(
                    !rows[..i].iter().any(|(seen, _)| seen == label),
                    "`{name}` lists `{label}` twice"
                );
            }
        }
    }
}
