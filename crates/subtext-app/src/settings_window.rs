//! The settings window.
//!
//! CotEditor puts its preferences in a separate window with nine panes down
//! the left, in this order: General, Appearance, Window, Edit, Mode, Format,
//! Snippets, Shortcuts and Integration. SubText keeps that shape — the same
//! nine panes, the same labels, and the same grouping of controls inside each
//! pane — because a preference the user knows by position should not move.
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

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    radio::RadioGroup,
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    WeakEntity, Window, div, px, rems,
};

use subtext_core::{LineEnding, TextEncoding, encoding};

use crate::keymap;
use crate::settings::{
    DocumentAppearance, DocumentConflictOption, EditOptions, FontType, NoDocumentOnLaunchOption,
    Settings, SettingsPane, WindowTabbing, WritingDirection,
};
use crate::workspace::Workspace;

/// How wide the label column of a settings row is, in rem.
const LABEL_COLUMN: f32 = 12.;

pub struct SettingsWindow {
    workspace: WeakEntity<Workspace>,
    pane: SettingsPane,
    /// Which tab of the Snippets pane is showing.
    snippet_tab: usize,
}

impl SettingsWindow {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let _ = (window, cx);
        Self { workspace, pane: SettingsPane::default(), snippet_tab: 0 }
    }

    /// The settings as they stand, read through the workspace.
    fn settings(&self, cx: &App) -> Settings {
        self.workspace
            .read_with(cx, |workspace, _| workspace.settings.clone())
            .unwrap_or_default()
    }

    /// Run `apply` against the workspace's settings and re-apply them.
    ///
    /// Every pane writes through here, so a control cannot change the model
    /// without the open documents hearing about it.
    fn edit(&self, cx: &mut App, apply: impl FnOnce(&mut Settings) + 'static) {
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
                // `selected`, not `primary`: this is navigation showing which
                // pane is current, not the default commit of a decision area.
                // Filling the rail width is what makes the selected row read as
                // a row rather than a floating button.
                Button::new(pane.title())
                    .ghost()
                    .w_full()
                    .selected(self.pane == pane)
                    .icon(icon_for(pane))
                    .label(pane.title())
                    .tooltip(pane.summary())
                    .accessibility_label(format!("{} settings", pane.title()))
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
            SettingsPane::Window => self.render_window(&settings, cx),
            SettingsPane::Edit => self.render_edit(&settings, cx),
            SettingsPane::Mode => self.render_mode(&settings, cx),
            SettingsPane::Format => self.render_format(&settings, cx),
            SettingsPane::Snippets => self.render_snippets(cx),
            SettingsPane::Shortcuts => self.render_shortcuts(cx),
            SettingsPane::Integration => self.render_integration(&settings, cx),
        };

        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                v_flex()
                    .w(rems(12.))
                    .min_w(rems(10.))
                    .h_full()
                    .p_2()
                    .gap_0p5()
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
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.pane.title()),
                    )
                    .child(body),
            )
    }
}

impl SettingsWindow {
    // ---- General ---------------------------------------------------------

    /// The General pane, in `GeneralSettingsView.swift`'s order.
    fn render_general(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let general = settings.general;

        let startup = v_flex()
            .gap_2()
            .child(
                Checkbox::new("quit-always-keeps-windows")
                    .checked(general.quit_always_keeps_windows)
                    .label("Reopen windows from last session")
                    .on_click(cx.listener(|this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| {
                            settings.general.quit_always_keeps_windows = checked
                        });
                    })),
            )
            .child(field(
                "When nothing else is open:",
                radio_row(
                    "no-document-on-launch",
                    &NoDocumentOnLaunchOption::ALL.map(NoDocumentOnLaunchOption::label),
                    general.no_document_on_launch_option as usize,
                    cx.listener(|this, index: &usize, _, cx| {
                        let option = NoDocumentOnLaunchOption::ALL[*index];
                        this.edit(cx, move |settings| {
                            settings.general.no_document_on_launch_option = option
                        });
                    }),
                ),
            ));

        let saving = v_flex()
            .gap_1()
            .child(
                Checkbox::new("enables-autosave-in-place")
                    .checked(general.enables_autosave_in_place)
                    .label("Enable Auto Save with Versions")
                    .on_click(cx.listener(|this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| {
                            settings.general.enables_autosave_in_place = checked
                        });
                    })),
            )
            .child(note(
                "Documents are saved automatically while you work, and earlier versions stay \
                 reachable from File ▸ Revert To.",
                cx,
            ));

        let conflicts = radio_row(
            "document-conflict",
            &DocumentConflictOption::ALL.map(DocumentConflictOption::label),
            general.document_conflict_option as usize,
            cx.listener(|this, index: &usize, _, cx| {
                let option = DocumentConflictOption::ALL[*index];
                this.edit(cx, move |settings| {
                    settings.general.document_conflict_option = option
                });
            }),
        );

        let transfers = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .child(Button::new("settings-export").small().label("Export…").disabled(true))
                    .child(Button::new("settings-import").small().label("Import…").disabled(true)),
            )
            .child(note(
                "Not implemented yet. The settings file can be edited directly, at the path below.",
                cx,
            ))
            .child(note(Settings::path().display().to_string(), cx));

        let warnings = v_flex()
            .gap_1()
            .child(Button::new("manage-warnings").small().label("Manage Warnings…").disabled(true))
            .child(note(
                "Not implemented yet. CotEditor lets you switch off each dialog it can show; \
                 SubText shows no such dialogs.",
                cx,
            ));

        let updates = v_flex()
            .gap_1()
            .child(
                Checkbox::new("automatic-update-checks")
                    .checked(general.enable_automatic_update_checks)
                    .label("Check for updates automatically")
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
                    .label("Update to prereleases")
                    .on_click(cx.listener(|this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| {
                            settings.general.checks_updates_for_beta = checked
                        });
                    })),
            );

        v_flex()
            .gap_6()
            .child(section("On startup", startup))
            .child(section("Document save", saving))
            .child(section("When a file is modified by another application", conflicts))
            .child(section("Settings import & export", transfers))
            .child(section("Dialog warnings", warnings))
            .child(section("Software update", updates))
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
            &DocumentAppearance::ALL.map(DocumentAppearance::label),
            DocumentAppearance::ALL
                .iter()
                .position(|mode| *mode == appearance.document_appearance)
                .unwrap_or(0),
            cx.listener(|this, index: &usize, _, cx| {
                let mode = DocumentAppearance::ALL[*index];
                this.edit(cx, move |settings| settings.appearance.document_appearance = mode);
            }),
        );

        let status_bar = radio_row(
            "status-bar-background",
            &["Tinted", "Opaque"],
            usize::from(settings.window.prefers_opaque_bar_background),
            cx.listener(|this, index: &usize, _, cx| {
                let opaque = *index == 1;
                this.edit(cx, move |settings| {
                    settings.window.prefers_opaque_bar_background = opaque
                });
            }),
        );

        let opacity = h_flex()
            .gap_3()
            .items_center()
            .child(stepper(
                "window-alpha",
                format!("{:.0}%", appearance.window_alpha * 100.0),
                cx,
                |this, cx| {
                    this.edit(cx, |settings| {
                        settings.appearance.window_alpha =
                            (settings.appearance.window_alpha - 0.05).max(0.2)
                    })
                },
                |this, cx| {
                    this.edit(cx, |settings| {
                        settings.appearance.window_alpha =
                            (settings.appearance.window_alpha + 0.05).min(1.0)
                    })
                },
            ))
            .child(note("20% – 100%, as in CotEditor", cx));

        let themes = self
            .workspace
            .read_with(cx, |workspace, _| workspace.theme_names())
            .unwrap_or_default();
        let current = settings
            .theme
            .clone()
            .and_then(|name| themes.iter().position(|theme| *theme == name));
        let theme_items = themes.into_iter().map(SharedString::from).collect::<Vec<_>>();
        let theme_list = pick_list(
            "theme-list",
            theme_items,
            current,
            cx,
            cx.listener(|this, index: &usize, _, cx| {
                let name = this
                    .workspace
                    .read_with(cx, |workspace, _| workspace.theme_names().get(*index).cloned())
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
            .child(field("Standard font:", standard))
            .child(field("Monospaced font:", monospaced))
            .child(field("Line height:", h_flex().gap_2().items_center().child(line_height).child(note("times", cx))))
            .child(field("Appearance:", appearance_mode))
            .child(field("Status bar:", status_bar))
            .child(field("Editor opacity:", opacity))
            .child(section("Theme", theme_list))
            .into_any_element()
    }

    /// One font row: the family in use, its size, antialiasing and ligatures.
    fn render_font(
        &self,
        key: &'static str,
        family: Option<String>,
        size: f32,
        antialias: bool,
        ligature: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown = family.unwrap_or_else(|| "System monospaced".to_string());

        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child(shown))
                    .child(stepper(
                        if key == "standard" { "standard-font-size" } else { "monospaced-font-size" },
                        format!("{size:.0} pt"),
                        cx,
                        |this, cx| {
                            this.workspace
                                .update(cx, |workspace, cx| workspace.bump_font(-1.0, cx))
                                .ok();
                        },
                        |this, cx| {
                            this.workspace
                                .update(cx, |workspace, cx| workspace.bump_font(1.0, cx))
                                .ok();
                        },
                    )),
            )
            .child(
                Checkbox::new(ElementId::Name(SharedString::from(format!("{key}-antialias"))))
                    .checked(antialias)
                    .label("Antialias text")
                    .on_click(cx.listener(move |this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| {
                            if key == "standard" {
                                settings.appearance.standard_antialias = checked;
                            } else {
                                settings.appearance.monospaced_antialias = checked;
                            }
                            settings.editor.antialias = checked;
                        });
                    })),
            )
            .child(
                Checkbox::new(ElementId::Name(SharedString::from(format!("{key}-ligature"))))
                    .checked(ligature)
                    .label("Use ligatures")
                    .on_click(cx.listener(move |this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| {
                            if key == "standard" {
                                settings.appearance.standard_ligature = checked;
                            } else {
                                settings.appearance.monospaced_ligature = checked;
                            }
                            settings.editor.ligatures = checked;
                        });
                    })),
            )
            .into_any_element()
    }

    // ---- Window ----------------------------------------------------------

    /// The Window pane, in `WindowSettingsView.swift`'s order.
    fn render_window(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let window = settings.window;
        let editor = settings.editor;

        let tabbing = radio_row(
            "window-tabbing",
            &WindowTabbing::ALL.map(WindowTabbing::label),
            WindowTabbing::ALL
                .iter()
                .position(|option| *option == window.window_tabbing)
                .unwrap_or(0),
            cx.listener(|this, index: &usize, _, cx| {
                let option = WindowTabbing::ALL[*index];
                this.edit(cx, move |settings| settings.window.window_tabbing = option);
            }),
        );

        let size = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_6()
                    .items_start()
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(div().text_sm().child(format!("{} px", window_size_label(window.window_width))))
                            .child(note("Width", cx)),
                    )
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(div().text_sm().child(format!("{} px", window_size_label(window.window_height))))
                            .child(note("Height", cx)),
                    ),
            )
            .child(note("“Auto” means the window sizes itself to its content.", cx));

        let show = v_flex()
            .gap_1()
            .child(edit_switch(
                "show-line-numbers",
                "Line numbers",
                editor.show_line_numbers,
                cx,
                |settings, checked| settings.editor.show_line_numbers = checked,
            ))
            .child(edit_switch(
                "show-invisibles",
                "Invisible characters",
                editor.show_invisibles,
                cx,
                |settings, checked| settings.editor.show_invisibles = checked,
            ))
            .child(
                v_flex()
                    .gap_1()
                    .pl_4()
                    .child(child_switch("show-invisible-new-line", "Line ending", editor.show_invisible_new_line, editor.show_invisibles, cx, |settings, checked| settings.editor.show_invisible_new_line = checked))
                    .child(child_switch("show-invisible-tab", "Tab", editor.show_invisible_tab, editor.show_invisibles, cx, |settings, checked| settings.editor.show_invisible_tab = checked))
                    .child(child_switch("show-invisible-space", "Space", editor.show_invisible_space, editor.show_invisibles, cx, |settings, checked| settings.editor.show_invisible_space = checked))
                    .child(child_switch("show-invisible-whitespaces", "Other whitespace", editor.show_invisible_whitespaces, editor.show_invisibles, cx, |settings, checked| settings.editor.show_invisible_whitespaces = checked))
                    .child(child_switch("show-invisible-control", "Other control characters", editor.show_invisible_control, editor.show_invisibles, cx, |settings, checked| settings.editor.show_invisible_control = checked)),
            )
            .child(edit_switch(
                "show-indent-guides",
                "Indent guides",
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
                        "Page guide at column:",
                        editor.show_page_guide,
                        cx,
                        |settings, checked| settings.editor.show_page_guide = checked,
                    ))
                    .child(stepper(
                        "page-guide-column",
                        format!("{}", editor.page_guide_column),
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
                "Wrap lines to editor width",
                editor.wrap_lines,
                cx,
                |settings, checked| settings.editor.wrap_lines = checked,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(edit_switch(
                        "hanging-indent",
                        "Indent wrapped lines by",
                        window.enables_hanging_indent,
                        cx,
                        |settings, checked| settings.window.enables_hanging_indent = checked,
                    ))
                    .child(stepper(
                        "hanging-indent-width",
                        format!("{:.0}", window.hanging_indent_width),
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
                    .child(note("spaces", cx)),
            );

        let writing_direction = radio_row(
            "writing-direction",
            &["Left to Right", "Right to Left"],
            usize::from(window.writing_direction == WritingDirection::RightToLeft),
            cx.listener(|this, index: &usize, _, cx| {
                let direction = if *index == 1 {
                    WritingDirection::RightToLeft
                } else {
                    WritingDirection::LeftToRight
                };
                this.edit(cx, move |settings| settings.window.writing_direction = direction);
            }),
        );

        let status_bar = h_flex()
            .gap_6()
            .items_start()
            .child(
                v_flex()
                    .gap_1()
                    .child(edit_switch("status-lines", "Line count", window.show_status_bar_lines, cx, |settings, checked| settings.window.show_status_bar_lines = checked))
                    .child(edit_switch("status-chars", "Character count", window.show_status_bar_chars, cx, |settings, checked| settings.window.show_status_bar_chars = checked))
                    .child(edit_switch("status-words", "Word count", window.show_status_bar_words, cx, |settings, checked| settings.window.show_status_bar_words = checked)),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(edit_switch("status-location", "Location", window.show_status_bar_location, cx, |settings, checked| settings.window.show_status_bar_location = checked))
                    .child(edit_switch("status-line", "Current line", window.show_status_bar_line, cx, |settings, checked| settings.window.show_status_bar_line = checked))
                    .child(edit_switch("status-column", "Current column", window.show_status_bar_column, cx, |settings, checked| settings.window.show_status_bar_column = checked)),
            );

        v_flex()
            .gap_4()
            .child(field("Open documents in tabs:", tabbing))
            .child(field("Window size:", size))
            .child(divider(cx))
            .child(field("Show:", show))
            .child(field(
                "Current line:",
                edit_switch("highlight-current-line", "Change background color", editor.highlight_current_line, cx, |settings, checked| settings.editor.highlight_current_line = checked),
            ))
            .child(divider(cx))
            .child(field("Line wrapping:", wrapping))
            .child(field("Writing direction:", writing_direction))
            .child(divider(cx))
            .child(field("Status bar shows:", status_bar))
            .into_any_element()
    }

    // ---- Edit ------------------------------------------------------------

    /// The Edit pane, in `EditSettingsView.swift`'s order.
    fn render_edit(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let edit: EditOptions = settings.edit;

        let indentation = v_flex()
            .gap_2()
            .child(field(
                "Prefer using:",
                radio_row(
                    "indent-style",
                    &["Spaces", "Tabs"],
                    usize::from(!edit.auto_expand_tab),
                    cx.listener(|this, index: &usize, _, cx| {
                        let uses_spaces = *index == 0;
                        this.edit(cx, move |settings| {
                            let width = settings.edit.tab_width;
                            settings.set_indentation(uses_spaces, width);
                        });
                    }),
                ),
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().text_sm().child("Indent width:"))
                    .child(stepper(
                        "indent-width",
                        format!("{}", edit.tab_width),
                        cx,
                        |this, cx| {
                            this.edit(cx, |settings| {
                                let width = settings.edit.tab_width.saturating_sub(1).max(1);
                                let spaces = settings.uses_spaces_for_indentation;
                                settings.set_indentation(spaces, width);
                            })
                        },
                        |this, cx| {
                            this.edit(cx, |settings| {
                                let width = (settings.edit.tab_width + 1).min(99);
                                let spaces = settings.uses_spaces_for_indentation;
                                settings.set_indentation(spaces, width);
                            })
                        },
                    ))
                    .child(note("spaces", cx)),
            )
            .child(edit_switch("detects-indent-style", "Detect indent style on document opening", edit.detects_indent_style, cx, |settings, checked| settings.edit.detects_indent_style = checked))
            .child(edit_switch("auto-indent", "Automatically indent while typing", edit.auto_indent, cx, |settings, checked| settings.edit.auto_indent = checked))
            .child(edit_switch("indent-with-tab-key", "Indent selection with Tab key", edit.indent_with_tab_key, cx, |settings, checked| settings.edit.indent_with_tab_key = checked));

        let whitespace = v_flex()
            .gap_1()
            .child(edit_switch("trim-trailing-whitespace", "Automatically trim trailing whitespace", edit.auto_trims_trailing_whitespace, cx, |settings, checked| settings.edit.auto_trims_trailing_whitespace = checked))
            .child(
                v_flex().pl_4().child(child_switch(
                    "trim-whitespace-only-lines",
                    "Including whitespace-only lines",
                    edit.trims_whitespace_only_lines,
                    edit.auto_trims_trailing_whitespace,
                    cx,
                    |settings, checked| settings.edit.trims_whitespace_only_lines = checked,
                )),
            );

        let comment = v_flex()
            .gap_1()
            .child(edit_switch("comment-delimiters-after-indent", "Insert comment delimiters after indent", edit.inserts_comment_delimiters_after_indent, cx, |settings, checked| settings.edit.inserts_comment_delimiters_after_indent = checked))
            .child(edit_switch("comment-spacer", "Add a space to comment delimiters", edit.appends_comment_spacer, cx, |settings, checked| settings.edit.appends_comment_spacer = checked));

        let content_parse = v_flex()
            .gap_1()
            .child(edit_switch("auto-link-detection", "Link URLs in document", edit.auto_link_detection, cx, |settings, checked| settings.edit.auto_link_detection = checked))
            .child(edit_switch("highlight-braces", "Highlight matching braces", edit.highlight_braces, cx, |settings, checked| settings.edit.highlight_braces = checked))
            .child(edit_switch("highlight-selection-instance", "Highlight instances of selected text", edit.highlight_selection_instance, cx, |settings, checked| settings.edit.highlight_selection_instance = checked))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .pl_4()
                    .child(div().text_sm().child("Delay:"))
                    .child(stepper(
                        "selection-instance-delay",
                        format!("{:.2}", edit.selection_instance_highlight_delay),
                        cx,
                        |this, cx| {
                            this.edit(cx, |settings| {
                                let value = (settings.edit.selection_instance_highlight_delay - 0.25).max(0.0);
                                settings.edit.selection_instance_highlight_delay = (value * 100.0).round() / 100.0;
                            })
                        },
                        |this, cx| {
                            this.edit(cx, |settings| {
                                let value = (settings.edit.selection_instance_highlight_delay + 0.25).min(10.0);
                                settings.edit.selection_instance_highlight_delay = (value * 100.0).round() / 100.0;
                            })
                        },
                    ))
                    .child(note("seconds", cx)),
            );

        let saving = v_flex()
            .gap_1()
            .child(edit_switch("save-utf8-bom", "Add a byte-order mark when saving as UTF-8", edit.save_utf8_bom, cx, |settings, checked| settings.edit.save_utf8_bom = checked))
            .child(note(
                "Switches whose behaviour SubText does not implement yet are drawn disabled \
                 rather than quietly ignored.",
                cx,
            ));

        v_flex()
            .gap_6()
            .child(section("Indentation", indentation))
            .child(section("Whitespace", whitespace))
            .child(section("Comment", comment))
            .child(section("Content parse", content_parse))
            .child(section("Saving", saving))
            .into_any_element()
    }

    // ---- Mode ------------------------------------------------------------

    /// The Mode pane: a list of modes on the left, that mode's options on the
    /// right, as `ModeSettingsView.swift` lays it out.
    fn render_mode(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let mode = settings.mode;

        let options = v_flex()
            .gap_4()
            .child(field(
                "Font:",
                radio_row(
                    "mode-font",
                    &["Standard", "Monospaced"],
                    usize::from(mode.font_type == FontType::Monospaced),
                    cx.listener(|this, index: &usize, _, cx| {
                        let font = if *index == 1 { FontType::Monospaced } else { FontType::Standard };
                        this.edit(cx, move |settings| settings.mode.font_type = font);
                    }),
                ),
            ))
            .child(field(
                "Substitution:",
                v_flex()
                    .gap_1()
                    .child(mode_switch("smart-insert-delete", "Smart copy/paste", mode.smart_insert_delete, false, cx, |settings, checked| settings.mode.smart_insert_delete = checked))
                    .child(mode_switch("smart-quotes", "Smart quotes", mode.automatic_quote_substitution, true, cx, |settings, checked| settings.mode.automatic_quote_substitution = checked))
                    .child(mode_switch("smart-dashes", "Smart dashes", mode.automatic_dash_substitution, true, cx, |settings, checked| settings.mode.automatic_dash_substitution = checked))
                    .child(mode_switch("text-replacement", "Text replacement", mode.automatic_text_replacement, true, cx, |settings, checked| settings.mode.automatic_text_replacement = checked))
                    .child(mode_switch("period-substitution", "Add period with double-space", mode.automatic_period_substitution, true, cx, |settings, checked| settings.mode.automatic_period_substitution = checked))
                    .child(mode_switch("symbol-balancing", "Automatically insert closing brackets and quotes", mode.automatic_symbol_balancing, false, cx, |settings, checked| settings.mode.automatic_symbol_balancing = checked)),
            ))
            .child(field(
                "Spelling:",
                v_flex()
                    .gap_1()
                    .child(mode_switch("spell-checking", "Check spelling while typing", mode.continuous_spell_checking, true, cx, |settings, checked| settings.mode.continuous_spell_checking = checked))
                    .child(mode_switch("grammar-checking", "Check grammar with spelling", mode.grammar_checking, true, cx, |settings, checked| settings.mode.grammar_checking = checked))
                    .child(mode_switch("spelling-correction", "Correct spelling automatically", mode.automatic_spelling_correction, true, cx, |settings, checked| settings.mode.automatic_spelling_correction = checked)),
            ))
            .child(field(
                "Completion:",
                v_flex()
                    .gap_1()
                    .child(note("Completion list includes:", cx))
                    .child(
                        v_flex()
                            .gap_1()
                            .pl_4()
                            .child(mode_switch("completion-standard", "Standard words", mode.completion_standard_words, false, cx, |settings, checked| settings.mode.completion_standard_words = checked))
                            .child(mode_switch("completion-document", "Words in document", mode.completion_document_words, false, cx, |settings, checked| settings.mode.completion_document_words = checked))
                            .child(mode_switch("completion-syntax", "Words defined in syntax", mode.completion_syntax_words, false, cx, |settings, checked| settings.mode.completion_syntax_words = checked)),
                    )
                    .child(mode_switch("automatic-completion", "Suggest completions while typing", mode.automatic_completion, true, cx, |settings, checked| settings.mode.automatic_completion = checked)),
            ));

        let modes = v_flex()
            .id("mode-list")
            .gap_0p5()
            .p_1()
            .w(rems(11.))
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
                    .label("General")
                    .accessibility_label("General mode"),
            )
            .child(
                Button::new("mode-code")
                    .ghost()
                    .small()
                    .w_full()
                    .selected(false)
                    .disabled(true)
                    .label("Code")
                    .tooltip("Per-syntax modes arrive with the syntax editor"),
            );

        v_flex()
            .gap_3()
            .child(note(
                "CotEditor keeps one set of options per syntax kind and one per syntax. \
                 SubText models the kind-level defaults; the per-syntax overrides are read \
                 from the same file CotEditor writes and arrive with the syntax editor.",
                cx,
            ))
            .child(h_flex().gap_4().items_start().child(modes).child(div().flex_1().min_w_0().child(options)))
            .into_any_element()
    }

    // ---- Format ----------------------------------------------------------

    /// The Format pane, in `FormatSettingsView.swift`'s order.
    fn render_format(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let endings = LineEnding::BASIC;
        let ending_index = endings
            .iter()
            .position(|ending| *ending == settings.default_line_ending)
            .unwrap_or(0);
        let ending_labels = endings.map(LineEnding::abbreviation);

        let line_endings = v_flex()
            .gap_1()
            .child(radio_row(
                "default-line-ending",
                &ending_labels,
                ending_index,
                cx.listener(|this, index: &usize, _, cx| {
                    let ending = LineEnding::BASIC[*index];
                    this.edit(cx, move |settings| settings.default_line_ending = ending);
                }),
            ))
            .child(note(
                format!(
                    "{} — {}",
                    endings[ending_index].abbreviation(),
                    endings[ending_index].platform_description()
                ),
                cx,
            ));

        let encodings: &'static [TextEncoding] = encoding::ALL;
        let current_encoding = settings.encoding().identifier();
        let encoding_index = encodings.iter().position(|e| e.identifier() == current_encoding);
        let encoding_items = encodings.iter().map(|e| SharedString::from(e.name)).collect::<Vec<_>>();
        let default_encoding = pick_list(
            "default-encoding",
            encoding_items,
            encoding_index,
            cx,
            cx.listener(move |this, index: &usize, _, cx| {
                let identifier = encodings.get(*index).map(|e| e.identifier());
                if let Some(identifier) = identifier {
                    this.edit(cx, move |settings| settings.default_encoding = identifier);
                }
            }),
        );

        let priorities = v_flex()
            .gap_1()
            .child(Button::new("encoding-priorities").small().label("Edit List…").disabled(true))
            .child(note(
                "Not implemented yet. CotEditor's encoding priority list decides which \
                 encodings the Open dialog tries; SubText tries UTF-8 only.",
                cx,
            ))
            .child(
                Checkbox::new("refer-to-encoding-tag")
                    .checked(settings.refer_to_encoding_tag)
                    .label("Refer to the encoding declaration in the document")
                    .on_click(cx.listener(|this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| settings.refer_to_encoding_tag = checked);
                    })),
            );

        let syntaxes = self
            .workspace
            .read_with(cx, |workspace, _| workspace.syntax_names())
            .unwrap_or_default();
        let mut syntax_items = vec![SharedString::from("None")];
        syntax_items.extend(syntaxes.iter().cloned().map(SharedString::from));
        let current_syntax = settings
            .default_syntax
            .as_ref()
            .and_then(|name| syntaxes.iter().position(|syntax| syntax == name).map(|index| index + 1))
            .or(Some(0));
        let default_syntax = pick_list(
            "default-syntax",
            syntax_items.clone(),
            current_syntax,
            cx,
            cx.listener(|this, index: &usize, _, cx| {
                let name = this
                    .workspace
                    .read_with(cx, |workspace, _| {
                        workspace.syntax_names().get(index.saturating_sub(1)).cloned()
                    })
                    .ok()
                    .flatten();
                let name = if *index == 0 { None } else { name };
                this.edit(cx, move |settings| settings.default_syntax = name);
            }),
        );

        let available = pick_list(
            "available-syntaxes",
            syntax_items,
            None,
            cx,
            cx.listener(|_, _, _, _| {}),
        );

        v_flex()
            .gap_6()
            .child(section("Default line endings", line_endings))
            .child(section("Default text encoding", default_encoding))
            .child(section("Encoding priorities", priorities))
            .child(section("Default syntax", default_syntax))
            .child(section(format!("Available syntaxes ({})", syntaxes.len()), available))
            .into_any_element()
    }

    // ---- Snippets --------------------------------------------------------

    /// The Snippets pane. CotEditor splits it into Command and File Drop tabs;
    /// SubText has no snippet store yet, so both tabs show their real empty
    /// state rather than invented content.
    fn render_snippets(&self, cx: &mut Context<Self>) -> AnyElement {
        let tabs = ["Command", "File Drop"]
            .into_iter()
            .map(|label| Tab::new().label(label))
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

        let (intro, empty) = if self.snippet_tab == 0 {
            (
                "Text to be inserted by a command in the menu or by keyboard shortcut:",
                "No snippets yet. CotEditor stores these under its application-support \
                 directory; SubText reads the same file, but the editor for it has not been built.",
            )
        } else {
            (
                "Files you can drop onto the editor to insert a path:",
                "No file-drop definitions yet. As with Command snippets, SubText reads \
                 CotEditor's file but the editor for it has not been built.",
            )
        };

        let table = v_flex()
            .id("snippet-table")
            .gap_0p5()
            .p_2()
            .h(rems(20.))
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(note(empty, cx));

        v_flex()
            .gap_3()
            .child(tab_bar)
            .child(note(intro, cx))
            .child(table)
            .child(
                h_flex()
                    .gap_2()
                    .child(Button::new("snippet-add").small().label("Add").disabled(true))
                    .child(Button::new("snippet-remove").small().label("Remove").disabled(true)),
            )
            .into_any_element()
    }

    // ---- Shortcuts -------------------------------------------------------

    /// The Shortcuts pane: every menu command's key equivalent, grouped by the
    /// menu it lives in. The table is the same one the menu bar is generated
    /// from, so the two cannot disagree.
    fn render_shortcuts(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut groups: Vec<(&str, Vec<(&str, &str)>)> = Vec::new();
        for (path, shortcut, _, _) in keymap::SHORTCUTS.iter() {
            let mut parts = path.split(" > ");
            let menu = parts.next().unwrap_or(path);
            let command = parts.next().unwrap_or(path);
            match groups.last_mut() {
                Some((name, rows)) if *name == menu => rows.push((command, shortcut)),
                _ => groups.push((menu, vec![(command, shortcut)])),
            }
        }

        let mut list = v_flex().id("shortcut-list").gap_3().p_2();
        for (menu, rows) in &groups {
            let mut column = v_flex().gap_0p5().child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child((*menu).to_string()),
            );
            for (command, shortcut) in rows {
                column = column.child(
                    h_flex()
                        .justify_between()
                        .gap_4()
                        .child(div().text_sm().truncate().child((*command).to_string()))
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
                "To change a shortcut, use the menu bar's own key equivalents. ⌘ maps to \
                 Control on Windows and Linux, where Command does not exist.",
                cx,
            ))
            .child(
                v_flex()
                    .id("shortcut-scroll")
                    .max_h(rems(28.))
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
                            .label("Restore Defaults")
                            .disabled(true)
                            .tooltip("Nothing has been customised yet"),
                    )
                    .child(note(
                        format!("{} shortcuts, all at their CotEditor defaults", keymap::SHORTCUTS.len()),
                        cx,
                    )),
            )
            .into_any_element()
    }

    // ---- Integration -----------------------------------------------------

    /// The Integration pane, in `IntegrationSettingsView.swift`'s order.
    fn render_integration(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let command_line = v_flex()
            .gap_1()
            .child(Button::new("learn-more-cli").small().label("Learn More…").disabled(true))
            .child(note(
                "With the `cot` command-line tool you can open files in CotEditor from a \
                 terminal. SubText installs `subtext` for the same purpose: put it on your \
                 PATH and run `subtext FILE`.",
                cx,
            ));

        let remote = v_flex()
            .gap_1()
            .child(
                Checkbox::new("enables-remote-editing")
                    .checked(settings.enables_remote_editing)
                    .label("Accept rmate connections")
                    .on_click(cx.listener(|this, checked, _, cx| {
                        let checked = *checked;
                        this.edit(cx, move |settings| settings.enables_remote_editing = checked);
                    })),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .pl_4()
                    .child(div().text_sm().child("Port:"))
                    .child(stepper(
                        "remote-editing-port",
                        format!("{}", settings.remote_editing_port),
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
            .child(note(
                "The setting is stored, but the rmate listener is not implemented yet: no \
                 socket is opened and no remote file can be received.",
                cx,
            ));

        v_flex()
            .gap_6()
            .child(section("Command-line tool", command_line))
            .child(section("Remote editing", remote))
            .into_any_element()
    }
}

// ---- shared pieces -------------------------------------------------------

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
    let label = label.into();
    let has_label = !label.is_empty();

    h_flex()
        .items_start()
        .gap_4()
        .when(has_label, |this| {
            this.child(
                div()
                    .w(rems(LABEL_COLUMN))
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
    let minus = cx.listener(move |this, _, _, cx| on_decrement(this, cx));
    let plus = cx.listener(move |this, _, _, cx| on_increment(this, cx));

    h_flex()
        .gap_1()
        .items_center()
        .child(
            Button::new(ElementId::Name(SharedString::from(format!("{id}-decrement"))))
                .small()
                .label("−")
                .accessibility_label("Decrease")
                .on_click(minus),
        )
        .child(div().min_w(rems(4.)).text_sm().text_center().child(value))
        .child(
            Button::new(ElementId::Name(SharedString::from(format!("{id}-increment"))))
                .small()
                .label("+")
                .accessibility_label("Increase")
                .on_click(plus),
        )
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
    RadioGroup::horizontal(id)
        .selected_index(Some(selected))
        .children(options.iter().copied())
        .on_click(on_click)
        .into_any_element()
}

/// The click handler a `pick_list` row runs, shared across the rows.
type ListClick = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// A bordered, scrollable list of mutually exclusive choices.
///
/// Used where CotEditor uses a list rather than a popup — the syntax list, the
/// theme list, the encoding list. Each row is a `Button` in its `selected`
/// state, so selection is drawn by the design system and announced by the
/// accessibility layer.
fn pick_list(
    id: &'static str,
    items: Vec<SharedString>,
    selected: Option<usize>,
    cx: &App,
    on_click: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let on_click: ListClick = Rc::new(on_click);

    let mut list = v_flex()
        .id(id)
        .gap_0p5()
        .p_1()
        .max_h(rems(18.))
        .overflow_y_scroll()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border);

    for (index, label) in items.into_iter().enumerate() {
        let on_click = on_click.clone();
        list = list.child(
            // The list is not reorderable, so the index is a stable identity
            // for the row; the name keeps it from colliding with other lists.
            Button::new(ElementId::NamedInteger(SharedString::from(id), index as u64))
                .ghost()
                .small()
                .w_full()
                .selected(selected == Some(index))
                .label(label)
                .on_click(move |_, window, cx| on_click(&index, window, cx)),
        );
    }
    list.into_any_element()
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
        .tooltip(if appkit_only {
            "Provided by the system text engine, which SubText does not use"
        } else {
            ""
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

fn window_size_label(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{value:.0}"),
        None => "Auto".to_string(),
    }
}
