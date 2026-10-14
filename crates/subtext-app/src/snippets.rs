//! The *Insert Snippet* submenu, and the key bindings behind it.
//!
//! Snippets are the user's data, which puts them in an awkward place: the menu
//! bar and the keymap are both built before anything is opened, and neither can
//! name an item the user has not written yet.
//!
//! So both are rebuilt whenever the answer changes. [`install`] clears the
//! keymap, lays the built-in bindings back down, adds the snippets' own, and
//! sets the menu bar with the submenu filled in. That is affordable because the
//! alternative — keeping the snippet bindings separate and removing only those
//! — is not something GPUI's keymap offers, and because neither table is large:
//! 72 bindings and 258 menu items.
//!
//! # Two things the caller has to decide
//!
//! *The scope.* A snippet may be limited to one syntax, and the submenu shows
//! only the ones that apply, so it depends on the document in front. The caller
//! passes the active document's syntax name, or `None` when there is no
//! document — see [`Workspace::snippets_scope`](crate::workspace::Workspace).
//!
//! *The command a shortcut belongs to.* Upstream refuses to store a shortcut
//! that another menu command already has, and refuses it in the settings pane,
//! where the user can be told which command it was. A settings file edited by
//! hand can still hold one, so [`install`] drops it here as well: a snippet must
//! never take a key that ⌘S or ⌘F already answers to.

use gpui_kit::{App, KeyBinding, Keystroke, Menu, MenuItem};

use subtext_core::Sections;
use subtext_core::snippets::Snippet;

use crate::actions;
use crate::keymap;
use crate::menu;

/// The submenu the snippet items belong to, as `resources/menu.json` spells it.
const SUBMENU: &str = "Insert Snippet";

/// Rebuild the submenu and the key bindings for `snippets`.
///
/// `scope` is the active document's syntax name, or `None` when no document is
/// open — see the module comment.
pub fn install(cx: &mut App, snippets: &[Snippet], scope: Option<&str>) {
    let sections = Sections::of(snippets, scope);
    let menus = with_snippets(menu::app_menus(), &sections);

    cx.clear_key_bindings();
    cx.bind_keys(keymap::default_bindings());

    // The shortcuts are checked against the keymap the built-in bindings have
    // just filled, which is what makes "already taken" mean the same thing here
    // as it does in the settings pane. The borrow has to end before the
    // bindings are added: `bind_keys` borrows the same keymap mutably.
    let added = {
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        bindings(&keymap, &sections)
    };

    cx.bind_keys(added);
    // After the keymap, because the platform reads a menu item's shortcut out
    // of it.
    cx.set_menus(menus);
}

/// The snippet bindings to add on top of `keymap`'s own.
fn bindings(keymap: &gpui_kit::Keymap, sections: &Sections) -> Vec<KeyBinding> {
    sections
        .bindings()
        .into_iter()
        .filter(|(_, keystroke)| free(keymap, keystroke))
        .map(|(id, keystroke)| {
            KeyBinding::new(&keystroke, actions::snippet::InsertSnippet { id }, None)
        })
        .collect()
}

/// Whether the built-in bindings leave `keystroke` alone.
///
/// `KeyBinding::new` panics on a keystroke GPUI cannot read, so the parse is
/// also what keeps a hand-written shortcut in the settings file from taking the
/// application down at launch. `Keystroke::parse` is not strict enough on its
/// own: `""` and `"secondary-"` both come back with no key at all, and a
/// binding on one of those would never match anything.
fn free(keymap: &gpui_kit::Keymap, keystroke: &str) -> bool {
    Keystroke::parse(keystroke).is_ok_and(|keystroke| {
        !keystroke.key.is_empty() && keymap.all_bindings_for_input(&[keystroke]).is_empty()
    })
}

// ---- the menu ------------------------------------------------------------

/// The menu bar with the *Insert Snippet* submenu filled in.
fn with_snippets(mut menus: Vec<Menu>, sections: &Sections) -> Vec<Menu> {
    for menu in &mut menus {
        if splice(menu, sections) {
            break;
        }
    }
    menus
}

/// Replace the submenu's items, and say whether this menu held it.
fn splice(menu: &mut Menu, sections: &Sections) -> bool {
    for item in &mut menu.items {
        let MenuItem::Submenu(submenu) = item else {
            continue;
        };
        if &*submenu.name == SUBMENU {
            submenu.items = items(sections);
            return true;
        }
        if splice(submenu, sections) {
            return true;
        }
    }
    false
}

/// The items the submenu holds, in upstream's order.
///
/// The general snippets come first with no heading, then a heading naming the
/// document's syntax with that syntax's snippets under it, then the separator
/// that closes each group, and *Edit Snippets…* last. A group with nothing in it
/// contributes neither its items nor its heading.
fn items(sections: &Sections) -> Vec<MenuItem> {
    let mut items = Vec::with_capacity(sections.general.len() + sections.scoped.len() + 4);

    if !sections.general.is_empty() {
        items.extend(sections.general.iter().map(|snippet| entry(snippet)));
        items.push(MenuItem::separator());
    }

    if !sections.scoped.is_empty()
        && let Some(scope) = sections.scope
    {
        // GPUI's menus have no section header — upstream's
        // `NSMenuItem.sectionHeader` — so the heading is a disabled item: it
        // reads as a label and cannot be chosen.
        items.push(MenuItem::action(scope, actions::app::NoOp).disabled(true));
        items.extend(sections.scoped.iter().map(|snippet| entry(snippet)));
        items.push(MenuItem::separator());
    }

    items.push(MenuItem::action("Edit Snippets…", actions::text::ShowSnippetEditor));
    items
}

fn entry(snippet: &Snippet) -> MenuItem {
    MenuItem::action(
        snippet.name.clone(),
        actions::snippet::InsertSnippet { id: snippet.id },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use subtext_core::Shortcut;

    fn snippet(id: u64, name: &str, scope: Option<&str>, spec: Option<&str>) -> Snippet {
        Snippet {
            id,
            name: name.into(),
            scope: scope.map(str::to_string),
            shortcut: spec.and_then(Shortcut::parse),
            format: String::new(),
        }
    }

    /// The labels of the *Insert Snippet* submenu, with separators and the
    /// heading marked so the shape is visible.
    fn labels(menus: &[Menu]) -> Vec<String> {
        fn find(menu: &Menu) -> Option<&Menu> {
            menu.items.iter().find_map(|item| match item {
                MenuItem::Submenu(submenu) if &*submenu.name == SUBMENU => Some(submenu),
                MenuItem::Submenu(submenu) => find(submenu),
                _ => None,
            })
        }

        let submenu = menus.iter().find_map(find).expect("no Insert Snippet submenu");
        submenu
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

    #[test]
    fn an_empty_snippet_list_leaves_the_generated_menu_alone() {
        let menus = with_snippets(menu::app_menus(), &Sections::of(&[], None));
        assert_eq!(labels(&menus), ["Edit Snippets…"]);
    }

    #[test]
    fn the_submenu_lists_the_general_group_then_the_syntax_heading() {
        let snippets = [
            snippet(1, "Header", None, None),
            snippet(2, "List", Some("HTML"), None),
            snippet(3, "Ruby", Some("Ruby"), None),
        ];
        let menus = with_snippets(
            menu::app_menus(),
            &Sections::of(&snippets, Some("HTML")),
        );
        assert_eq!(
            labels(&menus),
            ["Header", "―", "[HTML]", "List", "―", "Edit Snippets…"]
        );
    }

    #[test]
    fn a_document_without_a_syntax_has_no_heading() {
        let snippets = [
            snippet(1, "Header", None, None),
            snippet(2, "List", Some("HTML"), None),
        ];
        let menus = with_snippets(menu::app_menus(), &Sections::of(&snippets, None));
        assert_eq!(labels(&menus), ["Header", "―", "Edit Snippets…"]);
    }

    /// Every menu item has to name an action something can handle, or GPUI
    /// greys it out and the snippet becomes unreachable.
    #[test]
    fn every_item_runs_a_snippet() {
        let snippets = [snippet(7, "Header", None, None)];
        let menus = with_snippets(menu::app_menus(), &Sections::of(&snippets, None));

        let found = menus.iter().find_map(|menu| {
            menu.items.iter().find_map(|item| match item {
                MenuItem::Submenu(submenu) if &*submenu.name == SUBMENU => {
                    submenu.items.iter().find_map(|item| match item {
                        MenuItem::Action { action, .. } => action
                            .as_any()
                            .downcast_ref::<actions::snippet::InsertSnippet>()
                            .map(|action| action.id),
                        _ => None,
                    })
                }
                _ => None,
            })
        });
        assert_eq!(found, Some(7));
    }

    /// A shortcut the built-ins already answer to is not bound, which is what
    /// upstream's settings pane refuses to store in the first place.
    #[test]
    fn a_shortcut_a_built_in_command_has_is_left_alone() {
        assert!(!free(&keymap_of_defaults(), "secondary-s"));
        assert!(!free(&keymap_of_defaults(), "secondary-f"));
        assert!(free(&keymap_of_defaults(), "alt-shift-secondary-k"));
    }

    /// A keystroke GPUI cannot read would panic `KeyBinding::new`, so it has to
    /// be caught here rather than at launch.
    #[test]
    fn a_keystroke_the_keymap_cannot_read_is_not_bound() {
        assert!(!free(&keymap_of_defaults(), "not-a-keystroke"));
        assert!(!free(&keymap_of_defaults(), ""));
        assert!(!free(&keymap_of_defaults(), "secondary-"));
    }

    fn keymap_of_defaults() -> gpui_kit::Keymap {
        gpui_kit::Keymap::new(keymap::default_bindings())
    }

    /// Which snippets got a binding, and the keystroke each one answers.
    ///
    /// The match is asserted rather than assumed: a binding whose keystroke the
    /// keymap cannot resolve would be a binding that never fires.
    fn bound(sections: &Sections) -> Vec<(u64, String)> {
        let keymap = keymap_of_defaults();
        bindings(&keymap, sections)
            .into_iter()
            .map(|binding| {
                let id = binding
                    .action()
                    .as_any()
                    .downcast_ref::<actions::snippet::InsertSnippet>()
                    .expect("a snippet binding runs a snippet")
                    .id;
                let keystroke = sections
                    .bindings()
                    .into_iter()
                    .find(|(bound, _)| *bound == id)
                    .expect("the snippet was bound without being offered")
                    .1;
                let typed = gpui_kit::Keystroke::parse(&keystroke).unwrap();
                assert_eq!(
                    binding.match_keystrokes(&[typed]),
                    Some(false),
                    "{keystroke} does not match the binding it was made for"
                );
                (id, keystroke)
            })
            .collect()
    }

    #[test]
    fn the_bindings_are_the_snippets_the_submenu_offers() {
        let snippets = [
            snippet(1, "Header", None, Some("@$u")),
            snippet(2, "List", Some("HTML"), Some("@u")),
            snippet(3, "Ruby", Some("Ruby"), Some("@r")),
        ];
        let sections = Sections::of(&snippets, Some("HTML"));
        assert_eq!(
            bound(&sections),
            [(1, "secondary-shift-u".to_string()), (2, "secondary-u".to_string())]
        );
    }

    /// A shortcut that a built-in command answers to is dropped, and the rest of
    /// the list still binds.
    #[test]
    fn a_snippet_cannot_take_a_built_in_command_s_keystroke() {
        let snippets = [
            snippet(1, "Steals Save", None, Some("@s")),
            snippet(2, "Steals Find", None, Some("@f")),
            snippet(3, "Steals Highlight", None, Some("@$h")),
            snippet(4, "Free", None, Some("@$u")),
        ];
        let sections = Sections::of(&snippets, None);
        assert_eq!(bound(&sections), [(4, "secondary-shift-u".to_string())]);
    }
}
