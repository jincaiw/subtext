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
    AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, StatefulInteractiveElement as _, Styled as _, TestAppContext, Window, WindowBounds,
    WindowOptions, div, px, size,
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
    use subtext_app::settings::Settings;
    use subtext_app::workspace::Workspace;
    use subtext_app::{load_syntaxes, load_themes};

    cx.update(gpui_kit::init);

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
                    size(px(900.), px(600.)),
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
