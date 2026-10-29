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
pub mod command_bar;
pub mod console;
pub mod count_panel;
pub mod dialogs;
pub mod dispatch;
pub mod encoding_list_sheet;
pub mod find_panel;
pub mod folder_find;
pub mod highlighter;
pub mod keymap;
pub mod menu;
#[cfg(test)]
mod menu_contract;
pub mod menus;
pub mod multiple_replace_sheet;
pub mod outline;
pub mod resources;
pub mod scripts;
pub mod settings;
pub mod settings_window;
pub mod snippet_pane;
pub mod snippets;
pub mod sort_sheet;
pub mod surround_sheet;
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

            // The console exists for the life of the process: a script's
            // standard error is written to it whether or not its window is
            // open, so it has to exist before the first script can run.
            console::ConsoleHandle::init(cx);

            // The highlighter factory has to be in place before the first
            // editor is built, or the first document would open unhighlighted.
            // It is keyed by language name and holds its own copy of the
            // syntax list, so the workspace keeps the manager for lookups.
            highlighter::install_factory(std::sync::Arc::new(syntaxes.all().to_vec()), cx);

            // CotEditor applies the saved theme at launch; so does the editor,
            // which means the component layer has to be told as well as the
            // text. With nothing saved the default theme is the one in force,
            // and it has to reach the component layer too — the document pane
            // paints from there, so skipping it leaves the editor black on
            // white whatever the CotEditor theme says.
            let theme = settings
                .theme
                .as_deref()
                .and_then(|name| themes.by_name(name))
                .cloned()
                .unwrap_or_else(|| themes.default_theme().clone());
            let mode = if theme.is_dark() {
                gpui_kit::component::ThemeMode::Dark
            } else {
                gpui_kit::component::ThemeMode::Light
            };
            gpui_kit::component::Theme::change(mode, None, cx);
            highlighter::project_theme(&theme, cx);

            bridge::install(cx);
            // The keymap and the menu bar, with the saved snippets already in
            // them. The workspace rebuilds both as documents come and go; this is
            // what gives the application a menu before any window exists.
            //
            // No document is open yet, so the submenus that follow one are filled
            // with items that cannot be chosen — which is what they would be in
            // CotEditor with every document closed.
            let menu_state = menus::MenuState {
                document: None,
                gates: menus::MenuGates::default(),
                theme: settings
                    .theme
                    .clone()
                    .unwrap_or_else(|| themes.default_theme().name.clone()),
                syntaxes: syntaxes
                    .all()
                    .iter()
                    .map(|syntax| syntax.name.clone())
                    .collect(),
                themes: themes
                    .all()
                    .iter()
                    .map(|theme| theme.name.clone())
                    .collect(),
                encoding_list: settings.encoding_list.clone(),
                snippets: settings.snippets.clone(),
                // The user's scripts, read before the first window exists so
                // that the Script menu is theirs from the first frame.
                scripts: scripts::scan(),
                // And the documents they have opened before, for the same
                // reason: File ▸ Open Recent is a list the bar has to carry from
                // the first frame, not one it grows into.
                recent_documents: settings.recent_documents.clone(),
            };
            menus::install(cx, &menu_state);

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
        let workspace = cx.new(|cx| Workspace::new(syntaxes, themes, settings, window, cx));
        bridge::register(workspace.downgrade(), window.window_handle(), cx);
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
                // A manual smoke for the panels no test can reach; see
                // `Workspace::panel_sweep`.
                // `SUBTEXT_PANEL_PEEK=<n>` opens panel n and stays there, so a
                // screenshot can be taken; `SUBTEXT_PANEL_SWEEP=1` opens all of
                // them and quits. Both are debug-only.
                #[cfg(debug_assertions)]
                match std::env::var("SUBTEXT_PANEL_PEEK")
                    .ok()
                    .and_then(|value| value.parse::<u32>().ok())
                {
                    Some(step) => workspace.panel_peek(step, cx),
                    None => {
                        if std::env::var_os("SUBTEXT_PANEL_SWEEP").is_some() {
                            workspace.panel_sweep(cx);
                        }
                    }
                }
            });
        })
        .ok();
}
