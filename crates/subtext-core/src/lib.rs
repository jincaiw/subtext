//! # subtext-core
//!
//! The parts of a text editor that have nothing to do with a window: the
//! document model, text encodings, line endings, the operations behind the
//! Text menu, find and replace, and character inspection.
//!
//! This crate has no UI dependencies on purpose, so it can be unit-tested
//! without a window server and reused by the command-line tool.

pub mod charinfo;
pub mod colorcode;
pub mod completion;
pub mod document;
pub mod encoding;
pub mod fileattrs;
pub mod filedrop;
pub mod finder;
pub mod findresult;
pub mod folderfind;
pub mod imagesize;
pub mod issuereport;
pub mod line_ending;
pub mod matcher;
pub mod multireplace;
pub mod richtext;
pub mod scripts;
pub mod snippets;
pub mod textops;
pub mod unicode_tables;
pub mod warnings;

pub use charinfo::{
    CharacterCount, CharacterInfo, CharacterReport, Count, GeneralCategory, ScalarInfo,
};
pub use colorcode::{Color, ColorCodeType};
pub use completion::{
    CompletionSources, candidates, partial_word_range, should_complete_automatically,
};
pub use document::{Document, DocumentError, FileStatus};
pub use encoding::{Bom, DecodedFile, Detection, FileEncoding, TextEncoding};
pub use fileattrs::{FileAttributes, FilePermissions, format_timestamp};
pub use filedrop::{DroppedFile, FileDropItem, Variable as FileDropVariable};
pub use finder::{FindOptions, ReplacementSyntax, Searcher, replace_all, replace_one};
pub use findresult::{
    MatchLine, match_lines, match_lines_with_captures, truncate_head, truncate_head_with_ranges,
};
pub use folderfind::{
    Comparison, Conjunction, FileResult, FileScope, FileScopeError, Match, Metrics, Mode,
    Options as FolderFindOptions, Progress, Query, Search, SearchError, Summary, Target,
};
pub use line_ending::LineEnding;
pub use matcher::{AbbreviatedMatch, abbreviated_match, abbreviated_matched_ranges};
pub use multireplace::{MultipleReplace, Outcome, PatternError, Replacement, Settings};
pub use scripts::{
    InputType, Invocation, Item as ScriptItem, Launch, OutputType, Script, ScriptKind,
};
pub use snippets::{
    CURSOR_TOKEN, Insertion, Rejection, SELECTION_TOKEN, Sections, Shortcut, Snippet, TOKENS,
    Variable as SnippetVariable, indent_at, indent_range_at,
};
pub use textops::{
    CaseTransform, CommentDelimiters, Edit, LogicalLine, Normalization, SortKey, SortKeyError,
    SortOptions, SymbolPair, WidthTransform,
};
pub use warnings::{IncompatibleCharacter, incompatible_characters, inconsistent_line_endings};
