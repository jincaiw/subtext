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
//!   a command is absent is the project's rule, and the item is not dead. Those
//!   arms are not left to trust, though: [`NOTICE_ONLY`] names every command the
//!   project knowingly answers with an explanation, and the test below fails if
//!   the set of notice-only arms drifts from it in either direction, so a new
//!   dead item cannot hide behind a notice.
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

/// The commands that are answered with an explanation instead of an edit.
///
/// Upstream implements these through AppKit (`NSPrintOperation`, the spelling
/// and speech services, NSTextView's substitution panel and writing direction,
/// the font panel, AppleScript). GPUI exposes none of them, so the honest
/// answer is a notice saying why — but a notice is also indistinguishable from
/// a dead item at a glance, which is exactly the bug this file exists to catch.
/// Writing the names down here is the line between "understood limitation" and
/// "silently broken": a new notice-only arm fails the test until it is added
/// here with a reason, and an entry that no longer matches an arm fails too.
const NOTICE_ONLY: [&str; 24] = [
    "ShareDocument",
    "RunPageLayout",
    "PrintDocument",
    "ShowGuessPanel",
    "CheckSpelling",
    "ToggleContinuousSpellChecking",
    "ToggleGrammarChecking",
    "ToggleAutomaticSpellingCorrection",
    "ToggleAutomaticQuoteSubstitution",
    "ToggleAutomaticDashSubstitution",
    "ToggleAutomaticTextReplacement",
    "ToggleSmartInsertDelete",
    "OrderFrontSubstitutionsPanel",
    "StartSpeaking",
    "StopSpeaking",
    "ReplaceQuotesInSelection",
    "ReplaceDashesInSelection",
    "ReplaceTextInSelection",
    "OrderFrontFontPanel",
    "MakeWritingDirectionLeftToRight",
    "MakeWritingDirectionRightToLeft",
    "MakeLayoutOrientationHorizontal",
    "MakeLayoutOrientationVertical",
    "OpenAppleScriptDictionary",
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
        let Some(name) = right
            .trim()
            .strip_prefix('"')
            .and_then(|rest| rest.split('"').next())
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
        if !literal.is_empty()
            && literal
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            names.insert(literal.to_string());
        }
        rest = &after[close + 1..];
    }
    names
}

/// The name literals inside the dispatcher's `match name { … }`, with their
/// byte span in [`DISPATCH`] and the byte span of the whole match.
///
/// A literal counts only when it is identifier-shaped; notice messages hold
/// spaces, so they do not disturb the walk.
fn name_literals() -> (Vec<(usize, usize, String)>, std::ops::Range<usize>) {
    let start = DISPATCH.find("match name {").unwrap_or(0);
    let end = DISPATCH.find("other =>").unwrap_or(DISPATCH.len());
    let body = &DISPATCH[start..end];

    let mut literals = Vec::new();
    let mut at = 0;
    while let Some(open) = body[at..].find('"') {
        let open = at + open;
        let Some(close) = body[open + 1..].find('"').map(|offset| open + 1 + offset) else {
            break;
        };
        let literal = &body[open + 1..close];
        if !literal.is_empty()
            && literal
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            literals.push((open, close + 1, literal.to_string()));
        }
        at = close + 1;
    }
    (literals, start..end)
}

/// `text` with `//` line comments removed.
///
/// An arm's body is taken up to the next name literal, which means it also
/// carries any comment written above that next arm. The comments are prose and
/// would otherwise make every notice look like it did more than it does.
fn without_line_comments(text: &str) -> String {
    text.lines()
        .map(|line| line.find("//").map_or(line, |at| &line[..at]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether an arm body does nothing but call `self.notice(…)`.
///
/// The notice's arguments are removed first — the message text is prose, and
/// two of the calls are `format!`-wrapped — then what is left has to be only
/// the punctuation an arm needs. `RecolorAll` calls `self.notice` too, but it
/// also rehighlights, so it is not notice-only.
fn is_notice_only(arm_body: &str) -> bool {
    let mut rest = String::new();
    let mut at = 0;
    let mut found = false;
    while let Some(offset) = arm_body[at..].find("self.notice(") {
        let open = at + offset + "self.notice".len();
        rest.push_str(&arm_body[at..at + offset]);

        let mut depth = 0usize;
        let mut index = open;
        while index < arm_body.len() {
            match arm_body.as_bytes()[index] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        index += 1;
                        break;
                    }
                }
                _ => {}
            }
            index += 1;
        }
        at = index;
        found = true;
    }
    rest.push_str(&arm_body[at..]);
    found
        && rest
            .chars()
            .all(|c| c.is_whitespace() || matches!(c, '{' | '}' | ';' | ','))
}

/// The names whose dispatcher arm only calls `self.notice(…)`.
fn notice_only_names() -> BTreeSet<String> {
    let (literals, span) = name_literals();
    let body = &DISPATCH[span];

    let mut found = BTreeSet::new();
    let mut group: Vec<String> = Vec::new();
    for (index, (_, end, name)) in literals.iter().enumerate() {
        group.push(name.clone());
        let next = literals.get(index + 1).map_or(body.len(), |next| next.0);
        let between = &body[*end..next];
        let Some(arrow) = between.find("=>") else {
            // Another `| "Name"` alternative of the same pattern.
            continue;
        };
        let arm_body = without_line_comments(&body[*end + arrow + 2..next]);
        if is_notice_only(&arm_body) {
            found.extend(group.drain(..));
        } else {
            group.clear();
        }
    }
    found
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
        .filter(|action| !bound.contains_key(action) && !RUNTIME_ACTIONS.contains(&action.as_str()))
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

#[test]
fn only_documented_gaps_are_answered_with_a_notice() {
    // Both directions matter. A command that used to work must not be demoted
    // to a notice without the list being edited, and a name left in the list
    // after the arm became real would keep failing nothing but the docs.
    let found = notice_only_names();
    let expected: BTreeSet<String> = NOTICE_ONLY.iter().map(|name| name.to_string()).collect();
    let unexpected: Vec<&String> = found.difference(&expected).collect();
    let stale: Vec<&String> = expected.difference(&found).collect();
    assert!(
        unexpected.is_empty(),
        "these dispatcher arms only say a command is unavailable; either implement them or \
         add them to NOTICE_ONLY with the reason:\n  {unexpected:#?}"
    );
    assert!(
        stale.is_empty(),
        "NOTICE_ONLY lists commands that are no longer notice-only (or no longer dispatched):\n  \
         {stale:#?}"
    );
    // And the parser has to be reading real arms, not silently finding none.
    assert!(
        found.len() >= 20,
        "the notice parser found only {} names; the match probably changed shape",
        found.len()
    );
}

#[test]
fn the_generated_table_gates_exactly_the_commands_that_say_so() {
    // The menu bar is a tree that is set once, so a state-dependent item is
    // greyed by the *generator* rather than by the item. That makes the
    // generated line the contract: one gate per command that has one, and each
    // tests a field the workspace actually maintains.
    let gated = gated_items();
    let expected = [
        ("StraightenQuotesInSelection", "state.selection_empty"),
        ("SelectPrevItemOfOutlineMenu", "!state.can_previous_outline"),
        ("SelectNextItemOfOutlineMenu", "!state.can_next_outline"),
        // `EditorTextView.validateUserInterfaceItem`: the comment commands need
        // an editable buffer, and each needs the delimiter its command uses.
        ("ToggleComment", "!state.can_comment"),
        ("InlineCommentOut", "!state.can_inline_comment"),
        ("BlockCommentOut", "!state.can_block_comment"),
        ("Uncomment", "!state.can_uncomment"),
    ];
    assert_eq!(
        gated.len(),
        expected.len(),
        "expected {} state-dependent items, found {}:\n  {}",
        expected.len(),
        gated.len(),
        gated.join("\n  ")
    );
    for (action, gate) in expected {
        // Match the action, not the title: *Toggle Comment* is shown as
        // "Uncomment" when the whole selection is commented out, and that word
        // is also the action name of the item beside it. The action ends the
        // line with a `)` when it fits on one line and with a `,` when rustfmt
        // breaks the call across lines.
        let item = gated
            .iter()
            .find(|item| {
                item.contains(&format!("::{action},")) || item.contains(&format!("::{action})"))
            })
            .unwrap_or_else(|| panic!("{action} carries no gate:\n  {}", gated.join("\n  ")));
        assert!(
            item.contains(gate),
            "{action} tests a field the workspace does not maintain: {item}"
        );
    }
}

/// The gates reach the tree the platform is handed, not just the source text.
///
/// The test above proves the generator wrote a condition; this proves the
/// condition is the one the workspace fills in. Built twice with the two
/// answers the workspace can give and read back out of the menu bar.
#[test]
fn the_outline_items_follow_the_caret() {
    let nothing = crate::menu::app_menus(&crate::menus::MenuGates::default());
    assert_eq!(
        disabled(&nothing, "Select Previous Outline Item"),
        Some(true),
        "with no document the item has nothing to select"
    );
    assert_eq!(
        disabled(&nothing, "Select Next Outline Item"),
        Some(true),
        "with no document the item has nothing to select"
    );

    let both_ways = crate::menu::app_menus(&crate::menus::MenuGates {
        selection_empty: false,
        can_previous_outline: true,
        can_next_outline: true,
        ..Default::default()
    });
    assert_eq!(
        disabled(&both_ways, "Select Previous Outline Item"),
        Some(false)
    );
    assert_eq!(
        disabled(&both_ways, "Select Next Outline Item"),
        Some(false)
    );

    // And each one follows its own field, rather than both following one.
    let forward_only = crate::menu::app_menus(&crate::menus::MenuGates {
        can_next_outline: true,
        ..Default::default()
    });
    assert_eq!(
        disabled(&forward_only, "Select Previous Outline Item"),
        Some(true)
    );
    assert_eq!(
        disabled(&forward_only, "Select Next Outline Item"),
        Some(false)
    );
}

/// The comment item says which job it will do, as upstream's
/// `EditorTextView.validateUserInterfaceItem` retitles it on every menu open.
///
/// A GPUI menu is a tree set once, so the title comes out of the same gates
/// the disabled state does, and the bar is rebuilt when the answer changes
/// (`Workspace::sync_menu_gates`). The editable flag is deliberately not part
/// of this: upstream retitles a greyed item too.
#[test]
fn the_comment_item_is_titled_by_the_selection() {
    let uncomments = crate::menu::app_menus(&crate::menus::MenuGates {
        can_comment: true,
        can_uncomment_whole: true,
        ..Default::default()
    });
    assert_eq!(
        title_of(&uncomments, "ToggleComment").as_deref(),
        Some("Uncomment"),
        "the whole selection is commented out, so the item removes delimiters"
    );

    let comments_out = crate::menu::app_menus(&crate::menus::MenuGates {
        can_comment: true,
        can_uncomment_whole: false,
        ..Default::default()
    });
    assert_eq!(
        title_of(&comments_out, "ToggleComment").as_deref(),
        Some("Comment Out"),
        "there is something left to comment, so the item adds delimiters"
    );
}

/// The title of the one item that carries `action_name`, wherever it sits.
fn title_of(menus: &[gpui_kit::Menu], action_name: &str) -> Option<String> {
    fn walk(items: &[gpui_kit::MenuItem], action_name: &str) -> Option<String> {
        for item in items {
            match item {
                gpui_kit::MenuItem::Action { name, action, .. }
                    if crate::bridge::name_of(action.as_ref()) == action_name =>
                {
                    return Some(name.to_string());
                }
                gpui_kit::MenuItem::Submenu(menu) => {
                    if let Some(found) = walk(&menu.items, action_name) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    menus.iter().find_map(|menu| walk(&menu.items, action_name))
}

/// Whether the item titled `title` is greyed, wherever it sits in the bar.
fn disabled(menus: &[gpui_kit::Menu], title: &str) -> Option<bool> {
    fn walk(items: &[gpui_kit::MenuItem], title: &str) -> Option<bool> {
        for item in items {
            match item {
                gpui_kit::MenuItem::Action { name, disabled, .. } if name.as_ref() == title => {
                    return Some(*disabled);
                }
                gpui_kit::MenuItem::Submenu(menu) => {
                    if let Some(found) = walk(&menu.items, title) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    menus.iter().find_map(|menu| walk(&menu.items, title))
}

/// Every generated item that carries a state-dependent `.disabled(…)`.
///
/// The item is spelled across as many lines as rustfmt decides — the generated
/// files are formatted by the generator, so a test that read one line would
/// pass or fail on formatting rather than on the gate. Each result is the
/// `MenuItem::action(…)` call plus the suffix attached to it, joined.
fn gated_items() -> Vec<String> {
    let lines: Vec<&str> = MENU.lines().collect();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        // `.disabled(state.…)` for "greyed while this is true", and
        // `.disabled(!state.…)` for "live only while this is true" — both are
        // gates, and both have to be counted.
        if !line.contains(".disabled(state.") && !line.contains(".disabled(!state.") {
            continue;
        }
        let start = (0..=index)
            .rev()
            .find(|at| lines[*at].contains("MenuItem::action("))
            .unwrap_or(index);
        found.push(lines[start..=index].join(" "));
    }
    found
}
