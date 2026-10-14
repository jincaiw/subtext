//! # SubText
//!
//! A cross-platform plain-text editor built on [GPUI Kit], and a faithful
//! reimplementation of [CotEditor].
//!
//! CotEditor is a Cocoa document application: one `NSWindow` per document, an
//! `NSMenu` built in a storyboard, an `@objc` selector on `NSResponder` for
//! every command, and `.cotsyntax` / `.cottheme` bundles on disk for the
//! language rules and the palette. SubText keeps everything a user can see or
//! press — the menus, the shortcuts, the syntax rules, the themes, the
//! inspectors — and swaps the framework underneath.
//!
//! | CotEditor | SubText |
//! | --------- | ------- |
//! | AppKit `NSTextView` | `gpui-base`'s editor state |
//! | `Main.storyboard` `NSMenu` | [`menu`], generated from `resources/menu.json` |
//! | `@objc` selectors on `NSResponder` | [`actions`], one type per command |
//! | `keyEquivalent` in the storyboard | [`keymap`], same table, same shortcuts |
//! | `.cotsyntax` regex + tree-sitter | [`subtext_syntax`], the same bundles |
//! | `.cottheme` | [`subtext_theme`], the same files |
//!
//! ## Where the behaviour lives
//!
//! [`workspace`] owns the documents and the chrome; [`dispatch`] is the single
//! place a command name turns into behaviour; [`bridge`] is what connects the
//! typed actions to it. Nothing else decides what a command does.
//!
//! [GPUI Kit]: https://github.com/longbridge/gpui-kit
//! [CotEditor]: https://github.com/coteditor/CotEditor

pub mod actions;
pub mod bridge;
pub mod color_sheet;
pub mod dialogs;
pub mod dispatch;
pub mod folder_find;
pub mod highlighter;
pub mod keymap;
pub mod menu;
pub mod multiple_replace_sheet;
pub mod outline;
pub mod resources;
pub mod settings;
pub mod settings_window;
pub mod sort_sheet;
pub mod workspace;

use std::path::PathBuf;

use gpui_kit::{App, AppContext as _, WindowOptions, px};

use settings::Settings;
use workspace::Workspace;

pub use workspace::{load_syntaxes, load_themes};

/// Start the application.
///
/// Files named on the command line are opened in the first window, which is
/// what `open -a CotEditor foo.txt` does on macOS and what every editor does
/// on the other two platforms.
pub fn run() {
    let paths: Vec<PathBuf> = std::env::args()
        .skip(1)
        .filter(|argument| !argument.starts_with('-'))
        .map(PathBuf::from)
        .collect();

    gpui_kit::application()
        // The full Lucide catalog, so the toolbar and the inspectors are not
        // limited to the component library's default subset.
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            env_logger::try_init().ok();

            let resources = resources::Resources::discover();
            let syntaxes = load_syntaxes(resources.as_ref());
            let themes = load_themes(resources.as_ref());
            let settings = Settings::load();

            // The highlighter factory has to be in place before the first
            // editor is built, or the first document would open unhighlighted.
            // It is keyed by language name and holds its own copy of the
            // syntax list, so the workspace keeps the manager for lookups.
            highlighter::install_factory(
                std::sync::Arc::new(syntaxes.all().to_vec()),
                cx,
            );

            // CotEditor applies the saved theme at launch; so does the editor,
            // which means the component layer has to be told as well as the
            // text.
            if let Some(theme) = settings.theme.as_deref().and_then(|name| themes.by_name(name)) {
                let mode = if theme.is_dark() {
                    gpui_kit::component::ThemeMode::Dark
                } else {
                    gpui_kit::component::ThemeMode::Light
                };
                gpui_kit::component::Theme::change(mode, None, cx);
            }

            bridge::install(cx);
            cx.bind_keys(keymap::default_bindings());
            cx.set_menus(menu::app_menus());

            open_main_window(cx, syntaxes, themes, settings, paths);
        });
}

fn open_main_window(
    cx: &mut App,
    syntaxes: subtext_syntax::SyntaxManager,
    themes: subtext_theme::ThemeManager,
    settings: Settings,
    paths: Vec<PathBuf>,
) {
    let options = WindowOptions {
        window_min_size: Some(gpui_kit::size(px(720.), px(480.))),
        ..Default::default()
    };

    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(syntaxes, themes, settings, window, cx)
        });
        bridge::register(workspace.downgrade(), cx);
        workspace
    });

    let (handle, workspace) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            log::error!("could not open the main window: {error}");
            eprintln!("subtext: could not open the main window: {error}");
            return;
        }
    };

    if paths.is_empty() {
        return;
    }

    // Replace the empty starter document rather than adding to it, so opening
    // one file on the command line leaves one tab, not two.
    handle
        .update(cx, move |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                for path in paths {
                    workspace.open_path(path, window, cx);
                }
            });
        })
        .ok();
}
