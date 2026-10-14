//! The menu bar, and the one place it is rebuilt from.
//!
//! [`menu::app_menus`] is a compile-time table: every item CotEditor names in
//! its `Main.storyboard`, and the action each one runs. Five of its submenus are
//! not in the storyboard at all, though. CotEditor fills those in from the
//! document in front and from the settings, and a compile-time table cannot name
//! a syntax or a theme the user has not installed yet.
//!
//! So the generator leaves those five empty and disabled, and this module fills
//! them in. [`install`] is the only place the bar is set: it clears the keymap,
//! lays the built-in bindings back down, adds the snippets' own, and sets the
//! menu bar with every submenu filled in.
//!
//! # When it runs
//!
//! The submenus show a check beside the setting in force, so the bar depends on
//! the document in front as well as on the settings.
//! [`Workspace::refresh_menus`](crate::workspace::Workspace::refresh_menus) is
//! the caller for that: it compares a [`MenuSignature`] and returns early when
//! nothing it cares about has moved, because a rebuild is a full
//! `App::set_menus` over 258 items and every tab switch goes through it.
//! [`Workspace::rebuild_menus`](crate::workspace::Workspace::rebuild_menus) is
//! for the other case — the settings themselves changed, so there is nothing
//! worth comparing.
//!
//! # Naming an item the runtime does not know
//!
//! An item in one of these submenus cannot name its action at compile time, so
//! each one carries the value it stands for: a line ending's abbreviation, a
//! syntax's name, a theme's name. Those actions live in [`crate::actions`], and
//! [`crate::bridge`] turns each value back into the thing it names. Naming one
//! by its *position* would make an item run the wrong command the moment the
//! list above it changed.
//!
//! # The keymap
//!
//! Snippets are the reason the keymap is rebuilt here too: a snippet may carry a
//! key equivalent, and GPUI's keymap can add bindings but not remove individual
//! ones. Upstream refuses to store a shortcut another menu command already has —
//! and refuses it in the settings pane, where the user can be told which command
//! it was — but a settings file edited by hand can still hold one, so [`install`]
//! drops it here as well: a snippet must never take a key that ⌘S or ⌘F already
//! answers to.
//!
//! # Where this differs from CotEditor
//!
//! *NEL, LS and PS are always listed.* Upstream shows only the three line
//! endings a document is normally written with and reveals the other three while
//! the Option key is held — its `OptionalMenu` watches the modifier flags for as
//! long as the menu is open. GPUI's menu API has no hidden items and no
//! modifier tracking, so the six are always listed. Every choice upstream offers
//! is offered here; three of them take fewer keystrokes.

use gpui_kit::{App, Menu, MenuItem};

use subtext_core::line_ending::ALL as LINE_ENDINGS;
use subtext_core::snippets::Snippet;
use subtext_core::{LineEnding, Sections, TextEncoding};

use crate::actions;
use crate::keymap;
use crate::menu;
use crate::snippets;

/// The submenu names the runtime fills in, as `resources/menu.json` spells them.
///
/// The generator leaves each of these as an empty, disabled submenu, and the
/// names are unique across the whole bar — which is what lets [`replace`] find
/// one without being told the path it sits on.
const LINE_ENDINGS_MENU: &str = "Line Endings";
const SYNTAX_MENU: &str = "Syntax";
const THEME_MENU: &str = "Theme";
const INSERT_SNIPPET_MENU: &str = "Insert Snippet";

/// The syntax name that means "no highlighting".
///
/// Upstream's `SyntaxName.none`, and the reason the submenu has a `None` item at
/// all: a document with no syntax is a real state, not the absence of a choice.
const NO_SYNTAX: &str = "None";

/// The document in front, as the menu bar needs to see it.
///
/// A value rather than a reference to the document, because the bar is built
/// from inside a command, where the workspace is already borrowed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DocumentState {
    /// The syntax the document is highlighted with.
    pub syntax: String,
    /// The line endings the document uses.
    pub line_ending: LineEnding,
    /// The encoding the document will be written in.
    pub encoding: TextEncoding,
}

/// Everything the menu bar is built from.
pub struct MenuState {
    /// The document in front, or `None` when there is none.
    pub document: Option<DocumentState>,
    /// The theme in force. It belongs to the application, not to a document.
    pub theme: String,
    /// Every syntax, in the order the Format ▸ Syntax submenu lists them.
    pub syntaxes: Vec<String>,
    /// Every theme, in the order the Format ▸ Theme submenu lists them.
    pub themes: Vec<String>,
    /// The user's snippets.
    pub snippets: Vec<Snippet>,
}

/// The part of a [`MenuState`] that changes without the settings changing.
///
/// Kept apart from the state itself so that the comparison which decides whether
/// a rebuild is needed does not have to clone the syntax list, the theme list
/// and every snippet on each tab switch.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MenuSignature {
    document: Option<DocumentState>,
    theme: String,
}

impl MenuSignature {
    /// The signature of a bar showing `document` under `theme`.
    pub fn of(document: Option<DocumentState>, theme: impl Into<String>) -> Self {
        Self {
            document,
            theme: theme.into(),
        }
    }
}

impl MenuState {
    /// The part of this state that [`MenuSignature`] covers.
    pub fn signature(&self) -> MenuSignature {
        MenuSignature::of(self.document.clone(), self.theme.clone())
    }
}

/// Rebuild the menu bar, and the key bindings that go with it.
pub fn install(cx: &mut App, state: &MenuState) {
    let menus = build(state);

    cx.clear_key_bindings();
    cx.bind_keys(keymap::default_bindings());

    // The shortcuts are checked against the keymap the built-in bindings have
    // just filled, which is what makes "already taken" mean the same thing here
    // as it does in the settings pane. The borrow has to end before the bindings
    // are added: `bind_keys` borrows the same keymap mutably.
    let added = {
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        snippets::bindings(&keymap, &Sections::of(&state.snippets, scope(state)))
    };

    cx.bind_keys(added);
    // After the keymap, because the platform reads a menu item's shortcut out of
    // it.
    cx.set_menus(menus);
}

/// The syntax a snippet is matched against: the document in front's, if any.
fn scope(state: &MenuState) -> Option<&str> {
    state
        .document
        .as_ref()
        .map(|document| document.syntax.as_str())
}

/// The whole menu bar, with every runtime submenu filled in.
pub fn build(state: &MenuState) -> Vec<Menu> {
    let mut menus = menu::app_menus();

    replace(
        &mut menus,
        INSERT_SNIPPET_MENU,
        snippets::items(&Sections::of(&state.snippets, scope(state))),
    );
    replace(&mut menus, LINE_ENDINGS_MENU, line_endings(state));
    replace(&mut menus, SYNTAX_MENU, syntaxes(state));
    replace(&mut menus, THEME_MENU, themes(state));

    menus
}

/// Replace the items of the submenu called `name`, wherever it sits.
///
/// The names are unique across the bar, which is what lets this find one without
/// being handed the path it is on: the table is generated, so a path written
/// here would be a second copy of the storyboard's structure to keep in step.
///
/// Returns whether it found one. A miss would leave the submenu empty and
/// disabled, so the tests check every name against the generated table.
fn replace(menus: &mut [Menu], name: &str, items: Vec<MenuItem>) -> bool {
    let mut items = items;
    menus.iter_mut().any(|menu| splice(menu, name, &mut items))
}

/// The recursive half of [`replace`], which walks into submenus.
fn splice(menu: &mut Menu, name: &str, items: &mut Vec<MenuItem>) -> bool {
    for item in &mut menu.items {
        let MenuItem::Submenu(submenu) = item else {
            continue;
        };
        if &*submenu.name == name {
            submenu.items = std::mem::take(items);
            // The generator leaves the placeholder disabled, which is what keeps
            // an empty submenu from reading as a broken one before the first
            // document is open. It is no longer empty, so it is no longer
            // disabled.
            submenu.disabled = false;
            return true;
        }
        if splice(submenu, name, items) {
            return true;
        }
    }
    false
}

// ---- the submenus --------------------------------------------------------

/// Format ▸ Line Endings.
///
/// Six items in the order upstream enumerates `LineEnding`, each titled the way
/// its Format settings picker titles them — `"Windows (CRLF)"` — with the one
/// the document uses checked. See the module comment for why the three upstream
/// hides are listed here.
///
/// Every item is disabled when no document is open: there is nothing to convert.
fn line_endings(state: &MenuState) -> Vec<MenuItem> {
    let current = state.document.as_ref().map(|document| document.line_ending);

    LINE_ENDINGS
        .into_iter()
        .map(|ending| {
            let title = format!(
                "{} ({})",
                ending.platform_description(),
                ending.abbreviation()
            );
            MenuItem::action(
                title,
                actions::format::ChangeLineEnding {
                    ending: ending.abbreviation().to_string(),
                },
            )
            .checked(current == Some(ending))
            .disabled(current.is_none())
        })
        .collect()
}

/// Format ▸ Syntax.
///
/// *None* first, then a separator, then every syntax the user has — upstream's
/// order, and its shape: the separator is what keeps "no highlighting" from
/// reading as just another language. The one the document uses is checked.
///
/// Upstream also hides the syntaxes the user has asked not to see, and greys the
/// whole menu out when no document can take a syntax. SubText has no per-syntax
/// visibility setting, so the second half is all that applies.
fn syntaxes(state: &MenuState) -> Vec<MenuItem> {
    let current = state
        .document
        .as_ref()
        .map(|document| document.syntax.as_str());

    let mut items = Vec::with_capacity(state.syntaxes.len() + 2);
    items.push(
        MenuItem::action(
            NO_SYNTAX,
            actions::format::ChangeSyntax {
                name: NO_SYNTAX.to_string(),
            },
        )
        .checked(current == Some(NO_SYNTAX))
        .disabled(current.is_none()),
    );
    items.push(MenuItem::separator());
    items.extend(state.syntaxes.iter().map(|name| {
        MenuItem::action(
            name.clone(),
            actions::format::ChangeSyntax { name: name.clone() },
        )
        .checked(current == Some(name.as_str()))
        .disabled(current.is_none())
    }));
    items
}

/// Format ▸ Theme.
///
/// The theme list has no "none": a document always has a theme. The one in force
/// is checked, and the items stay live with no document open, because the theme
/// belongs to the application rather than to a document.
fn themes(state: &MenuState) -> Vec<MenuItem> {
    state
        .themes
        .iter()
        .map(|name| {
            MenuItem::action(
                name.clone(),
                actions::format::ChangeTheme { name: name.clone() },
            )
            .checked(*name == state.theme)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use subtext_core::snippets::Shortcut;

    fn document() -> DocumentState {
        DocumentState {
            syntax: "Markdown".into(),
            line_ending: LineEnding::CRLF,
            encoding: TextEncoding::UTF8,
        }
    }

    fn state() -> MenuState {
        MenuState {
            document: Some(document()),
            theme: "Solarized Dark".into(),
            syntaxes: vec!["HTML".into(), "Markdown".into(), "Ruby".into()],
            themes: vec!["Solarized Dark".into(), "Solarized Light".into()],
            snippets: Vec::new(),
        }
    }

    fn snippet(id: u64, name: &str, scope: Option<&str>, spec: Option<&str>) -> Snippet {
        Snippet {
            id,
            name: name.into(),
            scope: scope.map(str::to_string),
            shortcut: spec.and_then(Shortcut::parse),
            format: String::new(),
        }
    }

    /// The submenu called `name`, wherever it is.
    fn submenu<'a>(menus: &'a [Menu], name: &str) -> &'a Menu {
        fn find<'a>(menu: &'a Menu, name: &str) -> Option<&'a Menu> {
            menu.items.iter().find_map(|item| match item {
                MenuItem::Submenu(submenu) if &*submenu.name == name => Some(submenu),
                MenuItem::Submenu(submenu) => find(submenu, name),
                _ => None,
            })
        }
        menus
            .iter()
            .find_map(|menu| find(menu, name))
            .unwrap_or_else(|| panic!("no {name} submenu"))
    }

    /// The labels of a submenu, with separators marked and disabled items in
    /// brackets so the shape of the list is visible.
    fn labels(menus: &[Menu], name: &str) -> Vec<String> {
        submenu(menus, name)
            .items
            .iter()
            .map(|item| match item {
                MenuItem::Separator => "―".to_string(),
                MenuItem::Action { name, disabled, .. } => {
                    if *disabled {
                        format!("[{name}]")
                    } else {
                        name.to_string()
                    }
                }
                MenuItem::SystemMenu(_) => "?".to_string(),
                MenuItem::Submenu(submenu) => format!("({})", submenu.name),
            })
            .collect()
    }

    /// The checked label of a submenu, if it has one.
    fn checked(menus: &[Menu], name: &str) -> Option<String> {
        submenu(menus, name)
            .items
            .iter()
            .find(|item| item.is_checked())
            .map(|item| match item {
                MenuItem::Action { name, .. } => name.to_string(),
                _ => unreachable!("only actions can be checked"),
            })
    }

    /// Every placeholder the generator leaves behind is one this module fills
    /// in. A miss would leave a submenu permanently empty and disabled, which is
    /// exactly the state these four exist to leave behind.
    #[test]
    fn every_runtime_submenu_is_replaced() {
        for name in [
            LINE_ENDINGS_MENU,
            SYNTAX_MENU,
            THEME_MENU,
            INSERT_SNIPPET_MENU,
        ] {
            let menus = build(&state());
            assert!(!submenu(&menus, name).items.is_empty(), "{name} is empty");
            assert!(
                !submenu(&menus, name).disabled,
                "{name} is still disabled"
            );
        }
    }

    /// Upstream's `LineEnding.allCases`, in its order, titled as its Format
    /// settings picker titles them.
    #[test]
    fn the_line_endings_are_upstream_s_six_in_upstream_s_order() {
        let menus = build(&state());
        assert_eq!(
            labels(&menus, LINE_ENDINGS_MENU),
            [
                "macOS / Unix (LF)",
                "Classic Mac OS (CR)",
                "Windows (CRLF)",
                "Unix Next Line (NEL)",
                "Unix Line Separator (LS)",
                "Unix Paragraph Separator (PS)",
            ]
        );
    }

    #[test]
    fn the_line_ending_the_document_uses_is_the_checked_one() {
        let menus = build(&state());
        assert_eq!(
            checked(&menus, LINE_ENDINGS_MENU).as_deref(),
            Some("Windows (CRLF)")
        );
    }

    /// With nothing open there is nothing to convert, and no setting to check.
    #[test]
    fn a_document_less_bar_disables_the_document_submenus() {
        let mut state = state();
        state.document = None;
        let menus = build(&state);

        assert!(submenu(&menus, LINE_ENDINGS_MENU).items.iter().all(MenuItem::is_disabled));
        assert!(submenu(&menus, SYNTAX_MENU).items.iter().all(|item| {
            // The separator is the one item with no disabled flag to set.
            matches!(item, MenuItem::Separator) || item.is_disabled()
        }));
        assert_eq!(checked(&menus, LINE_ENDINGS_MENU), None);
        assert_eq!(checked(&menus, SYNTAX_MENU), None);
    }

    /// The theme is the application's, so it is still chosen — and still
    /// selectable — with no document open.
    #[test]
    fn the_theme_menu_checks_the_theme_in_force_without_a_document() {
        let mut state = state();
        state.document = None;
        let menus = build(&state);

        assert_eq!(
            checked(&menus, THEME_MENU).as_deref(),
            Some("Solarized Dark")
        );
        assert!(submenu(&menus, THEME_MENU).items.iter().all(|item| !item.is_disabled()));
    }

    /// `None` first, behind a separator, then every syntax — and the document's
    /// own is the one checked.
    #[test]
    fn the_syntax_menu_puts_none_first_behind_a_separator() {
        let menus = build(&state());
        assert_eq!(
            labels(&menus, SYNTAX_MENU),
            ["None", "―", "HTML", "Markdown", "Ruby"]
        );
        assert_eq!(checked(&menus, SYNTAX_MENU).as_deref(), Some("Markdown"));
    }

    #[test]
    fn a_document_with_no_syntax_checks_none() {
        let mut state = state();
        state.document.as_mut().unwrap().syntax = NO_SYNTAX.to_string();
        assert_eq!(checked(&build(&state), SYNTAX_MENU).as_deref(), Some(NO_SYNTAX));
    }

    /// A menu item whose action nothing handles is greyed out by GPUI, so every
    /// item in these submenus has to name a real action — and the action has to
    /// carry the value the item stands for, or it would run some other one.
    #[test]
    fn every_line_ending_item_names_the_ending_it_stands_for() {
        let menus = build(&state());
        let named: Vec<String> = submenu(&menus, LINE_ENDINGS_MENU)
            .items
            .iter()
            .map(|item| {
                let MenuItem::Action { action, .. } = item else {
                    panic!("a line ending item is not an action")
                };
                action
                    .as_any()
                    .downcast_ref::<actions::format::ChangeLineEnding>()
                    .expect("a line ending item runs ChangeLineEnding")
                    .ending
                    .clone()
            })
            .collect();
        assert_eq!(named, ["LF", "CR", "CRLF", "NEL", "LS", "PS"]);
    }

    #[test]
    fn every_syntax_item_names_the_syntax_it_stands_for() {
        let menus = build(&state());
        let named: Vec<String> = submenu(&menus, SYNTAX_MENU)
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { action, .. } => Some(
                    action
                        .as_any()
                        .downcast_ref::<actions::format::ChangeSyntax>()
                        .expect("a syntax item runs ChangeSyntax")
                        .name
                        .clone(),
                ),
                _ => None,
            })
            .collect();
        assert_eq!(named, ["None", "HTML", "Markdown", "Ruby"]);
    }

    // ---- the snippet submenu ---------------------------------------------

    #[test]
    fn an_empty_snippet_list_leaves_just_the_editor_item() {
        let menus = build(&state());
        assert_eq!(labels(&menus, INSERT_SNIPPET_MENU), ["Edit Snippets…"]);
    }

    #[test]
    fn the_snippet_submenu_lists_the_general_group_then_the_syntax_heading() {
        let mut state = state();
        state.snippets = vec![
            snippet(1, "Header", None, None),
            // For a syntax that is not the document's, so it belongs to a
            // heading the menu does not show.
            snippet(2, "List", Some("HTML"), None),
            snippet(3, "Emphasis", Some("Markdown"), None),
        ];
        assert_eq!(
            labels(&build(&state), INSERT_SNIPPET_MENU),
            ["Header", "―", "[Markdown]", "Emphasis", "―", "Edit Snippets…"]
        );
    }

    #[test]
    fn a_document_without_a_syntax_has_no_heading() {
        let mut state = state();
        state.document = None;
        state.snippets = vec![
            snippet(1, "Header", None, None),
            snippet(2, "List", Some("HTML"), None),
        ];
        assert_eq!(
            labels(&build(&state), INSERT_SNIPPET_MENU),
            ["Header", "―", "Edit Snippets…"]
        );
    }

    #[test]
    fn every_snippet_item_runs_its_own_snippet() {
        let mut state = state();
        state.snippets = vec![snippet(7, "Header", None, None)];
        let menus = build(&state);

        let found = submenu(&menus, INSERT_SNIPPET_MENU)
            .items
            .iter()
            .find_map(|item| match item {
                MenuItem::Action { action, .. } => action
                    .as_any()
                    .downcast_ref::<actions::snippet::InsertSnippet>()
                    .map(|action| action.id),
                _ => None,
            });
        assert_eq!(found, Some(7));
    }

    // ---- the signature ---------------------------------------------------

    /// The signature has to cover everything a rebuild is needed for, or a tab
    /// switch would leave the checks on the wrong document.
    #[test]
    fn the_signature_follows_the_document_and_the_theme() {
        let before = state().signature();

        let mut resyntaxed = state();
        resyntaxed.document.as_mut().unwrap().syntax = "Ruby".into();
        assert_ne!(resyntaxed.signature(), before, "a syntax change was missed");

        let mut reended = state();
        reended.document.as_mut().unwrap().line_ending = LineEnding::LF;
        assert_ne!(reended.signature(), before, "a line ending change was missed");

        let mut reencoded = state();
        reencoded.document.as_mut().unwrap().encoding = TextEncoding::UTF16_LE;
        assert_ne!(reencoded.signature(), before, "an encoding change was missed");

        let mut rethemed = state();
        rethemed.theme = "Solarized Light".into();
        assert_ne!(rethemed.signature(), before, "a theme change was missed");

        // And with the document closed the checks come off, which is a change
        // too.
        let mut closed = state();
        closed.document = None;
        assert_ne!(closed.signature(), before, "closing the document was missed");
    }

    /// The tables are not part of it: they change only when the settings do, and
    /// comparing them on every tab switch is what the signature exists to avoid.
    #[test]
    fn the_signature_ignores_the_tables() {
        let mut state = state();
        let before = state.signature();

        state.syntaxes.push("Python".into());
        state.themes.push("Novel".into());
        state.snippets = vec![snippet(1, "Header", None, None)];
        assert_eq!(state.signature(), before);
    }
}
