//! The interface language.
//!
//! CotEditor ships its interface in the system language, with a
//! `zh-Hans.lproj` among its resources. SubText does the same, and defaults to
//! Simplified Chinese rather than to English: this application's users read
//! Chinese first, and an editor that opens in a language they did not choose is
//! a worse first frame than one they have to switch away from.
//!
//! # How a string is translated
//!
//! [`t`] takes the English string — the one the source already holds — and
//! returns the translation for the active language, or the English string when
//! there is none. The English side is the key, so adding a language never
//! touches a call site: a call site is `t("Save")`, and the table decides what
//! the reader sees.
//!
//! The table is [`ZH_HANS`], a tab-separated file checked in beside the other
//! resources. Its menu half is CotEditor's own Simplified Chinese
//! `Main.strings`, joined against the storyboard; the rest is its `.xcstrings`
//! String Catalogs under `CotEditor/Localizables/`, read the same way. "CotEditor"
//! is replaced by "SubText" and no other wording changed — the interface has to
//! read the way upstream's does, not the way a machine would translate it.
//!
//! # Why the menu is translated after it is built
//!
//! [`crate::menus`] finds its runtime submenus (`Line Endings`, `Text Encoding`,
//! `Syntax`, …) by their *English* names, and [`crate::keymap`] keys every
//! shortcut by an English menu path. Translating the tree as it is built would
//! break both. So [`crate::menu::app_menus`] and [`crate::menus::build`] stay
//! English throughout, and [`localize_menus`] runs once at the very end —
//! between the finished tree and the platform that draws it.
//!
//! `Menu` and `MenuItem` own their strings and neither is `Clone`, so the pass
//! consumes the tree and rebuilds it. That is not a limitation to work around;
//! it is the reason the pass can be total. There is no item it forgets.
//!
//! # Where it is not translated
//!
//! Some strings are identifiers rather than labels, and translating them would
//! break something that matches on them: the Quick Actions command paths
//! ([`crate::menus::commands`]), the toolbar items' `slug()`, and
//! `SettingsPane::title()`, which is the rail's identity as well as its
//! heading. Those keep their English spelling and are translated at the point
//! they are *drawn*.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use gpui_kit::{Menu, MenuItem, OsMenu};
use serde::{Deserialize, Serialize};

/// The checked-in translation table.
const ZH_HANS: &str = include_str!("../../../resources/strings/zh-Hans.tsv");

/// The language the interface is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Simplified Chinese, and the default: this application's readers are
    /// Chinese-speaking, and the menus are already their language.
    #[default]
    Chinese,
    English,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::Chinese, Language::English];

    /// The name shown in the settings window, in the language itself.
    ///
    /// A language is named in its own language — an English reader looking for
    /// English should not have to find it spelled in Chinese. `English` is
    /// therefore the same in both, and `Chinese` is 简体中文.
    pub fn label(self) -> &'static str {
        match self {
            Language::Chinese => "简体中文",
            Language::English => "English",
        }
    }

    /// The BCP 47 tag, for anything that asks for one.
    pub fn tag(self) -> &'static str {
        match self {
            Language::Chinese => "zh-Hans",
            Language::English => "en",
        }
    }

    fn index(self) -> u8 {
        match self {
            Language::Chinese => 0,
            Language::English => 1,
        }
    }

    fn from_index(index: u8) -> Self {
        match index {
            1 => Language::English,
            _ => Language::Chinese,
        }
    }
}

/// The active language, for the whole process.
///
/// There is one window tree and one menu bar, so there is one language; a
/// per-window language would let two windows disagree about the menu bar they
/// share. An `AtomicU8` rather than a `Global` because [`t`] is called from
/// rendering and from menu construction, where there is no `App` to read a
/// global from.
static ACTIVE: AtomicU8 = AtomicU8::new(0);

/// The active language.
pub fn language() -> Language {
    Language::from_index(ACTIVE.load(Ordering::Relaxed))
}

/// Set the active language.
///
/// Called once at launch from the saved settings, and again whenever the
/// settings window changes it. The caller is responsible for redrawing — see
/// `Workspace::apply_settings` — because nothing here can notify a window.
pub fn set_language(language: Language) {
    ACTIVE.store(language.index(), Ordering::Relaxed);
}

/// The translation table, parsed once.
///
/// A `HashMap` rather than a perfect-hash or a sorted slice: the table is a few
/// hundred entries read at most once per drawn string, and a lookup that cannot
/// miss is worth more than a lookup that is a few nanoseconds faster. The keys
/// are `&'static str` borrowed from the `include_str!` above, so nothing is
/// allocated per lookup.
fn table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = HashMap::new();
        for line in ZH_HANS.lines() {
            // A comment, or the blank line the file ends on.
            let Some((english, translated)) = line.split_once('\t') else {
                continue;
            };
            if english.starts_with('#') || english.is_empty() {
                continue;
            }
            table.insert(english, translated);
        }
        table
    })
}

/// The translation of `english` for the active language.
///
/// Returns `english` unchanged when the language is English, or when the table
/// has no entry — an untranslated string is a string that still says what it
/// means, which is the right failure for an interface.
pub fn t(english: &str) -> &str {
    match language() {
        Language::English => english,
        Language::Chinese => table().get(english).copied().unwrap_or(english),
    }
}

/// A sentence with values filled into it, translated as a whole.
///
/// The key is the sentence the source spells, placeholders included, so the
/// translation is looked up *before* the values are put in: word order is the
/// translator's to choose, and a sentence assembled from translated fragments
/// in English order would read wrong in Chinese. A bare `{}` takes the next
/// value; `{1}`, `{2}`, … name a value by position, which is what a
/// translation needs when it moves a value past another — English's
/// `Found {1} matches in “{2}.”` becomes Chinese's `在“{2}”中找到{1}个匹配项。`
/// with the same values and no call-site change. Values beyond the
/// placeholders are dropped, and a `{}` left without a value stays as it is.
pub fn tf(english: &str, values: &[&dyn std::fmt::Display]) -> String {
    let sentence = t(english);
    let mut out = String::with_capacity(sentence.len());
    let mut rest = sentence;
    let mut next = 0usize;
    loop {
        let Some(open) = rest.find('{') else {
            out.push_str(rest);
            return out;
        };
        let Some(close) = rest[open..].find('}').map(|close| open + close) else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..open]);
        let name = &rest[open + 1..close];
        let index = match name {
            // A bare `{}` takes the next value.
            "" => {
                let index = next;
                next += 1;
                Some(index)
            }
            // A numbered one names its own, one-based, as a translation's
            // positional arguments do.
            number => number.parse::<usize>().ok().and_then(|n| n.checked_sub(1)),
        };
        match index.and_then(|index| values.get(index)) {
            Some(value) => out.push_str(&value.to_string()),
            // Nothing to fill it: the placeholder stays as the table spelled
            // it, rather than swallowing the rest of the sentence.
            None => out.push_str(&rest[open..=close]),
        }
        rest = &rest[close + 1..];
    }
}

/// A language held for the duration of a test.
///
/// The active language is one value for the whole process, and the test binary
/// runs tests on several threads, so a test that asserts a particular spelling
/// has to hold the language while it runs. The guard takes a process-wide
/// lock, sets the language, and puts the default back when it drops — which is
/// also when the next waiting test may set its own.
#[cfg(test)]
pub(crate) struct Pinned {
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for Pinned {
    fn drop(&mut self) {
        set_language(Language::default());
    }
}

/// Hold `language` until the returned guard drops.
#[cfg(test)]
pub(crate) fn pin(language: Language) -> Pinned {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // A test that panicked while holding the lock poisons it, but the language
    // it left behind is not a broken invariant — the next test sets its own.
    let lock = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    set_language(language);
    Pinned { _lock: lock }
}

/// The menu bar in the active language.
///
/// Consumes the tree and rebuilds it: `Menu` and `MenuItem` are not `Clone`, and
/// rebuilding is also what makes the pass exhaustive — a new `MenuItem` variant
/// would fail to compile here rather than slip through untranslated.
pub fn localize_menus(menus: Vec<Menu>) -> Vec<Menu> {
    menus.into_iter().map(localize_menu).collect()
}

fn localize_menu(menu: Menu) -> Menu {
    Menu {
        name: t(menu.name.as_str()).into(),
        items: menu.items.into_iter().map(localize_item).collect(),
        disabled: menu.disabled,
    }
}

fn localize_item(item: MenuItem) -> MenuItem {
    match item {
        MenuItem::Separator => MenuItem::Separator,
        MenuItem::Submenu(menu) => MenuItem::Submenu(localize_menu(menu)),
        MenuItem::SystemMenu(menu) => MenuItem::SystemMenu(OsMenu {
            name: t(menu.name.as_str()).into(),
            menu_type: menu.menu_type,
        }),
        MenuItem::Action {
            name,
            action,
            os_action,
            checked,
            disabled,
        } => MenuItem::Action {
            name: t(name.as_str()).into(),
            action,
            os_action,
            checked,
            disabled,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table has to be readable as a table: every entry has a translation,
    /// and no entry is its own key (which would mean a line was written twice).
    #[test]
    fn the_table_is_well_formed() {
        let table = table();
        assert!(
            table.len() > 200,
            "the table holds {} entries; the parser probably stopped reading it",
            table.len()
        );
        for (english, translated) in table {
            assert!(!translated.is_empty(), "{english} has an empty translation");
        }
    }

    /// The languages are all named, and none of them is named in the wrong
    /// language.
    #[test]
    fn every_language_has_a_label() {
        for language in Language::ALL {
            assert!(!language.label().is_empty());
            assert!(!language.tag().is_empty());
        }
    }

    /// The round trip through the stored index.
    #[test]
    fn the_active_language_round_trips() {
        let _language = pin(Language::Chinese);
        for chosen in Language::ALL {
            set_language(chosen);
            assert_eq!(language(), chosen);
        }
    }

    /// The menu pass reaches every item, including the ones inside submenus and
    /// the OS-managed Services menu.
    #[test]
    fn the_menu_pass_reaches_the_whole_tree() {
        let _language = pin(Language::Chinese);
        let menus = localize_menus(crate::menu::app_menus(&crate::menus::MenuGates::default()));
        let file = menus
            .iter()
            .find(|menu| menu.name.as_ref() == "文件")
            .expect("File is translated");
        assert!(
            file.items.iter().any(|item| matches!(
                item,
                MenuItem::Action { name, .. } if name.as_ref() == "保存"
            )),
            "File ▸ Save is translated"
        );
        let app = menus
            .iter()
            .find(|menu| menu.name.as_ref() == "SubText")
            .expect("the application menu is named");
        assert!(
            app.items.iter().any(|item| matches!(
                item,
                MenuItem::SystemMenu(menu) if menu.name.as_ref() == "服务"
            )),
            "the Services entry is translated too"
        );
    }

    /// English is the identity: the same tree, unchanged.
    #[test]
    fn english_leaves_the_menu_alone() {
        let _language = pin(Language::English);
        let before = crate::menu::app_menus(&crate::menus::MenuGates::default());
        let after = localize_menus(crate::menu::app_menus(&crate::menus::MenuGates::default()));
        assert_eq!(before.len(), after.len());
        for (before, after) in before.iter().zip(&after) {
            assert_eq!(before.name.as_ref(), after.name.as_ref());
            assert_eq!(before.items.len(), after.items.len());
        }
    }
}
