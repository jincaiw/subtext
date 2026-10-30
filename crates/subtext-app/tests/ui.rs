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
