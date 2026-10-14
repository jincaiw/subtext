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

use gpui_kit::{App, BorrowAppContext as _, Global, WeakEntity};

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

/// Every workspace this process has opened, newest last.
///
/// A `Global` rather than a field on some view, because a menu command is
/// dispatched to the application, not to a window: the handler has to be able
/// to find the views from the `App` alone.
#[derive(Default)]
struct Workspaces(Vec<WeakEntity<Workspace>>);

impl Global for Workspaces {}

/// Record a workspace so the global handlers can reach it.
pub fn register(workspace: WeakEntity<Workspace>, cx: &mut App) {
    if !cx.has_global::<Workspaces>() {
        cx.set_global(Workspaces::default());
    }
    cx.update_global::<Workspaces, _>(|registry, _| {
        registry.0.retain(|existing| existing.entity_id() != workspace.entity_id());
        registry.0.push(workspace);
    });
}

/// Run `name` in the active window's workspace.
///
/// The command's name is the last segment of its action path, which is the
/// vocabulary [`Workspace::dispatch`] speaks.
pub fn dispatch(name: &'static str, cx: &mut App) {
    let Some(window) = cx.active_window() else {
        return;
    };
    let workspaces: Vec<WeakEntity<Workspace>> = cx
        .try_global::<Workspaces>()
        .map(|registry| registry.0.clone())
        .unwrap_or_default();

    for workspace in workspaces {
        let handled = window.update(cx, |_, window, cx| {
            workspace
                .update(cx, |workspace, cx| workspace.dispatch(name, window, cx))
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
        actions::file::OpenRecent => "OpenRecent",
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
        actions::format::SetLineEndingLf => "SetLineEndingLf",
        actions::format::SetLineEndingCrlf => "SetLineEndingCrlf",
        actions::format::SetLineEndingCr => "SetLineEndingCr",
        actions::format::SetLineEndingNel => "SetLineEndingNel",
        actions::format::SetLineEndingLs => "SetLineEndingLs",
        actions::format::SetLineEndingPs => "SetLineEndingPs",
        actions::format::SetEncoding => "SetEncoding",
        actions::format::ReinterpretEncoding => "ReinterpretEncoding",
        actions::format::SetSyntax => "SetSyntax",
        actions::format::MakeFontStandard => "MakeFontStandard",
        actions::format::MakeFontMonospaced => "MakeFontMonospaced",
        actions::format::BiggerFont => "BiggerFont",
        actions::format::SmallerFont => "SmallerFont",
        actions::format::ResetFont => "ResetFont",
        actions::format::OrderFrontFontPanel => "OrderFrontFontPanel",
        actions::format::ToggleAntialias => "ToggleAntialias",
        actions::format::ToggleLigatures => "ToggleLigatures",
        actions::format::SetTheme => "SetTheme",
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
        actions::text::NormalizeNfc => "NormalizeNfc",
        actions::text::NormalizeNfd => "NormalizeNfd",
        actions::text::NormalizeNfkc => "NormalizeNfkc",
        actions::text::NormalizeNfkd => "NormalizeNfkd",
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
        actions::help::ReleaseNotes => "ReleaseNotes",
    };
}

