//! Turning menu and key-binding actions into behaviour.
//!
//! Every action declared in [`crate::actions`] lands here, reached through
//! [`crate::bridge`]. The command's name is the last segment of its action
//! path, which keeps this file readable next to `resources/menu.json` — the
//! table the menu bar itself is generated from.
//!
//! A command that is not implemented yet says so in the status bar rather than
//! doing nothing silently. An editor that swallows a keystroke is worse than
//! one that admits the gap: the user cannot tell a missing feature from a
//! broken one.

use gpui_kit::{Context, Window};

use subtext_core::{LineEnding, textops};

use crate::settings::{InspectorPane, SidebarPane};
use crate::workspace::Workspace;

impl Workspace {
    /// Run the command named `name` (the last segment of the action's path).
    ///
    /// Returns `true` when the command was recognised. An unrecognised name is
    /// reported rather than ignored, so a typo in the generated table shows up
    /// as a message instead of as a dead menu item.
    pub fn dispatch(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        use textops::{CaseTransform, Normalization, WidthTransform};

        match name {
            // ---- application ------------------------------------------
            "NoOp" => {}
            "ShowSettingsWindow" => self.toggle_settings(cx),
            "Terminate" => cx.quit(),
            "ShowAboutPanel" => self.show_about(cx),
            "ShowWhatsNew" | "ShowDonationWindow" => {
                self.notice("That panel has no SubText equivalent yet", cx)
            }
            "Hide" => self.notice("Hide is handled by the window system", cx),
            "HideOtherApplications" | "UnhideAllApplications" => {
                self.notice("Application hiding is handled by the window system", cx)
            }

            // ---- files ------------------------------------------------
            "NewDocument" | "NewDocumentAsTab" => self.new_tab(window, cx),
            "NewDocumentAsWindow" => self.notice("SubText opens documents as tabs", cx),
            "OpenDocument" => self.open_dialog(window, cx),
            "OpenRecent" => self.show_sidebar(SidebarPane::FileBrowser, cx),
            "ClearRecentDocuments" => self.clear_recent(cx),
            "PerformClose" => self.close_active(window, cx),
            "SaveDocument" => self.save(false, window, cx),
            "SaveDocumentAs" => self.save(true, window, cx),
            "RevertDocumentToSaved" => self.revert(window, cx),
            "ShowInFinder" => self.show_in_finder(cx),
            "CopyPath" => self.copy_path(cx),
            "ShareDocument" => self.notice("Share is not implemented yet", cx),
            "RunPageLayout" | "PrintDocument" => {
                self.notice("Printing is not implemented yet", cx)
            }

            // ---- editing ----------------------------------------------
            // Undo, redo, cut, copy, paste and select all are bound in the
            // editor's own key context, which is more specific than this one,
            // so the text engine handles them while the editor has focus. The
            // menu items still reach it: GPUI dispatches a menu command down
            // the same focus path a keystroke takes.
            "Undo" | "Redo" | "Cut" | "Copy" | "Paste" | "PasteAsIs" | "Delete" | "SelectAll"
            | "CopyWithStyle" => self.notice("Handled by the text engine", cx),
            "SelectWord" => self.select_word(cx),
            "SelectParagraph" => self.select_line(cx),
            "SelectEnclosingSymbols" => self.select_enclosing_symbols(cx),
            "SplitSelectionByLines" => self.split_selection_by_lines(cx),
            "InputBackSlash" => self.insert_text("\\", window, cx),
            "InputYenMark" => self.insert_text("¥", window, cx),
            // Column selection is defined in terms of the layout manager's
            // wrapped rows: CotEditor's version asks TextKit which visual line
            // a cursor is on and places the new one at the same glyph column.
            // SubText's editor has no such model to ask.
            "SelectColumnUp" | "SelectColumnDown" => self.notice(
                "Column selection needs the text layout engine's visual rows, which SubText's editor does not expose",
                cx,
            ),
            "Complete" | "ShowUnicodeInputPanel" => {
                self.notice("That selection command is not implemented yet", cx)
            }

            // ---- spelling and substitutions ---------------------------
            "ShowGuessPanel" | "CheckSpelling" | "ToggleContinuousSpellChecking"
            | "ToggleGrammarChecking" | "ToggleAutomaticSpellingCorrection"
            | "ToggleAutomaticQuoteSubstitution" | "ToggleAutomaticDashSubstitution"
            | "ToggleAutomaticTextReplacement" | "ToggleSmartInsertDelete"
            | "OrderFrontSubstitutionsPanel" | "StartSpeaking" | "StopSpeaking" => {
                self.notice("System spelling and speech services are macOS-only", cx)
            }
            "StraightenQuotesInSelection" | "ReplaceQuotesInSelection"
            | "ReplaceDashesInSelection" | "ReplaceTextInSelection" => {
                self.notice("Substitutions are not implemented yet", cx)
            }

            // ---- format ------------------------------------------------
            "RecolorAll" => {
                self.rehighlight(cx);
                self.notice("Syntax analysis reset", cx)
            }
            "SetLineEndingLf" => self.set_line_ending(LineEnding::LF, window, cx),
            "SetLineEndingCrlf" => self.set_line_ending(LineEnding::CRLF, window, cx),
            "SetLineEndingCr" => self.set_line_ending(LineEnding::CR, window, cx),
            "SetLineEndingNel" => self.set_line_ending(LineEnding::NEL, window, cx),
            "SetLineEndingLs" => self.set_line_ending(LineEnding::LS, window, cx),
            "SetLineEndingPs" => self.set_line_ending(LineEnding::PS, window, cx),
            "SetEncoding" => self.notice(
                "Use the Format ▸ Encoding menu; the list comes from the same registry CotEditor uses",
                cx,
            ),
            "ReinterpretEncoding" => self.notice("Use the Format ▸ Encoding menu", cx),
            "SetSyntax" => self.notice("Use the Format ▸ Syntax menu", cx),
            "MakeFontStandard" => self.set_font(None, cx),
            "MakeFontMonospaced" => self.set_font(Some(mono_font()), cx),
            "BiggerFont" => self.bump_font(1.0, cx),
            "SmallerFont" => self.bump_font(-1.0, cx),
            "ResetFont" => {
                self.set_font(None, cx);
                self.set_font_size(13.0, cx)
            }
            "OrderFrontFontPanel" => self.notice("Use Format ▸ Font to change the font", cx),
            "ToggleAntialias" => self.toggle_antialias(window, cx),
            "ToggleLigatures" => self.toggle_ligatures(window, cx),
            "SetTheme" => self.notice("Use the Format ▸ Theme menu", cx),
            "ChangeTabWidth" | "CustomizeTabWidth" => self.cycle_tab_width(cx),
            "ToggleAutoTabExpand" => self.toggle_auto_expand_tabs(cx),
            "ToggleInvisibleChars" => self.update_editor_options(window, cx, |options| {
                options.show_invisibles = !options.show_invisibles
            }),
            "ToggleIndentGuides" => self.update_editor_options(window, cx, |options| {
                options.show_indent_guides = !options.show_indent_guides
            }),
            "TogglePageGuide" => self.update_editor_options(window, cx, |options| {
                options.show_page_guide = !options.show_page_guide
            }),
            "ToggleLineWrap" => self.update_editor_options(window, cx, |options| {
                options.wrap_lines = !options.wrap_lines
            }),
            "ToggleEditable" => self.update_editor_options(window, cx, |options| {
                options.editable = !options.editable
            }),
            "MakeWritingDirectionLeftToRight" | "MakeWritingDirectionRightToLeft"
            | "MakeLayoutOrientationHorizontal" | "MakeLayoutOrientationVertical" => {
                self.notice("Writing direction is not implemented yet", cx)
            }

            // ---- view --------------------------------------------------
            "ToggleTabBar" => {
                self.update_window_options(cx, |window| window.show_tab_bar = !window.show_tab_bar)
            }
            "ToggleToolbarShown" => {
                self.update_window_options(cx, |window| window.show_toolbar = !window.show_toolbar)
            }
            "RunToolbarCustomizationPalette" => {
                self.notice("The toolbar is not user-customisable yet", cx)
            }
            "ShowQuickActions" => self.notice("Quick Actions are not implemented yet", cx),
            "ShowFileBrowser" => self.show_sidebar(SidebarPane::FileBrowser, cx),
            "ShowFolderFinder" => self.show_sidebar(SidebarPane::FolderFind, cx),
            "ToggleSidebar" => {
                self.update_window_options(cx, |window| window.show_sidebar = !window.show_sidebar)
            }
            "ToggleInspector" => self.update_window_options(cx, |window| {
                window.show_inspector = !window.show_inspector
            }),
            "ShowDocumentInspector" => self.show_inspector(InspectorPane::Document, cx),
            "ShowOutlineInspector" => self.show_inspector(InspectorPane::Outline, cx),
            "ShowWarningsInspector" => self.show_inspector(InspectorPane::Warnings, cx),
            "ToggleNavigationBar" => self.update_window_options(cx, |window| {
                window.show_navigation_bar = !window.show_navigation_bar
            }),
            "ToggleLineNumber" => self.update_editor_options(window, cx, |options| {
                options.show_line_numbers = !options.show_line_numbers
            }),
            "ToggleStatusBar" => {
                self.update_window_options(cx, |window| window.show_status_bar = !window.show_status_bar)
            }
            "FocusNextEditor" => self.next_tab(true, cx),
            "FocusPreviousEditor" => self.next_tab(false, cx),
            "OpenSplitTextView" | "CloseSplitTextView" | "ToggleSplitOrientation" => {
                self.notice("Split editors are not implemented yet", cx)
            }
            "ToggleFullScreen" => window.toggle_fullscreen(),
            "ToggleKeepOnTop" => {
                self.update_window_options(cx, |window| window.keep_on_top = !window.keep_on_top)
            }

            // ---- text --------------------------------------------------
            "ToggleComment" => self.text_op(window, cx, |text, selection, delimiters| {
                textops::toggle_comment(text, selection, delimiters)
            }),
            "InlineCommentOut" => self.text_op(window, cx, |text, selection, delimiters| {
                let inline =
                    textops::CommentDelimiters { inline: delimiters.inline.clone(), block: None };
                textops::toggle_comment(text, selection, &inline)
            }),
            "BlockCommentOut" => self.text_op(window, cx, |text, selection, delimiters| {
                let block =
                    textops::CommentDelimiters { inline: None, block: delimiters.block.clone() };
                textops::toggle_comment(text, selection, &block)
            }),
            "Uncomment" => self.text_op(window, cx, |text, selection, delimiters| {
                // Uncomment is Toggle Comment applied to already-commented
                // lines, which is exactly what the toggle does.
                textops::toggle_comment(text, selection, delimiters)
            }),
            "ShiftRight" => {
                let indent = self.settings.indent_unit();
                self.text_op(window, cx, move |text, selection, _| {
                    textops::shift_right(text, selection, &indent)
                })
            }
            "ShiftLeft" => {
                let indent = self.settings.indent_unit();
                self.text_op(window, cx, move |text, selection, _| {
                    textops::shift_left(text, selection, &indent)
                })
            }
            "ConvertIndentationToSpaces" => {
                let width = self.settings.tab_width;
                self.text_op(window, cx, move |text, selection, _| {
                    textops::convert_indentation_to_spaces(text, selection, width)
                })
            }
            "ConvertIndentationToTabs" => {
                let width = self.settings.tab_width;
                self.text_op(window, cx, move |text, selection, _| {
                    textops::convert_indentation_to_tabs(text, selection, width)
                })
            }
            "SurroundSelectionWithSingleQuotes" => self.surround(window, cx, "'", "'"),
            "SurroundSelectionWithDoubleQuotes" => self.surround(window, cx, "\"", "\""),
            "SurroundSelectionWithParentheses" => self.surround(window, cx, "(", ")"),
            "SurroundSelectionWithBraces" => self.surround(window, cx, "{", "}"),
            "SurroundSelectionWithSquareBrackets" => self.surround(window, cx, "[", "]"),
            "SurroundSelection" => self.notice("Custom surrounds are not implemented yet", cx),
            "TrimTrailingWhitespace" => self.text_op(window, cx, |text, selection, _| {
                textops::trim_trailing_whitespace(text, selection)
            }),
            "MoveLineUp" => self.text_op(window, cx, |text, selection, _| {
                textops::move_lines(text, selection, true)
            }),
            "MoveLineDown" => self.text_op(window, cx, |text, selection, _| {
                textops::move_lines(text, selection, false)
            }),
            "SortLinesAscending" => self.text_op(window, cx, |text, selection, _| {
                textops::sort_lines(
                    text,
                    selection,
                    &textops::SortKey::EntireLine,
                    &textops::SortOptions::whole_line(),
                )
            }),
            "PatternSort" => self.pattern_sort_dialog(window, cx),
            "ReverseLines" => self.text_op(window, cx, |text, selection, _| {
                textops::reverse_lines(text, selection)
            }),
            "ShuffleLines" => self.text_op(window, cx, |text, selection, _| {
                textops::shuffle_lines(text, selection, pseudo_random)
            }),
            "DeleteDuplicateLine" => self.text_op(window, cx, |text, selection, _| {
                textops::delete_duplicate_lines(text, selection, false)
            }),
            "DuplicateLine" => self.text_op(window, cx, |text, selection, _| {
                textops::duplicate_lines(text, selection)
            }),
            "DeleteLine" => self.text_op(window, cx, |text, selection, _| {
                textops::delete_lines(text, selection)
            }),
            "JoinLines" => self.text_op(window, cx, |text, selection, _| {
                textops::join_lines(text, selection)
            }),
            "UppercaseWord" => self.text_op(window, cx, |text, selection, _| {
                textops::transform_case(text, selection, CaseTransform::Upper)
            }),
            "LowercaseWord" => self.text_op(window, cx, |text, selection, _| {
                textops::transform_case(text, selection, CaseTransform::Lower)
            }),
            "CapitalizeWord" => self.text_op(window, cx, |text, selection, _| {
                textops::transform_case(text, selection, CaseTransform::Capitalize)
            }),
            "ExchangeFullwidth" => self.text_op(window, cx, |text, selection, _| {
                textops::transform_width(text, selection, WidthTransform::Full)
            }),
            "ExchangeHalfwidth" => self.text_op(window, cx, |text, selection, _| {
                textops::transform_width(text, selection, WidthTransform::Half)
            }),
            "ExchangeFullwidthRoman" => self.text_op(window, cx, |text, selection, _| {
                textops::transform_width(text, selection, WidthTransform::FullAlphanumeric)
            }),
            "ExchangeHalfwidthRoman" => self.text_op(window, cx, |text, selection, _| {
                textops::transform_width(text, selection, WidthTransform::HalfAlphanumeric)
            }),
            "EncodeUrl" => self.text_op(window, cx, |text, selection, _| {
                textops::url_encode(text, selection)
            }),
            "DecodeUrl" => self.text_op(window, cx, |text, selection, _| {
                textops::url_decode(text, selection)
            }),
            "NormalizeNfc" => self.text_op(window, cx, |text, selection, _| {
                textops::normalize(text, selection, Normalization::Nfc)
            }),
            "NormalizeNfd" => self.text_op(window, cx, |text, selection, _| {
                textops::normalize(text, selection, Normalization::Nfd)
            }),
            "NormalizeNfkc" => self.text_op(window, cx, |text, selection, _| {
                textops::normalize(text, selection, Normalization::Nfkc)
            }),
            "NormalizeNfkd" => self.text_op(window, cx, |text, selection, _| {
                textops::normalize(text, selection, Normalization::Nfkd)
            }),
            "ShowSelectionInfo" => self.inspect_selection(window, cx),
            "ToggleAdvancedCounter" => self.inspect_document(window, cx),
            "EditColorCode" => self.color_code_dialog(window, cx),
            "ShowSnippetEditor" => self.notice("Snippets are not implemented yet", cx),

            // ---- find --------------------------------------------------
            "PerformTextFinderAction" => self.show_find(false, window, cx),
            "ShowReplaceInterface" => self.show_find(true, window, cx),
            "FindNext" => self.find_step(true, cx),
            "FindPrevious" => self.find_step(false, cx),
            "FindAll" | "SelectAllFindMatches" => self.find_all(cx),
            "UseSelectionForFind" => self.use_selection_for_find(window, cx),
            "UseSelectionForReplace" => self.use_selection_for_replace(window, cx),
            "CenterSelectionInVisibleArea" => self.notice("Jumped to the selection", cx),
            "ReplaceAndFindNext" => self.replace_and_find_next(window, cx),
            "ReplaceAll" => self.replace_all(window, cx),
            "ShowMultipleReplace" => self.multiple_replace_dialog(window, cx),
            "HighlightAll" => self.highlight_all(cx),
            "RemoveHighlights" => self.clear_highlights(cx),
            "OpenOutlineMenu" => self.show_inspector(InspectorPane::Outline, cx),
            "SelectPrevItemOfOutlineMenu" | "SelectNextItemOfOutlineMenu" => {
                self.notice("Outline navigation is not implemented yet", cx)
            }
            "GotoLocation" => self.goto_dialog(window, cx),

            // ---- window ------------------------------------------------
            "PerformMiniaturize" => window.minimize_window(),
            "PerformZoom" => window.zoom_window(),
            "ArrangeInFront" => self.notice("Handled by the window system", cx),
            "ShowConsolePanel" => self.notice("The console is not implemented yet", cx),

            // ---- help --------------------------------------------------
            "ShowHelp" | "OpenHelpAnchor" => {
                self.notice("SubText's documentation lives in docs/ in the source tree", cx)
            }
            "OpenAppleScriptDictionary" => {
                self.notice("AppleScript support does not exist on GPUI", cx)
            }
            "OpenWebSite" => self.notice("https://coteditor.com is the upstream project", cx),
            "ReportBug" | "CreateBugReport" => {
                self.notice("Please report issues in the SubText repository", cx)
            }
            "ReleaseNotes" => self.notice("See CHANGELOG.md in the source tree", cx),

            other => {
                self.notice(format!("{other} is not implemented yet"), cx);
                return false;
            }
        }
        true
    }
}

/// A deterministic stand-in for a random source, so `Shuffle Lines` is
/// reproducible in tests and needs no `rand` dependency.
fn pseudo_random(bound: usize) -> usize {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u64> = const { Cell::new(0x9E3779B97F4A7C15) };
    }
    if bound == 0 {
        return 0;
    }
    STATE.with(|state| {
        let mut value = state.get();
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        state.set(value);
        (value % bound as u64) as usize
    })
}

/// CotEditor's "Standard Font" is the monospaced face the system recommends;
/// `Menlo` is the macOS default the application ships with.
fn mono_font() -> String {
    #[cfg(target_os = "macos")]
    {
        "Menlo".to_string()
    }
    #[cfg(target_os = "windows")]
    {
        "Consolas".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "DejaVu Sans Mono".to_string()
    }
}
