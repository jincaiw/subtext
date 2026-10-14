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
pub mod document;
pub mod encoding;
pub mod finder;
pub mod folderfind;
pub mod line_ending;
pub mod multireplace;
pub mod textops;

pub use charinfo::{CharacterCount, CharacterInfo};
pub use colorcode::{Color, ColorCodeType};
pub use document::{Document, DocumentError, FileStatus};
pub use encoding::{Bom, DecodedFile, Detection, TextEncoding};
pub use finder::{FindOptions, ReplacementSyntax, Searcher, replace_all, replace_one};
pub use folderfind::{
    Comparison, Conjunction, FileResult, FileScope, FileScopeError, Match, Metrics, Mode,
    Options as FolderFindOptions, Progress, Query, Search, SearchError, Summary, Target,
};
pub use line_ending::LineEnding;
pub use multireplace::{MultipleReplace, Outcome, PatternError, Replacement, Settings};
pub use textops::{
    CaseTransform, CommentDelimiters, Edit, LogicalLine, Normalization, SortKey, SortKeyError,
    SortOptions, SymbolPair, WidthTransform,
};
