//! Every command the application can perform.
//!
//! CotEditor drives its menu from `@objc` selectors on `NSResponder`. GPUI
//! has no responder chain, so each of those 153 selectors becomes a typed
//! action here and the menu, the key bindings and the toolbar all dispatch
//! through the same vocabulary. Grouping by namespace keeps the intent
//! readable: `text::ToggleComment`, `view::ToggleLineNumber`.

use gpui_kit::actions;

pub mod app {
    use super::*;
    actions!(subtext, [
        ShowAboutPanel,
        ShowDonationWindow,
        ShowSettingsWindow,
        ShowWhatsNew,
        Hide,
        HideOtherApplications,
        UnhideAllApplications,
        Terminate,
        NoOp,
    ]);
}

pub mod file {
    use super::*;
    actions!(subtext, [
        NewDocument,
        NewDocumentAsWindow,
        NewDocumentAsTab,
        OpenDocument,
        OpenRecent,
        ClearRecentDocuments,
        PerformClose,
        SaveDocument,
        SaveDocumentAs,
        RevertDocumentToSaved,
        ShowInFinder,
        CopyPath,
        ShareDocument,
        RunPageLayout,
        PrintDocument,
    ]);
}

pub mod edit {
    use super::*;
    actions!(subtext, [
        Undo,
        Redo,
        Cut,
        Copy,
        CopyWithStyle,
        Paste,
        PasteAsIs,
        Delete,
        SelectAll,
        SelectParagraph,
        SelectWord,
        SelectEnclosingSymbols,
        SelectColumnUp,
        SelectColumnDown,
        SplitSelectionByLines,
        Complete,
        InputBackSlash,
        InputYenMark,
        ShowUnicodeInputPanel,
    ]);
}

pub mod spelling {
    use super::*;
    actions!(subtext, [
        ShowGuessPanel,
        CheckSpelling,
        ToggleContinuousSpellChecking,
        ToggleGrammarChecking,
        ToggleAutomaticSpellingCorrection,
        ToggleAutomaticQuoteSubstitution,
        ToggleAutomaticDashSubstitution,
        ToggleAutomaticTextReplacement,
        ToggleSmartInsertDelete,
        OrderFrontSubstitutionsPanel,
        StraightenQuotesInSelection,
        ReplaceQuotesInSelection,
        ReplaceDashesInSelection,
        ReplaceTextInSelection,
        StartSpeaking,
        StopSpeaking,
    ]);
}

pub mod format {
    use super::*;
    actions!(subtext, [
        RecolorAll,
        SetLineEndingLf,
        SetLineEndingCrlf,
        SetLineEndingCr,
        SetLineEndingNel,
        SetLineEndingLs,
        SetLineEndingPs,
        SetEncoding,
        ReinterpretEncoding,
        SetSyntax,
        MakeFontStandard,
        MakeFontMonospaced,
        BiggerFont,
        SmallerFont,
        ResetFont,
        OrderFrontFontPanel,
        ToggleAntialias,
        ToggleLigatures,
        SetTheme,
        ChangeTabWidth,
        CustomizeTabWidth,
        ToggleAutoTabExpand,
        ToggleInvisibleChars,
        ToggleIndentGuides,
        TogglePageGuide,
        MakeWritingDirectionLeftToRight,
        MakeWritingDirectionRightToLeft,
        MakeLayoutOrientationHorizontal,
        MakeLayoutOrientationVertical,
        ToggleLineWrap,
        ToggleEditable,
    ]);
}

pub mod view {
    use super::*;
    actions!(subtext, [
        ToggleTabBar,
        ToggleToolbarShown,
        RunToolbarCustomizationPalette,
        ShowQuickActions,
        ShowFileBrowser,
        ShowFolderFinder,
        ToggleSidebar,
        ToggleInspector,
        ShowDocumentInspector,
        ShowOutlineInspector,
        ShowWarningsInspector,
        ToggleNavigationBar,
        ToggleLineNumber,
        ToggleStatusBar,
        FocusNextEditor,
        FocusPreviousEditor,
        OpenSplitTextView,
        CloseSplitTextView,
        ToggleSplitOrientation,
        ToggleFullScreen,
        ToggleKeepOnTop,
    ]);
}

pub mod text {
    use super::*;
    actions!(subtext, [
        ShowSnippetEditor,
        ToggleComment,
        InlineCommentOut,
        BlockCommentOut,
        Uncomment,
        ShiftRight,
        ShiftLeft,
        ConvertIndentationToSpaces,
        ConvertIndentationToTabs,
        SurroundSelectionWithSingleQuotes,
        SurroundSelectionWithDoubleQuotes,
        SurroundSelectionWithParentheses,
        SurroundSelectionWithBraces,
        SurroundSelectionWithSquareBrackets,
        SurroundSelection,
        TrimTrailingWhitespace,
        MoveLineUp,
        MoveLineDown,
        PatternSort,
        SortLinesAscending,
        ReverseLines,
        ShuffleLines,
        DeleteDuplicateLine,
        DuplicateLine,
        DeleteLine,
        JoinLines,
        UppercaseWord,
        LowercaseWord,
        CapitalizeWord,
        ExchangeFullwidth,
        ExchangeHalfwidth,
        ExchangeFullwidthRoman,
        ExchangeHalfwidthRoman,
        EncodeUrl,
        DecodeUrl,
        NormalizeNfc,
        NormalizeNfd,
        NormalizeNfkc,
        NormalizeNfkd,
        ShowSelectionInfo,
        ToggleAdvancedCounter,
        EditColorCode,
    ]);
}

pub mod find {
    use super::*;
    actions!(subtext, [
        PerformTextFinderAction,
        ShowReplaceInterface,
        FindNext,
        FindPrevious,
        FindAll,
        SelectAllFindMatches,
        UseSelectionForFind,
        UseSelectionForReplace,
        CenterSelectionInVisibleArea,
        ReplaceAndFindNext,
        ReplaceAll,
        ShowMultipleReplace,
        HighlightAll,
        RemoveHighlights,
        OpenOutlineMenu,
        SelectPrevItemOfOutlineMenu,
        SelectNextItemOfOutlineMenu,
        GotoLocation,
    ]);
}

pub mod window {
    use super::*;
    actions!(subtext, [
        PerformMiniaturize,
        PerformZoom,
        ArrangeInFront,
        ShowConsolePanel,
    ]);
}

pub mod help {
    use super::*;
    actions!(subtext, [
        ShowHelp,
        OpenHelpAnchor,
        OpenAppleScriptDictionary,
        OpenWebSite,
        ReportBug,
        CreateBugReport,
        ReleaseNotes,
    ]);
}

pub mod snippet {
    /// Run the snippet with this id.
    ///
    /// The one action here that carries a value, so it cannot come from
    /// [`actions!`](gpui_kit::actions) — that macro only makes unit structs.
    /// It has to carry one: snippets are the user's data, so their menu items
    /// cannot be named at compile time, and naming one by its *position* would
    /// make an item run the wrong snippet the moment a row is inserted above it
    /// in the settings table.
    ///
    /// `no_json` because the action is built from the menu and the keymap,
    /// never from a configuration file — the settings file names a snippet by
    /// the snippet's own id, not by this action's JSON.
    #[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
    #[action(namespace = subtext, no_json)]
    pub struct InsertSnippet {
        pub id: u64,
    }
}
