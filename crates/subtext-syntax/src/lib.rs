//! # subtext-syntax
//!
//! CotEditor's syntax definitions are `.cotsyntax` bundles — directories of
//! small JSON files. This crate reads them, picks the right one for a file,
//! and runs their highlighting rules over a buffer.
//!
//! SubText deliberately keeps CotEditor's own rule format instead of mapping
//! the definitions onto a tree-sitter grammar: the `.cotsyntax` files are the
//! source of truth for how a language is highlighted, and a 1:1 reading of
//! them is what makes the colours match.
//!
//! Two things are the exception. Twenty-two of the seventy syntaxes have no
//! outline *rules* at all, and twenty-three have no `Highlights.json` — theirs
//! live in tree-sitter queries beside the bundles, and there is no way to read
//! a query with a regex engine. See [`ts_outline`] and [`ts_highlight`].

mod highlight;
mod syntax;
pub mod ts_highlight;
pub mod ts_outline;

pub use highlight::{HighlightRule, HighlightToken, Highlighter, PRIORITY, RawRule, TokenSpan};
pub use syntax::{
    CompletionItem, FileMap, Indentation, OutlineItem, StringDelimiter, Syntax, SyntaxKind,
    SyntaxManager, SyntaxMetadata,
};
