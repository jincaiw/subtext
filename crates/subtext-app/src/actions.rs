//! Every command the application can perform.
//!
//! CotEditor drives its menu from `@objc` selectors on `NSResponder`. GPUI
//! has no responder chain, so each of those selectors — 155 of them, counted
//! out of the storyboard's 170 `<action>` elements — becomes a typed action
//! here and the menu, the key bindings and the toolbar all dispatch
//! through the same vocabulary. Grouping by namespace keeps the intent
//! readable: `text::ToggleComment`, `view::ToggleLineNumber`.
//!
//! A selector is not the same thing as an action, and this module is where the
//! two part company: one selector can stand for several actions when the item
//! carries a value upstream reads out of a tag, and an action can be one no
//! selector reaches at all — the menu bar builds it at runtime. What must never
//! happen is the third case: an action nothing can fire. The audit that removed
//! `file::OpenRecent` and `help::ReleaseNotes` is in `docs/PLAN.md` §8.9g.

use gpui_kit::actions;

pub mod app {
    use super::*;
    actions!(
        subtext,
        [
            ShowAboutPanel,
            ShowDonationWindow,
            ShowSettingsWindow,
            ShowWhatsNew,
            Hide,
            HideOtherApplications,
            UnhideAllApplications,
            Terminate,
            NoOp,
        ]
    );
}

pub mod file {
    use super::*;
    actions!(
        subtext,
        [
            NewDocument,
            NewDocumentAsWindow,
            NewDocumentAsTab,
            OpenDocument,
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
        ]
    );

    /// Open the document at this path.
    ///
    /// Upstream has no such selector. Its *Open Recent* submenu is marked
    /// `systemMenu="recentDocuments"` in the storyboard, which is AppKit's own
    /// hook: `NSDocumentController` inserts one item per recent document above
    /// `Clear Menu`, and each one fires `openDocument:` with the URL as its
    /// represented object. GPUI's menu items carry an action rather than a
    /// target and a tag, so the path travels in the action instead.
    ///
    /// The path rather than the row's position, for the reason
    /// [`script::RunScript`] gives: the list is reordered every time a document
    /// is opened, and an item run by position would open the wrong file.
    ///
    /// `no_json` because the action is built from the menu, never from a
    /// configuration file — the settings file names a recent document by its
    /// path, not by this action's JSON.
    #[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
    #[action(namespace = subtext, no_json)]
    pub struct OpenRecentDocument {
        pub path: String,
    }
}

pub mod edit {
    use super::*;
    actions!(
        subtext,
        [
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
        ]
    );
}

pub mod spelling {
    use super::*;
    actions!(
        subtext,
        [
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
        ]
    );
}

pub mod format {
    use super::*;
    actions!(
        subtext,
        [
            RecolorAll,
            CustomizeEncodings,
            MakeFontStandard,
            MakeFontMonospaced,
            BiggerFont,
            SmallerFont,
            ResetFont,
            OrderFrontFontPanel,
            ToggleAntialias,
            ToggleLigatures,
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
        ]
    );

    /// Set the frontmost document's line endings.
    ///
    /// Upstream has one selector for this — `changeLineEnding(_:)`, which reads
    /// the ending out of the menu item's tag. SubText's Format ▸ Line Endings
    /// submenu is built at runtime from `LineEnding::ALL`, so the ending has to
    /// travel with the action instead. The payload is the abbreviation the menu
    /// already shows — `"LF"`, `"CRLF"` — so the item and the command that runs
    /// it name the same thing.
    ///
    /// `no_json` for the same reason `InsertSnippet` has it: the action is built
    /// from the menu, never from a configuration file.
    #[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
    #[action(namespace = subtext, no_json)]
    pub struct ChangeLineEnding {
        pub ending: String,
    }

    /// Highlight the frontmost document as `name`.
    ///
    /// The syntax list is the user's, so the menu item cannot name the syntax at
    /// compile time. `"None"` is a value here rather than the absence of one —
    /// see [`crate::menus`].
    #[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
    #[action(namespace = subtext, no_json)]
    pub struct ChangeSyntax {
        pub name: String,
    }

    /// Apply the theme called `name`.
    ///
    /// The theme list is the user's, for the same reason.
    #[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
    #[action(namespace = subtext, no_json)]
    pub struct ChangeTheme {
        pub name: String,
    }

    /// Convert the frontmost document to the encoding `identifier` names.
    ///
    /// The payload is a [`FileEncoding`](subtext_core::FileEncoding) identifier
    /// — `"shift_jis"`, `"utf-8-bom"` — not an encoding's name and not its
    /// position in the list. Upstream has one selector for this,
    /// `changeEncoding(_:)`, which reads a `FileEncoding` out of the menu item;
    /// GPUI's menu items carry only an action, so the encoding travels inside
    /// it.
    ///
    /// The `-bom` suffix is the only thing that distinguishes the two UTF-8
    /// items, which is why the payload is the pair rather than the codec.
    #[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
    #[action(namespace = subtext, no_json)]
    pub struct ChangeEncoding {
        pub identifier: String,
    }
}

pub mod view {
    use super::*;
    actions!(
        subtext,
        [
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
        ]
    );
}

pub mod text {
    use super::*;
    actions!(
        subtext,
        [
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
            ShowSelectionInfo,
            ToggleAdvancedCounter,
            EditColorCode,
        ]
    );

    /// Normalize the selection in `form`.
    ///
    /// The Text ▸ Unicode Normalizations submenu is built at runtime from
    /// `Normalization::ALL`, so the form travels with the action. The payload is
    /// the form's [`identifier`](subtext_core::Normalization::identifier) — a
    /// name that does not move when the list above it does.
    #[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
    #[action(namespace = subtext, no_json)]
    pub struct NormalizeUnicode {
        pub form: String,
    }
}

pub mod find {
    use super::*;
    actions!(
        subtext,
        [
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
        ]
    );
}

pub mod window {
    use super::*;
    actions!(
        subtext,
        [
            PerformMiniaturize,
            PerformZoom,
            ArrangeInFront,
            ShowConsolePanel,
        ]
    );
}

pub mod help {
    use super::*;
    actions!(
        subtext,
        [
            ShowHelp,
            OpenHelpAnchor,
            OpenAppleScriptDictionary,
            OpenWebSite,
            ReportBug,
            CreateBugReport,
        ]
    );
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

pub mod script {
    use super::*;
    actions!(subtext, [OpenScriptsFolder,]);

    /// Run the script at this path over the document in front.
    ///
    /// Scripts are the user's files, so — like a snippet — the menu item cannot
    /// name its action at compile time. The payload is the script's *path*
    /// rather than its position in the menu: the folder can be rearranged
    /// between the item being built and being chosen, and a script run by
    /// position would then be the wrong one.
    ///
    /// A path rather than a name, because two folders may hold a script of the
    /// same name and they are different scripts.
    #[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
    #[action(namespace = subtext, no_json)]
    pub struct RunScript {
        pub path: String,
    }
}
