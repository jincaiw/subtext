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

mod highlight;
mod syntax;

pub use highlight::{HighlightRule, HighlightToken, Highlighter, RawRule, TokenSpan};
pub use syntax::{
    CompletionItem, FileMap, Indentation, OutlineItem, StringDelimiter, Syntax, SyntaxKind,
    SyntaxManager, SyntaxMetadata,
};
