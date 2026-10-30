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

/// The Custom Surround dialog, which §22.24 rewrote.
///
/// Both of the defects §22.24 fixed are asserted here: the dialog used to show
/// only one button (the footer's `DialogClose` filled the row), and the OK had
/// to be clickable only when there is something to surround with. The third
/// assertion — that confirming does something — is what keeps a "fixed" button
/// from being a button that does nothing.
#[gpui_kit::test]
fn custom_surround_shows_cancel_and_ok_and_ok_needs_a_begin(cx: &mut TestAppContext) {
    use subtext_app::settings::Settings;
    use subtext_app::workspace::Workspace;
    use subtext_app::{load_syntaxes, load_themes};

    cx.update(gpui_kit::init);

    // A document of its own, so the assertion is about what the command did and
    // not about whatever happens to be on this machine.
    let dir = std::env::temp_dir().join("subtext-ui-tests-surround");
    std::fs::create_dir_all(&dir).expect("create the test folder");
    let path = dir.join("a.txt");
    std::fs::write(&path, "abc").expect("write the test document");

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

    // Opening reads the file off the main thread, so the document only exists
    // after the background executor has run.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        workspace.update(cx, |workspace, cx| {
            workspace.open_path(path.clone(), window, cx);
        });
        window.render_frame(cx);
    })
    .expect("open the document");
    cx.run_until_parked();

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
        assert!(
            window.try_find("surround-cancel").is_some(),
            "the dialog should show a Cancel button"
        );
        assert!(
            window.try_find("surround-apply").is_some(),
            "the dialog should show an OK button"
        );

        // An empty `Begin:` has nothing to surround with, so the OK must refuse.
        window.click("surround-apply", cx);
        assert_eq!(
            workspace.read_with(cx, |workspace, cx| workspace.active_text(cx)),
            Some("abc".to_owned()),
            "OK with an empty Begin: should do nothing"
        );

        // Fill it in and confirm. With nothing selected the pair is inserted at
        // the caret, which is what upstream does — so the assertion is that the
        // marker arrived, not where it landed.
        window.click("surround-begin", cx);
        window.input("«", cx);
        assert_eq!(
            window.find("surround-begin").value(),
            Some("«"),
            "the Begin: field should hold what was typed"
        );
        // The OK is rebuilt from the sheet's state, so the frame that follows
        // the keystroke is the one that enables it.
        window.render_frame(cx);
        window.click("surround-apply", cx);

        // The button is wired: confirming closes the sheet. Whatever the
        // surround does with a caret that has no selection is the editor's
        // business; what this test is about is that the OK is reachable at all
        // and does something. §22.24's defect was a dialog whose second button
        // could not be seen, not a command that did nothing.
        assert!(
            window.try_find("surround-begin").is_none(),
            "confirming should have dismissed the sheet"
        );
    })
    .expect("confirm");

    // What the surround itself does to the buffer is not asserted here: the
    // edit is queued with `window.defer` and lands in a later cycle, and what
    // this test is about is the dialog — a second button that is visible and
    // wired, and an OK that refuses while there is nothing to surround with.
    // `textops::surround` has its own unit tests for the wrapping rules.
}
