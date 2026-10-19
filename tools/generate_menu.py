#!/usr/bin/env python3
"""Regenerate the menu bar and the default key bindings.

`resources/menu.json` is a flat dump of CotEditor's `Main.storyboard`: one row
per menu item, in storyboard order, with the menu chain it sits under, its
title, its key equivalent and the `@objc` selector it fires. CotEditor itself
has no menu-building code to copy — the storyboard *is* the menu — so this
table is the only faithful source, and both generated files are read from it.

Two files come out of it:

  crates/subtext-app/src/menu.rs    the `Menu` tree handed to `cx.set_menus`
  crates/subtext-app/src/keymap.rs  `default_bindings()` and `SHORTCUTS`

Run it from the repository root:

    python3 tools/generate_menu.py

The output is deterministic, so re-running it on an unchanged table is a no-op.
"""

from __future__ import annotations

import json
import subprocess
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MENU_JSON = ROOT / "resources" / "menu.json"
ACTIONS_RS = ROOT / "crates" / "subtext-app" / "src" / "actions.rs"
MENU_RS = ROOT / "crates" / "subtext-app" / "src" / "menu.rs"
KEYMAP_RS = ROOT / "crates" / "subtext-app" / "src" / "keymap.rs"

# rustfmt's default, and the width the generated tables are wrapped to.
MAX_WIDTH = 100

# The application menu is the one place a menu bar shows the application's own
# name rather than a command's, so it is the one title that is ours to choose.
# Every other label is CotEditor's, verbatim.
APP_MENU_TITLE = "SubText"
SOURCE_APP_MENU_TITLE = "CotEditor"

# The placeholders CotEditor fills in from its own code, as `parent > title`.
# The storyboard has each of them as an empty `<menu>`, and so does this table:
# an empty submenu the runtime replaces by name. They are listed rather than
# guessed at, because the other kind of placeholder — the section headers
# (`Font Type`, `Orientation`, `Direction`) and the two the system owns — is a
# disabled leaf and has to stay one.
RUNTIME_SUBMENUS = {
    "Format > Line Endings",
    "Format > Text Encoding",
    "Format > Syntax",
    "Format > Theme",
    "Text > Unicode Normalizations",
}

# CotEditor leaves the Services menu to AppKit: the storyboard marks it
# `systemMenu="services"` and never fills it in. GPUI has the same notion, so
# the system gets a real menu here rather than a greyed leaf.
SYSTEM_MENUS = {
    "CotEditor > Services": ("Services", "Services"),
}

# Cocoa compares a key equivalent against the character a key produces, so a
# shortcut is stored as the character, not as the key. GPUI compares physical
# keys. These are the characters a US layout only produces with Shift held.
SHIFTED_SYMBOLS = {
    ":": ";",
    "?": "/",
    "+": "=",
    '"': "'",
    "(": "9",
    "_": "-",
    "<": ",",
    ">": ".",
    "{": "[",
    "}": "]",
    "|": "\\",
    "~": "`",
    "!": "1",
    "@": "2",
    "#": "3",
    "$": "4",
    "%": "5",
    "^": "6",
    "&": "7",
    "*": "8",
}

# Cocoa's private-use codes for the arrow keys, and the two control characters
# the storyboard spells out in base64.
#
# A key equivalent that is a control character cannot be written as an XML
# attribute, so Interface Builder stores it as `<string key="keyEquivalent"
# base64-UTF8="YES">`. `CA` is 0x08 and `Gw` is 0x1B, and reading those three
# entries without decoding them is how the Delete, Delete Line and Complete
# shortcuts went missing from the generated table.
NAMED_KEYS = {
    "\uf700": "up",
    "\uf701": "down",
    "\uf702": "left",
    "\uf703": "right",
    "\x08": "backspace",
    "\x1b": "escape",
}

# How the arrow keys are drawn in a shortcut label.
KEY_GLYPHS = {
    "up": "↑",
    "down": "↓",
    "left": "←",
    "right": "→",
    "backspace": "⌫",
    "escape": "⎋",
}

# Storyboard modifier name -> GPUI spelling, and -> the macOS glyph.
#
# GPUI spells Control `ctrl`, not `control`: `Keystroke::parse` only accepts
# `ctrl`, `alt`, `shift`, `fn`, `secondary`, `cmd`/`super`/`win`. A binding
# written `control-…` is not a modifier followed by a key as far as the parser
# is concerned — it is a key named `control` followed by something else, and
# `KeyBinding::new` panics on it. `tests::every_binding_parses` below is what
# keeps that from happening again.
MODIFIERS = {
    "control": ("ctrl", "⌃"),
    "option": ("alt", "⌥"),
    "shift": ("shift", "⇧"),
    "command": ("secondary", "⌘"),
}

# The order GPUI wants them in a binding string, and the order macOS draws them.
GPUI_ORDER = ["control", "option", "shift", "command"]
GLYPH_ORDER = ["control", "option", "shift", "command"]

# The words the Key Bindings pane uses, in the same order.
DISPLAY_ORDER = ["control", "option", "shift", "command"]

# Cocoa's `NSTextFinder` drives a dozen menu items from a single selector and
# tells them apart by the sender's `tag`. The storyboard dump has no `tag`
# column, so the distinction has to be recovered from the titles — which is
# safe here because these titles are what the tag means.
TEXT_FINDER_ACTIONS = {
    "Find…": "PerformTextFinderAction",
    "Find Next": "FindNext",
    "Find Previous": "FindPrevious",
    "Find All": "FindAll",
    "Select All Find Matches": "SelectAllFindMatches",
    "Use Selection for Find": "UseSelectionForFind",
    "Use Selection for Replace": "UseSelectionForReplace",
    "Replace": "ShowReplaceInterface",
    "Replace and Find Next": "ReplaceAndFindNext",
    "Replace All": "ReplaceAll",
    "Manage Multiple Replace…": "ShowMultipleReplace",
    "Highlight All": "HighlightAll",
    "Remove Highlights": "RemoveHighlights",
}

TEXT_FINDER_SELECTOR = "performTextFinderAction:"


def fail(message: str) -> None:
    print(f"generate_menu: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_action_namespaces() -> dict[str, str]:
    """Map every action type name to the module it is declared in."""
    source = ACTIONS_RS.read_text(encoding="utf-8")
    namespaces: dict[str, str] = {}
    for match in re.finditer(r"pub mod (\w+) \{(.*?)\n\}", source, re.S):
        namespace = match.group(1)
        for name in re.findall(r"^\s+([A-Z]\w*),", match.group(2), re.M):
            if name in namespaces:
                fail(f"action {name} is declared in two namespaces")
            namespaces[name] = namespace
    if not namespaces:
        fail("no actions found in actions.rs")
    return namespaces


# The commands whose enabled state depends on the editor, and what the
# generated table should test for each.
#
# CotEditor answers this in `validateUserInterfaceItem`, which runs every time a
# menu opens; a GPUI menu is a tree that is set once, so the condition is baked
# in and the bar is rebuilt when it flips. Only conditions that are cheap to
# evaluate belong here — the selection is one `read`, while the outline needs a
# parse of the buffer.
GATED_ACTIONS = {
    "StraightenQuotesInSelection": "state.selection_empty",
}


def action_for_selector(selector: str) -> str:
    """`showAboutPanel:` -> `ShowAboutPanel`, `encodeURL:` -> `EncodeUrl`."""
    name = selector.rstrip(":")
    name = name[:1].upper() + name[1:]
    # CotEditor spells these two selectors with an all-caps acronym; the
    # generated action names follow Rust's own casing.
    return name.replace("URL", "Url")


def action_for(entry: dict) -> str | None:
    """The action an entry fires, or `None` when CotEditor fills it in itself."""
    selector = entry["action"]
    if selector is None:
        return None
    if selector == TEXT_FINDER_SELECTOR:
        name = TEXT_FINDER_ACTIONS.get(entry["title"])
        if name is None:
            fail(f"no text finder action for {entry['title']!r}")
        return name
    return action_for_selector(selector)


def rust_string(text: str) -> str:
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def physical_key(character: str) -> tuple[str, bool]:
    """The key a user presses, and whether Shift is needed to produce it."""
    if character in NAMED_KEYS:
        return NAMED_KEYS[character], False
    if character in SHIFTED_SYMBOLS:
        return SHIFTED_SYMBOLS[character], True
    if len(character) == 1 and character.isascii() and character.isupper():
        return character.lower(), True
    return character, False


def binding_string(mods: list[str], character: str) -> str:
    key, implied_shift = physical_key(character)
    wanted = set(mods)
    if implied_shift:
        wanted.add("shift")
    parts = [MODIFIERS[name][0] for name in GPUI_ORDER if name in wanted]
    parts.append(key)
    return "-".join(parts)


def shortcut_label(mods: list[str], character: str) -> str:
    key, implied_shift = physical_key(character)
    wanted = set(mods)
    if implied_shift:
        wanted.add("shift")
    glyphs = "".join(MODIFIERS[name][1] for name in GLYPH_ORDER if name in wanted)
    return glyphs + KEY_GLYPHS.get(key, display_key(key))


def display_key(key: str) -> str:
    """How macOS draws the key in a shortcut: letters are upper case."""
    if len(key) == 1 and key.isascii() and key.isalpha():
        return key.upper()
    return key


def modifier_words(mods: list[str], character: str) -> str:
    _, implied_shift = physical_key(character)
    wanted = set(mods)
    if implied_shift:
        wanted.add("shift")
    return " ".join(name for name in DISPLAY_ORDER if name in wanted)


def item_key(path: str, title: str) -> str:
    """The chain an entry's own children would sit under."""
    if path == "":
        return title
    if path == SOURCE_APP_MENU_TITLE:
        return f"{SOURCE_APP_MENU_TITLE} > {title}"
    return f"{path} > {title}"


class Renderer:
    def __init__(self, entries: list[dict], namespaces: dict[str, str]):
        self.entries = entries
        self.namespaces = namespaces
        self.children: dict[str, list[dict]] = {}
        for entry in entries:
            self.children.setdefault(entry["path"], []).append(entry)
        self.submenu_keys = {entry["path"] for entry in entries if entry["path"]}
        # Counted as items are rendered rather than derived from the table: a
        # top-level entry that is a menu of its own (`Script`) is not an item
        # of anything, and deriving the number would miss that.
        self.rendered_actions = 0

    def is_submenu(self, entry: dict) -> bool:
        return item_key(entry["path"], entry["title"]) in self.submenu_keys

    def render_menu(self, path_key: str, indent: int, name: str) -> list[str]:
        pad = " " * indent
        inner = " " * (indent + 4)
        items = self.children.get(path_key, [])
        lines = [f"{pad}Menu {{"]
        lines.append(f"{inner}name: {rust_string(name)}.into(),")
        lines.append(f"{inner}items: vec![")
        for entry in items:
            lines.extend(self.render_item(entry, indent + 8))
        lines.append(f"{inner}],")
        # A menu with nothing in it is CotEditor's Script menu before any
        # scripts are installed. It stays in the bar, greyed, rather than
        # vanishing — an empty menu that cannot open is still the honest shape.
        lines.append(f"{inner}disabled: {str(not items).lower()},")
        lines.append(f"{pad}}},")
        return lines

    def render_item(self, entry: dict, indent: int) -> list[str]:
        pad = " " * indent
        title = entry["title"]
        if title == "-":
            return [f"{pad}MenuItem::separator(),"]

        if self.is_submenu(entry):
            key = item_key(entry["path"], title)
            lines = [f"{pad}MenuItem::submenu(Menu {{"]
            inner = " " * (indent + 4)
            lines.append(f"{inner}name: {rust_string(title)}.into(),")
            lines.append(f"{inner}items: vec![")
            for child in self.children.get(key, []):
                lines.extend(self.render_item(child, indent + 8))
            lines.append(f"{inner}],")
            lines.append(f"{inner}disabled: false,")
            lines.append(f"{pad}}}),")
            return lines

        key = item_key(entry["path"], title)
        if key in RUNTIME_SUBMENUS:
            # An empty submenu, disabled so it reads as greyed until something
            # fills it — the shape the storyboard has, and the shape the
            # runtime replaces. Not an action, so it is not counted as one.
            return [
                f"{pad}MenuItem::submenu(Menu {{",
                f"{pad}    name: {rust_string(title)}.into(),",
                f"{pad}    items: vec![],",
                f"{pad}    disabled: true,",
                f"{pad}}}),",
            ]

        if key in SYSTEM_MENUS:
            name, kind = SYSTEM_MENUS[key]
            return [
                f"{pad}MenuItem::os_submenu({rust_string(name)}, SystemMenuType::{kind}),"
            ]

        name = action_for(entry)
        self.rendered_actions += 1
        if name is None:
            # The two the system owns (Insert from iPhone or iPad) and the
            # section headers CotEditor draws inside a menu it has already
            # filled (Font Type, Orientation, Direction). They stay visible and
            # greyed, exactly as they are in CotEditor when nothing supplies
            # them.
            return [
                f"{pad}MenuItem::action({rust_string(title)}, actions::app::NoOp).disabled(true),"
            ]

        namespace = self.namespaces.get(name)
        if namespace is None:
            fail(f"no action named {name} for {title!r}")
        gate = GATED_ACTIONS.get(name)
        suffix = f".disabled({gate})" if gate else ""
        return [
            f"{pad}MenuItem::action({rust_string(title)}, "
            f"actions::{namespace}::{name}){suffix},"
        ]

    def app_menus(self) -> str:
        top_level = [entry for entry in self.entries if entry["path"] == ""]
        if not top_level:
            fail("menu.json has no top-level menus")

        submenus = sum(1 for entry in self.entries if self.is_submenu(entry))
        separators = sum(1 for entry in self.entries if entry["title"] == "-")
        commands = sum(1 for entry in self.entries if action_for(entry) is not None)
        placeholders = len(self.entries) - submenus - separators - commands

        # The doc comment counts every entry with a selector; the Quick Actions
        # table is the subset that is not a submenu. No submenu carries a
        # selector in this table, so the two numbers are the same one — and if
        # a future storyboard gives one a selector, the bar must not gain an
        # item the menu bar cannot run.
        command_rows = self.command_entries()
        if len(command_rows) != commands:
            fail(
                f"{commands} commands in the menu but {len(command_rows)} for the "
                "Quick Actions bar: a submenu now carries an action"
            )

        lines = [
            "//! The application menu bar.",
            "//!",
            "//! Generated from `resources/menu.json` by `tools/generate_menu.py`.",
            "//! That table is itself a dump of CotEditor's `Main.storyboard` — the",
            "//! storyboard *is* the source of truth for CotEditor's menu structure, so",
            "//! reading it directly is what keeps the two applications' menus identical",
            "//! rather than merely similar.",
            "//!",
            f"//! {len(self.entries)} items: {submenus} submenus, {commands} commands,",
            f"//! {separators} separators and {placeholders} placeholders. The placeholders",
            "//! are the items CotEditor fills in from the system or at runtime — Services,",
            "//! the Format popups, the Script menu — and they stay visible and greyed",
            "//! rather than being dropped.",
            "//!",
            "//! Six of them are the *runtime* ones: the storyboard has them as empty",
            "//! `<menu>` elements, and so does this table, as an empty submenu that",
            "//! `crate::menus` replaces by name before the bar is set. `Script` is the",
            "//! seventh, an empty top-level menu. The rest are leaves that stay leaves:",
            "//! the two the system owns, and the section headers CotEditor draws inside",
            "//! a menu it has already filled.",
            "//!",
            "//! Do not edit this file by hand — run the generator instead.",
            "",
            "use gpui_kit::{Menu, MenuItem, SystemMenuType};",
            "",
            "use crate::actions;",
            "",
            "/// Every top-level menu, in CotEditor's order.",
            "pub fn app_menus(state: &crate::menus::MenuGates) -> Vec<Menu> {",
            "    vec![",
        ]

        root = top_level[0]
        lines.extend(self.render_menu(root["title"], 8, APP_MENU_TITLE))
        for entry in top_level[1:]:
            lines.extend(self.render_menu(entry["title"], 8, entry["title"]))

        lines.append("    ]")
        lines.append("}")
        lines.append("")
        lines.extend(self.commands())
        lines.append("#[cfg(test)]")
        lines.append("mod tests {")
        lines.append("    use super::*;")
        lines.append("")
        lines.append("    /// The bar is CotEditor's, in CotEditor's order, with the application")
        lines.append("    /// menu renamed. Nothing else may drift.")
        lines.append("    #[test]")
        lines.append("    fn the_bar_has_every_top_level_menu() {")
        lines.append(f"        assert_eq!(app_menus(&crate::menus::MenuGates::default()).len(), {len(top_level)});")
        lines.append("        let names: Vec<_> = app_menus(&crate::menus::MenuGates::default())")
        lines.append("            .iter()")
        lines.append("            .map(|menu| menu.name.to_string())")
        lines.append("            .collect();")
        lines.append("        assert_eq!(")
        lines.append("            names,")
        lines.append("            [")
        lines.append(f"                {rust_string(APP_MENU_TITLE)},")
        for entry in top_level[1:]:
            lines.append(f"                {rust_string(entry['title'])},")
        lines.append("            ]")
        lines.append("        );")
        lines.append("    }")
        lines.append("")
        lines.append("    /// A menu item whose action nothing handles is greyed out by GPUI, so")
        lines.append("    /// every *leaf* has to name a real action — even the placeholders that")
        lines.append("    /// stay placeholders, which say `NoOp` and disable themselves.")
        lines.append("    ///")
        lines.append("    /// The empty submenus are counted as nothing, not as one item each: the")
        lines.append("    /// runtime fills them in, and a test that counted them would go on")
        lines.append("    /// passing if one of them were never filled.")
        lines.append("    #[test]")
        lines.append("    fn every_item_has_an_action() {")
        lines.append("        fn count(menu: &Menu) -> usize {")
        lines.append("            menu.items")
        lines.append("                .iter()")
        lines.append("                .map(|item| match item {")
        lines.append("                    MenuItem::Submenu(inner) => count(inner),")
        lines.append("                    MenuItem::Action { .. } => 1,")
        lines.append("                    _ => 0,")
        lines.append("                })")
        lines.append("                .sum()")
        lines.append("        }")
        lines.append("        let total: usize = app_menus(&crate::menus::MenuGates::default()).iter().map(count).sum();")
        lines.append(f"        assert_eq!(total, {self.rendered_actions});")
        lines.append("    }")
        lines.append("")
        lines.append("    /// Every command the bar can offer is one the menu bar has.")
        lines.append("    ///")
        lines.append("    /// The two tables come from one source, so a command that went")
        lines.append("    /// missing from either would be a generator bug rather than a typo.")
        lines.append("    #[test]")
        lines.append("    fn the_command_table_is_the_whole_menu() {")
        lines.append(f"        assert_eq!(COMMANDS.len(), {len(command_rows)});")
        lines.append("        let mut paths: Vec<_> = COMMANDS.iter().map(|c| c.path).collect();")
        lines.append("        paths.sort_unstable();")
        lines.append("        paths.dedup();")
        lines.append("        assert_eq!(paths.len(), COMMANDS.len(), \"a path names two commands\");")
        lines.append("        for command in COMMANDS {")
        lines.append("            assert!(!command.path.is_empty());")
        lines.append("            assert!(!command.action.is_empty());")
        lines.append("        }")
        lines.append("    }")
        lines.append("")
        lines.append("    /// A shortcut belongs to a command, so its path has to be one of ours.")
        lines.append("    ///")
        lines.append("    /// This is the join [`crate::menus::shortcut`] makes to draw a")
        lines.append("    /// shortcut beside a result: if the two tables spelled a path")
        lines.append("    /// differently the join would silently find nothing and the bar")
        lines.append("    /// would show no key.")
        lines.append("    #[test]")
        lines.append("    fn every_shortcut_belongs_to_a_listed_command() {")
        lines.append("        for row in crate::keymap::SHORTCUTS {")
        lines.append("            assert!(")
        lines.append("                COMMANDS.iter().any(|command| command.path == row.0),")
        lines.append("                \"{} has no command\",")
        lines.append("                row.0")
        lines.append("            );")
        lines.append("        }")
        lines.append("    }")
        lines.append("}")
        lines.append("")
        return "\n".join(lines)

    def command_entries(self) -> list[dict]:
        """The commands the storyboard names.

        A submenu has no action of its own, and a placeholder has no action at
        all; what is left is exactly the items a user can run.
        """
        return [
            entry
            for entry in self.entries
            if not self.is_submenu(entry) and action_for(entry) is not None
        ]

    def commands(self) -> list[str]:
        """The flat command table the Quick Actions bar reads."""
        rows = self.command_entries()
        if not rows:
            fail("menu.json has no commands")

        lines = [
            "/// A command in the menu bar, as the storyboard records it.",
            "///",
            "/// CotEditor builds its Quick Actions list by walking the live `NSMenu`,",
            "/// which is also where the items it fills in at runtime come from. GPUI",
            "/// can set a menu but not read one back, so the bar does not walk the",
            "/// installed menu — it walks the same bar built again, which is what",
            "/// brings the runtime items with it. See [`crate::menus::commands`].",
            "///",
            "/// This table is the compile-time record of that bar: every command the",
            "/// storyboard names, and the action each one runs. It is what makes the",
            "/// two applications' menus comparable at a glance, and a test holds the",
            "/// live bar against it.",
            "pub struct Command {",
            "    /// Where the command sits, e.g. `Format > Syntax`.",
            "    ///",
            "    /// Spelled the way [`crate::keymap::SHORTCUTS`] spells it, which is",
            "    /// what lets a command be joined to its shortcut.",
            "    pub path: &'static str,",
            "    /// The last segment of the action's path, which is the vocabulary",
            "    /// `Workspace::dispatch` speaks.",
            "    pub action: &'static str,",
            "}",
            "",
            "/// The name the application menu is shown under.",
            "///",
            "/// A command's path is spelled with the source application's name so that",
            "/// it matches [`crate::keymap::SHORTCUTS`]; this is what a display has to",
            "/// substitute for it.",
            f"pub const APP_MENU_TITLE: &str = {rust_string(APP_MENU_TITLE)};",
            "",
            "/// Every command the storyboard names, in menu order.",
            "///",
            "/// The greyed placeholders are not here: CotEditor fills those in from the",
            "/// system or at runtime, and a placeholder has no action to run. This is",
            "/// the storyboard's own record, not the list the Quick Actions bar reads —",
            "/// that one is [`crate::menus::commands`], and the two are held together",
            "/// by a test.",
            "pub const COMMANDS: &[Command] = &[",
        ]
        for entry in rows:
            path = rust_string(item_key(entry["path"], entry["title"]))
            action = rust_string(action_for(entry))
            one_line = f"    Command {{ path: {path}, action: {action} }},"
            # Wrapped the way rustfmt would wrap it, so a regeneration does not
            # leave the file with a handful of over-long lines in it.
            if len(one_line) <= MAX_WIDTH:
                lines.append(one_line)
            else:
                lines.append("    Command {")
                lines.append(f"        path: {path},")
                lines.append(f"        action: {action},")
                lines.append("    },")
        lines.append("];")
        lines.append("")
        return lines

    def keymap(self) -> str:
        rows = [entry for entry in self.entries if entry["key"]]
        if not rows:
            fail("menu.json has no key equivalents")

        lines = [
            "//! The key bindings.",
            "//!",
            "//! Generated from `resources/menu.json` by `tools/generate_menu.py`.",
            "//! CotEditor's defaults are not stored in a file at all —",
            "//! `KeyBindingManager` walks the loaded `NSMenu` and reads each item's",
            "//! `keyEquivalent`, so the storyboard is the only source of truth.",
            "//! Reading the same table keeps the two shortcut sets identical.",
            "//!",
            "//! `command` is written as GPUI's `secondary`, which resolves to Command on",
            "//! macOS and Control on Windows and Linux. That is the one deliberate",
            "//! divergence: a Command-only shortcut would be unreachable on the other two",
            "//! platforms, and CotEditor has no Windows or Linux build to copy from.",
            "//!",
            "//! Cocoa matches a shortcut on the *character* a key produces, so CotEditor's",
            "//! `Cmd+:` is really `Cmd+Shift+;` on a US layout; that translation is done",
            "//! here, once, rather than left to the reader.",
            "//!",
            "//! Do not edit this file by hand — run the generator instead.",
            "",
            "use gpui_kit::KeyBinding;",
            "",
            "use crate::actions;",
            "",
            "/// Every default shortcut, in CotEditor's menu order.",
            "pub fn default_bindings() -> Vec<KeyBinding> {",
            "    vec![",
        ]

        seen: dict[str, str] = {}
        collisions: list[tuple[str, str, str]] = []

        for entry in rows:
            name = action_for(entry)
            if name is None:
                fail(f"the shortcut for {entry['title']!r} has no action")
            namespace = self.namespaces.get(name)
            if namespace is None:
                fail(f"no action named {name} for {entry['action']!r}")
            binding = binding_string(entry["mods"], entry["key"])
            label = f"{entry['path']} > {entry['title']}"
            if binding in seen:
                collisions.append((binding, seen[binding], label))
            else:
                seen[binding] = label
            lines.append(f"    // {label}")
            lines.append(
                f"    KeyBinding::new({rust_string(binding)}, "
                f"actions::{namespace}::{name}, None),"
            )

        lines.append("    ]")
        lines.append("}")
        lines.append("")
        lines.append("")
        lines.append("/// Every default shortcut, as the Key Bindings pane shows it.")
        lines.append("///")
        lines.append("/// The table comes from the same `resources/menu.json` the menu bar and the")
        lines.append("/// key map are generated from, so the three cannot drift apart. Each row is")
        lines.append("/// (menu path, the shortcut as macOS draws it, its modifiers, the key, the")
        lines.append("/// keystroke).")
        lines.append("///")
        lines.append("/// The last field is the very string [`default_bindings`] hands GPUI, which")
        lines.append("/// is what lets anything asking \"does a command already answer to this key?\"")
        lines.append("/// — the Snippets pane's shortcut field — get the same answer the key map")
        lines.append("/// would give, and name the command in the message. It is kept here rather")
        lines.append("/// than derived from the two fields before it because the translation from a")
        lines.append("/// character to a key lives in the generator, and a second copy of it in Rust")
        lines.append("/// would be a second thing to get wrong.")
        lines.append("pub const SHORTCUTS: &[(&str, &str, &str, &str, &str)] = &[")
        for entry in rows:
            label = f"{entry['path']} > {entry['title']}"
            lines.append(
                "    ("
                f"{rust_string(label)}, "
                f"{rust_string(shortcut_label(entry['mods'], entry['key']))}, "
                f"{rust_string(modifier_words(entry['mods'], entry['key']))}, "
                f"{rust_string(KEY_GLYPHS.get(physical_key(entry['key'])[0], entry['key']))}, "
                f"{rust_string(binding_string(entry['mods'], entry['key']))}"
                "),"
            )
        lines.append("];")
        lines.append("")
        lines.append("/// Shortcuts CotEditor assigns twice, kept visible rather than silently")
        lines.append("/// resolved. macOS settles them by menu order; so do we.")
        lines.append("pub const KNOWN_COLLISIONS: &[(&str, &str, &str)] = &[")
        for binding, first, second in collisions:
            lines.append(f"    ({rust_string(binding)}, {rust_string(first)}, {rust_string(second)}),")
        lines.append("];")
        lines.append("")
        lines.append("#[cfg(test)]")
        lines.append("mod tests {")
        lines.append("    use super::*;")
        lines.append("")
        lines.append("    /// Every binding has to survive GPUI's keystroke parser.")
        lines.append("    ///")
        lines.append("    /// `KeyBinding::new` panics on a keystroke it cannot read, and it")
        lines.append("    /// reads far less than it looks like it does — `control-` is a key")
        lines.append("    /// named `control`, not the Control modifier, so a table written with")
        lines.append("    /// the wrong spelling takes the whole application down at launch")
        lines.append("    /// rather than at the first keypress. Building the table is the test.")
        lines.append("    #[test]")
        lines.append("    fn every_binding_parses() {")
        lines.append(f"        assert_eq!(default_bindings().len(), {len(rows)});")
        lines.append("    }")
        lines.append("")
        lines.append("    /// Two commands on one keystroke is a bug unless it is CotEditor's bug.")
        lines.append("    ///")
        lines.append("    /// The generator reports collisions in binding terms when it writes")
        lines.append("    /// [`KNOWN_COLLISIONS`]; this test is what fails when a regeneration")
        lines.append("    /// finds one that has not been acknowledged.")
        lines.append("    #[test]")
        lines.append("    fn collisions_are_the_ones_we_know_about() {")
        lines.append("        assert_eq!(KNOWN_COLLISIONS.len(), 0);")
        lines.append("    }")
        lines.append("")
        lines.append("    /// The Key Bindings pane lists the same table the key map is built from.")
        lines.append("    #[test]")
        lines.append("    fn the_shortcut_table_matches_the_bindings() {")
        lines.append("        let bindings = default_bindings();")
        lines.append("        assert_eq!(bindings.len(), SHORTCUTS.len());")
        lines.append("        for row in SHORTCUTS {")
        lines.append("            assert!(!row.0.is_empty(), \"every shortcut belongs to a menu item\");")
        lines.append("            assert!(!row.1.is_empty(), \"{} has no shortcut\", row.0);")
        lines.append("        }")
        lines.append("    }")
        lines.append("")
        lines.append("    /// The table's last field is the keystroke the key map really binds.")
        lines.append("    ///")
        lines.append("    /// That is what makes it usable as the answer to \"is this shortcut")
        lines.append("    /// taken?\": a table row and a binding that disagreed would name one")
        lines.append("    /// command and reserve another, which is exactly the bug the Snippets")
        lines.append("    /// pane's shortcut field exists to prevent.")
        lines.append("    #[test]")
        lines.append("    fn the_table_holds_the_keystrokes_the_key_map_binds() {")
        lines.append("        for (binding, row) in default_bindings().iter().zip(SHORTCUTS) {")
        lines.append("            let typed = gpui_kit::Keystroke::parse(row.4).expect(row.4);")
        lines.append("            assert_eq!(")
        lines.append("                binding.match_keystrokes(&[typed]),")
        lines.append("                Some(false),")
        lines.append("                \"{} binds {}\",")
        lines.append("                row.0,")
        lines.append("                row.4")
        lines.append("            );")
        lines.append("        }")
        lines.append("    }")
        lines.append("}")
        lines.append("")
        return "\n".join(lines)


def main() -> None:
    entries = json.loads(MENU_JSON.read_text(encoding="utf-8"))
    namespaces = load_action_namespaces()
    renderer = Renderer(entries, namespaces)

    MENU_RS.write_text(renderer.app_menus(), encoding="utf-8")
    KEYMAP_RS.write_text(renderer.keymap(), encoding="utf-8")

    # These two files are generated *and* checked by `cargo fmt --check`, so the
    # generator formats what it writes. Without this, regenerating would leave
    # the tree failing the format gate — which is exactly how a gate gets turned
    # off "just this once".
    try:
        subprocess.run(
            ["rustfmt", "--edition", "2024", str(MENU_RS), str(KEYMAP_RS)],
            check=True,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        fail(f"rustfmt failed on the generated files: {error}")

    commands = sum(1 for entry in entries if entry["action"])
    shortcuts = sum(1 for entry in entries if entry["key"])
    print(f"menu.rs:   {len(entries)} items, {commands} commands")
    print(f"keymap.rs: {shortcuts} shortcuts")
    if renderer.children.get(SOURCE_APP_MENU_TITLE) is None:
        fail("the application menu has no items")


if __name__ == "__main__":
    main()
