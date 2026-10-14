//! Wiring the typed actions to the workspace.
//!
//! CotEditor's menus and key equivalents reach `@objc` selectors on
//! `NSResponder`, and Cocoa finds the first responder that implements one.
//! GPUI has no responder chain, so SubText does the lookup itself: every
//! action declared in [`crate::actions`] is registered here as a global
//! handler, and each one asks the open workspaces to run the command.
//!
//! The registration is also what makes a menu item live. GPUI greys an item
//! out unless something can handle its action — `App::is_action_available`
//! checks the window's handlers and then the global ones — so a command with
//! no registration would show up as a dead menu entry rather than as a
//! command that reports its own absence.

use std::path::{Path, PathBuf};

use gpui_kit::{
    Action, AnyWindowHandle, App, BorrowAppContext as _, Context, Global, WeakEntity, Window,
};

use subtext_core::{FileEncoding, LineEnding, Normalization};

use crate::actions;
use crate::workspace::Workspace;

/// Register one global handler per action, each forwarding its own name.
///
/// The alternative — a match arm per action in one handler — would need a
/// `TypeId` for each, which the type system does not hand out from a value.
macro_rules! bind_actions {
    ($cx:expr, $($path:path => $name:literal),* $(,)?) => {
        $(
            $cx.on_action(move |_: &$path, cx: &mut App| dispatch($name, cx));
        )*
    };
}

/// Register one global handler per runtime action, each running through
/// [`perform`].
///
/// The action is cloned into the closure because it has to outlive the borrow
/// the handler is handed: [`tell`] hands the command to a workspace, which may
/// not be able to answer it until after the handler has returned.
macro_rules! bind_runtime_actions {
    ($cx:expr, $($path:path),* $(,)?) => {
        $(
            $cx.on_action(|action: &$path, cx: &mut App| {
                let action = action.clone();
                tell(cx, move |workspace, window, cx| {
                    perform(&action, workspace, window, cx);
                });
            });
        )*
    };
}

/// Every workspace this process has opened, newest last, each with the window
/// it lives in.
///
/// A `Global` rather than a field on some view, because a menu command is
/// dispatched to the application, not to a window: the handler has to be able
/// to find the views from the `App` alone.
///
/// The window is recorded here rather than looked up with `App::active_window`
/// because a command has to be answered by the window the user is editing in,
/// and the key window is not always that one: the settings window and the
/// Quick Actions panel are both separate windows that take the keyboard, and a
/// command run from either of them still belongs to the document behind it.
#[derive(Default)]
struct Workspaces(Vec<(WeakEntity<Workspace>, AnyWindowHandle)>);

impl Global for Workspaces {}

/// Record a workspace, and the window it lives in, so the global handlers can
/// reach it.
pub fn register(workspace: WeakEntity<Workspace>, window: AnyWindowHandle, cx: &mut App) {
    if !cx.has_global::<Workspaces>() {
        cx.set_global(Workspaces::default());
    }
    cx.update_global::<Workspaces, _>(|registry, _| {
        registry
            .0
            .retain(|(existing, _)| existing.entity_id() != workspace.entity_id());
        registry.0.push((workspace, window));
    });
}

/// Run `name` in the active window's workspace.
///
/// The command's name is the last segment of its action path, which is the
/// vocabulary [`Workspace::dispatch`] speaks.
pub fn dispatch(name: &'static str, cx: &mut App) {
    run(cx, move |workspace, window, cx| workspace.dispatch(name, window, cx));
}

/// The name a command is run by: the last segment of its action's path.
///
/// [`Workspace::dispatch`] speaks this vocabulary, and it is the string
/// `resources/menu.json` records beside each command — so a command that went
/// missing from either shows up as a message rather than as a dead menu item.
pub fn name_of(action: &dyn Action) -> &'static str {
    // A derived action's name is `namespace::Struct`, and the namespace is the
    // crate's, not the menu's.
    action.name().rsplit("::").next().unwrap_or_default()
}

/// Run a runtime item's action in `workspace`, and say whether it was one.
///
/// The items the menu bar builds at runtime carry the value they stand for
/// rather than a name, so they cannot be run the way the rest are. This is the
/// one place that knows both vocabularies: an abbreviation or a name nothing
/// recognises is dropped rather than reported, because the menu was built from
/// the same list this resolves against — the only way to reach one is a stale
/// menu, and a stale menu is replaced on the next refresh.
///
/// Both callers come here: the global handlers below, for a menu click, and
/// [`Workspace::perform`], for the Quick Actions bar — which holds the action a
/// menu item carries and has to run it in the window it was opened from.
pub fn perform(
    action: &dyn Action,
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> bool {
    let any = action.as_any();

    if let Some(action) = any.downcast_ref::<actions::snippet::InsertSnippet>() {
        workspace.insert_snippet(action.id, window, cx);
        return true;
    }
    if let Some(action) = any.downcast_ref::<actions::format::ChangeLineEnding>() {
        if let Some(ending) = LineEnding::from_abbreviation(&action.ending) {
            workspace.set_line_ending(ending, window, cx);
        }
        return true;
    }
    if let Some(action) = any.downcast_ref::<actions::format::ChangeSyntax>() {
        workspace.set_syntax(&action.name, window, cx);
        return true;
    }
    if let Some(action) = any.downcast_ref::<actions::format::ChangeTheme>() {
        workspace.apply_theme_by_name(&action.name, cx);
        return true;
    }
    if let Some(action) = any.downcast_ref::<actions::format::ChangeEncoding>() {
        if let Some(encoding) = FileEncoding::from_identifier(&action.identifier) {
            workspace.change_encoding(encoding, window, cx);
        }
        return true;
    }
    if let Some(action) = any.downcast_ref::<actions::text::NormalizeUnicode>() {
        if let Some(form) = Normalization::from_identifier(&action.form) {
            workspace.normalize_unicode(form, window, cx);
        }
        return true;
    }
    // A recent document's action carries its path, and the path is what the
    // item stands for: the list is reordered every time a document is opened,
    // so an item run by position would open the wrong file.
    if let Some(action) = any.downcast_ref::<actions::file::OpenRecentDocument>() {
        workspace.open_path(PathBuf::from(&action.path), window, cx);
        return true;
    }
    // A script's action carries its path, which is the script's identity: the
    // folder can be rearranged between the item being built and being chosen,
    // and two folders may hold a script of the same name.
    if let Some(action) = any.downcast_ref::<actions::script::RunScript>() {
        workspace.run_script(Path::new(&action.path), window, cx);
        return true;
    }

    false
}

/// Run `command` in the active window's workspace.
///
/// [`run`] without the "is it mine" question, for the commands reached through a
/// runtime menu item: the item exists because a workspace put it there, so the
/// workspace it reaches is the one that wants it.
fn tell(cx: &mut App, command: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)) {
    run(cx, move |workspace, window, cx| {
        command(workspace, window, cx);
        true
    });
}

/// Hand `command` to the first workspace that recognises it.
///
/// The windows are tried in the order they were opened, newest last, so the
/// oldest one that can handle the command wins — which for every command here
/// means the only window there is. A workspace that returns `false` says the
/// command is not its business, and the search goes on.
///
/// Each command runs against its own workspace's window, never the key window:
/// see [`Workspaces`].
fn run(
    cx: &mut App,
    command: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) -> bool,
) {
    let workspaces: Vec<(WeakEntity<Workspace>, AnyWindowHandle)> = cx
        .try_global::<Workspaces>()
        .map(|registry| registry.0.clone())
        .unwrap_or_default();

    for (workspace, window) in workspaces {
        let handled = window.update(cx, |_, window, cx| {
            workspace
                .update(cx, |workspace, cx| command(workspace, window, cx))
                .is_ok()
        });
        if matches!(handled, Ok(true)) {
            return;
        }
    }
}

/// Register every action's global handler.
pub fn install(cx: &mut App) {
    if !cx.has_global::<Workspaces>() {
        cx.set_global(Workspaces::default());
    }
    // The items the menu bar builds at runtime carry a value rather than a name,
    // so none of them fits `bind_actions!`: a snippet carries its id, the three
    // Format popups carry the setting they stand for, a normalization carries
    // its form, a recent document and a script each carry a path. Each is turned
    // back into the thing it names by `perform`.
    bind_runtime_actions! { cx,
        actions::snippet::InsertSnippet,
        actions::format::ChangeLineEnding,
        actions::format::ChangeSyntax,
        actions::format::ChangeTheme,
        actions::format::ChangeEncoding,
        actions::text::NormalizeUnicode,
        actions::file::OpenRecentDocument,
        actions::script::RunScript,
    };
    bind_actions! { cx,
        actions::app::ShowAboutPanel => "ShowAboutPanel",
        actions::app::ShowDonationWindow => "ShowDonationWindow",
        actions::app::ShowSettingsWindow => "ShowSettingsWindow",
        actions::app::ShowWhatsNew => "ShowWhatsNew",
        actions::app::Hide => "Hide",
        actions::app::HideOtherApplications => "HideOtherApplications",
        actions::app::UnhideAllApplications => "UnhideAllApplications",
        actions::app::Terminate => "Terminate",
        actions::app::NoOp => "NoOp",
        actions::file::NewDocument => "NewDocument",
        actions::file::NewDocumentAsWindow => "NewDocumentAsWindow",
        actions::file::NewDocumentAsTab => "NewDocumentAsTab",
        actions::file::OpenDocument => "OpenDocument",
        actions::file::ClearRecentDocuments => "ClearRecentDocuments",
        actions::file::PerformClose => "PerformClose",
        actions::file::SaveDocument => "SaveDocument",
        actions::file::SaveDocumentAs => "SaveDocumentAs",
        actions::file::RevertDocumentToSaved => "RevertDocumentToSaved",
        actions::file::ShowInFinder => "ShowInFinder",
        actions::file::CopyPath => "CopyPath",
        actions::file::ShareDocument => "ShareDocument",
        actions::file::RunPageLayout => "RunPageLayout",
        actions::file::PrintDocument => "PrintDocument",
        actions::edit::Undo => "Undo",
        actions::edit::Redo => "Redo",
        actions::edit::Cut => "Cut",
        actions::edit::Copy => "Copy",
        actions::edit::CopyWithStyle => "CopyWithStyle",
        actions::edit::Paste => "Paste",
        actions::edit::PasteAsIs => "PasteAsIs",
        actions::edit::Delete => "Delete",
        actions::edit::SelectAll => "SelectAll",
        actions::edit::SelectParagraph => "SelectParagraph",
        actions::edit::SelectWord => "SelectWord",
        actions::edit::SelectEnclosingSymbols => "SelectEnclosingSymbols",
        actions::edit::SelectColumnUp => "SelectColumnUp",
        actions::edit::SelectColumnDown => "SelectColumnDown",
        actions::edit::SplitSelectionByLines => "SplitSelectionByLines",
        actions::edit::Complete => "Complete",
        actions::edit::InputBackSlash => "InputBackSlash",
        actions::edit::InputYenMark => "InputYenMark",
        actions::edit::ShowUnicodeInputPanel => "ShowUnicodeInputPanel",
        actions::spelling::ShowGuessPanel => "ShowGuessPanel",
        actions::spelling::CheckSpelling => "CheckSpelling",
        actions::spelling::ToggleContinuousSpellChecking => "ToggleContinuousSpellChecking",
        actions::spelling::ToggleGrammarChecking => "ToggleGrammarChecking",
        actions::spelling::ToggleAutomaticSpellingCorrection => "ToggleAutomaticSpellingCorrection",
        actions::spelling::ToggleAutomaticQuoteSubstitution => "ToggleAutomaticQuoteSubstitution",
        actions::spelling::ToggleAutomaticDashSubstitution => "ToggleAutomaticDashSubstitution",
        actions::spelling::ToggleAutomaticTextReplacement => "ToggleAutomaticTextReplacement",
        actions::spelling::ToggleSmartInsertDelete => "ToggleSmartInsertDelete",
        actions::spelling::OrderFrontSubstitutionsPanel => "OrderFrontSubstitutionsPanel",
        actions::spelling::StraightenQuotesInSelection => "StraightenQuotesInSelection",
        actions::spelling::ReplaceQuotesInSelection => "ReplaceQuotesInSelection",
        actions::spelling::ReplaceDashesInSelection => "ReplaceDashesInSelection",
        actions::spelling::ReplaceTextInSelection => "ReplaceTextInSelection",
        actions::spelling::StartSpeaking => "StartSpeaking",
        actions::spelling::StopSpeaking => "StopSpeaking",
        actions::format::RecolorAll => "RecolorAll",
        actions::format::CustomizeEncodings => "CustomizeEncodings",
        actions::format::MakeFontStandard => "MakeFontStandard",
        actions::format::MakeFontMonospaced => "MakeFontMonospaced",
        actions::format::BiggerFont => "BiggerFont",
        actions::format::SmallerFont => "SmallerFont",
        actions::format::ResetFont => "ResetFont",
        actions::format::OrderFrontFontPanel => "OrderFrontFontPanel",
        actions::format::ToggleAntialias => "ToggleAntialias",
        actions::format::ToggleLigatures => "ToggleLigatures",
        actions::format::ChangeTabWidth => "ChangeTabWidth",
        actions::format::CustomizeTabWidth => "CustomizeTabWidth",
        actions::format::ToggleAutoTabExpand => "ToggleAutoTabExpand",
        actions::format::ToggleInvisibleChars => "ToggleInvisibleChars",
        actions::format::ToggleIndentGuides => "ToggleIndentGuides",
        actions::format::TogglePageGuide => "TogglePageGuide",
        actions::format::MakeWritingDirectionLeftToRight => "MakeWritingDirectionLeftToRight",
        actions::format::MakeWritingDirectionRightToLeft => "MakeWritingDirectionRightToLeft",
        actions::format::MakeLayoutOrientationHorizontal => "MakeLayoutOrientationHorizontal",
        actions::format::MakeLayoutOrientationVertical => "MakeLayoutOrientationVertical",
        actions::format::ToggleLineWrap => "ToggleLineWrap",
        actions::format::ToggleEditable => "ToggleEditable",
        actions::view::ToggleTabBar => "ToggleTabBar",
        actions::view::ToggleToolbarShown => "ToggleToolbarShown",
        actions::view::RunToolbarCustomizationPalette => "RunToolbarCustomizationPalette",
        actions::view::ShowQuickActions => "ShowQuickActions",
        actions::view::ShowFileBrowser => "ShowFileBrowser",
        actions::view::ShowFolderFinder => "ShowFolderFinder",
        actions::view::ToggleSidebar => "ToggleSidebar",
        actions::view::ToggleInspector => "ToggleInspector",
        actions::view::ShowDocumentInspector => "ShowDocumentInspector",
        actions::view::ShowOutlineInspector => "ShowOutlineInspector",
        actions::view::ShowWarningsInspector => "ShowWarningsInspector",
        actions::view::ToggleNavigationBar => "ToggleNavigationBar",
        actions::view::ToggleLineNumber => "ToggleLineNumber",
        actions::view::ToggleStatusBar => "ToggleStatusBar",
        actions::view::FocusNextEditor => "FocusNextEditor",
        actions::view::FocusPreviousEditor => "FocusPreviousEditor",
        actions::view::OpenSplitTextView => "OpenSplitTextView",
        actions::view::CloseSplitTextView => "CloseSplitTextView",
        actions::view::ToggleSplitOrientation => "ToggleSplitOrientation",
        actions::view::ToggleFullScreen => "ToggleFullScreen",
        actions::view::ToggleKeepOnTop => "ToggleKeepOnTop",
        actions::text::ShowSnippetEditor => "ShowSnippetEditor",
        actions::text::ToggleComment => "ToggleComment",
        actions::text::InlineCommentOut => "InlineCommentOut",
        actions::text::BlockCommentOut => "BlockCommentOut",
        actions::text::Uncomment => "Uncomment",
        actions::text::ShiftRight => "ShiftRight",
        actions::text::ShiftLeft => "ShiftLeft",
        actions::text::ConvertIndentationToSpaces => "ConvertIndentationToSpaces",
        actions::text::ConvertIndentationToTabs => "ConvertIndentationToTabs",
        actions::text::SurroundSelectionWithSingleQuotes => "SurroundSelectionWithSingleQuotes",
        actions::text::SurroundSelectionWithDoubleQuotes => "SurroundSelectionWithDoubleQuotes",
        actions::text::SurroundSelectionWithParentheses => "SurroundSelectionWithParentheses",
        actions::text::SurroundSelectionWithBraces => "SurroundSelectionWithBraces",
        actions::text::SurroundSelectionWithSquareBrackets => "SurroundSelectionWithSquareBrackets",
        actions::text::SurroundSelection => "SurroundSelection",
        actions::text::TrimTrailingWhitespace => "TrimTrailingWhitespace",
        actions::text::MoveLineUp => "MoveLineUp",
        actions::text::MoveLineDown => "MoveLineDown",
        actions::text::PatternSort => "PatternSort",
        actions::text::SortLinesAscending => "SortLinesAscending",
        actions::text::ReverseLines => "ReverseLines",
        actions::text::ShuffleLines => "ShuffleLines",
        actions::text::DeleteDuplicateLine => "DeleteDuplicateLine",
        actions::text::DuplicateLine => "DuplicateLine",
        actions::text::DeleteLine => "DeleteLine",
        actions::text::JoinLines => "JoinLines",
        actions::text::UppercaseWord => "UppercaseWord",
        actions::text::LowercaseWord => "LowercaseWord",
        actions::text::CapitalizeWord => "CapitalizeWord",
        actions::text::ExchangeFullwidth => "ExchangeFullwidth",
        actions::text::ExchangeHalfwidth => "ExchangeHalfwidth",
        actions::text::ExchangeFullwidthRoman => "ExchangeFullwidthRoman",
        actions::text::ExchangeHalfwidthRoman => "ExchangeHalfwidthRoman",
        actions::text::EncodeUrl => "EncodeUrl",
        actions::text::DecodeUrl => "DecodeUrl",
        actions::text::ShowSelectionInfo => "ShowSelectionInfo",
        actions::text::ToggleAdvancedCounter => "ToggleAdvancedCounter",
        actions::text::EditColorCode => "EditColorCode",
        actions::find::PerformTextFinderAction => "PerformTextFinderAction",
        actions::find::ShowReplaceInterface => "ShowReplaceInterface",
        actions::find::FindNext => "FindNext",
        actions::find::FindPrevious => "FindPrevious",
        actions::find::FindAll => "FindAll",
        actions::find::SelectAllFindMatches => "SelectAllFindMatches",
        actions::find::UseSelectionForFind => "UseSelectionForFind",
        actions::find::UseSelectionForReplace => "UseSelectionForReplace",
        actions::find::CenterSelectionInVisibleArea => "CenterSelectionInVisibleArea",
        actions::find::ReplaceAndFindNext => "ReplaceAndFindNext",
        actions::find::ReplaceAll => "ReplaceAll",
        actions::find::ShowMultipleReplace => "ShowMultipleReplace",
        actions::find::HighlightAll => "HighlightAll",
        actions::find::RemoveHighlights => "RemoveHighlights",
        actions::find::OpenOutlineMenu => "OpenOutlineMenu",
        actions::find::SelectPrevItemOfOutlineMenu => "SelectPrevItemOfOutlineMenu",
        actions::find::SelectNextItemOfOutlineMenu => "SelectNextItemOfOutlineMenu",
        actions::find::GotoLocation => "GotoLocation",
        actions::window::PerformMiniaturize => "PerformMiniaturize",
        actions::window::PerformZoom => "PerformZoom",
        actions::window::ArrangeInFront => "ArrangeInFront",
        actions::window::ShowConsolePanel => "ShowConsolePanel",
        actions::help::ShowHelp => "ShowHelp",
        actions::help::OpenHelpAnchor => "OpenHelpAnchor",
        actions::help::OpenAppleScriptDictionary => "OpenAppleScriptDictionary",
        actions::help::OpenWebSite => "OpenWebSite",
        actions::help::ReportBug => "ReportBug",
        actions::help::CreateBugReport => "CreateBugReport",
        actions::script::OpenScriptsFolder => "OpenScriptsFolder",
    };
}

