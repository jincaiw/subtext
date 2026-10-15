//! Text operations behind CotEditor's **Text** menu.
//!
//! Every function here takes the buffer and the current selection and returns
//! the replacement buffer plus the selection that should follow the edit, so
//! the caller can push one undoable change. Offsets are byte offsets into the
//! buffer and are always kept on `char` boundaries.

use std::cmp::Ordering;
use std::ops::Range;

use crate::line_ending::line_ending_at;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

/// A buffer replacement: the new text and where the selection lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub text: String,
    pub selection: Range<usize>,
}

impl Edit {
    /// A no-op, used when an operation does not apply.
    pub fn unchanged(text: impl Into<String>, selection: Range<usize>) -> Self {
        Self { text: text.into(), selection }
    }
}

/// Comment markers for the document's syntax, used by [`toggle_comment`].
#[derive(Debug, Clone, Default)]
pub struct CommentDelimiters {
    /// Line comment prefix, e.g. `//` or `#`.
    pub inline: Option<String>,
    /// Block comment pair, e.g. `/*` and `*/`.
    pub block: Option<(String, String)>,
}

/// The lines touched by `selection`, as byte ranges.
///
/// A selection that ends exactly at the start of a line does not include that
/// line — the same rule NSTextView and CotEditor use.
pub fn selected_line_ranges(text: &str, selection: &Range<usize>) -> Vec<Range<usize>> {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let mut line_end = text[end..].find('\n').map_or(text.len(), |i| end + i);
    if line_end < text.len() {
        // include the newline itself so line moves keep their structure
        line_end += 1;
    }
    std::iter::once(line_start..line_end).collect()
}

/// Index of the first line wholly or partially covered by `offset`.
fn line_index_at(text: &str, offset: usize) -> usize {
    text[..floor_char_boundary(text, offset)].matches('\n').count()
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Round `index` up to the next character boundary.
fn ceil_char_boundary(text: &str, mut index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

/// Shift every selected line right by one indentation step.
pub fn shift_right(text: &str, selection: &Range<usize>, indent: &str) -> Edit {
    indent_lines(text, selection, indent)
}

/// Remove one indentation step from every selected line.
pub fn shift_left(text: &str, selection: &Range<usize>, indent: &str) -> Edit {
    let range = selection.clone();
    let (start, end) = (range.start.min(text.len()), range.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);

    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    for line in split_inclusive_newline(&text[block_start..block_end]) {
        let body = line.trim_end_matches('\n');
        let newline = &line[body.len()..];
        match strip_one_indent(body, indent) {
            Some(rest) => out.push_str(rest),
            None => out.push_str(body),
        }
        out.push_str(newline);
    }
    out.push_str(&text[block_end..]);

    // Keep the selection over the same characters where possible.
    let removed_before = removed_before(&text[..start], &text[block_start..start], indent);
    let new_start = start.saturating_sub(removed_before).max(block_start);
    let new_end = (end.saturating_sub(removed_before)).min(out.len());
    Edit { text: out, selection: new_start..new_end.max(new_start) }
}

fn removed_before(_prefix: &str, before_cursor: &str, indent: &str) -> usize {
    strip_one_indent(before_cursor, indent).map_or(0, |rest| before_cursor.len() - rest.len())
}

fn strip_one_indent<'a>(line: &'a str, indent: &'a str) -> Option<&'a str> {
    if line.is_empty() {
        return None;
    }
    if let Some(rest) = line.strip_prefix(indent) {
        return Some(rest);
    }
    // A partial indent made of the same unit still counts as one step, which
    // is what makes Shift Left feel right on a half-indented line: four spaces
    // of indent strips at most four leading spaces, one tab strips one tab.
    let unit = indent.chars().next()?;
    let mut taken = 0;
    for c in line.chars() {
        if c == unit && taken < indent.len() {
            taken += c.len_utf8();
        } else {
            break;
        }
    }
    if taken > 0 { Some(&line[taken..]) } else { None }
}

fn indent_lines(text: &str, selection: &Range<usize>, indent: &str) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);

    let mut out = String::with_capacity(text.len() + indent.len() * 4);
    out.push_str(&text[..block_start]);
    let mut count = 0;
    for line in split_inclusive_newline(&text[block_start..block_end]) {
        // A blank line is left alone: indenting it would only create trailing
        // whitespace the user then has to clean up.
        if !line.trim_end_matches('\n').is_empty() {
            out.push_str(indent);
            count += indent.len();
        }
        out.push_str(line);
    }
    out.push_str(&text[block_end..]);

    let extra = if start == block_start { 0 } else { indent.len() };
    let new_start = start + extra;
    let new_end = (end + count).min(out.len());
    Edit { text: out, selection: new_start..new_end }
}

fn block_end_of(text: &str, end: usize) -> usize {
    let end = floor_char_boundary(text, end);
    let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i + 1);
    line_end.min(text.len())
}

fn split_inclusive_newline(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            parts.push(&text[start..=i]);
            start = i + 1;
        }
    }
    if start < text.len() {
        parts.push(&text[start..]);
    }
    parts
}

/// Convert the leading whitespace of the selection to spaces.
pub fn convert_indentation_to_spaces(text: &str, selection: &Range<usize>, tab_width: usize) -> Edit {
    map_selected_lines(text, selection, |line| {
        let leading = line.len() - line.trim_start_matches([' ', '\t']).len();
        let mut column = 0;
        for c in line[..leading].chars() {
            if c == '\t' {
                column += tab_width - (column % tab_width);
            } else {
                column += 1;
            }
        }
        format!("{}{}", " ".repeat(column), &line[leading..])
    })
}

/// Convert the leading whitespace of the selection to tabs.
pub fn convert_indentation_to_tabs(text: &str, selection: &Range<usize>, tab_width: usize) -> Edit {
    map_selected_lines(text, selection, |line| {
        let leading = line.len() - line.trim_start_matches([' ', '\t']).len();
        let mut column = 0;
        for c in line[..leading].chars() {
            if c == '\t' {
                column += tab_width - (column % tab_width);
            } else {
                column += 1;
            }
        }
        let tabs = column / tab_width;
        let spaces = column % tab_width;
        format!("{}{}{}", "\t".repeat(tabs), " ".repeat(spaces), &line[leading..])
    })
}

/// Wrap the **selection** in `begin` and `end`.
///
/// CotEditor's `String.surround(in:begin:end:)`, which is what all six
/// *Surround Selection With* commands call. The delimiters go around the
/// selection itself and not around the lines it touches — upstream's own
/// fixture is `"foo\nbar\nbaz"` with `<` and `>` over the ranges
/// `0..<3`, `4..<4` and `8..<11`, giving `"<foo>\n<>bar\n<baz>"`. An empty
/// selection is a range like any other, so a bare caret gets the pair with the
/// caret left between its two halves.
///
/// The selection stays on the text that was selected rather than growing to
/// cover the delimiters, so pressing ⌘' twice nests the quotes.
pub fn surround(text: &str, selection: &Range<usize>, begin: &str, end: &str) -> Edit {
    let (start, end_of_selection) = if selection.start <= selection.end {
        (selection.start, selection.end)
    } else {
        (selection.end, selection.start)
    };
    let start = floor_char_boundary(text, start);
    let end_of_selection = ceil_char_boundary(text, end_of_selection);

    let mut out = String::with_capacity(text.len() + begin.len() + end.len());
    out.push_str(&text[..start]);
    out.push_str(begin);
    out.push_str(&text[start..end_of_selection]);
    out.push_str(end);
    out.push_str(&text[end_of_selection..]);

    let offset = start + begin.len();
    Edit { text: out, selection: offset..offset + (end_of_selection - start) }
}

/// Trim whitespace at the end of every selected line.
pub fn trim_trailing_whitespace(text: &str, selection: &Range<usize>) -> Edit {
    map_selected_lines(text, selection, |line| line.trim_end().to_string())
}

fn map_selected_lines(
    text: &str,
    selection: &Range<usize>,
    mut map: impl FnMut(&str) -> String,
) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);

    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    for line in split_inclusive_newline(&text[block_start..block_end]) {
        let body = line.trim_end_matches('\n');
        let newline = &line[body.len()..];
        out.push_str(&map(body));
        out.push_str(newline);
    }
    out.push_str(&text[block_end..]);
    let delta = out.len() as isize - text.len() as isize;
    let new_end = (end as isize + delta).max(start as isize) as usize;
    let new_end = new_end.min(out.len());
    Edit { text: out, selection: start..new_end }
}

/// Toggle line or block comments over the selection.
///
/// CotEditor's rule: if every selected line already carries the inline
/// prefix, remove it; otherwise add it. When the syntax has no inline
/// comment, the block delimiters are wrapped around the whole selection
/// instead, and unwrapped if they are already there.
pub fn toggle_comment(
    text: &str,
    selection: &Range<usize>,
    delimiters: &CommentDelimiters,
) -> Edit {
    if let Some(inline) = &delimiters.inline {
        let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
        let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
        let block_end = block_end_of(text, end);
        let lines = split_inclusive_newline(&text[block_start..block_end]);

        let all_commented = !lines.is_empty()
            && lines.iter().all(|line| {
                let body = line.trim_end_matches('\n');
                body.trim_start().is_empty() || body.trim_start().starts_with(inline.as_str())
            });

        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..block_start]);
        for line in &lines {
            let body = line.trim_end_matches('\n');
            let newline = &line[body.len()..];
            if body.trim_start().is_empty() {
                out.push_str(line);
                continue;
            }
            if all_commented {
                let indent = body.len() - body.trim_start().len();
                let stripped = body[indent..].strip_prefix(inline.as_str()).unwrap_or(&body[indent..]);
                // also eat the single space CotEditor inserts after the marker
                let stripped = stripped.strip_prefix(' ').unwrap_or(stripped);
                out.push_str(&body[..indent]);
                out.push_str(stripped);
            } else {
                let indent = body.len() - body.trim_start().len();
                out.push_str(&body[..indent]);
                out.push_str(inline);
                out.push(' ');
                out.push_str(&body[indent..]);
            }
            out.push_str(newline);
        }
        out.push_str(&text[block_end..]);
        let new_end = out.len().min(text.len() + lines.len() * (inline.len() + 1));
        return Edit { text: out, selection: start..new_end };
    }

    if let Some((open, close)) = &delimiters.block {
        let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
        let selected = &text[start..end];
        let trimmed = selected.trim();
        let (new_text, delta) = if trimmed.starts_with(open.as_str())
            && trimmed.ends_with(close.as_str())
            && trimmed.len() >= open.len() + close.len()
        {
            // Strip the outer delimiters, keeping whatever padding sat
            // between them and the text.
            let inner_start = selected.find(open.as_str()).unwrap_or(0);
            let inner_end = selected.rfind(close.as_str()).unwrap_or(selected.len());
            let mut replaced = String::with_capacity(selected.len());
            replaced.push_str(&selected[..inner_start]);
            replaced.push_str(&selected[inner_start + open.len()..inner_end]);
            replaced.push_str(&selected[inner_end + close.len()..]);
            let delta = -(open.len() as isize + close.len() as isize);
            (replaced, delta)
        } else {
            (format!("{open}{selected}{close}"), (open.len() + close.len()) as isize)
        };
        let mut out = String::with_capacity(text.len() + 4);
        out.push_str(&text[..start]);
        out.push_str(&new_text);
        out.push_str(&text[end..]);
        let new_end = (end as isize + delta).max(start as isize) as usize;
        return Edit { text: out, selection: start..new_end };
    }

    Edit::unchanged(text.to_string(), selection.clone())
}

/// Move the selected lines up one line, or down one line.
pub fn move_lines(text: &str, selection: &Range<usize>, up: bool) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);

    if up {
        if block_start == 0 {
            return Edit::unchanged(text.to_string(), selection.clone());
        }
        let previous_start = text[..block_start - 1].rfind('\n').map_or(0, |i| i + 1);
        let previous = &text[previous_start..block_start];
        let block = &text[block_start..block_end];
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..previous_start]);
        out.push_str(block);
        out.push_str(previous);
        out.push_str(&text[block_end..]);
        let shift = block.len() as isize - previous.len() as isize;
        let new_start = (start as isize + shift) as usize;
        let new_end = (end as isize + shift) as usize;
        Edit { text: out, selection: new_start..new_end }
    } else {
        if block_end >= text.len() {
            return Edit::unchanged(text.to_string(), selection.clone());
        }
        let next_end = text[block_end..].find('\n').map_or(text.len(), |i| block_end + i + 1);
        let next = &text[block_end..next_end];
        let block = &text[block_start..block_end];
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..block_start]);
        out.push_str(next);
        out.push_str(block);
        out.push_str(&text[next_end..]);
        let shift = next.len() as isize;
        let new_start = (start as isize + shift) as usize;
        let new_end = (end as isize + shift) as usize;
        Edit { text: out, selection: new_start..new_end }
    }
}

/// Duplicate every selected line.
pub fn duplicate_lines(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);
    let block = text[block_start..block_end].to_string();
    let needs_newline = !block.ends_with('\n');
    let mut inserted = block;
    if needs_newline {
        inserted.push('\n');
    }

    let mut out = String::with_capacity(text.len() + inserted.len());
    out.push_str(&text[..block_end]);
    out.push_str(&inserted);
    out.push_str(&text[block_end..]);
    let new_start = start + inserted.len();
    let new_end = end + inserted.len();
    Edit { text: out, selection: new_start..new_end }
}

/// Delete the selection, or the character before the caret when there is none.
///
/// CotEditor's `Edit ▸ Delete`, which is `NSTextView.delete:` behind ⌘⌫. That
/// action deletes backwards, so the shortcut does what ⌫ does — the menu's key
/// equivalent wins over AppKit's own `deleteToBeginningOfLine:`, and this is
/// the shortcut the menu actually shows.
///
/// "Character" is one extended grapheme cluster, so ⌘⌫ after `👍🏻` takes the
/// whole thing rather than half of it.
pub fn delete_backward(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = if selection.start <= selection.end {
        (selection.start, selection.end)
    } else {
        (selection.end, selection.start)
    };
    let start = floor_char_boundary(text, start);
    let end = ceil_char_boundary(text, end);

    // A selection is deleted outright.
    if start < end {
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..start]);
        out.push_str(&text[end..]);
        return Edit { text: out, selection: start..start };
    }

    // A bare caret deletes the character before it, and does nothing at the
    // start of the document.
    let Some(previous) = text[..start].grapheme_indices(true).next_back() else {
        return Edit::unchanged(text.to_string(), selection.clone());
    };
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..previous.0]);
    out.push_str(&text[start..]);
    Edit { text: out, selection: previous.0..previous.0 }
}

/// Delete every selected line.
pub fn delete_lines(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    out.push_str(&text[block_end..]);
    let caret = block_start.min(out.len());
    Edit { text: out, selection: caret..caret }
}

/// Join the selected lines into one, collapsing the whitespace at each break.
pub fn join_lines(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);
    let block = text[block_start..block_end].trim_end_matches('\n');
    let joined = block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    out.push_str(&joined);
    out.push_str(&text[block_end..]);
    Edit { text: out, selection: block_start..block_start + joined.len() }
}

/// Options for [`sort_lines`], mirroring CotEditor's *Sort Lines by Pattern*
/// sheet.
///
/// The defaults are CotEditor's, which is why they are not all `false`: the
/// sheet opens with case folding and numeric interpretation already on.
#[derive(Debug, Clone, PartialEq)]
pub struct SortOptions {
    pub descending: bool,
    pub ignores_case: bool,
    pub numeric: bool,
    /// Hold the first line at the top and sort the rest — a header row.
    pub keeps_first_line: bool,
}

impl Default for SortOptions {
    fn default() -> Self {
        Self { descending: false, ignores_case: true, numeric: true, keeps_first_line: false }
    }
}

impl SortOptions {
    /// What plain *Sort Lines* uses: the whole line, compared as text.
    ///
    /// CotEditor sorts that command with a localized comparator and no numeric
    /// reading, which is why `10` lands before `9` there. Locale collation is
    /// the one part of this we cannot reproduce without an ICU dependency; the
    /// rest of the shape is the same.
    pub fn whole_line() -> Self {
        Self { ignores_case: false, numeric: false, ..Self::default() }
    }
}

/// Where a line's sort key comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortKey {
    /// The whole line.
    EntireLine,
    /// One field of a delimited record, numbered from 1.
    Column { delimiter: String, column: usize },
    /// The text a regular expression matches, or one of its capture groups.
    RegularExpression { pattern: String, ignore_case: bool, group: Option<usize> },
}

impl SortKey {
    /// The part of `line` this key sorts on.
    ///
    /// `None` means the line has no such part — a record with too few fields,
    /// or a line the pattern does not match — and such lines sort to the end,
    /// which is how a header or a comment keeps out of the way.
    pub fn key_for(&self, line: &str) -> Option<String> {
        match self {
            SortKey::EntireLine => Some(line.to_string()),
            SortKey::Column { delimiter, column } => {
                // The field is a literal, so `\t` means a tab — which is how a
                // TSV file gets sorted by one of its columns.
                let delimiter = if delimiter.is_empty() {
                    ",".to_string()
                } else {
                    crate::finder::unescape(delimiter)
                };
                let index = column.saturating_sub(1);
                let field = line.split(delimiter.as_str()).nth(index)?;
                Some(field.trim().to_string())
            }
            SortKey::RegularExpression { pattern, ignore_case, group } => {
                let pattern = if *ignore_case {
                    format!("(?i){pattern}")
                } else {
                    pattern.clone()
                };
                let regex = regex::Regex::new(&pattern).ok()?;
                let captures = regex.captures(line)?;
                match group {
                    Some(index) => captures.get(*index).map(|m| m.as_str().to_string()),
                    None => captures.get(0).map(|m| m.as_str().to_string()),
                }
            }
        }
    }

    /// Why the key cannot be used, or `None` when it can.
    ///
    /// The sheet calls this before enabling its Sort button, so an
    /// unparseable pattern is reported while the user is still typing it
    /// rather than after they commit to a sort that would do nothing.
    pub fn validate(&self) -> Option<SortKeyError> {
        match self {
            SortKey::EntireLine => None,
            SortKey::Column { column, .. } => (*column == 0).then_some(SortKeyError::EmptyColumn),
            SortKey::RegularExpression { pattern, ignore_case, group } => {
                if pattern.is_empty() {
                    return Some(SortKeyError::EmptyPattern);
                }
                let pattern = if *ignore_case {
                    format!("(?i){pattern}")
                } else {
                    pattern.clone()
                };
                let Ok(regex) = regex::Regex::new(&pattern) else {
                    return Some(SortKeyError::InvalidPattern);
                };
                // Group 0 is the whole match, so the count of usable indices
                // is the capture count plus one.
                if let Some(index) = group
                    && *index > regex.captures_len() - 1
                {
                    return Some(SortKeyError::InvalidPattern);
                }
                None
            }
        }
    }

    /// Where in `line` the key sits, for the sheet's sample-line preview.
    ///
    /// This is not quite [`Self::key_for`]'s range. A delimited field that is
    /// nothing but spaces has the key `""` — it sorts as an empty string — but
    /// no range to highlight, and upstream draws the same distinction.
    pub fn span_for(&self, line: &str) -> Option<Range<usize>> {
        match self {
            SortKey::EntireLine => Some(0..line.len()),
            SortKey::Column { delimiter, column } => {
                let delimiter = if delimiter.is_empty() {
                    ",".to_string()
                } else {
                    crate::finder::unescape(delimiter)
                };
                let mut start = 0;
                for _ in 1..(*column).max(1) {
                    let offset = line[start..].find(delimiter.as_str())?;
                    start += offset + delimiter.len();
                }
                let end = line[start..]
                    .find(delimiter.as_str())
                    .map_or(line.len(), |offset| start + offset);
                let field = &line[start..end];
                let head = start + field.len() - field.trim_start().len();
                let tail = end - (field.len() - field.trim_end().len());
                (head < tail).then_some(head..tail)
            }
            SortKey::RegularExpression { pattern, ignore_case, group } => {
                let pattern = if *ignore_case {
                    format!("(?i){pattern}")
                } else {
                    pattern.clone()
                };
                let regex = regex::Regex::new(&pattern).ok()?;
                let captures = regex.captures(line)?;
                let matched = captures.get(group.unwrap_or(0))?;
                Some(matched.start()..matched.end())
            }
        }
    }
}

/// Why a [`SortKey`] cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKeyError {
    EmptyPattern,
    InvalidPattern,
    EmptyColumn,
}

impl SortKeyError {
    /// The message CotEditor's sheet shows next to the field.
    pub fn message(self) -> &'static str {
        match self {
            SortKeyError::EmptyPattern => "Empty pattern",
            SortKeyError::InvalidPattern => "Invalid pattern",
            SortKeyError::EmptyColumn => "The position starts at 1",
        }
    }
}

/// Sort the selected lines by `key`.
pub fn sort_lines(
    text: &str,
    selection: &Range<usize>,
    key: &SortKey,
    options: &SortOptions,
) -> Edit {
    reorder_lines(text, selection, |lines| {
        let first = options.keeps_first_line.then(|| lines.remove(0));

        let mut keyed: Vec<(Option<String>, LogicalLine)> = lines
            .drain(..)
            .map(|line| (key.key_for(&line.contents), line))
            .collect();
        keyed.sort_by(|(a, _), (b, _)| compare_keys(a, b, options));

        lines.extend(keyed.into_iter().map(|(_, line)| line));
        if options.descending {
            lines.reverse();
        }
        if let Some(first) = first {
            lines.insert(0, first);
        }
    })
}

/// Reverse the order of the selected lines.
pub fn reverse_lines(text: &str, selection: &Range<usize>) -> Edit {
    reorder_lines(text, selection, |lines| lines.reverse())
}

/// Shuffle the selected lines with a caller-supplied random source.
pub fn shuffle_lines(
    text: &str,
    selection: &Range<usize>,
    mut random: impl FnMut(usize) -> usize,
) -> Edit {
    reorder_lines(text, selection, |lines| {
        // Fisher-Yates.
        for i in (1..lines.len()).rev() {
            let j = random(i + 1) % (i + 1);
            lines.swap(i, j);
        }
    })
}

/// One logical line: its text, and the terminator that followed it.
///
/// The terminator is carried rather than normalised because a document can mix
/// them — a file assembled from two sources often does — and a sort that
/// rewrites every ending is a sort that quietly edits lines the user never
/// asked it to touch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalLine {
    /// The line, without its terminator.
    pub contents: String,
    /// `\n`, `\r\n`, `\r`, or `None` for the last line of a file that does
    /// not end in one.
    pub line_ending: Option<String>,
}

/// Split `range` into logical lines, each keeping its own terminator.
pub fn logical_lines(text: &str, range: &Range<usize>) -> Vec<LogicalLine> {
    let contents = line_content_ranges(text, range);
    let end = floor_char_boundary(text, range.end.min(text.len()));

    contents
        .iter()
        .enumerate()
        .map(|(index, line)| {
            // The terminator runs from the end of this line's text to the start
            // of the next line, or to the end of the range for the last one.
            let next = contents.get(index + 1).map_or(end, |next| next.start);
            let ending = &text[line.end..next.max(line.end)];
            LogicalLine {
                contents: text[line.clone()].to_string(),
                line_ending: (!ending.is_empty()).then(|| ending.to_string()),
            }
        })
        .collect()
}

/// Join logical lines, re-attaching each one's own terminator.
///
/// A line that arrives without one — the last line of a file that does not end
/// in a newline — is given `base` when it lands anywhere but last, and nothing
/// when it stays last unless `including_trailing_line_ending` says otherwise.
/// `base` is the first terminator the block had, so a block that had any keeps
/// ending its lines the way its author did.
pub fn join_logical_lines(
    lines: &[LogicalLine],
    base: &str,
    including_trailing_line_ending: bool,
) -> String {
    let last = lines.len().saturating_sub(1);
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        out.push_str(&line.contents);
        if index == last && !including_trailing_line_ending {
            continue;
        }
        out.push_str(line.line_ending.as_deref().unwrap_or(base));
    }
    out
}

/// The whole lines `selection` touches, terminator included.
fn line_block(text: &str, selection: &Range<usize>) -> Range<usize> {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);
    block_start..block_end
}

/// Reorder the selected lines with `reorder`, keeping each line's own
/// terminator attached to it.
///
/// The selection that comes back covers the first line's text, which is what
/// CotEditor leaves highlighted after a sort: the block the user picked is
/// still the block they are looking at, and the caret is at its head.
fn reorder_lines(
    text: &str,
    selection: &Range<usize>,
    reorder: impl FnOnce(&mut Vec<LogicalLine>),
) -> Edit {
    let block = line_block(text, selection);
    let mut lines = logical_lines(text, &block);
    let includes_trailing = lines.last().is_some_and(|line| line.line_ending.is_some());

    // One line is already sorted, and a block with no terminators at all has
    // nothing to re-attach — CotEditor bails out of both.
    if lines.len() < 2 {
        return Edit::unchanged(text, selection.clone());
    }
    let Some(base) = lines.iter().find_map(|line| line.line_ending.clone()) else {
        return Edit::unchanged(text, selection.clone());
    };

    reorder(&mut lines);

    let joined = join_logical_lines(&lines, &base, includes_trailing);
    let head = lines.first().map_or(0, |line| line.contents.len());
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block.start]);
    out.push_str(&joined);
    out.push_str(&text[block.end..]);
    Edit { text: out, selection: block.start..block.start + head }
}

/// Compare two sort keys under `options`.
fn compare_keys(a: &Option<String>, b: &Option<String>, options: &SortOptions) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            // A pair of numbers is compared as numbers; everything else — and
            // any pair where the numbers are equal — falls back to text.
            if options.numeric
                && let (Some(x), Some(y)) = (leading_number(a), leading_number(b))
                && x != y
            {
                return x.partial_cmp(&y).unwrap_or(Ordering::Equal);
            }
            if options.ignores_case {
                a.to_lowercase().cmp(&b.to_lowercase())
            } else {
                a.cmp(b)
            }
        }
        // A line with no key at all sorts after one that has a key.
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

/// The number at the head of `value`, if there is one.
///
/// CotEditor reads numbers with `FloatingPointFormatStyle`, which consumes a
/// *prefix*: `-1000.1 m/s` is `-1000.1`, and `dog 10` is nothing at all.
/// `f64::from_str` insists on the whole string, so the leading number is cut
/// out first.
///
/// Grouping separators follow the rule ICU applies — a separator only counts
/// when exactly three digits follow it — which is why `-1000,1` reads as
/// `-1000` and not as `-10001`. The locale is fixed to the one CotEditor falls
/// back on, so `.` is the decimal point and `,` and a space group digits.
fn leading_number(value: &str) -> Option<f64> {
    let text = value.trim_start();
    let bytes = text.as_bytes();
    let mut cleaned = String::new();
    let mut i = 0;

    if let Some(&sign @ (b'+' | b'-')) = bytes.first() {
        cleaned.push(sign as char);
        i = 1;
    }

    let integer_start = i;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    if i == integer_start {
        return None;
    }
    cleaned.push_str(&text[integer_start..i]);

    while let Some(&separator) = bytes.get(i)
        && matches!(separator, b',' | b' ')
    {
        let group = i + 1;
        let end = group + 3;
        if end > bytes.len() || !bytes[group..end].iter().all(u8::is_ascii_digit) {
            break;
        }
        cleaned.push_str(&text[group..end]);
        i = end;
    }

    if bytes.get(i) == Some(&b'.') {
        let point = i;
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i > point + 1 {
            cleaned.push_str(&text[point..i]);
        }
    }

    cleaned.parse().ok()
}

/// Remove duplicate lines, keeping the first occurrence.
pub fn delete_duplicate_lines(text: &str, selection: &Range<usize>, case_insensitive: bool) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);
    let block = &text[block_start..block_end];
    let trailing_newline = block.ends_with('\n');
    let body = block.trim_end_matches('\n');
    let lines: Vec<&str> = if body.is_empty() { Vec::new() } else { body.split('\n').collect() };

    let mut seen = std::collections::HashSet::new();
    let mut kept = Vec::with_capacity(lines.len());
    for line in lines {
        let key = if case_insensitive { line.to_lowercase() } else { line.to_string() };
        if seen.insert(key) {
            kept.push(line);
        }
    }

    let mut joined = kept.join("\n");
    if trailing_newline && !joined.is_empty() {
        joined.push('\n');
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    out.push_str(&joined);
    out.push_str(&text[block_end..]);
    Edit { text: out, selection: block_start..block_start + joined.len() }
}

/// Which case transformation to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseTransform {
    Upper,
    Lower,
    Capitalize,
}

/// Change the case of the selection.
pub fn transform_case(text: &str, selection: &Range<usize>, transform: CaseTransform) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start == end {
        return Edit::unchanged(text.to_string(), selection.clone());
    }
    let selected = &text[start..end];
    let replaced = match transform {
        CaseTransform::Upper => selected.to_uppercase(),
        CaseTransform::Lower => selected.to_lowercase(),
        CaseTransform::Capitalize => capitalize_words(selected),
    };
    let mut out = String::with_capacity(text.len() + replaced.len());
    out.push_str(&text[..start]);
    out.push_str(&replaced);
    out.push_str(&text[end..]);
    Edit { text: out, selection: start..start + replaced.len() }
}

/// Uppercase the first letter of each word, leaving the rest untouched —
/// CotEditor's "Capitalize", which is *not* title-casing.
fn capitalize_words(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut at_word_start = true;
    for c in input.chars() {
        if c.is_alphanumeric() {
            if at_word_start {
                out.extend(c.to_uppercase());
                at_word_start = false;
            } else {
                out.push(c);
            }
        } else {
            at_word_start = true;
            out.push(c);
        }
    }
    out
}

/// Replace every curly quote in the selection with the straight one.
///
/// CotEditor's `straightenQuotesInSelection:`, which is its own
/// `String.straighteningQuotes` applied to the selection. That property is two
/// character classes and nothing more — the four single quotation marks of the
/// U+2018 block fold to `'`, the four double ones of the U+201C block fold to
/// `"` — so guillemets, primes, the fullwidth forms and the straight quotes
/// themselves are all left alone.
///
/// Upstream greys this item out while the selection is empty, even though its
/// command would work on the word under the cursor, and it says why in its own
/// source: to keep the item's state the same as `Replace Quotes`, which does
/// need a selection. The empty case here is therefore the no-op it is for every
/// other transform in this module; the caller is what tells the user.
pub fn straighten_quotes(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start == end {
        return Edit::unchanged(text.to_string(), selection.clone());
    }

    let replaced: String = text[start..end]
        .chars()
        .map(|c| match c {
            // U+2018 ‘, U+2019 ’, U+201A ‚, U+201B ‛
            '\u{2018}'..='\u{201B}' => '\'',
            // U+201C “, U+201D ”, U+201E „, U+201F ‟
            '\u{201C}'..='\u{201F}' => '"',
            other => other,
        })
        .collect();

    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    out.push_str(&replaced);
    out.push_str(&text[end..]);
    Edit { text: out, selection: start..start + replaced.len() }
}

/// Which width conversion to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthTransform {
    /// Every character, including punctuation and katakana.
    Full,
    /// Only ASCII letters and digits.
    FullAlphanumeric,
    Half,
    HalfAlphanumeric,
}

/// Convert between full-width and half-width forms.
pub fn transform_width(text: &str, selection: &Range<usize>, transform: WidthTransform) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start == end {
        return Edit::unchanged(text.to_string(), selection.clone());
    }
    let selected = &text[start..end];
    let replaced = match transform {
        WidthTransform::Full => to_full_width(selected, true),
        WidthTransform::FullAlphanumeric => to_full_width(selected, false),
        WidthTransform::Half => to_half_width(selected, true),
        WidthTransform::HalfAlphanumeric => to_half_width(selected, false),
    };
    let mut out = String::with_capacity(text.len() + replaced.len());
    out.push_str(&text[..start]);
    out.push_str(&replaced);
    out.push_str(&text[end..]);
    Edit { text: out, selection: start..start + replaced.len() }
}

fn to_full_width(input: &str, include_kana: bool) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for c in input.chars() {
        match c {
            ' ' => out.push('\u{3000}'),
            '\u{21}'..='\u{7E}' => {
                out.push(char::from_u32(c as u32 + 0xFEE0).unwrap_or(c));
            }
            '\u{FF61}'..='\u{FF9F}' if include_kana => {
                out.push_str(&half_kana_to_full(c));
            }
            _ => out.push(c),
        }
    }
    out
}

fn to_half_width(input: &str, include_kana: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\u{3000}' => out.push(' '),
            '\u{FF01}'..='\u{FF5E}' => {
                out.push(char::from_u32(c as u32 - 0xFEE0).unwrap_or(c));
            }
            '\u{3041}'..='\u{30FF}' if include_kana => {
                out.push_str(&full_kana_to_half(c));
            }
            _ => out.push(c),
        }
    }
    out
}

/// Half-width katakana, with the dakuten/handakuten forms expanded.
fn half_kana_to_full(c: char) -> String {
    const TABLE: [(char, &str); 63] = [
        ('\u{FF61}', "。"), ('\u{FF62}', "「"), ('\u{FF63}', "」"), ('\u{FF64}', "、"),
        ('\u{FF65}', "・"), ('\u{FF66}', "ヲ"), ('\u{FF67}', "ァ"), ('\u{FF68}', "ィ"),
        ('\u{FF69}', "ゥ"), ('\u{FF6A}', "ェ"), ('\u{FF6B}', "ォ"), ('\u{FF6C}', "ャ"),
        ('\u{FF6D}', "ュ"), ('\u{FF6E}', "ョ"), ('\u{FF6F}', "ッ"), ('\u{FF70}', "ー"),
        ('\u{FF71}', "ア"), ('\u{FF72}', "イ"), ('\u{FF73}', "ウ"), ('\u{FF74}', "エ"),
        ('\u{FF75}', "オ"), ('\u{FF76}', "カ"), ('\u{FF77}', "キ"), ('\u{FF78}', "ク"),
        ('\u{FF79}', "ケ"), ('\u{FF7A}', "コ"), ('\u{FF7B}', "サ"), ('\u{FF7C}', "シ"),
        ('\u{FF7D}', "ス"), ('\u{FF7E}', "セ"), ('\u{FF7F}', "ソ"), ('\u{FF80}', "タ"),
        ('\u{FF81}', "チ"), ('\u{FF82}', "ツ"), ('\u{FF83}', "テ"), ('\u{FF84}', "ト"),
        ('\u{FF85}', "ナ"), ('\u{FF86}', "ニ"), ('\u{FF87}', "ヌ"), ('\u{FF88}', "ネ"),
        ('\u{FF89}', "ノ"), ('\u{FF8A}', "ハ"), ('\u{FF8B}', "ヒ"), ('\u{FF8C}', "フ"),
        ('\u{FF8D}', "ヘ"), ('\u{FF8E}', "ホ"), ('\u{FF8F}', "マ"), ('\u{FF90}', "ミ"),
        ('\u{FF91}', "ム"), ('\u{FF92}', "メ"), ('\u{FF93}', "モ"), ('\u{FF94}', "ヤ"),
        ('\u{FF95}', "ユ"), ('\u{FF96}', "ヨ"), ('\u{FF97}', "ラ"), ('\u{FF98}', "リ"),
        ('\u{FF99}', "ル"), ('\u{FF9A}', "レ"), ('\u{FF9B}', "ロ"), ('\u{FF9C}', "ワ"),
        ('\u{FF9D}', "ン"), ('\u{FF9E}', "\u{3099}"), ('\u{FF9F}', "\u{309A}"),
    ];
    for (from, to) in TABLE {
        if from == c {
            return to.to_string();
        }
    }
    c.to_string()
}

fn full_kana_to_half(c: char) -> String {
    // Reverse of the table above, applied to the voiced forms first.
    const VOICED: [(char, &str); 21] = [
        ('ガ', "ｶﾞ"), ('ギ', "ｷﾞ"), ('グ', "ｸﾞ"), ('ゲ', "ｹﾞ"), ('ゴ', "ｺﾞ"),
        ('ザ', "ｻﾞ"), ('ジ', "ｼﾞ"), ('ズ', "ｽﾞ"), ('ゼ', "ｾﾞ"), ('ゾ', "ｿﾞ"),
        ('ダ', "ﾀﾞ"), ('ヂ', "ﾁﾞ"), ('ヅ', "ﾂﾞ"), ('デ', "ﾃﾞ"), ('ド', "ﾄﾞ"),
        ('バ', "ﾊﾞ"), ('ビ', "ﾋﾞ"), ('ブ', "ﾌﾞ"), ('ベ', "ﾍﾞ"), ('ボ', "ﾎﾞ"),
        ('ヴ', "ｳﾞ"),
    ];
    const SEMI: [(char, &str); 5] =
        [('パ', "ﾊﾟ"), ('ピ', "ﾋﾟ"), ('プ', "ﾌﾟ"), ('ペ', "ﾍﾟ"), ('ポ', "ﾎﾟ")];
    const PLAIN: [(char, &str); 56] = [
        ('。', "｡"), ('「', "｢"), ('」', "｣"), ('、', "､"), ('・', "･"), ('ヲ', "ｦ"),
        ('ァ', "ｧ"), ('ィ', "ｨ"), ('ゥ', "ｩ"), ('ェ', "ｪ"), ('ォ', "ｫ"), ('ャ', "ｬ"),
        ('ュ', "ｭ"), ('ョ', "ｮ"), ('ッ', "ｯ"), ('ー', "ｰ"), ('ア', "ｱ"), ('イ', "ｲ"),
        ('ウ', "ｳ"), ('エ', "ｴ"), ('オ', "ｵ"), ('カ', "ｶ"), ('キ', "ｷ"), ('ク', "ｸ"),
        ('ケ', "ｹ"), ('コ', "ｺ"), ('サ', "ｻ"), ('シ', "ｼ"), ('ス', "ｽ"), ('セ', "ｾ"),
        ('ソ', "ｿ"), ('タ', "ﾀ"), ('チ', "ﾁ"), ('ツ', "ﾂ"), ('テ', "ﾃ"), ('ト', "ﾄ"),
        ('ナ', "ﾅ"), ('ニ', "ﾆ"), ('ヌ', "ﾇ"), ('ネ', "ﾈ"), ('ノ', "ﾉ"), ('ハ', "ﾊ"),
        ('ヒ', "ﾋ"), ('フ', "ﾌ"), ('ヘ', "ﾍ"), ('ホ', "ﾎ"), ('マ', "ﾏ"), ('ミ', "ﾐ"),
        ('ム', "ﾑ"), ('メ', "ﾒ"), ('モ', "ﾓ"), ('ヤ', "ﾔ"), ('ユ', "ﾕ"), ('ヨ', "ﾖ"),
        ('ラ', "ﾗ"), ('リ', "ﾘ"),
    ];
    for (from, to) in VOICED.iter().chain(SEMI.iter()).chain(PLAIN.iter()) {
        if *from == c {
            return (*to).to_string();
        }
    }
    // Hiragana shares its codepoints' shapes; map the common range by offset.
    if ('\u{3041}'..='\u{3093}').contains(&c) {
        let katakana = char::from_u32(c as u32 + 0x60).unwrap_or(c);
        if katakana != c {
            return full_kana_to_half(katakana);
        }
    }
    // The tail of the katakana block that the table above omits.
    const TAIL: [(char, &str); 13] = [
        ('ル', "ﾙ"), ('レ', "ﾚ"), ('ロ', "ﾛ"), ('ワ', "ﾜ"), ('ン', "ﾝ"), ('゛', "ﾞ"),
        ('゜', "ﾟ"), ('ヮ', "ﾜ"), ('ヰ', "ｲ"), ('ヱ', "ｴ"), ('ヵ', "ｶ"), ('ヶ', "ｹ"),
        ('ヽ', "ﾂ"),
    ];
    for (from, to) in TAIL {
        if from == c {
            return to.to_string();
        }
    }
    c.to_string()
}

/// Percent-encode the selection (RFC 3986 unreserved characters are kept).
pub fn url_encode(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start == end {
        return Edit::unchanged(text.to_string(), selection.clone());
    }
    let mut replaced = String::with_capacity((end - start) * 3);
    for byte in &text.as_bytes()[start..end] {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                replaced.push(*byte as char);
            }
            _ => replaced.push_str(&format!("%{byte:02X}")),
        }
    }
    let mut out = String::with_capacity(text.len() + replaced.len());
    out.push_str(&text[..start]);
    out.push_str(&replaced);
    out.push_str(&text[end..]);
    Edit { text: out, selection: start..start + replaced.len() }
}

/// Decode percent-escapes in the selection.
pub fn url_decode(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start == end {
        return Edit::unchanged(text.to_string(), selection.clone());
    }
    let bytes = &text.as_bytes()[start..end];
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                decoded.push(value);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    let replaced = String::from_utf8_lossy(&decoded).into_owned();
    let mut out = String::with_capacity(text.len() + replaced.len());
    out.push_str(&text[..start]);
    out.push_str(&replaced);
    out.push_str(&text[end..]);
    Edit { text: out, selection: start..start + replaced.len() }
}

/// Which Unicode normalization form to apply.
///
/// Upstream's `UnicodeNormalizationForm`, all seven of them, in its order. The
/// first four are Unicode's own, and the `unicode-normalization` crate provides
/// them. The last three are Apple's:
///
/// * `NfkcCaseFold` is NFKC with a full case fold, and with the
///   default-ignorable code points taken out before and after — upstream calls
///   `removingDefaultIgnorableCodePoints`, which is a regular expression over
///   the `Default_Ignorable_Code_Point` property.
/// * `ModifiedNfd` / `ModifiedNfc` are the forms HFS+ used. They are not
///   normalization forms at all: they are Apple's frozen tables, and the only
///   way to produce them is `CFStringGetFileSystemRepresentation` and the
///   composition-exclusion set behind `precomposedStringWithHFSPlusMapping`.
///
/// Both halves of that are Unicode data SubText does not carry, so
/// [`is_supported`](Self::is_supported) answers for the last three — see the
/// note on [`normalize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Normalization {
    /// Canonical decomposition.
    Nfd,
    /// Canonical composition.
    Nfc,
    /// Compatibility decomposition.
    Nfkd,
    /// Compatibility composition.
    Nfkc,
    /// NFKC, case-folded, with the default-ignorable code points removed.
    NfkcCaseFold,
    /// The decomposition HFS+ used.
    ModifiedNfd,
    /// The composition HFS+ used.
    ModifiedNfc,
}

impl Normalization {
    /// Every form, in upstream's `CaseIterable` order.
    pub const ALL: [Normalization; 7] = [
        Normalization::Nfd,
        Normalization::Nfc,
        Normalization::Nfkd,
        Normalization::Nfkc,
        Normalization::NfkcCaseFold,
        Normalization::ModifiedNfd,
        Normalization::ModifiedNfc,
    ];

    /// The five upstream calls the standard forms, which its menu lists first.
    pub const STANDARD: [Normalization; 5] = [
        Normalization::Nfd,
        Normalization::Nfc,
        Normalization::Nfkd,
        Normalization::Nfkc,
        Normalization::NfkcCaseFold,
    ];

    /// The two upstream calls the modified forms, which its menu lists after a
    /// separator.
    pub const MODIFIED: [Normalization; 2] =
        [Normalization::ModifiedNfd, Normalization::ModifiedNfc];

    /// The label the Text ▸ Unicode Normalizations submenu shows.
    pub fn name(self) -> &'static str {
        match self {
            Normalization::Nfd => "NFD",
            Normalization::Nfc => "NFC",
            Normalization::Nfkd => "NFKD",
            Normalization::Nfkc => "NFKC",
            Normalization::NfkcCaseFold => "NFKC Case-Fold",
            Normalization::ModifiedNfd => "Modified NFD",
            Normalization::ModifiedNfc => "Modified NFC",
        }
    }

    /// A stable identifier, used in an action's payload.
    pub fn identifier(self) -> &'static str {
        match self {
            Normalization::Nfd => "nfd",
            Normalization::Nfc => "nfc",
            Normalization::Nfkd => "nfkd",
            Normalization::Nfkc => "nfkc",
            Normalization::NfkcCaseFold => "nfkc-case-fold",
            Normalization::ModifiedNfd => "modified-nfd",
            Normalization::ModifiedNfc => "modified-nfc",
        }
    }

    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|form| form.identifier() == identifier)
    }

    /// Whether this build can actually apply the form.
    ///
    /// False for the three Apple forms — see the type. The menu still lists
    /// them, because a form that exists upstream and is missing here is worth
    /// naming rather than hiding; what a caller must not do is apply one and
    /// report success.
    pub fn is_supported(self) -> bool {
        match self {
            Normalization::Nfd
            | Normalization::Nfc
            | Normalization::Nfkd
            | Normalization::Nfkc => true,
            Normalization::NfkcCaseFold
            | Normalization::ModifiedNfd
            | Normalization::ModifiedNfc => false,
        }
    }
}

/// Apply `form` to the selection.
///
/// A form [`is_supported`](Normalization::is_supported) says no to leaves the
/// selection exactly as it found it — the caller is expected to have asked, and
/// to have said why in the status bar if the answer was no. Doing something
/// *close* instead would be worse than doing nothing: a half-normalized document
/// is one the user cannot tell from a normalized one.
pub fn normalize(text: &str, selection: &Range<usize>, form: Normalization) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start == end {
        return Edit::unchanged(text.to_string(), selection.clone());
    }
    let selected = &text[start..end];
    let replaced: String = match form {
        Normalization::Nfd => selected.nfd().collect(),
        Normalization::Nfc => selected.nfc().collect(),
        Normalization::Nfkd => selected.nfkd().collect(),
        Normalization::Nfkc => selected.nfkc().collect(),
        // See the note above: not a fallback, a refusal.
        Normalization::NfkcCaseFold
        | Normalization::ModifiedNfd
        | Normalization::ModifiedNfc => selected.to_string(),
    };
    let mut out = String::with_capacity(text.len() + replaced.len());
    out.push_str(&text[..start]);
    out.push_str(&replaced);
    out.push_str(&text[end..]);
    Edit { text: out, selection: start..start + replaced.len() }
}

// ---- enclosing symbols --------------------------------------------------

/// A pair of characters that enclose a region, such as `(` and `)`.
///
/// This is CotEditor's `SymbolPair`. The Select menu looks for the nearest
/// enclosing pair; a syntax's own quote delimiters are built from the same
/// shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolPair {
    pub begin: char,
    pub end: char,
}

impl SymbolPair {
    pub const fn new(begin: char, end: char) -> Self {
        Self { begin, end }
    }

    /// `()`, `{}` and `[]` — the pairs a syntax names as its brackets.
    pub const BRACES: [SymbolPair; 3] = [
        SymbolPair::new('(', ')'),
        SymbolPair::new('{', '}'),
        SymbolPair::new('[', ']'),
    ];

    /// `<>`, which *Select Enclosing Symbols* adds to [`Self::BRACES`].
    pub const LTGT: SymbolPair = SymbolPair::new('<', '>');

    /// What *Select Enclosing Symbols* searches, in CotEditor's order.
    pub const ENCLOSING: [SymbolPair; 4] = [
        SymbolPair::new('(', ')'),
        SymbolPair::new('{', '}'),
        SymbolPair::new('[', ']'),
        SymbolPair::new('<', '>'),
    ];
}

/// The nearest range enclosed by one of `candidates`, delimiters included.
///
/// This answers the question *Select Enclosing Symbols* asks: starting from
/// `range`, which bracketed region am I inside? It is CotEditor's
/// `rangeOfEnclosingSymbolPair(at:candidates:escapeCharacter:)` with the
/// escape character left out — see the note at the end.
///
/// It works outwards in two passes. The forward pass walks on from the end of
/// `range` until it meets a closing delimiter no opener accounts for, counting
/// nesting per pair so that `{ foo {} }` does not stop at the inner `}`. The
/// backward pass then walks back from the start of `range` to the matching
/// opener, and it considers only the pair the forward pass settled on — which
/// is why `def [ foo }` finds nothing rather than pairing `[` with `}`.
///
/// `range` and the result are byte offsets; the scan itself runs on
/// characters, because a delimiter is always exactly one of them.
///
/// CotEditor also lets a caller name an escape character, so that `\(` is not
/// an opener. Nothing in SubText needs that yet: the Select menu passes no
/// escape character, and the brace-highlighting path that does is not built.
pub fn range_of_enclosing_symbol_pair(
    text: &str,
    range: &Range<usize>,
    candidates: &[SymbolPair],
) -> Option<Range<usize>> {
    if candidates.is_empty() {
        return None;
    }

    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let to_char = |offset: usize| chars.partition_point(|(byte, _)| *byte < offset);

    let mut scanner = PairScanner {
        chars: &chars,
        candidates,
        scanning: to_char(range.start)..to_char(range.end),
        pair: None,
        finished: false,
        found: false,
    };
    let found = scanner.scan()?;

    // A character index maps to the byte offset of the character sitting at
    // it; `end` points one past the last character in play, which is the end
    // of the text when the delimiter was the last thing in the document.
    let to_byte = |index: usize| chars.get(index).map_or(text.len(), |(byte, _)| *byte);
    Some(to_byte(found.start)..to_byte(found.end))
}

/// The two-pass scan behind [`range_of_enclosing_symbol_pair`].
///
/// `scanning` is a half-open range of *character* indices, so its `end` points
/// one past the last character in play — which is what makes a closing
/// delimiter part of the range the scan reports.
struct PairScanner<'a> {
    chars: &'a [(usize, char)],
    candidates: &'a [SymbolPair],
    scanning: Range<usize>,
    /// The pair the forward pass found a closer for. The backward pass then
    /// looks for that pair's opener alone, which is what stops a stray `}` in
    /// `def [ foo }` from pairing up with the `[`.
    pair: Option<SymbolPair>,
    finished: bool,
    found: bool,
}

impl PairScanner<'_> {
    /// The nearest enclosing pair, or `None` when there is not one.
    fn scan(&mut self) -> Option<Range<usize>> {
        // The backward pass always finishes — either by finding the opener or
        // by running off the start of the text — so one round trip is enough.
        while !self.finished {
            self.scan_forward();
            if self.finished {
                return None;
            }
            self.scan_backward();
        }
        self.found.then(|| self.scanning.clone())
    }

    /// Walk on to the first closing delimiter no opener accounts for.
    fn scan_forward(&mut self) {
        let start = self.scanning.start;
        let mut depths = vec![0usize; self.candidates.len()];

        for i in self.scanning.end..self.chars.len() {
            let character = self.chars[i].1;

            // An opener wins over a closer for a pair whose two halves are the
            // same character, which is how `'` and `"` are meant to read.
            if let Some(pair) = position_of_begin(self.candidates, character) {
                depths[pair] += 1;
            } else if let Some(pair) = position_of_end(self.candidates, character) {
                if depths[pair] > 0 {
                    depths[pair] -= 1;
                } else {
                    // `i + 1`, not `i`: the closer belongs to the range the
                    // scan reports, so the end has to point past it.
                    self.scanning = start..i + 1;
                    self.pair = Some(self.candidates[pair]);
                    return;
                }
            }
        }

        self.finished = true;
    }

    /// Walk back to the opener that matches the closer the forward pass found.
    fn scan_backward(&mut self) {
        // Only the pair the forward pass settled on counts now. When it found
        // nothing — because the range ended inside unclosed text — every
        // candidate is still in play.
        let narrowed;
        let candidates: &[SymbolPair] = match self.pair {
            Some(pair) => {
                narrowed = [pair];
                &narrowed
            }
            None => self.candidates,
        };

        let start = self.scanning.start;
        let end = self.scanning.end;
        let mut depths = vec![0usize; candidates.len()];

        for i in (0..start).rev() {
            let character = self.chars[i].1;

            if let Some(pair) = position_of_begin(candidates, character) {
                if depths[pair] > 0 {
                    depths[pair] -= 1;
                } else {
                    self.finished = true;
                    self.found = true;
                    self.scanning = i..end;
                    return;
                }
            } else if let Some(pair) = position_of_end(candidates, character) {
                depths[pair] += 1;
            }
        }

        self.finished = true;
    }
}

/// Index of the first candidate that opens with `character`.
fn position_of_begin(candidates: &[SymbolPair], character: char) -> Option<usize> {
    candidates.iter().position(|pair| pair.begin == character)
}

/// Index of the first candidate that closes with `character`.
fn position_of_end(candidates: &[SymbolPair], character: char) -> Option<usize> {
    candidates.iter().position(|pair| pair.end == character)
}

// ---- selection expansion ------------------------------------------------

/// The logical-line content ranges `range` covers, terminators excluded.
///
/// This is CotEditor's `lineContentsRanges(for:)`, the work behind ⌃L
/// (*Split Selection by Lines*): a multi-line selection becomes one selection
/// per line, so the next keystroke lands on all of them.
///
/// The range is cut at line starts, each piece runs to the end of its line's
/// content — so a `\r\n` pair is never split down the middle — and each piece
/// is clipped to `range`. The clipping is what makes a caret mid-line yield
/// that caret back rather than the whole line, and what keeps a selection
/// ending exactly at a line start from pulling in the line below.
pub fn line_content_ranges(text: &str, range: &Range<usize>) -> Vec<Range<usize>> {
    let start = floor_char_boundary(text, range.start.min(text.len()));
    let end = floor_char_boundary(text, range.end.min(text.len()));
    if end < start {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    for (line_start, content_end) in line_bounds(text) {
        // A line counts when it holds the range's start, or when its own start
        // falls strictly inside the range. A line beginning exactly at the
        // range's end does not: the selection stopped before it.
        if line_start > start && line_start >= end {
            continue;
        }
        let from = line_start.max(start);
        let to = content_end.min(end);
        if from <= to {
            ranges.push(from..to);
        }
    }
    ranges
}

/// `(start, content_end)` for every logical line in `text`.
///
/// A line terminator is `\r\n`, a lone `\r`, or a lone `\n` — the three
/// NSString recognises, which is why a document saved with classic Mac line
/// endings still splits where its author meant it to. The final line always
/// appears, even when the text ends with a terminator and the line is empty.
fn line_bounds(text: &str) -> Vec<(usize, usize)> {
    // Scanning bytes is safe here: `\r` and `\n` are ASCII and cannot occur
    // inside a multi-byte sequence.
    let bytes = text.as_bytes();
    let mut bounds = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                bounds.push((start, i));
                i += if bytes.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                start = i;
            }
            b'\n' => {
                bounds.push((start, i));
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    bounds.push((start, text.len()));
    bounds
}

/// The line number (1-based) containing `offset`, and the column (1-based).
pub fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let line = line_index_at(text, offset) + 1;
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let column = text[line_start..offset].chars().count() + 1;
    (line, column)
}

/// Where the caret is, as CotEditor's status bar reports it: `(line, column,
/// location)`.
///
/// Three numbers that do **not** share a base. That is upstream's shape rather
/// than an accident of this port — `String+Counting.swift` gets them from three
/// different functions, and they are easy to reconcile wrongly:
///
/// | count | base | unit |
/// | --- | --- | --- |
/// | `Location` | 0 | grapheme clusters from the start of the text |
/// | `Line` | 1 | line endings wholly behind the caret |
/// | `Column` | 0 | grapheme clusters from the start of its own line |
///
/// The unit is Swift's `Character`, an extended grapheme cluster, which is why
/// a `\r\n` is one column wide and `🐱` is one too. `line_and_column` above
/// counts `\n` bytes and scalars instead, so it answers a different question —
/// it is the *Go to Line* dialog's, which parses a fuzzy line range and never
/// asks for a column.
///
/// A line ending belongs to the line it **terminates**: a caret sitting on the
/// `\r` of a `\r\n` is still on the line that `\r\n` ends, and only the
/// position after the whole ending is on the next line. Upstream reaches that
/// through Foundation's `getLineStart(_:end:contentsEnd:for:)`; this walks the
/// endings itself.
pub fn status_position(text: &str, offset: usize) -> (usize, usize, usize) {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let location = text[..offset].graphemes(true).count();

    let mut line = 1;
    let mut line_start = 0;
    let mut cursor = 0;
    while cursor < offset {
        match line_ending_at(text, cursor) {
            Some((_, len)) if cursor + len <= offset => {
                line += 1;
                cursor += len;
                line_start = cursor;
            }
            // The caret is inside this ending, so the ending is not behind it:
            // step over it for the scan, but leave the line alone.
            Some((_, len)) => cursor += len,
            None => cursor += text[cursor..].chars().next().map_or(1, char::len_utf8),
        }
    }

    let column = text[line_start..offset].graphemes(true).count();
    (line, column, location)
}

/// Byte offset of the start of 1-based `line`.
pub fn offset_of_line(text: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut current = 1;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            current += 1;
            if current == line {
                return i + 1;
            }
        }
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole document selected — what the Text-menu commands operate on
    /// when the user has picked everything.
    fn all(text: &str) -> Range<usize> {
        0..text.len()
    }

    #[test]
    fn shift_right_then_left_is_a_round_trip() {
        let text = "one\ntwo\n";
        let right = shift_right(text, &all(text), "    ");
        assert_eq!(right.text, "    one\n    two\n");
        let left = shift_left(&right.text, &right.selection, "    ");
        assert_eq!(left.text, text);
    }

    #[test]
    fn shift_left_strips_a_partial_indent() {
        let text = "  one\n";
        assert_eq!(shift_left(text, &all(text), "    ").text, "one\n");
    }

    #[test]
    fn shift_right_skips_blank_lines() {
        let text = "one\n\ntwo";
        assert_eq!(shift_right(text, &all(text), "  ").text, "  one\n\n  two");
    }

    #[test]
    fn comment_toggle_adds_then_removes() {
        let delimiters = CommentDelimiters { inline: Some("//".into()), block: None };
        let text = "let a = 1;\nlet b = 2;";
        let commented = toggle_comment(text, &all(text), &delimiters);
        assert_eq!(commented.text, "// let a = 1;\n// let b = 2;");
        let restored = toggle_comment(&commented.text, &all(&commented.text), &delimiters);
        assert_eq!(restored.text, text);
    }

    #[test]
    fn comment_toggle_preserves_indentation() {
        let delimiters = CommentDelimiters { inline: Some("#".into()), block: None };
        let text = "def f():\n    return 1";
        assert_eq!(
            toggle_comment(text, &all(text), &delimiters).text,
            "# def f():\n    # return 1"
        );
    }

    #[test]
    fn block_comment_wraps_and_unwraps() {
        let delimiters = CommentDelimiters { inline: None, block: Some(("/*".into(), "*/".into())) };
        let text = "abc";
        let wrapped = toggle_comment(text, &(0..3), &delimiters);
        assert_eq!(wrapped.text, "/*abc*/");
        let unwrapped = toggle_comment(&wrapped.text, &(0..wrapped.text.len()), &delimiters);
        assert_eq!(unwrapped.text, "abc");
    }

    #[test]
    fn move_lines_up_and_down_are_inverses() {
        let text = "a\nb\nc";
        let down = move_lines(text, &(0..1), false);
        assert_eq!(down.text, "b\na\nc");
        let up = move_lines(&down.text, &(2..3), true);
        assert_eq!(up.text, text);
    }

    #[test]
    fn duplicate_and_delete_line() {
        let text = "a\nb";
        let dup = duplicate_lines(text, &(0..1));
        assert_eq!(dup.text, "a\na\nb");
        let del = delete_lines(text, &(0..1));
        assert_eq!(del.text, "b");
    }

    #[test]
    fn join_lines_collapses_breaks() {
        let text = "one\n   two\nthree";
        assert_eq!(join_lines(text, &all(text)).text, "one two three");
    }

    #[test]
    fn sort_lines_ascending_and_descending() {
        let text = "banana\napple\ncherry";
        let options = SortOptions::whole_line();
        assert_eq!(
            sort_lines(text, &all(text), &SortKey::EntireLine, &options).text,
            "apple\nbanana\ncherry"
        );
        let desc = SortOptions { descending: true, ..options };
        assert_eq!(
            sort_lines(text, &all(text), &SortKey::EntireLine, &desc).text,
            "cherry\nbanana\napple"
        );
    }

    #[test]
    fn sort_lines_numeric_orders_by_value() {
        let text = "10\n9\n100";
        let options = SortOptions { numeric: true, ..SortOptions::whole_line() };
        assert_eq!(
            sort_lines(text, &all(text), &SortKey::EntireLine, &options).text,
            "9\n10\n100"
        );
    }

    #[test]
    fn sort_by_column_reads_the_field_and_leaves_short_rows_alone() {
        // CotEditor's own fixture: `dog, 🐕, 2, イヌ` and friends, sorted by the
        // third field.
        let text = "dog, 🐕, 2, イヌ\ncat, 🐈, 1, ねこ\ncow, 🐄, 3, ｳｼ\n";
        let key = SortKey::Column { delimiter: String::new(), column: 3 };
        assert_eq!(
            sort_lines(text, &all(text), &key, &SortOptions::default()).text,
            "cat, 🐈, 1, ねこ\ndog, 🐕, 2, イヌ\ncow, 🐄, 3, ｳｼ\n"
        );

        // A row with no such field has no key, so it sorts to the end.
        let ragged = "a,b,c\nx\ny,b,z\n";
        let key = SortKey::Column { delimiter: String::new(), column: 3 };
        assert_eq!(
            sort_lines(ragged, &all(ragged), &key, &SortOptions::default()).text,
            "a,b,c\ny,b,z\nx\n"
        );
    }

    #[test]
    fn sort_by_column_understands_an_escaped_delimiter() {
        let text = "b\t2\na\t1\n";
        let key = SortKey::Column { delimiter: "\\t".to_string(), column: 2 };
        assert_eq!(
            sort_lines(text, &all(text), &key, &SortOptions::default()).text,
            "a\t1\nb\t2\n"
        );
    }

    #[test]
    fn sort_by_regular_expression_uses_the_match_or_a_group() {
        let text = "dog, 🐕, 2, イヌ\ncat, 🐈, 1, ねこ\ncow, 🐄, 3, ｳｼ\n";
        let sorted = "cat, 🐈, 1, ねこ\ndog, 🐕, 2, イヌ\ncow, 🐄, 3, ｳｼ\n";

        let whole_match = SortKey::RegularExpression {
            pattern: ", ([0-9]),".to_string(),
            ignore_case: true,
            group: None,
        };
        assert_eq!(
            sort_lines(text, &all(text), &whole_match, &SortOptions::default()).text,
            sorted
        );

        let first_group = SortKey::RegularExpression {
            pattern: ", ([0-9]),".to_string(),
            ignore_case: true,
            group: Some(1),
        };
        assert_eq!(
            sort_lines(text, &all(text), &first_group, &SortOptions::default()).text,
            sorted
        );
    }

    #[test]
    fn a_line_without_a_key_keeps_out_of_the_way() {
        // Nothing matches `^x`, so every line has no key and the order stands.
        let text = "b\na\nc\n";
        let key =
            SortKey::RegularExpression { pattern: "^x".to_string(), ignore_case: true, group: None };
        assert_eq!(sort_lines(text, &all(text), &key, &SortOptions::default()).text, text);
    }

    #[test]
    fn sort_keeps_the_first_line_when_asked() {
        let text = "3\n12\n1\n";
        let options = SortOptions { keeps_first_line: true, ..SortOptions::whole_line() };
        assert_eq!(
            sort_lines(text, &all(text), &SortKey::EntireLine, &options).text,
            "3\n1\n12\n"
        );
    }

    #[test]
    fn sort_carries_each_line_s_own_terminator() {
        // CotEditor's `sortMixedLineEndings` fixture. A file whose lines end
        // differently must not have them rewritten by a sort.
        let options = SortOptions::whole_line();
        let key = SortKey::EntireLine;

        assert_eq!(
            sort_lines("b\r\na\nc", &(0..6), &key, &options).text,
            "a\nb\r\nc"
        );
        assert_eq!(
            sort_lines("b\r\na\n", &(0..5), &key, &options).text,
            "a\nb\r\n"
        );
    }

    #[test]
    fn sort_needs_two_lines_and_a_terminator() {
        // A single line is already sorted; a block with no terminator at all
        // has nothing to re-attach.
        let options = SortOptions::whole_line();
        for text in ["abc", "abc\n", ""] {
            let edit = sort_lines(text, &all(text), &SortKey::EntireLine, &options);
            assert_eq!(edit.text, text);
            assert_eq!(edit.selection, all(text));
        }
    }

    #[test]
    fn sort_hands_back_the_first_line_as_the_selection() {
        let text = "banana\napple\n";
        let edit = sort_lines(text, &all(text), &SortKey::EntireLine, &SortOptions::whole_line());
        assert_eq!(edit.text, "apple\nbanana\n");
        // "apple" — the first line of the result, not the whole block.
        assert_eq!(edit.selection, 0..5);
    }

    #[test]
    fn logical_lines_keep_their_endings_apart() {
        let text = "a\r\nb\nc";
        let lines = logical_lines(text, &(0..text.len()));
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].contents, "a");
        assert_eq!(lines[0].line_ending.as_deref(), Some("\r\n"));
        assert_eq!(lines[1].line_ending.as_deref(), Some("\n"));
        assert_eq!(lines[2].line_ending, None);

        // A block that ends in a terminator produces no phantom last line.
        let text = "a\nb\n";
        assert_eq!(logical_lines(text, &(0..text.len())).len(), 2);
    }

    #[test]
    fn leading_numbers_follow_the_parser_coteditor_uses() {
        assert_eq!(leading_number("0"), Some(0.0));
        assert_eq!(leading_number("10 000"), Some(10_000.0));
        assert_eq!(leading_number("-1000.1 m/s"), Some(-1000.1));
        // A grouping separator needs exactly three digits behind it, so the
        // comma in `-1000,1` ends the number rather than joining it.
        assert_eq!(leading_number("-1000,1 m/s"), Some(-1000.0));
        assert_eq!(leading_number("+1,000"), Some(1000.0));
        assert_eq!(leading_number("dog 10"), None);
        assert_eq!(leading_number(""), None);
    }

    #[test]
    fn sort_keys_report_why_they_cannot_be_used() {
        assert_eq!(SortKey::EntireLine.validate(), None);
        assert_eq!(
            SortKey::RegularExpression {
                pattern: String::new(),
                ignore_case: true,
                group: None,
            }
            .validate(),
            Some(SortKeyError::EmptyPattern)
        );
        assert_eq!(
            SortKey::RegularExpression {
                pattern: "\\".to_string(),
                ignore_case: true,
                group: None,
            }
            .validate(),
            Some(SortKeyError::InvalidPattern)
        );
        // `(a)(b)c` has two groups, so 0, 1 and 2 are usable and 3 is not.
        let key = |group| SortKey::RegularExpression {
            pattern: "(a)(b)c".to_string(),
            ignore_case: true,
            group: Some(group),
        };
        assert_eq!(key(2).validate(), None);
        assert_eq!(key(3).validate(), Some(SortKeyError::InvalidPattern));
        assert_eq!(
            SortKey::Column { delimiter: String::new(), column: 0 }.validate(),
            Some(SortKeyError::EmptyColumn)
        );
    }

    #[test]
    fn delete_duplicates_keeps_first() {
        let text = "a\nb\na\nB";
        assert_eq!(delete_duplicate_lines(text, &all(text), false).text, "a\nb\nB");
        assert_eq!(delete_duplicate_lines(text, &all(text), true).text, "a\nb");
    }

    #[test]
    fn case_transforms() {
        let text = "hello WORLD";
        assert_eq!(
            transform_case(text, &(0..5), CaseTransform::Upper).text,
            "HELLO WORLD"
        );
        assert_eq!(
            transform_case(text, &(0..11), CaseTransform::Capitalize).text,
            "Hello WORLD"
        );
    }

    /// The eight code points upstream folds, and the two it folds them to. The
    /// mapping is written as upstream writes it — two character classes — so
    /// the guillemets, the primes and the fullwidth forms that a reader might
    /// expect to find here are the point of the test.
    #[test]
    fn straighten_quotes_folds_the_eight_curly_ones() {
        let text = "‘a’‚b‛ “c”„d‟";
        let edit = straighten_quotes(text, &all(text));
        assert_eq!(edit.text, "'a''b' \"c\"\"d\"");
        // Nothing else moved, and the selection covers what was replaced.
        assert_eq!(edit.selection, 0..edit.text.len());
    }

    /// The characters that look like quotes and are *not* in the class:
    /// guillemets, primes, the fullwidth forms, and the straight quotes the
    /// fold produces — which is what makes it one-way rather than a toggle.
    #[test]
    fn straighten_quotes_leaves_every_other_quote_alone() {
        let text = "«a» ‹b› ′c″ ＂d＂ 'e' \"f\"";
        assert_eq!(straighten_quotes(text, &all(text)).text, text);
    }

    /// Only the selection is touched, and the multi-byte characters on either
    /// side of it keep their offsets: 日 is 0..3, 本 3..6, ‘ 6..9, 語 9..12,
    /// ’ 12..15 — so `6..15` is the quoted word and `6..11` is what it becomes.
    #[test]
    fn straighten_quotes_edits_only_the_selection() {
        let text = "日本‘語’";
        let edit = straighten_quotes(text, &(6..15));
        assert_eq!(edit.text, "日本'語'");
        assert_eq!(edit.selection, 6..11);
    }

    /// Upstream greys the item out with nothing selected, so this is the state
    /// the command can never be reached in — but it is the same no-op every
    /// other transform in this module is, rather than a panic or a surprise.
    #[test]
    fn straighten_quotes_with_nothing_selected_changes_nothing() {
        let text = "‘a’";
        let edit = straighten_quotes(text, &(3..3));
        assert_eq!(edit.text, text);
        assert_eq!(edit.selection, 3..3);
    }

    #[test]
    fn width_transforms_round_trip_ascii() {
        let text = "ABC 123";
        let full = transform_width(text, &(0..text.len()), WidthTransform::Full);
        assert_eq!(full.text, "ＡＢＣ　１２３");
        let half = transform_width(&full.text, &(0..full.text.len()), WidthTransform::Half);
        assert_eq!(half.text, text);
    }

    #[test]
    fn half_width_katakana_becomes_full_width() {
        let text = "ｶﾀｶﾅ";
        let full = transform_width(text, &(0..text.len()), WidthTransform::Full);
        assert_eq!(full.text, "カタカナ");
    }

    #[test]
    fn url_encode_decode_round_trip() {
        let text = "a b&c";
        let encoded = url_encode(text, &(0..text.len()));
        assert_eq!(encoded.text, "a%20b%26c");
        let decoded = url_decode(&encoded.text, &(0..encoded.text.len()));
        assert_eq!(decoded.text, text);
    }

    #[test]
    fn normalize_nfc_composes() {
        let text = "e\u{301}";
        let nfc = normalize(text, &(0..text.len()), Normalization::Nfc);
        assert_eq!(nfc.text, "é");
    }

    /// Upstream's `standardForms + [nil] + modifiedForms`: five labels, a
    /// separator, two more. The menu is built from these three constants, so
    /// their order is user-visible.
    #[test]
    fn the_forms_are_upstream_s_seven_in_upstream_s_order() {
        assert_eq!(
            Normalization::ALL.map(Normalization::name),
            [
                "NFD",
                "NFC",
                "NFKD",
                "NFKC",
                "NFKC Case-Fold",
                "Modified NFD",
                "Modified NFC",
            ]
        );
        assert_eq!(&Normalization::ALL[..5], Normalization::STANDARD);
        assert_eq!(&Normalization::ALL[5..], Normalization::MODIFIED);
    }

    /// The identifiers are what an action's payload carries, so every form has
    /// to have a distinct one and come back from it.
    #[test]
    fn every_form_round_trips_through_its_identifier() {
        for form in Normalization::ALL {
            assert_eq!(Normalization::from_identifier(form.identifier()), Some(form));
        }
        assert_eq!(Normalization::from_identifier("nfkccf"), None);
    }

    /// The four Unicode forms are the ones this build can apply. The three
    /// Apple ones need tables it does not carry, and a form it cannot apply must
    /// leave the text alone rather than approximate it.
    #[test]
    fn the_apple_forms_are_refused_rather_than_approximated() {
        let supported: Vec<Normalization> = Normalization::ALL
            .into_iter()
            .filter(|form| form.is_supported())
            .collect();
        assert_eq!(
            supported,
            [
                Normalization::Nfd,
                Normalization::Nfc,
                Normalization::Nfkd,
                Normalization::Nfkc,
            ]
        );

        // "ﬁ" is the ligature that NFKC decomposes to "fi" and NFD leaves be —
        // so an approximation of NFKC Case-Fold would show up here.
        let text = "ﬁ";
        for form in Normalization::MODIFIED.into_iter().chain([Normalization::NfkcCaseFold]) {
            assert_eq!(normalize(text, &(0..text.len()), form).text, text);
        }
    }

    #[test]
    fn delete_takes_the_selection_or_the_character_before_the_caret() {
        // A selection goes, and the caret lands where it began.
        let edit = delete_backward("hello world", &(5..11));
        assert_eq!(edit.text, "hello");
        assert_eq!(edit.selection, 5..5);

        // A bare caret takes the character before it.
        let edit = delete_backward("hello", &(5..5));
        assert_eq!(edit.text, "hell");
        assert_eq!(edit.selection, 4..4);

        // At the start of the document there is nothing to take.
        let edit = delete_backward("hello", &(0..0));
        assert_eq!(edit.text, "hello");
        assert_eq!(edit.selection, 0..0);
    }

    #[test]
    fn delete_takes_a_whole_character_not_a_code_point() {
        // `👍🏻` is one character of two scalars, so ⌘⌫ takes both of them.
        let text = "x👍🏻";
        let edit = delete_backward(text, &(text.len()..text.len()));
        assert_eq!(edit.text, "x");
        // A CRLF is one character of two scalars too.
        let text = "a\r\nb";
        let edit = delete_backward(text, &(3..3));
        assert_eq!(edit.text, "ab");
    }

    #[test]
    fn surround_and_trim() {
        let text = "a  \nb  ";
        assert_eq!(trim_trailing_whitespace(text, &all(text)).text, "a\nb");
    }

    /// Upstream's own fixture for `String.surround(in:begin:end:)`.
    ///
    /// `StringEditingTests.surround` runs it over three ranges of
    /// `"foo\nbar\nbaz"` — one full line, one *empty* range just after a
    /// newline, and one at the end — and expects `"<foo>\n<>bar\n<baz>"`.
    /// SubText has a single selection, so the three are three separate calls
    /// whose results are stitched the way upstream's `applying(_:)` stitches
    /// them.
    #[test]
    fn surround_wraps_the_selection_not_the_line() {
        let text = "foo\nbar\nbaz";

        // The middle range is empty and sits at offset 4, immediately after
        // the first newline: the pair is inserted there and the caret lands
        // between its halves.
        let middle = surround(text, &(4..4), "<", ">");
        assert_eq!(middle.text, "foo\n<>bar\nbaz");
        assert_eq!(middle.selection, 5..5);

        // A range that starts and ends inside one line wraps only what it
        // covers — the whole-line reading would have produced `(hello world)`.
        let partial = surround("hello world", &(6..11), "(", ")");
        assert_eq!(partial.text, "hello (world)");
        assert_eq!(partial.selection, 7..12);

        // The selection does not grow to cover the delimiters, so a second
        // surround nests rather than replacing.
        let once = surround("abc", &all("abc"), "'", "'");
        assert_eq!(once.text, "'abc'");
        assert_eq!(once.selection, 1..4);
        let twice = surround(&once.text, &once.selection, "\"", "\"");
        assert_eq!(twice.text, "'\"abc\"'");
    }

    #[test]
    fn surround_keeps_the_delimiters_off_a_multibyte_boundary() {
        // An offset in the middle of a character is rounded outwards rather
        // than panicking on a slice.
        let text = "aé b";
        let edit = surround(text, &(2..2), "[", "]");
        assert_eq!(edit.text, "a[é] b");
    }

    #[test]
    fn a_custom_surround_pair_can_be_anything() {
        // The sheet lets the user type both halves, and an empty end half
        // means "the same as the beginning" — upstream's `submit`.
        let text = "let x";
        assert_eq!(surround(text, &(4..5), "**", "**").text, "let **x**");
        assert_eq!(surround(text, &all(text), "```\n", "\n```").text, "```\nlet x\n```");
    }

    /// The three status-bar counts, against CotEditor's own test vectors.
    ///
    /// Every expectation below is copied from `StringCountingTests.swift`
    /// (`lineNumberAtLocation`, `lineNumberAtIndex`, `columnNumberAtIndex`) or
    /// from `String+Counting.swift`'s definitions. They are worth pinning
    /// because the three numbers have three different bases and a reader will
    /// assume they agree unless told otherwise.
    #[test]
    fn the_status_position_has_three_different_bases() {
        // `lineNumberAtLocation`: `"a\nb c\n\n"`, one-based, LF endings.
        let lf = "a\nb c\n\n";
        for (offset, line) in [(0, 1), (1, 1), (2, 2), (5, 2), (6, 3), (7, 4)] {
            assert_eq!(status_position(lf, offset).0, line, "line at {offset}");
        }

        // The same, with a CRLF: the LF of the pair is still on the line the
        // pair terminates, and only the position after it moves on.
        let crlf = "a\r\nb";
        for (offset, line) in [(0, 1), (1, 1), (2, 1), (3, 2), (4, 2)] {
            assert_eq!(status_position(crlf, offset).0, line, "line at {offset}");
        }

        // `lineNumberAtIndex`: `"a\n\n b"`.
        let blank = "a\n\n b";
        for (offset, line) in [(0, 1), (1, 1), (2, 2), (3, 3)] {
            assert_eq!(status_position(blank, offset).0, line, "line at {offset}");
        }
    }

    /// `columnNumberAtIndex`: `"aaa \r\n🐱 "`.
    ///
    /// The offsets in upstream's test are Swift `Character` offsets, so they are
    /// translated to byte offsets here — `\r\n` is one Character spanning two
    /// bytes, and `🐱` is one spanning four. That translation *is* the point:
    /// the unit is the grapheme cluster.
    #[test]
    fn the_status_column_counts_graphemes() {
        let text = "aaa \r\n🐱 ";
        // Character 3 is the space, 4 the CRLF, 5 the cat, 6 the last space.
        for (offset, column) in [(3, 3), (4, 4), (6, 0), (10, 1), (11, 2)] {
            assert_eq!(status_position(text, offset).1, column, "column at {offset}");
        }

        // …and `Location` is the same unit, from the start of the text.
        assert_eq!(status_position(text, 0).2, 0);
        assert_eq!(status_position(text, 3).2, 3, "three letters and a space");
        assert_eq!(status_position(text, 6).2, 5, "the CRLF is one cluster");
        assert_eq!(status_position(text, 11).2, 7, "…and the cat is another");
    }

    /// All six line endings, not just the two the status bar used to know.
    ///
    /// `line_and_column` counts `\n` bytes, so a document written with CR, NEL,
    /// LS or PS reported line 1 for every position. This is the count the status
    /// bar shows, so it has to know what the editor can write.
    #[test]
    fn every_line_ending_moves_the_line_number() {
        for ending in ["\n", "\r", "\r\n", "\u{85}", "\u{2028}", "\u{2029}"] {
            let text = format!("a{ending}b");
            let after = 1 + ending.len();
            assert_eq!(status_position(&text, 0).0, 1, "{ending:?} at the start");
            assert_eq!(status_position(&text, 1).0, 1, "{ending:?} inside the ending");
            assert_eq!(status_position(&text, after).0, 2, "{ending:?} behind us");
            assert_eq!(status_position(&text, after + 1).0, 2, "{ending:?} at the end");
        }
    }

    #[test]
    fn line_and_column_round_trip() {
        let text = "ab\ncde\nf";
        assert_eq!(line_and_column(text, 0), (1, 1));
        assert_eq!(line_and_column(text, 3), (2, 1));
        assert_eq!(line_and_column(text, 5), (2, 3));
        assert_eq!(offset_of_line(text, 2), 3);
        assert_eq!(offset_of_line(text, 3), 7);
    }

    #[test]
    fn indentation_conversion_respects_tab_width() {
        let text = "\tx\n  y";
        assert_eq!(
            convert_indentation_to_spaces(text, &all(text), 4).text,
            "    x\n  y"
        );
        assert_eq!(
            convert_indentation_to_tabs("        x", &all("        x"), 4).text,
            "\t\tx"
        );
    }

    #[test]
    fn enclosing_symbols_find_the_nearest_pair() {
        // CotEditor's own fixture for `rangeOfEnclosingSymbolPair`.
        let text = "def { foo {} | { bar } } ";
        let pairs = SymbolPair::BRACES;

        // Outside every pair, and past the end of the text.
        assert_eq!(range_of_enclosing_symbol_pair(text, &(1..2), &pairs), None);
        assert_eq!(range_of_enclosing_symbol_pair(text, &(24..24), &pairs), None);
        // The outer braces, not the inner ones the caret sits between.
        assert_eq!(
            range_of_enclosing_symbol_pair(text, &(13..14), &pairs),
            Some(4..24)
        );
        // Standing inside the empty pair, the nearest thing is that pair.
        assert_eq!(
            range_of_enclosing_symbol_pair(text, &(11..11), &pairs),
            Some(10..12)
        );
    }

    #[test]
    fn enclosing_symbols_will_not_pair_mismatched_delimiters() {
        // The `[` never gets to claim the `}`: the forward pass settles on the
        // brace, and the backward pass then finds no `{` before it.
        assert_eq!(
            range_of_enclosing_symbol_pair("def [ foo }", &(7..7), &SymbolPair::BRACES),
            None
        );
    }

    #[test]
    fn enclosing_symbols_nest_the_angle_pair() {
        let text = "if < foo < bar > > else";
        assert_eq!(
            range_of_enclosing_symbol_pair(text, &(8..8), &SymbolPair::ENCLOSING),
            Some(3..18)
        );
    }

    #[test]
    fn enclosing_symbols_measure_in_characters_not_bytes() {
        // A multi-byte character before the pair must not shift the answer:
        // the scan counts characters, the caller counts bytes.
        let text = "🐕 (woof)";
        let inside = text.find('(').expect("the fixture has an opener") + 1;
        assert_eq!(
            range_of_enclosing_symbol_pair(text, &(inside..inside), &SymbolPair::BRACES),
            Some(inside - 1..text.len())
        );
    }

    #[test]
    fn line_content_ranges_clip_to_the_selection() {
        // A caret mid-line comes back as that caret, not as the whole line.
        assert_eq!(line_content_ranges("foo\nbar", &(1..1)), vec![1..1]);
        // A selection reaching into two lines is cut at the line start, and
        // stops before the line whose start is its own end.
        assert_eq!(line_content_ranges("foo\nbar\nbaz", &(1..8)), vec![1..3, 4..7]);
    }

    #[test]
    fn line_content_ranges_cover_every_line_terminator() {
        assert_eq!(line_content_ranges("foo\nbar", &(0..7)), vec![0..3, 4..7]);
        assert_eq!(line_content_ranges("foo\r\nbar", &(0..8)), vec![0..3, 5..8]);
        assert_eq!(
            line_content_ranges("foo\r\r\rbar", &(0..9)),
            vec![0..3, 4..4, 5..5, 6..9]
        );
        // The line after a trailing terminator is empty, and an empty range at
        // the end of the selection is not a selection.
        assert_eq!(line_content_ranges("foo\nbar\n", &(0..8)), vec![0..3, 4..7]);
    }
}
