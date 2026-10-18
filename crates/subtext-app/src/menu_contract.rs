//! The standing gate: every command the menu can send has somewhere to go.
//!
//! # Why a gate
//!
//! CotEditor's menu items are `@objc` selectors on `NSResponder`, so a selector
//! with no implementation simply does nothing — and a menu bar is the one place
//! where "does nothing" is easy to miss, because the item still looks live.
//! SubText's chain has three links and any of them can drop a command silently:
//!
//! 1. the generated table ([`crate::menu`]) names an action,
//! 2. [`crate::bridge`] binds that action to the name the dispatcher matches on,
//! 3. [`crate::dispatch`] answers that name.
//!
//! A missing link shows up as a dead item, or — worse, for link 2 — as an item
//! that never appears at all, because GPUI greys out an action nothing handles.
//! Neither is visible in a build, so this file reads the three sources and
//! compares them.
//!
//! # What it does not cover
//!
//! * It compares **names**, not behaviour: a dispatcher arm that reports "not
//!   implemented yet" still counts as a handler. That is deliberate — saying why
//!   a command is absent is the project's rule, and the item is not dead.
//! * It reads the sources as text. It matches the shapes those files use today
//!   (one `path => "Name",` per line in the binding table, one `"Name"` literal
//!   per dispatcher arm). A rewrite into another shape is meant to fail loudly
//!   here rather than pass quietly: each parser counts what it found and the
//!   tests assert a floor, so a parser that stops matching reports zero instead
//!   of an empty difference.
//! * Runtime actions (snippets, Format popups, recent documents, scripts) carry
//!   a payload and go through `bind_runtime_actions!`; they are listed
//!   separately below rather than parsed, because their names are not literals
//!   in that table.

use std::collections::{BTreeMap, BTreeSet};

const MENU: &str = include_str!("menu.rs");
const BRIDGE: &str = include_str!("bridge.rs");
const DISPATCH: &str = include_str!("dispatch.rs");

/// The floors below are what the sources hold today.
///
/// They are a health check rather than a coverage target: if a parser stops
/// matching (a table rewritten into another shape), it reports zero and the
/// difference it computes would be empty — vacuously green. These make that
/// loud instead.
const MINIMUM_MENU_ACTIONS: usize = 150;
const MINIMUM_BOUND_ACTIONS: usize = 150;
const MINIMUM_DISPATCHED_NAMES: usize = 150;

/// Actions the bridge binds with a payload instead of a name.
///
/// Their names come from `perform`, which matches on the action's type, so they
/// have no `path => "Name"` line to parse. Listed so the "every menu action is
/// bound" test does not have to special-case them by prefix.
const RUNTIME_ACTIONS: [&str; 8] = [
    "actions::snippet::InsertSnippet",
    "actions::format::ChangeLineEnding",
    "actions::format::ChangeSyntax",
    "actions::format::ChangeTheme",
    "actions::format::ChangeEncoding",
    "actions::text::NormalizeUnicode",
    "actions::file::OpenRecentDocument",
    "actions::script::RunScript",
];

/// `actions::group::Name` → the name the dispatcher matches on.
fn bound_actions() -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in BRIDGE.lines() {
        let line = line.trim();
        let Some((left, right)) = line.split_once(" => ") else {
            continue;
        };
        if !left.starts_with("actions::") {
            continue;
        }
        let Some(name) = right.trim().strip_prefix('"').and_then(|rest| rest.split('"').next())
        else {
            continue;
        };
        map.insert(left.trim().to_string(), name.to_string());
    }
    map
}

/// Every action path named by the generated menu table.
fn menu_actions() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = MENU;
    while let Some(start) = rest.find("actions::") {
        rest = &rest[start..];
        let end = rest
            .find(|c: char| c == ')' || c == ',' || c == '\n' || c.is_whitespace())
            .unwrap_or(rest.len());
        found.insert(rest[..end].to_string());
        rest = &rest[end..];
    }
    found
}

/// The names the dispatcher answers, taken from its `match name { … }` arms.
///
/// The region ends at the catch-all arm: below it is the "not implemented yet"
/// report, whose message text would otherwise count as a name.
fn dispatched_names() -> BTreeSet<String> {
    let start = DISPATCH.find("match name {").unwrap_or(0);
    let end = DISPATCH.find("other =>").unwrap_or(DISPATCH.len());
    let body = &DISPATCH[start..end];

    let mut names = BTreeSet::new();
    let mut rest = body;
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('"') else { break };
        let literal = &after[..close];
        // Names are identifiers; message text in this region holds spaces.
        if !literal.is_empty() && literal.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            names.insert(literal.to_string());
        }
        rest = &after[close + 1..];
    }
    names
}

#[test]
fn the_parsers_find_the_tables_they_are_meant_to_read() {
    // The health check for this gate: a parser that stopped matching reports
    // nothing, and every difference computed from it would be empty.
    let menu = menu_actions();
    let bound = bound_actions();
    let dispatched = dispatched_names();
    assert!(
        menu.len() >= MINIMUM_MENU_ACTIONS,
        "the menu parser found {} actions, below the floor of {MINIMUM_MENU_ACTIONS} — \
         the generated table probably changed shape, so this gate stopped reading it",
        menu.len()
    );
    assert!(
        bound.len() >= MINIMUM_BOUND_ACTIONS,
        "the binding parser found {} actions, below the floor of {MINIMUM_BOUND_ACTIONS} — \
         the binding table probably changed shape",
        bound.len()
    );
    assert!(
        dispatched.len() >= MINIMUM_DISPATCHED_NAMES,
        "the dispatcher parser found {} names, below the floor of {MINIMUM_DISPATCHED_NAMES} — \
         the match probably changed shape",
        dispatched.len()
    );
}

#[test]
fn every_menu_action_is_bound_to_a_name() {
    let bound = bound_actions();
    let missing: Vec<String> = menu_actions()
        .into_iter()
        .filter(|action| {
            !bound.contains_key(action) && !RUNTIME_ACTIONS.contains(&action.as_str())
        })
        .collect();
    assert!(
        missing.is_empty(),
        "these menu actions have no handler in bridge.rs, so GPUI will grey their items out:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn every_bound_name_is_answered() {
    let dispatched = dispatched_names();
    let unhandled: Vec<String> = bound_actions()
        .into_iter()
        .filter(|(_, name)| !dispatched.contains(name))
        .map(|(action, name)| format!("{action} => \"{name}\""))
        .collect();
    assert!(
        unhandled.is_empty(),
        "these bound names reach dispatch() with no arm for them, so the item reports \
         \"not implemented yet\" even though a menu command exists:\n  {}",
        unhandled.join("\n  ")
    );
}
