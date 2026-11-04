//! UI integration tests: real views, real windows, real input.
//!
//! The whole file is gated: without the feature the harness does not exist, and
//! `cargo test --workspace` would fail to compile it.
#![cfg(feature = "ui-tests")]
//!
//! Run with
//!
//! ```text
//! cargo test -p subtext-app --features ui-tests --test ui
//! ```
//!
//! The feature is off by default because turning it on pulls `gpui-fast-*`
//! into the dependency graph (see the note in `Cargo.toml`), which is a lot to
//! build for the ordinary `cargo test --workspace` run.
//!
//! These tests exist because the alternative — photographing the app and
//! eyeballing the picture — is how several wrong conclusions got into
//! `docs/PLAN.md` (§22.23, §22.24). A test that clicks the button and reads
//! the result cannot be fooled by a stale window.

use gpui_kit::component::button::Button;
use gpui_kit::test::{TestSupportExt as _, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, Modifiers, MouseButton,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, TestAppContext,
    Window, WindowBounds, WindowOptions, div, point, px, size,
};

/// A minimal view with one button and one label, so this file can prove the
/// harness itself runs before anything is asserted about the application.
struct Counter {
    clicks: usize,
}

impl Render for Counter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_4()
            .child(
                Button::new("bump")
                    .label("Bump")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clicks += 1;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("count")
                    .aria_label(self.clicks.to_string())
                    .test_support()
                    .child(self.clicks.to_string()),
            )
    }
}

#[gpui_kit::test]
fn the_harness_itself_clicks_and_reads_back(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let handle = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(480.), px(320.)),
                ))),
                ..Default::default()
            },
            cx,
            |_window, cx| cx.new(|_| Counter { clicks: 0 }),
        )
        .expect("open the test window")
        .0
    });

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("count").label(), Some("0"));

        window.click("bump", cx);
        assert_eq!(
            window.find("count").label(),
            Some("1"),
            "the click should have reached the handler"
        );
    })
    .expect("run the frame");
}

// ---- the application's own dialogs ---------------------------------------

/// A real `Workspace` in a headless window, with one document open.
///
/// Every dialog test needs the same three things: the app initialised, a
/// window, and a document whose text can be compared afterwards. They are in
/// one place because the setup has two traps — `open_path` reads the file off
/// the main thread (so the executor has to be driven before the document
/// exists), and saving the pair of them here keeps each test about its dialog.
fn open_workspace_with_document(
    cx: &mut TestAppContext,
    name: &str,
    text: &str,
) -> (
    gpui_kit::AnyWindowHandle,
    gpui_kit::Entity<subtext_app::workspace::Workspace>,
) {
    open_workspace_with_document_sized(cx, name, text, size(px(900.), px(600.)))
}

/// The same window at a size the caller picks.
///
/// One test needs it: the toolbar palette is about 640 points tall, so a
/// 600-point window cannot show its footer without scrolling. The workspace
/// caps and scrolls the palette — see the overlay in `workspace.rs` — but the
/// test that clicks *Done* is about the click, not about scrolling, so it
/// opens a window the whole sheet fits in, the way a document window normally
/// is.
fn open_workspace_with_document_sized(
    cx: &mut TestAppContext,
    name: &str,
    text: &str,
    window_size: gpui_kit::Size<gpui_kit::Pixels>,
) -> (
    gpui_kit::AnyWindowHandle,
    gpui_kit::Entity<subtext_app::workspace::Workspace>,
) {
    use subtext_app::settings::Settings;
    use subtext_app::workspace::Workspace;
    use subtext_app::{load_syntaxes, load_themes};

    cx.update(gpui_kit::init);
    // Keep every settings read and write inside the test's own folder: the
    // workspaces these tests open save `Settings.json` (a resize does it
    // outright), and that file is the developer's own until we say otherwise.
    // A fresh folder per run: the config directory is read by `Settings::load`,
    // so a leftover file from the last run would decide what this one starts
    // from.
    subtext_app::resources::use_config_root(
        std::env::temp_dir().join(format!("subtext-ui-tests-config-{}", std::process::id())),
    );

    let dir = std::env::temp_dir().join(format!("subtext-ui-tests-{name}"));
    std::fs::create_dir_all(&dir).expect("create the test folder");
    let path = dir.join("a.txt");
    std::fs::write(&path, text).expect("write the test document");

    let (window, workspace) = cx.update(move |cx| {
        let resources = subtext_app::resources::Resources::discover();
        let syntaxes = load_syntaxes(resources.as_ref());
        let themes = load_themes(resources.as_ref());
        let settings = Settings::load();
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    window_size,
                ))),
                ..Default::default()
            },
            cx,
            move |window, cx| cx.new(|cx| Workspace::new(syntaxes, themes, settings, window, cx)),
        )
        .expect("open the test window")
    });

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        workspace.update(cx, |workspace, cx| {
            workspace.open_path(path.clone(), window, cx);
        });
        window.render_frame(cx);
    })
    .expect("open the document");
    cx.run_until_parked();

    (window, workspace)
}

/// Open a workspace the way `lib.rs` does at launch: the paths, if any, are
/// handed to the constructor, which creates no starter document of its own.
fn open_workspace_at_launch(
    cx: &mut TestAppContext,
    paths: Vec<std::path::PathBuf>,
) -> (
    gpui_kit::AnyWindowHandle,
    gpui_kit::Entity<subtext_app::workspace::Workspace>,
) {
    use subtext_app::settings::Settings;
    use subtext_app::workspace::Workspace;
    use subtext_app::{load_syntaxes, load_themes};

    cx.update(gpui_kit::init);
    subtext_app::resources::use_config_root(
        std::env::temp_dir().join(format!("subtext-ui-tests-config-{}", std::process::id())),
    );

    cx.update(move |cx| {
        let resources = subtext_app::resources::Resources::discover();
        let syntaxes = load_syntaxes(resources.as_ref());
        let themes = load_themes(resources.as_ref());
        let settings = Settings::load();
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(900.), px(600.)),
                ))),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| {
                    Workspace::new_with_paths(syntaxes, themes, settings, paths, window, cx)
                })
            },
        )
        .expect("open the test window")
    })
}

/// A dialog's button is a button, not a bar across the dialog.
///
/// This is the regression §22.24 fixed: `DialogClose` renders `div().size_full()`,
/// so a dialog footer built from one produced a control the full width of the
/// dialog. A number is what makes that visible in a test — the id is there
/// either way.
const NARROW: gpui_kit::Pixels = px(200.);

/// The Custom Surround dialog, which §22.24 rewrote.
#[gpui_kit::test]
fn custom_surround_shows_cancel_and_ok_and_ok_needs_a_begin(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "surround", "abc");

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.surround_sheet(window, cx);
        });
        // The dialog is drawn a frame after it is opened: the first frame
        // installs it, the second lays it out.
        window.render_frame(cx);
        window.render_frame(cx);

        assert!(
            window.try_find("surround-begin").is_some(),
            "the Custom Surround dialog should be open"
        );
        for id in ["surround-cancel", "surround-apply"] {
            let button = window
                .try_find(id)
                .unwrap_or_else(|| panic!("{id} is missing"));
            assert!(button.visible(), "{id} should be visible");
            assert!(
                button.bounds().size.width < NARROW,
                "{id} is {} wide — it should be a button, not a bar across the dialog",
                button.bounds().size.width
            );
        }

        // An empty `Begin:` has nothing to surround with, so the OK must refuse.
        window.click("surround-apply", cx);
        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some("abc".to_owned()),
            "OK with an empty Begin: should do nothing"
        );

        // Fill it in; the frame that follows the keystroke is the one that
        // rebuilds the OK out of the sheet's state.
        window.click("surround-begin", cx);
        window.input("«", cx);
        assert_eq!(
            window.find("surround-begin").value(),
            Some("«"),
            "the Begin: field should hold what was typed"
        );
        window.render_frame(cx);
        window.click("surround-apply", cx);
        assert!(
            window.try_find("surround-begin").is_none(),
            "confirming should have dismissed the sheet"
        );
    })
    .expect("confirm");
}

/// The *Multiple Replace* dialog's `Done`, which §22.24 also fixed.
///
/// Its footer held a lone `DialogClose`, which stretched across the dialog and
/// read as a divider rather than a button. Nothing else about that dialog is
/// asserted here; the point is the width.
#[gpui_kit::test]
fn multiple_replace_done_is_a_button_not_a_bar(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "replace", "abc\n");

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.multiple_replace_dialog(window, cx);
        });
        window.render_frame(cx);
        window.render_frame(cx);

        let done = window
            .try_find("multiple-replace-done")
            .expect("the dialog should show a Done button");
        assert!(done.visible(), "Done should be visible");
        assert!(
            done.bounds().size.width < NARROW,
            "Done is {} wide — it should be a button, not a bar across the dialog",
            done.bounds().size.width
        );
    })
    .expect("render the dialog");
}

/// Two settings panes whose buttons were once stretched across the window.
///
/// §22.19 fixed both: a button placed straight into a `v_flex` is stretched on
/// the cross axis, so *Manage Warnings…* and *Edit List…* were as wide as the
/// pane. The panes are a second window over the same workspace, which is how
/// the app opens them.
#[gpui_kit::test]
fn settings_buttons_keep_their_width(cx: &mut TestAppContext) {
    use subtext_app::settings::SettingsPane;
    use subtext_app::settings_window::SettingsWindow;

    let (_document, workspace) = open_workspace_with_document(cx, "settings", "abc");

    let (settings, settings_view) = cx.update(|cx| {
        let weak = workspace.downgrade();
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(760.), px(580.)),
                ))),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| SettingsWindow::new(weak, Some(SettingsPane::General), window, cx))
            },
        )
        .expect("open the settings window")
    });

    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);

        let manage = window
            .try_find("manage-warnings")
            .expect("the General pane should show Manage Warnings…");
        assert!(
            manage.bounds().size.width < NARROW,
            "Manage Warnings… is {} wide — it should not fill the pane",
            manage.bounds().size.width
        );

        // The Format pane's *Edit List…* is the other one that was stretched.
        settings_view.update(cx, |view, cx| view.show(SettingsPane::Format, cx));
        window.render_frame(cx);
        window.render_frame(cx);

        let edit = window
            .try_find("encoding-priorities")
            .expect("the Format pane should show Edit List…");
        assert!(
            edit.bounds().size.width < NARROW,
            "Edit List… is {} wide — it should not fill the pane",
            edit.bounds().size.width
        );
    })
    .expect("render the settings window");
}

/// The Format pane fits all five of its rows in the window.
///
/// The pane used to render the encoding and syntax choices as bordered inline
/// lists, several hundred points tall together, which pushed *Encoding
/// priorities*, *Default syntax* and *Available syntaxes* below the fold of a
/// 580 pt window. Upstream is a `Grid` of popup buttons and one fixed
/// 260 × 140 list, so every row is visible at once; this is the test that says
/// so, because a screenshot taken at the right moment can still miss a row that
/// scrolled away.
#[gpui_kit::test]
fn format_pane_rows_fit_without_scrolling(cx: &mut TestAppContext) {
    use subtext_app::settings::SettingsPane;
    use subtext_app::settings_window::SettingsWindow;

    let (_document, workspace) = open_workspace_with_document(cx, "format", "abc");

    let settings = cx.update(|cx| {
        let weak = workspace.downgrade();
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(760.), px(580.)),
                ))),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| SettingsWindow::new(weak, Some(SettingsPane::Format), window, cx))
            },
        )
        .expect("open the settings window")
        .0
    });

    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);

        for id in [
            "default-line-ending",
            "default-encoding",
            "encoding-priorities",
            "default-syntax",
            "available-syntaxes",
        ] {
            let row = window
                .try_find(id)
                .unwrap_or_else(|| panic!("the Format pane should show {id}"));
            let bounds = row.bounds();
            assert!(
                bounds.origin.y + bounds.size.height <= px(580.),
                "{id} ends at {} — it is below the fold",
                bounds.origin.y + bounds.size.height
            );
        }

        // Upstream caps the three pickers at 260 pt and gives the syntax list
        // 260 × 140; an inline list cannot be mistaken for one of these.
        for id in ["default-line-ending", "default-encoding", "default-syntax"] {
            let bounds = window.try_find(id).expect(id).bounds();
            assert!(
                (bounds.size.width - px(260.)).abs() < px(4.),
                "{id} is {} wide — a picker should be 260 wide",
                bounds.size.width
            );
        }
        let list = window
            .try_find("available-syntaxes")
            .expect("the syntax list");
        assert!(
            (list.bounds().size.width - px(260.)).abs() < px(4.)
                && (list.bounds().size.height - px(140.)).abs() < px(4.),
            "the syntax list is {} × {}, not 260 × 140",
            list.bounds().size.width,
            list.bounds().size.height
        );
    })
    .expect("render the Format pane");
}

/// The Mode pane's switches are one line tall.
///
/// §22.19 left this as a gap: *Smart copy/paste* wrapped onto two lines in a
/// 760 px window, and the reason was arithmetic — the options column sits
/// beside the mode list, so a 12 rem label column left about 150 px for the
/// switches. The label column there is 7 rem now, and this is the number that
/// says so. A screenshot showed it too, but only after someone thought to look.
///
/// *Automatically insert closing brackets and quotes* is allowed to wrap — it
/// is longer than the column can ever be, and upstream's `Toggle` says
/// `fixedSize(horizontal: false, vertical: true)` for exactly that reason — so
/// it is not in this list.
#[gpui_kit::test]
fn mode_pane_switches_stay_on_one_line(cx: &mut TestAppContext) {
    use subtext_app::settings::SettingsPane;
    use subtext_app::settings_window::SettingsWindow;

    let (_document, workspace) = open_workspace_with_document(cx, "mode", "abc");

    let window = cx.update(|cx| {
        let weak = workspace.downgrade();
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(760.), px(580.)),
                ))),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| SettingsWindow::new(weak, Some(SettingsPane::Mode), window, cx))
            },
        )
        .expect("open the settings window")
        .0
    });

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);

        for id in ["smart-insert-delete", "smart-quotes", "smart-dashes"] {
            let row = window
                .try_find(id)
                .unwrap_or_else(|| panic!("the Mode pane should show {id}"));
            assert!(
                row.bounds().size.height < px(26.),
                "{id} is {} tall — its label wrapped onto a second line",
                row.bounds().size.height
            );
        }
    })
    .expect("render the Mode pane");
}

/// The Mode pane's *Use custom settings* switch arms its own indentation rows.
///
/// Upstream's `ModeIndentOptionsView` disables the inner group while the switch
/// is off, and a disabled control must also be inert when something clicks it —
/// otherwise the pane would write a mode override the user never asked for.
/// Turning the switch on seeds that override from the Edit pane's defaults
/// (`defaultExpandsTab` / `defaultIndentWidth`), not from zero.
#[gpui_kit::test]
fn mode_indentation_follows_its_custom_switch(cx: &mut TestAppContext) {
    use subtext_app::settings::{ModeIndent, SettingsPane};
    use subtext_app::settings_window::SettingsWindow;

    let (_document, workspace) = open_workspace_with_document(cx, "mode-indent", "abc");

    // The Edit pane's defaults, which the switch should copy.
    cx.update(|cx| {
        workspace.update(cx, |workspace, _| {
            workspace.settings.set_indentation(true, 3);
        })
    });

    let settings = cx.update(|cx| {
        let weak = workspace.downgrade();
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    // Tall enough for the whole Mode pane. The settings window
                    // fits itself to the pane (see `on_pane_measured`), but the
                    // test harness measures visibility against the viewport it
                    // was opened with, and that viewport does not follow
                    // `Window::resize`; 760 × 760 is what a display shows
                    // once the window has fitted itself.
                    size(px(760.), px(760.)),
                ))),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| SettingsWindow::new(weak, Some(SettingsPane::Mode), window, cx))
            },
        )
        .expect("open the settings window")
        .0
    });

    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);

        // Off: the width stepper is disabled, so a click must not create an
        // override. This is the gesture that proves the dimming is not only
        // paint.
        assert_eq!(workspace.read(cx).settings.mode.indent, None);
        window.click("mode-indent-width-increment", cx);
        window.render_frame(cx);
        assert_eq!(
            workspace.read(cx).settings.mode.indent,
            None,
            "a disabled stepper should not write a mode override"
        );

        // On: the override appears, seeded from the Edit pane.
        window.click("mode-custom-indentation", cx);
        window.render_frame(cx);
        assert_eq!(
            workspace.read(cx).settings.mode.indent,
            Some(ModeIndent {
                expands_tab: true,
                width: 3
            }),
            "the switch should seed the override from the Edit pane's defaults"
        );

        // Now the stepper is live, and it edits the override.
        window.click("mode-indent-width-increment", cx);
        window.render_frame(cx);
        assert_eq!(
            workspace.read(cx).settings.mode.indent.map(|i| i.width),
            Some(4)
        );

        // Back off: the override is gone and the base defaults take over.
        window.click("mode-custom-indentation", cx);
        window.render_frame(cx);
        assert_eq!(workspace.read(cx).settings.mode.indent, None);
        assert_eq!(workspace.read(cx).settings.indent_unit(), "   ");
    })
    .expect("drive the Mode pane's indentation rows");
}

/// Moving the opacity slider in the settings window reaches the document.
///
/// The settings window is a second window, and it can only reach the
/// workspace through a weak handle. `WeakEntity::update_in` looks the window
/// up from the last window that *read* the entity — the settings window while
/// its pane renders — and a window refuses its own handle while it dispatches
/// the event that led here, so the update was silently dropped: the slider
/// moved, the percent field followed, and the document window (and the saved
/// settings) stayed where they were. The workspace now keeps its own window
/// handle and every pane edit is addressed to it; this is the gesture that
/// says so.
#[gpui_kit::test]
fn the_opacity_slider_moves_the_document_window(cx: &mut TestAppContext) {
    use subtext_app::settings::SettingsPane;
    use subtext_app::settings_window::SettingsWindow;

    let (_document, workspace) = open_workspace_with_document(cx, "opacity", "abc");

    let settings = cx.update(|cx| {
        let weak = workspace.downgrade();
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(760.), px(580.)),
                ))),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| SettingsWindow::new(weak, Some(SettingsPane::Appearance), window, cx))
            },
        )
        .expect("open the settings window")
        .0
    });

    let before = cx.update(|cx| workspace.read(cx).settings.appearance.window_alpha);
    // The slider spans 0.2…1.0. Click a quarter of the way along, on whichever
    // side of the middle the current value is not: the point has to produce a
    // different value, or the assertion below cannot tell the fix from the bug.
    let across = if before >= 0.6 { 0.25 } else { 0.75 };

    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);

        let slider = window
            .try_find("slider-bar-container")
            .expect("the Appearance pane should show the opacity slider");
        window.click_at(
            "slider-bar-container",
            point(
                slider.bounds().size.width * across,
                slider.bounds().size.height / 2.,
            ),
            cx,
        );
        window.render_frame(cx);
    })
    .expect("click the slider");

    let after = cx.update(|cx| workspace.read(cx).settings.appearance.window_alpha);
    let expected = 0.2 + 0.8 * across;
    assert!(
        (after - expected).abs() < 0.05,
        "the slider should have set the opacity to about {expected}, got {after}"
    );
    assert_ne!(
        after, before,
        "the document window's settings should have received the new opacity"
    );
}

/// Escape dismisses the sheet.
///
/// The design guide states it as a rule ("Escape dismisses the topmost
/// surface"), and §22.24 rewrote this dialog's buttons without ever checking
/// the keyboard way out. The sheet's Cancel and Escape both go through
/// `close_dialog`, but they are different code paths — the button is a click
/// handler, Escape is a dialog action.
#[gpui_kit::test]
fn escape_dismisses_the_custom_surround_sheet(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "escape", "abc");

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.surround_sheet(window, cx);
        });
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            window.try_find("surround-begin").is_some(),
            "the sheet should be open before Escape"
        );

        // No click first: the sheet takes the keyboard when it opens (§22.31).
        window.press("escape", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            window.try_find("surround-begin").is_none(),
            "Escape should have dismissed the sheet"
        );
    })
    .expect("dismiss with Escape");
}

/// The colour panel's two keyboard ways out, both without a click first.
///
/// Enter inserts and Escape cancels — the two keys a sheet is expected to
/// answer. Both had to be taken on the way down (§22.30, §22.31); before that
/// the code field swallowed them.
#[gpui_kit::test]
fn the_color_code_sheet_answers_enter_and_escape(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "keys-color", "#ff0000");

    cx.update_window(window, |_, window, cx| {
        for key in ["enter", "escape"] {
            workspace.update(cx, |workspace, cx| {
                workspace.color_code_dialog(window, cx);
            });
            window.render_frame(cx);
            window.render_frame(cx);
            assert!(
                window.try_find("color-insert").is_some(),
                "the sheet should be open before {key}"
            );

            // No click first: the panel takes the keyboard when it opens.
            window.press(key, cx);
            window.render_frame(cx);
            window.render_frame(cx);
            assert!(
                window.try_find("color-insert").is_none(),
                "{key} should have dismissed the sheet"
            );
        }
    })
    .expect("keys");
}

/// The sheet opens with the caret in `Begin:`.
///
/// A dialog that opens without focus leaves the keyboard with the editor
/// behind it: typing goes into the document and Escape goes nowhere. Upstream's
/// sheet puts the caret in the first field the moment it appears, and so does
/// this — the assertion is that nothing has to be clicked first.
#[gpui_kit::test]
fn the_surround_sheet_opens_with_the_caret_in_begin(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "caret", "abc");

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |workspace, cx| workspace.surround_sheet(window, cx));
        window.render_frame(cx);
        window.render_frame(cx);

        // Nothing is clicked: the keystroke has to land in the sheet's field.
        window.input("«", cx);
        assert_eq!(
            window.find("surround-begin").value(),
            Some("«"),
            "the sheet should take the keyboard when it opens"
        );

        // And Enter still confirms: the field has focus, but the dialog's
        // confirm action reaches `on_ok` all the same.
        window.press("enter", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            window.try_find("surround-begin").is_none(),
            "Enter should confirm the sheet while the caret is in Begin:"
        );
    })
    .expect("open with focus");
}

/// The sort sheet answers Enter and Escape without a click first.
///
/// Same family as §22.30/§22.31: a sheet with text fields in it loses the
/// keyboard to the editor unless something inside takes it.
#[gpui_kit::test]
fn the_sort_sheet_answers_enter_and_escape(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "sort", "b\na\nc\n");

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.pattern_sort_dialog(window, cx)
        });
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            window.try_find("sort-apply").is_some(),
            "the sort sheet should be open"
        );

        window.press("escape", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            window.try_find("sort-apply").is_none(),
            "Escape should close the sort sheet without a click first"
        );

        // Enter sorts and closes, from the same place the caret opens in.
        workspace.update(cx, |workspace, cx| {
            workspace.pattern_sort_dialog(window, cx)
        });
        window.render_frame(cx);
        window.render_frame(cx);
        window.press("enter", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            window.try_find("sort-apply").is_none(),
            "Enter should sort and close"
        );
    })
    .expect("keys");
}

/// ⌘F puts the caret in the find field.
///
/// The find bar is a window of its own, not a dialog, so the test has to find
/// that window: it is the last one the app opened after the document.
#[gpui_kit::test]
fn opening_find_puts_the_caret_in_the_query_field(cx: &mut TestAppContext) {
    let (document, workspace) = open_workspace_with_document(cx, "find", "alpha beta alpha\n");

    cx.update_window(document, |_, window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.show_find(false, window, cx);
        });
    })
    .expect("open the find bar");
    cx.run_until_parked();

    let bar = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != document)
            .expect("the find bar should be a window of its own")
    });

    cx.update_window(bar, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);

        let field = window
            .try_find("find-query")
            .expect("the find bar should be showing its query field");
        assert!(field.visible(), "the query field should be visible");

        window.input("alpha", cx);
        assert_eq!(
            window.find("find-query").value(),
            Some("alpha"),
            "typing after ⌘F should land in the query field"
        );

        // Escape closes the bar (§22.33), but that is not asserted here: the
        // handler calls `remove_window`, and the harness neither tears down the
        // element tree nor drops the window from `cx.windows()` when it does —
        // a temporary probe showed the handler running and both staying put. A
        // test that cannot see the effect would be worse than none.
    })
    .expect("find");
}

/// Enter in the find field goes to the next match.
///
/// CotEditor's find field answers Return with *find next*; a field that does
/// nothing is a field the user has to leave to reach the button. Pressed twice,
/// so the assertion is about the step rather than about the first search.
#[gpui_kit::test]
fn enter_in_the_find_field_goes_to_the_next_match(cx: &mut TestAppContext) {
    let (document, workspace) =
        open_workspace_with_document(cx, "find-enter", "alpha beta alpha\n");

    cx.update_window(document, |_, window, cx| {
        workspace.update(cx, |workspace, cx| workspace.show_find(false, window, cx));
    })
    .expect("open the find bar");
    cx.run_until_parked();

    let bar = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != document)
            .expect("the find bar should be a window of its own")
    });

    cx.update_window(bar, |_, window, cx| {
        window.render_frame(cx);
        window.input("alpha", cx);
        window.render_frame(cx);

        window.press("enter", cx);
        window.render_frame(cx);
        let first = workspace.read_with(cx, |workspace, _| workspace.find_status());
        assert_eq!(first.0, 2, "two matches in this document ({first:?})");

        window.press("enter", cx);
        window.render_frame(cx);
        let second = workspace.read_with(cx, |workspace, _| workspace.find_status());
        assert_eq!(second.0, 2, "still two matches ({second:?})");
        assert_ne!(
            first.1, second.1,
            "the second Return should move to the next match ({first:?} → {second:?})"
        );
    })
    .expect("return");
}

/// Enter in the replacement field replaces the match and moves on.
///
/// The companion to §22.34: Return in the *other* field means "replace and
/// find next", which is what the button beside it does. Keeping them apart is
/// why the handler checks which field holds the caret.
#[gpui_kit::test]
fn enter_in_the_replacement_field_replaces_and_finds_next(cx: &mut TestAppContext) {
    let (document, workspace) =
        open_workspace_with_document(cx, "find-replace", "alpha beta alpha\n");

    cx.update_window(document, |_, window, cx| {
        // `show_find(true)` focuses the *replacement* field, so the query is
        // typed with the bar opened the plain way and the caret moved after.
        workspace.update(cx, |workspace, cx| workspace.show_find(false, window, cx));
    })
    .expect("open the find bar");
    cx.run_until_parked();

    let bar = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != document)
            .expect("the find bar should be a window of its own")
    });

    cx.update_window(bar, |_, window, cx| {
        window.render_frame(cx);
        window.input("alpha", cx);
        window.render_frame(cx);

        window.click("find-replacement", cx);
        window.input("beta", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .expect("replace");

    cx.run_until_parked();
    let text = cx.update(|cx| workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)));
    assert_eq!(
        text.as_deref(),
        Some("beta beta alpha\n"),
        "Return in the replacement field should have replaced the first match"
    );
}

/// Typing in the editor changes the document.
///
/// The most basic thing a text editor does, and not asserted anywhere: every
/// earlier test read the buffer after a command. It also covers the same
/// question the dialogs had (§22.31) — does the document window take the
/// keyboard when it opens, or must the user click first?
#[gpui_kit::test]
fn typing_lands_in_the_editor_without_a_click(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "typing", "abc");

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.input("x", cx);
        window.render_frame(cx);

        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some("xabc".to_owned()),
            "the opening document should take the keyboard"
        );
    })
    .expect("type");
}

/// Switching documents takes the caret with it.
///
/// Without it the keyboard stays with the editor of the document just left, and
/// the next keystroke edits a document the user cannot see — the worst kind of
/// bug to have in an editor.
#[gpui_kit::test]
fn switching_documents_moves_the_caret_with_the_document(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "tabs", "first");

    let second = std::env::temp_dir()
        .join("subtext-ui-tests-tabs")
        .join("b.txt");
    std::fs::write(&second, "second").expect("write the second document");

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.open_path(second.clone(), window, cx)
        });
    })
    .expect("open the second document");
    cx.run_until_parked();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some("second".to_owned()),
            "the second document should be in front"
        );

        // Back to the first, then type: the text has to land *there*.
        workspace.update(cx, |workspace, cx| workspace.next_tab(false, window, cx));
        window.render_frame(cx);
        window.input("Q", cx);
        window.render_frame(cx);

        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some("Qfirst".to_owned()),
            "typing after a tab switch should land in the document now in front"
        );
    })
    .expect("switch");
}

/// New Window opens a document window of its own.
///
/// CotEditor's ⌘N and ⇧⌘N both produce a window, because every document has
/// one; SubText keeps several documents in one window, so this is the command
/// that has to make a window rather than a tab. The new window is blank and
/// the original document is left alone.
#[gpui_kit::test]
fn new_window_opens_a_second_document_window(cx: &mut TestAppContext) {
    let (document, workspace) = open_workspace_with_document(cx, "new-window", "first");
    let before: Vec<_> = cx.update(|cx| cx.windows());

    cx.update_window(document, |_, window, cx| {
        workspace.update(cx, |workspace, cx| workspace.open_new_window(window, cx));
        window.render_frame(cx);
    })
    .expect("open a new window");
    cx.run_until_parked();

    let after: Vec<_> = cx.update(|cx| cx.windows());
    assert_eq!(
        after.len(),
        before.len() + 1,
        "New Window should add one window, not a tab"
    );

    let opened = after
        .into_iter()
        .find(|handle| !before.contains(handle))
        .expect("the new window should be among the open ones");

    // Rendering the new window proves it was built: a workspace that failed to
    // initialise would not draw a frame.
    cx.update_window(opened, |_, window, cx| {
        window.render_frame(cx);
    })
    .expect("render the new window");

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.document_count()),
        2,
        "the original window keeps its starter document and its file"
    );
}

/// Files on the command line are the only tabs that open.
///
/// The launch path used to create an untitled document and then open the file
/// beside it, leaving `subtext notes.txt` with two tabs. CotEditor's transient
/// untitled document is dropped on open for the same reason.
#[gpui_kit::test]
fn command_line_paths_do_not_leave_a_starter_tab(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join("subtext-ui-tests-launch-path");
    std::fs::create_dir_all(&dir).expect("create the test folder");
    let path = dir.join("notes.txt");
    std::fs::write(&path, "notes").expect("write the test document");

    let (_, workspace) = open_workspace_at_launch(cx, vec![path]);
    cx.run_until_parked();

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.document_count()),
        1,
        "the file named on the command line should be the only tab"
    );

    // A path that cannot be read leaves no document behind, so the fallback
    // must supply the untitled one; an empty window has nothing to draw.
    let missing = dir.join("does-not-exist.txt");
    let (_, fallback) = open_workspace_at_launch(cx, vec![missing]);
    cx.run_until_parked();

    assert_eq!(
        fallback.read_with(cx, |workspace, _| workspace.document_count()),
        1,
        "an unreadable launch path should fall back to one untitled document"
    );
}

/// Select Column Down grows a real multi-cursor run.
///
/// CotEditor adds an insertion point below the last caret at the same column;
/// typing then lands on every line. This is the part a single-selection editor
/// could not do, so the assertion is the text after typing, not a selection
/// count.
#[gpui_kit::test]
fn select_column_down_adds_a_caret_on_each_line(cx: &mut TestAppContext) {
    let (window, workspace) =
        open_workspace_with_document(cx, "column-select", "alpha\nbeta\ngamma");

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let editor = workspace
            .read_with(cx, |workspace, _| workspace.active_editor().cloned())
            .expect("the opening document should have an editor");
        // The caret after "al", which is the column every new caret takes.
        editor.update(cx, |state, cx| state.set_selected_range(2..2, cx));

        workspace.update(cx, |workspace, cx| {
            workspace.select_column(false, cx);
            workspace.select_column(false, cx);
        });
        window.render_frame(cx);
        window.input("X", cx);
        window.render_frame(cx);

        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some("alXpha\nbeXta\ngaXmma".to_owned()),
            "one keystroke should land at the column on all three lines"
        );
    })
    .expect("select a column");
}

/// A column run lines carets up by what is painted, not by character count.
///
/// Two Chinese characters are four columns wide, so the caret that starts
/// after them must land after four characters of the Latin line below. A run
/// that counted characters instead would stop after two, which is the bug this
/// test exists for.
#[gpui_kit::test]
fn select_column_lines_up_wide_characters_with_what_is_painted(cx: &mut TestAppContext) {
    let (window, workspace) =
        open_workspace_with_document(cx, "column-wide", "名称 状态\nAlpha ready");

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let editor = workspace
            .read_with(cx, |workspace, _| workspace.active_editor().cloned())
            .expect("the opening document should have an editor");
        // Right after "名称", which is display column four.
        let after_name = "名称".len();
        editor.update(cx, |state, cx| {
            state.set_selected_range(after_name..after_name, cx)
        });

        workspace.update(cx, |workspace, cx| workspace.select_column(false, cx));
        window.render_frame(cx);
        window.input("X", cx);
        window.render_frame(cx);

        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some("名称X 状态\nAlphXa ready".to_owned()),
            "the caret should land after four painted columns, not two characters"
        );
    })
    .expect("select a column across wide characters");
}

/// Option-dragging down the text paints a block selection, and typing fills
/// it on every row.
///
/// This is the gesture itself, not the *Select Column* commands: upstream's
/// `EditorTextView` starts a rectangular selection when Option is held during
/// a drag. The block is invisible to this test, so it is proved the same way
/// as the command test — by typing into it and reading the buffer back. The
/// column is read from a plain click at the same point first, so the assertion
/// does not have to guess which character the drag started on.
#[gpui_kit::test]
fn option_dragging_selects_a_block_and_typing_fills_it(cx: &mut TestAppContext) {
    let (window, workspace) =
        open_workspace_with_document(cx, "option-drag", "abcdefghij\nabcdefghij\nabcdefghij");

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let editor = workspace
            .read_with(cx, |workspace, _| workspace.active_editor().cloned())
            .expect("the opening document should have an editor");
        let (text_bounds, line_height) = editor.read_with(cx, |state, _| {
            (
                state.text_bounds().expect("the editor should be laid out"),
                state.line_height().expect("the editor should be laid out"),
            )
        });

        // A point inside the first row, far enough from the gutter that the
        // column is not zero.
        let x = text_bounds.origin.x + line_height * 3.0;
        let first_row = text_bounds.origin.y + line_height * 0.5;
        let last_row = first_row + line_height * 2.0;
        let start = point(x, first_row);
        let end = point(x, last_row);

        // Where a plain click at that point puts the caret: the column the
        // block should use. Nothing is typed, so the document is untouched.
        let click = |window: &mut Window, cx: &mut gpui_kit::App| {
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    position: start,
                    modifiers: Modifiers::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                    first_mouse: false,
                }),
                cx,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    position: start,
                    modifiers: Modifiers::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                }),
                cx,
            );
        };
        click(window, cx);
        window.render_frame(cx);
        let column = editor.read_with(cx, |state, _| state.cursor());
        assert!(
            column > 0,
            "the block must start inside the first row to prove the gesture; got column {column}"
        );

        // Option alone, which is the modifier upstream's `EditorTextView`
        // reads for a rectangular selection. Nothing is typed between the
        // click and the drag, so the caret is the block's own start point.
        let block = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                position: start,
                modifiers: block,
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: end,
                modifiers: block,
                pressed_button: Some(MouseButton::Left),
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                position: end,
                modifiers: block,
                button: MouseButton::Left,
                click_count: 1,
            }),
            cx,
        );
        window.render_frame(cx);
        window.input("X", cx);
        window.render_frame(cx);

        let expected = ["abcdefghij", "abcdefghij", "abcdefghij"]
            .map(|line| {
                let column = column.min(line.len());
                format!("{}X{}", &line[..column], &line[column..])
            })
            .join("\n");
        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some(expected),
            "one keystroke should fill the block on every dragged row"
        );
    })
    .expect("option-drag a block");
}

/// CotEditor multiplies the font's own line height, not the point size.
///
/// `lineHeightMultiple` at 1.3 over a 13-point font is 19.5–20.8 points on
/// macOS, because AppKit multiplies `ascent + descent`. GPUI's
/// `DefiniteLength::Fraction` multiplies the point size instead, which gave
/// 16.9 — visibly tighter than the same setting upstream. The editor now
/// receives pixels computed the way AppKit computes them.
///
/// The harness resolves a fallback font whose natural line height is 16.9
/// points, so the editor measures 22 pixels here; the same multiplication with
/// the point size would give 16.9, which is what the bound rules out.
#[gpui_kit::test]
fn the_line_height_is_the_fonts_own_height_times_the_multiple(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "line-height", "one\ntwo\nthree");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let editor = workspace
            .read_with(cx, |workspace, _| workspace.active_editor().cloned())
            .expect("the opening document should have an editor");
        let line_height = editor.read_with(cx, |state, _| {
            state.line_height().expect("the editor should be laid out")
        });
        assert!(
            f32::from(line_height) > 17.5,
            "13pt at a multiple of 1.3 is 16.9 when the point size is multiplied \
             and 22 when the font's own line height is; got {line_height:?}"
        );
    })
    .expect("measure the line height");
}

/// A check that cannot reach the server reports itself when the user asked for
/// it, and stays silent when the launch asked instead.
///
/// The test application's HTTP client answers every request with a 404, so both
/// paths run without a network. That is deliberate: an automatic check that
/// popped a dialog would greet a laptop with no connection with an alert every
/// morning, and the difference between the two calls is the whole point.
#[gpui_kit::test]
fn a_manual_update_check_reports_a_failed_request_but_the_launch_check_does_not(
    cx: &mut TestAppContext,
) {
    use gpui_kit::component::WindowExt as _;

    let (window, workspace) = open_workspace_with_document(cx, "update-check", "hello");

    // The launch path: it runs, fails, and says nothing.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        workspace.update(cx, |workspace, cx| workspace.check_for_updates(true, cx));
    })
    .expect("start the launch check");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        assert!(
            !window.has_active_dialog(cx),
            "a failed automatic check must not interrupt the user"
        );
    })
    .expect("read the automatic result");

    // The menu path: the same failure, reported.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        workspace.update(cx, |workspace, cx| workspace.check_for_updates(false, cx));
    })
    .expect("start the manual check");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        assert!(
            window.has_active_dialog(cx),
            "a check the user asked for must say what went wrong"
        );
    })
    .expect("read the manual result");
}

/// Typing then saving writes the new text to the file.
///
/// The other half of §22.36: that test proved a keystroke reaches the buffer,
/// this one proves the buffer reaches the disk. Both are "the write path", and
/// neither had a test before.
#[gpui_kit::test]
fn typing_then_saving_writes_the_file(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "save", "abc");
    let path = std::env::temp_dir()
        .join("subtext-ui-tests-save")
        .join("a.txt");

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.input("x", cx);
        window.render_frame(cx);
        workspace.update(cx, |workspace, cx| workspace.save(false, window, cx));
        window.render_frame(cx);
    })
    .expect("save");
    cx.run_until_parked();

    let written = std::fs::read_to_string(&path).expect("the file should still be there");
    assert_eq!(
        written, "xabc",
        "saving after typing should write the edited buffer, not the original"
    );
}

/// Backspace, Enter and the arrows reach the document from the keyboard.
///
/// Ordinary characters arrive through the input method, so a
/// `window.input("x")` test passes even when the keymap is empty. Backspace,
/// Enter and the arrows are looked up in the keymap instead, and the menu
/// rebuild used to clear the framework's editor bindings along with its own —
/// which left a document that could type but not erase a character. This test
/// presses the keys the way a person does, and reads the buffer afterwards.
#[gpui_kit::test]
fn backspace_enter_and_arrows_edit_the_document(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "editing-keys", "abc");

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);

        // Type one character: "xabc", caret after the x.
        window.input("x", cx);
        window.render_frame(cx);

        // Backspace deletes it again, and the caret is back at the start.
        window.press("backspace", cx);
        window.render_frame(cx);
        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some("abc".to_owned()),
            "Backspace should delete the character before the caret"
        );

        // Right arrow moves over the "a", so the next character lands between
        // the first letter and the rest.
        window.press("right", cx);
        window.render_frame(cx);
        window.input("Z", cx);
        window.render_frame(cx);
        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some("aZbc".to_owned()),
            "Right should move the caret one character to the right"
        );

        // Enter splits the line at the caret.
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some("aZ\nbc".to_owned()),
            "Enter should insert a line break at the caret"
        );
    })
    .expect("editing keys");
}

/// Closing a sheet gives the keyboard back to the document.
///
/// The design guide's rule is that an overlay returns focus to what opened it.
/// Here that is the editor: if closing the sheet left the focus nowhere, the
/// user would have to click the text before typing again — and nothing in the
/// UI would say so.
#[gpui_kit::test]
fn closing_a_sheet_gives_the_keyboard_back_to_the_editor(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "focus-back", "abc");

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        workspace.update(cx, |workspace, cx| workspace.surround_sheet(window, cx));
        window.render_frame(cx);
        window.render_frame(cx);

        // Escape closes it; the caret then has to be back in the document.
        window.press("escape", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            window.try_find("surround-begin").is_none(),
            "the sheet should be closed"
        );

        window.input("x", cx);
        window.render_frame(cx);
        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some("xabc".to_owned()),
            "typing after closing the sheet should land in the document"
        );
    })
    .expect("focus back");
}

/// Find All lists every match.
///
/// The result table is the one part of the find bar nothing has touched: the
/// button fills it, the table draws it, and the row click jumps to the match.
/// This asserts the first step — two matches in this document become two rows.
#[gpui_kit::test]
fn find_all_lists_every_match(cx: &mut TestAppContext) {
    let (document, workspace) = open_workspace_with_document(cx, "find-all", "alpha beta alpha\n");

    cx.update_window(document, |_, window, cx| {
        workspace.update(cx, |workspace, cx| workspace.show_find(false, window, cx));
    })
    .expect("open find");
    cx.run_until_parked();

    let bar = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != document)
            .expect("the find bar should be a window of its own")
    });

    cx.update_window(bar, |_, window, cx| {
        window.render_frame(cx);
        window.input("alpha", cx);
        window.render_frame(cx);
        window.click("find-all", cx);
        window.render_frame(cx);
    })
    .expect("find all");

    let (rows, open) = cx.update(|cx| {
        workspace.read_with(cx, |workspace, _| {
            let (rows, open) = workspace.find_results();
            (rows.len(), open)
        })
    });
    assert_eq!(rows, 2, "two matches in this document should be two rows");
    assert!(open, "Find All should open the result table");
}

/// Replace All rewrites every match.
///
/// The bulk half of Replace: the button calls `replace_all`, which goes through
/// the editor's own `replace_all` so the whole pass is one undo transaction.
#[gpui_kit::test]
fn replace_all_rewrites_every_match(cx: &mut TestAppContext) {
    let (document, workspace) =
        open_workspace_with_document(cx, "replace-all", "alpha beta alpha\n");

    cx.update_window(document, |_, window, cx| {
        workspace.update(cx, |workspace, cx| workspace.show_find(false, window, cx));
    })
    .expect("open find");
    cx.run_until_parked();

    let bar = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != document)
            .expect("the find bar should be a window of its own")
    });

    cx.update_window(bar, |_, window, cx| {
        window.render_frame(cx);
        window.input("alpha", cx);
        window.render_frame(cx);
        window.click("find-replacement", cx);
        window.input("X", cx);
        window.render_frame(cx);
        window.click("replace-all", cx);
        window.render_frame(cx);
    })
    .expect("replace all");
    cx.run_until_parked();

    let text = cx.update(|cx| workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)));
    assert_eq!(
        text.as_deref(),
        Some("X beta X\n"),
        "Replace All should rewrite both matches"
    );
}

/// Clicking a result row selects that match.
///
/// Two things had to be true before this could be written, and neither was:
///
/// * the rows have to exist — the panel has to be tall enough for the table,
///   which is what §22.41 fixed (a click on *Find All* could not resize the
///   window it was running in);
/// * the cells have to be observable — a delegate-drawn row is not in the
///   window's element tree for tests unless it says `.test_support()`.
///
/// `bounds_changed` is the harness's half of the deal: `resize` only sets the
/// platform size, and layout reads `viewport_size`, which the platform updates
/// through that call. Without it every frame lays out at the old height and the
/// rows stay out of reach.
#[gpui_kit::test]
fn clicking_a_result_row_selects_that_match(cx: &mut TestAppContext) {
    let (document, workspace) =
        open_workspace_with_document(cx, "result-click", "alpha beta alpha\n");

    cx.update_window(document, |_, window, cx| {
        workspace.update(cx, |workspace, cx| workspace.show_find(false, window, cx));
    })
    .expect("open the find bar");
    cx.run_until_parked();

    let bar = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != document)
            .expect("the find bar should be a window of its own")
    });

    let second = cx
        .update_window(bar, |_, window, cx| {
            window.render_frame(cx);
            window.input("alpha", cx);
            window.render_frame(cx);
            window.click("find-all", cx);
            window.render_frame(cx);

            // The click grew the panel (§22.41); tell the window its size changed,
            // which is what the platform does on a real resize.
            window.bounds_changed(cx);
            window.render_frame(cx);

            let expected = workspace.read_with(cx, |workspace, _| workspace.find_panel_height());
            assert_eq!(
                window.bounds().size.height,
                px(expected),
                "Find All should leave the panel tall enough to show the table"
            );

            let rows = workspace.read_with(cx, |workspace, _| {
                workspace
                    .find_results()
                    .0
                    .iter()
                    .map(|row| row.range.clone())
                    .collect::<Vec<_>>()
            });
            assert_eq!(rows.len(), 2, "two matches in this document");
            let second = rows[1].clone();

            // The row drawn second, not the first: a click that always landed on
            // row 0 would pass a weaker assertion.
            window.click("find-result-1-text", cx);
            window.render_frame(cx);
            second
        })
        .expect("click the second row");

    let selected = workspace.read_with(cx, |workspace, _| {
        workspace.selected_find_result_ranges().to_vec()
    });
    assert_eq!(
        selected,
        vec![second],
        "clicking a result row should select exactly that match"
    );
}

/// Dragging the inspector divider resizes the pane — and the setting.
///
/// The gesture is the one thing about the split view no test could reach. §22.28
/// and §22.29 concluded it needed a framework change; that was wrong on both
/// counts. `TestWindowExt::drag` takes plain coordinates, and the handle is not
/// an unaddressable hairline — it straddles the inspector panel's **left edge**,
/// one inspector width in from the window's right edge. `inspector-path` sits a
/// whole label column inside the pane, which is what the earlier attempts were
/// aiming at (§22.43).
#[gpui_kit::test]
fn dragging_the_inspector_divider_resizes_the_pane(cx: &mut TestAppContext) {
    use subtext_app::settings::InspectorPane;

    let (window, workspace) = open_workspace_with_document(cx, "drags", "abc");

    let (window_width, rem) = cx
        .update_window(window, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.show_inspector(InspectorPane::Document, cx);
            });
            window.render_frame(cx);
            window.render_frame(cx);
            (
                window.bounds().size.width,
                gpui_kit::component::theme::ActiveTheme::theme(cx)
                    .font_size
                    .as_f32(),
            )
        })
        .expect("open the inspector");

    // The live pane's width is what positions the handle — it came from the same
    // setting, but it is the pane, not the value, that decides where the press
    // has to land.
    let pane_width =
        workspace.read_with(cx, |workspace, _| workspace.settings.window.inspector_width);

    // Two points in from the edge puts the press inside the handle's band
    // rather than on the boundary itself.
    let divider = window_width - px(pane_width * rem) + px(2.);
    let y = px(300.);

    cx.update_window(window, |_, window, cx| {
        let before = window.find("inspector-path").bounds().origin.x;
        window.drag(
            gpui_kit::point(divider, y),
            gpui_kit::point(divider - px(60.), y),
            cx,
        );
        window.render_frame(cx);
        let after = window.find("inspector-path").bounds().origin.x;
        let moved = f32::from(before - after);
        assert!(
            (40.0..=80.0).contains(&moved),
            "dragging 60 points left should move the pane about 60 (moved {moved})"
        );
    })
    .expect("drag the divider");

    // The inspector is the right-hand pane, so its left edge moving left makes
    // it *wider* — and that width is what the next launch restores.
    let after_width =
        workspace.read_with(cx, |workspace, _| workspace.settings.window.inspector_width);
    assert!(
        after_width > pane_width,
        "the drag should widen the inspector in the settings ({pane_width} → {after_width})"
    );
}

/// A test run must not write into the user's own settings.
///
/// Every workspace opened here behaves like the real app: opening a document
/// notes it in the recents, resizing a pane stores the width, quitting saves.
/// Until the harness pinned a config root, running the suite rewrote the
/// developer's `Settings.json` — the drag test did it by dragging a divider.
#[gpui_kit::test]
fn the_ui_harness_writes_settings_into_a_temporary_directory(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace_with_document(cx, "config-root", "abc");

    let path = subtext_app::settings::Settings::path();
    assert!(
        path.starts_with(std::env::temp_dir()),
        "settings must live under the temp dir during tests, not at {}",
        path.display()
    );

    // And a save really lands there rather than at the pinned path only. The
    // read happens inside the same synchronous step as the save, because every
    // test in this binary shares the process's config root: a parallel test's
    // own save can land between ours and a later read, and which write wins is
    // not ours to choose. What must hold — and what this asserts — is that a
    // save of the real settings file landed under the temp dir, not at the
    // developer's home.
    let saved = cx
        .update_window(window, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.settings.window.inspector_width = 17.5;
                workspace.settings.save().expect("save the settings");
                cx.notify();
            });
            window.render_frame(cx);
            std::fs::read_to_string(&path).ok()
        })
        .expect("save")
        .expect("the settings file should exist");
    assert!(
        saved.contains("inspector_width"),
        "the file that save() wrote should be the settings file, got: {saved}"
    );
}

/// Dragging a row of the real toolbar reorders it while the palette is open.
///
/// The palette's model tests call `move_row` directly, so they cannot say
/// whether the gesture is wired to it. AppKit's palette leaves the toolbar
/// itself as the place items are arranged — the strip under the grid is only a
/// picture of the shipped set — so this opens the palette and drives the
/// pointer on the toolbar's own slots, then reads the order back off the
/// workspace.
///
/// The palette has to be open for the slots to be drag sources: outside
/// customization the bar is inert, exactly as upstream's is.
#[gpui_kit::test]
fn dragging_an_item_reorders_the_toolbar(cx: &mut TestAppContext) {
    use subtext_app::toolbar::ToolbarItemId;

    let (window, workspace) =
        open_workspace_with_document(cx, "toolbar-reorder", "# Title\n\ntext\n");

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.set_toolbar_items(
                vec![
                    ToolbarItemId::Syntax,
                    ToolbarItemId::Comment,
                    ToolbarItemId::Find,
                    ToolbarItemId::FlexibleSpace,
                    ToolbarItemId::ToggleInspector,
                ],
                cx,
            );
            workspace.customize_toolbar(window, cx);
        });
        window.render_frame(cx);

        let comment = window.find(("toolbar-slot", 1usize)).bounds();
        let find = window.find(("toolbar-slot", 2usize)).bounds();
        window.drag(
            gpui_kit::point(comment.center().x, comment.center().y),
            gpui_kit::point(find.center().x, find.center().y),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            workspace.read(cx).toolbar_items(),
            vec![
                ToolbarItemId::Syntax,
                ToolbarItemId::Find,
                ToolbarItemId::Comment,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::ToggleInspector,
            ],
            "dragging the comment row onto the Find row should swap them"
        );
    })
    .expect("drag the item");
}

/// The syntax pop-up is a scrolling list, not a list that runs off the screen.
///
/// There are seventy syntaxes, and the menu builds an item for each. The widget
/// caps its height and adds a scroll container only when the menu is marked
/// `scrollable`; without the flag the list simply grew past the bottom of the
/// window and every syntax below the fold was unreachable. This opens the menu
/// and wheels it: the first row has to move up, which only a real scroll can do.
#[gpui_kit::test]
fn the_syntax_menu_scrolls_to_the_syntaxes_past_the_fold(cx: &mut TestAppContext) {
    let (window, _workspace) = open_workspace_with_document(cx, "syntax-menu", "text\n");

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("toolbar-syntax", cx);
        window.render_frame(cx);

        let items = window
            .within("popup-menu")
            .find_all_by_role(gpui_kit::Role::MenuItem);
        assert!(
            items.len() > 60,
            "expected the installed syntaxes in the menu, found {} items",
            items.len()
        );
        let first_row = items[0].bounds().origin.y;

        window.scroll(
            "popup-menu",
            gpui_kit::ScrollDelta::Lines(gpui_kit::point(0., -5.)),
            cx,
        );
        window.render_frame(cx);

        let scrolled = window
            .within("popup-menu")
            .find_all_by_role(gpui_kit::Role::MenuItem);
        assert!(
            scrolled[0].bounds().origin.y < first_row,
            "the syntax list did not scroll: the first row stayed at {first_row}"
        );
    })
    .expect("open and scroll the syntax menu");
}

/// Dragging a tile of the grid into the real toolbar adds it there.
///
/// The palette offers this twice — a click on a tile, and a drag out of the
/// grid — and the click is covered by the sheet's own model tests. The drag is
/// the one that can be present but inert: GPUI starts a gesture only when the
/// source's hitbox is the hovered one, and the tile is a `Button` wrapped in a
/// drag source. Pressing on the tile and releasing over the toolbar therefore
/// exercises the wiring the model tests cannot see — including the overlay's
/// occlusion, which is what has to leave the toolbar behind it live.
#[gpui_kit::test]
fn dragging_a_palette_tile_adds_it_to_the_toolbar(cx: &mut TestAppContext) {
    use subtext_app::toolbar::ToolbarItemId;

    let (window, workspace) = open_workspace_with_document(cx, "toolbar-tile", "# Title\n\ntext\n");

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.set_toolbar_items(
                vec![
                    ToolbarItemId::Syntax,
                    ToolbarItemId::FlexibleSpace,
                    ToolbarItemId::ToggleInspector,
                ],
                cx,
            );
            workspace.customize_toolbar(window, cx);
        });
        window.render_frame(cx);
        assert!(
            !workspace
                .read(cx)
                .toolbar_items()
                .contains(&ToolbarItemId::Comment),
            "the toolbar should start without the comment item"
        );

        let tile = window.find(("toolbar-tile-source", ToolbarItemId::Comment as usize));
        let syntax = window.find(("toolbar-slot", 0usize));
        let middle = |bounds: Bounds<gpui_kit::Pixels>| {
            gpui_kit::point(
                bounds.origin.x + bounds.size.width / 2.,
                bounds.origin.y + bounds.size.height / 2.,
            )
        };

        window.drag(middle(tile.bounds()), middle(syntax.bounds()), cx);
        window.render_frame(cx);

        let items = workspace.read(cx).toolbar_items();
        assert_eq!(
            items.first(),
            Some(&ToolbarItemId::Comment),
            "dropping the comment tile on the syntax row should insert it there, got: {items:?}"
        );
    })
    .expect("drag the tile");
}

/// Clicking a palette tile toggles its membership, and *Done* closes the sheet.
///
/// The sheet's model tests call `add` and `remove` directly, so they cannot say
/// whether the tile's button reaches them; and *Done* is a plain button whose
/// only job is to close the palette. Both are the paths a hand uses when it
/// does not drag: a click is the shortcut AppKit does not offer, and Done is
/// the sheet's default button. The second click is on the dimmed tile — the
/// in-use state must keep working, because it is the only way back out.
#[gpui_kit::test]
fn clicking_a_palette_tile_toggles_it_and_done_closes_the_sheet(cx: &mut TestAppContext) {
    use subtext_app::toolbar::ToolbarItemId;

    let (window, workspace) = open_workspace_with_document_sized(
        cx,
        "toolbar-click",
        "# Title\n\ntext\n",
        size(px(900.), px(800.)),
    );

    cx.update_window(window, |_, window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.set_toolbar_items(
                vec![
                    ToolbarItemId::Syntax,
                    ToolbarItemId::Comment,
                    ToolbarItemId::ToggleInspector,
                ],
                cx,
            );
            workspace.customize_toolbar(window, cx);
        });
        window.render_frame(cx);
        window.render_frame(cx);

        let tile = ("toolbar-tile", ToolbarItemId::Comment as usize);
        window.click(tile, cx);
        window.render_frame(cx);
        assert!(
            !workspace
                .read(cx)
                .toolbar_items()
                .contains(&ToolbarItemId::Comment),
            "clicking the in-use comment tile should remove it from the toolbar"
        );

        window.click(tile, cx);
        window.render_frame(cx);
        assert!(
            workspace
                .read(cx)
                .toolbar_items()
                .contains(&ToolbarItemId::Comment),
            "clicking the dimmed tile again should add the item back"
        );

        assert!(
            window.try_find("toolbar-sheet").is_some(),
            "the palette should still be open before Done"
        );
        window.click("toolbar-done", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            window.try_find("toolbar-sheet").is_none(),
            "Done should have closed the palette"
        );
    })
    .expect("click a palette tile");
}

/// Dragging a row of the encoding list reorders it.
///
/// §22.47 replaced the two arrow buttons with upstream's gesture — drag a row,
/// ⌫ to delete — and the model tests call `move_dragged` directly, so they
/// cannot say whether the gesture is wired to it. This drives the pointer path
/// the way a hand does (press, move past the threshold, release) and reads the
/// answer off the rows' own bounds: the row that was first has to end up lower
/// than it started.
#[gpui_kit::test]
fn dragging_a_row_reorders_the_encoding_list(cx: &mut TestAppContext) {
    use subtext_app::encoding_list_sheet::EncodingListSheet;
    use subtext_core::encoding::{EncodingList, TextEncoding};

    cx.update(gpui_kit::init);

    let list = EncodingList::from_identifiers(&[
        Some("utf-8".to_string()),
        Some("shift_jis".to_string()),
        Some("windows-1252".to_string()),
    ]);
    let sheet = cx.update(|cx| {
        cx.new(|cx| {
            EncodingListSheet::new(
                gpui_kit::WeakEntity::new_invalid(),
                list,
                TextEncoding::UTF8,
                cx,
            )
        })
    });

    let window = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(560.), px(420.)),
                ))),
                ..Default::default()
            },
            cx,
            |_window, _cx| sheet.clone(),
        )
        .expect("open the test window")
        .0
    });

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let first = window.find(("encoding-row", 0u64)).bounds();
        let third = window.find(("encoding-row", 2u64)).bounds();
        let middle = |bounds: Bounds<gpui_kit::Pixels>| {
            gpui_kit::point(
                bounds.origin.x + bounds.size.width / 2.,
                bounds.origin.y + bounds.size.height / 2.,
            )
        };

        window.drag(middle(first), middle(third), cx);
        window.render_frame(cx);

        let moved = window.find(("encoding-row", 0u64)).bounds();
        assert!(
            moved.origin.y > first.origin.y,
            "the first row should have been dragged below the last one \
             ({} → {})",
            f32::from(first.origin.y),
            f32::from(moved.origin.y)
        );
    })
    .expect("drag the row");
}

// ---- the menu bar's commands ---------------------------------------------

/// Wire the application's menu handlers into a test window.
///
/// The application does this in `lib.rs` when its window opens: `install`
/// registers one global handler per action, `register` records which workspace
/// a command belongs to. Without both, dispatching an action reaches nothing —
/// which is exactly the "the menu item does nothing" failure this file is here
/// to catch, so every menu test below starts here rather than clicking.
fn install_menu_handlers(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    workspace: &gpui_kit::Entity<subtext_app::workspace::Workspace>,
) {
    cx.update(|cx| {
        subtext_app::bridge::install(cx);
        subtext_app::bridge::register(workspace.downgrade(), window, cx);
    });
}

fn active_text(
    cx: &mut TestAppContext,
    workspace: &gpui_kit::Entity<subtext_app::workspace::Workspace>,
) -> Option<String> {
    cx.update(|cx| workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)))
}

fn clipboard_text(cx: &TestAppContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

fn select(
    cx: &mut TestAppContext,
    workspace: &gpui_kit::Entity<subtext_app::workspace::Workspace>,
    range: std::ops::Range<usize>,
) {
    let editor = workspace
        .read_with(cx, |workspace, _| workspace.active_editor().cloned())
        .expect("the opening document should have an editor");
    editor.update(cx, |state, cx| state.set_selected_range(range, cx));
}

/// *Edit ▸ Undo* undoes what was typed.
///
/// The menu item carries `actions::edit::Undo`, while the text engine listens
/// for `gpui_kit::base::input::Undo`. Sending the menu's action to the
/// workspace and having it report "handled by the text engine" used to leave
/// the command doing nothing; this test follows the menu's own path and reads
/// the document, so the difference between a notice and an edit is visible.
#[gpui_kit::test]
fn undo_from_the_menu_bar_undoes_the_typing(cx: &mut TestAppContext) {
    use subtext_app::actions;

    let (window, workspace) = open_workspace_with_document(cx, "menu-undo", "abc");
    install_menu_handlers(cx, window, &workspace);

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.input("X", cx);
        window.render_frame(cx);
    })
    .expect("type");
    assert_eq!(
        active_text(cx, &workspace),
        Some("Xabc".to_owned()),
        "the keystroke should have landed first"
    );

    cx.dispatch_action(window, actions::edit::Undo);
    cx.run_until_parked();

    assert_eq!(
        active_text(cx, &workspace),
        Some("abc".to_owned()),
        "Edit ▸ Undo should undo the typing, not merely report that it was seen"
    );
}

/// *Edit ▸ Select All* followed by *Edit ▸ Copy* puts the document on the
/// clipboard.
#[gpui_kit::test]
fn select_all_then_copy_from_the_menu_reaches_the_clipboard(cx: &mut TestAppContext) {
    use subtext_app::actions;

    let (window, workspace) = open_workspace_with_document(cx, "menu-copy", "alpha beta");
    install_menu_handlers(cx, window, &workspace);

    cx.dispatch_action(window, actions::edit::SelectAll);
    cx.run_until_parked();
    cx.dispatch_action(window, actions::edit::Copy);
    cx.run_until_parked();

    assert_eq!(
        clipboard_text(cx).as_deref(),
        Some("alpha beta"),
        "Select All and Copy should have put the whole document on the clipboard, \
         with nothing selected the copy has nothing to write"
    );
}

/// *Edit ▸ Cut* removes the selection and puts it on the clipboard.
#[gpui_kit::test]
fn cut_from_the_menu_removes_the_selection(cx: &mut TestAppContext) {
    use subtext_app::actions;

    let (window, workspace) = open_workspace_with_document(cx, "menu-cut", "alpha beta");
    install_menu_handlers(cx, window, &workspace);

    cx.dispatch_action(window, actions::edit::SelectAll);
    cx.run_until_parked();
    cx.dispatch_action(window, actions::edit::Cut);
    cx.run_until_parked();

    assert_eq!(active_text(cx, &workspace), Some(String::new()));
    assert_eq!(clipboard_text(cx).as_deref(), Some("alpha beta"));
}

/// *Edit ▸ Paste* inserts the clipboard's text at the caret.
#[gpui_kit::test]
fn paste_from_the_menu_inserts_the_clipboard(cx: &mut TestAppContext) {
    use subtext_app::actions;

    let (window, workspace) = open_workspace_with_document(cx, "menu-paste", "abc");
    install_menu_handlers(cx, window, &workspace);

    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("xyz".to_owned()));
    cx.dispatch_action(window, actions::edit::Paste);
    cx.run_until_parked();

    assert_eq!(
        active_text(cx, &workspace),
        Some("xyzabc".to_owned()),
        "the caret opens at the start of the document, so the paste belongs there"
    );
}

/// *Edit ▸ Paste as Plain Text* reads the clipboard through the workspace
/// rather than the engine — the command is answered without a focused field.
#[gpui_kit::test]
fn paste_as_plain_text_from_the_menu_inserts_the_clipboard(cx: &mut TestAppContext) {
    use subtext_app::actions;

    let (window, workspace) = open_workspace_with_document(cx, "menu-paste-asis", "abc");
    install_menu_handlers(cx, window, &workspace);

    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("xyz".to_owned()));
    cx.dispatch_action(window, actions::edit::PasteAsIs);
    cx.run_until_parked();

    assert_eq!(active_text(cx, &workspace), Some("xyzabc".to_owned()));
}

/// *Text ▸ Change Case ▸ Uppercase* reaches the selected word.
///
/// The case commands read the selection from the editor rather than from the
/// focused input field, so this walks the same menu path as the rest and then
/// reads the document.
#[gpui_kit::test]
fn uppercase_from_the_menu_transforms_the_selection(cx: &mut TestAppContext) {
    use subtext_app::actions;

    let (window, workspace) = open_workspace_with_document(cx, "menu-uppercase", "hello world");
    install_menu_handlers(cx, window, &workspace);
    select(cx, &workspace, 0..5);

    cx.dispatch_action(window, actions::text::UppercaseWord);
    cx.run_until_parked();

    assert_eq!(
        active_text(cx, &workspace),
        Some("HELLO world".to_owned()),
        "uppercase should touch only the selection"
    );
}

/// *Text ▸ Sort Lines* reorders the whole document when everything is selected.
#[gpui_kit::test]
fn sort_lines_from_the_menu_reorders_the_document(cx: &mut TestAppContext) {
    use subtext_app::actions;

    let (window, workspace) =
        open_workspace_with_document(cx, "menu-sort", "banana\napple\ncherry");
    install_menu_handlers(cx, window, &workspace);
    select(cx, &workspace, 0..18);

    cx.dispatch_action(window, actions::text::SortLinesAscending);
    cx.run_until_parked();

    assert_eq!(
        active_text(cx, &workspace),
        Some("apple\nbanana\ncherry".to_owned())
    );
}

/// *Text ▸ Reverse Lines* turns the selection's lines around.
#[gpui_kit::test]
fn reverse_lines_from_the_menu_reorders_the_document(cx: &mut TestAppContext) {
    use subtext_app::actions;

    let (window, workspace) = open_workspace_with_document(cx, "menu-reverse", "one\ntwo\nthree");
    install_menu_handlers(cx, window, &workspace);
    select(cx, &workspace, 0..13);

    cx.dispatch_action(window, actions::text::ReverseLines);
    cx.run_until_parked();

    assert_eq!(
        active_text(cx, &workspace),
        Some("three\ntwo\none".to_owned())
    );
}

/// *Text ▸ Delete Duplicate Line* keeps the first copy of each line.
#[gpui_kit::test]
fn delete_duplicate_line_from_the_menu_removes_the_repeat(cx: &mut TestAppContext) {
    use subtext_app::actions;

    let (window, workspace) = open_workspace_with_document(cx, "menu-dedup", "a\nb\na");
    install_menu_handlers(cx, window, &workspace);
    select(cx, &workspace, 0..5);

    cx.dispatch_action(window, actions::text::DeleteDuplicateLine);
    cx.run_until_parked();

    assert_eq!(active_text(cx, &workspace), Some("a\nb".to_owned()));
}

/// *Text ▸ Trim Trailing Whitespace* cleans every selected line.
#[gpui_kit::test]
fn trim_trailing_whitespace_from_the_menu_cleans_the_document(cx: &mut TestAppContext) {
    use subtext_app::actions;

    let (window, workspace) = open_workspace_with_document(cx, "menu-trim", "a  \nb\t\nc");
    install_menu_handlers(cx, window, &workspace);
    select(cx, &workspace, 0..8);

    cx.dispatch_action(window, actions::text::TrimTrailingWhitespace);
    cx.run_until_parked();

    assert_eq!(active_text(cx, &workspace), Some("a\nb\nc".to_owned()));
}
