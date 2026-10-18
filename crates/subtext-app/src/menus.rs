//! The menu bar, and the one place it is rebuilt from.
//!
//! [`menu::app_menus`] is a compile-time table: every item CotEditor names in
//! its `Main.storyboard`, and the action each one runs. Eight of them cannot be
//! written down at compile time, though. Five are empty `<menu>` elements in the
//! storyboard itself — CotEditor fills those in from the document in front and
//! from the settings, and a table cannot name a syntax or a theme the user has
//! not installed yet. The sixth is *Insert Snippet*, whose items are the user's
//! own data. The seventh is *Script*, which is an empty *top-level* menu whose
//! items are the user's own files. The eighth is *Open Recent*, whose items are
//! the documents the user has opened — and upstream does not write those down
//! either: it marks the submenu `systemMenu="recentDocuments"` and lets
//! `NSDocumentController` fill it.
//!
//! So the generator leaves the five empty and disabled, and this module fills
//! them in — and rebuilds the other three. Two of those three have a storyboard
//! item that this module re-creates as part of its list — *Insert Snippet*'s
//! *Edit Snippets…*, *Open Recent*'s *Clear Menu* — and *Script* has none.
//! [`install`] is the only place the bar is set: it clears the keymap,
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
//! The Script menu is in the signature for a different reason: its items are
//! the files in a folder the user can add to at any time, and there is no event
//! to wait for. Reading the folder is one `read_dir`, which is cheap enough to
//! do on every refresh, and the comparison is what keeps a folder that has not
//! changed from rebuilding the bar.
//!
//! The recent documents are in it for the same reason, one step further out: the
//! list is reordered by opening a document, which is a thing that happens in
//! another part of the application entirely, and clearing it is a menu command
//! of its own. Both callers reach the bar through
//! [`Workspace::refresh_menus`](crate::workspace::Workspace::refresh_menus), and
//! the comparison is what makes that free when neither happened.
//!
//! # Naming an item the runtime does not know
//!
//! An item in one of these submenus cannot name its action at compile time, so
//! each one carries the value it stands for: a line ending's abbreviation, a
//! syntax's name, a theme's name, a script's path. Those actions live in
//! [`crate::actions`], and [`crate::bridge`] turns each value back into the
//! thing it names. Naming one by its *position* would make an item run the
//! wrong command the moment the list above it changed.
//!
//! # The Quick Actions bar's list
//!
//! [`commands`] reads that same bar and flattens it into the rows ⌘K searches,
//! which is what upstream's `NSApp.actionCommands` does — and for the same
//! reason: a table written at compile time cannot name a syntax or a script the
//! user has not installed yet, so the bar the user can see and the list the bar
//! can search would drift apart the moment either was filled in.
//!
//! # The keymap
//!
//! Snippets are the reason the keymap is rebuilt here too: a snippet may carry a
//! key equivalent, and GPUI's keymap can add bindings but not remove individual
//! ones. Upstream refuses to store a shortcut another menu command already has —
//! and refuses it in the settings pane, where the user can be told which command
//! it was — but a settings file edited by hand can still hold one, so [`install`]
//! drops it here as well: a snippet must never take a key that ⌘S or ⌘F already
//! answers to. A script's key, which comes out of its file name, is held to the
//! same rule for the same reason.
//!
//! # Where this differs from CotEditor
//!
//! *NEL, LS and PS are always listed.* Upstream shows only the three line
//! endings a document is normally written with and reveals the other three while
//! the Option key is held — its `OptionalMenu` watches the modifier flags for as
//! long as the menu is open. GPUI's menu API has no hidden items and no
//! modifier tracking, so the six are always listed. Every choice upstream offers
//! is offered here; three of them take fewer keystrokes.
//!
//! *A script runs on a plain click.* Upstream opens a script in an editor on
//! Option and shows it in the Finder on Option-Shift; a GPUI menu item carries
//! no modifier state, so there is no second and third behaviour to reach. See
//! [`crate::scripts`].

use std::path::{Path, PathBuf};

use gpui_kit::{Action, App, Menu, MenuItem};

use subtext_core::encoding::EncodingList;
use subtext_core::line_ending::ALL as LINE_ENDINGS;
use subtext_core::scripts::Item as ScriptItem;
use subtext_core::snippets::{Shortcut, Snippet};
use subtext_core::{FileEncoding, LineEnding, Normalization, Sections};

use crate::actions;
use crate::keymap;
use crate::menu;
use crate::scripts;
use crate::snippets;

/// How a command's path separates two menu levels.
pub(crate) const PATH_SEPARATOR: &str = " > ";

/// How the application menu is spelled in a command's path.
///
/// [`menu::COMMANDS`] and [`crate::keymap::SHORTCUTS`] both spell it with the
/// source application's name, because the key map is generated from the same
/// table; the Quick Actions bar substitutes the display name when it draws a
/// row. The live bar's own title is already the display name, so [`spelled`]
/// puts the source name back.
pub(crate) const UPSTREAM_MENU_TITLE: &str = "CotEditor";

/// The submenu names the runtime fills in, as `resources/menu.json` spells them.
///
/// The generator leaves each of these as an empty, disabled submenu, and the
/// names are unique across the whole bar — which is what lets [`replace`] find
/// one without being told the path it sits on.
const LINE_ENDINGS_MENU: &str = "Line Endings";
const TEXT_ENCODING_MENU: &str = "Text Encoding";
const SYNTAX_MENU: &str = "Syntax";
const THEME_MENU: &str = "Theme";
const UNICODE_NORMALIZATIONS_MENU: &str = "Unicode Normalizations";
const INSERT_SNIPPET_MENU: &str = "Insert Snippet";

/// The submenu AppKit would have filled in.
///
/// Upstream marks this one `systemMenu="recentDocuments"`, and `NSDocumentController`
/// inserts an item per recent document above the `Clear Menu` the storyboard
/// writes down. GPUI has no such hook, so the list is built here instead.
const OPEN_RECENT_MENU: &str = "Open Recent";

/// How many recent documents the menu lists.
///
/// `NSDocumentController.maximumRecentDocumentCount`, measured: AppKit's default
/// is 10, and CotEditor never overrides it. The settings file keeps more than
/// this — see [`crate::settings::Settings::note_recent`] — because the sidebar's
/// file browser is a list the user scrolls rather than a menu they read, but the
/// menu is the one upstream bounds and it is bounded to the same number here.
const RECENT_DOCUMENT_LIMIT: usize = 10;

/// The one runtime menu that is a menu of its own rather than a submenu.
const SCRIPT_MENU: &str = "Script";

/// The last item of File ▸ Open Recent, and the only one the storyboard writes
/// down. The documents above it are the user's history.
const CLEAR_RECENT: &str = "Clear Menu";

/// The last item of Format ▸ Text Encoding.
///
/// Upstream appends it after a separator, whatever the list holds, and it is
/// the one item in that submenu that does not name an encoding.
const CUSTOMIZE_ENCODINGS: &str = "Customize Encodings List…";

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
    /// The encoding the document will be written in, mark and all.
    pub encoding: FileEncoding,
}

/// Everything the menu bar is built from.
/// The editor conditions the generated menu table is built against.
///
/// CotEditor asks `validateUserInterfaceItem` each time a menu opens, so its
/// items can grey themselves out for the moment. A GPUI menu is a tree that is
/// set once, so the condition is *baked in* at build time and the bar is rebuilt
/// when it flips — see `Workspace::sync_menu_gates`.
///
/// Only conditions that are cheap to read belong here: the selection is one
/// `read` of the editor, while "does the outline have entries" needs a parse of
/// the buffer, so that one is still open (PLAN §13.5).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MenuGates {
    /// No selection: *Straighten Quotes* has nothing to work on.
    pub selection_empty: bool,
}

pub struct MenuState {
    /// The document in front, or `None` when there is none.
    pub document: Option<DocumentState>,
    /// The theme in force. It belongs to the application, not to a document.
    pub theme: String,
    /// Every syntax, in the order the Format ▸ Syntax submenu lists them.
    pub syntaxes: Vec<String>,
    /// Every theme, in the order the Format ▸ Theme submenu lists them.
    pub themes: Vec<String>,
    /// The user's encoding priority list, in the order the Format ▸ Text
    /// Encoding submenu lists it.
    ///
    /// The whole list, separators and all: they are what cuts the submenu into
    /// groups, and upstream's own menu builder maps them item by item.
    pub encoding_list: EncodingList,
    /// The user's snippets.
    pub snippets: Vec<Snippet>,
    /// The user's scripts, as the folder they live in holds them.
    pub scripts: Vec<ScriptItem>,
    /// The documents the user has opened, newest first.
    ///
    /// The whole list, not the [`RECENT_DOCUMENT_LIMIT`] the menu shows: the
    /// sidebar's file browser scrolls through all of it, and the two are built
    /// from the same state.
    pub recent_documents: Vec<PathBuf>,
    /// The editor conditions the generated table tests against.
    pub gates: MenuGates,
}

/// The part of a [`MenuState`] that changes without the settings changing.
///
/// Kept apart from the state itself so that the comparison which decides whether
/// a rebuild is needed does not have to clone the syntax list, the theme list
/// and every snippet on each tab switch.
///
/// The scripts are here because the folder they come from is one the user can
/// add to at any time, and there is no event to wait for: the list is read on
/// every refresh and compared, which is what notices a script that has appeared
/// or gone. The list is a few paths, so the comparison is cheaper than the
/// `read_dir` that produced it.
///
/// The recent documents are here for the same reason, and they are the one entry
/// the user changes from *outside* the menu: opening a document reorders them.
/// The list is bounded, so comparing it is a walk over at most
/// [`crate::settings::Settings::note_recent`]'s cap of paths.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MenuSignature {
    document: Option<DocumentState>,
    theme: String,
    scripts: Vec<ScriptItem>,
    recent_documents: Vec<PathBuf>,
    /// Whether the bar was built with an empty selection — the one gate that
    /// changes often enough to matter, and the reason a caret move can rebuild
    /// the bar.
    selection_empty: bool,
}

impl MenuSignature {
    /// The signature of a bar showing `document` under `theme`, with `scripts`
    /// in the Script menu and `recent_documents` under File ▸ Open Recent.
    pub fn of(
        document: Option<DocumentState>,
        theme: impl Into<String>,
        scripts: Vec<ScriptItem>,
        recent_documents: Vec<PathBuf>,
        selection_empty: bool,
    ) -> Self {
        Self {
            document,
            theme: theme.into(),
            scripts,
            recent_documents,
            selection_empty,
        }
    }
}

impl MenuState {
    /// The part of this state that [`MenuSignature`] covers.
    pub fn signature(&self) -> MenuSignature {
        MenuSignature::of(
            self.document.clone(),
            self.theme.clone(),
            self.scripts.clone(),
            self.recent_documents.clone(),
            self.gates.selection_empty,
        )
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
        let mut added = snippets::bindings(&keymap, &Sections::of(&state.snippets, scope(state)));
        // After the snippets, so that of the two a snippet keeps a key they
        // both want: it is the older of the two features, and its list is the
        // one the user can see and edit.
        added.extend(scripts::bindings(&keymap, &state.scripts, scope(state)));
        added
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
    let mut menus = menu::app_menus(&state.gates);

    replace(
        &mut menus,
        INSERT_SNIPPET_MENU,
        snippets::items(&Sections::of(&state.snippets, scope(state))),
    );
    replace(&mut menus, LINE_ENDINGS_MENU, line_endings(state));
    replace(&mut menus, TEXT_ENCODING_MENU, encodings(state));
    replace(&mut menus, SYNTAX_MENU, syntaxes(state));
    replace(&mut menus, THEME_MENU, themes(state));
    replace(
        &mut menus,
        UNICODE_NORMALIZATIONS_MENU,
        normalizations(),
    );
    replace(&mut menus, SCRIPT_MENU, scripts::items(&state.scripts));
    replace(&mut menus, OPEN_RECENT_MENU, recent_documents(state));

    menus
}

/// Replace the items of the submenu called `name`, wherever it sits.
///
/// The names are unique across the bar, which is what lets this find one without
/// being handed the path it is on: the table is generated, so a path written
/// here would be a second copy of the storyboard's structure to keep in step.
///
/// A top-level menu counts as well as a submenu, because one of the runtime
/// menus — `Script` — is a menu of its own rather than a submenu of another.
///
/// Returns whether it found one. A miss would leave the submenu empty and
/// disabled, so the tests check every name against the generated table.
fn replace(menus: &mut [Menu], name: &str, items: Vec<MenuItem>) -> bool {
    let mut items = items;
    menus.iter_mut().any(|menu| {
        if &*menu.name == name {
            menu.items = std::mem::take(&mut items);
            // The generator leaves the placeholder disabled, which is what keeps
            // an empty menu from reading as a broken one before the first
            // document is open. It is no longer empty, so it is no longer
            // disabled.
            menu.disabled = false;
            return true;
        }
        splice(menu, name, &mut items)
    })
}

/// The recursive half of [`replace`], which walks into submenus.
fn splice(menu: &mut Menu, name: &str, items: &mut Vec<MenuItem>) -> bool {
    for item in &mut menu.items {
        let MenuItem::Submenu(submenu) = item else {
            continue;
        };
        if &*submenu.name == name {
            submenu.items = std::mem::take(items);
            submenu.disabled = false;
            return true;
        }
        if splice(submenu, name, items) {
            return true;
        }
    }
    false
}

// ---- the commands --------------------------------------------------------

/// What a command is, which is what the glyph beside its row says.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandKind {
    /// A menu command.
    Command,
    /// A script: a file in the user's Script folder.
    Script,
}

/// One command the menu bar offers, as the Quick Actions bar sees it.
pub struct Command {
    /// The path, spelled the way the bar spells it: `Format > Syntax > Swift`.
    pub path: String,
    /// The action the menu item carries, kept as the item carries it.
    ///
    /// An item the bar builds at runtime has no name to be run by: its action
    /// carries the value it stands for, and running it means running *that*
    /// item's command rather than one that happens to share its title.
    pub action: Box<dyn Action>,
    /// Which of the two glyphs the row draws.
    pub kind: CommandKind,
    /// The key to draw beside the row, if the command has one.
    pub shortcut: Option<String>,
}

impl Clone for Command {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            action: self.action.boxed_clone(),
            kind: self.kind,
            shortcut: self.shortcut.clone(),
        }
    }
}

/// Every command the menu bar offers, in menu order.
///
/// Read off the bar rather than out of a table, which is what upstream's
/// `NSApp.actionCommands` does and the only way the runtime items can appear at
/// all: [`menu::COMMANDS`] is a compile-time dump of the storyboard and cannot
/// name a syntax, a theme or a script the user has not installed yet.
///
/// Upstream's walk is copied item for item:
///
/// * a submenu is walked into, and its title prefixed to every path below it;
/// * a leaf counts only while it is enabled — so the items of a submenu that
///   needs a document (Syntax, Line Endings, Text Encoding) are not offered
///   with nothing open, and a snippet group's heading, which exists to be a
///   label, is not offered at all;
/// * the command that opens this bar is left out, because searching for it from
///   inside it is a loop. Upstream lists it in `unsupportedActions` for the
///   same reason.
pub fn commands(state: &MenuState) -> Vec<Command> {
    let mut commands = Vec::new();

    for menu in build(state) {
        // Upstream skips a menu with no title, which is how it leaves the Debug
        // menu out. Nothing here has an empty title; the walk has the same
        // shape so that one day something might.
        if !menu.name.is_empty() {
            walk(&menu, &mut Vec::new(), state, &mut commands);
        }
    }

    commands
}

/// The recursive half of [`commands`]: one menu, with the titles above it.
fn walk(menu: &Menu, ancestors: &mut Vec<String>, state: &MenuState, commands: &mut Vec<Command>) {
    ancestors.push(spelled(&menu.name).to_string());

    for item in &menu.items {
        match item {
            MenuItem::Action {
                name,
                action,
                disabled,
                ..
            } => {
                if *disabled || action.as_any().is::<actions::view::ShowQuickActions>() {
                    continue;
                }

                ancestors.push(name.to_string());
                let path = ancestors.join(PATH_SEPARATOR);
                let shortcut = shortcut(state, &path, action.as_ref());
                commands.push(Command {
                    path,
                    action: action.boxed_clone(),
                    kind: if action.as_any().is::<actions::script::RunScript>() {
                        CommandKind::Script
                    } else {
                        CommandKind::Command
                    },
                    shortcut,
                });
                ancestors.pop();
            }
            MenuItem::Submenu(submenu) => walk(submenu, ancestors, state, commands),
            // A separator is not a command, and a system menu's items are the
            // system's to name — Services is the only one, and it holds nothing
            // until the platform fills it.
            MenuItem::Separator | MenuItem::SystemMenu(_) => {}
        }
    }

    ancestors.pop();
}

/// A menu's title, as a command's path spells it.
fn spelled(title: &str) -> &str {
    if title == menu::APP_MENU_TITLE {
        UPSTREAM_MENU_TITLE
    } else {
        title
    }
}

/// The key to draw beside a command's row, if it has one.
///
/// Three sources, and the command's own action says which applies: a script's
/// key is written into its file name, a snippet's is its own record, and
/// everything else is a menu command whose key the generated key map holds
/// against the same path.
///
/// Both of the first two are looked up rather than carried because a menu item
/// has nowhere to put a key: GPUI reads an item's shortcut out of the keymap,
/// keyed by action, while this list is keyed by path.
fn shortcut(state: &MenuState, path: &str, action: &dyn Action) -> Option<String> {
    let any = action.as_any();

    if let Some(action) = any.downcast_ref::<actions::script::RunScript>() {
        return state
            .scripts
            .iter()
            .find_map(|item| item.find(Path::new(&action.path)))
            .and_then(|script| script.shortcut)
            .map(Shortcut::symbol);
    }

    if let Some(action) = any.downcast_ref::<actions::snippet::InsertSnippet>() {
        return state
            .snippets
            .iter()
            .find(|snippet| snippet.id == action.id)
            .and_then(|snippet| snippet.shortcut)
            .map(Shortcut::symbol);
    }

    let row = keymap::SHORTCUTS.iter().find(|row| row.0 == path)?;
    row_shortcut(row.2, row.3).map(Shortcut::symbol)
}

/// The [`Shortcut`] a generated key-map row stands for, read from its modifier
/// and key columns.
///
/// The row's own displayed form is not read back, and the reason is upstream's:
/// it spells the key in capitals whatever the modifiers — which is how a menu
/// shows it — so an uppercase letter there says nothing about Shift, and it
/// writes the chord without the thin space [`Shortcut::symbol`] puts between the
/// symbols. One list cannot show two spellings of the same thing, so every row
/// is rendered through the one formatter upstream's `Shortcut.symbol` uses.
///
/// The two columns this does read are unambiguous: the modifiers are words, and
/// the key is one character, spelled in capitals only when Shift is among them.
fn row_shortcut(modifiers: &str, key: &str) -> Option<Shortcut> {
    let mut chars = key.chars();
    let key = chars.next()?;
    if chars.next().is_some() {
        return None;
    }

    let has = |word: &str| modifiers.split_whitespace().any(|modifier| modifier == word);
    Some(Shortcut {
        key: key.to_ascii_lowercase(),
        control: has("control"),
        option: has("option"),
        shift: has("shift"),
        command: has("command"),
    })
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

/// File ▸ Open Recent.
///
/// The documents the user has opened, newest first, then a separator, then the
/// `Clear Menu` item the storyboard writes down. Upstream's version of this list
/// is AppKit's: the submenu is marked `systemMenu="recentDocuments"` and
/// `NSDocumentController` inserts one item per document above the storyboard's
/// own. Nothing here can call that hook, so the list is built from
/// [`crate::settings::Settings::recent_documents`], which is the same list the
/// sidebar's file browser shows.
///
/// Two details are upstream's rather than this build's choice, and both were
/// measured against AppKit:
///
/// * *At most [`RECENT_DOCUMENT_LIMIT`].* `NSDocumentController` caps the menu at
///   `maximumRecentDocumentCount`, which is 10 and which CotEditor never raises.
/// * *`Clear Menu` is disabled with nothing to clear.*
///   `-[NSDocumentController validateUserInterfaceItem:]` answers `NO` for
///   `clearRecentDocuments:` while `recentDocumentURLs` is empty. A disabled item
///   is also what keeps the entry out of the Quick Actions bar, which upstream
///   fills by walking the live menu and skipping disabled leaves.
///
/// The titles are the files' names, which is what the sidebar already puts on a
/// recent document's row. What AppKit writes there instead is its own rendering
/// and this build cannot read it; the file's name is the honest reading of
/// "the recent document", and a path would be a second line the menu cannot show.
fn recent_documents(state: &MenuState) -> Vec<MenuItem> {
    let shown = state.recent_documents.len().min(RECENT_DOCUMENT_LIMIT);
    let mut items = Vec::with_capacity(shown + 2);

    items.extend(state.recent_documents.iter().take(shown).map(|path| {
        MenuItem::action(
            recent_document_name(path),
            actions::file::OpenRecentDocument {
                path: path.display().to_string(),
            },
        )
    }));

    // The separator only separates when there is something above it. Upstream
    // has the same shape: with an empty history the submenu is `Clear Menu`
    // alone, with no rule over it.
    if shown > 0 {
        items.push(MenuItem::separator());
    }
    items.push(
        MenuItem::action(CLEAR_RECENT, actions::file::ClearRecentDocuments)
            .disabled(state.recent_documents.is_empty()),
    );
    items
}

/// The name a recent document is listed under.
///
/// A path with no file name — `/` — is listed as itself rather than dropped:
/// `file_name` answers `None` for it, and the row it belongs to is still a
/// document the user opened.
fn recent_document_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
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

/// Format ▸ Text Encoding.
///
/// Upstream's `AppDelegate.updateEncodingMenu`, which builds the items out of
/// `EncodingManager.fileEncodings` each time the menu opens:
///
/// * the document's own encoding goes to the *top*, followed by a separator,
///   when the list below does not already hold it — the case a file was opened
///   in an encoding the user had taken out of their list;
/// * the list follows, groups separated by separators, with "Unicode (UTF-8)
///   with BOM" sitting right after "Unicode (UTF-8)" because that is where
///   `fileEncodings(from:)` splices it;
/// * and the submenu ends with a separator and *Customize Encodings List…*,
///   whatever the list holds.
///
/// The list is the user's, edited in the Format settings pane — see
/// [`EncodingList`]. Upstream reads it from `UserDefaults` on every open; here
/// it arrives in the [`MenuState`], and the settings pane rebuilds the bar when
/// it changes.
///
/// The encoding in force is checked. The pair matters there: a UTF-8 document
/// with a byte-order mark is checked on the *with BOM* item, not on the plain
/// one, which is what upstream's `validateUserInterfaceItem` compares.
///
/// Every encoding item is disabled with no document open. Upstream leaves them
/// enabled and lets the responder chain fail to find a target, which greys them
/// the same way.
fn encodings(state: &MenuState) -> Vec<MenuItem> {
    encoding_items(
        &FileEncoding::menu_groups(&state.encoding_list),
        state.document.as_ref().map(|document| document.encoding),
    )
}

/// The items of Format ▸ Text Encoding: `groups` with `current` checked.
///
/// The list is a parameter rather than read from the registry in here because
/// the branch below fires only when the list *lacks* the encoding in force —
/// which is the ordinary case now that the list is the user's. The document's
/// encoding goes to the top, behind a separator, so the menu can always show
/// what the document is in even when the user has taken that encoding out.
fn encoding_items(groups: &[Vec<FileEncoding>], current: Option<FileEncoding>) -> Vec<MenuItem> {
    let mut items = Vec::with_capacity(groups.iter().map(Vec::len).sum::<usize>() + groups.len() + 2);

    if let Some(current) = current.filter(|encoding| !groups.iter().flatten().any(|e| e == encoding))
    {
        items.push(encoding_item(current, true, false));
        items.push(MenuItem::separator());
    }

    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            items.push(MenuItem::separator());
        }
        items.extend(group.iter().map(|encoding| {
            encoding_item(*encoding, current == Some(*encoding), current.is_none())
        }));
    }

    // Upstream appends this whatever the list holds, and it is never disabled:
    // it is the way to the list itself, not a way to change an encoding.
    items.push(MenuItem::separator());
    items.push(MenuItem::action(
        CUSTOMIZE_ENCODINGS,
        actions::format::CustomizeEncodings,
    ));
    items
}

/// One encoding item, titled as CotEditor titles it and carrying the file
/// encoding it stands for.
fn encoding_item(file_encoding: FileEncoding, checked: bool, disabled: bool) -> MenuItem {
    MenuItem::action(
        file_encoding.name(),
        actions::format::ChangeEncoding {
            identifier: file_encoding.identifier(),
        },
    )
    .checked(checked)
    .disabled(disabled)
}

/// Text ▸ Unicode Normalizations.
///
/// Upstream's `standardForms + [nil] + modifiedForms`: five labels, a separator,
/// two more. The separator is what keeps the two HFS+ forms from reading as two
/// more Unicode ones, which is exactly what they are not.
///
/// Nothing is checked and nothing is disabled. A normalization is an edit, not a
/// setting the document is in, and upstream lets it run on an empty selection —
/// where it does nothing, because there is nothing selected to normalize.
fn normalizations() -> Vec<MenuItem> {
    let mut items = Vec::with_capacity(Normalization::ALL.len() + 1);
    items.extend(Normalization::STANDARD.into_iter().map(normalization));
    items.push(MenuItem::separator());
    items.extend(Normalization::MODIFIED.into_iter().map(normalization));
    items
}

fn normalization(form: Normalization) -> MenuItem {
    MenuItem::action(
        form.name(),
        actions::text::NormalizeUnicode {
            form: form.identifier().to_string(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use subtext_core::snippets::Shortcut;
    use subtext_core::{Script, ScriptKind, TextEncoding};

    fn document() -> DocumentState {
        DocumentState {
            syntax: "Markdown".into(),
            line_ending: LineEnding::CRLF,
            encoding: FileEncoding::of(TextEncoding::UTF8),
        }
    }

    fn state() -> MenuState {
        MenuState {
            gates: MenuGates::default(),
            document: Some(document()),
            theme: "Solarized Dark".into(),
            syntaxes: vec!["HTML".into(), "Markdown".into(), "Ruby".into()],
            themes: vec!["Solarized Dark".into(), "Solarized Light".into()],
            encoding_list: EncodingList::defaults(),
            snippets: Vec::new(),
            scripts: Vec::new(),
            recent_documents: Vec::new(),
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

    /// The menu or submenu called `name`, wherever it is.
    ///
    /// A top-level menu counts, because `Script` is one: it is the only runtime
    /// menu that is not a submenu of another.
    fn named<'a>(menus: &'a [Menu], name: &str) -> &'a Menu {
        fn find<'a>(menu: &'a Menu, name: &str) -> Option<&'a Menu> {
            menu.items.iter().find_map(|item| match item {
                MenuItem::Submenu(submenu) if &*submenu.name == name => Some(submenu),
                MenuItem::Submenu(submenu) => find(submenu, name),
                _ => None,
            })
        }
        menus
            .iter()
            .find(|menu| &*menu.name == name)
            .or_else(|| menus.iter().find_map(|menu| find(menu, name)))
            .unwrap_or_else(|| panic!("no {name} menu"))
    }

    /// The labels of some items, with separators marked and disabled items in
    /// brackets so the shape of the list is visible.
    fn item_labels(items: &[MenuItem]) -> Vec<String> {
        items
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

    /// The labels of a submenu.
    fn labels(menus: &[Menu], name: &str) -> Vec<String> {
        item_labels(&named(menus, name).items)
    }

    /// Labels cut back into groups at each separator, the trailing one dropped.
    fn grouped(labels: &[String]) -> Vec<Vec<String>> {
        labels
            .split(|label| label == "―")
            .map(<[String]>::to_vec)
            .filter(|group| !group.is_empty())
            .collect()
    }

    /// The checked label of some items, if any is checked.
    fn checked_label(items: &[MenuItem]) -> Option<String> {
        items
            .iter()
            .find(|item| item.is_checked())
            .map(|item| match item {
                MenuItem::Action { name, .. } => name.to_string(),
                _ => unreachable!("only actions can be checked"),
            })
    }

    /// The checked label of a submenu, if it has one.
    fn checked(menus: &[Menu], name: &str) -> Option<String> {
        checked_label(&named(menus, name).items)
    }

    /// Every placeholder the generator leaves behind is one this module fills
    /// in. A miss would leave a submenu permanently empty and disabled, which is
    /// exactly the state these exist to leave behind.
    #[test]
    fn every_runtime_submenu_is_replaced() {
        for name in [
            LINE_ENDINGS_MENU,
            TEXT_ENCODING_MENU,
            SYNTAX_MENU,
            THEME_MENU,
            UNICODE_NORMALIZATIONS_MENU,
            INSERT_SNIPPET_MENU,
            SCRIPT_MENU,
        ] {
            let menus = build(&state());
            assert!(!named(&menus, name).items.is_empty(), "{name} is empty");
            assert!(!named(&menus, name).disabled, "{name} is still disabled");
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

        assert!(named(&menus, LINE_ENDINGS_MENU).items.iter().all(MenuItem::is_disabled));
        assert!(named(&menus, SYNTAX_MENU).items.iter().all(|item| {
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
        assert!(named(&menus, THEME_MENU).items.iter().all(|item| !item.is_disabled()));
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
        let carried: Vec<String> = named(&menus, LINE_ENDINGS_MENU)
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
        assert_eq!(carried, ["LF", "CR", "CRLF", "NEL", "LS", "PS"]);
    }

    #[test]
    fn every_syntax_item_names_the_syntax_it_stands_for() {
        let menus = build(&state());
        let carried: Vec<String> = named(&menus, SYNTAX_MENU)
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
        assert_eq!(carried, ["None", "HTML", "Markdown", "Ruby"]);
    }

    // ---- the encoding submenu --------------------------------------------

    /// The list is the registry, cut where upstream cuts it, with the way to the
    /// list editor behind a final separator.
    ///
    /// The document is UTF-8 without a mark, which the list already holds, so
    /// nothing is lifted to the top and the groups come out one for one.
    #[test]
    fn the_encoding_menu_is_the_registry_in_upstream_s_groups() {
        let menus = build(&state());
        let groups = grouped(&labels(&menus, TEXT_ENCODING_MENU));
        let registry = FileEncoding::menu_groups(&EncodingList::defaults());

        assert_eq!(groups.len(), registry.len() + 1, "one group per registry group, plus the tail");
        for (group, registry) in groups.iter().zip(registry.iter()) {
            assert_eq!(group, &registry.iter().map(|e| e.name()).collect::<Vec<_>>());
        }
        assert_eq!(groups.last().unwrap(), &[CUSTOMIZE_ENCODINGS]);
    }

    /// Upstream splices the mark item in right after UTF-8, and it is the first
    /// thing in the submenu.
    #[test]
    fn the_utf8_pair_opens_the_list() {
        let menus = build(&state());
        let identifiers = encoding_identifiers(&menus);
        assert_eq!(identifiers[..2], ["utf-8", "utf-8-bom"]);
        assert_eq!(
            labels(&menus, TEXT_ENCODING_MENU)[..3],
            ["Unicode (UTF-8)", "Unicode (UTF-8) with BOM", "―"]
        );
    }

    /// A UTF-8 document with a mark is in a different encoding from one without,
    /// and upstream checks one item for each. Two items sharing an identifier
    /// would both be checked — which is what the registry's unique identifiers
    /// exist to prevent, and why the payload is the pair rather than the codec.
    #[test]
    fn the_encoding_the_document_is_in_is_the_checked_one() {
        assert_eq!(
            checked(&build(&state()), TEXT_ENCODING_MENU).as_deref(),
            Some("Unicode (UTF-8)")
        );

        let mut with_bom = state();
        with_bom.document.as_mut().unwrap().encoding = FileEncoding::utf8_with_bom();
        assert_eq!(
            checked(&build(&with_bom), TEXT_ENCODING_MENU).as_deref(),
            Some("Unicode (UTF-8) with BOM")
        );
    }

    /// Upstream lifts the document's encoding to the top, behind a separator,
    /// when the list does not hold it. SubText's list is the whole registry, so
    /// nothing the application does reaches this branch — the tests do.
    #[test]
    fn an_encoding_the_list_lacks_goes_to_the_top_behind_a_separator() {
        let groups = vec![
            vec![FileEncoding::of(TextEncoding::UTF8)],
            vec![FileEncoding::of(TextEncoding::from_identifier("euc-jp").unwrap())],
        ];

        let missing = encoding_items(&groups, Some(FileEncoding::of(TextEncoding::UTF16_LE)));
        assert_eq!(
            item_labels(&missing),
            [
                "Unicode (UTF-16LE)",
                "―",
                "Unicode (UTF-8)",
                "―",
                "Japanese (EUC)",
                "―",
                CUSTOMIZE_ENCODINGS,
            ]
        );
        assert_eq!(
            checked_label(&missing).as_deref(),
            Some("Unicode (UTF-16LE)")
        );

        // The other half: when the list already holds it, it stays where it is.
        let listed = encoding_items(&groups, Some(FileEncoding::of(TextEncoding::UTF8)));
        assert_eq!(
            item_labels(&listed),
            ["Unicode (UTF-8)", "―", "Japanese (EUC)", "―", CUSTOMIZE_ENCODINGS]
        );
    }

    /// With nothing open there is nothing to convert, but the way to the list
    /// editor is still there — it is not a way to change an encoding.
    #[test]
    fn a_document_less_bar_disables_the_encodings_and_leaves_the_list_editor() {
        let mut state = state();
        state.document = None;
        let menus = build(&state);
        let labels = labels(&menus, TEXT_ENCODING_MENU);

        assert_eq!(labels.last().unwrap(), CUSTOMIZE_ENCODINGS);
        for label in &labels[..labels.len() - 1] {
            assert!(
                label == "―" || label.starts_with('['),
                "{label} is selectable with no document open"
            );
        }
        assert_eq!(checked(&menus, TEXT_ENCODING_MENU), None);
    }

    /// Every item carries its own encoding, and an encoding the runtime cannot
    /// resolve would leave the item greyed — so each one has to resolve.
    #[test]
    fn every_encoding_item_names_the_file_encoding_it_stands_for() {
        let menus = build(&state());
        let identifiers = encoding_identifiers(&menus);

        let mut unique = identifiers.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), identifiers.len(), "two items name one encoding");

        for identifier in &identifiers {
            assert!(
                FileEncoding::from_identifier(identifier).is_some(),
                "{identifier} names nothing"
            );
        }
    }

    /// The identifiers the encoding submenu's items carry, in order.
    fn encoding_identifiers(menus: &[Menu]) -> Vec<String> {
        named(menus, TEXT_ENCODING_MENU)
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { action, .. } => action
                    .as_any()
                    .downcast_ref::<actions::format::ChangeEncoding>()
                    .map(|action| action.identifier.clone()),
                _ => None,
            })
            .collect()
    }

    // ---- the normalization submenu ----------------------------------------

    /// Upstream's `standardForms + [nil] + modifiedForms`, which its menu
    /// builder splices together.
    #[test]
    fn the_normalization_menu_is_five_then_a_separator_then_two() {
        let menus = build(&state());
        assert_eq!(
            labels(&menus, UNICODE_NORMALIZATIONS_MENU),
            [
                "NFD",
                "NFC",
                "NFKD",
                "NFKC",
                "NFKC Case-Fold",
                "―",
                "Modified NFD",
                "Modified NFC",
            ]
        );
    }

    /// Nothing is checked: a normalization is an edit, not a setting. Nothing is
    /// greyed either — the three forms this build cannot apply say so when they
    /// run, rather than looking unavailable.
    #[test]
    fn the_normalization_menu_has_no_checks_and_no_dead_items() {
        let menus = build(&state());
        let items = &named(&menus, UNICODE_NORMALIZATIONS_MENU).items;
        assert!(!items.iter().any(MenuItem::is_checked));
        assert!(!items.iter().any(MenuItem::is_disabled));
    }

    /// Each item carries its own form, and a form the runtime does not know is
    /// dropped rather than run — which is what makes the identifiers matter.
    #[test]
    fn every_normalization_item_names_the_form_it_stands_for() {
        let menus = build(&state());
        let carried: Vec<&str> = named(&menus, UNICODE_NORMALIZATIONS_MENU)
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { action, .. } => {
                    let form = &action
                        .as_any()
                        .downcast_ref::<actions::text::NormalizeUnicode>()
                        .expect("a normalization item runs NormalizeUnicode")
                        .form;
                    Some(
                        Normalization::from_identifier(form)
                            .expect("an item names a form the runtime knows")
                            .name(),
                    )
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            carried,
            ["NFD", "NFC", "NFKD", "NFKC", "NFKC Case-Fold", "Modified NFD", "Modified NFC"]
        );
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

        let found = named(&menus, INSERT_SNIPPET_MENU)
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

    // ---- the script menu -------------------------------------------------

    /// One script, as the folder scan would have found it.
    fn script(name: &str, path: &str, spec: Option<&str>) -> Script {
        Script {
            path: PathBuf::from(path),
            name: name.to_string(),
            kind: ScriptKind::Unix,
            shortcut: spec.and_then(Shortcut::parse),
        }
    }

    /// A state whose Script menu holds two scripts, one behind a folder.
    fn scripted() -> MenuState {
        let mut state = state();
        state.scripts = vec![
            ScriptItem::Script(script("Save", "/scripts/Save.@s.sh", Some("@s"))),
            ScriptItem::Separator,
            ScriptItem::Folder {
                name: "Python".into(),
                items: vec![ScriptItem::Script(script(
                    "Clean",
                    "/scripts/Python/Clean.py",
                    None,
                ))],
            },
        ];
        state
    }

    /// The folder's own entries first, then the way to the folder — upstream's
    /// shape, whatever the folder holds.
    #[test]
    fn the_script_menu_is_the_folder_then_the_way_to_it() {
        assert_eq!(
            labels(&build(&scripted()), SCRIPT_MENU),
            ["Save", "―", "(Python)", "―", "Open Scripts Folder"]
        );
    }

    /// The Script menu is a menu of its own, not a submenu of another, so it is
    /// filled in where it sits at the top level.
    #[test]
    fn the_script_menu_is_a_top_level_menu() {
        let menus = build(&state());
        let script = menus
            .iter()
            .find(|menu| &*menu.name == SCRIPT_MENU)
            .expect("the bar has a Script menu");
        assert!(!script.disabled);
        assert_eq!(labels(&menus, SCRIPT_MENU), ["Open Scripts Folder"]);
    }

    /// A script item carries the script's path, not its position: the folder can
    /// be rearranged between the item being built and being chosen.
    #[test]
    fn every_script_item_names_the_script_it_stands_for() {
        let menus = build(&scripted());
        let carried: Vec<String> = named(&menus, SCRIPT_MENU)
            .items
            .iter()
            .filter_map(|item| match item {
                // The way to the folder is the one item here that does not name
                // a script.
                MenuItem::Action { action, .. } => action
                    .as_any()
                    .downcast_ref::<actions::script::RunScript>()
                    .map(|action| action.path.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(carried, ["/scripts/Save.@s.sh"]);
    }

    /// Scripts belong to the application rather than to a document, so the menu
    /// is the same with nothing open.
    #[test]
    fn the_script_menu_does_not_follow_the_document() {
        let mut state = scripted();
        state.document = None;
        assert_eq!(
            labels(&build(&state), SCRIPT_MENU),
            labels(&build(&scripted()), SCRIPT_MENU)
        );
    }

    // ---- the recent documents --------------------------------------------

    /// The history, newest first, then the rule, then the item the storyboard
    /// writes down. Upstream's `Clear Menu` sits under the same separator.
    #[test]
    fn the_recent_menu_is_the_history_then_the_way_to_clear_it() {
        let mut state = state();
        state.recent_documents = vec![
            PathBuf::from("/notes/todo.txt"),
            PathBuf::from("/notes/design.md"),
        ];

        assert_eq!(
            labels(&build(&state), OPEN_RECENT_MENU),
            ["todo.txt", "design.md", "―", "Clear Menu"]
        );
    }

    /// The titles are the files' names, which is what the sidebar puts on a
    /// recent document's row. A path with no file name is listed as itself
    /// rather than dropped: the row is still a document the user opened.
    #[test]
    fn a_recent_document_is_listed_under_its_file_name() {
        let mut state = state();
        state.recent_documents = vec![PathBuf::from("/")];

        assert_eq!(
            labels(&build(&state), OPEN_RECENT_MENU),
            ["/", "―", "Clear Menu"]
        );
    }

    /// With nothing to clear the item is greyed, which is what
    /// `-[NSDocumentController validateUserInterfaceItem:]` answers for
    /// `clearRecentDocuments:` while `recentDocumentURLs` is empty — and what
    /// keeps it out of the Quick Actions bar, whose walk skips disabled leaves.
    #[test]
    fn an_empty_history_leaves_just_the_clear_item_and_greys_it_out() {
        assert_eq!(labels(&build(&state()), OPEN_RECENT_MENU), ["[Clear Menu]"]);
        assert!(!paths(&state()).contains(&"File > Open Recent > Clear Menu".to_string()));

        let mut state = state();
        state.recent_documents = vec![PathBuf::from("/notes/todo.txt")];
        assert!(paths(&state).contains(&"File > Open Recent > Clear Menu".to_string()));
    }

    /// `NSDocumentController.maximumRecentDocumentCount` is 10 and CotEditor
    /// never raises it, so the menu stops there however long the history is —
    /// while the settings keep more, because the sidebar scrolls.
    #[test]
    fn the_recent_menu_stops_at_upstream_s_ten() {
        let mut state = state();
        state.recent_documents = (0..14)
            .map(|index| PathBuf::from(format!("/notes/file-{index}.txt")))
            .collect();

        let listed = labels(&build(&state), OPEN_RECENT_MENU);
        assert_eq!(listed.len(), RECENT_DOCUMENT_LIMIT + 2);
        assert_eq!(listed[0], "file-0.txt");
        assert_eq!(listed[RECENT_DOCUMENT_LIMIT - 1], "file-9.txt");
        assert_eq!(listed[RECENT_DOCUMENT_LIMIT], "―");
        assert_eq!(listed[RECENT_DOCUMENT_LIMIT + 1], "Clear Menu");
    }

    /// Each item carries its own path, not its position: opening a document
    /// reorders the list, and an item run by position would open the wrong file.
    #[test]
    fn every_recent_item_names_the_path_it_stands_for() {
        let mut state = state();
        state.recent_documents = vec![
            PathBuf::from("/notes/todo.txt"),
            PathBuf::from("/notes/design.md"),
        ];

        let carried: Vec<String> = named(&build(&state), OPEN_RECENT_MENU)
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { action, .. } => action
                    .as_any()
                    .downcast_ref::<actions::file::OpenRecentDocument>()
                    .map(|action| action.path.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(carried, ["/notes/todo.txt", "/notes/design.md"]);
    }

    // ---- the signature ---------------------------------------------------

    /// The recent documents are part of it, because they are the one entry the
    /// user changes from outside the menu: opening a document reorders them, and
    /// clearing them is a command of its own.
    #[test]
    fn the_signature_follows_the_recent_documents() {
        let mut state = state();
        state.recent_documents = vec![PathBuf::from("/notes/todo.txt")];
        let before = state.signature();

        state.recent_documents.insert(0, PathBuf::from("/notes/design.md"));
        assert_ne!(before, state.signature());

        state.recent_documents.clear();
        assert_ne!(before, state.signature());
    }

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
        reencoded.document.as_mut().unwrap().encoding = FileEncoding::of(TextEncoding::UTF16_LE);
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

    /// The scripts are part of it, unlike the syntax and theme tables: the
    /// folder they come from is one the user can add to at any time, so reading
    /// it again and comparing is what notices a script that appeared or went.
    #[test]
    fn the_signature_follows_the_scripts() {
        let before = state().signature();

        let mut appeared = state();
        appeared.scripts = vec![ScriptItem::Script(script("Save", "/scripts/Save.@s.sh", None))];
        assert_ne!(
            appeared.signature(),
            before,
            "a script that appeared was missed"
        );

        let mut shortcut_added = state();
        shortcut_added.scripts = vec![ScriptItem::Script(script(
            "Save",
            "/scripts/Save.@s.sh",
            Some("@s"),
        ))];
        assert_ne!(
            shortcut_added.signature(),
            appeared.signature(),
            "a script that gained a shortcut was missed"
        );
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

    // ---- the commands ----------------------------------------------------

    /// The path of every command the bar offers.
    fn paths(state: &MenuState) -> Vec<String> {
        commands(state).into_iter().map(|command| command.path).collect()
    }

    /// The list is the bar flattened: a submenu's title goes in front of every
    /// path below it, and nothing that is not a command is in it.
    #[test]
    fn the_commands_are_the_bar_flattened() {
        let paths = paths(&state());

        assert!(paths.contains(&"File > Save".to_string()));
        assert!(paths.contains(&"Format > Tab Width > Custom…".to_string()));
        assert!(paths.contains(&"Edit > Select > Select All".to_string()));
        // The three the storyboard leaves empty, and the one that is a menu of
        // its own. A table written at compile time could name none of them.
        assert!(paths.contains(&"Format > Syntax > Ruby".to_string()));
        assert!(paths.contains(&"Format > Line Endings > Windows (CRLF)".to_string()));
        assert!(paths.contains(&"Format > Theme > Solarized Dark".to_string()));
        assert!(paths.contains(&"Format > Text Encoding > Unicode (UTF-8)".to_string()));
        assert!(paths.contains(&"Text > Unicode Normalizations > NFC".to_string()));
        assert!(paths.contains(&"Script > Open Scripts Folder".to_string()));
        assert!(!paths.iter().any(|path| path.contains('―')));
    }

    /// The path is spelled the way [`crate::keymap::SHORTCUTS`] spells it, so
    /// the two can be joined — and the Quick Actions bar substitutes the display
    /// name when it draws a row.
    #[test]
    fn the_application_menu_is_spelled_the_way_the_key_map_spells_it() {
        let paths = paths(&state());
        assert!(paths.contains(&"CotEditor > Settings…".to_string()));
        assert!(!paths.iter().any(|path| path.starts_with("SubText > ")));
    }

    /// A greyed item is not a command: upstream's walk asks `isEnabled` before
    /// it takes a leaf, and so does this one.
    #[test]
    fn a_disabled_item_is_not_a_command() {
        let mut state = state();
        state.snippets = vec![
            snippet(1, "Header", None, None),
            snippet(2, "Emphasis", Some("Markdown"), None),
        ];

        // The group's heading is in the menu with a document open, and it is
        // disabled: it exists to be a label, and a label is not a command.
        let found = paths(&state);
        assert!(found.contains(&"Text > Insert Snippet > Emphasis".to_string()));
        assert!(!found.contains(&"Text > Insert Snippet > Markdown".to_string()));

        state.document = None;
        let found = paths(&state);

        assert!(!found.iter().any(|path| path.starts_with("Format > Syntax > ")));
        assert!(!found.iter().any(|path| path.starts_with("Format > Line Endings > ")));
        assert!(!found.contains(&"Format > Text Encoding > Unicode (UTF-8)".to_string()));
        assert!(!found.contains(&"Text > Insert Snippet > Emphasis".to_string()));
        // The way to the list editor is not a way to change an encoding, so it
        // stays — upstream leaves it live with nothing open.
        assert!(found.contains(&"Format > Text Encoding > Customize Encodings List…".to_string()));
        // And the theme belongs to the application rather than to a document.
        assert!(found.contains(&"Format > Theme > Solarized Dark".to_string()));
    }

    /// Searching from inside the bar cannot find the command that opens it.
    #[test]
    fn the_command_that_opens_the_bar_is_left_out() {
        assert!(
            !paths(&state()).contains(&"View > Show Quick Actions…".to_string()),
            "the bar offers the command that opens it"
        );
    }

    /// A script is marked as one, which is what the row's glyph reads off. The
    /// way to the folder is a command like any other.
    #[test]
    fn a_script_is_marked_as_one() {
        let found: Vec<(String, CommandKind)> = commands(&scripted())
            .into_iter()
            .filter(|command| command.path.starts_with("Script > "))
            .map(|command| (command.path, command.kind))
            .collect();

        assert_eq!(
            found,
            [
                ("Script > Save".to_string(), CommandKind::Script),
                ("Script > Python > Clean".to_string(), CommandKind::Script),
                (
                    "Script > Open Scripts Folder".to_string(),
                    CommandKind::Command
                ),
            ]
        );
    }

    /// A script's key is written into its file name, so it has to be read back
    /// out of the script rather than out of the key map, which knows nothing of
    /// the user's files.
    #[test]
    fn a_script_shows_the_key_its_file_name_carries() {
        let commands = commands(&scripted());

        let save = commands.iter().find(|c| c.path == "Script > Save").unwrap();
        assert_eq!(save.shortcut, Some(Shortcut::parse("@s").expect("a key").symbol()));

        let clean = commands
            .iter()
            .find(|c| c.path == "Script > Python > Clean")
            .unwrap();
        assert_eq!(clean.shortcut, None);
    }

    /// A snippet's key is its own record, for the same reason.
    #[test]
    fn a_snippet_shows_the_key_it_carries() {
        let mut state = state();
        state.snippets = vec![snippet(1, "Header", None, Some("@$k"))];

        let commands = commands(&state);
        let header = commands
            .iter()
            .find(|c| c.path == "Text > Insert Snippet > Header")
            .unwrap();
        assert_eq!(
            header.shortcut,
            Some(Shortcut::parse("@$k").expect("a key").symbol())
        );
    }

    /// For everything the table names, the bar agrees with it: same path, same
    /// action. The two are joined here so that a command one has and the other
    /// does not fails a test rather than disappearing from ⌘K.
    ///
    /// The state is given a recent document so that *Clear Menu* is live. It is
    /// the one entry whose presence in the bar is a state rather than a
    /// structure: the storyboard writes it down unconditionally, and upstream
    /// greys it while there is nothing to clear — which this build does too, and
    /// which `an_empty_history_leaves_just_the_clear_item_and_greys_it_out`
    /// covers from the other side.
    #[test]
    fn the_live_bar_agrees_with_the_generated_table() {
        let mut state = state();
        state.recent_documents = vec![PathBuf::from("/notes/todo.txt")];

        let live: Vec<(String, &'static str)> = commands(&state)
            .into_iter()
            .map(|command| (command.path, crate::bridge::name_of(command.action.as_ref())))
            .collect();

        for entry in menu::COMMANDS {
            // The one the bar leaves out, because it is the bar.
            if entry.path == "View > Show Quick Actions…" {
                continue;
            }
            assert!(
                live.iter().any(|(path, action)| path == entry.path && *action == entry.action),
                "{} is in the table but not in the bar",
                entry.path
            );
        }
    }

    /// Every command carries an action rather than a name, which is the only
    /// way the runtime items can be run: there is nothing to look a syntax or a
    /// script up by.
    #[test]
    fn every_command_carries_an_action() {
        for command in commands(&scripted()) {
            assert!(!command.path.is_empty());
            assert!(!crate::bridge::name_of(command.action.as_ref()).is_empty());
        }
    }

    /// The generated key map's modifier and key columns describe a shortcut
    /// this build can read: a modifier word it does not know would silently drop
    /// that modifier from the row, and a key of more than one character is not a
    /// shortcut at all.
    #[test]
    fn every_key_map_row_names_the_modifiers_it_has() {
        for row in keymap::SHORTCUTS {
            let shortcut = row_shortcut(row.2, row.3).unwrap_or_else(|| panic!("{} has no key", row.0));
            let named = [shortcut.control, shortcut.option, shortcut.shift, shortcut.command]
                .into_iter()
                .filter(|set| *set)
                .count();
            assert_eq!(
                named,
                row.2.split_whitespace().count(),
                "{} lost a modifier",
                row.0
            );
        }
    }
}
