//! The settings window.
//!
//! CotEditor puts its preferences in a separate window with five panes down
//! the left: General, Appearance, Editor, Format and Key Bindings. SubText
//! keeps that shape, and keeps the same relationship between the two windows:
//! the settings window edits the very same `Settings` the main window reads,
//! so a change is visible in the open documents as soon as it is made — there
//! is no Apply button in either application.
//!
//! The window holds a weak handle to the workspace rather than a copy of the
//! settings. A copy would be a second source of truth, and the two would
//! disagree the first time the menu bar changed something.

use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, button::{Button, ButtonVariants as _}, h_flex,
    switch::Switch, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    WeakEntity, Window, div, px,
};

use subtext_core::{LineEnding, encoding};

use crate::keymap;
use crate::settings::{Settings, SettingsPane};
use crate::workspace::Workspace;

pub struct SettingsWindow {
    workspace: WeakEntity<Workspace>,
    pane: SettingsPane,
}

impl SettingsWindow {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let _ = (window, cx);
        Self { workspace, pane: SettingsPane::default() }
    }

    /// The settings as they stand, read through the workspace.
    fn settings(&self, cx: &App) -> Settings {
        self.workspace
            .read_with(cx, |workspace, _| workspace.settings.clone())
            .unwrap_or_default()
    }

    /// Run `apply` against the workspace's settings and re-apply them.
    fn edit(
        &self,
        cx: &mut App,
        apply: impl FnOnce(&mut Settings) + 'static,
    ) {
        let mut apply = Some(apply);
        self.workspace
            .update_in(cx, move |workspace, window, cx| {
                if let Some(apply) = apply.take() {
                    apply(&mut workspace.settings);
                }
                workspace.apply_settings(window, cx);
            })
            .ok();
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title("Settings");
        let settings = self.settings(cx);

        let rail = SettingsPane::ALL
            .into_iter()
            .map(|pane| {
                let selected = self.pane == pane;
                Button::new(pane.title())
                    .when(selected, |this| this.primary())
                    .when(!selected, |this| this.ghost())
                    .icon(icon_for(pane))
                    .label(pane.title())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pane = pane;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let body = match self.pane {
            SettingsPane::General => self.render_general(&settings, cx),
            SettingsPane::Appearance => self.render_appearance(&settings, cx),
            SettingsPane::Editor => self.render_editor(&settings, cx),
            SettingsPane::Format => self.render_format(&settings, cx),
            SettingsPane::KeyBindings => self.render_key_bindings(cx),
        };

        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                v_flex()
                    .w(px(180.))
                    .h_full()
                    .p_2()
                    .gap_1()
                    .bg(cx.theme().secondary)
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .children(rail),
            )
            .child(
                v_flex()
                    .id("settings-pane")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .p_4()
                    .gap_4()
                    .overflow_y_scroll()
                    .child(body),
            )
    }
}

impl SettingsWindow {
    fn render_general(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let endings = [
            (LineEnding::LF, "LF"),
            (LineEnding::CRLF, "CRLF"),
            (LineEnding::CR, "CR"),
        ];
        let current_ending = settings.default_line_ending;
        let ending_buttons = endings
            .into_iter()
            .map(|(ending, label)| {
                let selected = current_ending == ending;
                Button::new(label)
                    .small()
                    .when(selected, |this| this.primary())
                    .when(!selected, |this| this.ghost())
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit(cx, move |settings| settings.default_line_ending = ending);
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        // The encodings CotEditor offers in its popup, most-used first. The
        // full registry stays reachable from the Format menu.
        const COMMON: [&str; 7] = [
            "Unicode (UTF-8)",
            "Unicode (UTF-16 Little-Endian)",
            "Unicode (UTF-16 Big-Endian)",
            "Japanese (Shift JIS)",
            "Japanese (EUC-JP)",
            "Western (Windows Latin 1)",
            "Chinese (GB 18030)",
        ];
        let common = COMMON
            .into_iter()
            .filter_map(|name| encoding::ALL.iter().copied().find(|e| e.name == name))
            .collect::<Vec<_>>();
        let current_encoding = settings.encoding();
        let encoding_buttons = common
            .into_iter()
            .map(|encoding| {
                let selected = current_encoding == encoding;
                Button::new(encoding.name)
                    .small()
                    .when(selected, |this| this.primary())
                    .when(!selected, |this| this.ghost())
                    .label(encoding.name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit(cx, move |settings| {
                            settings.default_encoding = encoding.identifier();
                        });
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let recent = settings.recent_documents.len();
        let syntax = settings
            .default_syntax
            .clone()
            .unwrap_or_else(|| "Plain Text".to_string());

        v_flex()
            .gap_4()
            .child(section(
                "New documents",
                v_flex()
                    .gap_3()
                    .child(row("Line endings", h_flex().gap_1().children(ending_buttons)))
                    .child(row(
                        "Encoding",
                        h_flex().gap_1().flex_wrap().children(encoding_buttons),
                    ))
                    .child(row(
                        "Syntax",
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(syntax),
                    )),
            ))
            .child(section(
                "Recent documents",
                v_flex()
                    .gap_3()
                    .child(row(
                        "Remembered",
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{recent} of 20")),
                    ))
                    .child(row(
                        "",
                        Button::new("clear-recent")
                            .small()
                            .label("Clear Menu")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.workspace
                                    .update_in(cx, |workspace, window, cx| {
                                        workspace.clear_recent(cx);
                                        workspace.apply_settings(window, cx);
                                    })
                                    .ok();
                            })),
                    )),
            ))
            .into_any_element()
    }

    fn render_appearance(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let themes = self
            .workspace
            .read_with(cx, |workspace, _| workspace.theme_names())
            .unwrap_or_default();
        let current = settings.theme.clone();

        let theme_buttons = themes
            .into_iter()
            .map(|name| {
                let selected = current.as_deref() == Some(name.as_str());
                // The button's element id and its label both need the name,
                // and the click handler needs a third copy to survive the
                // closure — hence the clone.
                let label = name.clone();
                let chosen = name.clone();
                Button::new(name)
                    .small()
                    .when(selected, |this| this.primary())
                    .when(!selected, |this| this.ghost())
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| {
                                workspace.apply_theme_by_name(&chosen, cx)
                            })
                            .ok();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let font_family = settings
            .font_family
            .clone()
            .unwrap_or_else(|| "Monospaced (system)".to_string());
        let font_size = settings.font_size;

        v_flex()
            .gap_4()
            .child(section(
                "Theme",
                v_flex().gap_1().child(h_flex().gap_1().flex_wrap().children(theme_buttons)),
            ))
            .child(section(
                "Font",
                v_flex()
                    .gap_3()
                    .child(row(
                        "Family",
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(font_family),
                    ))
                    .child(row(
                        "Size",
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("font-smaller")
                                    .small()
                                    .label("Smaller")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.workspace
                                            .update(cx, |workspace, cx| workspace.bump_font(-1.0, cx))
                                            .ok();
                                    })),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .w(px(56.))
                                    .text_center()
                                    .child(format!("{font_size:.0} pt")),
                            )
                            .child(
                                Button::new("font-bigger")
                                    .small()
                                    .label("Bigger")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.workspace
                                            .update(cx, |workspace, cx| workspace.bump_font(1.0, cx))
                                            .ok();
                                    })),
                            )
                            .child(
                                Button::new("font-reset")
                                    .small()
                                    .ghost()
                                    .label("Reset")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.workspace
                                            .update(cx, |workspace, cx| {
                                                workspace.set_font(None, cx);
                                                workspace.set_font_size(13.0, cx);
                                            })
                                            .ok();
                                    })),
                            ),
                    )),
            ))
            .into_any_element()
    }

    fn render_editor(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let mut switches = v_flex().gap_1();

        macro_rules! editor_switch {
            ($id:literal, $label:literal, $field:ident) => {{
                let weak = self.workspace.clone();
                let checked = settings.editor.$field;
                switches = switches.child(
                    Switch::new($id)
                        .checked(checked)
                        .label($label)
                        .on_click(move |checked, window, cx| {
                            let checked = *checked;
                            weak.update_in(cx, |workspace, window, cx| {
                                workspace.update_editor_options(window, cx, |options| {
                                    options.$field = checked
                                });
                            })
                            .ok();
                            let _ = window;
                        }),
                );
            }};
        }

        editor_switch!("opt-line-numbers", "Show line numbers", show_line_numbers);
        editor_switch!("opt-wrap", "Wrap lines", wrap_lines);
        editor_switch!("opt-invisibles", "Show invisible characters", show_invisibles);
        editor_switch!("opt-indent-guides", "Show indent guides", show_indent_guides);
        editor_switch!("opt-page-guide", "Show page guide", show_page_guide);
        editor_switch!("opt-active-line", "Highlight the current line", highlight_current_line);
        editor_switch!("opt-editable", "Editable", editable);

        let tab_width = settings.tab_width;
        let width_buttons = [2usize, 4, 8]
            .into_iter()
            .map(|width| {
                let selected = tab_width == width;
                Button::new(SharedString::from(format!("tab-{width}")))
                    .small()
                    .when(selected, |this| this.primary())
                    .when(!selected, |this| this.ghost())
                    .label(format!("{width}"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit(cx, move |settings| settings.tab_width = width);
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let expand = settings.auto_expand_tabs;
        let weak = self.workspace.clone();

        v_flex()
            .gap_4()
            .child(section("Behaviour", switches))
            .child(section(
                "Indentation",
                v_flex()
                    .gap_3()
                    .child(row("Tab width", h_flex().gap_1().children(width_buttons)))
                    .child(
                        Switch::new("opt-expand-tabs")
                            .checked(expand)
                            .label("Expand tabs to spaces")
                            .on_click(move |checked, _, cx| {
                                let checked = *checked;
                                weak.update(cx, |workspace, cx| {
                                    workspace.settings.auto_expand_tabs = checked;
                                    workspace.settings.uses_spaces_for_indentation = checked;
                                    let _ = workspace.settings.save();
                                    cx.notify();
                                })
                                .ok();
                            }),
                    ),
            ))
            .into_any_element()
    }

    fn render_format(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let antialias = settings.editor.antialias;
        let ligatures = settings.editor.ligatures;

        let weak_antialias = self.workspace.clone();
        let weak_ligatures = self.workspace.clone();

        v_flex()
            .gap_4()
            .child(section(
                "Rendering",
                v_flex()
                    .gap_1()
                    .child(
                        Switch::new("opt-antialias")
                            .checked(antialias)
                            .label("Antialias text")
                            .on_click(move |checked, window, cx| {
                                let checked = *checked;
                                weak_antialias
                                    .update_in(cx, |workspace, window, cx| {
                                        workspace.update_editor_options(window, cx, |options| {
                                            options.antialias = checked
                                        });
                                    })
                                    .ok();
                                let _ = window;
                            }),
                    )
                    .child(
                        Switch::new("opt-ligatures")
                            .checked(ligatures)
                            .label("Use ligatures")
                            .on_click(move |checked, window, cx| {
                                let checked = *checked;
                                weak_ligatures
                                    .update_in(cx, |workspace, window, cx| {
                                        workspace.update_editor_options(window, cx, |options| {
                                            options.ligatures = checked
                                        });
                                    })
                                    .ok();
                                let _ = window;
                            }),
                    ),
            ))
            .child(section(
                "Document",
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Line endings, encoding and syntax for the open document live \
                                 in the Format menu, where CotEditor keeps them.",
                            ),
                    ),
            ))
            .into_any_element()
    }

    fn render_key_bindings(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = v_flex().gap_0p5();
        for (path, shortcut, _, _) in keymap::SHORTCUTS {
            list = list.child(
                h_flex()
                    .justify_between()
                    .gap_4()
                    .py_0p5()
                    .child(
                        div()
                            .text_sm()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child((*path).to_string()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .flex_shrink_0()
                            .child((*shortcut).to_string()),
                    ),
            );
        }

        v_flex()
            .gap_3()
            .child(section(
                "Default key bindings",
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "These are CotEditor's own defaults, read from the same menu \
                                 table the menu bar is built from. ⌘ maps to Control on \
                                 Windows and Linux, where Command does not exist.",
                            ),
                    )
                    .child(list),
            ))
            .into_any_element()
    }
}

/// A titled group of settings.
fn section(title: &'static str, body: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(body)
}

/// A label on the left and its control on the right.
fn row(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    h_flex()
        .justify_between()
        .gap_4()
        .items_center()
        .child(div().text_sm().flex_shrink_0().child(label))
        .child(div().flex_1().flex().justify_end().child(control))
}

fn icon_for(pane: SettingsPane) -> gpui_kit::assets::IconName {
    use gpui_kit::assets::IconName;
    match pane {
        SettingsPane::General => IconName::Settings,
        SettingsPane::Appearance => IconName::Palette,
        SettingsPane::Editor => IconName::CaseSensitive,
        SettingsPane::Format => IconName::FileText,
        SettingsPane::KeyBindings => IconName::Menu,
    }
}

